//! SRP rationale: one bounded nonblocking relation cache per verifier realm.
//! Misses enqueue optional local-owner queries and IMMEDIATELY return to the
//! exact solver. No network, waiting, or worker-count policy lives here.

use std::sync::{Arc, Mutex, OnceLock, RwLock};

use clearra_accelerator_activation::VerifiedAcceleratorAuthority;
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_rules::kicks::KickTableProfileId;

use crate::conditioned_local_index::{
    compare_context, conditions_overlap, record_is_canonical, LocalRelationCandidateIndex,
    LocalRelationCandidateLookup, LocalRelationContextPreparation, PreparedLocalRelationContext,
};
use crate::conditioned_local_peer_wire::{
    self as wire, LocalRelationPeerError, PeerIdentity, PeerQuery,
};
use crate::conditioned_local_product::{self as product, LocalRelationProductError};
use crate::conditioned_local_relation::{
    ConditionedPoseWindow, ExactConditionedLocalRelation, LocalRelationRowFrame,
};
use crate::conditioned_reachability::ConditionedReachabilityEntryPose;
use crate::legal_board::ProviderStatus;

pub const MIN_RELATION_PEER_RESERVED_BYTES: usize = 1024 * 1024;
pub const MAX_RELATION_PEER_RESERVED_BYTES: usize = 2 * 1024 * 1024;
const MAX_RECORDS: usize = 256;
const MAX_MISSES: usize = 128;
// Input, decoded reply and output/transport coexist during import. Charge
// their complete bounded sizes, not only the installed cache's current use.
const TRANSIENT_RESERVE: usize = 3 * wire::MAX_RELATION_PEER_WIRE_BYTES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PreparedPeerContext {
    identity: PeerIdentity,
    context: PreparedLocalRelationContext,
}

pub(crate) enum PeerRecord<'a> {
    Seed(&'a ExactConditionedLocalRelation),
    Cached(Arc<ExactConditionedLocalRelation>),
}
impl core::ops::Deref for PeerRecord<'_> {
    type Target = ExactConditionedLocalRelation;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Seed(record) => record,
            Self::Cached(record) => record,
        }
    }
}

struct CacheEntry {
    context: u32,
    record: Arc<ExactConditionedLocalRelation>,
    bytes: usize,
}

struct CacheState {
    records: Vec<CacheEntry>,
    bytes: usize,
    pending: Vec<PeerQuery>,
    sent: Vec<PeerQuery>,
    misses: Vec<PeerQuery>,
    invalid: bool,
}

pub(crate) struct QualifiedRelationPeer {
    identity: PeerIdentity,
    heads: LocalRelationCandidateIndex,
    dependencies: Vec<u64>,
    reserved_bytes: usize,
    cache_budget: usize,
    state: Mutex<CacheState>,
}

impl QualifiedRelationPeer {
    /// This is only for the trusted in-app worker channel. Network asset
    /// installation must use the complete file/signature qualification path.
    fn from_trusted_seed(
        bytes: &[u8],
        authority: &VerifiedAcceleratorAuthority,
        reserved_bytes: usize,
    ) -> Result<Self, LocalRelationPeerError> {
        if !(MIN_RELATION_PEER_RESERVED_BYTES..=MAX_RELATION_PEER_RESERVED_BYTES)
            .contains(&reserved_bytes)
        {
            return Err(LocalRelationPeerError::Budget);
        }
        let profile = match authority.profile() {
            "srs" => KickTableProfileId::Srs90,
            "srs-plus" => KickTableProfileId::SrsPlus,
            "srs-x" => KickTableProfileId::SrsX,
            "jstris-180" => KickTableProfileId::Jstris180,
            "no-kick" => KickTableProfileId::NoKick,
            _ => return Err(LocalRelationPeerError::SnapshotMismatch),
        };
        let identity = wire::seed_identity(bytes, profile)?;
        if !identity.matches(authority)
            || crate::conditioned_local_pack::built_in_local_relation_binding(profile)
                .map(|binding| binding.rule_identity != identity.rule)
                .unwrap_or(true)
        {
            return Err(LocalRelationPeerError::SnapshotMismatch);
        }
        let (heads, dependencies) = wire::decode_seed(bytes, identity)?;
        let heads = LocalRelationCandidateIndex::new(profile, heads)
            .map_err(|_| LocalRelationPeerError::InvalidWire)?;
        let state = CacheState {
            records: Vec::with_capacity(MAX_RECORDS),
            bytes: 0,
            pending: Vec::with_capacity(wire::MAX_RELATION_PEER_BATCH),
            sent: Vec::with_capacity(wire::MAX_RELATION_PEER_BATCH),
            misses: Vec::with_capacity(MAX_MISSES),
            invalid: false,
        };
        let control_bytes = core::mem::size_of::<Self>()
            + heads.retained_bytes()
            + dependencies.capacity() * core::mem::size_of::<u64>()
            + state.records.capacity() * core::mem::size_of::<CacheEntry>()
            + (state.pending.capacity() + state.sent.capacity() + state.misses.capacity())
                * core::mem::size_of::<PeerQuery>();
        let cache_budget = reserved_bytes
            .checked_sub(control_bytes + TRANSIENT_RESERVE)
            .filter(|available| *available >= 4096)
            .ok_or(LocalRelationPeerError::Budget)?;
        Ok(Self {
            identity,
            heads,
            dependencies,
            reserved_bytes,
            cache_budget,
            state: Mutex::new(state),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_context(
        &self,
        width: u8,
        height: u8,
        frame: LocalRelationRowFrame,
        piece: PieceKind,
        profile: KickTableProfileId,
        window: ConditionedPoseWindow,
        entries: &[ConditionedReachabilityEntryPose],
    ) -> Result<PreparedPeerContext, ProviderStatus> {
        match self
            .heads
            .prepare_context(width, height, frame, piece, profile, window, entries)
        {
            LocalRelationContextPreparation::Ready(context) => Ok(PreparedPeerContext {
                identity: self.identity,
                context,
            }),
            LocalRelationContextPreparation::Miss => Err(ProviderStatus::Miss),
            LocalRelationContextPreparation::OutOfScope => Err(ProviderStatus::OutOfScope),
        }
    }

    pub(crate) fn lookup(
        &self,
        board: u64,
        prepared: PreparedPeerContext,
    ) -> Result<PeerRecord<'_>, ProviderStatus> {
        if prepared.identity != self.identity {
            return Err(ProviderStatus::SnapshotMismatch);
        }
        if !prepared.context.accepts_board(board) {
            return Err(ProviderStatus::OutOfScope);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| ProviderStatus::InvalidAsset)?;
        if state.invalid {
            return Err(ProviderStatus::InvalidAsset);
        }
        if let LocalRelationCandidateLookup::Hit(record) =
            self.heads.lookup_prepared(board, prepared.context)
        {
            return Ok(PeerRecord::Seed(record));
        }
        let group = prepared.context.group();
        if let Some(position) = state.records.iter().position(|entry| {
            entry.context as usize == group
                && board & entry.record.dependency_mask == entry.record.dependency_occupancy
        }) {
            let entry = state.records.remove(position);
            let record = Arc::clone(&entry.record);
            state.records.push(entry); // bounded LRU, no capacity growth
            return Ok(PeerRecord::Cached(record));
        }
        let query = PeerQuery {
            context: group as u32,
            board: board & self.dependencies[group],
        };
        if state.pending.len() < wire::MAX_RELATION_PEER_BATCH
            && !state.pending.contains(&query)
            && !state.sent.contains(&query)
            && !state.misses.contains(&query)
        {
            state.pending.push(query);
        }
        Err(ProviderStatus::Miss)
    }

    fn drain(&self) -> Result<Option<Vec<u8>>, LocalRelationPeerError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| LocalRelationPeerError::RegistryUnavailable)?;
        if state.invalid {
            return Err(LocalRelationPeerError::InvalidWire);
        }
        if !state.sent.is_empty() || state.pending.is_empty() {
            return Ok(None);
        }
        let wire = wire::encode_queries(self.identity, &state.pending)?;
        // Preserve both preallocated queue capacities: no unbounded batches.
        let count = state.pending.len();
        for index in 0..count {
            let query = state.pending[index];
            state.sent.push(query);
        }
        state.pending.clear();
        Ok(Some(wire))
    }

    fn import(&self, bytes: &[u8]) -> Result<(), LocalRelationPeerError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| LocalRelationPeerError::RegistryUnavailable)?;
        if state.invalid {
            return Err(LocalRelationPeerError::InvalidWire);
        }
        let result = self.validate_and_apply(bytes, &mut state);
        if result.is_err() {
            // The host MUST invalidate the current result when import fails.
            // Poisoning also prevents a caller from ignoring the error and
            // retaining any previously imported negative relation authority.
            state.invalid = true;
            state.records.clear();
            state.bytes = 0;
            state.pending.clear();
            state.sent.clear();
            state.misses.clear();
        }
        result
    }

    fn validate_and_apply(
        &self,
        bytes: &[u8],
        state: &mut CacheState,
    ) -> Result<(), LocalRelationPeerError> {
        let mut replies = wire::decode_reply(bytes, self.identity)?;
        if replies.len() != state.sent.len()
            || state.sent.is_empty()
            || replies
                .iter()
                .zip(&state.sent)
                .any(|(reply, query)| reply.query != *query)
        {
            return Err(LocalRelationPeerError::InFlight);
        }
        // Validate the COMPLETE reply before adding any record to the cache.
        for (position, reply) in replies.iter().enumerate() {
            let Some(record) = reply.record.as_ref() else {
                continue;
            };
            let group = reply.query.context as usize;
            let (head, _) = self
                .heads
                .context_heads()
                .nth(group)
                .ok_or(LocalRelationPeerError::InvalidWire)?;
            if !record_is_canonical(record)
                || compare_context(head, record) != core::cmp::Ordering::Equal
                || record.dependency_mask & !self.dependencies[group] != 0
                || reply.query.board & record.dependency_mask != record.dependency_occupancy
            {
                return Err(LocalRelationPeerError::InvalidWire);
            }
            let differs = |other: &ExactConditionedLocalRelation| {
                conditions_overlap(other, record)
                    && (other.grounded_lock_anchors != record.grounded_lock_anchors
                        || other.exits != record.exits)
            };
            if differs(head)
                || state
                    .records
                    .iter()
                    .any(|other| other.context as usize == group && differs(&other.record))
                || replies[..position].iter().any(|other| {
                    other.query.context as usize == group
                        && other.record.as_ref().is_some_and(differs)
                })
            {
                return Err(LocalRelationPeerError::ConflictingRecord);
            }
        }
        for reply in &mut replies {
            let Some(mut record) = reply.record.take() else {
                if state.misses.len() == MAX_MISSES {
                    state.misses.remove(0);
                }
                state.misses.push(reply.query); // provider miss, NEVER UNSAT
                continue;
            };
            let (head, _) = self
                .heads
                .context_heads()
                .nth(reply.query.context as usize)
                .ok_or(LocalRelationPeerError::InvalidWire)?;
            record.entries = Arc::clone(&head.entries); // share the context's pose slice
            let bytes = core::mem::size_of::<CacheEntry>()
                + core::mem::size_of::<ExactConditionedLocalRelation>()
                + 2 * core::mem::size_of::<usize>()
                + record.exits.capacity()
                    * core::mem::size_of::<ConditionedReachabilityEntryPose>();
            if bytes > self.cache_budget {
                continue;
            } // optional cache, exact fallback
            while state.records.len() == MAX_RECORDS || state.bytes + bytes > self.cache_budget {
                state.bytes -= state.records.remove(0).bytes;
            }
            state.bytes += bytes;
            state.records.push(CacheEntry {
                context: reply.query.context,
                record: Arc::new(record),
                bytes,
            });
        }
        state.sent.clear();
        Ok(())
    }
}

type Slots = [Option<Arc<QualifiedRelationPeer>>; 5];
static PEERS: OnceLock<RwLock<Slots>> = OnceLock::new();

pub fn export_qualified_local_relation_peer_seed(
    profile: KickTableProfileId,
) -> Result<Vec<u8>, LocalRelationPeerError> {
    let owner = product::qualified_local_relation_snapshot(profile)
        .ok_or(LocalRelationPeerError::NotLoaded)?;
    wire::encode_seed(&owner)
}

pub fn answer_qualified_local_relation_peer_queries(
    profile: KickTableProfileId,
    bytes: &[u8],
) -> Result<Vec<u8>, LocalRelationPeerError> {
    let owner = product::qualified_local_relation_snapshot(profile)
        .ok_or(LocalRelationPeerError::NotLoaded)?;
    let queries = wire::decode_queries(bytes, PeerIdentity::owner(&owner))?;
    wire::encode_reply(&owner, &queries)
}

/// Trusted in-app derivative admission, not a public network asset loader.
pub fn install_trusted_local_relation_peer(
    bytes: &[u8],
    authority: &VerifiedAcceleratorAuthority,
    reserved_bytes: usize,
) -> Result<(), LocalRelationPeerError> {
    let peer = QualifiedRelationPeer::from_trusted_seed(bytes, authority, reserved_bytes)?;
    let slot = product::profile_slot(peer.identity.profile)
        .map_err(|_| LocalRelationPeerError::SnapshotMismatch)?;
    let _mutation = crate::legal_board::accelerator_registry_mutation_lock()
        .lock()
        .map_err(|_| LocalRelationPeerError::RegistryUnavailable)?;
    if product::qualified_local_relation_snapshot(peer.identity.profile).is_some() {
        return Err(LocalRelationPeerError::InFlight);
    }
    let combined = reserved_bytes
        .saturating_add(
            product::installed_local_relation_bytes(Some(slot))
                .map_err(|_| LocalRelationPeerError::RegistryUnavailable)?,
        )
        .saturating_add(
            crate::legal_board::installed_legal_board_bytes(None)
                .map_err(|_| LocalRelationPeerError::RegistryUnavailable)?,
        )
        .saturating_add(
            crate::legal_board::installed_legal_board_synopsis_bytes(None)
                .map_err(|_| LocalRelationPeerError::RegistryUnavailable)?,
        )
        .saturating_add(
            crate::conditioned_reachability::installed_conditioned_reachability_bytes(None)
                .map_err(|_| LocalRelationPeerError::RegistryUnavailable)?,
        );
    if combined > crate::legal_board::MAX_ACTIVE_ACCELERATOR_BYTES {
        return Err(LocalRelationPeerError::Budget);
    }
    let mut peers = PEERS
        .get_or_init(|| RwLock::new(Default::default()))
        .write()
        .map_err(|_| LocalRelationPeerError::RegistryUnavailable)?;
    if peers[slot]
        .as_ref()
        .is_some_and(|peer| Arc::strong_count(peer) > 1)
    {
        return Err(LocalRelationPeerError::InFlight);
    }
    peers[slot] = Some(Arc::new(peer));
    Ok(())
}

pub fn remove_trusted_local_relation_peer(
    profile: KickTableProfileId,
) -> Result<bool, LocalRelationPeerError> {
    let _mutation = crate::legal_board::accelerator_registry_mutation_lock()
        .lock()
        .map_err(|_| LocalRelationPeerError::RegistryUnavailable)?;
    let slot =
        product::profile_slot(profile).map_err(|_| LocalRelationPeerError::SnapshotMismatch)?;
    let mut peers = PEERS
        .get_or_init(|| RwLock::new(Default::default()))
        .write()
        .map_err(|_| LocalRelationPeerError::RegistryUnavailable)?;
    if peers[slot]
        .as_ref()
        .is_some_and(|peer| Arc::strong_count(peer) > 1)
    {
        return Err(LocalRelationPeerError::InFlight);
    }
    Ok(peers[slot].take().is_some())
}

pub fn drain_local_relation_peer_queries(
    profile: KickTableProfileId,
) -> Result<Option<Vec<u8>>, LocalRelationPeerError> {
    peer_snapshot(profile)
        .ok_or(LocalRelationPeerError::NotLoaded)?
        .drain()
}
pub fn import_trusted_local_relation_peer_reply(
    profile: KickTableProfileId,
    bytes: &[u8],
) -> Result<(), LocalRelationPeerError> {
    peer_snapshot(profile)
        .ok_or(LocalRelationPeerError::NotLoaded)?
        .import(bytes)
}
pub(crate) fn peer_snapshot(profile: KickTableProfileId) -> Option<Arc<QualifiedRelationPeer>> {
    let slot = product::profile_slot(profile).ok()?;
    PEERS.get()?.read().ok()?.get(slot)?.clone()
}
pub(crate) fn installed_peer_bytes(
    exclude_slot: Option<usize>,
) -> Result<usize, LocalRelationProductError> {
    let Some(peers) = PEERS.get() else {
        return Ok(0);
    };
    let peers = peers
        .read()
        .map_err(|_| LocalRelationProductError::RegistryUnavailable)?;
    Ok(peers
        .iter()
        .enumerate()
        .filter(|(slot, _)| Some(*slot) != exclude_slot)
        .filter_map(|(_, peer)| peer.as_ref())
        .fold(0_usize, |bytes, peer| {
            bytes.saturating_add(peer.reserved_bytes)
        }))
}

#[cfg(test)]
#[path = "conditioned_local_peer_tests.rs"]
mod tests;
