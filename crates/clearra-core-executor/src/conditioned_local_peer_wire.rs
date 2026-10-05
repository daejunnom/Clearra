//! SRP rationale: bounded, generation-bound in-app relation transport codec.
//! This wire is not a downloadable asset or a replacement for signature
//! qualification. Only a fully qualified local owner can produce its data.

use clearra_accelerator_activation::{AcceleratorProduct, VerifiedAcceleratorAuthority};
use clearra_rules::kicks::KickTableProfileId;
use sha2::{Digest, Sha256};

use crate::conditioned_local_index::{compare_context, record_is_canonical};
use crate::conditioned_local_pack::{decode_record, encode_record};
use crate::conditioned_local_product::{
    QualifiedLocalRelationPack, LOCAL_RELATION_COMPLETENESS_SCOPE,
};
use crate::conditioned_local_relation::ExactConditionedLocalRelation;
use crate::legal_board::accelerator_profile_name;

pub const MAX_RELATION_PEER_WIRE_BYTES: usize = 256 * 1024;
pub const MAX_RELATION_PEER_BATCH: usize = 64;
pub(crate) const MAX_CONTEXTS: usize = 256;
const HEADER: usize = 192;
const CHECKSUM: usize = 32;
const MAGIC: &[u8; 8] = b"CLLP0001";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalRelationPeerError {
    InvalidWire,
    SnapshotMismatch,
    Budget,
    NotLoaded,
    InFlight,
    ConflictingRecord,
    RegistryUnavailable,
}

impl LocalRelationPeerError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidWire => "local_relation_peer_wire_invalid",
            Self::SnapshotMismatch => "local_relation_peer_snapshot_mismatch",
            Self::Budget => "local_relation_peer_budget_exceeded",
            Self::NotLoaded => "local_relation_peer_not_loaded",
            Self::InFlight => "local_relation_peer_in_flight",
            Self::ConflictingRecord => "local_relation_peer_record_conflict",
            Self::RegistryUnavailable => "local_relation_peer_registry_unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PeerIdentity {
    pub profile: KickTableProfileId,
    pub rule: [u8; 32],
    pub generation: [u8; 32],
    pub statement: [u8; 32],
    pub payload: [u8; 32],
    pub proof: [u8; 32],
}

impl PeerIdentity {
    pub fn owner(pack: &QualifiedLocalRelationPack) -> Self {
        Self {
            profile: pack.binding().kick_profile,
            rule: pack.binding().rule_identity,
            generation: pack.generation_identity(),
            statement: pack.signed_catalog_identity(),
            payload: pack.pack.encoded_identity(),
            proof: pack.bounded_exhaustive_identity(),
        }
    }

    pub fn matches(self, authority: &VerifiedAcceleratorAuthority) -> bool {
        authority.product() == AcceleratorProduct::BoardConditionedReachability
            && accelerator_profile_name(self.profile).ok() == Some(authority.profile())
            && authority.completeness_scope() == LOCAL_RELATION_COMPLETENESS_SCOPE
            && self.rule == authority.rule_identity()
            && self.generation == authority.generation_identity()
            && self.statement == authority.statement_identity()
            && self.payload == authority.payload_identity()
            && self.proof == authority.qualification_identity()
            && self.statement != [0; 32]
            && self.proof != [0; 32]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PeerQuery {
    pub context: u32,
    /// Occupancy projected only by the union of ALL dependencies in this
    /// context. Exact exit continuation still uses the actual physical board.
    pub board: u64,
}

pub(crate) struct PeerReply {
    pub query: PeerQuery,
    pub record: Option<ExactConditionedLocalRelation>,
}

fn start(identity: PeerIdentity, kind: u32, count: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(HEADER);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&kind.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&(count as u32).to_le_bytes());
    let profile =
        crate::conditioned_local_product::profile_slot(identity.profile).unwrap_or(usize::MAX);
    bytes.extend_from_slice(&(profile as u32).to_le_bytes());
    for digest in [
        identity.rule,
        identity.generation,
        identity.statement,
        identity.payload,
        identity.proof,
    ] {
        bytes.extend_from_slice(&digest);
    }
    bytes.extend_from_slice(&[0; 4]);
    bytes
}

fn finish(mut bytes: Vec<u8>) -> Result<Vec<u8>, LocalRelationPeerError> {
    if bytes.len() + CHECKSUM > MAX_RELATION_PEER_WIRE_BYTES {
        return Err(LocalRelationPeerError::Budget);
    }
    let length = (bytes.len() + CHECKSUM) as u32;
    bytes[16..20].copy_from_slice(&length.to_le_bytes());
    let digest = Sha256::digest(&bytes);
    bytes.extend_from_slice(&digest);
    Ok(bytes)
}

fn header(
    bytes: &[u8],
    expected: PeerIdentity,
    kind: u32,
    maximum: usize,
) -> Result<usize, LocalRelationPeerError> {
    if bytes.len() < HEADER + CHECKSUM
        || bytes.len() > MAX_RELATION_PEER_WIRE_BYTES
        || &bytes[..8] != MAGIC
        || u32_at(bytes, 8)? != 1
        || u32_at(bytes, 12)? != kind
        || u32_at(bytes, 16)? as usize != bytes.len()
        || bytes[188..192] != [0; 4]
    {
        return Err(LocalRelationPeerError::InvalidWire);
    }
    let end = bytes.len() - CHECKSUM;
    if Sha256::digest(&bytes[..end])[..] != bytes[end..] {
        return Err(LocalRelationPeerError::InvalidWire);
    }
    let expected_header = start(expected, kind, 0);
    if bytes[24..188] != expected_header[24..188] {
        return Err(LocalRelationPeerError::SnapshotMismatch);
    }
    let count = u32_at(bytes, 20)? as usize;
    if count == 0 || count > maximum {
        return Err(LocalRelationPeerError::InvalidWire);
    }
    Ok(count)
}

/// Signature authority is checked against these fixed fields before parsing
/// any context allocation. Enum decoding comes only from the expected profile.
pub(crate) fn seed_identity(
    bytes: &[u8],
    profile: KickTableProfileId,
) -> Result<PeerIdentity, LocalRelationPeerError> {
    if bytes.len() < HEADER {
        return Err(LocalRelationPeerError::InvalidWire);
    }
    let digest = |at: usize| {
        bytes[at..at + 32]
            .try_into()
            .map_err(|_| LocalRelationPeerError::InvalidWire)
    };
    Ok(PeerIdentity {
        profile,
        rule: digest(28)?,
        generation: digest(60)?,
        statement: digest(92)?,
        payload: digest(124)?,
        proof: digest(156)?,
    })
}

pub(crate) fn encode_seed(
    pack: &QualifiedLocalRelationPack,
) -> Result<Vec<u8>, LocalRelationPeerError> {
    let heads: Vec<_> = pack.pack.context_heads().take(MAX_CONTEXTS + 1).collect();
    if heads.is_empty() || heads.len() > MAX_CONTEXTS {
        return Err(LocalRelationPeerError::Budget);
    }
    let mut bytes = start(PeerIdentity::owner(pack), 0, heads.len());
    for (head, union) in heads {
        bytes.extend_from_slice(&union.to_le_bytes());
        encode_record(&mut bytes, head).map_err(|_| LocalRelationPeerError::InvalidWire)?;
        if bytes.len() > MAX_RELATION_PEER_WIRE_BYTES {
            return Err(LocalRelationPeerError::Budget);
        }
    }
    finish(bytes)
}

pub(crate) fn decode_seed(
    bytes: &[u8],
    identity: PeerIdentity,
) -> Result<(Vec<ExactConditionedLocalRelation>, Vec<u64>), LocalRelationPeerError> {
    let count = header(bytes, identity, 0, MAX_CONTEXTS)?;
    let mut cursor = HEADER;
    let mut records = Vec::with_capacity(count);
    let mut masks = Vec::with_capacity(count);
    for _ in 0..count {
        let mask = u64_at(bytes, cursor)?;
        cursor += 8;
        let record = decode_record(bytes, &mut cursor, identity.profile)
            .map_err(|_| LocalRelationPeerError::InvalidWire)?;
        if !record_is_canonical(&record)
            // Dependency cells use the complete target coordinate frame,
            // including known-empty cells above the compacted physical board.
            // Query occupancy is separately restricted to surviving rows by
            // PreparedLocalRelationContext::accepts_board/record_is_canonical.
            || mask >> (record.width * record.height) != 0
            || record.dependency_mask & !mask != 0
            || records
                .last()
                .is_some_and(|last| compare_context(last, &record) != core::cmp::Ordering::Less)
        {
            return Err(LocalRelationPeerError::InvalidWire);
        }
        records.push(record);
        masks.push(mask);
    }
    if cursor != bytes.len() - CHECKSUM {
        return Err(LocalRelationPeerError::InvalidWire);
    }
    Ok((records, masks))
}

pub(crate) fn encode_queries(
    identity: PeerIdentity,
    queries: &[PeerQuery],
) -> Result<Vec<u8>, LocalRelationPeerError> {
    if queries.is_empty() || queries.len() > MAX_RELATION_PEER_BATCH {
        return Err(LocalRelationPeerError::InvalidWire);
    }
    let mut bytes = start(identity, 1, queries.len());
    for query in queries {
        write_query(&mut bytes, *query);
    }
    finish(bytes)
}

pub(crate) fn decode_queries(
    bytes: &[u8],
    identity: PeerIdentity,
) -> Result<Vec<PeerQuery>, LocalRelationPeerError> {
    let count = header(bytes, identity, 1, MAX_RELATION_PEER_BATCH)?;
    if bytes.len() != HEADER + count * 12 + CHECKSUM {
        return Err(LocalRelationPeerError::InvalidWire);
    }
    let mut cursor = HEADER;
    let mut queries = Vec::with_capacity(count);
    for _ in 0..count {
        let query = read_query(bytes, &mut cursor)?;
        if queries.contains(&query) {
            return Err(LocalRelationPeerError::InvalidWire);
        }
        queries.push(query);
    }
    Ok(queries)
}

pub(crate) fn encode_reply(
    pack: &QualifiedLocalRelationPack,
    queries: &[PeerQuery],
) -> Result<Vec<u8>, LocalRelationPeerError> {
    let mut bytes = start(PeerIdentity::owner(pack), 2, queries.len());
    for query in queries {
        let context = pack
            .pack
            .prepared_group(query.context as usize)
            .ok_or(LocalRelationPeerError::InvalidWire)?;
        if !context.accepts_board(query.board) {
            return Err(LocalRelationPeerError::InvalidWire);
        }
        write_query(&mut bytes, *query);
        match pack.pack.lookup_prepared(query.board, context) {
            crate::conditioned_local_index::LocalRelationCandidateLookup::Hit(record) => {
                bytes.extend_from_slice(&[1, 0, 0, 0]);
                encode_record(&mut bytes, record)
                    .map_err(|_| LocalRelationPeerError::InvalidWire)?;
            }
            crate::conditioned_local_index::LocalRelationCandidateLookup::Miss => {
                bytes.extend_from_slice(&[0; 4])
            }
            crate::conditioned_local_index::LocalRelationCandidateLookup::OutOfScope => {
                return Err(LocalRelationPeerError::InvalidWire)
            }
        }
    }
    finish(bytes)
}

pub(crate) fn decode_reply(
    bytes: &[u8],
    identity: PeerIdentity,
) -> Result<Vec<PeerReply>, LocalRelationPeerError> {
    let count = header(bytes, identity, 2, MAX_RELATION_PEER_BATCH)?;
    let mut cursor = HEADER;
    let mut replies = Vec::with_capacity(count);
    for _ in 0..count {
        let query = read_query(bytes, &mut cursor)?;
        let flag = bytes
            .get(cursor..cursor + 4)
            .ok_or(LocalRelationPeerError::InvalidWire)?;
        cursor += 4;
        let record = match flag {
            [0, 0, 0, 0] => None,
            [1, 0, 0, 0] => Some(
                decode_record(bytes, &mut cursor, identity.profile)
                    .map_err(|_| LocalRelationPeerError::InvalidWire)?,
            ),
            _ => return Err(LocalRelationPeerError::InvalidWire),
        };
        replies.push(PeerReply { query, record });
    }
    if cursor != bytes.len() - CHECKSUM {
        return Err(LocalRelationPeerError::InvalidWire);
    }
    Ok(replies)
}

fn write_query(bytes: &mut Vec<u8>, query: PeerQuery) {
    bytes.extend_from_slice(&query.context.to_le_bytes());
    bytes.extend_from_slice(&query.board.to_le_bytes());
}
fn read_query(bytes: &[u8], cursor: &mut usize) -> Result<PeerQuery, LocalRelationPeerError> {
    let query = PeerQuery {
        context: u32_at(bytes, *cursor)?,
        board: u64_at(bytes, *cursor + 4)?,
    };
    *cursor += 12;
    Ok(query)
}
fn u32_at(bytes: &[u8], at: usize) -> Result<u32, LocalRelationPeerError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .and_then(|slice| slice.try_into().ok())
            .ok_or(LocalRelationPeerError::InvalidWire)?,
    ))
}
fn u64_at(bytes: &[u8], at: usize) -> Result<u64, LocalRelationPeerError> {
    Ok(u64::from_le_bytes(
        bytes
            .get(at..at + 8)
            .and_then(|slice| slice.try_into().ok())
            .ok_or(LocalRelationPeerError::InvalidWire)?,
    ))
}
