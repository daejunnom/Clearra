//! Immutable *candidate* binary for bounded local entry-to-exit relations.
//!
//! Parsing proves structure, binding and integrity, not semantic qualification.
//! This format is deliberately distinct from the older sparse spawn-to-lock
//! cache. The separate signed product wrapper checks authority before a
//! parsed candidate can enter the process registry.

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_rules::kicks::KickTableProfileId;
use sha2::{Digest, Sha256};

use crate::conditioned_local_index::{
    compare_record_key, piece_code, LocalRelationCandidateIndex, LocalRelationCandidateLookup,
    LocalRelationIndexError,
};
use crate::conditioned_local_relation::{
    ConditionedPoseWindow, ExactConditionedLocalRelation, LocalRelationRowFrame,
};
use crate::conditioned_reachability::ConditionedReachabilityEntryPose;

const MAGIC: &[u8; 8] = b"CLLR0002";
const VERSION: u32 = 2;
const HEADER_BYTES: usize = 128;
const RECORD_BASE_BYTES: usize = 72;
const BOOLEAN_RELATION: u8 = 1;
const MAX_PACK_BYTES: usize = 16 * 1024 * 1024;
const MAX_POSES_PER_SIDE: usize = 640;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalRelationBinding {
    pub kick_profile: KickTableProfileId,
    pub rule_identity: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalRelationPackError {
    TooLarge,
    Header,
    UnsupportedProfile,
    BindingMismatch,
    SnapshotMismatch,
    PayloadDigest,
    GenerationIdentity,
    Record,
    NonCanonicalOrder,
    Index(LocalRelationIndexError),
}

impl LocalRelationPackError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::TooLarge => "local_relation_pack_too_large",
            Self::Header => "local_relation_pack_header_invalid",
            Self::UnsupportedProfile => "local_relation_pack_profile_unsupported",
            Self::BindingMismatch => "local_relation_pack_binding_mismatch",
            Self::SnapshotMismatch => "local_relation_pack_snapshot_mismatch",
            Self::PayloadDigest => "local_relation_pack_payload_digest_mismatch",
            Self::GenerationIdentity => "local_relation_pack_generation_identity_mismatch",
            Self::Record => "local_relation_pack_record_invalid",
            Self::NonCanonicalOrder => "local_relation_pack_order_noncanonical",
            Self::Index(_) => "local_relation_pack_index_invalid",
        }
    }
}

/// Structurally valid candidate. Its lookup is never an authoritative product
/// prune: a miss, hit or empty local lock set still needs global composition.
pub struct LocalRelationCandidatePack {
    binding: LocalRelationBinding,
    generation_identity: [u8; 32],
    payload_identity: [u8; 32],
    encoded_identity: [u8; 32],
    encoded_bytes: usize,
    index: LocalRelationCandidateIndex,
}

impl LocalRelationCandidatePack {
    pub const fn binding(&self) -> LocalRelationBinding {
        self.binding
    }

    pub const fn generation_identity(&self) -> [u8; 32] {
        self.generation_identity
    }

    pub const fn payload_identity(&self) -> [u8; 32] {
        self.payload_identity
    }

    /// Digest of the complete serialized bundle, including its header. The
    /// payload digest above covers only records and must not be substituted
    /// for the signed asset's complete-file identity.
    pub const fn encoded_identity(&self) -> [u8; 32] {
        self.encoded_identity
    }

    pub const fn encoded_bytes(&self) -> usize {
        self.encoded_bytes
    }

    /// Known Rust-owned storage after parsing. This excludes allocator and
    /// process overhead, so the product's hard peak still needs measurement.
    pub fn logical_resident_bytes(&self) -> usize {
        core::mem::size_of::<Self>().saturating_add(self.index.retained_bytes())
    }

    pub fn record_count(&self) -> usize {
        self.index.record_count()
    }

    #[cfg(any(test, feature = "qualification-reference"))]
    pub(crate) fn records(&self) -> &[ExactConditionedLocalRelation] {
        self.index.records()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn lookup(
        &self,
        width: u8,
        height: u8,
        board: u64,
        piece: PieceKind,
        profile: KickTableProfileId,
        window: ConditionedPoseWindow,
        entries: &[ConditionedReachabilityEntryPose],
    ) -> LocalRelationCandidateLookup<'_> {
        self.index
            .lookup(width, height, board, piece, profile, window, entries)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn lookup_with_frame(
        &self,
        width: u8,
        height: u8,
        board: u64,
        frame: LocalRelationRowFrame,
        piece: PieceKind,
        profile: KickTableProfileId,
        window: ConditionedPoseWindow,
        entries: &[ConditionedReachabilityEntryPose],
    ) -> LocalRelationCandidateLookup<'_> {
        self.index
            .lookup_with_frame(width, height, board, frame, piece, profile, window, entries)
    }
}

/// The domain string includes the full physical-board boundary and the rule
/// feature set. The legal-board digest below is only a conservative upstream
/// shape/kick fingerprint; it does not confer that product's 4L authority.
pub fn built_in_local_relation_binding(
    profile: KickTableProfileId,
) -> Result<LocalRelationBinding, LocalRelationPackError> {
    let base = crate::legal_board::built_in_rule_identity(profile)
        .map_err(|_| LocalRelationPackError::UnsupportedProfile)?;
    let mut digest = Sha256::new();
    digest.update(b"clearra.conditioned-local-relation.rule.v2\0");
    digest.update(base);
    digest.update(
        b"board=physical-width-10-height-1-6\0coordinate=board64-bottom-left\0\
boundary=closed-left-right-bottom-open-top\0entry=actual-placeable-pose-set\0\
window=inclusive-anchor-rectangle\0row-frame=target-height-plus-deleted-original-row-mask\0\
transition=translation-plus-first-success-ordered-kick\0\
result=window-locks-plus-all-first-exits\0evidence=boolean\0",
    );
    Ok(LocalRelationBinding {
        kick_profile: profile,
        rule_identity: digest.finalize().into(),
    })
}

pub fn encode_local_relation_candidate_pack(
    binding: LocalRelationBinding,
    records: &[ExactConditionedLocalRelation],
) -> Result<Vec<u8>, LocalRelationPackError> {
    let mut canonical = records.to_vec();
    canonical.sort_unstable_by(compare_record_key);
    if canonical
        .windows(2)
        .any(|pair| compare_record_key(&pair[0], &pair[1]).is_eq())
    {
        return Err(LocalRelationPackError::NonCanonicalOrder);
    }
    LocalRelationCandidateIndex::new(binding.kick_profile, canonical.clone())
        .map_err(LocalRelationPackError::Index)?;
    let mut output = vec![0_u8; HEADER_BYTES];
    output[..8].copy_from_slice(MAGIC);
    output[8..12].copy_from_slice(&VERSION.to_le_bytes());
    output[12] = encode_profile(binding.kick_profile)?;
    output[13] = BOOLEAN_RELATION;
    output[16..48].copy_from_slice(&binding.rule_identity);
    output[112..120].copy_from_slice(&(canonical.len() as u64).to_le_bytes());
    for record in &canonical {
        encode_record(&mut output, record)?;
        if output.len() > MAX_PACK_BYTES {
            return Err(LocalRelationPackError::TooLarge);
        }
    }
    let encoded_len = output.len() as u64;
    output[120..128].copy_from_slice(&encoded_len.to_le_bytes());
    let payload_identity: [u8; 32] = Sha256::digest(&output[HEADER_BYTES..]).into();
    output[48..80].copy_from_slice(&payload_identity);
    let generation = generation_identity(binding, payload_identity, canonical.len());
    output[80..112].copy_from_slice(&generation);
    Ok(output)
}

/// Coalesce only byte-equivalent logical relations produced by overlapping
/// cover domains. The pack encoder itself remains strict: callers importing
/// an already-canonical record set must not silently lose duplicate keys.
/// A shared key with different evidence is an error, never a tie-break.
pub fn coalesce_identical_local_relation_records(
    records: &mut Vec<ExactConditionedLocalRelation>,
) -> Result<(), LocalRelationPackError> {
    records.sort_unstable_by(compare_record_key);
    if records
        .windows(2)
        .any(|pair| compare_record_key(&pair[0], &pair[1]).is_eq() && pair[0] != pair[1])
    {
        return Err(LocalRelationPackError::NonCanonicalOrder);
    }
    records.dedup_by(|left, right| compare_record_key(left, right).is_eq());
    Ok(())
}

pub fn load_local_relation_candidate_pack(
    bytes: &[u8],
    expected_binding: LocalRelationBinding,
    expected_generation: Option<[u8; 32]>,
) -> Result<LocalRelationCandidatePack, LocalRelationPackError> {
    if bytes.len() > MAX_PACK_BYTES {
        return Err(LocalRelationPackError::TooLarge);
    }
    if bytes.len() < HEADER_BYTES || bytes.get(..8) != Some(MAGIC.as_slice()) {
        return Err(LocalRelationPackError::Header);
    }
    if read_u32(&bytes[8..12])? != VERSION
        || bytes[13] != BOOLEAN_RELATION
        || bytes[14..16].iter().any(|byte| *byte != 0)
        || read_u64(&bytes[120..128])? != bytes.len() as u64
    {
        return Err(LocalRelationPackError::Header);
    }
    let binding = LocalRelationBinding {
        kick_profile: decode_profile(bytes[12])?,
        rule_identity: bytes[16..48]
            .try_into()
            .map_err(|_| LocalRelationPackError::Header)?,
    };
    if binding != expected_binding {
        return Err(LocalRelationPackError::BindingMismatch);
    }
    let payload_identity: [u8; 32] = bytes[48..80]
        .try_into()
        .map_err(|_| LocalRelationPackError::Header)?;
    let generation: [u8; 32] = bytes[80..112]
        .try_into()
        .map_err(|_| LocalRelationPackError::Header)?;
    if expected_generation.is_some_and(|expected| expected != generation) {
        return Err(LocalRelationPackError::SnapshotMismatch);
    }
    if Sha256::digest(&bytes[HEADER_BYTES..]).as_slice() != payload_identity {
        return Err(LocalRelationPackError::PayloadDigest);
    }
    let count =
        usize::try_from(read_u64(&bytes[112..120])?).map_err(|_| LocalRelationPackError::Header)?;
    if count == 0 || count > (bytes.len() - HEADER_BYTES) / RECORD_BASE_BYTES {
        return Err(LocalRelationPackError::Header);
    }
    if generation_identity(binding, payload_identity, count) != generation {
        return Err(LocalRelationPackError::GenerationIdentity);
    }
    let mut records = Vec::with_capacity(count);
    let mut cursor = HEADER_BYTES;
    for _ in 0..count {
        let record = decode_record(bytes, &mut cursor, binding.kick_profile)?;
        if records
            .last()
            .is_some_and(|previous| !compare_record_key(previous, &record).is_lt())
        {
            return Err(LocalRelationPackError::NonCanonicalOrder);
        }
        records.push(record);
    }
    if cursor != bytes.len() {
        return Err(LocalRelationPackError::Record);
    }
    let index = LocalRelationCandidateIndex::new(binding.kick_profile, records)
        .map_err(LocalRelationPackError::Index)?;
    Ok(LocalRelationCandidatePack {
        binding,
        generation_identity: generation,
        payload_identity,
        encoded_identity: Sha256::digest(bytes).into(),
        encoded_bytes: bytes.len(),
        index,
    })
}

fn generation_identity(
    binding: LocalRelationBinding,
    payload_identity: [u8; 32],
    count: usize,
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"clearra.conditioned-local-relation.generation.v2\0");
    digest.update([encode_profile(binding.kick_profile).unwrap_or(u8::MAX)]);
    digest.update(binding.rule_identity);
    digest.update(payload_identity);
    digest.update((count as u64).to_le_bytes());
    digest.finalize().into()
}

fn encode_record(
    output: &mut Vec<u8>,
    record: &ExactConditionedLocalRelation,
) -> Result<(), LocalRelationPackError> {
    if record.entries.len() > MAX_POSES_PER_SIDE || record.exits.len() > MAX_POSES_PER_SIDE {
        return Err(LocalRelationPackError::Record);
    }
    let length = RECORD_BASE_BYTES
        .checked_add(
            record
                .entries
                .len()
                .checked_add(record.exits.len())
                .and_then(|count| count.checked_mul(3))
                .ok_or(LocalRelationPackError::TooLarge)?,
        )
        .ok_or(LocalRelationPackError::TooLarge)?;
    let length = u32::try_from(length).map_err(|_| LocalRelationPackError::TooLarge)?;
    output.extend_from_slice(&length.to_le_bytes());
    output.extend_from_slice(&[
        record.width,
        record.height,
        piece_code(record.piece),
        record.row_frame.deleted_original_rows(),
        record.window.min_x as u8,
        record.window.max_x as u8,
        record.window.min_y as u8,
        record.window.max_y as u8,
    ]);
    output.extend_from_slice(&(record.entries.len() as u16).to_le_bytes());
    output.extend_from_slice(&(record.exits.len() as u16).to_le_bytes());
    output.extend_from_slice(&record.board.to_le_bytes());
    output.extend_from_slice(&record.dependency_mask.to_le_bytes());
    output.extend_from_slice(&record.dependency_occupancy.to_le_bytes());
    for anchors in record.grounded_lock_anchors {
        output.extend_from_slice(&anchors.to_le_bytes());
    }
    for pose in record.entries.iter().chain(&record.exits) {
        output.extend_from_slice(&[pose.rotation.quarter_turns(), pose.x as u8, pose.y as u8]);
    }
    Ok(())
}

fn decode_record(
    bytes: &[u8],
    cursor: &mut usize,
    profile: KickTableProfileId,
) -> Result<ExactConditionedLocalRelation, LocalRelationPackError> {
    let base_end = (*cursor)
        .checked_add(RECORD_BASE_BYTES)
        .ok_or(LocalRelationPackError::Record)?;
    let base = bytes
        .get(*cursor..base_end)
        .ok_or(LocalRelationPackError::Record)?;
    let length =
        usize::try_from(read_u32(&base[..4])?).map_err(|_| LocalRelationPackError::Record)?;
    let entry_count = usize::from(read_u16(&base[12..14])?);
    let exit_count = usize::from(read_u16(&base[14..16])?);
    if entry_count == 0
        || entry_count > MAX_POSES_PER_SIDE
        || exit_count > MAX_POSES_PER_SIDE
        || length != RECORD_BASE_BYTES + (entry_count + exit_count) * 3
    {
        return Err(LocalRelationPackError::Record);
    }
    let end = (*cursor)
        .checked_add(length)
        .ok_or(LocalRelationPackError::Record)?;
    let record = bytes
        .get(*cursor..end)
        .ok_or(LocalRelationPackError::Record)?;
    let mut anchors = [0_u64; 4];
    for (rotation, destination) in anchors.iter_mut().enumerate() {
        let start = 40 + rotation * 8;
        *destination = read_u64(&base[start..start + 8])?;
    }
    let mut poses = Vec::with_capacity(entry_count + exit_count);
    for encoded in record[RECORD_BASE_BYTES..].chunks_exact(3) {
        poses.push(ConditionedReachabilityEntryPose {
            rotation: RotationState::from_quarter_turns(encoded[0])
                .map_err(|_| LocalRelationPackError::Record)?,
            x: encoded[1] as i8,
            y: encoded[2] as i8,
        });
    }
    let exits = poses.split_off(entry_count);
    *cursor = end;
    let row_frame = LocalRelationRowFrame::new(base[5], u16::from(base[7]))
        .ok_or(LocalRelationPackError::Record)?;
    Ok(ExactConditionedLocalRelation {
        width: base[4],
        height: base[5],
        board: read_u64(&base[16..24])?,
        row_frame,
        piece: decode_piece(base[6])?,
        kick_profile: profile,
        window: ConditionedPoseWindow {
            min_x: base[8] as i8,
            max_x: base[9] as i8,
            min_y: base[10] as i8,
            max_y: base[11] as i8,
        },
        entries: poses,
        dependency_mask: read_u64(&base[24..32])?,
        dependency_occupancy: read_u64(&base[32..40])?,
        grounded_lock_anchors: anchors,
        exits,
    })
}

fn read_u16(bytes: &[u8]) -> Result<u16, LocalRelationPackError> {
    Ok(u16::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| LocalRelationPackError::Record)?,
    ))
}

fn read_u32(bytes: &[u8]) -> Result<u32, LocalRelationPackError> {
    Ok(u32::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| LocalRelationPackError::Header)?,
    ))
}

fn read_u64(bytes: &[u8]) -> Result<u64, LocalRelationPackError> {
    Ok(u64::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| LocalRelationPackError::Record)?,
    ))
}

fn encode_profile(profile: KickTableProfileId) -> Result<u8, LocalRelationPackError> {
    Ok(match profile {
        KickTableProfileId::Srs90 => 0,
        KickTableProfileId::SrsPlus => 1,
        KickTableProfileId::SrsX => 2,
        KickTableProfileId::Jstris180 => 3,
        KickTableProfileId::NoKick => 4,
        _ => return Err(LocalRelationPackError::UnsupportedProfile),
    })
}

fn decode_profile(code: u8) -> Result<KickTableProfileId, LocalRelationPackError> {
    match code {
        0 => Ok(KickTableProfileId::Srs90),
        1 => Ok(KickTableProfileId::SrsPlus),
        2 => Ok(KickTableProfileId::SrsX),
        3 => Ok(KickTableProfileId::Jstris180),
        4 => Ok(KickTableProfileId::NoKick),
        _ => Err(LocalRelationPackError::UnsupportedProfile),
    }
}

fn decode_piece(code: u8) -> Result<PieceKind, LocalRelationPackError> {
    match code {
        0 => Ok(PieceKind::I),
        1 => Ok(PieceKind::O),
        2 => Ok(PieceKind::T),
        3 => Ok(PieceKind::S),
        4 => Ok(PieceKind::Z),
        5 => Ok(PieceKind::J),
        6 => Ok(PieceKind::L),
        _ => Err(LocalRelationPackError::Record),
    }
}

#[cfg(test)]
#[path = "conditioned_local_pack_tests.rs"]
mod tests;
