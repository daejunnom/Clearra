#[cfg(any(
    test,
    feature = "placement-dependency-pc",
    all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )
))]
use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_replay::{RotationRequest, ScoringLockEvidence};
#[cfg(any(test, feature = "local-search-ab"))]
use clearra_rules::kicks::normalized_kick_delta;
use clearra_rules::kicks::{KickTableProfile, KickTableProfileId, KickTransition};
use std::sync::{Arc, OnceLock};

use crate::conditioned_local_product::LocalRelationProductLookup;
use crate::conditioned_local_relation::{solver_local_relation_windows, LocalRelationRowFrame};
use crate::conditioned_local_source::{PreparedRelationContext, RelationSource};
use crate::conditioned_reachability::ConditionedReachabilityEntryPose;

#[path = "reachability_local_relation.rs"]
mod local_relation;
pub(crate) use local_relation::exact_local_relation;

#[cfg(any(test, feature = "local-search-ab"))]
#[path = "reachability_dependency_cache.rs"]
mod dependency_cache;
#[cfg(any(test, feature = "local-search-ab"))]
#[path = "reachability_independence.rs"]
mod independence;
#[cfg(any(test, feature = "local-search-ab"))]
#[path = "reachability_no_entry_components.rs"]
mod no_entry_components;
#[cfg(any(test, feature = "local-search-ab"))]
#[path = "reachability_reverse_dependency.rs"]
mod reverse_dependency;

#[cfg(any(test, feature = "local-search-ab"))]
#[path = "monotone_reachability.rs"]
mod monotone_reachability;
#[path = "reachability_static_influence.rs"]
pub(super) mod static_influence;

#[cfg(all(test, not(target_family = "wasm")))]
#[path = "reachability_global_kick_pruning_bench.rs"]
mod global_kick_pruning_bench;
#[cfg(test)]
#[path = "reachability_peer_tests.rs"]
mod peer_tests;
#[cfg(test)]
#[path = "reachability_transition_guard_tests.rs"]
mod transition_guard_tests;

#[cfg(test)]
#[path = "reachability_board_family.rs"]
mod board_family;
#[cfg(test)]
#[path = "reachability_boolean_family.rs"]
mod boolean_family;

#[cfg(any(
    test,
    feature = "placement-dependency-pc",
    all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )
))]
#[path = "reachability_symbolic_prefix.rs"]
mod symbolic_prefix;
#[cfg(any(
    test,
    feature = "placement-dependency-pc",
    all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )
))]
pub(super) use symbolic_prefix::{PrefixLockRelation, PrefixPoseWork};
#[cfg(any(
    test,
    feature = "placement-dependency-pc",
    all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )
))]
#[path = "reachability_symbolic_target.rs"]
mod symbolic_target;
#[cfg(any(
    test,
    feature = "placement-dependency-pc",
    all(
        feature = "local-search-ab",
        feature = "parallel",
        not(target_family = "wasm")
    )
))]
use symbolic_target::ReverseScratch;

use super::{
    catalog::{GeometryCatalog, InstantiatedRealization},
    kick_profiles::{builtin_kick_profile, normalized_kick_delta_wide, KickSkyBounds},
    mix_digest, piece_index,
};

#[path = "reachability_anchor_planes.rs"]
mod anchor_planes;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ReachableLocks {
    anchors: [u64; 4],
}

impl ReachableLocks {
    pub fn insert(&mut self, width: u8, rotation: RotationState, x: i8, y: i8) {
        if x < 0 || y < 0 || x >= width as i8 {
            return;
        }
        let anchor = y as usize * width as usize + x as usize;
        if anchor < 64 {
            self.anchors[rotation.quarter_turns() as usize] |= 1_u64 << anchor;
        }
    }

    pub fn contains(&self, width: u8, rotation: RotationState, x: i8, y: i8) -> bool {
        if x < 0 || y < 0 || x >= width as i8 {
            return false;
        }
        let anchor = y as usize * width as usize + x as usize;
        anchor < 64 && self.anchors[rotation.quarter_turns() as usize] & (1_u64 << anchor) != 0
    }

    pub fn union_with(&mut self, other: Self) {
        for (destination, source) in self.anchors.iter_mut().zip(other.anchors) {
            *destination |= source;
        }
    }

    #[cfg(feature = "placement-dependency-pc")]
    pub(super) fn intersects(&self, other: Self) -> bool {
        self.anchors
            .iter()
            .zip(other.anchors)
            .any(|(&found, wanted)| found & wanted != 0)
    }

    fn contains_all(&self, desired: Self) -> bool {
        self.anchors
            .iter()
            .zip(desired.anchors)
            .all(|(found, wanted)| found & wanted == wanted)
    }
}

/// Query-local reusable storage for one exhaustive SRS+ reachability search.
/// The queue uses a cursor rather than `VecDeque`, so a cache miss performs no
/// allocation after the largest board seen by the worker has been prepared.
#[derive(Default)]
pub(super) struct ReachabilityScratch {
    visited_generations: Vec<u16>,
    generation: u16,
    queue: Vec<u16>,
}

impl ReachabilityScratch {
    fn retained_bytes(&self) -> usize {
        self.visited_generations.capacity() * core::mem::size_of::<u16>()
            + self.queue.capacity() * core::mem::size_of::<u16>()
    }

    fn begin_search(&mut self, state_count: usize) -> u16 {
        if self.visited_generations.len() < state_count {
            self.visited_generations.resize(state_count, 0);
            if self.queue.capacity() < state_count {
                self.queue.reserve(state_count - self.queue.capacity());
            }
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.visited_generations.fill(0);
            self.generation = 1;
        }
        self.queue.clear();
        self.generation
    }
}

pub(super) struct ReachabilitySearchResult {
    pub locks: ReachableLocks,
    pub visited_state_count: usize,
    pub exhaustive: bool,
}

const REACHABILITY_CACHE_CAPACITY: usize = 1 << 16;
const REACHABILITY_CACHE_WAYS: usize = 4;
const REACHABILITY_PAYLOADS_PER_CHUNK: usize = 1 << 8;
const REACHABILITY_PAYLOAD_CHUNKS: usize =
    REACHABILITY_CACHE_CAPACITY / REACHABILITY_PAYLOADS_PER_CHUNK;
const SMALL_SEARCH_EXHAUSTIVE_OBSERVATIONS: u8 = 32;
const LARGE_SEARCH_EXHAUSTIVE_OBSERVATIONS: u8 = 16;

const _: () = assert!(
    REACHABILITY_CACHE_CAPACITY <= u16::MAX as usize + 1
        && REACHABILITY_CACHE_CAPACITY % REACHABILITY_PAYLOADS_PER_CHUNK == 0
);

/// Conservative owned-storage bound, including template compilation scratch,
/// for one connected Board64 catalog. Imported content must use the authority
/// overload rather than estimate a table from its display profile ID.
pub(super) fn checked_reachability_retained_upper_bound(
    width: u8,
    height: u8,
    profile_id: KickTableProfileId,
) -> Option<u128> {
    let profile = builtin_kick_profile(profile_id)?;
    checked_reachability_storage_upper_bound(width, height, |piece| {
        let ceiling = source_ceiling(height, piece, profile.supports_180(), profile);
        let mut maximum_candidates = 0_u128;
        for from in RotationState::ALL {
            let mut candidates = 0_u128;
            for (slot, to) in [
                from.clockwise(),
                from.counter_clockwise(),
                from.rotated_180(),
            ]
            .into_iter()
            .enumerate()
            {
                if slot == 2 && !profile.supports_180() {
                    continue;
                }
                candidates = candidates.checked_add(
                    profile
                        .sequence_for(KickTransition::new(piece, from, to))
                        .map_or(0, |sequence| sequence.offsets().len() as u128),
                )?;
            }
            maximum_candidates = maximum_candidates.max(candidates);
        }
        Some((ceiling, maximum_candidates))
    })
}

pub(super) fn checked_authority_reachability_storage_upper_bound(
    authority: &super::compact_kick_authority::CompactKickAuthority,
) -> Option<u128> {
    let (width, height) = authority.dimensions();
    checked_reachability_storage_upper_bound(width, height, |piece| {
        let table = authority.piece(piece);
        let mut maximum_candidates = 0_u128;
        for orientation in table.ordered.chunks_exact(3) {
            let candidates = orientation.iter().try_fold(0_u128, |count, sequence| {
                count.checked_add(sequence.len() as u128)
            })?;
            maximum_candidates = maximum_candidates.max(candidates);
        }
        Some((table.ceiling, maximum_candidates))
    })?
    .checked_add(authority.retained_bytes() as u128)
}

fn checked_reachability_storage_upper_bound(
    width: u8,
    height: u8,
    mut piece_bounds: impl FnMut(PieceKind) -> Option<(i16, u128)>,
) -> Option<u128> {
    if !(1..=10).contains(&width) || !(1..=6).contains(&height) {
        return None;
    }
    let mut bytes = 0_u128;
    #[cfg(any(test, feature = "local-search-ab"))]
    if super::placement_dependency::enabled() {
        bytes = (dependency_cache::PlacementDependencyCache::retained_upper_bound()
            + independence::static_dependency_peak_upper_bound()) as u128;
    }
    #[cfg(any(test, feature = "local-search-ab"))]
    if static_influence::enabled() {
        bytes = bytes
            .checked_add(static_influence::StaticInfluenceCache::retained_upper_bound() as u128)?;
        if !super::placement_dependency::enabled() {
            bytes = bytes.checked_add(independence::static_dependency_peak_upper_bound() as u128)?;
        }
        bytes = bytes.checked_add(
            7_u128
                .checked_mul(4)?
                .checked_mul(u128::from(width))?
                .checked_mul(u128::from(height))?
                .checked_mul(core::mem::size_of::<OnceLock<Option<u64>>>() as u128)?,
        )?;
    }
    let mut maximum_scratch_states = 0_u128;
    for piece in PieceKind::STANDARD_TETROMINOES {
        let (ceiling, maximum_rotation_targets_per_state) = piece_bounds(piece)?;
        let vertical_states = u128::try_from(ceiling).ok()?.checked_add(1)?;
        let state_count = 4_u128
            .checked_mul(vertical_states)?
            .checked_mul(u128::from(width))?;
        maximum_scratch_states = maximum_scratch_states.max(state_count);
        let rotation_target_count = state_count.checked_mul(maximum_rotation_targets_per_state)?;
        let state_linear_bytes = state_count.checked_mul(
            (core::mem::size_of::<u64>()
                + core::mem::size_of::<[u16; 3]>() * 2
                + core::mem::size_of::<u16>() * 2
                + core::mem::size_of::<ConditionedReachabilityEntryPose>()) as u128,
        )?;
        let state_word_bytes = state_count
            .checked_add(63)?
            .checked_div(64)?
            .checked_mul(core::mem::size_of::<u64>() as u128)?;
        // Forward offsets: 3*n+1; reverse offsets: n+1; the concurrently
        // retained reverse cursor: n+1. Summing each piece's cursor is a
        // conservative peak even though pieces compile one at a time.
        let offset_bytes = state_count
            .checked_mul(5)?
            .checked_add(3)?
            .checked_mul(core::mem::size_of::<u32>() as u128)?;
        // The two push-grown forward vectors can retain geometric capacity;
        // inverse sources are allocated once at their final logical length.
        let forward_capacity = if rotation_target_count == 0 {
            0
        } else {
            rotation_target_count.checked_mul(2)?.max(4)
        };
        let rotation_bytes = forward_capacity
            .checked_mul((core::mem::size_of::<u16>() + core::mem::size_of::<u32>()) as u128)?
            .checked_add(
                rotation_target_count
                    .checked_mul(core::mem::size_of::<ReverseRotationSource>() as u128)?,
            )?;
        let compile_deltas = maximum_rotation_targets_per_state
            .checked_mul(8)?
            .checked_add(12 * 4)?
            .checked_mul(core::mem::size_of::<(i16, i16)>() as u128)?
            .checked_add((12 * core::mem::size_of::<Vec<(i16, i16)>>()) as u128)?;
        bytes = bytes
            .checked_add(state_linear_bytes)?
            .checked_add(state_word_bytes)?
            .checked_add(offset_bytes)?
            .checked_add(rotation_bytes)?
            .checked_add(compile_deltas)?
            .checked_add(
                (core::mem::size_of::<ReachabilityTemplate>() + 2 * core::mem::size_of::<usize>())
                    as u128,
            )?;
    }

    let cache_entry_bytes = (core::mem::size_of::<ReachabilityCacheKey>()
        + core::mem::size_of::<ReachabilityCachePayload>()
        + core::mem::size_of::<ReachabilityObservation>()) as u128;
    bytes =
        bytes.checked_add((REACHABILITY_CACHE_CAPACITY as u128).checked_mul(cache_entry_bytes)?)?;
    bytes = bytes.checked_add(
        (REACHABILITY_PAYLOAD_CHUNKS as u128)
            .checked_mul(core::mem::size_of::<Vec<ReachabilityCachePayload>>() as u128)?,
    )?;
    bytes = bytes.checked_add(
        7_u128
            .checked_mul(conditioned_context_frame_capacity(height)?)?
            .checked_mul(core::mem::size_of::<Option<PreparedRelationWindows>>() as u128)?,
    )?;

    // Scratch is shared across templates. Account for retained growth in
    // both the scalar queue/generation table and the exact lane reverse owner.
    bytes = bytes.checked_add(
        maximum_scratch_states.checked_mul((core::mem::size_of::<u16>() * 4) as u128)?,
    )?;
    #[cfg(any(
        test,
        feature = "placement-dependency-pc",
        all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        )
    ))]
    {
        bytes = bytes.checked_add(ReverseScratch::checked_retained_upper_bound(
            maximum_scratch_states,
        )?)?;
    }
    bytes.checked_add(core::mem::size_of::<ReachabilityWorkspace>() as u128)
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct ReachabilityCacheKey {
    board: u64,
    piece_code: u8,
    observation: u8,
    exhaustive: bool,
    valid: bool,
    payload_index: u16,
}

const _: () = assert!(core::mem::size_of::<ReachabilityCacheKey>() == 16);

#[derive(Clone, Copy)]
struct ReachabilityCachePayload {
    locks: ReachableLocks,
    unreachable_locks: ReachableLocks,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReachabilityCacheLookup {
    Reachable,
    ExhaustivelyUnreachable,
    Search {
        exhaustive: bool,
        admit: bool,
        key_present: bool,
    },
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct ReachabilityObservation {
    tag: u16,
    count: u8,
    valid: bool,
}

const _: () = assert!(core::mem::size_of::<ReachabilityObservation>() == 4);

/// Query-local, bounded exact reachability cache.
///
/// A cold miss needs only the admission observations. Allocate the four-way
/// key table on first admission and the larger lock payloads in fixed chunks
/// as slots are populated. Chunking keeps payload indices stable and avoids
/// an old-plus-new buffer peak when the cache grows. Misses still read only
/// hot keys when the table exists; no payload is accessed before a key match.
/// Partial searches store only concrete evidence. A missing lock is
/// authoritative only when `exhaustive` is set.
#[derive(Default)]
struct ReachabilityCache {
    keys: Vec<ReachabilityCacheKey>,
    payloads: Vec<Vec<ReachabilityCachePayload>>,
    payload_count: usize,
    payload_retained_bytes: usize,
    observations: Vec<ReachabilityObservation>,
    exhaustive_observations: u8,
    allocation_disabled: bool,
}

impl ReachabilityCache {
    #[cfg(feature = "placement-dependency-pc")]
    fn lock_family(&self, board: u64, piece: PieceKind) -> Option<(ReachableLocks, bool)> {
        if self.keys.is_empty() {
            return None;
        }
        let code = piece_index(piece) as u8 + 1;
        let first =
            reachability_cache_set_start(reachability_cache_hash(board, code), self.keys.len());
        for key in &self.keys[first..first + REACHABILITY_CACHE_WAYS] {
            if key.valid && key.board == board && key.piece_code == code {
                let locks = self.payload(key.payload_index).locks;
                return Some((locks, key.exhaustive));
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn query(
        &mut self,
        board: u64,
        piece: PieceKind,
        width: u8,
        rotation: RotationState,
        x: i8,
        y: i8,
    ) -> ReachabilityCacheLookup {
        let piece_code = piece_index(piece) as u8 + 1;
        let hash = reachability_cache_hash(board, piece_code);
        if self.keys.is_empty() {
            if !self.initialize_observations() {
                return ReachabilityCacheLookup::Search {
                    exhaustive: false,
                    admit: false,
                    key_present: false,
                };
            }
        } else {
            let set_start = reachability_cache_set_start(hash, self.keys.len());
            for index in set_start..set_start + REACHABILITY_CACHE_WAYS {
                let key = &self.keys[index];
                if !key.valid || key.board != board || key.piece_code != piece_code {
                    continue;
                }
                let payload = self.payload(key.payload_index);
                if payload.locks.contains(width, rotation, x, y) {
                    return ReachabilityCacheLookup::Reachable;
                }
                if key.exhaustive || payload.unreachable_locks.contains(width, rotation, x, y) {
                    return ReachabilityCacheLookup::ExhaustivelyUnreachable;
                }
                let exhaustive = key.observation >= self.exhaustive_observations;
                self.keys[index].observation = key.observation.saturating_add(1);
                return ReachabilityCacheLookup::Search {
                    exhaustive,
                    admit: true,
                    key_present: true,
                };
            }
        }
        let prior_observations = self.observe(hash);
        ReachabilityCacheLookup::Search {
            exhaustive: false,
            admit: prior_observations > 0,
            key_present: false,
        }
    }

    fn insert(
        &mut self,
        board: u64,
        piece: PieceKind,
        locks: ReachableLocks,
        unreachable_locks: ReachableLocks,
        exhaustive: bool,
        admit: bool,
    ) {
        if !exhaustive && !admit {
            return;
        }
        if self.keys.is_empty() && !self.initialize_keys() {
            return;
        }
        let piece_code = piece_index(piece) as u8 + 1;
        let hash = reachability_cache_hash(board, piece_code);
        let set_start = reachability_cache_set_start(hash, self.keys.len());
        let mut victim = set_start;
        let mut victim_observation = u8::MAX;
        for index in set_start..set_start + REACHABILITY_CACHE_WAYS {
            let key = self.keys[index];
            if key.valid && key.board == board && key.piece_code == piece_code {
                let payload = self.payload_mut(key.payload_index);
                payload.locks.union_with(locks);
                payload.unreachable_locks.union_with(unreachable_locks);
                self.keys[index].exhaustive |= exhaustive;
                self.keys[index].observation = self.keys[index].observation.max(1);
                return;
            }
            if !key.valid {
                victim = index;
                victim_observation = 0;
                break;
            }
            if key.observation < victim_observation {
                victim = index;
                victim_observation = key.observation;
            }
        }
        if !exhaustive && victim_observation > 1 {
            return;
        }
        let payload = ReachabilityCachePayload {
            locks,
            unreachable_locks,
        };
        let key = self.keys[victim];
        let payload_index = if key.valid {
            *self.payload_mut(key.payload_index) = payload;
            key.payload_index
        } else {
            let Some(index) = self.allocate_payload(payload) else {
                // Existing entries are still exact, and replacement can
                // reuse their slots even when growing the cache failed.
                return;
            };
            index
        };
        self.keys[victim] = ReachabilityCacheKey {
            board,
            piece_code,
            observation: 1,
            exhaustive,
            valid: true,
            payload_index,
        };
    }

    fn configure(&mut self, operation_count: usize) {
        self.exhaustive_observations = if operation_count >= 7 {
            LARGE_SEARCH_EXHAUSTIVE_OBSERVATIONS
        } else {
            SMALL_SEARCH_EXHAUSTIVE_OBSERVATIONS
        };
    }

    fn observe(&mut self, hash: u64) -> u8 {
        let index = hash as usize & (self.observations.len() - 1);
        let tag = (hash >> 48) as u16;
        let observation = &mut self.observations[index];
        if !observation.valid || observation.tag != tag {
            *observation = ReachabilityObservation {
                tag,
                count: 1,
                valid: true,
            };
            return 0;
        }
        let prior = observation.count;
        observation.count = observation.count.saturating_add(1);
        prior
    }

    fn initialize_observations(&mut self) -> bool {
        if !self.observations.is_empty() {
            return true;
        }
        if self.allocation_disabled {
            return false;
        }
        if self
            .observations
            .try_reserve_exact(REACHABILITY_CACHE_CAPACITY)
            .is_err()
        {
            self.allocation_disabled = true;
            return false;
        }
        self.observations.resize(
            REACHABILITY_CACHE_CAPACITY,
            ReachabilityObservation::default(),
        );
        if self.exhaustive_observations == 0 {
            self.exhaustive_observations = SMALL_SEARCH_EXHAUSTIVE_OBSERVATIONS;
        }
        true
    }

    fn initialize_keys(&mut self) -> bool {
        if self.allocation_disabled || !self.initialize_observations() {
            return false;
        }
        if self
            .keys
            .try_reserve_exact(REACHABILITY_CACHE_CAPACITY)
            .is_err()
        {
            self.allocation_disabled = true;
            return false;
        }
        self.keys
            .resize(REACHABILITY_CACHE_CAPACITY, ReachabilityCacheKey::default());
        true
    }

    fn allocate_payload(&mut self, payload: ReachabilityCachePayload) -> Option<u16> {
        if self.allocation_disabled {
            return None;
        }
        let index = u16::try_from(self.payload_count).ok()?;
        let chunk_index = usize::from(index) / REACHABILITY_PAYLOADS_PER_CHUNK;
        if chunk_index == self.payloads.len() {
            if self.payloads.is_empty()
                && self
                    .payloads
                    .try_reserve_exact(REACHABILITY_PAYLOAD_CHUNKS)
                    .is_err()
            {
                self.allocation_disabled = true;
                return None;
            }
            let mut chunk = Vec::new();
            if chunk
                .try_reserve_exact(REACHABILITY_PAYLOADS_PER_CHUNK)
                .is_err()
            {
                self.allocation_disabled = true;
                return None;
            }
            self.payload_retained_bytes +=
                chunk.capacity() * core::mem::size_of::<ReachabilityCachePayload>();
            self.payloads.push(chunk);
        }
        self.payloads[chunk_index].push(payload);
        self.payload_count += 1;
        Some(index)
    }

    fn payload(&self, index: u16) -> &ReachabilityCachePayload {
        let index = usize::from(index);
        &self.payloads[index / REACHABILITY_PAYLOADS_PER_CHUNK]
            [index % REACHABILITY_PAYLOADS_PER_CHUNK]
    }

    fn payload_mut(&mut self, index: u16) -> &mut ReachabilityCachePayload {
        let index = usize::from(index);
        &mut self.payloads[index / REACHABILITY_PAYLOADS_PER_CHUNK]
            [index % REACHABILITY_PAYLOADS_PER_CHUNK]
    }

    fn retained_bytes(&self) -> usize {
        self.keys.capacity() * core::mem::size_of::<ReachabilityCacheKey>()
            + self.payloads.capacity() * core::mem::size_of::<Vec<ReachabilityCachePayload>>()
            + self.payload_retained_bytes
            + self.observations.capacity() * core::mem::size_of::<ReachabilityObservation>()
    }
}

fn reachability_cache_hash(board: u64, piece_code: u8) -> u64 {
    mix_digest(board, u64::from(piece_code))
}

fn reachability_cache_set_start(hash: u64, capacity: usize) -> usize {
    let set_count = capacity / REACHABILITY_CACHE_WAYS;
    (hash as usize & (set_count - 1)) * REACHABILITY_CACHE_WAYS
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ReachabilityMetrics {
    pub static_influence: static_influence::StaticInfluenceMetrics,
    pub lock_queries: usize,
    pub harddrop_queries: usize,
    pub harddrop_hits: usize,
    pub cache_reachable_hits: usize,
    pub cache_unreachable_hits: usize,
    pub cache_key_misses: usize,
    pub conditioned_complete_hits: usize,
    pub conditioned_misses: usize,
    pub conditioned_lookup_attempts: usize,
    pub conditioned_empty_entry_sets: usize,
    pub conditioned_no_query_context: usize,
    pub conditioned_cache_short_circuits: usize,
    pub conditioned_out_of_scope: usize,
    pub conditioned_unknown: usize,
    pub conditioned_snapshot_mismatch: usize,
    pub conditioned_invalid_asset: usize,
    pub conditioned_requested: bool,
    pub conditioned_policy_enabled: bool,
    pub conditioned_snapshot_active: bool,
    pub partial_searches: usize,
    pub exhaustive_searches: usize,
}

/// Reusable query-owned reachability engine shared by every buildability
/// stage. Templates are immutable after first use; mutable scratch and cache
/// never escape the query session.
pub(super) struct ReachabilityWorkspace {
    local_import: Option<super::compact_kick_authority::CompactKickAuthority>,
    #[cfg(any(test, feature = "local-search-ab"))]
    placement_dependencies: dependency_cache::PlacementDependencyCache,
    #[cfg(any(test, feature = "local-search-ab"))]
    static_influence_cache: static_influence::StaticInfluenceCache,
    cache: ReachabilityCache,
    scratch: ReachabilityScratch,
    #[cfg(any(
        test,
        feature = "placement-dependency-pc",
        all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        )
    ))]
    symbolic_reverse_scratch: ReverseScratch,
    templates: [Option<Arc<ReachabilityTemplate>>; 7],
    shared_templates: Option<Arc<SharedReachabilityTemplates>>,
    template_dimensions: Option<(u8, u8)>,
    kick_profile_id: KickTableProfileId,
    conditioned: Option<RelationSource>,
    conditioned_profile: Option<KickTableProfileId>,
    conditioned_enabled: Option<bool>,
    conditioned_policy_enabled: Option<bool>,
    conditioned_contexts: [Vec<Option<PreparedRelationWindows>>; 7],
    conditioned_boolean_shortcuts: bool,
    generated_states: usize,
    metrics: ReachabilityMetrics,
}

/// Which part of the original ordered kick list may supply the last edge.
#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum KickWitnessOrder {
    RawFirst,
    LaterOnly,
    // Product experiments currently attribute first and later kicks
    // separately. Keep the inclusive policy available to independent tests.
    #[allow(dead_code)]
    Any,
}

/// Sufficient occupancy bounds for one exact Boolean lock. The owner must
/// keep the catalog, piece, pose, ordered kick profile and deleted-row frame
/// fixed; neither a different target nor a different evidence mode inherits
/// the proof.
#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct KickWitnessGuard {
    pub(super) minimum: u64,
    pub(super) maximum: u64,
}

#[derive(Clone, Copy)]
struct PreparedRelationWindows {
    contexts: [Result<PreparedRelationContext, crate::legal_board::ProviderStatus>; 3],
    count: usize,
}

/// An ordered no-entry proof with a finite occupancy upper bound. The
/// conditional-language owner separately pins its candidate and pose.
#[cfg(any(test, feature = "local-search-ab"))]
#[derive(Clone, Copy)]
pub(super) struct OrderedNoEntryBounds {
    minimum: u64,
    maximum: u64,
}

#[cfg(any(test, feature = "local-search-ab"))]
impl OrderedNoEntryBounds {
    pub(super) fn excludes(self, board: u64) -> bool {
        self.minimum & !board == 0 && board & !self.maximum == 0
    }
}

fn conditioned_context_frame_capacity(height: u8) -> Option<u128> {
    // Vec's first allocation has capacity at least four for this element
    // size, even when a 1L target needs only two original-row masks.
    Some(1_u128.checked_shl(u32::from(height))?.max(4))
}

impl Default for ReachabilityWorkspace {
    fn default() -> Self {
        Self {
            local_import: None,
            #[cfg(any(test, feature = "local-search-ab"))]
            placement_dependencies: dependency_cache::PlacementDependencyCache::default(),
            #[cfg(any(test, feature = "local-search-ab"))]
            static_influence_cache: static_influence::StaticInfluenceCache::default(),
            cache: ReachabilityCache::default(),
            scratch: ReachabilityScratch::default(),
            #[cfg(any(
                test,
                feature = "placement-dependency-pc",
                all(
                    feature = "local-search-ab",
                    feature = "parallel",
                    not(target_family = "wasm")
                )
            ))]
            symbolic_reverse_scratch: ReverseScratch::default(),
            templates: std::array::from_fn(|_| None),
            shared_templates: None,
            template_dimensions: None,
            kick_profile_id: KickTableProfileId::SrsPlus,
            conditioned: None,
            conditioned_profile: None,
            conditioned_enabled: None,
            conditioned_policy_enabled: None,
            conditioned_contexts: std::array::from_fn(|_| Vec::new()),
            conditioned_boolean_shortcuts: true,
            generated_states: 0,
            metrics: ReachabilityMetrics::default(),
        }
    }
}

impl ReachabilityWorkspace {
    pub(super) fn bind_local_import(
        &mut self,
        authority: super::compact_kick_authority::CompactKickAuthority,
        catalog: &GeometryCatalog,
    ) -> Result<(), &'static str> {
        if self.local_import.is_some()
            || self.shared_templates.is_some()
            || self.template_dimensions.is_some()
            || self.conditioned_profile.is_some()
            || authority.dimensions() != (catalog.width(), catalog.height())
        {
            return Err("wasm_local_ordered_kick_owner_mismatch");
        }
        self.kick_profile_id = authority.profile_id();
        self.local_import = Some(authority);
        Ok(())
    }

    pub(super) fn has_local_import(&self) -> bool {
        self.local_import.is_some()
    }

    #[cfg(any(feature = "parallel", test))]
    pub fn set_shared_templates(
        &mut self,
        templates: Arc<SharedReachabilityTemplates>,
    ) -> Result<(), &'static str> {
        if self.shared_templates.is_some()
            || self.template_dimensions.is_some()
            || self.has_local_import()
        {
            return Err("wasm_shared_reachability_request_already_initialized");
        }
        self.local_import = templates.authority.clone();
        if let Some(authority) = &self.local_import {
            self.kick_profile_id = authority.profile_id();
        }
        self.shared_templates = Some(templates);
        Ok(())
    }

    #[cfg(any(feature = "parallel", test))]
    pub fn private_retained_bytes(&self) -> usize {
        self.retained_bytes_with_shared(false)
    }

    pub fn lock_harddrop_reachable_instantiated(
        &mut self,
        width: u8,
        board: u64,
        realization: InstantiatedRealization,
    ) -> bool {
        self.metrics.harddrop_queries = self.metrics.harddrop_queries.saturating_add(1);
        let grounded =
            realization.lock_y == 0 || board & (realization.lock_mask >> usize::from(width)) != 0;
        let reachable = grounded && board & harddrop_path_mask(realization.lock_mask, width) == 0;
        self.metrics.harddrop_hits = self
            .metrics
            .harddrop_hits
            .saturating_add(usize::from(reachable));
        reachable
    }

    #[allow(clippy::too_many_arguments)]
    pub fn lock_reachable_instantiated(
        &mut self,
        catalog: &GeometryCatalog,
        board: u64,
        piece: PieceKind,
        realization: InstantiatedRealization,
    ) -> bool {
        self.lock_reachable_instantiated_in_frame(catalog, board, piece, realization, None)
    }

    /// Only a caller that owns BuildUp's original-row correspondence may
    /// supply a local relation frame. Other callers retain the exact path.
    pub fn lock_reachable_instantiated_in_frame(
        &mut self,
        catalog: &GeometryCatalog,
        board: u64,
        piece: PieceKind,
        realization: InstantiatedRealization,
        frame: Option<LocalRelationRowFrame>,
    ) -> bool {
        self.metrics.lock_queries = self.metrics.lock_queries.saturating_add(1);
        if self.lock_harddrop_reachable_instantiated(catalog.width(), board, realization) {
            return true;
        }
        self.prepare_template(catalog, piece);
        self.lock_reachable_cached(
            catalog,
            board,
            piece,
            realization.rotation,
            realization.x,
            realization.lock_y,
            frame,
        )
    }

    pub fn prepare_template(&mut self, catalog: &GeometryCatalog, piece: PieceKind) {
        let _ = self.template(catalog, piece);
    }

    /// Query all requested lock poses under this workspace's already bound
    /// request authority. Preparing the template also clears dimension-stale
    /// cache entries; an imported ordered table is never reconstructed from a
    /// nominal built-in ID. A partial result contains every desired pose, and
    /// only an exhaustive result can certify absence. The caller charges the
    /// returned actual visited-state count, including zero for exact reuse.
    #[cfg(feature = "placement-dependency-pc")]
    pub(super) fn reachable_locks_for_board(
        &mut self,
        catalog: &GeometryCatalog,
        board: u64,
        piece: PieceKind,
        desired: ReachableLocks,
        control: &ExecutionControl,
    ) -> Result<ReachabilitySearchResult, super::WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(super::WasmExactSearchError::Cancelled);
        }
        let cells = u32::from(catalog.width()) * u32::from(catalog.height());
        if cells == 0 || cells > 64 || (cells < 64 && board >> cells != 0) {
            return Err(super::WasmExactSearchError::InvalidProblem(
                "wasm_observation_board_state_invalid",
            ));
        }
        self.prepare_template(catalog, piece);
        let desired_count = desired
            .anchors
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum::<usize>();
        if desired_count == 0 {
            return Err(super::WasmExactSearchError::InvalidProblem(
                "wasm_observation_reachability_empty_target",
            ));
        }
        self.metrics.lock_queries = self.metrics.lock_queries.saturating_add(desired_count);
        let known = self.cache.lock_family(board, piece);
        if let Some((locks, exhaustive)) = known {
            if exhaustive || locks.contains_all(desired) {
                let positive = locks
                    .anchors
                    .iter()
                    .zip(desired.anchors)
                    .map(|(&found, wanted)| (found & wanted).count_ones() as usize)
                    .sum::<usize>();
                self.metrics.cache_reachable_hits =
                    self.metrics.cache_reachable_hits.saturating_add(positive);
                self.metrics.cache_unreachable_hits = self
                    .metrics
                    .cache_unreachable_hits
                    .saturating_add(desired_count - positive);
                self.metrics.conditioned_cache_short_circuits = self
                    .metrics
                    .conditioned_cache_short_circuits
                    .saturating_add(if self.conditioned.is_some() {
                        desired_count
                    } else {
                        0
                    });
                return Ok(ReachabilitySearchResult {
                    locks,
                    visited_state_count: 0,
                    exhaustive,
                });
            }
        }
        self.metrics.cache_key_misses = self
            .metrics
            .cache_key_misses
            .saturating_add(usize::from(known.is_none()));
        let template = self.templates[piece_index(piece)]
            .as_deref()
            .expect("prepared request template");
        let result = search_reachable_locks(template, board, &mut self.scratch, Some(desired));
        if result.exhaustive {
            self.metrics.exhaustive_searches = self.metrics.exhaustive_searches.saturating_add(1);
        } else {
            self.metrics.partial_searches = self.metrics.partial_searches.saturating_add(1);
        }
        self.generated_states = self
            .generated_states
            .saturating_add(result.visited_state_count);
        self.cache.insert(
            board,
            piece,
            result.locks,
            ReachableLocks::default(),
            result.exhaustive,
            true,
        );
        if control.is_cancelled() {
            return Err(super::WasmExactSearchError::Cancelled);
        }
        Ok(result)
    }

    /// Prove an entire fixed-row occupancy interval from an existing positive
    /// observation. The explicit profile is intentional: structural admission
    /// runs before this workspace is configured for the next candidate. This
    /// method neither configures a template nor performs a primitive search.
    #[cfg(any(test, feature = "local-search-ab"))]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn proves_lock_for_interval(
        &self,
        catalog: &GeometryCatalog,
        minimum_board: u64,
        maximum_board: u64,
        piece: PieceKind,
        profile: KickTableProfileId,
        realization: InstantiatedRealization,
        frame: Option<LocalRelationRowFrame>,
    ) -> bool {
        self.placement_dependencies.proves_positive_interval(
            catalog,
            minimum_board,
            maximum_board,
            piece,
            profile,
            realization.rotation,
            realization.x,
            realization.lock_y,
            frame,
        )
    }

    /// A cold, analytic O proof for a complete fixed-row occupancy interval.
    /// Failure is Unknown; only `proven=true` may replace the physical graph.
    /// The explicit profile and ILC mask belong to the candidate being proved.
    #[cfg(any(test, feature = "local-search-ab"))]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_monotone_lock_interval(
        &self,
        catalog: &GeometryCatalog,
        minimum_board: u64,
        maximum_board: u64,
        piece: PieceKind,
        profile: KickTableProfileId,
        realization: InstantiatedRealization,
        frame: Option<LocalRelationRowFrame>,
    ) -> Option<(bool, usize)> {
        if !static_influence::enabled() || piece != PieceKind::O {
            return None;
        }
        let capability = monotone_reachability::MonotoneOCapability::for_profile(profile)?;
        let attempt = capability.proves_lock_interval(
            catalog.width(),
            catalog.height(),
            minimum_board,
            maximum_board,
            realization.rotation,
            realization.x,
            realization.lock_y,
            realization.lock_mask,
            frame?,
        )?;
        Some((attempt.proven, usize::from(attempt.row_steps)))
    }

    /// A cold positive certificate using only left/right/down translations.
    /// The currently connected builtin profiles share this core's all-x,
    /// all-rotation sky entry and Boolean lock contract. No rotation or ordered
    /// kick edge is needed by the witness. A miss is Unknown, not unreachable.
    #[cfg(any(test, feature = "local-search-ab"))]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_translation_lock_interval(
        &self,
        catalog: &GeometryCatalog,
        minimum_board: u64,
        maximum_board: u64,
        piece: PieceKind,
        profile: KickTableProfileId,
        realization: InstantiatedRealization,
        frame: Option<LocalRelationRowFrame>,
    ) -> Option<(bool, usize)> {
        if !super::structural_order::translation_intervals_enabled() {
            return None;
        }
        let _ = builtin_kick_profile(profile)?;
        let attempt = monotone_reachability::proves_translation_lock_interval(
            piece,
            catalog.width(),
            catalog.height(),
            minimum_board,
            maximum_board,
            realization.rotation,
            realization.x,
            realization.lock_y,
            realization.lock_mask,
            frame?,
        )?;
        Some((attempt.proven, usize::from(attempt.row_steps)))
    }

    /// Prove one Boolean lock over a fixed-frame occupancy interval by
    /// descending from a canonical sky seed and then using raw kick zero.
    /// Neither a failed attempt nor a different ILC realization inherits
    /// any conclusion. This does not supply witness or rotation evidence.
    #[cfg(any(test, feature = "local-search-ab"))]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_prove_first_kick_lock_interval(
        &mut self,
        catalog: &GeometryCatalog,
        minimum_board: u64,
        maximum_board: u64,
        piece: PieceKind,
        profile: KickTableProfileId,
        realization: InstantiatedRealization,
        frame: Option<LocalRelationRowFrame>,
    ) -> bool {
        self.try_last_kick_lock_interval::<false>(
            catalog,
            minimum_board,
            maximum_board,
            piece,
            profile,
            realization,
            frame,
            KickWitnessOrder::RawFirst,
        )
        .is_some()
    }

    /// Prove Boolean reachability by a raw kick index strictly above zero.
    /// Earlier kicks must be geometrically inadmissible or blocked on the
    /// minimum occupancy. A miss is Unknown and this supplies no replay,
    /// score, or rotation evidence.
    #[cfg(any(test, feature = "local-search-ab"))]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_prove_ordered_kick_lock_interval(
        &mut self,
        catalog: &GeometryCatalog,
        minimum_board: u64,
        maximum_board: u64,
        piece: PieceKind,
        profile: KickTableProfileId,
        realization: InstantiatedRealization,
        frame: Option<LocalRelationRowFrame>,
    ) -> bool {
        self.try_last_kick_lock_interval::<false>(
            catalog,
            minimum_board,
            maximum_board,
            piece,
            profile,
            realization,
            frame,
            KickWitnessOrder::LaterOnly,
        )
        .is_some()
    }

    /// Return a reusable witness guard rather than only acknowledging the
    /// input interval. It can accept incomparable occupancies, but only while
    /// holding the same physical pose and original-row frame fixed.
    #[cfg(any(test, feature = "local-search-ab"))]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_kick_witness_guard(
        &mut self,
        catalog: &GeometryCatalog,
        minimum_board: u64,
        maximum_board: u64,
        piece: PieceKind,
        profile: KickTableProfileId,
        realization: InstantiatedRealization,
        frame: Option<LocalRelationRowFrame>,
        order: KickWitnessOrder,
    ) -> Option<KickWitnessGuard> {
        self.try_last_kick_lock_interval::<true>(
            catalog,
            minimum_board,
            maximum_board,
            piece,
            profile,
            realization,
            frame,
            order,
        )
    }

    #[cfg(any(test, feature = "local-search-ab"))]
    #[allow(clippy::too_many_arguments)]
    fn try_last_kick_lock_interval<const RETAIN_GUARD: bool>(
        &mut self,
        catalog: &GeometryCatalog,
        minimum_board: u64,
        maximum_board: u64,
        piece: PieceKind,
        profile: KickTableProfileId,
        realization: InstantiatedRealization,
        frame: Option<LocalRelationRowFrame>,
        order: KickWitnessOrder,
    ) -> Option<KickWitnessGuard> {
        if self.kick_profile_id != profile
            || builtin_kick_profile(profile).is_none()
            || minimum_board & !maximum_board != 0
        {
            return None;
        }
        let frame = frame.filter(|frame| frame.target_height() == catalog.height())?;
        if !frame.accepts_physical_board(catalog.width(), maximum_board | realization.lock_mask)
            || realization.lock_mask.count_ones() != 4
        {
            return None;
        }
        let frame_mask = if RETAIN_GUARD {
            let live_bits = u32::from(catalog.width()) * u32::from(frame.surviving_rows());
            1_u64.checked_shl(live_bits)? - 1
        } else {
            maximum_board
        };
        let template = self.template(catalog, piece);
        let state = State {
            rotation: realization.rotation,
            x: realization.x,
            y: i16::from(realization.lock_y),
        };
        let target = state_index(template.width, template.ceiling, state)?;
        if template.state_masks[target] != realization.lock_mask {
            return None;
        }
        monotone_reachability::last_kick_lock_interval::<RETAIN_GUARD>(
            template,
            minimum_board,
            maximum_board,
            target,
            frame_mask,
            order,
        )
    }

    /// Prove that this exact ILC pose has no entry on any board containing the
    /// minimum occupancy in the same original-row frame. A miss is Unknown.
    /// Only canonical sky entry is covered; callers requiring a full operation
    /// must prove every concrete realization independently. The optional
    /// horizontal quotient preserves this contract and uses the same template.
    #[cfg(any(test, feature = "local-search-ab"))]
    pub(super) fn try_prove_lock_pose_unreachable_at_minimum(
        &mut self,
        catalog: &GeometryCatalog,
        minimum_board: u64,
        piece: PieceKind,
        profile: KickTableProfileId,
        realization: InstantiatedRealization,
        frame: Option<LocalRelationRowFrame>,
    ) -> bool {
        let Some((template, target)) =
            self.no_entry_pose(catalog, minimum_board, piece, profile, realization, frame)
        else {
            return false;
        };
        if no_entry_components::enabled() {
            no_entry_components::proves_no_entry(template, minimum_board, target)
        } else {
            independence::bounded_no_entry_on_minimum_board(template, minimum_board, target)
        }
    }

    /// The experimental conditional-language caller owns exactly one concrete
    /// board. The returned negative only applies to that point and is never
    /// retained as a minimum-board certificate.
    #[cfg(any(test, feature = "local-search-ab"))]
    pub(super) fn try_prove_ordered_lock_pose_unreachable_at_point(
        &mut self,
        catalog: &GeometryCatalog,
        board: u64,
        piece: PieceKind,
        profile: KickTableProfileId,
        realization: InstantiatedRealization,
        frame: Option<LocalRelationRowFrame>,
    ) -> (bool, usize) {
        let Some((template, target)) =
            self.no_entry_pose(catalog, board, piece, profile, realization, frame)
        else {
            return (false, 0);
        };
        independence::bounded_no_entry_on_ordered_interval(template, board, board, target)
    }

    /// Retain a bounded no-entry interval from the very same exact point
    /// proof. A different original-row frame must never reuse it.
    #[cfg(any(test, feature = "local-search-ab"))]
    pub(super) fn try_prove_ordered_lock_pose_closed_within(
        &mut self,
        catalog: &GeometryCatalog,
        board: u64,
        piece: PieceKind,
        profile: KickTableProfileId,
        realization: InstantiatedRealization,
        frame: LocalRelationRowFrame,
    ) -> (Option<OrderedNoEntryBounds>, usize) {
        let cells = u32::from(catalog.width()) * u32::from(frame.surviving_rows());
        let Some(frame_mask) = u64::BITS
            .checked_sub(cells)
            .and_then(|empty_bits| u64::MAX.checked_shr(empty_bits))
        else {
            return (None, 0);
        };
        let Some((template, target)) =
            self.no_entry_pose(catalog, board, piece, profile, realization, Some(frame))
        else {
            return (None, 0);
        };
        let (required_clear, checked_poses) =
            independence::bounded_ordered_no_entry_clear_mask(template, board, target);
        let guard = required_clear
            .map(|clear| OrderedNoEntryBounds {
                minimum: board,
                maximum: frame_mask & !(clear | realization.lock_mask),
            })
            .filter(|bounds| bounds.excludes(board));
        (guard, checked_poses)
    }

    #[cfg(any(test, feature = "local-search-ab"))]
    fn no_entry_pose(
        &mut self,
        catalog: &GeometryCatalog,
        board: u64,
        piece: PieceKind,
        profile: KickTableProfileId,
        realization: InstantiatedRealization,
        frame: Option<LocalRelationRowFrame>,
    ) -> Option<(&ReachabilityTemplate, usize)> {
        // Use the workspace's existing exact template, without reconfiguring
        // its profile or lazily compiling a second transition graph.
        if self.kick_profile_id != profile || builtin_kick_profile(profile).is_none() {
            return None;
        }
        let frame = frame.filter(|frame| frame.target_height() == catalog.height())?;
        if !frame.accepts_physical_board(catalog.width(), board | realization.lock_mask)
            || realization.lock_mask.count_ones() != 4
        {
            return None;
        }
        let template = self.template(catalog, piece);
        let state = State {
            rotation: realization.rotation,
            x: realization.x,
            y: i16::from(realization.lock_y),
        };
        let target = state_index(template.width, template.ceiling, state)?;
        (template.state_masks[target] == realization.lock_mask).then_some((template, target))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn scoring_lock_evidence(
        &mut self,
        catalog: &GeometryCatalog,
        board: u64,
        piece: PieceKind,
        rotation: RotationState,
        x: i8,
        y: i8,
    ) -> ScoringLockEvidence {
        self.prepare_template(catalog, piece);
        let template = self.templates[piece_index(piece)]
            .as_ref()
            .expect("scoring evidence initialized the reachability template");
        let immobile = scoring_lock_is_immobile(template, board, rotation, x, y);
        best_scoring_lock_evidence(template, board, rotation, x, y, &mut self.scratch)
            .unwrap_or_else(|| ScoringLockEvidence::no_rotation(rotation))
            .with_immobile_before_clear(immobile)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn lock_reachable_after_harddrop_miss_in_frame(
        &mut self,
        catalog: &GeometryCatalog,
        board: u64,
        piece: PieceKind,
        rotation: RotationState,
        x: i8,
        y: i8,
        frame: Option<LocalRelationRowFrame>,
    ) -> bool {
        self.metrics.lock_queries = self.metrics.lock_queries.saturating_add(1);
        self.lock_reachable_cached(catalog, board, piece, rotation, x, y, frame)
    }

    // The cache key binds the complete pose and original-row frame together.
    #[allow(clippy::too_many_arguments)]
    fn lock_reachable_cached(
        &mut self,
        catalog: &GeometryCatalog,
        board: u64,
        piece: PieceKind,
        rotation: RotationState,
        x: i8,
        y: i8,
        frame: Option<LocalRelationRowFrame>,
    ) -> bool {
        let (exhaustive, admit, key_present) =
            match self
                .cache
                .query(board, piece, catalog.width(), rotation, x, y)
            {
                ReachabilityCacheLookup::Reachable => {
                    self.metrics.cache_reachable_hits =
                        self.metrics.cache_reachable_hits.saturating_add(1);
                    self.metrics.conditioned_cache_short_circuits = self
                        .metrics
                        .conditioned_cache_short_circuits
                        .saturating_add(usize::from(self.conditioned.is_some()));
                    return true;
                }
                ReachabilityCacheLookup::ExhaustivelyUnreachable => {
                    self.metrics.cache_unreachable_hits =
                        self.metrics.cache_unreachable_hits.saturating_add(1);
                    self.metrics.conditioned_cache_short_circuits = self
                        .metrics
                        .conditioned_cache_short_circuits
                        .saturating_add(usize::from(self.conditioned.is_some()));
                    return false;
                }
                ReachabilityCacheLookup::Search {
                    exhaustive,
                    admit,
                    key_present,
                } => (exhaustive, admit, key_present),
            };
        #[cfg(any(test, feature = "local-search-ab"))]
        let reuse_builtin_proofs = !self.has_local_import();
        #[cfg(any(test, feature = "local-search-ab"))]
        let static_query = if reuse_builtin_proofs && static_influence::enabled() {
            if piece == PieceKind::O {
                if let Some(result) =
                    monotone_reachability::MonotoneOCapability::for_profile(self.kick_profile_id)
                        .and_then(|capability| {
                            capability.lock_reachable(
                                catalog.width(),
                                catalog.height(),
                                board,
                                rotation,
                                x,
                                y,
                            )
                        })
                {
                    self.metrics.static_influence.answer(
                        result.reachable,
                        usize::from(result.row_steps),
                        false,
                    );
                    let cells = usize::from(catalog.width()) * usize::from(catalog.height());
                    let all = u64::MAX >> (64 - cells);
                    if result.influence_mask != all {
                        self.metrics.static_influence.masked_queries = self
                            .metrics
                            .static_influence
                            .masked_queries
                            .saturating_add(1);
                        self.metrics.static_influence.ignored_cells =
                            self.metrics.static_influence.ignored_cells.saturating_add(
                                (all & !result.influence_mask).count_ones() as usize,
                            );
                    }
                    self.cache_exact_target(
                        board,
                        piece,
                        catalog.width(),
                        rotation,
                        x,
                        y,
                        result.reachable,
                        admit,
                    );
                    return result.reachable;
                }
            }
            let query = self.templates[piece_index(piece)]
                .as_ref()
                .and_then(|template| {
                    self.static_influence_cache.prepare(
                        template,
                        board,
                        piece,
                        rotation,
                        x,
                        y,
                        &mut self.metrics.static_influence,
                    )
                });
            if let Some(reachable) =
                query.and_then(|query| self.static_influence_cache.lookup(query))
            {
                self.metrics.static_influence.answer(reachable, 0, true);
                self.cache_exact_target(
                    board,
                    piece,
                    catalog.width(),
                    rotation,
                    x,
                    y,
                    reachable,
                    admit,
                );
                return reachable;
            }
            query
        } else {
            None
        };
        #[cfg(any(test, feature = "local-search-ab"))]
        if let Some(reachable) = reuse_builtin_proofs
            .then(|| {
                self.placement_dependencies.lookup(
                    catalog,
                    board,
                    piece,
                    self.kick_profile_id,
                    rotation,
                    x,
                    y,
                    frame,
                )
            })
            .flatten()
        {
            if let Some(query) = static_query {
                self.static_influence_cache.insert(query, reachable);
            }
            // Preserve the existing exact-board cache as the cheapest first
            // tier. A target certificate proves no other lock on this board.
            let mut desired = ReachableLocks::default();
            desired.insert(catalog.width(), rotation, x, y);
            self.cache.insert(
                board,
                piece,
                if reachable {
                    desired
                } else {
                    ReachableLocks::default()
                },
                if reachable {
                    ReachableLocks::default()
                } else {
                    desired
                },
                false,
                admit,
            );
            return reachable;
        }
        if self.conditioned.is_some()
            && (frame.is_none() || self.templates[piece_index(piece)].is_none())
        {
            self.metrics.conditioned_no_query_context =
                self.metrics.conditioned_no_query_context.saturating_add(1);
        }
        let frame = frame.filter(|frame| {
            let matches = frame.target_height() == catalog.height();
            if !matches && self.conditioned.is_some() {
                self.metrics.conditioned_out_of_scope =
                    self.metrics.conditioned_out_of_scope.saturating_add(1);
            }
            matches
        });
        if let (Some(conditioned), Some(frame), Some(template)) = (
            self.conditioned.as_ref(),
            frame,
            self.templates[piece_index(piece)].as_ref(),
        ) {
            let entries = template.sky_entry_poses.get_or_init(|| {
                canonical_sky_entry_poses(template.width, template.ceiling, &template.sky_seeds)
            });
            if !entries.is_empty() {
                // Profile, frame, window and actual sky entries do not vary
                // with board occupancy. Resolve them once per session rather
                // than allocating/sorting entries and searching known-absent
                // windows on billions of repeated BuildUp lock queries.
                let prepared_frames = &mut self.conditioned_contexts[piece_index(piece)];
                if prepared_frames.is_empty() {
                    prepared_frames.resize(1 << catalog.height(), None);
                }
                let prepared = prepared_frames[usize::from(frame.deleted_original_rows())]
                    .get_or_insert_with(|| {
                        let mut prepared = PreparedRelationWindows {
                            contexts: [Err(crate::legal_board::ProviderStatus::Miss); 3],
                            count: 0,
                        };
                        if let Some((windows, count)) =
                            i8::try_from(template.ceiling).ok().and_then(|ceiling| {
                                solver_local_relation_windows(
                                    catalog.width(),
                                    catalog.height(),
                                    ceiling,
                                )
                            })
                        {
                            prepared.count = count;
                            for (slot, window) in windows.into_iter().take(count).enumerate() {
                                prepared.contexts[slot] = conditioned.prepare_context(
                                    catalog.width(),
                                    catalog.height(),
                                    frame,
                                    piece,
                                    self.kick_profile_id,
                                    window,
                                    entries,
                                );
                            }
                        }
                        prepared
                    });
                let mut saw_miss = false;
                // Prefer a closed full-height relation (no continuation),
                // then a top-half relation, then the original sky window.
                // Each miss is only an asset miss, never a negative result.
                for prepared_context in prepared.contexts.iter().take(prepared.count).copied() {
                    let scratch = &mut self.scratch;
                    let generated_states = &mut self.generated_states;
                    let mut composition_exhaustive = false;
                    let lookup = match prepared_context {
                        Err(status) => LocalRelationProductLookup::PassThrough(status),
                        Ok(context) => {
                            self.metrics.conditioned_lookup_attempts =
                                self.metrics.conditioned_lookup_attempts.saturating_add(1);
                            match conditioned
                                .lookup_prepared_record_for_proven_entries(board, context)
                            {
                                Err(status) => LocalRelationProductLookup::PassThrough(status),
                                Ok(record) => {
                                    let mut anchors = record.grounded_lock_anchors();
                                    composition_exhaustive = record.exits().is_empty();
                                    let already_proved =
                                        anchors_contain(anchors, catalog.width(), rotation, x, y);
                                    if self.conditioned_boolean_shortcuts
                                        && !exhaustive
                                        && !already_proved
                                        && !record.exits().is_empty()
                                    {
                                        // This pack already proves every first exit reachable
                                        // from the same actual sky entries on this dependency-
                                        // matching physical board. Use those poses as success
                                        // seeds for the existing exact reverse traversal. A
                                        // cold Boolean lock query must not restart a broad
                                        // forward search merely because a sky record hit.
                                        if let Some(result) =
                                            reverse_lock_reachable_via_proven_exits(
                                                template,
                                                board,
                                                rotation,
                                                x,
                                                y,
                                                scratch,
                                                record.exits(),
                                            )
                                        {
                                            *generated_states = generated_states
                                                .saturating_add(result.visited_state_count);
                                            self.metrics.conditioned_complete_hits = self
                                                .metrics
                                                .conditioned_complete_hits
                                                .saturating_add(1);
                                            let mut desired = ReachableLocks::default();
                                            desired.insert(catalog.width(), rotation, x, y);
                                            self.cache.insert(
                                                board,
                                                piece,
                                                if result.reachable {
                                                    desired
                                                } else {
                                                    ReachableLocks::default()
                                                },
                                                if result.reachable {
                                                    ReachableLocks::default()
                                                } else {
                                                    desired
                                                },
                                                false,
                                                true,
                                            );
                                            return result.reachable;
                                        }
                                    }
                                    let continued = if !record.exits().is_empty()
                                        && (exhaustive || !already_proved)
                                    {
                                        let mut desired = ReachableLocks::default();
                                        desired.insert(catalog.width(), rotation, x, y);
                                        search_reachable_locks_from_entries_for_desired(
                                            template,
                                            board,
                                            scratch,
                                            record.exits(),
                                            (!exhaustive).then_some(DesiredLocks::Any(desired)),
                                        )
                                    } else {
                                        None
                                    };
                                    if let Some(continued) = continued {
                                        *generated_states = generated_states
                                            .saturating_add(continued.visited_state_count);
                                        composition_exhaustive = continued.exhaustive;
                                        for (local, continued) in
                                            anchors.iter_mut().zip(continued.locks.anchors)
                                        {
                                            *local |= continued;
                                        }
                                        LocalRelationProductLookup::ComposedForProvenEntries(
                                            anchors,
                                        )
                                    } else if composition_exhaustive || already_proved {
                                        LocalRelationProductLookup::ComposedForProvenEntries(
                                            anchors,
                                        )
                                    } else {
                                        LocalRelationProductLookup::PassThrough(
                                            crate::legal_board::ProviderStatus::Unknown,
                                        )
                                    }
                                }
                            }
                        }
                    };
                    match lookup {
                        LocalRelationProductLookup::ComposedForProvenEntries(anchors) => {
                            self.metrics.conditioned_complete_hits =
                                self.metrics.conditioned_complete_hits.saturating_add(1);
                            // Early positive continuation proves only the
                            // returned lock subset. Full exit exhaustion (or
                            // a closed relation) alone authorizes negatives.
                            self.cache.insert(
                                board,
                                piece,
                                ReachableLocks { anchors },
                                ReachableLocks::default(),
                                composition_exhaustive,
                                true,
                            );
                            return anchors_contain(anchors, catalog.width(), rotation, x, y);
                        }
                        LocalRelationProductLookup::PassThrough(status) => {
                            match status {
                                crate::legal_board::ProviderStatus::OutOfScope => {
                                    self.metrics.conditioned_out_of_scope =
                                        self.metrics.conditioned_out_of_scope.saturating_add(1);
                                }
                                crate::legal_board::ProviderStatus::SnapshotMismatch => {
                                    self.metrics.conditioned_snapshot_mismatch = self
                                        .metrics
                                        .conditioned_snapshot_mismatch
                                        .saturating_add(1);
                                }
                                crate::legal_board::ProviderStatus::InvalidAsset => {
                                    self.metrics.conditioned_invalid_asset =
                                        self.metrics.conditioned_invalid_asset.saturating_add(1);
                                    break;
                                }
                                crate::legal_board::ProviderStatus::Unknown => {
                                    self.metrics.conditioned_unknown =
                                        self.metrics.conditioned_unknown.saturating_add(1);
                                    break;
                                }
                                _ => {}
                            }
                            saw_miss |= status == crate::legal_board::ProviderStatus::Miss;
                        }
                    }
                }
                if saw_miss {
                    self.metrics.conditioned_misses =
                        self.metrics.conditioned_misses.saturating_add(1);
                }
            } else {
                self.metrics.conditioned_empty_entry_sets =
                    self.metrics.conditioned_empty_entry_sets.saturating_add(1);
            }
        }
        self.metrics.cache_key_misses = self
            .metrics
            .cache_key_misses
            .saturating_add(usize::from(!key_present));
        if exhaustive {
            self.metrics.exhaustive_searches = self.metrics.exhaustive_searches.saturating_add(1);
        } else {
            self.metrics.partial_searches = self.metrics.partial_searches.saturating_add(1);
        }
        let mut desired = ReachableLocks::default();
        desired.insert(catalog.width(), rotation, x, y);
        let template_index = piece_index(piece);
        let template = self.templates[template_index]
            .as_ref()
            .expect("harddrop query initialized the reachability template");
        let (reachable, locks, unreachable_locks, all_locks_exhaustive, visited_state_count) =
            if exhaustive {
                let result = try_anchor_plane_lock_family(
                    template,
                    piece,
                    board,
                    self.kick_profile_id,
                    self.local_import.as_ref(),
                )
                .unwrap_or_else(|| {
                    search_reachable_locks(template, board, &mut self.scratch, None)
                });
                #[cfg(any(test, feature = "local-search-ab"))]
                if reuse_builtin_proofs {
                    self.placement_dependencies.observe_full(
                        catalog,
                        template,
                        board,
                        piece,
                        self.kick_profile_id,
                        rotation,
                        x,
                        y,
                        frame,
                        &self.scratch,
                        &result,
                    );
                }
                (
                    result.locks.contains(catalog.width(), rotation, x, y),
                    result.locks,
                    ReachableLocks::default(),
                    result.exhaustive,
                    result.visited_state_count,
                )
            } else {
                let result = reverse_lock_reachable_in_request(
                    template,
                    board,
                    rotation,
                    x,
                    y,
                    &mut self.scratch,
                    AnchorMovementRequest {
                        piece,
                        profile: self.kick_profile_id,
                        authority: self.local_import.as_ref(),
                    },
                );
                #[cfg(any(test, feature = "local-search-ab"))]
                if reuse_builtin_proofs {
                    self.placement_dependencies.observe_reverse(
                        catalog,
                        template,
                        board,
                        piece,
                        self.kick_profile_id,
                        rotation,
                        x,
                        y,
                        frame,
                        &self.scratch,
                        &result,
                    );
                }
                (
                    result.reachable,
                    if result.reachable {
                        desired
                    } else {
                        ReachableLocks::default()
                    },
                    if result.reachable {
                        ReachableLocks::default()
                    } else {
                        desired
                    },
                    false,
                    result.visited_state_count,
                )
            };
        self.generated_states = self.generated_states.saturating_add(visited_state_count);
        #[cfg(any(test, feature = "local-search-ab"))]
        if let Some(query) = static_query {
            self.static_influence_cache.insert(query, reachable);
        }
        self.cache.insert(
            board,
            piece,
            locks,
            unreachable_locks,
            all_locks_exhaustive,
            admit,
        );
        reachable
    }

    #[cfg(any(test, feature = "local-search-ab"))]
    #[allow(clippy::too_many_arguments)]
    fn cache_exact_target(
        &mut self,
        board: u64,
        piece: PieceKind,
        width: u8,
        rotation: RotationState,
        x: i8,
        y: i8,
        reachable: bool,
        admit: bool,
    ) {
        let mut desired = ReachableLocks::default();
        desired.insert(width, rotation, x, y);
        self.cache.insert(
            board,
            piece,
            if reachable {
                desired
            } else {
                ReachableLocks::default()
            },
            if reachable {
                ReachableLocks::default()
            } else {
                desired
            },
            false,
            admit,
        );
    }

    pub fn configure(&mut self, operation_count: usize) {
        self.cache.configure(operation_count);
    }

    /// A Boolean proof of one target cannot replace a full lock-family,
    /// concrete witness, spin/finesse evidence or multiplicity request.
    pub fn configure_conditioned_boolean_shortcuts(&mut self, enabled: bool) {
        self.conditioned_boolean_shortcuts = enabled;
    }

    pub fn configure_kick_profile(
        &mut self,
        profile_id: KickTableProfileId,
        conditioned_enabled: bool,
    ) {
        if self.has_local_import() {
            // A nominal profile ID cannot authorize a built-in data pack
            // for a submitted ordered variant. Never replace the owner.
            self.metrics.conditioned_requested = conditioned_enabled;
            return;
        }
        let policy_enabled = crate::search_prune_policy::conditioned_reachability_enabled();
        // Pin one immutable pack for the complete workspace/session. A host
        // update becomes visible only to a new workspace, never mid-result.
        if self.conditioned_profile == Some(profile_id)
            && self.conditioned_enabled == Some(conditioned_enabled)
            && self.conditioned_policy_enabled == Some(policy_enabled)
        {
            return;
        }
        if self
            .conditioned_policy_enabled
            .is_some_and(|previous| previous != policy_enabled)
        {
            self.clear_cache_preserving_policy();
        }
        if self.kick_profile_id != profile_id {
            self.kick_profile_id = profile_id;
            self.templates = std::array::from_fn(|_| None);
            self.template_dimensions = None;
            self.clear_cache_preserving_policy();
        }
        self.metrics.conditioned_requested = conditioned_enabled;
        self.metrics.conditioned_policy_enabled = conditioned_enabled && policy_enabled;
        self.conditioned = (conditioned_enabled && policy_enabled)
            .then(|| RelationSource::snapshot(profile_id))
            .flatten();
        self.metrics.conditioned_snapshot_active = self.conditioned.is_some();
        self.conditioned_profile = Some(profile_id);
        self.conditioned_enabled = Some(conditioned_enabled);
        self.conditioned_policy_enabled = Some(policy_enabled);
        self.conditioned_contexts = std::array::from_fn(|_| Vec::new());
    }

    pub const fn generated_state_count(&self) -> usize {
        self.generated_states
    }

    pub const fn metrics(&self) -> ReachabilityMetrics {
        self.metrics
    }

    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes_with_shared(true)
    }

    fn retained_bytes_with_shared(&self, include_shared: bool) -> usize {
        #[cfg(any(test, feature = "local-search-ab"))]
        let prototype_bytes = self.placement_dependencies.retained_bytes()
            + self.static_influence_cache.retained_bytes();
        #[cfg(not(any(test, feature = "local-search-ab")))]
        let prototype_bytes = 0;
        let import_bytes = self.local_import.as_ref().map_or(0, |authority| {
            let shared = self
                .shared_templates
                .as_ref()
                .and_then(|owner| owner.authority.as_ref())
                == Some(authority);
            if include_shared || !shared {
                authority.retained_bytes()
            } else {
                0
            }
        });
        #[cfg(any(
            test,
            feature = "placement-dependency-pc",
            all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            )
        ))]
        let symbolic_reverse_bytes = self.symbolic_reverse_scratch.retained_bytes();
        #[cfg(not(any(
            test,
            feature = "placement-dependency-pc",
            all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            )
        )))]
        let symbolic_reverse_bytes = 0;
        self.cache.retained_bytes()
            + prototype_bytes
            + import_bytes
            + symbolic_reverse_bytes
            + self
                .templates
                .iter()
                .enumerate()
                .filter_map(|(index, template)| {
                    let template = template.as_ref()?;
                    let shared = self
                        .shared_templates
                        .as_ref()
                        .and_then(|owner| owner.templates[index].get())
                        .is_some_and(|owned| Arc::ptr_eq(owned, template));
                    (include_shared || !shared).then(|| template.retained_bytes())
                })
                .sum::<usize>()
            + self.scratch.retained_bytes()
            + self
                .conditioned_contexts
                .iter()
                .map(|frames| {
                    frames.capacity() * core::mem::size_of::<Option<PreparedRelationWindows>>()
                })
                .sum::<usize>()
    }

    #[cfg(any(
        test,
        feature = "placement-dependency-pc",
        all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        )
    ))]
    pub(super) fn compile_prefix_lock_relation_from_planes(
        &mut self,
        catalog: &GeometryCatalog,
        piece: PieceKind,
        occupied: &[u64; u64::BITS as usize],
        lanes: u64,
        control: &ExecutionControl,
        work: &mut PrefixPoseWork,
    ) -> Result<Option<PrefixLockRelation>, super::WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(super::WasmExactSearchError::Cancelled);
        }
        symbolic_prefix::compile_prefix_lock_relation_from_planes(
            self.template(catalog, piece),
            occupied,
            lanes,
            control,
            work,
        )
    }

    #[cfg(any(
        test,
        feature = "placement-dependency-pc",
        all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        )
    ))]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn compile_prefix_target_lanes_from_planes(
        &mut self,
        catalog: &GeometryCatalog,
        piece: PieceKind,
        occupied: &[u64; u64::BITS as usize],
        lanes: u64,
        locks: &[InstantiatedRealization],
        control: &ExecutionControl,
        work: &mut PrefixPoseWork,
    ) -> Result<Option<u64>, super::WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(super::WasmExactSearchError::Cancelled);
        }
        self.prepare_template(catalog, piece);
        let template = self.templates[piece_index(piece)]
            .as_deref()
            .expect("symbolic reverse reachability initialized the template");
        symbolic_target::compile_prefix_target_lanes_from_planes(
            template,
            occupied,
            lanes,
            locks,
            control,
            work,
            &mut self.symbolic_reverse_scratch,
        )
    }

    fn template(&mut self, catalog: &GeometryCatalog, piece: PieceKind) -> &ReachabilityTemplate {
        let dimensions = (catalog.width(), catalog.height());
        if self.template_dimensions != Some(dimensions) {
            // A query workspace may be reused for another 1-6L target. Both
            // compiled poses and cached board/lock keys depend on dimensions.
            self.templates = std::array::from_fn(|_| None);
            self.clear_cache_preserving_policy();
            self.conditioned_contexts = std::array::from_fn(|_| Vec::new());
            self.template_dimensions = Some(dimensions);
        }
        let index = piece_index(piece);
        let profile_id = self.kick_profile_id;
        self.templates[index].get_or_insert_with(|| {
            if let Some(shared) = &self.shared_templates {
                if shared.matches(
                    catalog.width(),
                    catalog.height(),
                    profile_id,
                    self.local_import.as_ref(),
                ) {
                    return shared.template(piece);
                }
            }
            if let Some(authority) = &self.local_import {
                // Only the immutable catalog used to bind this workspace
                // reaches the local ordered-profile path.
                assert_eq!(authority.dimensions(), dimensions);
                return Arc::new(ReachabilityTemplate::compile_authority(authority, piece));
            }
            // A reused workspace may change dimensions/profile. Never borrow
            // a stale compiled rule; preserve the exact local path instead.
            Arc::new(ReachabilityTemplate::compile(
                catalog.width(),
                catalog.height(),
                piece,
                profile_id,
            ))
        })
    }

    fn clear_cache_preserving_policy(&mut self) {
        #[cfg(any(test, feature = "local-search-ab"))]
        {
            self.static_influence_cache = static_influence::StaticInfluenceCache::default();
        }
        self.cache = ReachabilityCache {
            exhaustive_observations: self.cache.exhaustive_observations,
            ..ReachabilityCache::default()
        };
    }
}

pub(crate) fn exact_spawn_lock_anchors(
    width: u8,
    height: u8,
    board: u64,
    piece: PieceKind,
    profile_id: KickTableProfileId,
) -> Option<[u64; 4]> {
    if width != 10
        || !(1..=6).contains(&height)
        || board >> (u32::from(width) * u32::from(height)) != 0
        || builtin_kick_profile(profile_id).is_none()
    {
        return None;
    }
    let template = ReachabilityTemplate::compile(width, height, piece, profile_id);
    let mut scratch = ReachabilityScratch::default();
    let result = search_reachable_locks(&template, board, &mut scratch, None);
    result.exhaustive.then_some(result.locks.anchors)
}

pub(crate) fn exact_entry_lock_anchors(
    width: u8,
    height: u8,
    board: u64,
    piece: PieceKind,
    profile_id: KickTableProfileId,
    entries: &[ConditionedReachabilityEntryPose],
) -> Option<[u64; 4]> {
    if width != 10
        || !(1..=6).contains(&height)
        || board >> (u32::from(width) * u32::from(height)) != 0
        || entries.is_empty()
        || builtin_kick_profile(profile_id).is_none()
    {
        return None;
    }
    let template = ReachabilityTemplate::compile(width, height, piece, profile_id);
    let mut scratch = ReachabilityScratch::default();
    let result = search_reachable_locks_from_entries(&template, board, &mut scratch, entries)?;
    result.exhaustive.then_some(result.locks.anchors)
}

fn canonical_sky_entry_poses(
    width: u8,
    ceiling: i16,
    sky_seeds: &[u16],
) -> Vec<ConditionedReachabilityEntryPose> {
    // Conditioned packs have their own narrow, declared-entry contract.
    // An arbitrary imported table never acquires a partial seed projection.
    if ceiling > i16::from(i8::MAX) {
        return Vec::new();
    }
    let mut poses = sky_seeds
        .iter()
        .map(|&index| {
            let pose = state_from_index(width, ceiling, usize::from(index));
            ConditionedReachabilityEntryPose {
                rotation: pose.rotation,
                x: pose.x,
                y: i8::try_from(pose.y).expect("checked conditioned sky extent"),
            }
        })
        .collect::<Vec<_>>();
    poses.sort_unstable_by_key(|pose| (pose.rotation.quarter_turns(), pose.x, pose.y));
    poses
}

#[cfg(any(test, feature = "qualification-reference"))]
pub(crate) fn exact_solver_local_relation_spawn_entries(
    height: u8,
    piece: PieceKind,
    profile_id: KickTableProfileId,
) -> Option<(i8, Vec<ConditionedReachabilityEntryPose>)> {
    if !(1..=6).contains(&height) || builtin_kick_profile(profile_id).is_none() {
        return None;
    }
    let template = ReachabilityTemplate::compile(10, height, piece, profile_id);
    Some((
        i8::try_from(template.ceiling).ok()?,
        canonical_sky_entry_poses(template.width, template.ceiling, &template.sky_seeds),
    ))
}

fn anchors_contain(anchors: [u64; 4], width: u8, rotation: RotationState, x: i8, y: i8) -> bool {
    if x < 0 || y < 0 || x >= width as i8 {
        return false;
    }
    let anchor = y as usize * width as usize + x as usize;
    anchor < 64 && anchors[rotation.quarter_turns() as usize] & (1_u64 << anchor) != 0
}

const INVALID_STATE_MASK: u64 = u64::MAX;

/// One execution-request owner, not a process-global/profile-only cache.
/// First-success kick transitions and sky entries are immutable; board-dependent
/// occupancy, visited/frontier and mutable result caches remain worker-private.
pub(super) struct SharedReachabilityTemplates {
    width: u8,
    height: u8,
    profile: KickTableProfileId,
    authority: Option<super::compact_kick_authority::CompactKickAuthority>,
    templates: [OnceLock<Arc<ReachabilityTemplate>>; 7],
}

impl SharedReachabilityTemplates {
    #[cfg(any(feature = "parallel", test))]
    pub fn new(width: u8, height: u8, profile: KickTableProfileId) -> Self {
        Self {
            width,
            height,
            profile,
            authority: None,
            templates: std::array::from_fn(|_| OnceLock::new()),
        }
    }

    #[cfg(any(feature = "parallel", test))]
    pub fn with_authority(authority: super::compact_kick_authority::CompactKickAuthority) -> Self {
        let (width, height) = authority.dimensions();
        Self {
            width,
            height,
            profile: authority.profile_id(),
            authority: Some(authority),
            templates: std::array::from_fn(|_| OnceLock::new()),
        }
    }

    fn matches(
        &self,
        width: u8,
        height: u8,
        profile: KickTableProfileId,
        authority: Option<&super::compact_kick_authority::CompactKickAuthority>,
    ) -> bool {
        self.width == width
            && self.height == height
            && self.profile == profile
            && self.authority.as_ref() == authority
    }

    fn template(&self, piece: PieceKind) -> Arc<ReachabilityTemplate> {
        Arc::clone(self.templates[piece_index(piece)].get_or_init(|| {
            if let Some(authority) = &self.authority {
                return Arc::new(ReachabilityTemplate::compile_authority(authority, piece));
            }
            Arc::new(ReachabilityTemplate::compile(
                self.width,
                self.height,
                piece,
                self.profile,
            ))
        }))
    }

    #[cfg(any(feature = "parallel", test))]
    pub fn retained_bytes(&self) -> usize {
        core::mem::size_of::<Self>()
            + 2 * core::mem::size_of::<usize>()
            + self
                .authority
                .as_ref()
                .map_or(0, |authority| authority.retained_bytes())
            + self
                .templates
                .iter()
                .filter_map(OnceLock::get)
                .map(|template| {
                    core::mem::size_of::<ReachabilityTemplate>()
                        + 2 * core::mem::size_of::<usize>()
                        + template.retained_bytes()
                })
                .sum::<usize>()
    }
}

pub(super) struct ReachabilityTemplate {
    width: u8,
    height: u8,
    ceiling: i16,
    allow_180: bool,
    state_masks: Vec<u64>,
    translation_targets: Vec<[u16; 3]>,
    reverse_translation_sources: Vec<[u16; 3]>,
    sky_seeds: Vec<u16>,
    sky_entry_poses: OnceLock<Vec<ConditionedReachabilityEntryPose>>,
    sky_seed_words: Vec<u64>,
    rotation_target_offsets: Vec<u32>,
    rotation_targets: Vec<u16>,
    rotation_kick_indices: Vec<u32>,
    reverse_rotation_offsets: Vec<u32>,
    reverse_rotation_sources: Vec<ReverseRotationSource>,
    // Impossible on every occupancy, removed once from the reverse index.
    // Forward candidates remain intact for first-success guard reads.
    #[cfg_attr(
        not(any(
            test,
            feature = "placement-dependency-pc",
            all(
                feature = "local-search-ab",
                feature = "parallel",
                not(target_family = "wasm")
            )
        )),
        allow(dead_code)
    )]
    globally_pruned_rotation_sources: usize,
    #[cfg(any(test, feature = "local-search-ab"))]
    #[cfg_attr(not(test), allow(dead_code))]
    global_kick_prune_compile_work: GlobalKickPruneCompileWork,
    // Shared immutable per-target proof, compiled before an occupancy search.
    // None is an unsupported proof, never an unreachable target.
    #[cfg(any(test, feature = "local-search-ab"))]
    static_dependency_masks: Vec<OnceLock<Option<u64>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
struct ReverseRotationSource {
    source: u16,
    slot: u8,
    // Reuses the original alignment byte. Applies only after both endpoints
    // have been proved collision-free; interval callers keep the full guard.
    free_endpoint_guard: u8,
    prior_target_count: u32,
}

const _: () = assert!(core::mem::size_of::<ReverseRotationSource>() == 8);

/// Native experiment modes share the same complete forward guard input.
/// Product builds retain the current implementation until total cost is
/// measured; `Improved` changes evaluation reuse, never the predicate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(
    not(any(test, all(feature = "local-search-ab", not(target_family = "wasm")))),
    allow(dead_code)
)]
enum GlobalKickPruneMode {
    Off,
    Current,
    Improved,
}

/// Compilation diagnostics are owned once by the immutable template. They
/// must not be summed once per relation or once per worker sharing it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct GlobalKickPruneCompileWork {
    #[cfg(any(test, feature = "local-search-ab"))]
    predicate_calls: u64,
    #[cfg(any(test, feature = "local-search-ab"))]
    prior_mask_checks: u64,
    #[cfg(any(test, feature = "local-search-ab"))]
    keep_bitmap_peak_bytes: usize,
}

impl ReachabilityTemplate {
    pub fn compile(
        width: u8,
        height: u8,
        piece: PieceKind,
        profile_id: KickTableProfileId,
    ) -> Self {
        let profile = builtin_kick_profile(profile_id)
            .expect("WASM reachability only compiles connected exact kick profiles");
        let allow_180 = profile.supports_180();
        let mut kick_deltas: [Vec<(i16, i16)>; 12] = std::array::from_fn(|_| Vec::new());
        for from in RotationState::ALL {
            let targets = [
                from.clockwise(),
                from.counter_clockwise(),
                from.rotated_180(),
            ];
            for (slot, to) in targets.into_iter().enumerate() {
                if slot == 2 && !allow_180 {
                    continue;
                }
                let index = rotation_transition_index(from, slot);
                if let Some(sequence) = profile.sequence_for(KickTransition::new(piece, from, to)) {
                    kick_deltas[index].extend(sequence.offsets().iter().map(|offset| {
                        normalized_kick_delta_wide(piece, from, to, offset.dx(), offset.dy())
                    }));
                }
            }
        }
        let ordered = kick_deltas.each_ref().map(Vec::as_slice);
        let ceiling = KickSkyBounds::for_ordered_deltas(ordered).source_ceiling(height);
        Self::compile_normalized(width, height, piece, allow_180, ceiling, ordered)
    }

    pub(super) fn compile_authority(
        authority: &super::compact_kick_authority::CompactKickAuthority,
        piece: PieceKind,
    ) -> Self {
        let validated = authority.piece(piece);
        Self::compile_normalized(
            validated.width,
            validated.height,
            piece,
            validated.allow_180,
            validated.ceiling,
            validated.ordered,
        )
    }

    fn compile_normalized(
        width: u8,
        height: u8,
        piece: PieceKind,
        allow_180: bool,
        ceiling: i16,
        kick_deltas: [&[(i16, i16)]; 12],
    ) -> Self {
        let state_count = 4 * (ceiling as usize + 1) * width as usize;
        let definition = standard_tetromino_registry()
            .get(piece)
            .expect("standard tetromino exists");
        let mut state_masks = vec![INVALID_STATE_MASK; state_count];
        for rotation in RotationState::ALL {
            let shape = definition.shape(rotation);
            for y in 0..=ceiling {
                for x in 0..width as i8 {
                    let state = State { rotation, x, y };
                    let index = state_index(width, ceiling, state)
                        .expect("compiled reachability state is in range");
                    let mut mask = 0_u64;
                    let mut valid = true;
                    for cell in shape.cells() {
                        let cell_x = i16::from(x) + i16::from(cell.x());
                        let cell_y = i16::from(y) + i16::from(cell.y());
                        if cell_x < 0 || cell_x >= i16::from(width) || cell_y < 0 {
                            valid = false;
                            break;
                        }
                        if cell_y < i16::from(height) {
                            mask |= 1_u64 << (cell_y as usize * width as usize + cell_x as usize);
                        }
                    }
                    if valid {
                        state_masks[index] = mask;
                    }
                }
            }
        }
        let (rotation_target_offsets, rotation_targets, rotation_kick_indices) =
            compile_rotation_targets(width, ceiling, &state_masks, &kick_deltas);
        let (translation_targets, sky_seeds) =
            compile_translation_targets_and_seeds(width, height, ceiling, &state_masks);
        let reverse_translation_sources = compile_reverse_translation_sources(&translation_targets);
        let sky_seed_words = compile_seed_words(state_count, &sky_seeds);
        let (reverse_rotation_offsets, reverse_rotation_sources, _global_kick_prune_compile_work) =
            compile_reverse_rotation_sources(
                &state_masks,
                &rotation_target_offsets,
                &rotation_targets,
                global_kick_pruning_mode(),
            );
        let globally_pruned_rotation_sources =
            rotation_targets.len() - reverse_rotation_sources.len();
        Self {
            width,
            height,
            ceiling,
            allow_180,
            state_masks,
            translation_targets,
            reverse_translation_sources,
            sky_seeds,
            sky_entry_poses: OnceLock::new(),
            sky_seed_words,
            rotation_target_offsets,
            rotation_targets,
            rotation_kick_indices,
            reverse_rotation_offsets,
            reverse_rotation_sources,
            globally_pruned_rotation_sources,
            #[cfg(any(test, feature = "local-search-ab"))]
            global_kick_prune_compile_work: _global_kick_prune_compile_work,
            #[cfg(any(test, feature = "local-search-ab"))]
            static_dependency_masks: {
                let mut masks = Vec::new();
                if static_influence::enabled()
                    && (1..=10).contains(&width)
                    && (1..=6).contains(&height)
                {
                    let count = 4 * usize::from(width) * usize::from(height);
                    if masks.try_reserve_exact(count).is_ok() {
                        masks.extend((0..count).map(|_| OnceLock::new()));
                    }
                }
                masks
            },
        }
    }

    pub fn retained_bytes(&self) -> usize {
        #[cfg(any(test, feature = "local-search-ab"))]
        let static_bytes =
            self.static_dependency_masks.capacity() * core::mem::size_of::<OnceLock<Option<u64>>>();
        #[cfg(not(any(test, feature = "local-search-ab")))]
        let static_bytes = 0;
        static_bytes
            + self.state_masks.capacity() * core::mem::size_of::<u64>()
            + self.translation_targets.capacity() * core::mem::size_of::<[u16; 3]>()
            + self.reverse_translation_sources.capacity() * core::mem::size_of::<[u16; 3]>()
            + self.sky_seeds.capacity() * core::mem::size_of::<u16>()
            + self.sky_entry_poses.get().map_or(0, |entries| {
                entries.capacity() * core::mem::size_of::<ConditionedReachabilityEntryPose>()
            })
            + self.sky_seed_words.capacity() * core::mem::size_of::<u64>()
            + self.rotation_target_offsets.capacity() * core::mem::size_of::<u32>()
            + self.rotation_targets.capacity() * core::mem::size_of::<u16>()
            + self.rotation_kick_indices.capacity() * core::mem::size_of::<u32>()
            + self.reverse_rotation_offsets.capacity() * core::mem::size_of::<u32>()
            + self.reverse_rotation_sources.capacity()
                * core::mem::size_of::<ReverseRotationSource>()
    }

    fn is_sky_seed(&self, state_index: usize) -> bool {
        self.sky_seed_words[state_index / 64] & (1_u64 << (state_index % 64)) != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct State {
    rotation: RotationState,
    x: i8,
    y: i16,
}

/// The workspace selected this template from this exact piece and authority.
/// This adapter is used only where the existing policy already requests the
/// complete forward lock family; short target-reverse queries stay unchanged.
fn try_anchor_plane_lock_family(
    template: &ReachabilityTemplate,
    piece: PieceKind,
    board: u64,
    profile: KickTableProfileId,
    authority: Option<&super::compact_kick_authority::CompactKickAuthority>,
) -> Option<ReachabilitySearchResult> {
    if !anchor_planes::full_forward_enabled() || !anchor_plane_trace_compatible() {
        return None;
    }
    let ordered = AnchorMovementRequest {
        piece,
        profile,
        authority,
    }
    .ordered(template)?;
    anchor_planes::search_reachable_locks(template, piece, board, &ordered)
        .map(|result| result.search)
}

fn anchor_plane_trace_compatible() -> bool {
    #[cfg(any(test, feature = "local-search-ab"))]
    if super::placement_dependency::enabled() {
        // These observers recover a certificate from the scalar scratch
        // trace. A bitmap answer must never expose a stale preceding trace.
        return false;
    }
    true
}

#[derive(Clone, Copy)]
struct AnchorMovementRequest<'a> {
    piece: PieceKind,
    profile: KickTableProfileId,
    authority: Option<&'a super::compact_kick_authority::CompactKickAuthority>,
}

impl<'a> AnchorMovementRequest<'a> {
    fn ordered(self, template: &ReachabilityTemplate) -> Option<anchor_planes::OrderedKicks<'a>> {
        if let Some(authority) = self.authority {
            let input = authority.piece(self.piece);
            if (input.width, input.height, input.ceiling, input.allow_180)
                != (
                    template.width,
                    template.height,
                    template.ceiling,
                    template.allow_180,
                )
            {
                return None;
            }
            Some(anchor_planes::OrderedKicks::Normalized(input.ordered))
        } else {
            Some(anchor_planes::OrderedKicks::from_profile(
                self.piece,
                builtin_kick_profile(self.profile)?,
                template.allow_180,
            ))
        }
    }
}

#[derive(Clone, Copy)]
enum DesiredLocks {
    All(ReachableLocks),
    Any(ReachableLocks),
}

pub(super) fn search_reachable_locks(
    template: &ReachabilityTemplate,
    board: u64,
    scratch: &mut ReachabilityScratch,
    desired: Option<ReachableLocks>,
) -> ReachabilitySearchResult {
    search_reachable_locks_from_seed_indices(
        template,
        board,
        scratch,
        desired.map(DesiredLocks::All),
        &template.sky_seeds,
    )
}

/// Finds any one exact reachable lock from a geometrically prefiltered set.
/// A non-exhaustive result is a positive witness; an exhaustive result is a
/// complete negative for the requested lock set under the same ordered kicks.
pub(super) fn search_reachable_any_desired_lock(
    template: &ReachabilityTemplate,
    board: u64,
    scratch: &mut ReachabilityScratch,
    desired: ReachableLocks,
) -> ReachabilitySearchResult {
    search_reachable_locks_from_seed_indices(
        template,
        board,
        scratch,
        Some(DesiredLocks::Any(desired)),
        &template.sky_seeds,
    )
}

/// Exact full-board traversal from the caller's actual entry poses. This is
/// only a relation primitive: a cropped window must separately prove that
/// its boundary is closed before using absence as a negative certificate.
fn search_reachable_locks_from_entries(
    template: &ReachabilityTemplate,
    board: u64,
    scratch: &mut ReachabilityScratch,
    entries: &[ConditionedReachabilityEntryPose],
) -> Option<ReachabilitySearchResult> {
    search_reachable_locks_from_entries_for_desired(template, board, scratch, entries, None)
}

fn search_reachable_locks_from_entries_for_desired(
    template: &ReachabilityTemplate,
    board: u64,
    scratch: &mut ReachabilityScratch,
    entries: &[ConditionedReachabilityEntryPose],
    desired: Option<DesiredLocks>,
) -> Option<ReachabilitySearchResult> {
    if entries.len() > template.state_masks.len() {
        return None;
    }
    for &entry in entries {
        let index = state_index(
            template.width,
            template.ceiling,
            State {
                rotation: entry.rotation,
                x: entry.x,
                y: i16::from(entry.y),
            },
        )?;
        if template.state_masks[index] == INVALID_STATE_MASK
            || board & template.state_masks[index] != 0
        {
            return None;
        }
        u16::try_from(index).ok()?;
    }
    let indices = entries.iter().map(|entry| {
        state_index(
            template.width,
            template.ceiling,
            State {
                rotation: entry.rotation,
                x: entry.x,
                y: i16::from(entry.y),
            },
        )
        .expect("entry poses were validated above") as u16
    });
    Some(search_reachable_locks_from_seed_iter(
        template, board, scratch, desired, indices,
    ))
}

fn search_reachable_locks_from_seed_indices(
    template: &ReachabilityTemplate,
    board: u64,
    scratch: &mut ReachabilityScratch,
    desired: Option<DesiredLocks>,
    seeds: &[u16],
) -> ReachabilitySearchResult {
    search_reachable_locks_from_seed_iter(template, board, scratch, desired, seeds.iter().copied())
}

fn search_reachable_locks_from_seed_iter(
    template: &ReachabilityTemplate,
    board: u64,
    scratch: &mut ReachabilityScratch,
    desired: Option<DesiredLocks>,
    seeds: impl Iterator<Item = u16>,
) -> ReachabilitySearchResult {
    let width = template.width;
    let height = template.height;
    let allow_180 = template.allow_180;
    let ceiling = template.ceiling;
    let state_count = 4 * (ceiling as usize + 1) * width as usize;
    let generation = scratch.begin_search(state_count);

    for seed in seeds {
        push_index_if_placeable(
            template,
            board,
            seed,
            &mut scratch.visited_generations,
            generation,
            &mut scratch.queue,
        );
    }

    let mut locks = ReachableLocks::default();
    let mut queue_cursor = 0;
    while queue_cursor < scratch.queue.len() {
        let state_index = usize::from(scratch.queue[queue_cursor]);
        let state = state_from_index(width, ceiling, state_index);
        queue_cursor += 1;
        if state.y < i16::from(height) && grounded_index(template, board, state_index) {
            let anchor = state.y as usize * width as usize + state.x as usize;
            locks.anchors[state.rotation.quarter_turns() as usize] |= 1_u64 << anchor;
            let desired_reached = match desired {
                Some(DesiredLocks::All(wanted)) => locks.contains_all(wanted),
                Some(DesiredLocks::Any(wanted)) => {
                    wanted.contains(width, state.rotation, state.x, state.y as i8)
                }
                None => false,
            };
            if desired_reached {
                return ReachabilitySearchResult {
                    locks,
                    visited_state_count: queue_cursor,
                    exhaustive: false,
                };
            }
        }

        for &target in &template.translation_targets[state_index] {
            if target != INVALID_STATE_INDEX {
                push_index_if_placeable(
                    template,
                    board,
                    target,
                    &mut scratch.visited_generations,
                    generation,
                    &mut scratch.queue,
                );
            }
        }

        let rotation_count = if allow_180 { 3 } else { 2 };
        for slot in 0..rotation_count {
            if let Some(rotated) = first_successful_kick(template, board, state_index, slot) {
                // The ordered kick selector has already validated this exact
                // target. Reusing that fact avoids a second collision test.
                let index = usize::from(rotated);
                if scratch.visited_generations[index] != generation {
                    scratch.visited_generations[index] = generation;
                    scratch.queue.push(rotated);
                }
            }
        }
    }
    ReachabilitySearchResult {
        locks,
        visited_state_count: queue_cursor,
        exhaustive: true,
    }
}

/// Test-only optimistic reachability superset for independent completion proofs.
///
/// Unlike the product search, this traversal admits every collision-free kick
/// target instead of only the first successful target and records poses without
/// requiring ground support. Every product-reachable lock is therefore present,
/// while an optimistic-only pose is never accepted as a live witness. The
/// completion oracle may use absence from this set as a negative certificate;
/// presence must still pass [`search_reachable_locks`].
#[cfg(test)]
pub(super) fn optimistic_reachable_poses(
    template: &ReachabilityTemplate,
    board: u64,
) -> ReachableLocks {
    let mut visited = vec![false; template.state_masks.len()];
    let mut queue = Vec::with_capacity(template.state_masks.len());
    for &seed in &template.sky_seeds {
        let index = usize::from(seed);
        if template.state_masks[index] != INVALID_STATE_MASK
            && board & template.state_masks[index] == 0
        {
            visited[index] = true;
            queue.push(seed);
        }
    }

    let mut poses = ReachableLocks::default();
    let mut cursor = 0;
    while cursor < queue.len() {
        let index = usize::from(queue[cursor]);
        cursor += 1;
        let state = state_from_index(template.width, template.ceiling, index);
        if state.y < i16::from(template.height) {
            poses.insert(template.width, state.rotation, state.x, state.y as i8);
        }

        for &target in &template.translation_targets[index] {
            push_optimistic_target(template, board, target, &mut visited, &mut queue);
        }
        let start = template.rotation_target_offsets[index * 3] as usize;
        let end = template.rotation_target_offsets
            [index * 3 + if template.allow_180 { 3 } else { 2 }] as usize;
        for &target in &template.rotation_targets[start..end] {
            push_optimistic_target(template, board, target, &mut visited, &mut queue);
        }
    }
    poses
}

#[cfg(test)]
fn push_optimistic_target(
    template: &ReachabilityTemplate,
    board: u64,
    target: u16,
    visited: &mut [bool],
    queue: &mut Vec<u16>,
) {
    if target == INVALID_STATE_INDEX {
        return;
    }
    let index = usize::from(target);
    if !visited[index]
        && template.state_masks[index] != INVALID_STATE_MASK
        && board & template.state_masks[index] == 0
    {
        visited[index] = true;
        queue.push(target);
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ReverseReachabilityResult {
    reachable: bool,
    visited_state_count: usize,
}

fn reverse_lock_reachable(
    template: &ReachabilityTemplate,
    board: u64,
    rotation: RotationState,
    x: i8,
    y: i8,
    scratch: &mut ReachabilityScratch,
) -> ReverseReachabilityResult {
    reverse_lock_reachable_with_request::<false>(template, board, rotation, x, y, scratch, None)
}

#[allow(clippy::too_many_arguments)]
fn reverse_lock_reachable_in_request(
    template: &ReachabilityTemplate,
    board: u64,
    rotation: RotationState,
    x: i8,
    y: i8,
    scratch: &mut ReachabilityScratch,
    request: AnchorMovementRequest<'_>,
) -> ReverseReachabilityResult {
    reverse_lock_reachable_with_request::<true>(
        template,
        board,
        rotation,
        x,
        y,
        scratch,
        Some(request),
    )
}

#[allow(clippy::too_many_arguments)]
fn reverse_lock_reachable_with_request<const ALLOW_ANCHOR: bool>(
    template: &ReachabilityTemplate,
    board: u64,
    rotation: RotationState,
    x: i8,
    y: i8,
    scratch: &mut ReachabilityScratch,
    request: Option<AnchorMovementRequest<'_>>,
) -> ReverseReachabilityResult {
    let target = State {
        rotation,
        x,
        y: i16::from(y),
    };
    let Some(target_index) = state_index(template.width, template.ceiling, target) else {
        return ReverseReachabilityResult::default();
    };
    if template.state_masks[target_index] == INVALID_STATE_MASK
        || board & template.state_masks[target_index] != 0
        || !grounded_index(template, board, target_index)
    {
        return ReverseReachabilityResult::default();
    }

    reverse_lock_reachable_large_with_proven_exit_words::<false, ALLOW_ANCHOR>(
        template,
        board,
        target_index,
        scratch,
        &[],
        request,
    )
}

fn reverse_lock_reachable_large(
    template: &ReachabilityTemplate,
    board: u64,
    target_index: usize,
    scratch: &mut ReachabilityScratch,
) -> ReverseReachabilityResult {
    reverse_lock_reachable_large_with_proven_exit_words::<false, false>(
        template,
        board,
        target_index,
        scratch,
        &[],
        None,
    )
}

/// Compose a *qualified matching* relation's globally proved exits with one
/// Boolean target query. This helper never qualifies an arbitrary entry pose:
/// the caller must own the signed relation/context/dependency proof. Invalid
/// or blocked exit footprints return None so the caller retains the original
/// exact path. Standard sky seeds remain in the traversal, so neither missing
/// exits nor a local boundary can turn unknown absence into a negative.
#[allow(clippy::too_many_arguments)]
fn reverse_lock_reachable_via_proven_exits(
    template: &ReachabilityTemplate,
    board: u64,
    rotation: RotationState,
    x: i8,
    y: i8,
    scratch: &mut ReachabilityScratch,
    exits: &[ConditionedReachabilityEntryPose],
) -> Option<ReverseReachabilityResult> {
    // Pack windows have max_y <= 32 and the product width is ten. A larger
    // compiled domain fails open instead of allocating an unbounded bitmap.
    const MAX_PROVEN_EXIT_WORDS: usize = (4_usize * 33 * 10).div_ceil(64);
    let word_count = template.state_masks.len().div_ceil(64);
    if exits.is_empty() || word_count > MAX_PROVEN_EXIT_WORDS {
        return None;
    }
    let mut words = [0_u64; MAX_PROVEN_EXIT_WORDS];
    for exit in exits {
        let index = state_index(
            template.width,
            template.ceiling,
            State {
                rotation: exit.rotation,
                x: exit.x,
                y: i16::from(exit.y),
            },
        )?;
        if template.state_masks[index] == INVALID_STATE_MASK
            || board & template.state_masks[index] != 0
        {
            return None;
        }
        words[index / 64] |= 1_u64 << (index % 64);
    }
    let target = state_index(
        template.width,
        template.ceiling,
        State {
            rotation,
            x,
            y: i16::from(y),
        },
    )?;
    if template.state_masks[target] == INVALID_STATE_MASK
        || board & template.state_masks[target] != 0
        || !grounded_index(template, board, target)
    {
        return Some(ReverseReachabilityResult::default());
    }
    Some(reverse_lock_reachable_large_with_proven_exit_words::<
        true,
        false,
    >(
        template,
        board,
        target,
        scratch,
        &words[..word_count],
        None,
    ))
}

fn reverse_lock_reachable_large_with_proven_exit_words<
    const USE_PROVEN_EXITS: bool,
    const ALLOW_ANCHOR: bool,
>(
    template: &ReachabilityTemplate,
    board: u64,
    target_index: usize,
    scratch: &mut ReachabilityScratch,
    proven_exit_words: &[u64],
    request: Option<AnchorMovementRequest<'_>>,
) -> ReverseReachabilityResult {
    let state_count = template.state_masks.len();
    let generation = scratch.begin_search(state_count);
    // `reverse_lock_reachable` already proved that the target is a valid,
    // collision-free grounded lock. Seed it directly so every short reverse
    // search does not repeat those board-mask checks.
    scratch.visited_generations[target_index] = generation;
    scratch.queue.push(target_index as u16);

    let mut queue_cursor = 0usize;
    let mut next_anchor_check = 32usize;
    while queue_cursor < scratch.queue.len() {
        let current = usize::from(scratch.queue[queue_cursor]);
        if template.is_sky_seed(current)
            || (USE_PROVEN_EXITS
                && proven_exit_words
                    .get(current / 64)
                    .is_some_and(|word| word & (1_u64 << (current % 64)) != 0))
        {
            return ReverseReachabilityResult {
                reachable: true,
                visited_state_count: queue_cursor + 1,
            };
        }
        // Preserve short and sparse scalar queries. The current non-seed is
        // still pending here: advancing the cursor would lose its expansion.
        // Sparse checkpoints are reconsidered only after twice as many pops.
        if ALLOW_ANCHOR
            && queue_cursor == next_anchor_check
            && anchor_planes::hybrid_reverse_enabled()
            && anchor_plane_trace_compatible()
        {
            // A real adapter/kernel refusal is permanent for this query.
            // Only a cheap sparse-frontier refusal schedules another check.
            next_anchor_check = usize::MAX;
            match anchor_planes::reverse_frontier_is_dense(template, &scratch.queue, queue_cursor) {
                Some(false) => next_anchor_check = queue_cursor.saturating_mul(2),
                Some(true) => {
                    if let Some(request) = request {
                        if let Some(ordered) = request.ordered(template) {
                            if let Some(result) = anchor_planes::finish_reverse_search(
                                template,
                                request.piece,
                                board,
                                &ordered,
                                &scratch.queue,
                                queue_cursor,
                            ) {
                                return result.search;
                            }
                        }
                    }
                }
                None => {}
            }
        }
        queue_cursor += 1;

        for &source in &template.reverse_translation_sources[current] {
            if source == INVALID_STATE_INDEX {
                continue;
            }
            push_index_if_placeable(
                template,
                board,
                source,
                &mut scratch.visited_generations,
                generation,
                &mut scratch.queue,
            );
        }

        let reverse_start = template.reverse_rotation_offsets[current] as usize;
        let reverse_end = template.reverse_rotation_offsets[current + 1] as usize;
        for source in &template.reverse_rotation_sources[reverse_start..reverse_end] {
            let index = usize::from(source.source);
            if scratch.visited_generations[index] == generation
                || template.state_masks[index] == INVALID_STATE_MASK
                || board & template.state_masks[index] != 0
                || !reverse_rotation_is_first_success_on_free_endpoints(template, board, *source)
            {
                continue;
            }
            scratch.visited_generations[index] = generation;
            scratch.queue.push(source.source);
        }
    }

    ReverseReachabilityResult {
        reachable: false,
        visited_state_count: queue_cursor,
    }
}

fn best_scoring_lock_evidence(
    template: &ReachabilityTemplate,
    board: u64,
    rotation: RotationState,
    x: i8,
    y: i8,
    scratch: &mut ReachabilityScratch,
) -> Option<ScoringLockEvidence> {
    preferred_scoring_lock_evidence::<false>(template, board, rotation, x, y, scratch)
}

fn preferred_scoring_lock_evidence<const LEGACY_T_SPIN: bool>(
    template: &ReachabilityTemplate,
    board: u64,
    rotation: RotationState,
    x: i8,
    y: i8,
    scratch: &mut ReachabilityScratch,
) -> Option<ScoringLockEvidence> {
    let target = State {
        rotation,
        x,
        y: i16::from(y),
    };
    let target_index = state_index(template.width, template.ceiling, target)?;
    if template.state_masks[target_index] == INVALID_STATE_MASK
        || board & template.state_masks[target_index] != 0
        || !grounded_index(template, board, target_index)
    {
        return None;
    }

    let start = template.reverse_rotation_offsets[target_index] as usize;
    let end = template.reverse_rotation_offsets[target_index + 1] as usize;
    let vertical_shadow = downward_occupancy_shadow(board, template.width);
    let mut best = None::<((bool, bool, u32), ScoringLockEvidence)>;
    for source in &template.reverse_rotation_sources[start..end] {
        let source_index = usize::from(source.source);
        if template.state_masks[source_index] == INVALID_STATE_MASK
            || board & template.state_masks[source_index] != 0
            || !reverse_rotation_is_first_success_on_free_endpoints(template, board, *source)
        {
            continue;
        }
        let transition = source_index * 3 + usize::from(source.slot);
        let target_offset = template.rotation_target_offsets[transition] as usize
            + source.prior_target_count as usize;
        let kick_index = template.rotation_kick_indices[target_offset];
        let is_quarter_turn = source.slot < 2;
        let rank = if LEGACY_T_SPIN {
            // Native highest-T-spin trace first tries a final kick at index
            // four, then any reachable final rotation. Its DFS tie order
            // within either tier is not part of this evidence contract.
            (kick_index == 4, false, 0)
        } else {
            (
                is_quarter_turn && kick_index == 4,
                is_quarter_turn,
                kick_index,
            )
        };
        // The incumbent was already proved reachable. Only a strictly
        // better rank can replace it; keeping iteration order preserves the
        // first reachable predecessor on a tie without another reverse BFS.
        if best.as_ref().is_some_and(|(current, _)| rank <= *current)
            || (template.state_masks[source_index] & vertical_shadow != 0
                && !reverse_lock_reachable_large(template, board, source_index, scratch).reachable)
        {
            continue;
        }
        let predecessor = state_from_index(template.width, template.ceiling, source_index);
        let request = match source.slot {
            0 => RotationRequest::Clockwise,
            1 => RotationRequest::CounterClockwise,
            _ => RotationRequest::HalfTurn,
        };
        let evidence = ScoringLockEvidence::rotation(
            predecessor.rotation,
            request,
            u8::try_from(kick_index).ok()?,
            target.x - predecessor.x,
            i8::try_from(target.y - predecessor.y).ok()?,
            predecessor.x,
            i8::try_from(predecessor.y).ok()?,
        );
        best = Some((rank, evidence));
    }
    best.map(|(_, evidence)| evidence)
}

/// Query-local exact movement authority used by operation-document analysis.
pub(crate) struct DocumentReachabilityEngine {
    width: u8,
    height: u8,
    profile_id: KickTableProfileId,
    templates: [Option<ReachabilityTemplate>; 7],
    scratch: ReachabilityScratch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DocumentLockReachability {
    pub valid_target: bool,
    pub reachable: bool,
    pub lock_mask: u64,
    pub visited_state_count: usize,
    pub first_success_kick_evidence: bool,
}

impl DocumentReachabilityEngine {
    pub(crate) fn new(width: u8, height: u8, profile_id: KickTableProfileId) -> Option<Self> {
        builtin_kick_profile(profile_id)?;
        if width == 0 || height == 0 || usize::from(width) * usize::from(height) > 64 {
            return None;
        }
        Some(Self {
            width,
            height,
            profile_id,
            templates: std::array::from_fn(|_| None),
            scratch: ReachabilityScratch::default(),
        })
    }

    pub(crate) fn analyze_lock(
        &mut self,
        board: u64,
        piece: PieceKind,
        rotation: RotationState,
        x: i8,
        y: i8,
    ) -> DocumentLockReachability {
        self.analyze_lock_with_evidence::<true>(board, piece, rotation, x, y)
    }

    pub(super) fn check_lock(
        &mut self,
        board: u64,
        piece: PieceKind,
        rotation: RotationState,
        x: i8,
        y: i8,
    ) -> DocumentLockReachability {
        self.analyze_lock_with_evidence::<false>(board, piece, rotation, x, y)
    }

    pub(super) fn preferred_t_spin_rotation(
        &mut self,
        board: u64,
        rotation: RotationState,
        x: i8,
        y: i8,
    ) -> Option<ScoringLockEvidence> {
        let template = self.templates[piece_index(PieceKind::T)].get_or_insert_with(|| {
            ReachabilityTemplate::compile(self.width, self.height, PieceKind::T, self.profile_id)
        });
        preferred_scoring_lock_evidence::<true>(template, board, rotation, x, y, &mut self.scratch)
            .map(|evidence| {
                evidence.with_immobile_before_clear(scoring_lock_is_immobile(
                    template, board, rotation, x, y,
                ))
            })
    }

    fn analyze_lock_with_evidence<const CAPTURE_KICK: bool>(
        &mut self,
        board: u64,
        piece: PieceKind,
        rotation: RotationState,
        x: i8,
        y: i8,
    ) -> DocumentLockReachability {
        let piece_slot = piece_index(piece);
        let template = self.templates[piece_slot].get_or_insert_with(|| {
            ReachabilityTemplate::compile(self.width, self.height, piece, self.profile_id)
        });
        let Some(target_index) = state_index(
            template.width,
            template.ceiling,
            State {
                rotation,
                x,
                y: i16::from(y),
            },
        ) else {
            return DocumentLockReachability {
                valid_target: false,
                reachable: false,
                lock_mask: 0,
                visited_state_count: 0,
                first_success_kick_evidence: false,
            };
        };
        let lock_mask = template.state_masks[target_index];
        if lock_mask == INVALID_STATE_MASK
            || board & lock_mask != 0
            || !grounded_index(template, board, target_index)
        {
            return DocumentLockReachability {
                valid_target: lock_mask != INVALID_STATE_MASK,
                reachable: false,
                lock_mask: lock_mask.min(INVALID_STATE_MASK - 1),
                visited_state_count: 0,
                first_success_kick_evidence: false,
            };
        }
        let result = reverse_lock_reachable(template, board, rotation, x, y, &mut self.scratch);
        let kick = CAPTURE_KICK
            && result.reachable
            && best_scoring_lock_evidence(template, board, rotation, x, y, &mut self.scratch)
                .is_some();
        DocumentLockReachability {
            valid_target: true,
            reachable: result.reachable,
            lock_mask,
            visited_state_count: result.visited_state_count,
            first_success_kick_evidence: kick,
        }
    }
}

fn scoring_lock_is_immobile(
    template: &ReachabilityTemplate,
    board: u64,
    rotation: RotationState,
    x: i8,
    y: i8,
) -> bool {
    let Some(target_index) = state_index(
        template.width,
        template.ceiling,
        State {
            rotation,
            x,
            y: i16::from(y),
        },
    ) else {
        return false;
    };
    let translations_blocked = template.translation_targets[target_index]
        .iter()
        .all(|target| {
            *target == INVALID_STATE_INDEX
                || board & template.state_masks[usize::from(*target)] != 0
        });
    let upward_blocked = state_index(
        template.width,
        template.ceiling,
        State {
            rotation,
            x,
            y: i16::from(y) + 1,
        },
    )
    .is_none_or(|target| {
        template.state_masks[target] == INVALID_STATE_MASK
            || board & template.state_masks[target] != 0
    });
    translations_blocked && upward_blocked
}

fn reverse_rotation_is_first_success(
    template: &ReachabilityTemplate,
    board: u64,
    source: ReverseRotationSource,
) -> bool {
    reverse_rotation_prior_targets(template, source)
        .iter()
        .all(|target| board & template.state_masks[usize::from(*target)] != 0)
}

const ORDERED_GUARD: u8 = 0;
const NO_PRIOR_GUARD: u8 = 1;
const SINGLE_BIT_GUARD_BASE: u8 = 2;
const SINGLE_MASK_GUARD_BASE: u8 = SINGLE_BIT_GUARD_BASE + u64::BITS as u8;

/// A prior clause can replace the conjunction exactly when it is contained
/// in every other clause after removing the already-free endpoints. A mere
/// nonempty intersection is insufficient: {a,b} and {a,c} may be blocked by
/// b and c while a stays free. Long imported tables retain their full order.
fn compile_free_endpoint_guard(
    state_masks: &[u64],
    source_mask: u64,
    target_mask: u64,
    prior_targets: &[u16],
) -> u8 {
    if prior_targets.is_empty() {
        return NO_PRIOR_GUARD;
    }
    let free = source_mask | target_mask;
    let intersection = prior_targets.iter().fold(!free, |mask, &target| {
        mask & state_masks[usize::from(target)]
    });
    let Some(index) = prior_targets
        .iter()
        .position(|&target| state_masks[usize::from(target)] & !free == intersection)
    else {
        return ORDERED_GUARD;
    };
    if intersection.count_ones() == 1 {
        return SINGLE_BIT_GUARD_BASE + intersection.trailing_zeros() as u8;
    }
    u8::try_from(index)
        .ok()
        .and_then(|index| SINGLE_MASK_GUARD_BASE.checked_add(index))
        .unwrap_or(ORDERED_GUARD)
}

fn reverse_rotation_is_first_success_on_free_endpoints(
    template: &ReachabilityTemplate,
    board: u64,
    source: ReverseRotationSource,
) -> bool {
    match source.free_endpoint_guard {
        ORDERED_GUARD => reverse_rotation_is_first_success(template, board, source),
        NO_PRIOR_GUARD => true,
        bit if bit < SINGLE_MASK_GUARD_BASE => {
            board & (1_u64 << (bit - SINGLE_BIT_GUARD_BASE)) != 0
        }
        mask => {
            let prior = reverse_rotation_prior_targets(template, source);
            let target = prior[usize::from(mask - SINGLE_MASK_GUARD_BASE)];
            board & template.state_masks[usize::from(target)] != 0
        }
    }
}

/// A positive vertical-entry certificate for this template's all-column,
/// all-orientation sky seeds. A blocked shadow is Unknown, never a negative
/// movement proof. Doubling shifts share one board transform across all
/// predecessors of the same scoring query without retaining a heap table.
fn downward_occupancy_shadow(mut board: u64, width: u8) -> u64 {
    if width == 0 {
        return u64::MAX;
    }
    let mut shift = u32::from(width);
    while shift < u64::BITS {
        board |= board >> shift;
        shift *= 2;
    }
    board
}

fn reverse_rotation_prior_targets(
    template: &ReachabilityTemplate,
    source: ReverseRotationSource,
) -> &[u16] {
    let transition = usize::from(source.source) * 3 + usize::from(source.slot);
    let prior_start = template.rotation_target_offsets[transition] as usize;
    let prior_end = prior_start + source.prior_target_count as usize;
    &template.rotation_targets[prior_start..prior_end]
}

const INVALID_STATE_INDEX: u16 = u16::MAX;

fn push_index_if_placeable(
    template: &ReachabilityTemplate,
    board: u64,
    state_index: u16,
    visited_generations: &mut [u16],
    generation: u16,
    queue: &mut Vec<u16>,
) {
    let index = usize::from(state_index);
    if template.state_masks[index] == INVALID_STATE_MASK || board & template.state_masks[index] != 0
    {
        return;
    }
    if visited_generations[index] != generation {
        visited_generations[index] = generation;
        queue.push(state_index);
    }
}

fn state_index(width: u8, ceiling: i16, state: State) -> Option<usize> {
    if state.x < 0 || state.y < 0 || state.x >= width as i8 || state.y > ceiling {
        return None;
    }
    Some(
        (state.rotation.quarter_turns() as usize * (ceiling as usize + 1) + state.y as usize)
            * width as usize
            + state.x as usize,
    )
}

fn grounded_index(template: &ReachabilityTemplate, board: u64, state_index: usize) -> bool {
    let state = state_from_index(template.width, template.ceiling, state_index);
    if state.y == 0 {
        return true;
    }
    let down = template.translation_targets[state_index][0];
    down == INVALID_STATE_INDEX || board & template.state_masks[usize::from(down)] != 0
}

fn first_successful_kick(
    template: &ReachabilityTemplate,
    board: u64,
    source: usize,
    transition_slot: usize,
) -> Option<u16> {
    let transition = source * 3 + transition_slot;
    let start = template.rotation_target_offsets[transition] as usize;
    let end = template.rotation_target_offsets[transition + 1] as usize;
    template.rotation_targets[start..end]
        .iter()
        .find(|&&target| board & template.state_masks[usize::from(target)] == 0)
        .copied()
}

fn compile_translation_targets_and_seeds(
    width: u8,
    height: u8,
    ceiling: i16,
    state_masks: &[u64],
) -> (Vec<[u16; 3]>, Vec<u16>) {
    let mut translations = Vec::with_capacity(state_masks.len());
    let mut seeds = Vec::new();
    for source in 0..state_masks.len() {
        let state = state_from_index(width, ceiling, source);
        let candidates = [
            State {
                y: state.y - 1,
                ..state
            },
            State {
                x: state.x - 1,
                ..state
            },
            State {
                x: state.x + 1,
                ..state
            },
        ];
        let mut targets = [INVALID_STATE_INDEX; 3];
        for (slot, candidate) in candidates.into_iter().enumerate() {
            let Some(target) = state_index(width, ceiling, candidate) else {
                continue;
            };
            if state_masks[target] == INVALID_STATE_MASK {
                continue;
            }
            targets[slot] = u16::try_from(target).unwrap_or(INVALID_STATE_INDEX);
        }
        translations.push(targets);
        if state.y >= i16::from(height) && state_masks[source] != INVALID_STATE_MASK {
            if let Ok(source) = u16::try_from(source) {
                seeds.push(source);
            }
        }
    }
    (translations, seeds)
}

#[inline]
pub(super) fn harddrop_path_mask(lock_mask: u64, width: u8) -> u64 {
    let shift = u32::from(width);
    let mut path = lock_mask;
    path |= path << shift;
    path |= path << (shift * 2);
    if shift * 4 < u64::BITS {
        path |= path << (shift * 4);
    }
    path
}

fn compile_reverse_translation_sources(forward: &[[u16; 3]]) -> Vec<[u16; 3]> {
    let mut reverse = vec![[INVALID_STATE_INDEX; 3]; forward.len()];
    for (source, targets) in forward.iter().enumerate() {
        let Ok(source) = u16::try_from(source) else {
            continue;
        };
        for (slot, &target) in targets.iter().enumerate() {
            if target != INVALID_STATE_INDEX {
                reverse[usize::from(target)][slot] = source;
            }
        }
    }
    reverse
}

fn compile_rotation_targets(
    width: u8,
    ceiling: i16,
    state_masks: &[u64],
    kick_deltas: &[&[(i16, i16)]; 12],
) -> (Vec<u32>, Vec<u16>, Vec<u32>) {
    let mut offsets = Vec::with_capacity(state_masks.len() * 3 + 1);
    let mut targets = Vec::new();
    let mut kick_indices = Vec::new();
    offsets.push(0);
    for source in 0..state_masks.len() {
        let state = state_from_index(width, ceiling, source);
        let rotations = [
            state.rotation.clockwise(),
            state.rotation.counter_clockwise(),
            state.rotation.rotated_180(),
        ];
        for (slot, to) in rotations.into_iter().enumerate() {
            let transition = rotation_transition_index(state.rotation, slot);
            for (kick_index, &(dx, dy)) in kick_deltas[transition].iter().enumerate() {
                let column = i16::from(state.x) + dx;
                if !(0..i16::from(width)).contains(&column) {
                    continue;
                }
                let candidate = State {
                    rotation: to,
                    x: column as i8,
                    y: state.y + dy,
                };
                let Some(target) = state_index(width, ceiling, candidate) else {
                    continue;
                };
                if state_masks[target] == INVALID_STATE_MASK {
                    continue;
                }
                let Ok(target) = u16::try_from(target) else {
                    continue;
                };
                targets.push(target);
                kick_indices.push(u32::try_from(kick_index).expect("admitted kick ordinal"));
            }
            offsets.push(targets.len() as u32);
        }
    }
    (offsets, targets, kick_indices)
}

fn compile_seed_words(state_count: usize, seeds: &[u16]) -> Vec<u64> {
    let mut words = vec![0_u64; state_count.div_ceil(64)];
    for &seed in seeds {
        let index = usize::from(seed);
        words[index / 64] |= 1_u64 << (index % 64);
    }
    words
}

fn compile_reverse_rotation_sources(
    state_masks: &[u64],
    forward_offsets: &[u32],
    forward_targets: &[u16],
    mode: GlobalKickPruneMode,
) -> (
    Vec<u32>,
    Vec<ReverseRotationSource>,
    GlobalKickPruneCompileWork,
) {
    let state_count = state_masks.len();
    // An earlier candidate contained in the required-free source/destination
    // union must succeed whenever this candidate could. Such a later edge is
    // impossible on every board. Keep the original candidate ordinal and
    // forward prefix: a removed edge may still have a free target that blocks
    // another edge. This compilation never runs in the query hot loop.
    // Count and fill see immutable masks and ordered prefixes. A retained bit
    // therefore carries the exact count-pass decision into fill; it cannot
    // alter edge identity, order or the original first-success guard prefix.
    // This transient bitmap is released after CSR compilation.
    let mut retained_bits = if mode == GlobalKickPruneMode::Improved {
        vec![0_u64; forward_targets.len().div_ceil(64)]
    } else {
        Vec::new()
    };
    #[allow(unused_mut)]
    let mut work = GlobalKickPruneCompileWork::default();
    #[cfg(any(test, feature = "local-search-ab"))]
    {
        work.keep_bitmap_peak_bytes = retained_bits.capacity() * core::mem::size_of::<u64>();
    }
    #[allow(unused_mut)]
    let mut retained = |transition: usize, candidate: usize| {
        if mode == GlobalKickPruneMode::Off {
            return true;
        }
        #[cfg(any(test, feature = "local-search-ab"))]
        {
            work.predicate_calls += 1;
        }
        edge_possible_in_cube(
            state_masks[transition / 3],
            state_masks[usize::from(forward_targets[candidate])],
            forward_targets[forward_offsets[transition] as usize..candidate]
                .iter()
                .map(|&prior| {
                    #[cfg(any(test, feature = "local-search-ab"))]
                    {
                        work.prior_mask_checks += 1;
                    }
                    state_masks[usize::from(prior)]
                }),
            0,
            u64::MAX,
        )
    };
    let mut counts = vec![0_u32; state_count + 1];
    for transition in 0..state_count * 3 {
        let start = forward_offsets[transition] as usize;
        let end = forward_offsets[transition + 1] as usize;
        for candidate in start..end {
            if retained(transition, candidate) {
                let target = usize::from(forward_targets[candidate]);
                counts[target + 1] = counts[target + 1].saturating_add(1);
                if mode == GlobalKickPruneMode::Improved {
                    retained_bits[candidate / 64] |= 1_u64 << (candidate % 64);
                }
            }
        }
    }
    for index in 1..counts.len() {
        counts[index] = counts[index].saturating_add(counts[index - 1]);
    }
    let offsets = counts.clone();
    let mut cursors = counts;
    let mut sources = vec![
        ReverseRotationSource {
            source: 0,
            slot: 0,
            free_endpoint_guard: ORDERED_GUARD,
            prior_target_count: 0,
        };
        *cursors.last().expect("one count per pose and sentinel") as usize
    ];
    for transition in 0..state_count * 3 {
        let start = forward_offsets[transition] as usize;
        let end = forward_offsets[transition + 1] as usize;
        let source = u16::try_from(transition / 3).expect("admitted template pose index");
        let slot = (transition % 3) as u8;
        for (prior_target_count, &target) in forward_targets[start..end].iter().enumerate() {
            let candidate = start + prior_target_count;
            let keep = match mode {
                GlobalKickPruneMode::Off => true,
                GlobalKickPruneMode::Current => retained(transition, candidate),
                GlobalKickPruneMode::Improved => {
                    retained_bits[candidate / 64] & (1_u64 << (candidate % 64)) != 0
                }
            };
            if !keep {
                continue;
            }
            let prior_target_count = u32::try_from(prior_target_count)
                .expect("admitted complete ordered candidate prefix");
            let target = usize::from(target);
            let cursor = cursors[target] as usize;
            sources[cursor] = ReverseRotationSource {
                source,
                slot,
                free_endpoint_guard: compile_free_endpoint_guard(
                    state_masks,
                    state_masks[usize::from(source)],
                    state_masks[target],
                    &forward_targets[start..candidate],
                ),
                prior_target_count,
            };
            cursors[target] += 1;
        }
    }
    (offsets, sources, work)
}

/// Exact edge-existence test over `fixed | subset(variable)`.
///
/// The caller validates the cube and its board frame. Source and destination
/// must stay empty. Every earlier valid kick needs a possible blocker outside
/// those required-empty cells. The single board `fixed | (variable & !empty)`
/// witnesses all those blockers simultaneously. This tests edge existence,
/// never source reachability or independence of correlated operation choices.
fn edge_possible_in_cube(
    source_mask: u64,
    target_mask: u64,
    prior_masks: impl IntoIterator<Item = u64>,
    fixed: u64,
    variable: u64,
) -> bool {
    if source_mask == INVALID_STATE_MASK || target_mask == INVALID_STATE_MASK {
        return false;
    }
    let required_empty = source_mask | target_mask;
    fixed & required_empty == 0
        && prior_masks.into_iter().all(|mask| {
            mask == INVALID_STATE_MASK
                || mask & fixed != 0
                || mask & variable & !required_empty != 0
        })
}

#[cfg(test)]
thread_local! {
    static GLOBAL_KICK_PRUNING_OVERRIDE: std::cell::Cell<Option<GlobalKickPruneMode>> = const {
        std::cell::Cell::new(None)
    };
}

/// Affects templates newly compiled on this test thread, not existing owners
/// or spawned workers. Native multiworker A/B uses the process-wide axis.
#[cfg(test)]
pub(super) fn with_global_kick_pruning<T>(enabled: bool, action: impl FnOnce() -> T) -> T {
    with_global_kick_prune_mode(
        if enabled {
            GlobalKickPruneMode::Current
        } else {
            GlobalKickPruneMode::Off
        },
        action,
    )
}

#[cfg(test)]
fn with_global_kick_prune_mode<T>(mode: GlobalKickPruneMode, action: impl FnOnce() -> T) -> T {
    struct Restore(Option<GlobalKickPruneMode>);
    impl Drop for Restore {
        fn drop(&mut self) {
            GLOBAL_KICK_PRUNING_OVERRIDE.with(|mode| mode.set(self.0));
        }
    }
    let _restore = Restore(GLOBAL_KICK_PRUNING_OVERRIDE.with(|slot| slot.replace(Some(mode))));
    action()
}

fn global_kick_pruning_mode() -> GlobalKickPruneMode {
    #[cfg(test)]
    if let Some(mode) = GLOBAL_KICK_PRUNING_OVERRIDE.with(std::cell::Cell::get) {
        return mode;
    }
    #[cfg(all(feature = "local-search-ab", not(target_family = "wasm")))]
    {
        // Diagnostic A/B only. Product builds have no configurable bypass;
        // the benchmark entrypoint validates this process-wide 0/1/2 axis.
        static MODE: OnceLock<GlobalKickPruneMode> = OnceLock::new();
        *MODE.get_or_init(
            || match std::env::var("CLEARRA_STRICT_GLOBAL_KICK_PRUNE").as_deref() {
                Ok("0") => GlobalKickPruneMode::Off,
                Ok("1") => GlobalKickPruneMode::Current,
                _ => GlobalKickPruneMode::Improved,
            },
        )
    }
    #[cfg(not(all(feature = "local-search-ab", not(target_family = "wasm"))))]
    GlobalKickPruneMode::Improved
}

fn state_from_index(width: u8, ceiling: i16, index: usize) -> State {
    let rotation_stride = (ceiling as usize + 1) * width as usize;
    let rotation = RotationState::from_quarter_turns((index / rotation_stride) as u8)
        .expect("compiled reachability rotation is valid");
    let remainder = index % rotation_stride;
    State {
        rotation,
        x: (remainder % width as usize) as i8,
        y: (remainder / width as usize) as i16,
    }
}

fn rotation_transition_index(from: RotationState, transition_slot: usize) -> usize {
    from.quarter_turns() as usize * 3 + transition_slot
}

fn source_ceiling(
    height: u8,
    piece: PieceKind,
    allow_180: bool,
    profile: &KickTableProfile,
) -> i16 {
    let mut ordered: [Vec<(i16, i16)>; 12] = std::array::from_fn(|_| Vec::new());
    for from in RotationState::ALL {
        for (slot, to) in [
            from.clockwise(),
            from.counter_clockwise(),
            from.rotated_180(),
        ]
        .into_iter()
        .enumerate()
        {
            if slot == 2 && !allow_180 {
                continue;
            }
            if let Some(sequence) = profile.sequence_for(KickTransition::new(piece, from, to)) {
                ordered[rotation_transition_index(from, slot)].extend(
                    sequence.offsets().iter().map(|offset| {
                        normalized_kick_delta_wide(piece, from, to, offset.dx(), offset.dy())
                    }),
                );
            }
        }
    }
    KickSkyBounds::for_ordered_deltas(ordered.each_ref().map(Vec::as_slice)).source_ceiling(height)
}

#[cfg(test)]
mod tests {
    use super::{
        best_scoring_lock_evidence, builtin_kick_profile, normalized_kick_delta,
        reachability_cache_hash, reachability_cache_set_start, reverse_lock_reachable,
        state_from_index, state_index, ReachabilityCache, ReachabilityCacheLookup,
        ReachabilityCachePayload, ReachabilityObservation, ReachabilityScratch,
        ReachabilityTemplate, ReachabilityWorkspace, ReachableLocks, State,
        LARGE_SEARCH_EXHAUSTIVE_OBSERVATIONS, REACHABILITY_CACHE_CAPACITY, REACHABILITY_CACHE_WAYS,
        REACHABILITY_PAYLOADS_PER_CHUNK,
    };
    use crate::conditioned_local_index::LocalRelationCandidateLookup;
    use crate::conditioned_local_pack::{
        built_in_local_relation_binding, encode_local_relation_candidate_pack,
        load_local_relation_candidate_pack,
    };
    use crate::conditioned_local_product::qualified_local_relation_for_solver_test;
    use crate::conditioned_local_relation::{
        derive_exact_conditioned_local_relation, ConditionedPoseWindow, LocalRelationRowFrame,
    };
    use crate::conditioned_reachability::ConditionedReachabilityEntryPose;
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_rules::kicks::{KickTableProfileId, KickTransition};

    #[cfg(feature = "placement-dependency-pc")]
    #[test]
    fn requested_lock_family_reuses_cache_and_preserves_imported_ordered_authority() {
        use crate::backend::wasm_cpu::{
            catalog::GeometryCatalog, compact_kick_authority::CompactKickAuthority,
        };
        use clearra_core_domain::execution_cancellation::ExecutionControl;
        use clearra_rules::{
            kicks::{
                KickOffsetSequence, KickTableEntry, KickTableProfile, SrsKicks,
                VerifiedKickTableProfile,
            },
            profile::rule_profile::RuleProfileId,
        };

        const INITIAL: u64 = 0x000f_83c7;
        let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
            10,
            2,
            INITIAL,
            0x4038 | 0x3c00,
        )
        .unwrap();
        let canonical = VerifiedKickTableProfile::try_new(SrsKicks::profile()).unwrap();
        let base = canonical.profile();
        let transition =
            KickTransition::new(PieceKind::T, RotationState::Right, RotationState::Zero);
        let mut offsets = base.sequence_for(transition).unwrap().offsets().to_vec();
        offsets[..=2].rotate_right(1);
        let mut entries = base.entries().to_vec();
        *entries
            .iter_mut()
            .find(|entry| entry.transition() == transition)
            .unwrap() = KickTableEntry::new(transition, KickOffsetSequence::new(offsets));
        let changed = VerifiedKickTableProfile::try_new(KickTableProfile::new(
            base.id(),
            base.source_rule(),
            entries,
        ))
        .unwrap();
        assert_eq!(canonical.profile().id(), changed.profile().id());
        let control = ExecutionControl::default();
        let mut desired = ReachableLocks::default();
        desired.insert(10, RotationState::Zero, 3, 0);
        for (profile, expected) in [(&canonical, false), (&changed, true)] {
            let authority =
                CompactKickAuthority::admit_profile(10, 2, RuleProfileId::Srs, profile).unwrap();
            let mut workspace = ReachabilityWorkspace::default();
            workspace.bind_local_import(authority, &catalog).unwrap();
            let first = workspace
                .reachable_locks_for_board(&catalog, INITIAL, PieceKind::T, desired, &control)
                .unwrap();
            assert_eq!(
                first.locks.contains(10, RotationState::Zero, 3, 0),
                expected
            );
            assert!(first.visited_state_count > 0);
            assert!(expected || first.exhaustive);
            let generated = workspace.generated_state_count();
            let repeated = workspace
                .reachable_locks_for_board(&catalog, INITIAL, PieceKind::T, desired, &control)
                .unwrap();
            assert_eq!(
                repeated.locks.contains(10, RotationState::Zero, 3, 0),
                expected
            );
            assert_eq!(repeated.visited_state_count, 0);
            assert_eq!(workspace.generated_state_count(), generated);
            assert_eq!(workspace.metrics().lock_queries, 2);
            assert_eq!(
                workspace.metrics().cache_reachable_hits,
                usize::from(expected)
            );
            assert_eq!(
                workspace.metrics().cache_unreachable_hits,
                usize::from(!expected)
            );
        }
    }

    #[test]
    fn v081_shared_reachability_templates_and_sky_entries_have_one_request_owner() {
        use std::sync::{Arc, Barrier};
        let owner = Arc::new(super::SharedReachabilityTemplates::new(
            10,
            4,
            KickTableProfileId::SrsPlus,
        ));
        let empty_bytes = owner.retained_bytes();
        let barrier = Barrier::new(11);
        let workers = std::thread::scope(|scope| {
            let handles = (0..11)
                .map(|_| {
                    let owner = Arc::clone(&owner);
                    let barrier = &barrier;
                    scope.spawn(move || {
                        barrier.wait();
                        let template = owner.template(PieceKind::T);
                        template.sky_entry_poses.get_or_init(|| {
                            super::canonical_sky_entry_poses(
                                10,
                                template.ceiling,
                                &template.sky_seeds,
                            )
                        });
                        template
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(
            owner
                .templates
                .iter()
                .filter(|slot| slot.get().is_some())
                .count(),
            1
        );
        assert!(owner.retained_bytes() > empty_bytes);
        for worker in &workers {
            assert!(Arc::ptr_eq(worker, &workers[0]));
            assert_eq!(
                worker.sky_entry_poses.get().unwrap().as_ptr(),
                workers[0].sky_entry_poses.get().unwrap().as_ptr()
            );
        }
        assert_eq!(
            owner.retained_bytes(),
            empty_bytes
                + core::mem::size_of::<ReachabilityTemplate>()
                + 2 * core::mem::size_of::<usize>()
                + workers[0].retained_bytes()
        );
        let next_request =
            super::SharedReachabilityTemplates::new(10, 4, KickTableProfileId::SrsPlus);
        assert!(!Arc::ptr_eq(
            &next_request.template(PieceKind::T),
            &workers[0]
        ));
    }

    #[test]
    fn v081_shared_reachability_keeps_mutable_cache_and_scratch_private() {
        use std::sync::Arc;
        let owner = Arc::new(super::SharedReachabilityTemplates::new(
            10,
            4,
            KickTableProfileId::SrsPlus,
        ));
        let catalog =
            super::GeometryCatalog::compile_for_required_cells_on_dimensions(10, 4, 0, 0).unwrap();
        let mut first = ReachabilityWorkspace::default();
        let mut second = ReachabilityWorkspace::default();
        first.set_shared_templates(Arc::clone(&owner)).unwrap();
        second.set_shared_templates(Arc::clone(&owner)).unwrap();
        first.prepare_template(&catalog, PieceKind::T);
        second.prepare_template(&catalog, PieceKind::T);
        assert_eq!(first.private_retained_bytes(), 0);
        assert_eq!(second.private_retained_bytes(), 0);
        first.lock_reachable_after_harddrop_miss_in_frame(
            &catalog,
            0,
            PieceKind::T,
            RotationState::Zero,
            4,
            0,
            None,
        );
        assert!(first.private_retained_bytes() > 0);
        assert_eq!(second.private_retained_bytes(), 0);
        assert!(second.cache.keys.is_empty());
        assert!(second.scratch.visited_generations.is_empty());
        let first_private_bytes = first.private_retained_bytes();
        let template = owner.template(PieceKind::T);
        template.sky_entry_poses.get_or_init(|| {
            super::canonical_sky_entry_poses(10, template.ceiling, &template.sky_seeds)
        });
        assert_eq!(first.private_retained_bytes(), first_private_bytes);
        assert_eq!(
            first.retained_bytes(),
            first_private_bytes + template.retained_bytes()
        );
        assert_eq!(second.retained_bytes(), template.retained_bytes());
        second.lock_reachable_after_harddrop_miss_in_frame(
            &catalog,
            0,
            PieceKind::T,
            RotationState::Zero,
            4,
            0,
            None,
        );
        assert_ne!(
            first.cache.observations.as_ptr(),
            second.cache.observations.as_ptr()
        );
        assert_ne!(
            first.scratch.visited_generations.as_ptr(),
            second.scratch.visited_generations.as_ptr()
        );
    }

    #[test]
    fn reachability_cache_allocates_lock_evidence_only_when_admitted() {
        let mut cache = ReachabilityCache::default();
        let lookup = |cache: &mut ReachabilityCache, x| {
            cache.query(0, PieceKind::T, 10, RotationState::Zero, x, 0)
        };
        let miss = |admit| ReachabilityCacheLookup::Search {
            exhaustive: false,
            admit,
            key_present: false,
        };
        assert_eq!(cache.retained_bytes(), 0);
        assert_eq!(lookup(&mut cache, 4), miss(false));
        assert!(cache.keys.is_empty());
        assert!(cache.payloads.is_empty());
        assert_eq!(
            cache.retained_bytes(),
            REACHABILITY_CACHE_CAPACITY * core::mem::size_of::<ReachabilityObservation>(),
        );

        let mut positive = ReachableLocks::default();
        positive.insert(10, RotationState::Zero, 4, 0);
        let mut negative = ReachableLocks::default();
        negative.insert(10, RotationState::Zero, 5, 0);
        cache.insert(0, PieceKind::T, positive, negative, false, false);
        assert!(cache.keys.is_empty());
        assert_eq!(lookup(&mut cache, 4), miss(true));
        cache.insert(0, PieceKind::T, positive, negative, false, true);
        assert_eq!(cache.payload_count, 1);
        assert_eq!(cache.payloads.len(), 1);
        assert_eq!(
            cache.payload_retained_bytes,
            REACHABILITY_PAYLOADS_PER_CHUNK * core::mem::size_of::<ReachabilityCachePayload>(),
        );
        assert_eq!(lookup(&mut cache, 4), ReachabilityCacheLookup::Reachable);
        assert_eq!(
            lookup(&mut cache, 5),
            ReachabilityCacheLookup::ExhaustivelyUnreachable,
        );
        assert_eq!(
            lookup(&mut cache, 6),
            ReachabilityCacheLookup::Search {
                exhaustive: false,
                admit: true,
                key_present: true,
            },
        );

        let mut second = ReachableLocks::default();
        second.insert(10, RotationState::Zero, 7, 0);
        cache.insert(
            0,
            PieceKind::T,
            second,
            ReachableLocks::default(),
            true,
            true,
        );
        assert_eq!(cache.payload_count, 1);
        assert_eq!(lookup(&mut cache, 4), ReachabilityCacheLookup::Reachable);
        assert_eq!(lookup(&mut cache, 7), ReachabilityCacheLookup::Reachable);
        assert_eq!(
            lookup(&mut cache, 6),
            ReachabilityCacheLookup::ExhaustivelyUnreachable,
        );
    }

    #[test]
    fn reachability_cache_payload_chunks_keep_earlier_slots_live() {
        let mut cache = ReachabilityCache::default();
        let piece = PieceKind::T;
        let piece_code = super::piece_index(piece) as u8 + 1;
        let mut sets = std::collections::HashSet::new();
        let boards: Vec<_> = (0..4096)
            .filter(|&board| {
                sets.insert(reachability_cache_set_start(
                    reachability_cache_hash(board, piece_code),
                    REACHABILITY_CACHE_CAPACITY,
                ))
            })
            .take(REACHABILITY_PAYLOADS_PER_CHUNK + 1)
            .collect();
        assert_eq!(boards.len(), REACHABILITY_PAYLOADS_PER_CHUNK + 1);
        let mut positive = ReachableLocks::default();
        positive.insert(10, RotationState::Zero, 4, 0);
        for &board in &boards[..REACHABILITY_PAYLOADS_PER_CHUNK] {
            cache.insert(
                board,
                piece,
                positive,
                ReachableLocks::default(),
                true,
                true,
            );
        }
        let first_payload = cache.payloads[0].as_ptr();
        assert_eq!(cache.payloads.len(), 1);
        let new_board = boards[REACHABILITY_PAYLOADS_PER_CHUNK];
        cache.insert(
            new_board,
            piece,
            positive,
            ReachableLocks::default(),
            true,
            true,
        );
        assert_eq!(cache.payloads.len(), 2);
        assert_eq!(cache.payloads[0].as_ptr(), first_payload);
        assert_eq!(cache.payload_count, boards.len());
        for board in boards {
            assert_eq!(
                cache.query(board, piece, 10, RotationState::Zero, 4, 0),
                ReachabilityCacheLookup::Reachable,
            );
        }
    }

    #[test]
    fn reachability_cache_replacement_reuses_slot_without_stale_evidence() {
        let mut cache = ReachabilityCache::default();
        let piece = PieceKind::T;
        let piece_code = super::piece_index(piece) as u8 + 1;
        let set_for = |board| {
            reachability_cache_set_start(
                reachability_cache_hash(board, piece_code),
                REACHABILITY_CACHE_CAPACITY,
            )
        };
        let set_start = set_for(0);
        let boards: Vec<_> = (0..(1 << 20))
            .filter(|&board| set_for(board) == set_start)
            .take(REACHABILITY_CACHE_WAYS + 1)
            .collect();
        assert_eq!(boards.len(), REACHABILITY_CACHE_WAYS + 1);
        let mut positive = ReachableLocks::default();
        positive.insert(10, RotationState::Zero, 4, 0);
        let mut negative = ReachableLocks::default();
        negative.insert(10, RotationState::Zero, 5, 0);
        for &board in &boards[..REACHABILITY_CACHE_WAYS] {
            cache.insert(board, piece, positive, negative, false, true);
        }
        let payload_index = cache.keys[set_start].payload_index;
        let bytes = cache.retained_bytes();
        cache.allocation_disabled = true;
        let new_board = boards[REACHABILITY_CACHE_WAYS];
        let mut replacement = ReachableLocks::default();
        replacement.insert(10, RotationState::Zero, 6, 0);
        cache.insert(
            new_board,
            piece,
            replacement,
            ReachableLocks::default(),
            false,
            true,
        );
        assert_eq!(cache.keys[set_start].board, new_board);
        assert_eq!(cache.keys[set_start].payload_index, payload_index);
        assert_eq!(cache.payload_count, REACHABILITY_CACHE_WAYS);
        assert_eq!(cache.retained_bytes(), bytes);
        assert_eq!(
            cache.query(new_board, piece, 10, RotationState::Zero, 6, 0),
            ReachabilityCacheLookup::Reachable,
        );
        for x in [4, 5] {
            assert_eq!(
                cache.query(new_board, piece, 10, RotationState::Zero, x, 0),
                ReachabilityCacheLookup::Search {
                    exhaustive: false,
                    admit: true,
                    key_present: true,
                },
            );
        }
    }

    #[test]
    fn v081_shared_reachability_identity_mismatch_uses_exact_private_template() {
        use std::sync::Arc;
        let owner = Arc::new(super::SharedReachabilityTemplates::new(
            10,
            4,
            KickTableProfileId::SrsPlus,
        ));
        let mut workspace = ReachabilityWorkspace::default();
        workspace.set_shared_templates(Arc::clone(&owner)).unwrap();
        assert!(workspace.set_shared_templates(Arc::clone(&owner)).is_err());
        for (width, height, profile, shared) in [
            (10, 4, KickTableProfileId::SrsPlus, true),
            (10, 4, KickTableProfileId::SrsX, false),
            (10, 6, KickTableProfileId::SrsPlus, false),
            (8, 4, KickTableProfileId::SrsPlus, false),
            (10, 4, KickTableProfileId::SrsPlus, true),
        ] {
            workspace.configure_kick_profile(profile, false);
            let catalog = super::GeometryCatalog::compile_for_required_cells_on_dimensions(
                width, height, 0, 0,
            )
            .unwrap();
            workspace.prepare_template(&catalog, PieceKind::J);
            let actual = workspace.templates[super::piece_index(PieceKind::J)]
                .as_ref()
                .unwrap();
            assert_eq!(Arc::ptr_eq(actual, &owner.template(PieceKind::J)), shared);
            assert_eq!(actual.width, width);
            assert_eq!(actual.height, height);
            let reference = ReachabilityTemplate::compile(width, height, PieceKind::J, profile);
            assert_eq!(actual.allow_180, reference.allow_180);
            assert_eq!(actual.rotation_targets, reference.rotation_targets);
            assert_eq!(
                actual.reverse_rotation_sources,
                reference.reverse_rotation_sources
            );
            assert_eq!(
                workspace.private_retained_bytes(),
                if shared { 0 } else { actual.retained_bytes() }
            );
        }
        let mut used = ReachabilityWorkspace::default();
        let catalog =
            super::GeometryCatalog::compile_for_required_cells_on_dimensions(10, 4, 0, 0).unwrap();
        used.prepare_template(&catalog, PieceKind::T);
        assert!(used.set_shared_templates(owner).is_err());
    }

    #[test]
    fn v081_shared_templates_preserve_exact_reachability_for_every_profile() {
        for profile in [
            KickTableProfileId::Srs90,
            KickTableProfileId::SrsPlus,
            KickTableProfileId::SrsX,
            KickTableProfileId::Jstris180,
            KickTableProfileId::NoKick,
        ] {
            for height in 1..=6 {
                let owner = super::SharedReachabilityTemplates::new(10, height, profile);
                for piece in PieceKind::STANDARD_TETROMINOES {
                    let shared = owner.template(piece);
                    let private = ReachabilityTemplate::compile(10, height, piece, profile);
                    for board in [0, 0b1110010011, 0b1001100010] {
                        let shared_result = super::search_reachable_locks(
                            &shared,
                            board,
                            &mut ReachabilityScratch::default(),
                            None,
                        );
                        let private_result = super::search_reachable_locks(
                            &private,
                            board,
                            &mut ReachabilityScratch::default(),
                            None,
                        );
                        assert_eq!(shared_result.locks.anchors, private_result.locks.anchors);
                        assert_eq!(
                            shared_result.visited_state_count,
                            private_result.visited_state_count
                        );
                        assert_eq!(shared_result.exhaustive, private_result.exhaustive);
                    }
                }
            }
        }
    }

    /// Compose explicit request-on, policy-on and pinned-snapshot test inputs
    /// without mutating the process-wide A/B policy or asset registry. Policy
    /// admission itself is covered by the separate request-off diagnostics.
    fn pin_owned_conditioned_test_snapshot(
        workspace: &mut ReachabilityWorkspace,
        profile: KickTableProfileId,
        snapshot: std::sync::Arc<crate::conditioned_local_product::QualifiedLocalRelationPack>,
    ) {
        assert!(
            snapshot.accounted_bytes() <= crate::legal_board::MAX_ACTIVE_ACCELERATOR_BYTES,
            "test-only snapshot must still fit the unchanged shared asset cap"
        );
        workspace.configure_kick_profile(profile, false);
        workspace.conditioned = Some(snapshot.into());
        workspace.conditioned_enabled = Some(true);
        workspace.conditioned_policy_enabled = Some(true);
        workspace.metrics.conditioned_requested = true;
        workspace.metrics.conditioned_policy_enabled = true;
        workspace.metrics.conditioned_snapshot_active = true;
    }

    #[test]
    fn conditioned_diagnostics_distinguish_request_off_from_a_missing_hit() {
        let mut workspace = ReachabilityWorkspace::default();
        workspace.configure_kick_profile(KickTableProfileId::SrsPlus, false);
        let metrics = workspace.metrics();
        assert!(!metrics.conditioned_requested);
        assert!(!metrics.conditioned_policy_enabled);
        assert!(!metrics.conditioned_snapshot_active);
        assert_eq!(metrics.conditioned_lookup_attempts, 0);
        assert_eq!(metrics.conditioned_complete_hits, 0);
        assert_eq!(metrics.conditioned_misses, 0);
    }

    #[test]
    fn one_line_prepared_context_projection_covers_vec_minimum_capacity() {
        for height in 1..=6 {
            let mut frames = Vec::<Option<super::PreparedRelationWindows>>::new();
            frames.resize(1 << height, None);
            let actual =
                frames.capacity() * core::mem::size_of::<Option<super::PreparedRelationWindows>>();
            let projected = super::conditioned_context_frame_capacity(height).unwrap()
                * core::mem::size_of::<Option<super::PreparedRelationWindows>>() as u128;
            assert!(
                projected >= actual as u128,
                "{height}L projection misses Vec growth capacity"
            );
        }
        assert_eq!(super::conditioned_context_frame_capacity(1), Some(4));
        assert_eq!(super::conditioned_context_frame_capacity(128), None);
        assert!(super::checked_reachability_retained_upper_bound(
            10,
            1,
            KickTableProfileId::SrsPlus
        )
        .is_some());
    }

    #[test]
    fn conditioned_early_positive_does_not_cache_other_locks_as_unreachable() {
        let profile = KickTableProfileId::SrsPlus;
        let piece = PieceKind::O;
        let template = ReachabilityTemplate::compile(10, 4, piece, profile);
        let window = ConditionedPoseWindow {
            min_x: 0,
            max_x: 9,
            min_y: 4,
            max_y: i8::try_from(template.ceiling).unwrap(),
        };
        let entries = super::canonical_sky_entry_poses(10, template.ceiling, &template.sky_seeds);
        let relation =
            derive_exact_conditioned_local_relation(10, 4, 0, piece, profile, window, &entries)
                .unwrap();
        assert!(!relation.exits().is_empty());
        let binding = built_in_local_relation_binding(profile).unwrap();
        let bytes = encode_local_relation_candidate_pack(binding, &[relation]).unwrap();
        let loaded = load_local_relation_candidate_pack(&bytes, binding, None).unwrap();
        let reference = super::exact_spawn_lock_anchors(10, 4, 0, piece, profile).unwrap();
        let catalog =
            super::GeometryCatalog::compile_for_required_cells_on_dimensions(10, 4, 0, 0).unwrap();
        let mut workspace = ReachabilityWorkspace::default();
        workspace.configure_kick_profile(profile, false);
        // Pin locally to avoid global registry contention with other unit tests.
        workspace.conditioned =
            Some(std::sync::Arc::new(qualified_local_relation_for_solver_test(loaded)).into());
        workspace.prepare_template(&catalog, piece);
        assert!(workspace.lock_reachable_after_harddrop_miss_in_frame(
            &catalog,
            0,
            piece,
            RotationState::Zero,
            4,
            0,
            Some(LocalRelationRowFrame::new(4, 0).unwrap()),
        ));
        let key_index = workspace
            .cache
            .keys
            .iter()
            .position(|key| key.valid && key.board == 0)
            .unwrap();
        assert!(!workspace.cache.keys[key_index].exhaustive);
        let returned = workspace
            .cache
            .payload(workspace.cache.keys[key_index].payload_index)
            .locks
            .anchors;
        let (slot, missing) = reference
            .iter()
            .zip(returned)
            .enumerate()
            .find_map(|(slot, (expected, returned))| {
                let missing = *expected & !returned;
                (missing != 0).then_some((slot, missing.trailing_zeros()))
            })
            .expect("early positive intentionally leaves other reachable locks unexplored");
        let rotation = RotationState::ALL[slot];
        let x = (missing % 10) as i8;
        let y = (missing / 10) as i8;
        assert!(matches!(
            workspace.cache.query(0, piece, 10, rotation, x, y),
            ReachabilityCacheLookup::Search { .. }
        ));
        assert!(workspace.lock_reachable_after_harddrop_miss_in_frame(
            &catalog,
            0,
            piece,
            rotation,
            x,
            y,
            Some(LocalRelationRowFrame::new(4, 0).unwrap()),
        ));
        assert_eq!(workspace.metrics().conditioned_lookup_attempts, 2);
    }

    #[test]
    fn v081_conditioned_reverse_exit_macro_matches_independent_primitive() {
        use crate::reachability_reference::reference_spawn_lock_anchors;
        for profile in [
            KickTableProfileId::Srs90,
            KickTableProfileId::SrsPlus,
            KickTableProfileId::SrsX,
            KickTableProfileId::Jstris180,
            KickTableProfileId::NoKick,
        ] {
            for height in [2, 4, 6] {
                for piece in [PieceKind::T, PieceKind::J, PieceKind::L] {
                    let template = ReachabilityTemplate::compile(10, height, piece, profile);
                    let entries =
                        super::canonical_sky_entry_poses(10, template.ceiling, &template.sky_seeds);
                    for board in [0, 0b1110010011, 0b1001100010 | (0b1001000001 << 10)] {
                        let record = derive_exact_conditioned_local_relation(
                            10,
                            height,
                            board,
                            piece,
                            profile,
                            ConditionedPoseWindow {
                                min_x: 0,
                                max_x: 9,
                                min_y: (height / 2) as i8,
                                max_y: i8::try_from(template.ceiling).unwrap(),
                            },
                            &entries,
                        )
                        .unwrap();
                        let reference =
                            reference_spawn_lock_anchors(10, height, board, piece, profile)
                                .unwrap();
                        // Lower-board obstacles and a window first-exit cover
                        // exercise both unreachable locks and paths that leave
                        // the local window before returning through a kick.
                        for rotation in RotationState::ALL {
                            for x in [0, 4, 8] {
                                for y in [0, (height / 2) as i8, height as i8 - 1] {
                                    if record.exits().is_empty() {
                                        continue;
                                    }
                                    let result = super::reverse_lock_reachable_via_proven_exits(
                                        &template,
                                        board,
                                        rotation,
                                        x,
                                        y,
                                        &mut ReachabilityScratch::default(),
                                        record.exits(),
                                    )
                                    .expect("audited local exits remain collision-free");
                                    let expected = reference[rotation.quarter_turns() as usize]
                                        & (1_u64 << (y as usize * 10 + x as usize))
                                        != 0;
                                    assert_eq!(
                                        result.reachable, expected,
                                        "{profile:?} {height}L {piece:?} {board:#x} {rotation:?}/{x}/{y}",
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn v081_conditioned_reverse_exit_macro_rejects_invalid_or_blocked_exit_footprints() {
        let template =
            ReachabilityTemplate::compile(10, 4, PieceKind::O, KickTableProfileId::SrsPlus);
        let entry = |x, y| ConditionedReachabilityEntryPose {
            rotation: RotationState::Zero,
            x,
            y,
        };
        for (board, exits) in [
            (0, vec![entry(-1, 1)]),
            (
                0,
                vec![entry(4, i8::try_from(template.ceiling + 1).unwrap())],
            ),
            (1_u64 << 14, vec![entry(4, 1)]),
            (0, Vec::new()),
        ] {
            assert!(super::reverse_lock_reachable_via_proven_exits(
                &template,
                board,
                RotationState::Zero,
                4,
                0,
                &mut ReachabilityScratch::default(),
                &exits,
            )
            .is_none());
        }
    }

    #[test]
    fn v081_conditioned_boolean_macro_keeps_full_family_modes_on_original_composer() {
        let profile = KickTableProfileId::SrsPlus;
        let piece = PieceKind::O;
        let template = ReachabilityTemplate::compile(10, 4, piece, profile);
        let entries = super::canonical_sky_entry_poses(10, template.ceiling, &template.sky_seeds);
        let record = derive_exact_conditioned_local_relation(
            10,
            4,
            0,
            piece,
            profile,
            ConditionedPoseWindow {
                min_x: 0,
                max_x: 9,
                min_y: 4,
                max_y: i8::try_from(template.ceiling).unwrap(),
            },
            &entries,
        )
        .unwrap();
        let binding = built_in_local_relation_binding(profile).unwrap();
        let bytes = encode_local_relation_candidate_pack(binding, &[record]).unwrap();
        let loaded = load_local_relation_candidate_pack(&bytes, binding, None).unwrap();
        let snapshot = std::sync::Arc::new(qualified_local_relation_for_solver_test(loaded));
        let catalog =
            super::GeometryCatalog::compile_for_required_cells_on_dimensions(10, 4, 0, 0).unwrap();
        for shortcut_enabled in [false, true] {
            let mut workspace = ReachabilityWorkspace::default();
            pin_owned_conditioned_test_snapshot(
                &mut workspace,
                profile,
                std::sync::Arc::clone(&snapshot),
            );
            workspace.configure_conditioned_boolean_shortcuts(shortcut_enabled);
            workspace.prepare_template(&catalog, piece);
            assert!(workspace.lock_reachable_after_harddrop_miss_in_frame(
                &catalog,
                0,
                piece,
                RotationState::Zero,
                4,
                0,
                Some(LocalRelationRowFrame::new(4, 0).unwrap()),
            ));
            let slot = workspace
                .cache
                .keys
                .iter()
                .position(|key| key.valid && key.board == 0)
                .unwrap();
            assert!(!workspace.cache.keys[slot].exhaustive);
            let payload_index = workspace.cache.keys[slot].payload_index;
            let partial_locks = workspace.cache.payload(payload_index).locks.anchors;
            if shortcut_enabled {
                let mut desired = ReachableLocks::default();
                desired.insert(10, RotationState::Zero, 4, 0);
                assert_eq!(partial_locks, desired.anchors);
            }
            // An exhaustive-cache request still runs the full composer,
            // irrespective of the Boolean shortcut switch.
            let reference = crate::reachability_reference::reference_spawn_lock_anchors(
                10, 4, 0, piece, profile,
            )
            .unwrap();
            let (missing_slot, missing_anchor) = reference
                .iter()
                .zip(partial_locks)
                .enumerate()
                .find_map(|(rotation, (expected, returned))| {
                    let missing = *expected & !returned;
                    (missing != 0).then_some((rotation, missing.trailing_zeros()))
                })
                .expect("a partial positive leaves other targets unknown");
            workspace.cache.keys[slot].observation = u8::MAX;
            assert!(workspace.lock_reachable_after_harddrop_miss_in_frame(
                &catalog,
                0,
                piece,
                RotationState::ALL[missing_slot],
                (missing_anchor % 10) as i8,
                (missing_anchor / 10) as i8,
                Some(LocalRelationRowFrame::new(4, 0).unwrap()),
            ));
            assert!(workspace.cache.keys[slot].exhaustive);
            assert_eq!(
                workspace.cache.payload(payload_index).locks.anchors,
                reference,
            );
        }
    }

    #[test]
    fn changing_target_height_invalidates_reachability_templates_and_cache() {
        let short = super::GeometryCatalog::compile_for_required_cells_on_dimensions(10, 4, 0, 0)
            .expect("four-row catalog");
        let tall = super::GeometryCatalog::compile_for_required_cells_on_dimensions(10, 6, 0, 0)
            .expect("six-row catalog");
        let mut workspace = ReachabilityWorkspace::default();
        workspace.configure(8);
        workspace.configure_kick_profile(KickTableProfileId::SrsPlus, false);
        workspace.prepare_template(&short, PieceKind::T);
        assert_eq!(
            workspace.templates[super::piece_index(PieceKind::T)]
                .as_ref()
                .unwrap()
                .height,
            4
        );

        let mut reachable = ReachableLocks::default();
        reachable.insert(10, RotationState::Zero, 4, 0);
        workspace.cache.insert(
            0,
            PieceKind::T,
            reachable,
            ReachableLocks::default(),
            true,
            true,
        );
        assert_eq!(
            workspace
                .cache
                .query(0, PieceKind::T, 10, RotationState::Zero, 4, 0),
            ReachabilityCacheLookup::Reachable,
        );

        workspace.prepare_template(&tall, PieceKind::T);
        assert_eq!(
            workspace.templates[super::piece_index(PieceKind::T)]
                .as_ref()
                .unwrap()
                .height,
            6
        );
        assert!(workspace.cache.keys.is_empty());
        assert_eq!(
            workspace.cache.exhaustive_observations,
            LARGE_SEARCH_EXHAUSTIVE_OBSERVATIONS,
        );
    }

    #[test]
    fn build_frame_uses_pinned_local_relation_and_frame_miss_falls_back() {
        let profile = KickTableProfileId::NoKick;
        let piece = PieceKind::T;
        let template = ReachabilityTemplate::compile(10, 4, piece, profile);
        let window = ConditionedPoseWindow {
            min_x: 0,
            max_x: 9,
            min_y: 4,
            max_y: i8::try_from(template.ceiling).unwrap(),
        };
        let mut entries = template
            .sky_seeds
            .iter()
            .map(|&index| {
                let state = state_from_index(10, template.ceiling, usize::from(index));
                ConditionedReachabilityEntryPose {
                    rotation: state.rotation,
                    x: state.x,
                    y: i8::try_from(state.y).unwrap(),
                }
            })
            .collect::<Vec<_>>();
        entries.sort_unstable_by_key(|pose| (pose.rotation.quarter_turns(), pose.x, pose.y));
        let record =
            derive_exact_conditioned_local_relation(10, 4, 0, piece, profile, window, &entries)
                .expect("full sky-seed window is a valid local relation");
        let binding = built_in_local_relation_binding(profile).unwrap();
        let bytes = encode_local_relation_candidate_pack(binding, &[record]).unwrap();
        let loaded = load_local_relation_candidate_pack(&bytes, binding, None).unwrap();
        let snapshot = std::sync::Arc::new(qualified_local_relation_for_solver_test(loaded));

        let catalog = super::GeometryCatalog::compile_for_required_cells_on_dimensions(10, 4, 0, 0)
            .expect("four-row geometry dimensions");
        let frame = LocalRelationRowFrame::new(4, 0).unwrap();
        let reference = super::exact_spawn_lock_anchors(10, 4, 0, piece, profile).unwrap();
        let mut workspace = ReachabilityWorkspace::default();
        pin_owned_conditioned_test_snapshot(
            &mut workspace,
            profile,
            std::sync::Arc::clone(&snapshot),
        );
        workspace.prepare_template(&catalog, piece);
        let got = workspace.lock_reachable_after_harddrop_miss_in_frame(
            &catalog,
            0,
            piece,
            RotationState::Zero,
            4,
            0,
            Some(frame),
        );
        let bit = 1_u64 << 4;
        assert_eq!(got, reference[0] & bit != 0);
        assert_eq!(workspace.metrics().conditioned_complete_hits, 1);
        assert_eq!(workspace.metrics().conditioned_misses, 0);
        assert!(workspace.metrics().conditioned_requested);
        assert!(workspace.metrics().conditioned_policy_enabled);
        assert!(workspace.metrics().conditioned_snapshot_active);
        assert!(workspace.metrics().conditioned_lookup_attempts > 0);
        assert_eq!(workspace.metrics().conditioned_out_of_scope, 0);
        assert_eq!(workspace.metrics().conditioned_unknown, 0);
        assert!(workspace.generated_state_count() > 0);
        let repeated = workspace.lock_reachable_after_harddrop_miss_in_frame(
            &catalog,
            0,
            piece,
            RotationState::Zero,
            4,
            0,
            Some(frame),
        );
        assert_eq!(repeated, got);
        assert_eq!(workspace.metrics().conditioned_complete_hits, 1);
        assert_eq!(
            workspace.metrics().cache_reachable_hits + workspace.metrics().cache_unreachable_hits,
            1,
        );

        let other_frame = LocalRelationRowFrame::new(4, 1).unwrap();
        let mut frame_miss_workspace = ReachabilityWorkspace::default();
        pin_owned_conditioned_test_snapshot(&mut frame_miss_workspace, profile, snapshot);
        frame_miss_workspace.prepare_template(&catalog, piece);
        let fallback = frame_miss_workspace.lock_reachable_after_harddrop_miss_in_frame(
            &catalog,
            0,
            piece,
            RotationState::Zero,
            4,
            0,
            Some(other_frame),
        );
        assert_eq!(fallback, got);
        assert_eq!(frame_miss_workspace.metrics().conditioned_misses, 1);
        assert!(frame_miss_workspace.metrics().conditioned_snapshot_active);
        // The pinned generation has no record for this original-row frame.
        // Its immutable absence is prepared once, not re-searched per board.
        assert_eq!(
            frame_miss_workspace.metrics().conditioned_lookup_attempts,
            0
        );
    }

    /// Local-only acceptance of the generated, source-bound product-sized
    /// candidates. The assets are intentionally not checked into Git and a
    /// test-only constructor never grants a release signature.
    #[test]
    #[ignore = "requires five generated packs in the local candidate directory"]
    fn generated_five_profile_packs_reach_the_real_build_frame_lookup() {
        let directory = std::env::var_os("CLEARRA_CONDITIONED_CANDIDATE_DIR")
            .map(std::path::PathBuf::from)
            .expect("explicit generated candidate-pack directory is required");
        for (profile, name) in [
            (KickTableProfileId::Srs90, "srs"),
            (KickTableProfileId::SrsPlus, "srs-plus"),
            (KickTableProfileId::SrsX, "srs-x"),
            (KickTableProfileId::Jstris180, "jstris-180"),
            (KickTableProfileId::NoKick, "no-kick"),
        ] {
            let bytes = std::fs::read(directory.join(format!("{name}.cllr")))
                .expect("complete candidate pack is present");
            let binding = built_in_local_relation_binding(profile).unwrap();
            let pack = load_local_relation_candidate_pack(&bytes, binding, None).unwrap();
            let mut indexed_boards = 0;
            for height in 1..=6 {
                let deleted_frames: &[u16] = if height == 2 { &[0, 1, 2] } else { &[0] };
                for piece in [
                    PieceKind::I,
                    PieceKind::O,
                    PieceKind::T,
                    PieceKind::S,
                    PieceKind::Z,
                    PieceKind::J,
                    PieceKind::L,
                ] {
                    let template = ReachabilityTemplate::compile(10, height, piece, profile);
                    let window = ConditionedPoseWindow {
                        min_x: 0,
                        max_x: 9,
                        min_y: height as i8,
                        max_y: i8::try_from(template.ceiling).unwrap(),
                    };
                    let mut entries = template
                        .sky_seeds
                        .iter()
                        .map(|&index| {
                            let state = state_from_index(10, template.ceiling, usize::from(index));
                            ConditionedReachabilityEntryPose {
                                rotation: state.rotation,
                                x: state.x,
                                y: i8::try_from(state.y).unwrap(),
                            }
                        })
                        .collect::<Vec<_>>();
                    entries.sort_unstable_by_key(|pose| {
                        (pose.rotation.quarter_turns(), pose.x, pose.y)
                    });
                    for &deleted in deleted_frames {
                        let frame = LocalRelationRowFrame::new(height, deleted).unwrap();
                        for board in 0..=255_u64 {
                            assert!(
                                matches!(
                                    pack.lookup_with_frame(
                                        10, height, board, frame, piece, profile, window, &entries
                                    ),
                                    LocalRelationCandidateLookup::Hit(_)
                                ),
                                "{name} {height}L {piece:?} deleted={deleted} board={board:#x}"
                            );
                            indexed_boards += 1;
                        }
                    }
                }
            }
            assert_eq!(indexed_boards, 56 * 256, "{name} declared occupancy domain");
            let snapshot = std::sync::Arc::new(qualified_local_relation_for_solver_test(pack));
            let mut checked_contexts = 0;
            for height in 1..=6 {
                let catalog = super::GeometryCatalog::compile_for_required_cells_on_dimensions(
                    10, height, 0, 0,
                )
                .expect("product dimensions");
                let deleted_frames: &[u16] = if height == 2 { &[0, 1, 2] } else { &[0] };
                for piece in [
                    PieceKind::I,
                    PieceKind::O,
                    PieceKind::T,
                    PieceKind::S,
                    PieceKind::Z,
                    PieceKind::J,
                    PieceKind::L,
                ] {
                    let reference =
                        super::exact_spawn_lock_anchors(10, height, 0, piece, profile).unwrap();
                    for &deleted in deleted_frames {
                        let frame = LocalRelationRowFrame::new(height, deleted).unwrap();
                        let mut workspace = ReachabilityWorkspace::default();
                        pin_owned_conditioned_test_snapshot(
                            &mut workspace,
                            profile,
                            std::sync::Arc::clone(&snapshot),
                        );
                        workspace.prepare_template(&catalog, piece);
                        let got = workspace.lock_reachable_after_harddrop_miss_in_frame(
                            &catalog,
                            0,
                            piece,
                            RotationState::Zero,
                            4,
                            0,
                            Some(frame),
                        );
                        assert_eq!(
                            got,
                            reference[0] & (1_u64 << 4) != 0,
                            "{name} {height}L {piece:?} deleted={deleted}"
                        );
                        assert_eq!(
                            workspace.metrics().conditioned_complete_hits,
                            1,
                            "{name} {height}L {piece:?} deleted={deleted} missed its pack"
                        );
                        checked_contexts += 1;
                    }
                }
            }
            assert_eq!(checked_contexts, 56, "{name} product context coverage");
        }
    }

    #[test]
    fn full_height_relation_skips_exact_continuation_without_changing_locks() {
        let profile = KickTableProfileId::Jstris180;
        let piece = PieceKind::J;
        let template = ReachabilityTemplate::compile(10, 4, piece, profile);
        let window = ConditionedPoseWindow {
            min_x: 0,
            max_x: 9,
            min_y: 0,
            max_y: i8::try_from(template.ceiling).unwrap(),
        };
        let mut entries = template
            .sky_seeds
            .iter()
            .map(|&index| {
                let state = state_from_index(10, template.ceiling, usize::from(index));
                ConditionedReachabilityEntryPose {
                    rotation: state.rotation,
                    x: state.x,
                    y: i8::try_from(state.y).unwrap(),
                }
            })
            .collect::<Vec<_>>();
        entries.sort_unstable_by_key(|pose| (pose.rotation.quarter_turns(), pose.x, pose.y));
        let record =
            derive_exact_conditioned_local_relation(10, 4, 0, piece, profile, window, &entries)
                .unwrap();
        assert!(record.exits().is_empty());
        let reference = super::exact_spawn_lock_anchors(10, 4, 0, piece, profile).unwrap();
        assert_eq!(record.grounded_lock_anchors(), reference);
        let binding = built_in_local_relation_binding(profile).unwrap();
        let bytes = encode_local_relation_candidate_pack(binding, &[record]).unwrap();
        let loaded = load_local_relation_candidate_pack(&bytes, binding, None).unwrap();
        let snapshot = std::sync::Arc::new(qualified_local_relation_for_solver_test(loaded));

        let catalog =
            super::GeometryCatalog::compile_for_required_cells_on_dimensions(10, 4, 0, 0).unwrap();
        let mut workspace = ReachabilityWorkspace::default();
        pin_owned_conditioned_test_snapshot(&mut workspace, profile, snapshot);
        workspace.prepare_template(&catalog, piece);
        let got = workspace.lock_reachable_after_harddrop_miss_in_frame(
            &catalog,
            0,
            piece,
            RotationState::Zero,
            4,
            0,
            Some(LocalRelationRowFrame::new(4, 0).unwrap()),
        );
        assert_eq!(got, reference[0] & (1_u64 << 4) != 0);
        assert_eq!(workspace.metrics().conditioned_complete_hits, 1);
        assert_eq!(workspace.generated_state_count(), 0);
    }

    #[test]
    fn top_half_relation_continues_from_first_exits_on_the_query_board() {
        let profile = KickTableProfileId::SrsX;
        let piece = PieceKind::L;
        let template = ReachabilityTemplate::compile(10, 4, piece, profile);
        let window = ConditionedPoseWindow {
            min_x: 0,
            max_x: 9,
            min_y: 2,
            max_y: i8::try_from(template.ceiling).unwrap(),
        };
        let mut entries = template
            .sky_seeds
            .iter()
            .map(|&index| {
                let state = state_from_index(10, template.ceiling, usize::from(index));
                ConditionedReachabilityEntryPose {
                    rotation: state.rotation,
                    x: state.x,
                    y: i8::try_from(state.y).unwrap(),
                }
            })
            .collect::<Vec<_>>();
        entries.sort_unstable_by_key(|pose| (pose.rotation.quarter_turns(), pose.x, pose.y));
        let record =
            derive_exact_conditioned_local_relation(10, 4, 0, piece, profile, window, &entries)
                .unwrap();
        assert!(!record.exits().is_empty());
        let reference = super::exact_spawn_lock_anchors(10, 4, 0, piece, profile).unwrap();
        assert_eq!(
            record.compose_exact_global_lock_anchors_for_board(0),
            Some(reference)
        );
        let binding = built_in_local_relation_binding(profile).unwrap();
        let bytes = encode_local_relation_candidate_pack(binding, &[record]).unwrap();
        let loaded = load_local_relation_candidate_pack(&bytes, binding, None).unwrap();
        let snapshot = std::sync::Arc::new(qualified_local_relation_for_solver_test(loaded));

        let catalog =
            super::GeometryCatalog::compile_for_required_cells_on_dimensions(10, 4, 0, 0).unwrap();
        let mut workspace = ReachabilityWorkspace::default();
        pin_owned_conditioned_test_snapshot(&mut workspace, profile, snapshot);
        workspace.prepare_template(&catalog, piece);
        let got = workspace.lock_reachable_after_harddrop_miss_in_frame(
            &catalog,
            0,
            piece,
            RotationState::Zero,
            4,
            0,
            Some(LocalRelationRowFrame::new(4, 0).unwrap()),
        );
        assert_eq!(got, reference[0] & (1_u64 << 4) != 0);
        assert_eq!(workspace.metrics().conditioned_complete_hits, 1);
        assert!(workspace.generated_state_count() > 0);
    }

    #[test]
    fn srs_x_reachability_compiles_every_canonical_kick_in_order() {
        const WIDTH: u8 = 10;
        const HEIGHT: u8 = 6;
        const SOURCE_X: i8 = 4;
        const SOURCE_Y: i8 = 4;

        let profile = builtin_kick_profile(KickTableProfileId::SrsX).expect("SRS-X profile");
        for piece in PieceKind::STANDARD_TETROMINOES {
            let template =
                ReachabilityTemplate::compile(WIDTH, HEIGHT, piece, KickTableProfileId::SrsX);
            for from in RotationState::ALL {
                let source = State {
                    rotation: from,
                    x: SOURCE_X,
                    y: i16::from(SOURCE_Y),
                };
                let source_index = state_index(WIDTH, template.ceiling, source)
                    .expect("interior source state is compiled");
                for (slot, to) in [
                    from.clockwise(),
                    from.counter_clockwise(),
                    from.rotated_180(),
                ]
                .into_iter()
                .enumerate()
                {
                    let sequence = profile
                        .sequence_for(KickTransition::new(piece, from, to))
                        .expect("all standard SRS-X transitions are connected");
                    let transition_index = source_index * 3 + slot;
                    let start = template.rotation_target_offsets[transition_index] as usize;
                    let end = template.rotation_target_offsets[transition_index + 1] as usize;
                    let compiled_targets = template.rotation_targets[start..end]
                        .iter()
                        .map(|&index| state_from_index(WIDTH, template.ceiling, usize::from(index)))
                        .collect::<Vec<_>>();
                    let expected_targets = sequence
                        .offsets()
                        .iter()
                        .map(|offset| {
                            let (dx, dy) =
                                normalized_kick_delta(piece, from, to, offset.dx(), offset.dy());
                            State {
                                rotation: to,
                                x: SOURCE_X + dx,
                                y: i16::from(SOURCE_Y) + i16::from(dy),
                            }
                        })
                        .collect::<Vec<_>>();

                    assert_eq!(
                        compiled_targets, expected_targets,
                        "{piece:?} {from:?}->{to:?}"
                    );
                    assert_eq!(
                        template.rotation_kick_indices[start..end],
                        (0..sequence.offsets().len())
                            .map(|index| index as u32)
                            .collect::<Vec<_>>(),
                        "{piece:?} {from:?}->{to:?} kick order"
                    );
                }
            }
        }
    }

    #[test]
    fn srs_plus_triple_kick_dependency_changes_t_lock_reachability() {
        const WIDTH: u8 = 5;
        const HEIGHT: u8 = 6;

        let template =
            ReachabilityTemplate::compile(WIDTH, HEIGHT, PieceKind::T, KickTableProfileId::SrsPlus);
        let mut scratch = ReachabilityScratch::default();

        // L and the two Z placements form the lower TST cavity. The two gray
        // cells are fixed field cells. J supplies the upper-left kick surface.
        let without_j = board_mask(
            WIDTH,
            &[
                (0, 0),
                (1, 0),
                (0, 1),
                (0, 2),
                (3, 0),
                (4, 0),
                (2, 1),
                (3, 1),
                (3, 2),
                (4, 2),
                (2, 3),
                (3, 3),
                (4, 1),
                (4, 3),
            ],
        );
        let with_j = without_j | board_mask(WIDTH, &[(0, 3), (0, 4), (0, 5), (1, 5)]);

        assert!(
            !reverse_lock_reachable(
                &template,
                without_j,
                RotationState::Right,
                1,
                1,
                &mut scratch,
            )
            .reachable
        );
        assert!(
            reverse_lock_reachable(&template, with_j, RotationState::Right, 1, 1, &mut scratch,)
                .reachable
        );
        let evidence =
            best_scoring_lock_evidence(&template, with_j, RotationState::Right, 1, 1, &mut scratch)
                .expect("J must enable the final T rotation");
        assert!(evidence.last_action_was_rotation());
        assert!(evidence.used_kick());
    }

    // Reference: perform the full reverse search before ranking each
    // predecessor, including reachable losers and equal-rank moves.
    fn unpruned_scoring_lock_evidence<const LEGACY_T_SPIN: bool>(
        template: &ReachabilityTemplate,
        board: u64,
        target: State,
        scratch: &mut ReachabilityScratch,
    ) -> (Option<clearra_replay::ScoringLockEvidence>, bool) {
        let Some(target_index) = state_index(template.width, template.ceiling, target) else {
            return (None, false);
        };
        if template.state_masks[target_index] == super::INVALID_STATE_MASK
            || board & template.state_masks[target_index] != 0
            || !super::grounded_index(template, board, target_index)
        {
            return (None, false);
        }
        let start = template.reverse_rotation_offsets[target_index] as usize;
        let end = template.reverse_rotation_offsets[target_index + 1] as usize;
        let mut best = None::<((bool, bool, u32), clearra_replay::ScoringLockEvidence)>;
        let mut tied = false;
        for source in &template.reverse_rotation_sources[start..end] {
            let source_index = usize::from(source.source);
            if !super::reverse_rotation_is_first_success(template, board, *source)
                || template.state_masks[source_index] == super::INVALID_STATE_MASK
                || board & template.state_masks[source_index] != 0
                || !super::reverse_lock_reachable_large(template, board, source_index, scratch)
                    .reachable
            {
                continue;
            }
            let transition = source_index * 3 + usize::from(source.slot);
            let offset = template.rotation_target_offsets[transition] as usize
                + source.prior_target_count as usize;
            let kick = template.rotation_kick_indices[offset];
            let quarter_turn = source.slot < 2;
            let rank = if LEGACY_T_SPIN {
                (kick == 4, false, 0)
            } else {
                (quarter_turn && kick == 4, quarter_turn, kick)
            };
            if best.as_ref().is_some_and(|(current, _)| rank == *current) {
                tied = true;
            }
            if best.as_ref().is_none_or(|(current, _)| rank > *current) {
                let predecessor = state_from_index(template.width, template.ceiling, source_index);
                let request = match source.slot {
                    0 => clearra_replay::RotationRequest::Clockwise,
                    1 => clearra_replay::RotationRequest::CounterClockwise,
                    _ => clearra_replay::RotationRequest::HalfTurn,
                };
                best = Some((
                    rank,
                    clearra_replay::ScoringLockEvidence::rotation(
                        predecessor.rotation,
                        request,
                        u8::try_from(kick).expect("builtin kick ordinal"),
                        target.x - predecessor.x,
                        i8::try_from(target.y - predecessor.y).expect("builtin kick delta"),
                        predecessor.x,
                        i8::try_from(predecessor.y).expect("builtin sky source"),
                    ),
                ));
            }
        }
        (best.map(|(_, evidence)| evidence), tied)
    }

    #[test]
    fn scoring_rotation_rank_pruning_preserves_exact_evidence_and_first_tie() {
        let mut saw = [false; 2];
        let mut saw_tie = [false; 2];
        let mut saw_unavailable = [false; 2];
        let mut fewer_searches = [false; 2];
        for profile in [
            KickTableProfileId::Srs90,
            KickTableProfileId::SrsPlus,
            KickTableProfileId::SrsX,
            KickTableProfileId::Jstris180,
        ] {
            let template = ReachabilityTemplate::compile(5, 4, PieceKind::T, profile);
            let mut reference_scratch = ReachabilityScratch::default();
            let mut scratch = ReachabilityScratch::default();
            for board in [0, 0b00111, 0b11001, 0b01010_10001] {
                for rotation in RotationState::ALL {
                    for x in 0..5 {
                        for y in 0..4 {
                            let target = State {
                                rotation,
                                x,
                                y: i16::from(y),
                            };
                            for (index, legacy) in [false, true].into_iter().enumerate() {
                                let reference_before = reference_scratch.generation;
                                let before = scratch.generation;
                                let (actual, (expected, tied)) = if legacy {
                                    (
                                        super::preferred_scoring_lock_evidence::<true>(
                                            &template,
                                            board,
                                            rotation,
                                            x,
                                            y,
                                            &mut scratch,
                                        ),
                                        unpruned_scoring_lock_evidence::<true>(
                                            &template,
                                            board,
                                            target,
                                            &mut reference_scratch,
                                        ),
                                    )
                                } else {
                                    (
                                        best_scoring_lock_evidence(
                                            &template,
                                            board,
                                            rotation,
                                            x,
                                            y,
                                            &mut scratch,
                                        ),
                                        unpruned_scoring_lock_evidence::<false>(
                                            &template,
                                            board,
                                            target,
                                            &mut reference_scratch,
                                        ),
                                    )
                                };
                                assert_eq!(actual, expected, "{profile:?} {target:?} {board:#x}");
                                saw[index] |= actual.is_some();
                                saw_unavailable[index] |= actual.is_none();
                                saw_tie[index] |= tied;
                                let actual_searches = scratch.generation - before;
                                let reference_searches =
                                    reference_scratch.generation - reference_before;
                                assert!(actual_searches <= reference_searches);
                                fewer_searches[index] |= actual_searches < reference_searches;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(saw, [true; 2]);
        assert_eq!(saw_unavailable, [true; 2]);
        assert_eq!(saw_tie, [true; 2]);
        assert_eq!(fewer_searches, [true; 2]);
    }

    fn board_mask(width: u8, cells: &[(u8, u8)]) -> u64 {
        cells.iter().fold(0_u64, |mask, &(x, y)| {
            mask | (1_u64 << (usize::from(y) * usize::from(width) + usize::from(x)))
        })
    }
}

#[cfg(test)]
#[path = "reachability_reference_tests.rs"]
mod reference_tests;
// SRP rationale: this module has one behavior-level change reason: exhaustive SRS+ lock reachability over compact WASM boards.
