//! Exact, profile-bound legal-board asset contracts.
//!
//! This crate owns only the immutable `F_k ∩ R_k` bundle representation,
//! its row-normalization codec, and fail-open lookup outcomes. It does not
//! generate movement domains, download assets, decide product qualification,
//! or implement BuildUp reachability.

use clearra_accelerator_activation::{AcceleratorProduct, VerifiedAcceleratorAuthority};
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_rules::kicks::{
    KickTableProfile, KickTableProfileId, KickTransition, NoKick, SrsKicks,
};
use sha2::{Digest, Sha256};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, OnceLock, RwLock,
};

const MAGIC: &[u8; 8] = b"CLLB0001";
const VERSION: u32 = 1;
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
const CHECKPOINT_STRIDE: u64 = 256;
const EXACT_INTERSECTION_KIND: u8 = 1;
const PROFILE_SLOTS: usize = 5;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProviderStatus {
    Ready,
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

/// Typed correspondence between BuildUp's original logical rows and the
/// legal-board product's bottom-prefix normalization.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OriginalRowFrame {
    deleted_original_rows: u8,
}

impl OriginalRowFrame {
    pub fn from_deleted_rows(value: u16) -> Result<Self, RowCodecError> {
        if value >> 4 != 0 {
            return Err(RowCodecError::DeletedRowsOutsideFourRowFrame);
        }
        Ok(Self {
            deleted_original_rows: value as u8,
        })
    }

    pub const fn deleted_original_rows(self) -> u8 {
        self.deleted_original_rows
    }

    pub const fn cleared_row_count(self) -> u8 {
        self.deleted_original_rows.count_ones() as u8
    }

    pub const fn surviving_original_rows(self) -> u8 {
        4 - self.cleared_row_count()
    }

    /// Convert a compact physical board to the single canonical graph form:
    /// cleared rows are represented only as a full bottom-row prefix. Their
    /// original positions remain in this frame for replay/witness ownership.
    pub fn normalize_product_board(self, physical_board: u64) -> Result<u64, RowCodecError> {
        let surviving_bits = u32::from(self.surviving_original_rows()) * 10;
        if physical_board >> surviving_bits != 0 {
            return Err(RowCodecError::PhysicalCellsOutsideSurvivingRows);
        }
        let prefix_bits = u32::from(self.cleared_row_count()) * 10;
        let prefix = if prefix_bits == 0 {
            0
        } else {
            (1_u64 << prefix_bits) - 1
        };
        Ok((physical_board << prefix_bits) | prefix)
    }

    /// Recover the compact physical board from the product membership key.
    /// The original-row map is retained by this typed frame rather than being
    /// encoded into the product board itself.
    pub fn compact_physical_board(self, product_board: u64) -> Result<u64, RowCodecError> {
        if product_board & !FIELD_MASK != 0 {
            return Err(RowCodecError::PhysicalCellsOutsideSurvivingRows);
        }
        let prefix_bits = u32::from(self.cleared_row_count()) * 10;
        let prefix = if prefix_bits == 0 {
            0
        } else {
            (1_u64 << prefix_bits) - 1
        };
        if product_board & prefix != prefix {
            return Err(RowCodecError::MissingClearedBottomPrefix);
        }
        let physical_board = product_board >> prefix_bits;
        let surviving_bits = u32::from(self.surviving_original_rows()) * 10;
        if physical_board >> surviving_bits != 0 {
            return Err(RowCodecError::PhysicalCellsOutsideSurvivingRows);
        }
        Ok(physical_board)
    }

    /// Reinsert full cleared rows at their original logical positions for
    /// replay coordinates. This representation must never be used as a legal
    /// board membership key.
    pub fn replay_frame_board(self, physical_board: u64) -> Result<u64, RowCodecError> {
        let surviving_bits = u32::from(self.surviving_original_rows()) * 10;
        if physical_board >> surviving_bits != 0 {
            return Err(RowCodecError::PhysicalCellsOutsideSurvivingRows);
        }
        let mut replay = 0_u64;
        let mut physical_row = 0_u32;
        for original_row in 0_u32..4 {
            let row = if self.deleted_original_rows & (1 << original_row) != 0 {
                (1_u64 << 10) - 1
            } else {
                let row = (physical_board >> (physical_row * 10)) & ((1_u64 << 10) - 1);
                physical_row += 1;
                row
            };
            replay |= row << (original_row * 10);
        }
        Ok(replay)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegalBoardBinding {
    pub kick_profile: KickTableProfileId,
    pub rule_identity: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnsupportedLegalBoardProfile;

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
            || authority.profile() != board.binding.kick_profile.as_str()
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

    pub fn shared_bytes(&self) -> usize {
        self.board.compressed_bytes() + self.board.sparse_index_bytes()
    }
}

#[derive(Default)]
struct LegalBoardRegistry {
    slots: [Option<Arc<QualifiedExactLegalBoard>>; PROFILE_SLOTS],
}

static LEGAL_BOARD_REGISTRY: OnceLock<RwLock<LegalBoardRegistry>> = OnceLock::new();
static LEGAL_BOARD_REGISTRY_EPOCH: AtomicU64 = AtomicU64::new(1);

pub fn install_qualified_exact_legal_board(
    board: QualifiedExactLegalBoard,
) -> Result<Option<Arc<QualifiedExactLegalBoard>>, LegalBoardAssetError> {
    let slot = profile_slot(board.binding().kick_profile)?;
    let combined = board.shared_bytes().saturating_add(
        crate::conditioned_reachability::conditioned_reachability_snapshot(
            board.binding().kick_profile,
        )
        .as_ref()
        .map_or(0, |pack| pack.shared_bytes()),
    );
    if combined > MAX_ACTIVE_ACCELERATOR_BYTES {
        return Err(LegalBoardAssetError::ActiveSessionTooLarge);
    }
    let registry = LEGAL_BOARD_REGISTRY.get_or_init(|| RwLock::new(LegalBoardRegistry::default()));
    let mut guard = registry
        .write()
        .map_err(|_| LegalBoardAssetError::RegistryUnavailable)?;
    let prior = guard.slots[slot].replace(Arc::new(board));
    LEGAL_BOARD_REGISTRY_EPOCH.fetch_add(1, Ordering::Release);
    Ok(prior)
}

pub fn remove_qualified_exact_legal_board(
    profile: KickTableProfileId,
) -> Result<Option<Arc<QualifiedExactLegalBoard>>, LegalBoardAssetError> {
    let slot = profile_slot(profile)?;
    let registry = LEGAL_BOARD_REGISTRY.get_or_init(|| RwLock::new(LegalBoardRegistry::default()));
    let mut guard = registry
        .write()
        .map_err(|_| LegalBoardAssetError::RegistryUnavailable)?;
    let prior = guard.slots[slot].take();
    LEGAL_BOARD_REGISTRY_EPOCH.fetch_add(1, Ordering::Release);
    Ok(prior)
}

pub(crate) fn qualified_legal_board_epoch() -> u64 {
    LEGAL_BOARD_REGISTRY_EPOCH.load(Ordering::Acquire)
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

impl ExactLegalBoard {
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
            .map(|layer| layer.checkpoints.capacity() * core::mem::size_of::<Checkpoint>())
            .sum()
    }

    pub fn layer_count(&self, layer: usize) -> Option<u64> {
        self.layers.get(layer).map(|value| value.directory.count)
    }

    pub fn layer_payload_digest(&self, layer: usize) -> Option<[u8; 32]> {
        self.layers.get(layer).map(|value| value.directory.digest)
    }

    pub fn decide(&self, query: LegalBoardQuery) -> LegalBoardDecision {
        if query.width != 10
            || query.height != 4
            || query.initial_board != 0
            || query.placed_piece_count >= LAYER_COUNT
            || query.completion != CompletionCapability::ClearToEmpty
        {
            return LegalBoardDecision::PassThrough(ProviderStatus::OutOfScope);
        }
        if query.kick_profile != self.binding.kick_profile {
            return LegalBoardDecision::PassThrough(ProviderStatus::SnapshotMismatch);
        }
        let frame = match OriginalRowFrame::from_deleted_rows(query.deleted_original_rows) {
            Ok(value) => value,
            Err(_) => return LegalBoardDecision::PassThrough(ProviderStatus::OutOfScope),
        };
        let normalized = match frame.normalize_product_board(query.physical_board) {
            Ok(value) => value,
            Err(_) => return LegalBoardDecision::PassThrough(ProviderStatus::OutOfScope),
        };
        if normalized.count_ones() != (query.placed_piece_count as u32) * 4 {
            return LegalBoardDecision::PassThrough(ProviderStatus::OutOfScope);
        }
        match layer_contains(
            &self.bytes,
            &self.layers[query.placed_piece_count],
            normalized,
        ) {
            Ok(true) => LegalBoardDecision::CandidateAllowed,
            Ok(false) => LegalBoardDecision::VerifiedAbsent,
            Err(_) => LegalBoardDecision::PassThrough(ProviderStatus::InvalidAsset),
        }
    }
}

/// Encode strictly sorted `L_k = F_k ∩ R_k` layers. Qualification and
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
    })
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
    digest.update(b"clearra.legal-board.exact-intersection.generation.v1\0");
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

    fn binding() -> LegalBoardBinding {
        LegalBoardBinding {
            kick_profile: KickTableProfileId::Jstris180,
            rule_identity: [7; 32],
        }
    }

    fn layers() -> [Vec<u64>; LAYER_COUNT] {
        let mut layers: [Vec<u64>; LAYER_COUNT] = std::array::from_fn(|_| Vec::new());
        layers[0].push(0);
        layers[1].extend([0b1111, 0b1111_0000]);
        layers[3].push(ROW_MASK | (0b11 << 30));
        layers[10].push(FIELD_MASK);
        layers
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
