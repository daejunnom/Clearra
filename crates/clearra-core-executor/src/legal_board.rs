//! Exact, profile-bound legal-board asset contracts.
//!
//! This crate owns only the immutable `F_k ∩ R_k` bundle representation,
//! its row-normalization codec, and fail-open lookup outcomes. It does not
//! generate movement domains, download assets, decide product qualification,
//! or implement BuildUp reachability.

// SRP rationale: this module has one change reason: the exact legal-board
// bundle contract, from physical-row encoding through qualified lookup.

use clearra_accelerator_activation::{AcceleratorProduct, VerifiedAcceleratorAuthority};
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_rules::kicks::{
    KickTableProfile, KickTableProfileId, KickTransition, NoKick, SrsKicks,
};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use crate::row_frame::{CompactedRowFrame, CompactedRowFrameError};

const MAGIC: &[u8; 8] = b"CLLB0002";
const VERSION: u32 = 2;
const HEADER_BYTES: usize = 160;
const DIRECTORY_ENTRY_BYTES: usize = 64;
const LAYER_COUNT: usize = 11;
const PAYLOAD_OFFSET: usize = 1024;
const FIELD_MASK: u64 = (1_u64 << 40) - 1;
#[cfg(test)]
const ROW_MASK: u64 = (1_u64 << 10) - 1;
const MAX_BUNDLE_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_ACTIVE_ACCELERATOR_BYTES: usize = 128 * 1024 * 1024;
pub const EXACT_LEGAL_BOARD_COMPLETENESS_SCOPE: &str =
    "empty-origin-10x4-four-lines-f-intersection-r";
// Exact diagnostic membership uses sparse checkpoints. Product negative
// pruning uses the no-false-negative filter below instead of walking deltas
// for every BuildUp subset.
const CHECKPOINT_STRIDE: u64 = 256;
const NEGATIVE_FILTER_KEYS_PER_WORD: u64 = 8;
const NEGATIVE_FILTER_MAX_WORDS: usize = 1 << 20;
const EXACT_INTERSECTION_KIND: u8 = 1;
const PROFILE_SLOTS: usize = 5;
const SYNOPSIS_MAGIC: &[u8; 8] = b"CLLS0001";
const SYNOPSIS_HEADER_BYTES: usize = 8 + 1 + 32 * 3 + LAYER_COUNT * 4;
const SYNOPSIS_DIGEST_BYTES: usize = 32;
pub const MAX_DISTRIBUTED_SYNOPSIS_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProviderStatus {
    Ready,
    LoadedComplete,
    NotLoaded,
    OutOfScope,
    NotQualified,
    Miss,
    InvalidAsset,
    SnapshotMismatch,
    RateLimited,
    Offline,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegalBoardDecision {
    CandidateAllowed,
    VerifiedAbsent,
    PassThrough(ProviderStatus),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletionCapability {
    ClearToEmpty,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegalBoardQuery {
    pub width: u8,
    pub height: u8,
    pub initial_board: u64,
    pub kick_profile: KickTableProfileId,
    pub physical_board: u64,
    pub deleted_original_rows: u16,
    pub placed_piece_count: usize,
    pub completion: CompletionCapability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowCodecError {
    DeletedRowsOutsideFourRowFrame,
    PhysicalCellsOutsideSurvivingRows,
    MissingClearedBottomPrefix,
}

impl From<CompactedRowFrameError> for RowCodecError {
    fn from(error: CompactedRowFrameError) -> Self {
        match error {
            CompactedRowFrameError::PhysicalCellsOutsideSurvivingRows => {
                Self::PhysicalCellsOutsideSurvivingRows
            }
            CompactedRowFrameError::MissingClearedBottomPrefix => Self::MissingClearedBottomPrefix,
        }
    }
}

/// Typed correspondence between BuildUp's original logical rows and the
/// legal-board product's bottom-prefix normalization.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OriginalRowFrame {
    frame: CompactedRowFrame,
}

impl OriginalRowFrame {
    pub fn from_deleted_rows(value: u16) -> Result<Self, RowCodecError> {
        let frame = CompactedRowFrame::new(4, value)
            .ok_or(RowCodecError::DeletedRowsOutsideFourRowFrame)?;
        Ok(Self { frame })
    }

    pub const fn deleted_original_rows(self) -> u8 {
        self.frame.deleted_original_rows()
    }

    pub const fn cleared_row_count(self) -> u8 {
        self.frame.cleared_rows()
    }

    pub const fn surviving_original_rows(self) -> u8 {
        self.frame.surviving_rows()
    }

    /// Convert a compact physical board to the single canonical graph form:
    /// cleared rows are represented only as a full bottom-row prefix. Their
    /// original positions remain in this frame for replay/witness ownership.
    pub fn normalize_product_board(self, physical_board: u64) -> Result<u64, RowCodecError> {
        self.frame
            .bottom_prefix_board(physical_board)
            .map_err(Into::into)
    }

    /// Recover the compact physical board from the product membership key.
    /// The original-row map is retained by this typed frame rather than being
    /// encoded into the product board itself.
    pub fn compact_physical_board(self, product_board: u64) -> Result<u64, RowCodecError> {
        self.frame
            .compact_from_bottom_prefix(product_board)
            .map_err(Into::into)
    }

    /// Reinsert full cleared rows at their original logical positions for
    /// replay coordinates. This representation must never be used as a legal
    /// board membership key.
    pub fn replay_frame_board(self, physical_board: u64) -> Result<u64, RowCodecError> {
        self.frame
            .replay_frame_board(physical_board)
            .map_err(Into::into)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegalBoardBinding {
    pub kick_profile: KickTableProfileId,
    pub rule_identity: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnsupportedLegalBoardProfile;

/// Canonical public profile identifier shared by catalogs, download stores,
/// signed statements, and diagnostics.  `srs-90` remains the rules-layer ID;
/// the accelerator products deliberately use the established public `srs`
/// slot so a signed SRS asset cannot become impossible to activate merely
/// because the two layers use different names for the same kick profile.
pub const fn accelerator_profile_name(
    kick_profile: KickTableProfileId,
) -> Result<&'static str, UnsupportedLegalBoardProfile> {
    match kick_profile {
        KickTableProfileId::Srs90 => Ok("srs"),
        KickTableProfileId::SrsPlus => Ok("srs-plus"),
        KickTableProfileId::SrsX => Ok("srs-x"),
        KickTableProfileId::Jstris180 => Ok("jstris-180"),
        KickTableProfileId::NoKick => Ok("no-kick"),
        _ => Err(UnsupportedLegalBoardProfile),
    }
}

/// Fingerprint every movement semantic consumed by the exact legal-board
/// generator and lookup. Keeping this in the format owner prevents a producer
/// and consumer from silently calculating different identities.
pub fn built_in_rule_identity(
    kick_profile: KickTableProfileId,
) -> Result<[u8; 32], UnsupportedLegalBoardProfile> {
    let profile: KickTableProfile = match kick_profile {
        KickTableProfileId::Srs90 => SrsKicks::profile(),
        KickTableProfileId::SrsPlus => SrsKicks::srs_plus_profile(),
        KickTableProfileId::SrsX => SrsKicks::srs_x_profile(),
        KickTableProfileId::Jstris180 => SrsKicks::jstris_180_profile(),
        KickTableProfileId::NoKick => NoKick::profile(),
        _ => return Err(UnsupportedLegalBoardProfile),
    };
    let mut digest = Sha256::new();
    digest.update(
        b"clearra.pc4.legal-board.exact-domain.v2\0width=10\0height=4\0\
boundary=closed-left-right-bottom-open-top\0spawn=reachable-template-profile-defined\0\
lock=grounded-after-first-success-ordered-kick\0line-clear=physical-then-compact\0\
normalization=full-bottom-prefix-plus-original-row-frame\0coordinate=board64-bottom-left\0",
    );
    digest.update(kick_profile.as_str().as_bytes());
    digest.update([0]);
    let registry = standard_tetromino_registry();
    for piece in PieceKind::STANDARD_TETROMINOES {
        let definition = registry.get(piece).ok_or(UnsupportedLegalBoardProfile)?;
        for rotation in RotationState::ALL {
            digest.update([piece.as_ascii() as u8, rotation.quarter_turns()]);
            for cell in definition.shape(rotation).cells() {
                digest.update([cell.x() as u8, cell.y() as u8]);
            }
        }
        for from in RotationState::ALL {
            for to in RotationState::ALL {
                if from == to {
                    continue;
                }
                digest.update([
                    piece.as_ascii() as u8,
                    from.quarter_turns(),
                    to.quarter_turns(),
                ]);
                if let Some(sequence) = profile.sequence_for(KickTransition::new(piece, from, to)) {
                    digest.update((sequence.len() as u32).to_le_bytes());
                    for offset in sequence.offsets() {
                        digest.update([offset.dx() as u8, offset.dy() as u8]);
                    }
                } else {
                    digest.update(u32::MAX.to_le_bytes());
                }
            }
        }
    }
    Ok(digest.finalize().into())
}

pub fn built_in_binding(
    kick_profile: KickTableProfileId,
) -> Result<LegalBoardBinding, UnsupportedLegalBoardProfile> {
    Ok(LegalBoardBinding {
        kick_profile,
        rule_identity: built_in_rule_identity(kick_profile)?,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegalBoardExpectation {
    pub binding: LegalBoardBinding,
    pub generation_identity: Option<[u8; 32]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegalBoardAssetError {
    TooLarge,
    Header,
    UnsupportedVersion,
    UnsupportedProfile,
    NotExactIntersection,
    NotQualified,
    BindingMismatch,
    SnapshotMismatch,
    Directory,
    PayloadDigest,
    GenerationIdentity,
    NonCanonicalEncoding,
    LayerArea,
    TerminalDomainIncomplete,
    ActiveSessionTooLarge,
    ActiveSessionInUse,
    RegistryUnavailable,
}

impl LegalBoardAssetError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::TooLarge => "legal_board_asset_too_large",
            Self::Header => "legal_board_asset_header_invalid",
            Self::UnsupportedVersion => "legal_board_asset_version_unsupported",
            Self::UnsupportedProfile => "legal_board_asset_profile_unsupported",
            Self::NotExactIntersection => "legal_board_asset_not_exact_intersection",
            Self::NotQualified => "legal_board_asset_not_qualified",
            Self::BindingMismatch => "legal_board_asset_binding_mismatch",
            Self::SnapshotMismatch => "legal_board_asset_snapshot_mismatch",
            Self::Directory => "legal_board_asset_directory_invalid",
            Self::PayloadDigest => "legal_board_asset_payload_digest_mismatch",
            Self::GenerationIdentity => "legal_board_asset_generation_identity_mismatch",
            Self::NonCanonicalEncoding => "legal_board_asset_encoding_noncanonical",
            Self::LayerArea => "legal_board_asset_layer_area_mismatch",
            Self::TerminalDomainIncomplete => "legal_board_asset_terminal_domain_incomplete",
            Self::ActiveSessionTooLarge => "legal_board_active_session_too_large",
            Self::ActiveSessionInUse => "legal_board_active_session_in_use",
            Self::RegistryUnavailable => "legal_board_registry_unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LayerDirectory {
    count: u64,
    offset: usize,
    length: usize,
    digest: [u8; 32],
}

#[derive(Clone, Copy, Debug)]
struct Checkpoint {
    first_value: u64,
    byte_offset: usize,
    prior_value: u64,
    ordinal: u64,
}

#[derive(Debug)]
struct LayerIndex {
    directory: LayerDirectory,
    checkpoints: Vec<Checkpoint>,
    negative_filter: NegativeFilter,
}

/// A per-layer blocked Bloom filter. A false positive merely keeps an
/// otherwise impossible candidate on the ordinary exact path; a negative is
/// a proof of absence from the already-authenticated complete layer.
#[derive(Debug)]
struct NegativeFilter {
    words: Vec<u64>,
}

impl NegativeFilter {
    fn new(expected_keys: u64) -> Result<Self, LegalBoardAssetError> {
        let target_words = expected_keys
            .div_ceil(NEGATIVE_FILTER_KEYS_PER_WORD)
            .min(NEGATIVE_FILTER_MAX_WORDS as u64)
            .max(1) as usize;
        let word_count = target_words.next_power_of_two();
        let mut words = Vec::new();
        words
            .try_reserve_exact(word_count)
            .map_err(|_| LegalBoardAssetError::TooLarge)?;
        words.resize(word_count, 0);
        Ok(Self { words })
    }

    #[inline]
    fn insert(&mut self, key: u64) {
        let (word, mask) = self.location(key);
        self.words[word] |= mask;
    }

    #[inline]
    fn may_contain(&self, key: u64) -> bool {
        let (word, mask) = self.location(key);
        self.words[word] & mask == mask
    }

    /// Fold a power-of-two word table by ORing buckets that share the low
    /// hash bits. Every inserted key keeps all four of its bits; folding can
    /// increase false positives but cannot create a false negative.
    fn folded(&self, word_count: usize) -> Result<Self, LegalBoardSynopsisError> {
        debug_assert!(word_count.is_power_of_two() && word_count <= self.words.len());
        let mut words = Vec::new();
        words
            .try_reserve_exact(word_count)
            .map_err(|_| LegalBoardSynopsisError::AllocationUnavailable)?;
        words.resize(word_count, 0);
        for (index, &word) in self.words.iter().enumerate() {
            words[index & (word_count - 1)] |= word;
        }
        Ok(Self { words })
    }

    #[inline]
    fn location(&self, key: u64) -> (usize, u64) {
        let hash = bloom_mix64(key);
        let word = (hash as usize) & (self.words.len() - 1);
        let mask = (1_u64 << ((hash >> 32) & 63))
            | (1_u64 << ((hash >> 38) & 63))
            | (1_u64 << ((hash >> 44) & 63))
            | (1_u64 << ((hash >> 50) & 63));
        (word, mask)
    }
}

#[inline]
fn bloom_mix64(mut key: u64) -> u64 {
    key ^= key >> 30;
    key = key.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    key ^= key >> 27;
    key = key.wrapping_mul(0x94d0_49bb_1331_11eb);
    key ^ (key >> 31)
}

/// Shared compressed owner. Clones share payload and sparse indices; no
/// worker receives a decoded copy of all legal boards.
#[derive(Clone, Debug)]
pub struct ExactLegalBoard {
    bytes: Arc<[u8]>,
    binding: LegalBoardBinding,
    generation_identity: [u8; 32],
    layers: Arc<[LayerIndex; LAYER_COUNT]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegalBoardSynopsisError {
    BudgetTooSmall,
    AllocationUnavailable,
    InvalidWire,
    AuthorityMismatch,
}

/// A bounded, negative-only derivative of one signed complete legal-board.
/// It never claims positive membership. The full asset stays with its owner;
/// a browser worker transport may share this derivative without giving a
/// second worker a copy of the complete bundle or sparse index.
#[derive(Debug)]
pub struct LegalBoardNegativeSynopsis {
    binding: LegalBoardBinding,
    generation_identity: [u8; 32],
    signed_catalog_identity: [u8; 32],
    filters: Vec<NegativeFilter>,
}

impl LegalBoardNegativeSynopsis {
    pub const fn generation_identity(&self) -> [u8; 32] {
        self.generation_identity
    }

    pub const fn signed_catalog_identity(&self) -> [u8; 32] {
        self.signed_catalog_identity
    }

    pub fn retained_bytes(&self) -> usize {
        core::mem::size_of::<Self>()
            + self.filters.capacity() * core::mem::size_of::<NegativeFilter>()
            + self
                .filters
                .iter()
                .map(|filter| filter.words.capacity() * core::mem::size_of::<u64>())
                .sum::<usize>()
    }

    pub fn decide_negative_only(&self, query: LegalBoardQuery) -> LegalBoardDecision {
        let (layer, storage_key) = match scoped_storage_key_for_binding(self.binding, query) {
            Ok(value) => value,
            Err(status) => return LegalBoardDecision::PassThrough(status),
        };
        if self.filters[layer].may_contain(storage_key) {
            LegalBoardDecision::CandidateAllowed
        } else {
            LegalBoardDecision::VerifiedAbsent
        }
    }

    /// Transport a bounded derivative between trusted workers of the same
    /// application job. The signed complete bundle remains with the owner.
    /// The wire digest detects accidental corruption; it does not turn an
    /// arbitrary sender into a source of negative-proof authority.
    pub fn to_trusted_worker_wire(&self) -> Result<Vec<u8>, LegalBoardSynopsisError> {
        let word_bytes = self
            .filters
            .iter()
            .try_fold(0_usize, |sum, filter| {
                sum.checked_add(filter.words.len().checked_mul(8)?)
            })
            .ok_or(LegalBoardSynopsisError::InvalidWire)?;
        let total = SYNOPSIS_HEADER_BYTES
            .checked_add(word_bytes)
            .and_then(|bytes| bytes.checked_add(SYNOPSIS_DIGEST_BYTES))
            .ok_or(LegalBoardSynopsisError::InvalidWire)?;
        if total > MAX_DISTRIBUTED_SYNOPSIS_BYTES {
            return Err(LegalBoardSynopsisError::BudgetTooSmall);
        }
        let mut wire = Vec::new();
        wire.try_reserve_exact(total)
            .map_err(|_| LegalBoardSynopsisError::AllocationUnavailable)?;
        wire.extend_from_slice(SYNOPSIS_MAGIC);
        wire.push(
            profile_slot(self.binding.kick_profile)
                .map_err(|_| LegalBoardSynopsisError::InvalidWire)? as u8,
        );
        wire.extend_from_slice(&self.binding.rule_identity);
        wire.extend_from_slice(&self.generation_identity);
        wire.extend_from_slice(&self.signed_catalog_identity);
        for filter in &self.filters {
            wire.extend_from_slice(&(filter.words.len() as u32).to_le_bytes());
        }
        for filter in &self.filters {
            for word in &filter.words {
                wire.extend_from_slice(&word.to_le_bytes());
            }
        }
        let digest: [u8; 32] = Sha256::digest(&wire).into();
        wire.extend_from_slice(&digest);
        Ok(wire)
    }

    /// Only the signed-bundle owner may supply this derivative. The receiver
    /// separately authenticates the embedded catalog statement and compares
    /// all three identities before it installs the bounded summary.
    pub fn from_trusted_worker_wire(
        wire: &[u8],
        authority: &VerifiedAcceleratorAuthority,
    ) -> Result<Self, LegalBoardSynopsisError> {
        if wire.len() < SYNOPSIS_HEADER_BYTES + SYNOPSIS_DIGEST_BYTES
            || wire.len() > MAX_DISTRIBUTED_SYNOPSIS_BYTES
            || wire.get(..8) != Some(SYNOPSIS_MAGIC.as_slice())
        {
            return Err(LegalBoardSynopsisError::InvalidWire);
        }
        let digest_start = wire.len() - SYNOPSIS_DIGEST_BYTES;
        let digest: [u8; 32] = Sha256::digest(&wire[..digest_start]).into();
        if wire[digest_start..] != digest {
            return Err(LegalBoardSynopsisError::InvalidWire);
        }
        let profile = decode_profile(wire[8]).map_err(|_| LegalBoardSynopsisError::InvalidWire)?;
        let binding =
            built_in_binding(profile).map_err(|_| LegalBoardSynopsisError::AuthorityMismatch)?;
        if authority.product() != AcceleratorProduct::ExactLegalBoard
            || authority.profile()
                != accelerator_profile_name(profile)
                    .map_err(|_| LegalBoardSynopsisError::AuthorityMismatch)?
            || authority.rule_identity() != binding.rule_identity
            || authority.completeness_scope() != EXACT_LEGAL_BOARD_COMPLETENESS_SCOPE
            || wire[9..41] != binding.rule_identity
            || wire[41..73] != authority.generation_identity()
            || wire[73..105] != authority.statement_identity()
        {
            return Err(LegalBoardSynopsisError::AuthorityMismatch);
        }
        let mut counts = [0_usize; LAYER_COUNT];
        let mut expected_end = SYNOPSIS_HEADER_BYTES;
        for (layer, count) in counts.iter_mut().enumerate() {
            let offset = 105 + layer * 4;
            *count = u32::from_le_bytes(
                wire[offset..offset + 4]
                    .try_into()
                    .map_err(|_| LegalBoardSynopsisError::InvalidWire)?,
            ) as usize;
            if *count == 0 || !count.is_power_of_two() || *count > NEGATIVE_FILTER_MAX_WORDS {
                return Err(LegalBoardSynopsisError::InvalidWire);
            }
            expected_end = expected_end
                .checked_add(
                    count
                        .checked_mul(8)
                        .ok_or(LegalBoardSynopsisError::InvalidWire)?,
                )
                .ok_or(LegalBoardSynopsisError::InvalidWire)?;
        }
        if expected_end != digest_start {
            return Err(LegalBoardSynopsisError::InvalidWire);
        }
        let mut cursor = SYNOPSIS_HEADER_BYTES;
        let mut filters = Vec::new();
        filters
            .try_reserve_exact(LAYER_COUNT)
            .map_err(|_| LegalBoardSynopsisError::AllocationUnavailable)?;
        for count in counts {
            let mut words = Vec::new();
            words
                .try_reserve_exact(count)
                .map_err(|_| LegalBoardSynopsisError::AllocationUnavailable)?;
            for _ in 0..count {
                words.push(u64::from_le_bytes(
                    wire[cursor..cursor + 8]
                        .try_into()
                        .map_err(|_| LegalBoardSynopsisError::InvalidWire)?,
                ));
                cursor += 8;
            }
            filters.push(NegativeFilter { words });
        }
        Ok(Self {
            binding,
            generation_identity: authority.generation_identity(),
            signed_catalog_identity: authority.statement_identity(),
            filters,
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) enum LegalBoardNegativeOwner {
    Complete(Arc<QualifiedExactLegalBoard>),
    TrustedSynopsis(Arc<LegalBoardNegativeSynopsis>),
}

impl LegalBoardNegativeOwner {
    #[inline]
    pub(crate) fn decide_negative_only(&self, query: LegalBoardQuery) -> LegalBoardDecision {
        match self {
            Self::Complete(board) => board.decide_negative_only(query),
            Self::TrustedSynopsis(synopsis) => synopsis.decide_negative_only(query),
        }
    }
}

#[derive(Clone, Debug)]
pub struct QualifiedExactLegalBoard {
    board: ExactLegalBoard,
    signed_catalog_identity: [u8; 32],
    exhaustive_differential_identity: [u8; 32],
}

impl QualifiedExactLegalBoard {
    pub fn qualify(
        board: ExactLegalBoard,
        authority: &VerifiedAcceleratorAuthority,
    ) -> Result<Self, LegalBoardAssetError> {
        let payload_identity: [u8; 32] = Sha256::digest(&*board.bytes).into();
        if authority.product() != AcceleratorProduct::ExactLegalBoard
            || authority.profile()
                != accelerator_profile_name(board.binding.kick_profile)
                    .map_err(|_| LegalBoardAssetError::UnsupportedProfile)?
            || authority.generation_identity() != board.generation_identity
            || authority.rule_identity() != board.binding.rule_identity
            || authority.payload_bytes() != board.bytes.len() as u64
            || authority.payload_identity() != payload_identity
            || authority.completeness_scope() != EXACT_LEGAL_BOARD_COMPLETENESS_SCOPE
            || authority.statement_identity() == [0; 32]
            || authority.qualification_identity() == [0; 32]
        {
            return Err(LegalBoardAssetError::NotQualified);
        }
        Ok(Self {
            board,
            signed_catalog_identity: authority.statement_identity(),
            exhaustive_differential_identity: authority.qualification_identity(),
        })
    }

    pub const fn binding(&self) -> LegalBoardBinding {
        self.board.binding()
    }

    pub const fn generation_identity(&self) -> [u8; 32] {
        self.board.generation_identity()
    }

    pub const fn signed_catalog_identity(&self) -> [u8; 32] {
        self.signed_catalog_identity
    }

    pub const fn exhaustive_differential_identity(&self) -> [u8; 32] {
        self.exhaustive_differential_identity
    }

    pub fn decide(&self, query: LegalBoardQuery) -> LegalBoardDecision {
        self.board.decide(query)
    }

    /// Negative-only hot-path decision. Positive Bloom matches are passed to
    /// the ordinary exact search, not treated as proof of membership.
    pub fn decide_negative_only(&self, query: LegalBoardQuery) -> LegalBoardDecision {
        self.board.decide_negative_only(query)
    }

    pub fn shared_bytes(&self) -> usize {
        self.board.compressed_bytes() + self.board.sparse_index_bytes()
    }

    /// The source must first pass the signed complete-product admission.
    /// This derivative may be folded to a host-owned memory budget, but it
    /// cannot be used to assert that a Bloom-positive state is legal.
    pub fn negative_synopsis(
        &self,
        maximum_bytes: usize,
    ) -> Result<LegalBoardNegativeSynopsis, LegalBoardSynopsisError> {
        self.board
            .negative_synopsis(maximum_bytes, self.signed_catalog_identity)
    }
}

#[derive(Default)]
struct LegalBoardRegistry {
    slots: [Option<Arc<QualifiedExactLegalBoard>>; PROFILE_SLOTS],
}

static LEGAL_BOARD_REGISTRY: OnceLock<RwLock<LegalBoardRegistry>> = OnceLock::new();
static LEGAL_BOARD_SYNOPSIS_REGISTRY: OnceLock<
    RwLock<[Option<Arc<LegalBoardNegativeSynopsis>>; PROFILE_SLOTS]>,
> = OnceLock::new();
static ACCELERATOR_REGISTRY_MUTATION: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) fn accelerator_registry_mutation_lock() -> &'static Mutex<()> {
    ACCELERATOR_REGISTRY_MUTATION.get_or_init(|| Mutex::new(()))
}

pub fn install_qualified_exact_legal_board(
    board: QualifiedExactLegalBoard,
) -> Result<Option<Arc<QualifiedExactLegalBoard>>, LegalBoardAssetError> {
    // Legal-board and conditioned-reachability live in separate registries,
    // but their 128 MiB limit is one transaction. Serialize both install and
    // removal paths so concurrent cross-product installs cannot each observe
    // the other registry before publication and exceed the combined limit.
    let _mutation = accelerator_registry_mutation_lock()
        .lock()
        .map_err(|_| LegalBoardAssetError::RegistryUnavailable)?;
    let slot = profile_slot(board.binding().kick_profile)?;
    // Count every resident profile, not just the selected profile. A host may
    // retain a prior profile while the next request is being prepared.
    let combined = board
        .shared_bytes()
        .saturating_add(installed_legal_board_bytes(Some(slot))?)
        .saturating_add(installed_legal_board_synopsis_bytes(None)?)
        .saturating_add(
            crate::conditioned_reachability::installed_conditioned_reachability_bytes(None)
                .map_err(|_| LegalBoardAssetError::RegistryUnavailable)?,
        )
        .saturating_add(
            crate::conditioned_local_product::installed_local_relation_bytes(None)
                .map_err(|_| LegalBoardAssetError::RegistryUnavailable)?,
        );
    if combined > MAX_ACTIVE_ACCELERATOR_BYTES {
        return Err(LegalBoardAssetError::ActiveSessionTooLarge);
    }
    let registry = LEGAL_BOARD_REGISTRY.get_or_init(|| RwLock::new(LegalBoardRegistry::default()));
    let mut guard = registry
        .write()
        .map_err(|_| LegalBoardAssetError::RegistryUnavailable)?;
    if trusted_legal_board_synopsis_snapshot(board.binding().kick_profile).is_some() {
        return Err(LegalBoardAssetError::ActiveSessionInUse);
    }
    if let Some(active) = guard.slots[slot].as_ref() {
        if active.generation_identity() == board.generation_identity()
            && active.signed_catalog_identity() == board.signed_catalog_identity()
        {
            return Ok(Some(Arc::clone(active)));
        }
        if Arc::strong_count(active) > 1 {
            return Err(LegalBoardAssetError::ActiveSessionInUse);
        }
    }
    let prior = guard.slots[slot].replace(Arc::new(board));
    Ok(prior)
}

pub fn remove_qualified_exact_legal_board(
    profile: KickTableProfileId,
) -> Result<Option<Arc<QualifiedExactLegalBoard>>, LegalBoardAssetError> {
    let _mutation = accelerator_registry_mutation_lock()
        .lock()
        .map_err(|_| LegalBoardAssetError::RegistryUnavailable)?;
    let slot = profile_slot(profile)?;
    let registry = LEGAL_BOARD_REGISTRY.get_or_init(|| RwLock::new(LegalBoardRegistry::default()));
    let mut guard = registry
        .write()
        .map_err(|_| LegalBoardAssetError::RegistryUnavailable)?;
    if guard.slots[slot]
        .as_ref()
        .is_some_and(|active| Arc::strong_count(active) > 1)
    {
        return Err(LegalBoardAssetError::ActiveSessionInUse);
    }
    let prior = guard.slots[slot].take();
    Ok(prior)
}

pub(crate) fn qualified_legal_board_snapshot(
    profile: KickTableProfileId,
) -> Option<Arc<QualifiedExactLegalBoard>> {
    let slot = profile_slot(profile).ok()?;
    LEGAL_BOARD_REGISTRY
        .get_or_init(|| RwLock::new(LegalBoardRegistry::default()))
        .read()
        .ok()?
        .slots[slot]
        .clone()
}

pub(crate) fn legal_board_negative_snapshot(
    profile: KickTableProfileId,
) -> Option<LegalBoardNegativeOwner> {
    qualified_legal_board_snapshot(profile)
        .map(LegalBoardNegativeOwner::Complete)
        .or_else(|| {
            trusted_legal_board_synopsis_snapshot(profile)
                .map(LegalBoardNegativeOwner::TrustedSynopsis)
        })
}

fn trusted_legal_board_synopsis_snapshot(
    profile: KickTableProfileId,
) -> Option<Arc<LegalBoardNegativeSynopsis>> {
    let slot = profile_slot(profile).ok()?;
    LEGAL_BOARD_SYNOPSIS_REGISTRY
        .get()?
        .read()
        .ok()?
        .get(slot)?
        .clone()
}

/// Install a small negative-only derivative delivered by the trusted browser
/// owner. The caller must have authenticated the embedded catalog and must
/// not accept this wire from an external client or network endpoint.
pub fn install_trusted_legal_board_synopsis(
    wire: &[u8],
    authority: &VerifiedAcceleratorAuthority,
) -> Result<(), LegalBoardAssetError> {
    let synopsis = LegalBoardNegativeSynopsis::from_trusted_worker_wire(wire, authority)
        .map_err(|_| LegalBoardAssetError::NotQualified)?;
    let _mutation = accelerator_registry_mutation_lock()
        .lock()
        .map_err(|_| LegalBoardAssetError::RegistryUnavailable)?;
    let slot = profile_slot(synopsis.binding.kick_profile)?;
    if qualified_legal_board_snapshot(synopsis.binding.kick_profile).is_some() {
        return Err(LegalBoardAssetError::ActiveSessionInUse);
    }
    let combined = synopsis
        .retained_bytes()
        .saturating_add(installed_legal_board_bytes(None)?)
        .saturating_add(installed_legal_board_synopsis_bytes(Some(slot))?)
        .saturating_add(
            crate::conditioned_reachability::installed_conditioned_reachability_bytes(None)
                .map_err(|_| LegalBoardAssetError::RegistryUnavailable)?,
        )
        .saturating_add(
            crate::conditioned_local_product::installed_local_relation_bytes(None)
                .map_err(|_| LegalBoardAssetError::RegistryUnavailable)?,
        );
    if combined > MAX_ACTIVE_ACCELERATOR_BYTES {
        return Err(LegalBoardAssetError::ActiveSessionTooLarge);
    }
    let registry =
        LEGAL_BOARD_SYNOPSIS_REGISTRY.get_or_init(|| RwLock::new(std::array::from_fn(|_| None)));
    let mut guard = registry
        .write()
        .map_err(|_| LegalBoardAssetError::RegistryUnavailable)?;
    if guard[slot]
        .as_ref()
        .is_some_and(|active| Arc::strong_count(active) > 1)
    {
        return Err(LegalBoardAssetError::ActiveSessionInUse);
    }
    guard[slot] = Some(Arc::new(synopsis));
    Ok(())
}

pub fn remove_trusted_legal_board_synopsis(
    profile: KickTableProfileId,
) -> Result<bool, LegalBoardAssetError> {
    let _mutation = accelerator_registry_mutation_lock()
        .lock()
        .map_err(|_| LegalBoardAssetError::RegistryUnavailable)?;
    let slot = profile_slot(profile)?;
    let Some(registry) = LEGAL_BOARD_SYNOPSIS_REGISTRY.get() else {
        return Ok(false);
    };
    let mut guard = registry
        .write()
        .map_err(|_| LegalBoardAssetError::RegistryUnavailable)?;
    if guard[slot]
        .as_ref()
        .is_some_and(|active| Arc::strong_count(active) > 1)
    {
        return Err(LegalBoardAssetError::ActiveSessionInUse);
    }
    Ok(guard[slot].take().is_some())
}

pub(crate) fn installed_legal_board_synopsis_bytes(
    exclude_slot: Option<usize>,
) -> Result<usize, LegalBoardAssetError> {
    let Some(registry) = LEGAL_BOARD_SYNOPSIS_REGISTRY.get() else {
        return Ok(0);
    };
    let guard = registry
        .read()
        .map_err(|_| LegalBoardAssetError::RegistryUnavailable)?;
    Ok(guard
        .iter()
        .enumerate()
        .filter(|(index, _)| Some(*index) != exclude_slot)
        .filter_map(|(_, synopsis)| synopsis.as_ref())
        .fold(0_usize, |sum, synopsis| {
            sum.saturating_add(synopsis.retained_bytes())
        }))
}

pub(crate) fn installed_legal_board_bytes(
    exclude_slot: Option<usize>,
) -> Result<usize, LegalBoardAssetError> {
    let Some(registry) = LEGAL_BOARD_REGISTRY.get() else {
        return Ok(0);
    };
    let guard = registry
        .read()
        .map_err(|_| LegalBoardAssetError::RegistryUnavailable)?;
    Ok(guard
        .slots
        .iter()
        .enumerate()
        .filter(|(index, _)| Some(*index) != exclude_slot)
        .filter_map(|(_, slot)| slot.as_ref())
        .fold(0_usize, |total, board| {
            total.saturating_add(board.shared_bytes())
        }))
}

/// Host-side fast path for an already pinned immutable generation. This does
/// not expose the payload and lets repeated native requests avoid rereading a
/// complete bundle merely to rediscover the same signed identity.
pub fn active_qualified_exact_legal_board_identity(
    profile: KickTableProfileId,
) -> Option<([u8; 32], [u8; 32])> {
    let board = qualified_legal_board_snapshot(profile)?;
    Some((board.generation_identity(), board.signed_catalog_identity()))
}

/// The complete signed bundle remains installed only in this owner. A peer
/// receives no source payload or sparse index, only a bounded negative-only
/// derivative whose positive results still enter the exact solver.
pub fn export_qualified_legal_board_synopsis(
    profile: KickTableProfileId,
    maximum_wire_bytes: usize,
) -> Result<Option<Vec<u8>>, LegalBoardSynopsisError> {
    let Some(board) = qualified_legal_board_snapshot(profile) else {
        return Ok(None);
    };
    if maximum_wire_bytes > MAX_DISTRIBUTED_SYNOPSIS_BYTES {
        return Err(LegalBoardSynopsisError::BudgetTooSmall);
    }
    let synopsis = board.negative_synopsis(maximum_wire_bytes.saturating_sub(256))?;
    let wire = synopsis.to_trusted_worker_wire()?;
    if wire.len() > maximum_wire_bytes {
        return Err(LegalBoardSynopsisError::BudgetTooSmall);
    }
    Ok(Some(wire))
}

impl ExactLegalBoard {
    fn negative_synopsis(
        &self,
        maximum_bytes: usize,
        signed_catalog_identity: [u8; 32],
    ) -> Result<LegalBoardNegativeSynopsis, LegalBoardSynopsisError> {
        let structural_bytes = core::mem::size_of::<LegalBoardNegativeSynopsis>()
            + LAYER_COUNT * (core::mem::size_of::<NegativeFilter>() + core::mem::size_of::<u64>());
        if maximum_bytes < structural_bytes {
            return Err(LegalBoardSynopsisError::BudgetTooSmall);
        }
        let mut word_counts: [usize; LAYER_COUNT] =
            std::array::from_fn(|layer| self.layers[layer].negative_filter.words.len());
        let mut total_words = word_counts.iter().sum::<usize>();
        let word_budget = (maximum_bytes
            - core::mem::size_of::<LegalBoardNegativeSynopsis>()
            - LAYER_COUNT * core::mem::size_of::<NegativeFilter>())
            / core::mem::size_of::<u64>();
        while total_words > word_budget {
            let (layer, &count) = word_counts
                .iter()
                .enumerate()
                .filter(|(_, count)| **count > 1)
                .max_by_key(|(_, count)| **count)
                .ok_or(LegalBoardSynopsisError::BudgetTooSmall)?;
            let folded_count = count / 2;
            word_counts[layer] = folded_count;
            total_words -= folded_count;
        }
        let mut filters = Vec::new();
        filters
            .try_reserve_exact(LAYER_COUNT)
            .map_err(|_| LegalBoardSynopsisError::AllocationUnavailable)?;
        for (layer, &word_count) in word_counts.iter().enumerate() {
            filters.push(self.layers[layer].negative_filter.folded(word_count)?);
        }
        let synopsis = LegalBoardNegativeSynopsis {
            binding: self.binding,
            generation_identity: self.generation_identity,
            signed_catalog_identity,
            filters,
        };
        if synopsis.retained_bytes() > maximum_bytes {
            return Err(LegalBoardSynopsisError::BudgetTooSmall);
        }
        Ok(synopsis)
    }

    pub fn load(
        bytes: Arc<[u8]>,
        expectation: LegalBoardExpectation,
    ) -> Result<Self, LegalBoardAssetError> {
        if bytes.len() > MAX_BUNDLE_BYTES {
            return Err(LegalBoardAssetError::TooLarge);
        }
        if bytes.len() < PAYLOAD_OFFSET || bytes.get(..8) != Some(MAGIC.as_slice()) {
            return Err(LegalBoardAssetError::Header);
        }
        if read_u32(&bytes[8..12])? != VERSION {
            return Err(LegalBoardAssetError::UnsupportedVersion);
        }
        let profile = decode_profile(bytes[12])?;
        if bytes[13] != EXACT_INTERSECTION_KIND || usize::from(bytes[14]) != LAYER_COUNT {
            return Err(LegalBoardAssetError::NotExactIntersection);
        }
        if bytes[15] != 0 || bytes[128..HEADER_BYTES].iter().any(|value| *value != 0) {
            return Err(LegalBoardAssetError::Header);
        }
        let rule_identity = array32(&bytes[16..48])?;
        let payload_digest = array32(&bytes[48..80])?;
        let generation_identity = array32(&bytes[80..112])?;
        let declared_length = usize::try_from(read_u64(&bytes[112..120])?)
            .map_err(|_| LegalBoardAssetError::Header)?;
        let declared_payload_offset = usize::try_from(read_u64(&bytes[120..128])?)
            .map_err(|_| LegalBoardAssetError::Header)?;
        if declared_length != bytes.len() || declared_payload_offset != PAYLOAD_OFFSET {
            return Err(LegalBoardAssetError::Header);
        }
        let binding = LegalBoardBinding {
            kick_profile: profile,
            rule_identity,
        };
        if binding != expectation.binding {
            return Err(LegalBoardAssetError::BindingMismatch);
        }
        if expectation
            .generation_identity
            .is_some_and(|expected| expected != generation_identity)
        {
            return Err(LegalBoardAssetError::SnapshotMismatch);
        }
        if Sha256::digest(&bytes[PAYLOAD_OFFSET..]).as_slice() != payload_digest {
            return Err(LegalBoardAssetError::PayloadDigest);
        }

        let directories = parse_directories(&bytes)?;
        let expected_generation = generation_identity_for(
            binding,
            payload_digest,
            &bytes[HEADER_BYTES..PAYLOAD_OFFSET],
        );
        if expected_generation != generation_identity {
            return Err(LegalBoardAssetError::GenerationIdentity);
        }
        let mut layers = Vec::with_capacity(LAYER_COUNT);
        for (layer, directory) in directories.into_iter().enumerate() {
            layers.push(index_layer(&bytes, layer, directory)?);
        }
        let layers: [LayerIndex; LAYER_COUNT] = layers
            .try_into()
            .map_err(|_| LegalBoardAssetError::Directory)?;
        if !layer_contains(&bytes, &layers[0], 0)?
            || !layer_contains(&bytes, &layers[10], FIELD_MASK)?
        {
            return Err(LegalBoardAssetError::TerminalDomainIncomplete);
        }
        Ok(Self {
            bytes,
            binding,
            generation_identity,
            layers: Arc::new(layers),
        })
    }

    pub const fn binding(&self) -> LegalBoardBinding {
        self.binding
    }

    pub const fn generation_identity(&self) -> [u8; 32] {
        self.generation_identity
    }

    pub fn compressed_bytes(&self) -> usize {
        self.bytes.len()
    }

    pub fn sparse_index_bytes(&self) -> usize {
        self.layers
            .iter()
            .map(|layer| {
                layer.checkpoints.capacity() * core::mem::size_of::<Checkpoint>()
                    + layer.negative_filter.words.capacity() * core::mem::size_of::<u64>()
            })
            .sum()
    }

    pub fn layer_count(&self, layer: usize) -> Option<u64> {
        self.layers.get(layer).map(|value| value.directory.count)
    }

    pub fn layer_payload_digest(&self, layer: usize) -> Option<[u8; 32]> {
        self.layers.get(layer).map(|value| value.directory.digest)
    }

    fn scoped_storage_key(&self, query: LegalBoardQuery) -> Result<(usize, u64), ProviderStatus> {
        scoped_storage_key_for_binding(self.binding, query)
    }

    pub fn decide_negative_only(&self, query: LegalBoardQuery) -> LegalBoardDecision {
        let (layer, storage_key) = match self.scoped_storage_key(query) {
            Ok(value) => value,
            Err(status) => return LegalBoardDecision::PassThrough(status),
        };
        if self.layers[layer].negative_filter.may_contain(storage_key) {
            LegalBoardDecision::CandidateAllowed
        } else {
            LegalBoardDecision::VerifiedAbsent
        }
    }

    pub fn decide(&self, query: LegalBoardQuery) -> LegalBoardDecision {
        let (layer, storage_key) = match self.scoped_storage_key(query) {
            Ok(value) => value,
            Err(status) => return LegalBoardDecision::PassThrough(status),
        };
        if !self.layers[layer].negative_filter.may_contain(storage_key) {
            return LegalBoardDecision::VerifiedAbsent;
        }
        match layer_contains(&self.bytes, &self.layers[layer], storage_key) {
            Ok(true) => LegalBoardDecision::CandidateAllowed,
            Ok(false) => LegalBoardDecision::VerifiedAbsent,
            Err(_) => LegalBoardDecision::PassThrough(ProviderStatus::InvalidAsset),
        }
    }
}

fn scoped_storage_key_for_binding(
    binding: LegalBoardBinding,
    query: LegalBoardQuery,
) -> Result<(usize, u64), ProviderStatus> {
    if query.width != 10
        || query.height != 4
        || query.initial_board != 0
        || query.placed_piece_count >= LAYER_COUNT
        || query.completion != CompletionCapability::ClearToEmpty
    {
        return Err(ProviderStatus::OutOfScope);
    }
    if query.kick_profile != binding.kick_profile {
        return Err(ProviderStatus::SnapshotMismatch);
    }
    let frame = match OriginalRowFrame::from_deleted_rows(query.deleted_original_rows) {
        Ok(value) => value,
        Err(_) => return Err(ProviderStatus::OutOfScope),
    };
    let normalized = match frame.normalize_product_board(query.physical_board) {
        Ok(value) => value,
        Err(_) => return Err(ProviderStatus::OutOfScope),
    };
    if normalized.count_ones() != (query.placed_piece_count as u32) * 4 {
        return Err(ProviderStatus::OutOfScope);
    }
    // The immutable domain layers and bundle encode each 10-bit row in
    // right-to-left storage order. BuildUp owns a left-to-right Board64
    // mask. A row mirror is not generally a legal-board symmetry under
    // ordered kicks, so membership must use the bundle's exact key.
    let storage_key = bundle_key_from_clearra_board(normalized);
    Ok((query.placed_piece_count, storage_key))
}

/// Encode strictly sorted `L_k = F_k ∩ R_k` storage-key layers. Each row's
/// bit order is right-to-left, as in the independently generated domain files;
/// callers must not pass Clearra's left-to-right Board64 masks directly.
/// Qualification and
/// publication remain external; this routine only creates the immutable
/// representation and its self-authenticating generation identity.
pub fn encode_exact_intersection(
    binding: LegalBoardBinding,
    layers: &[Vec<u64>; LAYER_COUNT],
) -> Result<Vec<u8>, LegalBoardAssetError> {
    validate_terminal_layers(layers)?;
    let mut output = vec![0_u8; PAYLOAD_OFFSET];
    output[..8].copy_from_slice(MAGIC);
    output[8..12].copy_from_slice(&VERSION.to_le_bytes());
    output[12] = encode_profile(binding.kick_profile)?;
    output[13] = EXACT_INTERSECTION_KIND;
    output[14] = LAYER_COUNT as u8;
    output[16..48].copy_from_slice(&binding.rule_identity);

    let mut cursor = PAYLOAD_OFFSET;
    for (layer, fields) in layers.iter().enumerate() {
        validate_layer(layer, fields)?;
        let begin = output.len();
        let mut prior = 0_u64;
        for (ordinal, &field) in fields.iter().enumerate() {
            let delta = if ordinal == 0 {
                field
            } else {
                field
                    .checked_sub(prior)
                    .ok_or(LegalBoardAssetError::NonCanonicalEncoding)?
            };
            write_uleb128(delta, &mut output);
            prior = field;
        }
        let length = output.len() - begin;
        let digest: [u8; 32] = Sha256::digest(&output[begin..]).into();
        write_directory(
            &mut output[..PAYLOAD_OFFSET],
            layer,
            LayerDirectory {
                count: fields.len() as u64,
                offset: cursor,
                length,
                digest,
            },
        )?;
        cursor = output.len();
        if output.len() > MAX_BUNDLE_BYTES {
            return Err(LegalBoardAssetError::TooLarge);
        }
    }
    let payload_digest: [u8; 32] = Sha256::digest(&output[PAYLOAD_OFFSET..]).into();
    output[48..80].copy_from_slice(&payload_digest);
    let output_length = output.len() as u64;
    output[112..120].copy_from_slice(&output_length.to_le_bytes());
    output[120..128].copy_from_slice(&(PAYLOAD_OFFSET as u64).to_le_bytes());
    let generation = generation_identity_for(
        binding,
        payload_digest,
        &output[HEADER_BYTES..PAYLOAD_OFFSET],
    );
    output[80..112].copy_from_slice(&generation);
    Ok(output)
}

/// Encode the same immutable format one verified layer at a time. The source
/// callback must visit every field in canonical order; its own I/O failure is
/// kept distinct from a bundle-contract failure. The output remains bounded
/// by the product's compressed 64 MiB limit, independent of raw layer sizes.
#[derive(Debug)]
pub enum LegalBoardStreamEncodeError<E> {
    Asset(LegalBoardAssetError),
    Source(E),
}

pub fn encode_exact_intersection_streaming<E, F>(
    binding: LegalBoardBinding,
    mut visit_layer: F,
) -> Result<Vec<u8>, LegalBoardStreamEncodeError<E>>
where
    F: FnMut(usize, &mut dyn FnMut(u64) -> Result<(), LegalBoardAssetError>) -> Result<(), E>,
{
    let mut output = vec![0_u8; PAYLOAD_OFFSET];
    output[..8].copy_from_slice(MAGIC);
    output[8..12].copy_from_slice(&VERSION.to_le_bytes());
    output[12] =
        encode_profile(binding.kick_profile).map_err(LegalBoardStreamEncodeError::Asset)?;
    output[13] = EXACT_INTERSECTION_KIND;
    output[14] = LAYER_COUNT as u8;
    output[16..48].copy_from_slice(&binding.rule_identity);

    let mut cursor = PAYLOAD_OFFSET;
    let mut has_empty_origin = false;
    let mut has_full_terminal = false;
    for layer in 0..LAYER_COUNT {
        let begin = output.len();
        let mut prior = None;
        let mut count = 0_u64;
        let mut emit = |field: u64| -> Result<(), LegalBoardAssetError> {
            if field & !FIELD_MASK != 0 || field.count_ones() != (layer as u32) * 4 {
                return Err(LegalBoardAssetError::LayerArea);
            }
            if prior.is_some_and(|value| value >= field) {
                return Err(LegalBoardAssetError::NonCanonicalEncoding);
            }
            if layer == 0 && field == 0 {
                has_empty_origin = true;
            }
            if layer == LAYER_COUNT - 1 && field == FIELD_MASK {
                has_full_terminal = true;
            }
            let delta = prior.map_or(field, |value| field - value);
            write_uleb128(delta, &mut output);
            if output.len() > MAX_BUNDLE_BYTES {
                return Err(LegalBoardAssetError::TooLarge);
            }
            prior = Some(field);
            count = count
                .checked_add(1)
                .ok_or(LegalBoardAssetError::Directory)?;
            Ok(())
        };
        visit_layer(layer, &mut emit).map_err(LegalBoardStreamEncodeError::Source)?;
        drop(emit);
        let length = output.len() - begin;
        let digest: [u8; 32] = Sha256::digest(&output[begin..]).into();
        write_directory(
            &mut output[..PAYLOAD_OFFSET],
            layer,
            LayerDirectory {
                count,
                offset: cursor,
                length,
                digest,
            },
        )
        .map_err(LegalBoardStreamEncodeError::Asset)?;
        cursor = output.len();
    }
    if !has_empty_origin || !has_full_terminal {
        return Err(LegalBoardStreamEncodeError::Asset(
            LegalBoardAssetError::TerminalDomainIncomplete,
        ));
    }
    let payload_digest: [u8; 32] = Sha256::digest(&output[PAYLOAD_OFFSET..]).into();
    output[48..80].copy_from_slice(&payload_digest);
    let output_length = output.len() as u64;
    output[112..120].copy_from_slice(&output_length.to_le_bytes());
    output[120..128].copy_from_slice(&(PAYLOAD_OFFSET as u64).to_le_bytes());
    let generation = generation_identity_for(
        binding,
        payload_digest,
        &output[HEADER_BYTES..PAYLOAD_OFFSET],
    );
    output[80..112].copy_from_slice(&generation);
    Ok(output)
}

fn validate_terminal_layers(layers: &[Vec<u64>; LAYER_COUNT]) -> Result<(), LegalBoardAssetError> {
    if layers[0].binary_search(&0).is_err() || layers[10].binary_search(&FIELD_MASK).is_err() {
        return Err(LegalBoardAssetError::TerminalDomainIncomplete);
    }
    Ok(())
}

fn validate_layer(layer: usize, fields: &[u64]) -> Result<(), LegalBoardAssetError> {
    let expected_area = (layer as u32) * 4;
    let mut prior = None;
    for &field in fields {
        if field & !FIELD_MASK != 0 || field.count_ones() != expected_area {
            return Err(LegalBoardAssetError::LayerArea);
        }
        if prior.is_some_and(|value| value >= field) {
            return Err(LegalBoardAssetError::NonCanonicalEncoding);
        }
        prior = Some(field);
    }
    Ok(())
}

fn parse_directories(bytes: &[u8]) -> Result<[LayerDirectory; LAYER_COUNT], LegalBoardAssetError> {
    let mut directories = Vec::with_capacity(LAYER_COUNT);
    let mut expected_offset = PAYLOAD_OFFSET;
    for layer in 0..LAYER_COUNT {
        let begin = HEADER_BYTES + layer * DIRECTORY_ENTRY_BYTES;
        let entry = &bytes[begin..begin + DIRECTORY_ENTRY_BYTES];
        if usize::from(entry[0]) != layer || entry[1..8].iter().any(|value| *value != 0) {
            return Err(LegalBoardAssetError::Directory);
        }
        let count = read_u64(&entry[8..16])?;
        let offset = usize::try_from(read_u64(&entry[16..24])?)
            .map_err(|_| LegalBoardAssetError::Directory)?;
        let length = usize::try_from(read_u64(&entry[24..32])?)
            .map_err(|_| LegalBoardAssetError::Directory)?;
        let digest = array32(&entry[32..64])?;
        if offset != expected_offset
            || offset
                .checked_add(length)
                .is_none_or(|end| end > bytes.len())
        {
            return Err(LegalBoardAssetError::Directory);
        }
        if Sha256::digest(&bytes[offset..offset + length]).as_slice() != digest {
            return Err(LegalBoardAssetError::PayloadDigest);
        }
        expected_offset = offset + length;
        directories.push(LayerDirectory {
            count,
            offset,
            length,
            digest,
        });
    }
    if expected_offset != bytes.len()
        || bytes[HEADER_BYTES + LAYER_COUNT * DIRECTORY_ENTRY_BYTES..PAYLOAD_OFFSET]
            .iter()
            .any(|value| *value != 0)
    {
        return Err(LegalBoardAssetError::Directory);
    }
    directories
        .try_into()
        .map_err(|_| LegalBoardAssetError::Directory)
}

fn index_layer(
    bytes: &[u8],
    layer: usize,
    directory: LayerDirectory,
) -> Result<LayerIndex, LegalBoardAssetError> {
    let end = directory.offset + directory.length;
    let mut cursor = directory.offset;
    let mut prior = 0_u64;
    let mut checkpoints = Vec::new();
    let mut negative_filter = NegativeFilter::new(directory.count)?;
    for ordinal in 0..directory.count {
        let value_offset = cursor;
        let delta = read_uleb128(bytes, &mut cursor, end)?;
        let value = if ordinal == 0 {
            delta
        } else {
            prior
                .checked_add(delta)
                .ok_or(LegalBoardAssetError::NonCanonicalEncoding)?
        };
        if ordinal > 0 && value <= prior {
            return Err(LegalBoardAssetError::NonCanonicalEncoding);
        }
        if value & !FIELD_MASK != 0 || value.count_ones() != (layer as u32) * 4 {
            return Err(LegalBoardAssetError::LayerArea);
        }
        negative_filter.insert(value);
        if ordinal % CHECKPOINT_STRIDE == 0 {
            checkpoints.push(Checkpoint {
                first_value: value,
                byte_offset: value_offset,
                prior_value: if ordinal == 0 { 0 } else { prior },
                ordinal,
            });
        }
        prior = value;
    }
    if cursor != end || (directory.count == 0 && directory.length != 0) {
        return Err(LegalBoardAssetError::NonCanonicalEncoding);
    }
    Ok(LayerIndex {
        directory,
        checkpoints,
        negative_filter,
    })
}

/// Involutive conversion between Clearra's `y * 10 + x` Board64 layout and
/// the immutable legal-board storage key. It is deliberately owned here, not
/// by the separate PC4 Tablebase product.
pub(crate) fn bundle_key_from_clearra_board(board: u64) -> u64 {
    let mut key = 0_u64;
    for row in 0..4 {
        let cells = ((board >> (row * 10)) & 1023) as u16;
        key |= u64::from(cells.reverse_bits() >> 6) << (row * 10);
    }
    key
}

fn layer_contains(
    bytes: &[u8],
    layer: &LayerIndex,
    target: u64,
) -> Result<bool, LegalBoardAssetError> {
    if layer.directory.count == 0 {
        return Ok(false);
    }
    let checkpoint_index = match layer
        .checkpoints
        .binary_search_by_key(&target, |checkpoint| checkpoint.first_value)
    {
        Ok(index) => return Ok(layer.checkpoints[index].first_value == target),
        Err(0) => return Ok(false),
        Err(index) => index - 1,
    };
    let checkpoint = layer.checkpoints[checkpoint_index];
    let end_ordinal = layer
        .directory
        .count
        .min(checkpoint.ordinal + CHECKPOINT_STRIDE);
    let end = layer.directory.offset + layer.directory.length;
    let mut cursor = checkpoint.byte_offset;
    let mut prior = checkpoint.prior_value;
    for ordinal in checkpoint.ordinal..end_ordinal {
        let delta = read_uleb128(bytes, &mut cursor, end)?;
        let value = if ordinal == 0 { delta } else { prior + delta };
        if value == target {
            return Ok(true);
        }
        if value > target {
            return Ok(false);
        }
        prior = value;
    }
    Ok(false)
}

fn write_directory(
    header: &mut [u8],
    layer: usize,
    directory: LayerDirectory,
) -> Result<(), LegalBoardAssetError> {
    let begin = HEADER_BYTES + layer * DIRECTORY_ENTRY_BYTES;
    let entry = header
        .get_mut(begin..begin + DIRECTORY_ENTRY_BYTES)
        .ok_or(LegalBoardAssetError::Directory)?;
    entry[0] = layer as u8;
    entry[8..16].copy_from_slice(&directory.count.to_le_bytes());
    entry[16..24].copy_from_slice(&(directory.offset as u64).to_le_bytes());
    entry[24..32].copy_from_slice(&(directory.length as u64).to_le_bytes());
    entry[32..64].copy_from_slice(&directory.digest);
    Ok(())
}

fn generation_identity_for(
    binding: LegalBoardBinding,
    payload_digest: [u8; 32],
    directory: &[u8],
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"clearra.legal-board.exact-intersection.generation.v2\0");
    digest.update([encode_profile(binding.kick_profile).unwrap_or(u8::MAX)]);
    digest.update(binding.rule_identity);
    digest.update(payload_digest);
    digest.update(directory);
    digest.finalize().into()
}

fn write_uleb128(mut value: u64, output: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn read_uleb128(bytes: &[u8], cursor: &mut usize, end: usize) -> Result<u64, LegalBoardAssetError> {
    let begin = *cursor;
    let mut value = 0_u64;
    let mut shift = 0_u32;
    loop {
        if *cursor >= end || shift >= 64 {
            return Err(LegalBoardAssetError::NonCanonicalEncoding);
        }
        let byte = bytes[*cursor];
        *cursor += 1;
        let low = u64::from(byte & 0x7f);
        if shift == 63 && low > 1 {
            return Err(LegalBoardAssetError::NonCanonicalEncoding);
        }
        value |= low << shift;
        if byte & 0x80 == 0 {
            let consumed = *cursor - begin;
            if consumed != uleb128_len(value) {
                return Err(LegalBoardAssetError::NonCanonicalEncoding);
            }
            return Ok(value);
        }
        shift += 7;
    }
}

fn uleb128_len(mut value: u64) -> usize {
    let mut length = 1;
    while value >= 0x80 {
        value >>= 7;
        length += 1;
    }
    length
}

fn encode_profile(profile: KickTableProfileId) -> Result<u8, LegalBoardAssetError> {
    u8::try_from(profile_slot(profile)?).map_err(|_| LegalBoardAssetError::UnsupportedProfile)
}

fn profile_slot(profile: KickTableProfileId) -> Result<usize, LegalBoardAssetError> {
    Ok(match profile {
        KickTableProfileId::Srs90 => 0,
        KickTableProfileId::SrsPlus => 1,
        KickTableProfileId::SrsX => 2,
        KickTableProfileId::Jstris180 => 3,
        KickTableProfileId::NoKick => 4,
        _ => return Err(LegalBoardAssetError::UnsupportedProfile),
    })
}

fn decode_profile(value: u8) -> Result<KickTableProfileId, LegalBoardAssetError> {
    match value {
        0 => Ok(KickTableProfileId::Srs90),
        1 => Ok(KickTableProfileId::SrsPlus),
        2 => Ok(KickTableProfileId::SrsX),
        3 => Ok(KickTableProfileId::Jstris180),
        4 => Ok(KickTableProfileId::NoKick),
        _ => Err(LegalBoardAssetError::UnsupportedProfile),
    }
}

fn read_u32(bytes: &[u8]) -> Result<u32, LegalBoardAssetError> {
    Ok(u32::from_le_bytes(
        bytes.try_into().map_err(|_| LegalBoardAssetError::Header)?,
    ))
}

fn read_u64(bytes: &[u8]) -> Result<u64, LegalBoardAssetError> {
    Ok(u64::from_le_bytes(
        bytes.try_into().map_err(|_| LegalBoardAssetError::Header)?,
    ))
}

fn array32(bytes: &[u8]) -> Result<[u8; 32], LegalBoardAssetError> {
    bytes.try_into().map_err(|_| LegalBoardAssetError::Header)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_accelerator_activation::{
        verify_accelerator_envelope, PinnedPublicKey, StaticPublicKeyring, ASSET_STATEMENT_SCHEMA,
        SIGNATURE_ALGORITHM, SIGNATURE_DOMAIN, SIGNED_ASSET_ENVELOPE_SCHEMA,
    };
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::json;

    fn signed_test_authority(
        binding: LegalBoardBinding,
        generation: [u8; 32],
        payload: &[u8],
    ) -> VerifiedAcceleratorAuthority {
        let signing = SigningKey::from_bytes(&[43_u8; 32]);
        let hex = |bytes: &[u8]| {
            bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        };
        let statement = serde_json::to_string(&json!({
            "algorithm": SIGNATURE_ALGORITHM,
            "asset_url": "https://github.com/daejunnom/Clearra/releases/download/test/legal.cllb",
            "completeness_scope": EXACT_LEGAL_BOARD_COMPLETENESS_SCOPE,
            "generation_identity": hex(&generation),
            "key_id": "test-only-legal-synopsis",
            "payload_bytes": payload.len().to_string(),
            "payload_identity": hex(&Sha256::digest(payload)),
            "product": AcceleratorProduct::ExactLegalBoard.as_str(),
            "profile": accelerator_profile_name(binding.kick_profile).unwrap(),
            "qualification_identity": "aa".repeat(32),
            "repository": "daejunnom/Clearra",
            "revision": "bb".repeat(20),
            "rule_identity": hex(&binding.rule_identity),
            "schema": ASSET_STATEMENT_SCHEMA
        }))
        .unwrap();
        let mut signed = SIGNATURE_DOMAIN.to_vec();
        signed.extend_from_slice(statement.as_bytes());
        let envelope = serde_json::to_string(&json!({
            "schema": SIGNED_ASSET_ENVELOPE_SCHEMA,
            "signature_hex": hex(&signing.sign(&signed).to_bytes()),
            "statement_json": statement
        }))
        .unwrap();
        let keys = [PinnedPublicKey {
            key_id: "test-only-legal-synopsis",
            public_key: signing.verifying_key().to_bytes(),
        }];
        verify_accelerator_envelope(&envelope, StaticPublicKeyring::new(&keys)).unwrap()
    }

    fn binding() -> LegalBoardBinding {
        LegalBoardBinding {
            kick_profile: KickTableProfileId::Jstris180,
            rule_identity: [7; 32],
        }
    }

    #[test]
    fn accelerator_profiles_use_the_public_catalog_names() {
        assert_eq!(
            accelerator_profile_name(KickTableProfileId::Srs90).unwrap(),
            "srs"
        );
        assert_eq!(
            accelerator_profile_name(KickTableProfileId::SrsPlus).unwrap(),
            "srs-plus"
        );
        assert_eq!(
            accelerator_profile_name(KickTableProfileId::SrsX).unwrap(),
            "srs-x"
        );
        assert_eq!(
            accelerator_profile_name(KickTableProfileId::Jstris180).unwrap(),
            "jstris-180"
        );
        assert_eq!(
            accelerator_profile_name(KickTableProfileId::NoKick).unwrap(),
            "no-kick"
        );
        assert!(accelerator_profile_name(KickTableProfileId::Custom).is_err());
    }

    fn layers() -> [Vec<u64>; LAYER_COUNT] {
        let mut layers: [Vec<u64>; LAYER_COUNT] = std::array::from_fn(|_| Vec::new());
        layers[0].push(0);
        layers[1].extend([
            bundle_key_from_clearra_board(0b1111),
            bundle_key_from_clearra_board(0b1111_0000),
        ]);
        layers[1].sort_unstable();
        layers[3].push(bundle_key_from_clearra_board(ROW_MASK | (0b11 << 30)));
        layers[10].push(FIELD_MASK);
        layers
    }

    #[test]
    fn asymmetric_row_storage_key_preserves_exact_membership() {
        // One known SRS+ P7P4 solution was falsely rejected at layer six
        // because the bundle stored row-reversed keys but lookup used Board64.
        let board = 0xf830_3d1bff_u64;
        let key = bundle_key_from_clearra_board(board);
        assert_eq!(key, 0x07f0_362fff);
        assert_eq!(bundle_key_from_clearra_board(key), board);
        assert_ne!(key, board);

        let mut layers: [Vec<u64>; LAYER_COUNT] = std::array::from_fn(|_| Vec::new());
        layers[0].push(0);
        layers[6].push(key);
        layers[10].push(FIELD_MASK);
        let encoded = encode_exact_intersection(binding(), &layers).unwrap();
        let loaded = ExactLegalBoard::load(
            Arc::from(encoded),
            LegalBoardExpectation {
                binding: binding(),
                generation_identity: None,
            },
        )
        .unwrap();
        let frame = OriginalRowFrame::from_deleted_rows(1 << 1).unwrap();
        let query = LegalBoardQuery {
            width: 10,
            height: 4,
            initial_board: 0,
            kick_profile: KickTableProfileId::Jstris180,
            physical_board: frame.compact_physical_board(board).unwrap(),
            deleted_original_rows: 1 << 1,
            placed_piece_count: 6,
            completion: CompletionCapability::ClearToEmpty,
        };
        assert_eq!(loaded.decide(query), LegalBoardDecision::CandidateAllowed);
        assert_eq!(
            loaded.decide_negative_only(query),
            LegalBoardDecision::CandidateAllowed
        );
    }

    #[test]
    fn storage_key_matches_independent_four_row_bit_mapping() {
        for row in 0..4 {
            for pattern in 0_u64..1024 {
                let board = pattern << (row * 10);
                let expected = (0..10).fold(0_u64, |key, x| {
                    if pattern & (1 << x) == 0 {
                        key
                    } else {
                        key | (1 << (row * 10 + 9 - x))
                    }
                });
                assert_eq!(bundle_key_from_clearra_board(board), expected);
                assert_eq!(bundle_key_from_clearra_board(expected), board);
            }
        }
    }

    #[test]
    fn blocked_negative_filter_never_rejects_inserted_keys() {
        for size in [1_u64, 7, 8, 9, 127, 1024, 8193] {
            let mut filter = NegativeFilter::new(size).unwrap();
            let keys = (0..size)
                .map(|index| index.wrapping_mul(0x9e37_79b9_7f4a_7c15))
                .collect::<Vec<_>>();
            for &key in &keys {
                filter.insert(key);
            }
            for &key in &keys {
                assert!(filter.may_contain(key));
            }
        }
    }

    #[test]
    fn folding_a_negative_filter_preserves_every_inserted_key() {
        let mut filter = NegativeFilter::new(8193).unwrap();
        let keys = (0_u64..8193)
            .map(|index| index.wrapping_mul(0x9e37_79b9_7f4a_7c15))
            .collect::<Vec<_>>();
        for &key in &keys {
            filter.insert(key);
        }
        assert!(filter.words.len() >= 2048);
        for target_words in [1, 2, 16, 256, filter.words.len()] {
            let folded = filter.folded(target_words).unwrap();
            assert_eq!(folded.words.len(), target_words);
            assert!(keys.iter().all(|&key| folded.may_contain(key)));
        }
    }

    #[test]
    fn bounded_negative_synopsis_retains_binding_and_never_rejects_known_layers() {
        let input_layers = layers();
        let encoded = encode_exact_intersection(binding(), &input_layers).unwrap();
        let board = ExactLegalBoard::load(
            Arc::from(encoded),
            LegalBoardExpectation {
                binding: binding(),
                generation_identity: None,
            },
        )
        .unwrap();
        // Construct the wrapper only inside this unit test. Product code can
        // obtain it solely through a verified signing authority.
        let qualified = QualifiedExactLegalBoard {
            board,
            signed_catalog_identity: [3; 32],
            exhaustive_differential_identity: [4; 32],
        };
        let minimum_bytes = core::mem::size_of::<LegalBoardNegativeSynopsis>()
            + LAYER_COUNT * (core::mem::size_of::<NegativeFilter>() + core::mem::size_of::<u64>());
        assert!(matches!(
            qualified.negative_synopsis(minimum_bytes - 1),
            Err(LegalBoardSynopsisError::BudgetTooSmall)
        ));
        let synopsis = qualified.negative_synopsis(minimum_bytes).unwrap();
        assert!(synopsis.retained_bytes() <= minimum_bytes);
        assert_eq!(
            synopsis.generation_identity(),
            qualified.generation_identity()
        );
        assert_eq!(synopsis.signed_catalog_identity(), [3; 32]);
        for (layer, keys) in input_layers.iter().enumerate() {
            for &storage_key in keys {
                let query = LegalBoardQuery {
                    width: 10,
                    height: 4,
                    initial_board: 0,
                    kick_profile: binding().kick_profile,
                    physical_board: bundle_key_from_clearra_board(storage_key),
                    deleted_original_rows: 0,
                    placed_piece_count: layer,
                    completion: CompletionCapability::ClearToEmpty,
                };
                assert_eq!(
                    synopsis.decide_negative_only(query),
                    LegalBoardDecision::CandidateAllowed,
                    "folded synopsis rejected a stored layer {layer} key"
                );
                assert_eq!(
                    synopsis.decide_negative_only(LegalBoardQuery {
                        initial_board: 1,
                        ..query
                    }),
                    LegalBoardDecision::PassThrough(ProviderStatus::OutOfScope)
                );
            }
        }
    }

    #[test]
    fn trusted_worker_synopsis_roundtrip_preserves_negative_scope_and_rejects_corruption() {
        let binding = built_in_binding(KickTableProfileId::Jstris180).unwrap();
        let input_layers = layers();
        let encoded = encode_exact_intersection(binding, &input_layers).unwrap();
        let complete = ExactLegalBoard::load(
            Arc::from(encoded.clone()),
            LegalBoardExpectation {
                binding,
                generation_identity: None,
            },
        )
        .unwrap();
        let authority = signed_test_authority(binding, complete.generation_identity(), &encoded);
        let qualified = QualifiedExactLegalBoard::qualify(complete, &authority).unwrap();
        let synopsis = qualified.negative_synopsis(64 * 1024).unwrap();
        let wire = synopsis.to_trusted_worker_wire().unwrap();
        let restored =
            LegalBoardNegativeSynopsis::from_trusted_worker_wire(&wire, &authority).unwrap();
        assert!(wire.len() <= 64 * 1024);
        let other_generation = signed_test_authority(binding, [17; 32], &encoded);
        assert_eq!(
            LegalBoardNegativeSynopsis::from_trusted_worker_wire(&wire, &other_generation)
                .unwrap_err(),
            LegalBoardSynopsisError::AuthorityMismatch
        );
        let other_profile = signed_test_authority(
            built_in_binding(KickTableProfileId::Srs90).unwrap(),
            authority.generation_identity(),
            &encoded,
        );
        assert_eq!(
            LegalBoardNegativeSynopsis::from_trusted_worker_wire(&wire, &other_profile)
                .unwrap_err(),
            LegalBoardSynopsisError::AuthorityMismatch
        );
        for (layer, keys) in input_layers.iter().enumerate() {
            for &key in keys {
                let query = LegalBoardQuery {
                    width: 10,
                    height: 4,
                    initial_board: 0,
                    kick_profile: KickTableProfileId::Jstris180,
                    physical_board: bundle_key_from_clearra_board(key),
                    deleted_original_rows: 0,
                    placed_piece_count: layer,
                    completion: CompletionCapability::ClearToEmpty,
                };
                assert_eq!(
                    restored.decide_negative_only(query),
                    LegalBoardDecision::CandidateAllowed
                );
                assert_eq!(
                    restored.decide_negative_only(LegalBoardQuery { height: 5, ..query }),
                    LegalBoardDecision::PassThrough(ProviderStatus::OutOfScope)
                );
            }
        }
        let mut corrupted = wire.clone();
        *corrupted.last_mut().unwrap() ^= 1;
        assert_eq!(
            LegalBoardNegativeSynopsis::from_trusted_worker_wire(&corrupted, &authority)
                .unwrap_err(),
            LegalBoardSynopsisError::InvalidWire
        );
        let mut tampered_words = wire.clone();
        tampered_words[SYNOPSIS_HEADER_BYTES] ^= 1;
        let digest: [u8; 32] = Sha256::digest(&tampered_words[..wire.len() - 32]).into();
        tampered_words[wire.len() - 32..].copy_from_slice(&digest);
        // Same-origin transport is the trust boundary: a recomputed checksum
        // is not a proof that the derivative came from the signed bundle.
        assert!(
            LegalBoardNegativeSynopsis::from_trusted_worker_wire(&tampered_words, &authority)
                .is_ok()
        );

        install_trusted_legal_board_synopsis(&wire, &authority).unwrap();
        let snapshot = legal_board_negative_snapshot(KickTableProfileId::Jstris180).unwrap();
        let known = LegalBoardQuery {
            width: 10,
            height: 4,
            initial_board: 0,
            kick_profile: KickTableProfileId::Jstris180,
            physical_board: 0b1111,
            deleted_original_rows: 0,
            placed_piece_count: 1,
            completion: CompletionCapability::ClearToEmpty,
        };
        assert_eq!(
            snapshot.decide_negative_only(known),
            LegalBoardDecision::CandidateAllowed
        );
        assert_eq!(
            remove_trusted_legal_board_synopsis(KickTableProfileId::Jstris180),
            Err(LegalBoardAssetError::ActiveSessionInUse)
        );
        drop(snapshot);
        assert!(remove_trusted_legal_board_synopsis(KickTableProfileId::Jstris180).unwrap());
        assert!(legal_board_negative_snapshot(KickTableProfileId::Jstris180).is_none());
    }

    #[test]
    fn negative_filter_false_positive_never_becomes_exact_membership() {
        let encoded = encode_exact_intersection(binding(), &layers()).unwrap();
        let mut loaded = ExactLegalBoard::load(
            Arc::from(encoded),
            LegalBoardExpectation {
                binding: binding(),
                generation_identity: None,
            },
        )
        .unwrap();
        Arc::get_mut(&mut loaded.layers).unwrap()[1]
            .negative_filter
            .words
            .fill(u64::MAX);
        let absent = LegalBoardQuery {
            width: 10,
            height: 4,
            initial_board: 0,
            kick_profile: KickTableProfileId::Jstris180,
            physical_board: 0b11110,
            deleted_original_rows: 0,
            placed_piece_count: 1,
            completion: CompletionCapability::ClearToEmpty,
        };
        assert_eq!(
            loaded.decide_negative_only(absent),
            LegalBoardDecision::CandidateAllowed
        );
        assert_eq!(loaded.decide(absent), LegalBoardDecision::VerifiedAbsent);
    }

    #[test]
    fn original_deleted_rows_normalize_to_one_bottom_prefix_codec() {
        let physical = 0b11 << 10;
        let row_two_deleted = OriginalRowFrame::from_deleted_rows(1 << 2).unwrap();
        assert_eq!(row_two_deleted.deleted_original_rows(), 1 << 2);
        assert_eq!(
            row_two_deleted.normalize_product_board(physical).unwrap(),
            ROW_MASK | (0b11 << 20)
        );

        let rows_zero_and_three = OriginalRowFrame::from_deleted_rows((1 << 0) | (1 << 3)).unwrap();
        assert_eq!(
            rows_zero_and_three.normalize_product_board(0b101).unwrap(),
            (1_u64 << 20) - 1 | (0b101 << 20)
        );
        assert!(OriginalRowFrame::from_deleted_rows(1 << 4).is_err());
        assert!(row_two_deleted.normalize_product_board(1 << 30).is_err());
    }

    #[test]
    fn every_deleted_row_mask_round_trips_compact_and_replay_coordinates() {
        for deleted_rows in 0_u16..16 {
            let frame = OriginalRowFrame::from_deleted_rows(deleted_rows).unwrap();
            let surviving_bits = u32::from(frame.surviving_original_rows()) * 10;
            let physical = if surviving_bits == 0 {
                0
            } else {
                0x29a5_a55a_5a5a_u64 & ((1_u64 << surviving_bits) - 1)
            };
            let product = frame.normalize_product_board(physical).unwrap();
            assert_eq!(frame.compact_physical_board(product).unwrap(), physical);

            let replay = frame.replay_frame_board(physical).unwrap();
            let mut compacted = 0_u64;
            let mut compacted_row = 0_u32;
            for original_row in 0_u32..4 {
                let row = (replay >> (original_row * 10)) & ROW_MASK;
                if deleted_rows & (1 << original_row) != 0 {
                    assert_eq!(row, ROW_MASK);
                } else {
                    compacted |= row << (compacted_row * 10);
                    compacted_row += 1;
                }
            }
            assert_eq!(compacted, physical);
        }
    }

    #[test]
    fn exact_bundle_round_trips_without_decoding_whole_layers() {
        let encoded = encode_exact_intersection(binding(), &layers()).unwrap();
        let generation: [u8; 32] = encoded[80..112].try_into().unwrap();
        let loaded = ExactLegalBoard::load(
            Arc::from(encoded),
            LegalBoardExpectation {
                binding: binding(),
                generation_identity: Some(generation),
            },
        )
        .unwrap();
        assert_eq!(loaded.layer_count(1), Some(2));
        assert!(loaded.sparse_index_bytes() < loaded.compressed_bytes());
        assert_eq!(
            loaded.decide(LegalBoardQuery {
                width: 10,
                height: 4,
                initial_board: 0,
                kick_profile: KickTableProfileId::Jstris180,
                physical_board: 0b1111,
                deleted_original_rows: 0,
                placed_piece_count: 1,
                completion: CompletionCapability::ClearToEmpty,
            }),
            LegalBoardDecision::CandidateAllowed
        );
        assert_eq!(
            loaded.decide(LegalBoardQuery {
                physical_board: 0b11110,
                ..LegalBoardQuery {
                    width: 10,
                    height: 4,
                    initial_board: 0,
                    kick_profile: KickTableProfileId::Jstris180,
                    physical_board: 0,
                    deleted_original_rows: 0,
                    placed_piece_count: 1,
                    completion: CompletionCapability::ClearToEmpty,
                }
            }),
            LegalBoardDecision::VerifiedAbsent
        );
    }

    #[test]
    fn streamed_layer_encoder_matches_existing_bundle_bytes() {
        let layers = layers();
        let expected = encode_exact_intersection(binding(), &layers).unwrap();
        let observed = encode_exact_intersection_streaming(binding(), |layer, emit| {
            for &field in &layers[layer] {
                emit(field).map_err(|error| error.code().to_owned())?;
            }
            Ok::<(), String>(())
        })
        .unwrap();
        assert_eq!(observed, expected);
    }

    #[test]
    fn scope_and_binding_fail_open_but_corrupt_assets_fail_load() {
        let mut encoded = encode_exact_intersection(binding(), &layers()).unwrap();
        let loaded = ExactLegalBoard::load(
            Arc::from(encoded.clone()),
            LegalBoardExpectation {
                binding: binding(),
                generation_identity: None,
            },
        )
        .unwrap();
        let query = LegalBoardQuery {
            width: 10,
            height: 4,
            initial_board: 0,
            kick_profile: KickTableProfileId::Jstris180,
            physical_board: 0b11110,
            deleted_original_rows: 0,
            placed_piece_count: 1,
            completion: CompletionCapability::Other,
        };
        assert_eq!(
            loaded.decide(query),
            LegalBoardDecision::PassThrough(ProviderStatus::OutOfScope)
        );
        assert_eq!(
            loaded.decide(LegalBoardQuery {
                completion: CompletionCapability::ClearToEmpty,
                kick_profile: KickTableProfileId::SrsPlus,
                ..query
            }),
            LegalBoardDecision::PassThrough(ProviderStatus::SnapshotMismatch)
        );
        let last = encoded.len() - 1;
        encoded[last] ^= 1;
        assert_eq!(
            ExactLegalBoard::load(
                Arc::from(encoded),
                LegalBoardExpectation {
                    binding: binding(),
                    generation_identity: None,
                },
            )
            .unwrap_err(),
            LegalBoardAssetError::PayloadDigest
        );
    }
}
