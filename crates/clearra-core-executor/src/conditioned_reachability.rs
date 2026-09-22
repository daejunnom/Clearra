//! Immutable sparse exact reachability pack for BuildUp Boolean queries.
//!
//! Every present record contains the complete spawn-to-lock relation for one
//! `(profile, dimensions, physical board, piece)` query. A missing record is
//! never a negative answer. Witness, spin, finesse, and multiplicity remain
//! owned by the existing exact traversal.

use clearra_accelerator_activation::{AcceleratorProduct, VerifiedAcceleratorAuthority};
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_rules::kicks::KickTableProfileId;
use sha2::{Digest, Sha256};
use std::sync::{Arc, OnceLock, RwLock};

use crate::legal_board::{accelerator_profile_name, built_in_rule_identity, ProviderStatus};

const MAGIC: &[u8; 8] = b"CLBR0001";
const VERSION: u32 = 1;
const HEADER_BYTES: usize = 256;
const RECORD_BYTES: usize = 48;
const BOOLEAN_EVIDENCE: u8 = 1;
const MAX_PACK_BYTES: usize = 16 * 1024 * 1024;
const PROFILE_SLOTS: usize = 5;
pub const CONDITIONED_REACHABILITY_COMPLETENESS_SCOPE: &str =
    "width-10-height-1-6-spawn-to-lock-boolean-complete-records";

/// The bounded exact-cache candidate has one deliberately narrow evidence
/// contract. Other evidence modes stay on the existing exact traversal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditionedEvidenceLevel {
    Boolean,
    Witness,
    Spin,
    Finesse,
    CountAll,
}

/// Entry poses are the exact collision-free sky seeds compiled by the same
/// profile-bound reachability template as the fallback search. This is not an
/// arbitrary local window and may not be substituted for another entry set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditionedEntryPoseSet {
    ProfileSkySeeds,
    Explicit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditionedTargetScope {
    AllGroundedLocks,
    SelectedLocks,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConditionedReachabilityQuery {
    pub width: u8,
    pub height: u8,
    pub board: u64,
    pub piece: PieceKind,
    pub kick_profile: KickTableProfileId,
    pub entry_poses: ConditionedEntryPoseSet,
    pub target_scope: ConditionedTargetScope,
    pub evidence: ConditionedEvidenceLevel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConditionedReachabilityRecord {
    pub width: u8,
    pub height: u8,
    pub board: u64,
    pub piece: PieceKind,
    pub reachable_lock_anchors: [u64; 4],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConditionedReachabilityBinding {
    pub kick_profile: KickTableProfileId,
    pub rule_identity: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConditionedReachabilityExpectation {
    pub binding: ConditionedReachabilityBinding,
    pub generation_identity: Option<[u8; 32]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditionedReachabilityAssetError {
    TooLarge,
    Header,
    UnsupportedVersion,
    UnsupportedProfile,
    BindingMismatch,
    SnapshotMismatch,
    PayloadDigest,
    GenerationIdentity,
    Record,
    NonCanonicalOrder,
    NotQualified,
    ActiveSessionTooLarge,
    ActiveSessionInUse,
    RegistryUnavailable,
}

impl ConditionedReachabilityAssetError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::TooLarge => "conditioned_reachability_asset_too_large",
            Self::Header => "conditioned_reachability_asset_header_invalid",
            Self::UnsupportedVersion => "conditioned_reachability_asset_version_unsupported",
            Self::UnsupportedProfile => "conditioned_reachability_asset_profile_unsupported",
            Self::BindingMismatch => "conditioned_reachability_asset_binding_mismatch",
            Self::SnapshotMismatch => "conditioned_reachability_asset_snapshot_mismatch",
            Self::PayloadDigest => "conditioned_reachability_asset_payload_digest_mismatch",
            Self::GenerationIdentity => {
                "conditioned_reachability_asset_generation_identity_mismatch"
            }
            Self::Record => "conditioned_reachability_asset_record_invalid",
            Self::NonCanonicalOrder => "conditioned_reachability_asset_order_noncanonical",
            Self::NotQualified => "conditioned_reachability_asset_not_qualified",
            Self::ActiveSessionTooLarge => "conditioned_reachability_active_session_too_large",
            Self::ActiveSessionInUse => "conditioned_reachability_active_session_in_use",
            Self::RegistryUnavailable => "conditioned_reachability_registry_unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditionedReachabilityLookup {
    Complete([u64; 4]),
    PassThrough(ProviderStatus),
}

#[derive(Clone, Debug)]
pub struct BoardConditionedReachability {
    bytes: Arc<[u8]>,
    binding: ConditionedReachabilityBinding,
    generation_identity: [u8; 32],
    record_count: usize,
}

/// Product-eligible wrapper. Merely loading a structurally valid sparse cache
/// never grants it authority to skip exact traversal.
#[derive(Clone, Debug)]
pub struct QualifiedBoardConditionedReachability {
    pack: BoardConditionedReachability,
    signed_catalog_identity: [u8; 32],
    bounded_exhaustive_identity: [u8; 32],
}

impl QualifiedBoardConditionedReachability {
    pub fn qualify(
        pack: BoardConditionedReachability,
        authority: &VerifiedAcceleratorAuthority,
    ) -> Result<Self, ConditionedReachabilityAssetError> {
        let payload_identity: [u8; 32] = Sha256::digest(&*pack.bytes).into();
        if authority.product() != AcceleratorProduct::BoardConditionedReachability
            || authority.profile()
                != accelerator_profile_name(pack.binding.kick_profile)
                    .map_err(|_| ConditionedReachabilityAssetError::UnsupportedProfile)?
            || authority.generation_identity() != pack.generation_identity
            || authority.rule_identity() != pack.binding.rule_identity
            || authority.payload_bytes() != pack.bytes.len() as u64
            || authority.payload_identity() != payload_identity
            || authority.completeness_scope() != CONDITIONED_REACHABILITY_COMPLETENESS_SCOPE
            || authority.statement_identity() == [0; 32]
            || authority.qualification_identity() == [0; 32]
        {
            return Err(ConditionedReachabilityAssetError::NotQualified);
        }
        Ok(Self {
            pack,
            signed_catalog_identity: authority.statement_identity(),
            bounded_exhaustive_identity: authority.qualification_identity(),
        })
    }

    pub const fn binding(&self) -> ConditionedReachabilityBinding {
        self.pack.binding()
    }

    pub const fn generation_identity(&self) -> [u8; 32] {
        self.pack.generation_identity()
    }

    pub const fn signed_catalog_identity(&self) -> [u8; 32] {
        self.signed_catalog_identity
    }

    pub const fn bounded_exhaustive_identity(&self) -> [u8; 32] {
        self.bounded_exhaustive_identity
    }

    pub fn shared_bytes(&self) -> usize {
        self.pack.compressed_bytes()
    }

    pub fn lookup(
        &self,
        width: u8,
        height: u8,
        board: u64,
        piece: PieceKind,
        kick_profile: KickTableProfileId,
    ) -> ConditionedReachabilityLookup {
        self.pack.lookup_query(ConditionedReachabilityQuery {
            width,
            height,
            board,
            piece,
            kick_profile,
            entry_poses: ConditionedEntryPoseSet::ProfileSkySeeds,
            target_scope: ConditionedTargetScope::AllGroundedLocks,
            evidence: ConditionedEvidenceLevel::Boolean,
        })
    }
}

impl BoardConditionedReachability {
    pub fn load(
        bytes: Arc<[u8]>,
        expectation: ConditionedReachabilityExpectation,
    ) -> Result<Self, ConditionedReachabilityAssetError> {
        if bytes.len() > MAX_PACK_BYTES {
            return Err(ConditionedReachabilityAssetError::TooLarge);
        }
        if bytes.len() < HEADER_BYTES || bytes.get(..8) != Some(MAGIC.as_slice()) {
            return Err(ConditionedReachabilityAssetError::Header);
        }
        if read_u32(&bytes[8..12])? != VERSION {
            return Err(ConditionedReachabilityAssetError::UnsupportedVersion);
        }
        let profile = decode_profile(bytes[12])?;
        if bytes[13] != BOOLEAN_EVIDENCE
            || bytes[14] != 6
            || bytes[15] != 10
            || read_u32(&bytes[16..20])? as usize != RECORD_BYTES
            || bytes[132..HEADER_BYTES].iter().any(|value| *value != 0)
        {
            return Err(ConditionedReachabilityAssetError::Header);
        }
        let binding = ConditionedReachabilityBinding {
            kick_profile: profile,
            rule_identity: array32(&bytes[20..52])?,
        };
        if binding != expectation.binding {
            return Err(ConditionedReachabilityAssetError::BindingMismatch);
        }
        let payload_digest = array32(&bytes[52..84])?;
        let generation_identity = array32(&bytes[84..116])?;
        if expectation
            .generation_identity
            .is_some_and(|expected| expected != generation_identity)
        {
            return Err(ConditionedReachabilityAssetError::SnapshotMismatch);
        }
        let record_count = usize::try_from(read_u64(&bytes[116..124])?)
            .map_err(|_| ConditionedReachabilityAssetError::Header)?;
        let declared_length = usize::try_from(read_u64(&bytes[124..132])?)
            .map_err(|_| ConditionedReachabilityAssetError::Header)?;
        if declared_length != bytes.len()
            || HEADER_BYTES.checked_add(
                record_count
                    .checked_mul(RECORD_BYTES)
                    .ok_or(ConditionedReachabilityAssetError::Header)?,
            ) != Some(bytes.len())
        {
            return Err(ConditionedReachabilityAssetError::Header);
        }
        if Sha256::digest(&bytes[HEADER_BYTES..]).as_slice() != payload_digest {
            return Err(ConditionedReachabilityAssetError::PayloadDigest);
        }
        if generation_identity_for(binding, payload_digest, record_count) != generation_identity {
            return Err(ConditionedReachabilityAssetError::GenerationIdentity);
        }
        let mut prior = None;
        for index in 0..record_count {
            let record = read_record(&bytes, index)?;
            validate_record(record)?;
            let key = record_key(record);
            if prior.is_some_and(|value| value >= key) {
                return Err(ConditionedReachabilityAssetError::NonCanonicalOrder);
            }
            prior = Some(key);
        }
        Ok(Self {
            bytes,
            binding,
            generation_identity,
            record_count,
        })
    }

    pub const fn binding(&self) -> ConditionedReachabilityBinding {
        self.binding
    }

    pub const fn generation_identity(&self) -> [u8; 32] {
        self.generation_identity
    }

    pub fn compressed_bytes(&self) -> usize {
        self.bytes.len()
    }

    pub const fn record_count(&self) -> usize {
        self.record_count
    }

    pub fn lookup(
        &self,
        width: u8,
        height: u8,
        board: u64,
        piece: PieceKind,
        kick_profile: KickTableProfileId,
    ) -> ConditionedReachabilityLookup {
        self.lookup_query(ConditionedReachabilityQuery {
            width,
            height,
            board,
            piece,
            kick_profile,
            entry_poses: ConditionedEntryPoseSet::ProfileSkySeeds,
            target_scope: ConditionedTargetScope::AllGroundedLocks,
            evidence: ConditionedEvidenceLevel::Boolean,
        })
    }

    pub fn lookup_query(
        &self,
        query: ConditionedReachabilityQuery,
    ) -> ConditionedReachabilityLookup {
        if query.kick_profile != self.binding.kick_profile {
            return ConditionedReachabilityLookup::PassThrough(ProviderStatus::SnapshotMismatch);
        }
        if query.entry_poses != ConditionedEntryPoseSet::ProfileSkySeeds
            || query.target_scope != ConditionedTargetScope::AllGroundedLocks
            || query.evidence != ConditionedEvidenceLevel::Boolean
            || query.width != 10
            || !(1..=6).contains(&query.height)
            || query.board >> (u32::from(query.width) * u32::from(query.height)) != 0
        {
            return ConditionedReachabilityLookup::PassThrough(ProviderStatus::OutOfScope);
        }
        let wanted = (
            query.width,
            query.height,
            query.board,
            piece_code(query.piece),
        );
        let mut low = 0;
        let mut high = self.record_count;
        while low < high {
            let middle = low + (high - low) / 2;
            let record = match read_record(&self.bytes, middle) {
                Ok(value) => value,
                Err(_) => {
                    return ConditionedReachabilityLookup::PassThrough(ProviderStatus::InvalidAsset)
                }
            };
            match record_key(record).cmp(&wanted) {
                core::cmp::Ordering::Less => low = middle + 1,
                core::cmp::Ordering::Greater => high = middle,
                core::cmp::Ordering::Equal => {
                    return ConditionedReachabilityLookup::Complete(record.reachable_lock_anchors)
                }
            }
        }
        ConditionedReachabilityLookup::PassThrough(ProviderStatus::Miss)
    }
}

pub fn built_in_conditioned_reachability_binding(
    kick_profile: KickTableProfileId,
) -> Result<ConditionedReachabilityBinding, ConditionedReachabilityAssetError> {
    let base_rule_identity = built_in_rule_identity(kick_profile)
        .map_err(|_| ConditionedReachabilityAssetError::UnsupportedProfile)?;
    let mut digest = Sha256::new();
    digest.update(b"clearra.conditioned-reachability.relation-schema.v2\0");
    digest.update(base_rule_identity);
    digest.update(
        b"entry=profile-sky-seeds\0target=all-grounded-locks\0evidence=boolean\0\
dependency=complete-board-plus-closed-left-right-bottom-open-top\0\
transition=translation-plus-first-success-ordered-kick\0\
goal=independent-spawn-to-lock\0coordinate=board64-bottom-left\0",
    );
    Ok(ConditionedReachabilityBinding {
        kick_profile,
        rule_identity: digest.finalize().into(),
    })
}

/// Deterministic producer primitive for one exact sparse-cache record.  This
/// performs the same exhaustive spawn-to-lock traversal used by the fallback;
/// qualification must still compare the resulting pack with an independent
/// primitive reference before signing it.
pub fn derive_exact_conditioned_reachability_record(
    width: u8,
    height: u8,
    board: u64,
    piece: PieceKind,
    kick_profile: KickTableProfileId,
) -> Result<ConditionedReachabilityRecord, ConditionedReachabilityAssetError> {
    let reachable_lock_anchors =
        crate::backend::exact_spawn_lock_anchors(width, height, board, piece, kick_profile)
            .ok_or(ConditionedReachabilityAssetError::Record)?;
    Ok(ConditionedReachabilityRecord {
        width,
        height,
        board,
        piece,
        reachable_lock_anchors,
    })
}

pub fn encode_conditioned_reachability(
    binding: ConditionedReachabilityBinding,
    records: &[ConditionedReachabilityRecord],
) -> Result<Vec<u8>, ConditionedReachabilityAssetError> {
    let mut canonical = records.to_vec();
    canonical.sort_unstable_by_key(|record| record_key(*record));
    if canonical
        .windows(2)
        .any(|pair| record_key(pair[0]) == record_key(pair[1]))
    {
        return Err(ConditionedReachabilityAssetError::NonCanonicalOrder);
    }
    for record in &canonical {
        validate_record(*record)?;
    }
    let total_length = HEADER_BYTES
        .checked_add(
            canonical
                .len()
                .checked_mul(RECORD_BYTES)
                .ok_or(ConditionedReachabilityAssetError::TooLarge)?,
        )
        .ok_or(ConditionedReachabilityAssetError::TooLarge)?;
    if total_length > MAX_PACK_BYTES {
        return Err(ConditionedReachabilityAssetError::TooLarge);
    }
    let mut output = vec![0_u8; HEADER_BYTES];
    output[..8].copy_from_slice(MAGIC);
    output[8..12].copy_from_slice(&VERSION.to_le_bytes());
    output[12] = encode_profile(binding.kick_profile)?;
    output[13] = BOOLEAN_EVIDENCE;
    output[14] = 6;
    output[15] = 10;
    output[16..20].copy_from_slice(&(RECORD_BYTES as u32).to_le_bytes());
    output[20..52].copy_from_slice(&binding.rule_identity);
    output[116..124].copy_from_slice(&(canonical.len() as u64).to_le_bytes());
    output[124..132].copy_from_slice(&(total_length as u64).to_le_bytes());
    for record in canonical {
        write_record(&mut output, record)?;
    }
    let payload_digest: [u8; 32] = Sha256::digest(&output[HEADER_BYTES..]).into();
    output[52..84].copy_from_slice(&payload_digest);
    let generation = generation_identity_for(binding, payload_digest, records.len());
    output[84..116].copy_from_slice(&generation);
    Ok(output)
}

#[derive(Default)]
struct Registry {
    slots: [Option<Arc<QualifiedBoardConditionedReachability>>; PROFILE_SLOTS],
}

static REGISTRY: OnceLock<RwLock<Registry>> = OnceLock::new();

pub fn install_conditioned_reachability_pack(
    pack: QualifiedBoardConditionedReachability,
) -> Result<Option<Arc<QualifiedBoardConditionedReachability>>, ConditionedReachabilityAssetError> {
    let _mutation = crate::legal_board::accelerator_registry_mutation_lock()
        .lock()
        .map_err(|_| ConditionedReachabilityAssetError::RegistryUnavailable)?;
    let slot = profile_slot(pack.binding().kick_profile)?;
    let combined = pack.shared_bytes().saturating_add(
        crate::legal_board::qualified_legal_board_snapshot(pack.binding().kick_profile)
            .as_ref()
            .map_or(0, |board| board.shared_bytes()),
    );
    if combined > crate::legal_board::MAX_ACTIVE_ACCELERATOR_BYTES {
        return Err(ConditionedReachabilityAssetError::ActiveSessionTooLarge);
    }
    let registry = REGISTRY.get_or_init(|| RwLock::new(Registry::default()));
    let mut guard = registry
        .write()
        .map_err(|_| ConditionedReachabilityAssetError::RegistryUnavailable)?;
    if let Some(active) = guard.slots[slot].as_ref() {
        if active.generation_identity() == pack.generation_identity()
            && active.signed_catalog_identity() == pack.signed_catalog_identity()
        {
            return Ok(Some(Arc::clone(active)));
        }
        if Arc::strong_count(active) > 1 {
            return Err(ConditionedReachabilityAssetError::ActiveSessionInUse);
        }
    }
    let prior = guard.slots[slot].replace(Arc::new(pack));
    Ok(prior)
}

pub fn remove_conditioned_reachability_pack(
    profile: KickTableProfileId,
) -> Result<Option<Arc<QualifiedBoardConditionedReachability>>, ConditionedReachabilityAssetError> {
    let _mutation = crate::legal_board::accelerator_registry_mutation_lock()
        .lock()
        .map_err(|_| ConditionedReachabilityAssetError::RegistryUnavailable)?;
    let slot = profile_slot(profile)?;
    let registry = REGISTRY.get_or_init(|| RwLock::new(Registry::default()));
    let mut guard = registry
        .write()
        .map_err(|_| ConditionedReachabilityAssetError::RegistryUnavailable)?;
    if guard.slots[slot]
        .as_ref()
        .is_some_and(|active| Arc::strong_count(active) > 1)
    {
        return Err(ConditionedReachabilityAssetError::ActiveSessionInUse);
    }
    let prior = guard.slots[slot].take();
    Ok(prior)
}

pub(crate) fn conditioned_reachability_snapshot(
    profile: KickTableProfileId,
) -> Option<Arc<QualifiedBoardConditionedReachability>> {
    let slot = profile_slot(profile).ok()?;
    REGISTRY
        .get_or_init(|| RwLock::new(Registry::default()))
        .read()
        .ok()?
        .slots[slot]
        .clone()
}

/// Host-side fast path for an already pinned immutable generation. Workers
/// continue to clone only the shared owner rather than reload the pack.
pub fn active_conditioned_reachability_identity(
    profile: KickTableProfileId,
) -> Option<([u8; 32], [u8; 32])> {
    let pack = conditioned_reachability_snapshot(profile)?;
    Some((pack.generation_identity(), pack.signed_catalog_identity()))
}

fn validate_record(
    record: ConditionedReachabilityRecord,
) -> Result<(), ConditionedReachabilityAssetError> {
    if record.width != 10
        || !(1..=6).contains(&record.height)
        || record.board >> (u32::from(record.width) * u32::from(record.height)) != 0
    {
        return Err(ConditionedReachabilityAssetError::Record);
    }
    let pose_bits = u32::from(record.width) * u32::from(record.height);
    if record
        .reachable_lock_anchors
        .iter()
        .any(|anchors| *anchors >> pose_bits != 0)
    {
        return Err(ConditionedReachabilityAssetError::Record);
    }
    Ok(())
}

fn record_key(record: ConditionedReachabilityRecord) -> (u8, u8, u64, u8) {
    (
        record.width,
        record.height,
        record.board,
        piece_code(record.piece),
    )
}

fn write_record(
    output: &mut Vec<u8>,
    record: ConditionedReachabilityRecord,
) -> Result<(), ConditionedReachabilityAssetError> {
    let begin = output.len();
    output.extend_from_slice(&record.board.to_le_bytes());
    output.push(record.width);
    output.push(record.height);
    output.push(piece_code(record.piece));
    output.extend_from_slice(&[0; 5]);
    for anchors in record.reachable_lock_anchors {
        output.extend_from_slice(&anchors.to_le_bytes());
    }
    if output.len() - begin != RECORD_BYTES {
        return Err(ConditionedReachabilityAssetError::Record);
    }
    Ok(())
}

fn read_record(
    bytes: &[u8],
    index: usize,
) -> Result<ConditionedReachabilityRecord, ConditionedReachabilityAssetError> {
    let begin = HEADER_BYTES
        .checked_add(
            index
                .checked_mul(RECORD_BYTES)
                .ok_or(ConditionedReachabilityAssetError::Record)?,
        )
        .ok_or(ConditionedReachabilityAssetError::Record)?;
    let record = bytes
        .get(begin..begin + RECORD_BYTES)
        .ok_or(ConditionedReachabilityAssetError::Record)?;
    if record[11..16].iter().any(|value| *value != 0) {
        return Err(ConditionedReachabilityAssetError::Record);
    }
    let piece = decode_piece(record[10])?;
    let mut anchors = [0_u64; 4];
    for (rotation, destination) in anchors.iter_mut().enumerate() {
        let offset = 16 + rotation * 8;
        *destination = read_u64(&record[offset..offset + 8])?;
    }
    Ok(ConditionedReachabilityRecord {
        width: record[8],
        height: record[9],
        board: read_u64(&record[..8])?,
        piece,
        reachable_lock_anchors: anchors,
    })
}

fn generation_identity_for(
    binding: ConditionedReachabilityBinding,
    payload_digest: [u8; 32],
    record_count: usize,
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"clearra.conditioned-reachability.boolean-full-board.v1\0");
    digest.update([encode_profile(binding.kick_profile).unwrap_or(u8::MAX)]);
    digest.update(binding.rule_identity);
    digest.update(payload_digest);
    digest.update((record_count as u64).to_le_bytes());
    digest.finalize().into()
}

fn profile_slot(profile: KickTableProfileId) -> Result<usize, ConditionedReachabilityAssetError> {
    Ok(match profile {
        KickTableProfileId::Srs90 => 0,
        KickTableProfileId::SrsPlus => 1,
        KickTableProfileId::SrsX => 2,
        KickTableProfileId::Jstris180 => 3,
        KickTableProfileId::NoKick => 4,
        _ => return Err(ConditionedReachabilityAssetError::UnsupportedProfile),
    })
}

fn encode_profile(profile: KickTableProfileId) -> Result<u8, ConditionedReachabilityAssetError> {
    u8::try_from(profile_slot(profile)?)
        .map_err(|_| ConditionedReachabilityAssetError::UnsupportedProfile)
}

fn decode_profile(value: u8) -> Result<KickTableProfileId, ConditionedReachabilityAssetError> {
    match value {
        0 => Ok(KickTableProfileId::Srs90),
        1 => Ok(KickTableProfileId::SrsPlus),
        2 => Ok(KickTableProfileId::SrsX),
        3 => Ok(KickTableProfileId::Jstris180),
        4 => Ok(KickTableProfileId::NoKick),
        _ => Err(ConditionedReachabilityAssetError::UnsupportedProfile),
    }
}

fn piece_code(piece: PieceKind) -> u8 {
    match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    }
}

fn decode_piece(value: u8) -> Result<PieceKind, ConditionedReachabilityAssetError> {
    match value {
        0 => Ok(PieceKind::I),
        1 => Ok(PieceKind::O),
        2 => Ok(PieceKind::T),
        3 => Ok(PieceKind::S),
        4 => Ok(PieceKind::Z),
        5 => Ok(PieceKind::J),
        6 => Ok(PieceKind::L),
        _ => Err(ConditionedReachabilityAssetError::Record),
    }
}

fn read_u32(bytes: &[u8]) -> Result<u32, ConditionedReachabilityAssetError> {
    Ok(u32::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| ConditionedReachabilityAssetError::Header)?,
    ))
}

fn read_u64(bytes: &[u8]) -> Result<u64, ConditionedReachabilityAssetError> {
    Ok(u64::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| ConditionedReachabilityAssetError::Header)?,
    ))
}

fn array32(bytes: &[u8]) -> Result<[u8; 32], ConditionedReachabilityAssetError> {
    bytes
        .try_into()
        .map_err(|_| ConditionedReachabilityAssetError::Header)
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

    use super::*;

    fn fixture() -> (
        ConditionedReachabilityBinding,
        Vec<ConditionedReachabilityRecord>,
    ) {
        let binding =
            built_in_conditioned_reachability_binding(KickTableProfileId::SrsPlus).unwrap();
        let mut anchors = [0_u64; 4];
        anchors[usize::from(RotationState::Zero.quarter_turns())] = 1 << 3;
        (
            binding,
            vec![ConditionedReachabilityRecord {
                width: 10,
                height: 4,
                board: 0b111,
                piece: PieceKind::I,
                reachable_lock_anchors: anchors,
            }],
        )
    }

    #[test]
    fn exact_present_record_and_sparse_miss_are_distinct() {
        let (binding, records) = fixture();
        let bytes = encode_conditioned_reachability(binding, &records).unwrap();
        let generation = bytes[84..116].try_into().unwrap();
        let pack = BoardConditionedReachability::load(
            Arc::from(bytes),
            ConditionedReachabilityExpectation {
                binding,
                generation_identity: Some(generation),
            },
        )
        .unwrap();
        assert_eq!(
            pack.lookup(10, 4, 0b111, PieceKind::I, KickTableProfileId::SrsPlus),
            ConditionedReachabilityLookup::Complete(records[0].reachable_lock_anchors)
        );
        assert_eq!(
            pack.lookup(10, 4, 0b11, PieceKind::I, KickTableProfileId::SrsPlus),
            ConditionedReachabilityLookup::PassThrough(ProviderStatus::Miss)
        );
        assert_eq!(
            pack.lookup_query(ConditionedReachabilityQuery {
                width: 10,
                height: 4,
                board: 0b111,
                piece: PieceKind::I,
                kick_profile: KickTableProfileId::SrsPlus,
                entry_poses: ConditionedEntryPoseSet::ProfileSkySeeds,
                target_scope: ConditionedTargetScope::AllGroundedLocks,
                evidence: ConditionedEvidenceLevel::Witness,
            }),
            ConditionedReachabilityLookup::PassThrough(ProviderStatus::OutOfScope)
        );
    }

    #[test]
    fn relation_binding_fingerprints_more_than_the_base_kick_table() {
        let srs_plus =
            built_in_conditioned_reachability_binding(KickTableProfileId::SrsPlus).unwrap();
        let srs_x = built_in_conditioned_reachability_binding(KickTableProfileId::SrsX).unwrap();
        assert_ne!(srs_plus.rule_identity, srs_x.rule_identity);
        assert_ne!(
            srs_plus.rule_identity,
            built_in_rule_identity(KickTableProfileId::SrsPlus).unwrap()
        );
    }

    #[test]
    fn profile_scope_and_corruption_fail_closed_at_load_or_open_at_lookup() {
        let (binding, records) = fixture();
        let mut bytes = encode_conditioned_reachability(binding, &records).unwrap();
        let pack = BoardConditionedReachability::load(
            Arc::from(bytes.clone()),
            ConditionedReachabilityExpectation {
                binding,
                generation_identity: None,
            },
        )
        .unwrap();
        assert_eq!(
            pack.lookup(10, 4, 0b111, PieceKind::I, KickTableProfileId::SrsX),
            ConditionedReachabilityLookup::PassThrough(ProviderStatus::SnapshotMismatch)
        );
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert_eq!(
            BoardConditionedReachability::load(
                Arc::from(bytes),
                ConditionedReachabilityExpectation {
                    binding,
                    generation_identity: None,
                },
            )
            .unwrap_err(),
            ConditionedReachabilityAssetError::PayloadDigest
        );
    }

    #[test]
    fn structural_load_does_not_grant_product_qualification() {
        let (binding, records) = fixture();
        let bytes = encode_conditioned_reachability(binding, &records).unwrap();
        let generation = bytes[84..116].try_into().unwrap();
        let pack = BoardConditionedReachability::load(
            Arc::from(bytes),
            ConditionedReachabilityExpectation {
                binding,
                generation_identity: Some(generation),
            },
        )
        .unwrap();
        // Structural parsing has no constructor for the product-eligible
        // wrapper. That constructor now requires an opaque authority emitted
        // only by signature verification in clearra-accelerator-activation.
        assert_eq!(pack.generation_identity(), generation);
        assert_eq!(pack.record_count(), 1);
    }
}
