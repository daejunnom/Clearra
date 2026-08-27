use std::sync::Arc;

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_coverage::pattern::{
    pattern_bitset::{PatternBitSet, PatternBitSetAllocationError},
    pattern_id::PatternId,
};

use crate::hold_automaton::HoldAutomatonState;

use super::bag_multiset_reachability::{
    checked_reachable_bag_multiset_peak_upper_bound, reachable_bag_multisets,
    reachable_bag_multisets_with_memory_limit, BoundedBagMultisetError,
};
use super::hold_multiset_reachability::ReachableMultisetWorkspace;
use super::materialized_pattern_universe::{
    MaterializedPatternUniverse, MaterializedPatternUniverseStructure,
};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PieceMultisetKey {
    counts: [u8; 7],
    total_count: u8,
}

const _: () = assert!(core::mem::size_of::<PieceMultisetKey>() == 8);

impl PieceMultisetKey {
    pub fn from_pieces(pieces: impl IntoIterator<Item = PieceKind>) -> Self {
        let mut key = Self::default();
        for piece in pieces {
            key.push(piece);
        }
        key
    }

    pub fn from_counts(counts: [u8; 7]) -> Self {
        let total_count = counts
            .iter()
            .copied()
            .fold(0_u8, |total, count| total.saturating_add(count));
        Self {
            counts,
            total_count,
        }
    }

    pub const fn counts(self) -> [u8; 7] {
        self.counts
    }

    pub const fn total_count(self) -> u8 {
        self.total_count
    }

    pub const fn count(self, piece: PieceKind) -> u8 {
        self.counts[piece_index(piece)]
    }

    fn componentwise_max(self, other: Self) -> Self {
        let mut counts = [0; 7];
        let mut total_count = 0u8;
        for index in 0..counts.len() {
            counts[index] = self.counts[index].max(other.counts[index]);
            total_count = total_count.saturating_add(counts[index]);
        }
        Self {
            counts,
            total_count,
        }
    }

    pub(super) fn push(&mut self, piece: PieceKind) {
        let index = piece_index(piece);
        self.counts[index] = self.counts[index].saturating_add(1);
        self.total_count = self.total_count.saturating_add(1);
    }

    pub(super) fn remove(&mut self, piece: PieceKind) -> bool {
        let index = piece_index(piece);
        if self.counts[index] == 0 {
            return false;
        }
        self.counts[index] -= 1;
        self.total_count -= 1;
        true
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackingMultisetFamily {
    envelope: PieceMultisetKey,
    groups: Vec<PackingMultisetGroup>,
    membership_kind: PackingPatternMembershipKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackingPatternMembershipKind {
    ExactMaterialized,
    ExactSymbolicStandardBag,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackingHoldProjection {
    PreserveFinalHoldLanguage,
    ReleaseHeldAtTerminal,
}

const MULTISET_WORKER_STACK_BYTES: usize = 1024 * 1024;
const MAX_PATTERN_MEMBERSHIPS_WITH_HOLD: u128 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackingMultisetFamilyBuildProjection {
    pub worker_count: usize,
    pub max_group_count: u128,
    pub max_record_count: u128,
    pub record_storage_bytes: u128,
    pub group_storage_bytes: u128,
    pub pattern_bitset_storage_bytes: u128,
    pub symbolic_frontier_bytes: u128,
    pub worker_stack_bytes: u128,
    pub worker_coordination_bytes: u128,
    pub required_peak_bytes: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FamilyMemoryLimit {
    already_retained_bytes: u128,
    max_memory_bytes: u128,
}

impl PackingPatternMembershipKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExactMaterialized => "exact-materialized",
            Self::ExactSymbolicStandardBag => "exact-symbolic-standard-bag",
        }
    }
}

impl PackingMultisetFamily {
    pub const fn envelope(&self) -> PieceMultisetKey {
        self.envelope
    }

    pub fn groups(&self) -> &[PackingMultisetGroup] {
        &self.groups
    }

    pub fn len(&self) -> usize {
        self.groups.len()
    }

    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    pub const fn membership_kind(&self) -> PackingPatternMembershipKind {
        self.membership_kind
    }

    /// Measures the storage retained by the materialized family without
    /// allocating a secondary identity set. Shared symbolic pattern bitsets
    /// are counted exactly once by allocation identity.
    pub fn checked_retained_bytes(&self) -> Option<u128> {
        let group_bytes = u128::try_from(self.groups.capacity())
            .ok()?
            .checked_mul(core::mem::size_of::<PackingMultisetGroup>() as u128)?;
        self.groups
            .iter()
            .enumerate()
            .try_fold(group_bytes, |bytes, (index, group)| {
                let already_counted = self.groups[..index]
                    .iter()
                    .any(|prior| Arc::ptr_eq(&prior.pattern_bits, &group.pattern_bits));
                if already_counted {
                    Some(bytes)
                } else {
                    bytes.checked_add(group.pattern_bits.checked_shared_retained_bytes()?)
                }
            })
    }

    pub fn single_group(&self, index: usize) -> Option<Self> {
        let group = self.groups.get(index)?.clone();
        Some(Self {
            envelope: group.key(),
            groups: vec![group],
            membership_kind: self.membership_kind,
        })
    }

    pub fn pattern_bits(&self, key: PieceMultisetKey) -> Option<&PatternBitSet> {
        self.groups
            .binary_search_by_key(&key, PackingMultisetGroup::key)
            .ok()
            .map(|index| self.groups[index].pattern_bits())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackingMultisetGroup {
    key: PieceMultisetKey,
    pattern_bits: Arc<PatternBitSet>,
}

impl PackingMultisetGroup {
    pub const fn key(&self) -> PieceMultisetKey {
        self.key
    }

    pub fn pattern_bits(&self) -> &PatternBitSet {
        self.pattern_bits.as_ref()
    }

    pub fn shared_pattern_bits(&self) -> Arc<PatternBitSet> {
        Arc::clone(&self.pattern_bits)
    }
}

impl MaterializedPatternUniverse {
    /// Derives an allocation-free upper bound from the actual record, group,
    /// worker-stack, and private PatternBitSet layouts used by family
    /// construction. The projection is intentionally about construction peak;
    /// it is not a proxy for pattern descriptor count.
    pub fn checked_packing_multiset_family_build_projection(
        &self,
        placed_piece_count: usize,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
        hold_projection: PackingHoldProjection,
        requested_workers: usize,
    ) -> Option<PackingMultisetFamilyBuildProjection> {
        let (source_piece_count, source_hold_enabled, empty) = match hold_projection {
            PackingHoldProjection::PreserveFinalHoldLanguage => {
                (placed_piece_count, hold_enabled, false)
            }
            PackingHoldProjection::ReleaseHeldAtTerminal if hold_enabled => (
                placed_piece_count.checked_sub(usize::from(initial_hold.hold_piece().is_some()))?,
                false,
                false,
            ),
            PackingHoldProjection::ReleaseHeldAtTerminal => (0, false, true),
        };
        if empty {
            return Some(PackingMultisetFamilyBuildProjection {
                worker_count: 0,
                max_group_count: 0,
                max_record_count: 0,
                record_storage_bytes: 0,
                group_storage_bytes: 0,
                pattern_bitset_storage_bytes: 0,
                symbolic_frontier_bytes: 0,
                worker_stack_bytes: 0,
                worker_coordination_bytes: 0,
                required_peak_bytes: 0,
            });
        }

        let symbolic = matches!(
            self.structure(),
            MaterializedPatternUniverseStructure::Standard7BagLexicographic { .. }
        );
        let pattern_count = self.pattern_count();
        if !symbolic && pattern_count as u128 > u32::MAX as u128 + 1 {
            return None;
        }
        let max_key_count = checked_multiset_key_count(source_piece_count)?;
        let membership_count = if symbolic {
            0
        } else {
            (pattern_count as u128).checked_mul(if source_hold_enabled {
                MAX_PATTERN_MEMBERSHIPS_WITH_HOLD
            } else {
                1
            })?
        };
        let max_group_count = if symbolic {
            max_key_count
        } else {
            max_key_count.min(membership_count)
        };
        usize::try_from(max_group_count).ok()?;
        usize::try_from(membership_count).ok()?;

        let group_storage_bytes =
            max_group_count.checked_mul(core::mem::size_of::<PackingMultisetGroup>() as u128)?;
        let record_storage_bytes =
            membership_count.checked_mul(core::mem::size_of::<PatternMultisetRecord>() as u128)?;
        if symbolic {
            let bitset = PatternBitSet::checked_all_projection(pattern_count)?;
            let symbolic_frontier_bytes =
                checked_reachable_bag_multiset_peak_upper_bound(source_piece_count)?;
            let key_storage_bytes =
                max_group_count.checked_mul(core::mem::size_of::<PieceMultisetKey>() as u128)?;
            let retained_family_build = key_storage_bytes
                .checked_add(group_storage_bytes)?
                .checked_add(bitset.constructor_peak_bytes)?;
            return Some(PackingMultisetFamilyBuildProjection {
                worker_count: 0,
                max_group_count,
                max_record_count: 0,
                record_storage_bytes: 0,
                group_storage_bytes,
                pattern_bitset_storage_bytes: bitset.shared_retained_bytes,
                symbolic_frontier_bytes,
                worker_stack_bytes: 0,
                worker_coordination_bytes: 0,
                required_peak_bytes: symbolic_frontier_bytes.max(retained_family_build),
            });
        }

        let worker_count = effective_multiset_worker_count(requested_workers, pattern_count);
        let parallel = worker_count > 1 && pattern_count >= 256;
        let active_workers = if parallel { worker_count } else { 1 };
        let worker_stack_bytes = if parallel {
            (active_workers as u128).checked_mul(MULTISET_WORKER_STACK_BYTES as u128)?
        } else {
            0
        };
        let max_sequence_len = self.max_sequence_len_for_projection()?;
        let sequence_storage_bytes = (active_workers as u128)
            .checked_mul(max_sequence_len as u128)?
            .checked_mul(core::mem::size_of::<PieceKind>() as u128)?;
        let workspace_key_capacity = PieceKind::STANDARD_TETROMINOES
            .len()
            .min(source_piece_count.saturating_add(1))
            .saturating_add(1);
        let workspace_storage_bytes = (active_workers as u128)
            .checked_mul(workspace_key_capacity as u128)?
            .checked_mul(core::mem::size_of::<PieceMultisetKey>() as u128)?;
        let worker_scratch_bytes = sequence_storage_bytes.checked_add(workspace_storage_bytes)?;
        let worker_coordination_bytes = if parallel {
            let handle_bytes = core::mem::size_of::<
                std::thread::ScopedJoinHandle<
                    'static,
                    Result<Vec<PatternMultisetRecord>, PackingMultisetBuildError>,
                >,
            >() as u128;
            let shard_owner_bytes = core::mem::size_of::<Vec<PatternMultisetRecord>>() as u128;
            (active_workers as u128).checked_mul(handle_bytes.checked_add(shard_owner_bytes)?)?
        } else {
            0
        };
        let worker_phase = record_storage_bytes
            .checked_add(worker_stack_bytes)?
            .checked_add(worker_scratch_bytes)?
            .checked_add(worker_coordination_bytes)?;
        let merge_phase = if parallel {
            record_storage_bytes
                .checked_mul(2)?
                .checked_add(worker_coordination_bytes)?
        } else {
            record_storage_bytes
        };
        let pattern_bitset_storage_bytes = PatternBitSet::checked_shared_storage_upper_bound(
            pattern_count,
            max_group_count,
            membership_count,
        )?;
        let pattern_bitset_construction_bytes =
            PatternBitSet::checked_shared_construction_upper_bound(
                pattern_count,
                max_group_count,
                membership_count,
            )?;
        let finalization_phase = record_storage_bytes
            .checked_add(group_storage_bytes)?
            .checked_add(pattern_bitset_construction_bytes)?;
        Some(PackingMultisetFamilyBuildProjection {
            worker_count: active_workers,
            max_group_count,
            max_record_count: membership_count,
            record_storage_bytes,
            group_storage_bytes,
            pattern_bitset_storage_bytes,
            symbolic_frontier_bytes: 0,
            worker_stack_bytes,
            worker_coordination_bytes,
            required_peak_bytes: worker_phase.max(merge_phase).max(finalization_phase),
        })
    }

    pub fn packing_multiset_family_for_execution_with_workers_and_memory_limit(
        &self,
        placed_piece_count: usize,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
        hold_projection: PackingHoldProjection,
        requested_workers: usize,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<PackingMultisetFamily, PackingMultisetBuildError> {
        let projection = self
            .checked_packing_multiset_family_build_projection(
                placed_piece_count,
                initial_hold,
                hold_enabled,
                hold_projection,
                requested_workers,
            )
            .ok_or(PackingMultisetBuildError::ProjectionOverflow)?;
        let required_memory_bytes = already_retained_bytes
            .checked_add(projection.required_peak_bytes)
            .ok_or(PackingMultisetBuildError::ProjectionOverflow)?;
        if required_memory_bytes > max_memory_bytes {
            return Err(PackingMultisetBuildError::MemoryCapacityExceeded {
                required_memory_bytes,
                max_memory_bytes,
            });
        }
        let limit = FamilyMemoryLimit {
            already_retained_bytes,
            max_memory_bytes,
        };
        self.packing_multiset_family_for_execution_with_workers_bounded(
            placed_piece_count,
            initial_hold,
            hold_enabled,
            hold_projection,
            requested_workers,
            Some(limit),
        )
    }

    pub fn packing_multiset_family_for_execution(
        &self,
        placed_piece_count: usize,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
        hold_projection: PackingHoldProjection,
    ) -> PackingMultisetFamily {
        match hold_projection {
            PackingHoldProjection::PreserveFinalHoldLanguage => {
                self.packing_multiset_family(placed_piece_count, initial_hold, hold_enabled)
            }
            PackingHoldProjection::ReleaseHeldAtTerminal if hold_enabled => {
                self.terminally_released_multiset_family(placed_piece_count, initial_hold)
            }
            PackingHoldProjection::ReleaseHeldAtTerminal => empty_multiset_family(),
        }
    }

    pub fn packing_multiset_family_for_execution_with_workers(
        &self,
        placed_piece_count: usize,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
        hold_projection: PackingHoldProjection,
        requested_workers: usize,
    ) -> Result<PackingMultisetFamily, PackingMultisetBuildError> {
        self.packing_multiset_family_for_execution_with_workers_bounded(
            placed_piece_count,
            initial_hold,
            hold_enabled,
            hold_projection,
            requested_workers,
            None,
        )
    }

    fn packing_multiset_family_for_execution_with_workers_bounded(
        &self,
        placed_piece_count: usize,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
        hold_projection: PackingHoldProjection,
        requested_workers: usize,
        memory_limit: Option<FamilyMemoryLimit>,
    ) -> Result<PackingMultisetFamily, PackingMultisetBuildError> {
        match hold_projection {
            PackingHoldProjection::PreserveFinalHoldLanguage => self
                .packing_multiset_family_with_workers_bounded(
                    placed_piece_count,
                    initial_hold,
                    hold_enabled,
                    requested_workers,
                    memory_limit,
                ),
            PackingHoldProjection::ReleaseHeldAtTerminal if hold_enabled => self
                .terminally_released_multiset_family_with_workers_bounded(
                    placed_piece_count,
                    initial_hold,
                    requested_workers,
                    memory_limit,
                ),
            PackingHoldProjection::ReleaseHeldAtTerminal => Ok(empty_multiset_family()),
        }
    }

    pub fn packing_multiset_family(
        &self,
        placed_piece_count: usize,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
    ) -> PackingMultisetFamily {
        if let Some(family) =
            self.symbolic_standard_bag_family(placed_piece_count, initial_hold, hold_enabled)
        {
            return family;
        }
        let groups = self.packing_multiset_groups(placed_piece_count, initial_hold, hold_enabled);
        let envelope = groups
            .iter()
            .fold(PieceMultisetKey::default(), |envelope, group| {
                envelope.componentwise_max(group.key())
            });
        PackingMultisetFamily {
            envelope,
            groups,
            membership_kind: PackingPatternMembershipKind::ExactMaterialized,
        }
    }

    pub fn packing_multiset_family_with_workers(
        &self,
        placed_piece_count: usize,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
        requested_workers: usize,
    ) -> Result<PackingMultisetFamily, PackingMultisetBuildError> {
        self.packing_multiset_family_with_workers_bounded(
            placed_piece_count,
            initial_hold,
            hold_enabled,
            requested_workers,
            None,
        )
    }

    fn packing_multiset_family_with_workers_bounded(
        &self,
        placed_piece_count: usize,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
        requested_workers: usize,
        memory_limit: Option<FamilyMemoryLimit>,
    ) -> Result<PackingMultisetFamily, PackingMultisetBuildError> {
        if let Some(family) = self.symbolic_standard_bag_family_bounded(
            placed_piece_count,
            initial_hold,
            hold_enabled,
            memory_limit,
        )? {
            return Ok(family);
        }
        let groups = self.packing_multiset_groups_with_workers(
            placed_piece_count,
            initial_hold,
            hold_enabled,
            requested_workers,
            memory_limit,
        )?;
        let envelope = groups
            .iter()
            .fold(PieceMultisetKey::default(), |envelope, group| {
                envelope.componentwise_max(group.key())
            });
        Ok(PackingMultisetFamily {
            envelope,
            groups,
            membership_kind: PackingPatternMembershipKind::ExactMaterialized,
        })
    }

    fn terminally_released_multiset_family(
        &self,
        placed_piece_count: usize,
        initial_hold: HoldAutomatonState,
    ) -> PackingMultisetFamily {
        let Some(source_piece_count) =
            placed_piece_count.checked_sub(usize::from(initial_hold.hold_piece().is_some()))
        else {
            return empty_multiset_family();
        };
        let initial_piece = initial_hold.hold_piece();
        let source_only = HoldAutomatonState::new(
            initial_hold.piece_source_id(),
            initial_hold.cursor(),
            None,
            initial_hold.bag_epoch(),
            initial_hold.bag_remainder_key(),
            initial_hold.provenance(),
        );
        append_initial_hold_piece(
            self.packing_multiset_family(source_piece_count, source_only, false),
            initial_piece,
        )
    }

    fn terminally_released_multiset_family_with_workers_bounded(
        &self,
        placed_piece_count: usize,
        initial_hold: HoldAutomatonState,
        requested_workers: usize,
        memory_limit: Option<FamilyMemoryLimit>,
    ) -> Result<PackingMultisetFamily, PackingMultisetBuildError> {
        let Some(source_piece_count) =
            placed_piece_count.checked_sub(usize::from(initial_hold.hold_piece().is_some()))
        else {
            return Ok(empty_multiset_family());
        };
        let initial_piece = initial_hold.hold_piece();
        let source_only = HoldAutomatonState::new(
            initial_hold.piece_source_id(),
            initial_hold.cursor(),
            None,
            initial_hold.bag_epoch(),
            initial_hold.bag_remainder_key(),
            initial_hold.provenance(),
        );
        self.packing_multiset_family_with_workers_bounded(
            source_piece_count,
            source_only,
            false,
            requested_workers,
            memory_limit,
        )
        .map(|family| append_initial_hold_piece(family, initial_piece))
    }

    fn symbolic_standard_bag_family(
        &self,
        placed_piece_count: usize,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
    ) -> Option<PackingMultisetFamily> {
        self.symbolic_standard_bag_family_bounded(
            placed_piece_count,
            initial_hold,
            hold_enabled,
            None,
        )
        .ok()
        .flatten()
    }

    fn symbolic_standard_bag_family_bounded(
        &self,
        placed_piece_count: usize,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
        memory_limit: Option<FamilyMemoryLimit>,
    ) -> Result<Option<PackingMultisetFamily>, PackingMultisetBuildError> {
        if !matches!(
            self.structure(),
            MaterializedPatternUniverseStructure::Standard7BagLexicographic { .. }
        ) {
            return Ok(None);
        }
        let keys = match memory_limit {
            Some(limit) => {
                let (keys, report) = reachable_bag_multisets_with_memory_limit(
                    &PieceKind::STANDARD_TETROMINOES,
                    placed_piece_count,
                    initial_hold,
                    hold_enabled,
                    limit.already_retained_bytes,
                    limit.max_memory_bytes,
                )
                .map_err(packing_bag_multiset_error)?;
                debug_assert!(
                    checked_reachable_bag_multiset_peak_upper_bound(placed_piece_count)
                        .is_some_and(|upper| report.peak_bytes <= upper)
                );
                keys
            }
            None => reachable_bag_multisets(
                &PieceKind::STANDARD_TETROMINOES,
                placed_piece_count,
                initial_hold,
                hold_enabled,
            )
            .map_err(PackingMultisetBuildError::BagProjection)?,
        };
        if keys.is_empty() {
            return Ok(None);
        }
        let all_patterns = match memory_limit {
            Some(limit) => {
                let live_key_bytes = (keys.capacity() as u128)
                    .checked_mul(core::mem::size_of::<PieceMultisetKey>() as u128)
                    .and_then(|bytes| limit.already_retained_bytes.checked_add(bytes))
                    .ok_or(PackingMultisetBuildError::ProjectionOverflow)?;
                Arc::new(
                    PatternBitSet::all_with_memory_limit(
                        self.pattern_count(),
                        live_key_bytes,
                        limit.max_memory_bytes,
                    )
                    .map_err(PackingMultisetBuildError::PatternBitSet)?,
                )
            }
            None => Arc::new(PatternBitSet::all(self.pattern_count())),
        };
        let groups = keys
            .into_iter()
            .map(|key| PackingMultisetGroup {
                key,
                pattern_bits: Arc::clone(&all_patterns),
            })
            .collect::<Vec<_>>();
        let envelope = groups
            .iter()
            .fold(PieceMultisetKey::default(), |envelope, group| {
                envelope.componentwise_max(group.key())
            });
        Ok(Some(PackingMultisetFamily {
            envelope,
            groups,
            membership_kind: PackingPatternMembershipKind::ExactSymbolicStandardBag,
        }))
    }

    pub fn packing_multiset_groups(
        &self,
        placed_piece_count: usize,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
    ) -> Vec<PackingMultisetGroup> {
        let mut grouped = MultisetGroupAccumulator::new(self.pattern_count());
        let mut workspace = ReachableMultisetWorkspace::new(placed_piece_count);
        let mut sequence = Vec::new();
        for pattern_index in 0..self.pattern_count() {
            let pattern_id = PatternId::new(pattern_index);
            self.write_sequence_at(pattern_index, &mut sequence);
            for &key in workspace.reachable_multisets(
                &sequence,
                placed_piece_count,
                initial_hold,
                hold_enabled,
            ) {
                grouped.record(key, pattern_id);
            }
        }
        grouped.finish()
    }

    fn packing_multiset_groups_with_workers(
        &self,
        placed_piece_count: usize,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
        requested_workers: usize,
        memory_limit: Option<FamilyMemoryLimit>,
    ) -> Result<Vec<PackingMultisetGroup>, PackingMultisetBuildError> {
        const PARALLEL_PATTERN_THRESHOLD: usize = 256;
        let worker_count = effective_multiset_worker_count(requested_workers, self.pattern_count());
        let max_sequence_len = self
            .max_sequence_len_for_projection()
            .ok_or(PackingMultisetBuildError::ProjectionOverflow)?;
        if worker_count == 1 || self.pattern_count() < PARALLEL_PATTERN_THRESHOLD {
            let mut records = collect_pattern_multiset_records(
                self,
                0,
                self.pattern_count(),
                placed_piece_count,
                initial_hold,
                hold_enabled,
                max_sequence_len,
            )?;
            records.sort_unstable();
            records.dedup();
            return groups_from_sorted_records(records, self.pattern_count(), memory_limit);
        }

        let shards = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            handles.try_reserve_exact(worker_count).map_err(|_| {
                PackingMultisetBuildError::AllocationFailed {
                    required_memory_bytes: (worker_count as u128).saturating_mul(
                        core::mem::size_of::<
                            std::thread::ScopedJoinHandle<
                                'static,
                                Result<Vec<PatternMultisetRecord>, PackingMultisetBuildError>,
                            >,
                        >() as u128,
                    ),
                }
            })?;
            for worker_index in 0..worker_count {
                let begin = self.pattern_count() * worker_index / worker_count;
                let end = self.pattern_count() * (worker_index + 1) / worker_count;
                handles.push(
                    std::thread::Builder::new()
                        .stack_size(MULTISET_WORKER_STACK_BYTES)
                        .spawn_scoped(scope, move || {
                            collect_pattern_multiset_records(
                                self,
                                begin,
                                end,
                                placed_piece_count,
                                initial_hold,
                                hold_enabled,
                                max_sequence_len,
                            )
                        })
                        .map_err(|_| PackingMultisetBuildError::WorkerSpawnFailed)?,
                );
            }

            let mut shards = Vec::new();
            shards.try_reserve_exact(worker_count).map_err(|_| {
                PackingMultisetBuildError::AllocationFailed {
                    required_memory_bytes: (worker_count as u128)
                        .saturating_mul(core::mem::size_of::<Vec<PatternMultisetRecord>>() as u128),
                }
            })?;
            for handle in handles {
                shards.push(
                    handle
                        .join()
                        .map_err(|_| PackingMultisetBuildError::WorkerPanicked)??,
                );
            }
            Ok::<_, PackingMultisetBuildError>(shards)
        })?;

        let record_count = shards
            .iter()
            .try_fold(0usize, |count, shard| count.checked_add(shard.len()))
            .ok_or(PackingMultisetBuildError::ProjectionOverflow)?;
        let mut records = Vec::new();
        records.try_reserve_exact(record_count).map_err(|_| {
            PackingMultisetBuildError::AllocationFailed {
                required_memory_bytes: (record_count as u128)
                    .saturating_mul(core::mem::size_of::<PatternMultisetRecord>() as u128),
            }
        })?;
        for mut shard in shards {
            records.append(&mut shard);
        }
        records.sort_unstable();
        records.dedup();
        groups_from_sorted_records(records, self.pattern_count(), memory_limit)
    }

    fn max_sequence_len_for_projection(&self) -> Option<usize> {
        match self.structure() {
            MaterializedPatternUniverseStructure::Standard7BagLexicographic { sequence_len }
            | MaterializedPatternUniverseStructure::ObservedStandard7BagLexicographic {
                sequence_len,
                ..
            }
            | MaterializedPatternUniverseStructure::FactorizedQueueExpression { sequence_len } => {
                Some(sequence_len as usize)
            }
            MaterializedPatternUniverseStructure::Explicit => (0..self.pattern_count())
                .try_fold(0usize, |longest, index| {
                    Some(longest.max(self.sequence_len_at(index)))
                }),
        }
    }
}

fn effective_multiset_worker_count(requested_workers: usize, pattern_count: usize) -> usize {
    let available_workers =
        clearra_core_domain::runtime_cpu_capacity::CpuCapacity::current().hard_limit();
    requested_workers
        .max(1)
        .min(available_workers)
        .min(pattern_count.max(1))
}

fn checked_multiset_key_count(total_piece_count: usize) -> Option<u128> {
    let total = total_piece_count as u128;
    (1_u128..=6).try_fold(1_u128, |count, divisor| {
        count
            .checked_mul(total.checked_add(divisor)?)?
            .checked_div(divisor)
    })
}

fn collect_pattern_multiset_records(
    universe: &MaterializedPatternUniverse,
    begin: usize,
    end: usize,
    placed_piece_count: usize,
    initial_hold: HoldAutomatonState,
    hold_enabled: bool,
    max_sequence_len: usize,
) -> Result<Vec<PatternMultisetRecord>, PackingMultisetBuildError> {
    let max_memberships = if hold_enabled {
        usize::try_from(MAX_PATTERN_MEMBERSHIPS_WITH_HOLD)
            .map_err(|_| PackingMultisetBuildError::ProjectionOverflow)?
    } else {
        1
    };
    let max_records = end
        .checked_sub(begin)
        .and_then(|patterns| patterns.checked_mul(max_memberships))
        .ok_or(PackingMultisetBuildError::ProjectionOverflow)?;
    let required_record_bytes = (max_records as u128)
        .checked_mul(core::mem::size_of::<PatternMultisetRecord>() as u128)
        .ok_or(PackingMultisetBuildError::ProjectionOverflow)?;
    let mut records = Vec::new();
    records.try_reserve_exact(max_records).map_err(|_| {
        PackingMultisetBuildError::AllocationFailed {
            required_memory_bytes: required_record_bytes,
        }
    })?;
    let mut workspace = ReachableMultisetWorkspace::new(placed_piece_count);
    let mut sequence = Vec::new();
    sequence.try_reserve_exact(max_sequence_len).map_err(|_| {
        PackingMultisetBuildError::AllocationFailed {
            required_memory_bytes: (max_sequence_len as u128)
                .saturating_mul(core::mem::size_of::<PieceKind>() as u128),
        }
    })?;
    for pattern_index in begin..end {
        universe.write_sequence_at(pattern_index, &mut sequence);
        for &key in
            workspace.reachable_multisets(&sequence, placed_piece_count, initial_hold, hold_enabled)
        {
            records.push(PatternMultisetRecord { key, pattern_index });
        }
    }
    Ok(records)
}

fn groups_from_sorted_records(
    records: Vec<PatternMultisetRecord>,
    pattern_count: usize,
    memory_limit: Option<FamilyMemoryLimit>,
) -> Result<Vec<PackingMultisetGroup>, PackingMultisetBuildError> {
    let group_count = usize::from(!records.is_empty())
        .checked_add(
            records
                .windows(2)
                .filter(|pair| pair[0].key != pair[1].key)
                .count(),
        )
        .ok_or(PackingMultisetBuildError::ProjectionOverflow)?;
    let mut groups = Vec::new();
    groups.try_reserve_exact(group_count).map_err(|_| {
        PackingMultisetBuildError::AllocationFailed {
            required_memory_bytes: (group_count as u128)
                .saturating_mul(core::mem::size_of::<PackingMultisetGroup>() as u128),
        }
    })?;
    let records_bytes = (records.capacity() as u128)
        .checked_mul(core::mem::size_of::<PatternMultisetRecord>() as u128)
        .ok_or(PackingMultisetBuildError::ProjectionOverflow)?;
    let groups_bytes = (groups.capacity() as u128)
        .checked_mul(core::mem::size_of::<PackingMultisetGroup>() as u128)
        .ok_or(PackingMultisetBuildError::ProjectionOverflow)?;
    let fixed_live_bytes = records_bytes
        .checked_add(groups_bytes)
        .ok_or(PackingMultisetBuildError::ProjectionOverflow)?;
    let mut shared_bitset_bytes = 0_u128;
    let mut begin = 0usize;
    while begin < records.len() {
        let key = records[begin].key;
        let mut end = begin + 1;
        while end < records.len() && records[end].key == key {
            end += 1;
        }
        let id_count = end - begin;
        let projection =
            PatternBitSet::checked_allocation_projection(pattern_count, id_count, id_count)
                .ok_or(PackingMultisetBuildError::ProjectionOverflow)?;
        let construction_base = fixed_live_bytes
            .checked_add(shared_bitset_bytes)
            .ok_or(PackingMultisetBuildError::ProjectionOverflow)?;
        ensure_family_limit(
            memory_limit,
            construction_base,
            projection.constructor_peak_bytes,
        )?;
        let mut pattern_ids = Vec::new();
        pattern_ids.try_reserve_exact(id_count).map_err(|_| {
            PackingMultisetBuildError::AllocationFailed {
                required_memory_bytes: construction_base
                    .saturating_add(projection.constructor_peak_bytes),
            }
        })?;
        for record in &records[begin..end] {
            pattern_ids.push(
                u32::try_from(record.pattern_index)
                    .map_err(|_| PackingMultisetBuildError::ProjectionOverflow)?,
            );
        }
        let pattern_bits = match memory_limit {
            Some(limit) => PatternBitSet::from_pattern_indices_with_memory_limit(
                pattern_count,
                pattern_ids,
                limit
                    .already_retained_bytes
                    .checked_add(construction_base)
                    .ok_or(PackingMultisetBuildError::ProjectionOverflow)?,
                limit.max_memory_bytes,
            )
            .map_err(PackingMultisetBuildError::PatternBitSet)?,
            None => PatternBitSet::from_pattern_indices(pattern_count, pattern_ids)
                .map_err(PatternBitSetAllocationError::InvalidPattern)
                .map_err(PackingMultisetBuildError::PatternBitSet)?,
        };
        shared_bitset_bytes = shared_bitset_bytes
            .checked_add(
                pattern_bits
                    .checked_shared_retained_bytes()
                    .ok_or(PackingMultisetBuildError::ProjectionOverflow)?,
            )
            .ok_or(PackingMultisetBuildError::ProjectionOverflow)?;
        groups.push(PackingMultisetGroup {
            key,
            pattern_bits: Arc::new(pattern_bits),
        });
        begin = end;
    }
    Ok(groups)
}

fn ensure_family_limit(
    memory_limit: Option<FamilyMemoryLimit>,
    live_bytes: u128,
    future_bytes: u128,
) -> Result<(), PackingMultisetBuildError> {
    let Some(limit) = memory_limit else {
        return Ok(());
    };
    let required_memory_bytes = limit
        .already_retained_bytes
        .checked_add(live_bytes)
        .and_then(|bytes| bytes.checked_add(future_bytes))
        .ok_or(PackingMultisetBuildError::ProjectionOverflow)?;
    if required_memory_bytes <= limit.max_memory_bytes {
        return Ok(());
    }
    Err(PackingMultisetBuildError::MemoryCapacityExceeded {
        required_memory_bytes,
        max_memory_bytes: limit.max_memory_bytes,
    })
}

fn packing_bag_multiset_error(error: BoundedBagMultisetError) -> PackingMultisetBuildError {
    match error {
        BoundedBagMultisetError::Projection(error) => {
            PackingMultisetBuildError::BagProjection(error)
        }
        BoundedBagMultisetError::ProjectionOverflow => {
            PackingMultisetBuildError::ProjectionOverflow
        }
        BoundedBagMultisetError::AllocationFailed {
            required_memory_bytes,
        } => PackingMultisetBuildError::AllocationFailed {
            required_memory_bytes,
        },
        BoundedBagMultisetError::MemoryCapacityExceeded {
            required_memory_bytes,
            max_memory_bytes,
        } => PackingMultisetBuildError::MemoryCapacityExceeded {
            required_memory_bytes,
            max_memory_bytes,
        },
    }
}

fn empty_multiset_family() -> PackingMultisetFamily {
    PackingMultisetFamily {
        envelope: PieceMultisetKey::default(),
        groups: Vec::new(),
        membership_kind: PackingPatternMembershipKind::ExactMaterialized,
    }
}

fn append_initial_hold_piece(
    mut family: PackingMultisetFamily,
    initial_piece: Option<PieceKind>,
) -> PackingMultisetFamily {
    let Some(initial_piece) = initial_piece else {
        return family;
    };
    for group in &mut family.groups {
        group.key.push(initial_piece);
    }
    family
        .groups
        .sort_unstable_by_key(PackingMultisetGroup::key);
    family.envelope = family
        .groups
        .iter()
        .fold(PieceMultisetKey::default(), |envelope, group| {
            envelope.componentwise_max(group.key())
        });
    family
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackingMultisetBuildError {
    WorkerPanicked,
    WorkerSpawnFailed,
    ProjectionOverflow,
    AllocationFailed {
        required_memory_bytes: u128,
    },
    MemoryCapacityExceeded {
        required_memory_bytes: u128,
        max_memory_bytes: u128,
    },
    PatternBitSet(PatternBitSetAllocationError),
    BagProjection(crate::execution_automaton::SupplyExecutionError),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PatternMultisetRecord {
    key: PieceMultisetKey,
    pattern_index: usize,
}

const EMPTY_GROUP_INDEX: u32 = u32::MAX;
const INITIAL_GROUP_BUCKET_COUNT: usize = 16;

struct MultisetGroupAccumulator {
    pattern_count: usize,
    group_keys: Vec<PieceMultisetKey>,
    group_pattern_ids: Vec<Vec<u32>>,
    bucket_heads: Vec<u32>,
    next_indices: Vec<u32>,
}

impl MultisetGroupAccumulator {
    fn new(pattern_count: usize) -> Self {
        Self {
            pattern_count,
            group_keys: Vec::new(),
            group_pattern_ids: Vec::new(),
            bucket_heads: vec![EMPTY_GROUP_INDEX; INITIAL_GROUP_BUCKET_COUNT],
            next_indices: Vec::new(),
        }
    }

    fn record(&mut self, key: PieceMultisetKey, pattern_id: PatternId) {
        self.grow_if_needed();
        let bucket = multiset_bucket(key, self.bucket_heads.len());
        let mut index = self.bucket_heads[bucket];
        while index != EMPTY_GROUP_INDEX {
            if self.group_keys[index as usize] == key {
                self.group_pattern_ids[index as usize].push(
                    u32::try_from(pattern_id.index())
                        .expect("materialized pattern id fits the compact product index"),
                );
                return;
            }
            index = self.next_indices[index as usize];
        }

        let index =
            u32::try_from(self.group_keys.len()).expect("piece multiset group count fits u32");
        self.group_keys.push(key);
        self.group_pattern_ids
            .push(vec![u32::try_from(pattern_id.index()).expect(
                "materialized pattern id fits the compact product index",
            )]);
        self.next_indices.push(self.bucket_heads[bucket]);
        self.bucket_heads[bucket] = index;
    }

    fn finish(self) -> Vec<PackingMultisetGroup> {
        let mut groups = self
            .group_keys
            .into_iter()
            .zip(self.group_pattern_ids)
            .map(|(key, pattern_ids)| sparse_group(key, self.pattern_count, pattern_ids))
            .collect::<Vec<_>>();
        groups.sort_unstable_by_key(PackingMultisetGroup::key);
        groups
    }

    fn grow_if_needed(&mut self) {
        if self.group_keys.len().saturating_mul(4) < self.bucket_heads.len().saturating_mul(3) {
            return;
        }
        let new_count = self
            .bucket_heads
            .len()
            .checked_mul(2)
            .expect("piece multiset group bucket count overflow");
        let mut grown = vec![EMPTY_GROUP_INDEX; new_count];
        for (index, key) in self.group_keys.iter().copied().enumerate() {
            let bucket = multiset_bucket(key, new_count);
            self.next_indices[index] = grown[bucket];
            grown[bucket] = index as u32;
        }
        self.bucket_heads = grown;
    }
}

fn sparse_group(
    key: PieceMultisetKey,
    pattern_count: usize,
    mut pattern_ids: Vec<u32>,
) -> PackingMultisetGroup {
    pattern_ids.sort_unstable();
    pattern_ids.dedup();
    let pattern_bits = PatternBitSet::from_pattern_indices(pattern_count, pattern_ids)
        .expect("materialized pattern ids belong to their universe");
    PackingMultisetGroup {
        key,
        pattern_bits: Arc::new(pattern_bits),
    }
}

fn multiset_bucket(key: PieceMultisetKey, bucket_count: usize) -> usize {
    debug_assert!(bucket_count.is_power_of_two());
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for count in key.counts.into_iter().chain([key.total_count]) {
        hash ^= u64::from(count);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (hash as usize) & (bucket_count - 1)
}

const fn piece_index(piece: PieceKind) -> usize {
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

#[cfg(test)]
mod tests {
    use clearra_core_domain::probability::probability_value::ProbabilityValue;
    use clearra_coverage::universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    };

    use crate::{
        hold_automaton::SupplyProvenanceId, pattern_universe::PatternUniverseMaterializer,
        piece_source::PieceSourceId,
    };

    use super::*;

    #[test]
    fn retained_family_measurement_counts_shared_pattern_storage_once() {
        let shared = Arc::new(PatternBitSet::all(129));
        let family = PackingMultisetFamily {
            envelope: PieceMultisetKey::from_pieces([PieceKind::I]),
            groups: vec![
                PackingMultisetGroup {
                    key: PieceMultisetKey::from_pieces([PieceKind::I]),
                    pattern_bits: Arc::clone(&shared),
                },
                PackingMultisetGroup {
                    key: PieceMultisetKey::from_pieces([PieceKind::O]),
                    pattern_bits: Arc::clone(&shared),
                },
            ],
            membership_kind: PackingPatternMembershipKind::ExactSymbolicStandardBag,
        };
        let group_bytes = family.groups.capacity() * core::mem::size_of::<PackingMultisetGroup>();
        assert_eq!(
            family.checked_retained_bytes(),
            shared
                .checked_shared_retained_bytes()
                .and_then(|shared_bytes| (group_bytes as u128).checked_add(shared_bytes))
        );
    }

    #[test]
    fn family_projection_counts_records_groups_and_private_bitset_storage_fieldwise() {
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(81),
            PatternWeightModelId::new(81),
            vec![vec![PieceKind::I], vec![PieceKind::O]],
            vec![
                ProbabilityValue::new(0.5).expect("half probability"),
                ProbabilityValue::new(0.5).expect("half probability"),
            ],
            2,
            true,
            None,
        )
        .expect("two-pattern universe");
        let initial_hold = HoldAutomatonState::new(
            PieceSourceId::new(81),
            0,
            None,
            0,
            0,
            SupplyProvenanceId(81),
        );

        let projection = universe
            .checked_packing_multiset_family_build_projection(
                1,
                initial_hold,
                false,
                PackingHoldProjection::PreserveFinalHoldLanguage,
                4,
            )
            .expect("checked projection");

        assert_eq!(projection.worker_count, 1);
        assert_eq!(projection.max_record_count, 2);
        assert_eq!(projection.max_group_count, 2);
        assert_eq!(
            projection.record_storage_bytes,
            2 * core::mem::size_of::<PatternMultisetRecord>() as u128
        );
        assert_eq!(
            projection.group_storage_bytes,
            2 * core::mem::size_of::<PackingMultisetGroup>() as u128
        );
        assert_eq!(
            projection.pattern_bitset_storage_bytes,
            PatternBitSet::checked_shared_storage_upper_bound(2, 2, 2).expect("bitset storage")
        );
        assert_eq!(projection.worker_stack_bytes, 0);
        assert_eq!(projection.worker_coordination_bytes, 0);
    }

    #[test]
    fn bounded_family_rejects_one_byte_short_and_matches_unbounded_at_exact_cap() {
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(82),
            PatternWeightModelId::new(82),
            vec![vec![PieceKind::I], vec![PieceKind::O]],
            vec![
                ProbabilityValue::new(0.5).expect("half probability"),
                ProbabilityValue::new(0.5).expect("half probability"),
            ],
            2,
            true,
            None,
        )
        .expect("two-pattern universe");
        let initial_hold = HoldAutomatonState::new(
            PieceSourceId::new(82),
            0,
            None,
            0,
            0,
            SupplyProvenanceId(82),
        );
        let projection = universe
            .checked_packing_multiset_family_build_projection(
                1,
                initial_hold,
                false,
                PackingHoldProjection::PreserveFinalHoldLanguage,
                1,
            )
            .expect("checked projection");
        let already_retained = 17;
        assert_eq!(
            universe.packing_multiset_family_for_execution_with_workers_and_memory_limit(
                1,
                initial_hold,
                false,
                PackingHoldProjection::PreserveFinalHoldLanguage,
                1,
                already_retained,
                already_retained + projection.required_peak_bytes - 1,
            ),
            Err(PackingMultisetBuildError::MemoryCapacityExceeded {
                required_memory_bytes: already_retained + projection.required_peak_bytes,
                max_memory_bytes: already_retained + projection.required_peak_bytes - 1,
            })
        );
        let bounded = universe
            .packing_multiset_family_for_execution_with_workers_and_memory_limit(
                1,
                initial_hold,
                false,
                PackingHoldProjection::PreserveFinalHoldLanguage,
                1,
                already_retained,
                already_retained + projection.required_peak_bytes,
            )
            .expect("exact cap admits family");
        assert_eq!(
            bounded,
            universe
                .packing_multiset_family_for_execution_with_workers(
                    1,
                    initial_hold,
                    false,
                    PackingHoldProjection::PreserveFinalHoldLanguage,
                    1,
                )
                .expect("unbounded reference")
        );
    }

    #[test]
    fn multiset_projection_checks_combinatorial_and_u128_overflow() {
        assert_eq!(checked_multiset_key_count(0), Some(1));
        assert_eq!(checked_multiset_key_count(1), Some(7));
        assert_eq!(checked_multiset_key_count(2), Some(28));
        assert_eq!(checked_multiset_key_count(usize::MAX), None);
        assert!(PatternBitSet::checked_shared_construction_upper_bound(
            usize::MAX,
            u128::MAX,
            u128::MAX
        )
        .is_none());
    }

    #[test]
    fn parallel_pattern_preprocessing_matches_serial_exactly() {
        let universe = PatternUniverseMaterializer::standard_7_bag(6, 5_040, 17)
            .expect("complete six-draw bag universe");
        let initial_hold = HoldAutomatonState::new(
            PieceSourceId::new(17),
            0,
            None,
            0,
            0,
            SupplyProvenanceId(17),
        );

        let serial = universe.packing_multiset_family(5, initial_hold, true);
        let parallel = universe
            .packing_multiset_family_with_workers(5, initial_hold, true, 4)
            .expect("parallel preprocessing");

        assert_eq!(parallel, serial);
    }

    #[test]
    fn truncated_standard_bag_keeps_exact_symbolic_p7p3_multisets() {
        let universe = PatternUniverseMaterializer::standard_7_bag(10, 64, 17)
            .expect("truncated ten-draw bag universe");
        assert!(!universe.complete());
        let initial_hold = HoldAutomatonState::new(
            PieceSourceId::new(17),
            0,
            None,
            0,
            0,
            SupplyProvenanceId(17),
        );

        let family = universe.packing_multiset_family(10, initial_hold, false);

        assert_eq!(
            family.membership_kind(),
            PackingPatternMembershipKind::ExactSymbolicStandardBag
        );
        assert_eq!(family.groups().len(), 35);
        assert!(family.groups().iter().all(|group| {
            let counts = group.key().counts();
            group.key().total_count() == 10
                && counts.iter().all(|count| (1..=2).contains(count))
                && counts.iter().filter(|count| **count == 2).count() == 3
        }));
    }

    #[test]
    fn terminal_projection_places_every_finite_source_piece_with_an_empty_initial_hold() {
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(1),
            PatternWeightModelId::new(1),
            vec![vec![PieceKind::I, PieceKind::O, PieceKind::T]],
            vec![ProbabilityValue::ONE],
            1,
            true,
            None,
        )
        .expect("finite universe");
        let initial_hold =
            HoldAutomatonState::new(PieceSourceId::new(1), 0, None, 0, 0, SupplyProvenanceId(1));

        let family = universe.packing_multiset_family_for_execution(
            3,
            initial_hold,
            true,
            PackingHoldProjection::ReleaseHeldAtTerminal,
        );

        assert_eq!(family.groups().len(), 1);
        let key = family.groups()[0].key();
        assert_eq!(key.total_count(), 3);
        assert_eq!(key.count(PieceKind::I), 1);
        assert_eq!(key.count(PieceKind::O), 1);
        assert_eq!(key.count(PieceKind::T), 1);
    }

    #[test]
    fn terminal_projection_preserves_an_occupied_initial_hold_as_placed_inventory() {
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(2),
            PatternWeightModelId::new(2),
            vec![vec![PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::Z]],
            vec![ProbabilityValue::ONE],
            2,
            true,
            None,
        )
        .expect("finite universe");
        let initial_hold = HoldAutomatonState::new(
            PieceSourceId::new(2),
            0,
            Some(PieceKind::S),
            0,
            0,
            SupplyProvenanceId(2),
        );

        let serial = universe.packing_multiset_family_for_execution(
            5,
            initial_hold,
            true,
            PackingHoldProjection::ReleaseHeldAtTerminal,
        );
        let parallel = universe
            .packing_multiset_family_for_execution_with_workers(
                5,
                initial_hold,
                true,
                PackingHoldProjection::ReleaseHeldAtTerminal,
                4,
            )
            .expect("parallel terminal projection");

        assert_eq!(parallel, serial);
        assert_eq!(serial.groups().len(), 1);
        let key = serial.groups()[0].key();
        assert_eq!(key.total_count(), 5);
        assert_eq!(key.count(PieceKind::S), 1);
        assert_eq!(key.count(PieceKind::I), 1);
        assert_eq!(key.count(PieceKind::O), 1);
        assert_eq!(key.count(PieceKind::T), 1);
        assert_eq!(key.count(PieceKind::Z), 1);
    }

    #[test]
    fn terminal_projection_is_fail_closed_when_hold_is_disabled() {
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(3),
            PatternWeightModelId::new(3),
            vec![vec![PieceKind::I]],
            vec![ProbabilityValue::ONE],
            3,
            true,
            None,
        )
        .expect("finite universe");
        let initial_hold = HoldAutomatonState::new(
            PieceSourceId::new(3),
            0,
            Some(PieceKind::O),
            0,
            0,
            SupplyProvenanceId(3),
        );

        let family = universe.packing_multiset_family_for_execution(
            2,
            initial_hold,
            false,
            PackingHoldProjection::ReleaseHeldAtTerminal,
        );

        assert!(family.groups().is_empty());
    }
}
