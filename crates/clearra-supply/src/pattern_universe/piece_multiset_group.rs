use std::sync::Arc;

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_coverage::pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId};

use crate::hold_automaton::HoldAutomatonState;

use super::hold_multiset_reachability::ReachableMultisetWorkspace;
use super::materialized_pattern_universe::{
    MaterializedPatternUniverse, MaterializedPatternUniverseStructure,
};
use super::reachable_bag_multisets;

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
        if let Some(family) =
            self.symbolic_standard_bag_family(placed_piece_count, initial_hold, hold_enabled)
        {
            return Ok(family);
        }
        let groups = self.packing_multiset_groups_with_workers(
            placed_piece_count,
            initial_hold,
            hold_enabled,
            requested_workers,
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

    fn symbolic_standard_bag_family(
        &self,
        placed_piece_count: usize,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
    ) -> Option<PackingMultisetFamily> {
        if !matches!(
            self.structure(),
            MaterializedPatternUniverseStructure::Standard7BagLexicographic { .. }
        ) {
            return None;
        }
        let keys = reachable_bag_multisets(
            &PieceKind::STANDARD_TETROMINOES,
            placed_piece_count,
            initial_hold,
            hold_enabled,
        )
        .ok()?;
        if keys.is_empty() {
            return None;
        }
        let all_patterns = Arc::new(PatternBitSet::all(self.pattern_count()));
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
        Some(PackingMultisetFamily {
            envelope,
            groups,
            membership_kind: PackingPatternMembershipKind::ExactSymbolicStandardBag,
        })
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
    ) -> Result<Vec<PackingMultisetGroup>, PackingMultisetBuildError> {
        const PARALLEL_PATTERN_THRESHOLD: usize = 256;
        let available_workers = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .max(1);
        let worker_count = requested_workers
            .max(1)
            .min(available_workers)
            .min(self.pattern_count().max(1));
        if worker_count == 1 || self.pattern_count() < PARALLEL_PATTERN_THRESHOLD {
            return Ok(self.packing_multiset_groups(
                placed_piece_count,
                initial_hold,
                hold_enabled,
            ));
        }

        let shards = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(worker_count);
            for worker_index in 0..worker_count {
                let begin = self.pattern_count() * worker_index / worker_count;
                let end = self.pattern_count() * (worker_index + 1) / worker_count;
                handles.push(scope.spawn(move || {
                    let mut workspace = ReachableMultisetWorkspace::new(placed_piece_count);
                    let mut sequence = Vec::new();
                    let mut records = Vec::with_capacity(end.saturating_sub(begin));
                    for pattern_index in begin..end {
                        self.write_sequence_at(pattern_index, &mut sequence);
                        for &key in workspace.reachable_multisets(
                            &sequence,
                            placed_piece_count,
                            initial_hold,
                            hold_enabled,
                        ) {
                            records.push(PatternMultisetRecord { key, pattern_index });
                        }
                    }
                    records
                }));
            }

            let mut shards = Vec::with_capacity(worker_count);
            for handle in handles {
                shards.push(
                    handle
                        .join()
                        .map_err(|_| PackingMultisetBuildError::WorkerPanicked)?,
                );
            }
            Ok::<_, PackingMultisetBuildError>(shards)
        })?;

        let record_count = shards.iter().map(Vec::len).sum();
        let mut records = Vec::with_capacity(record_count);
        for mut shard in shards {
            records.append(&mut shard);
        }
        records.sort_unstable();
        records.dedup();

        let mut groups = Vec::<PackingMultisetGroup>::new();
        let mut current_key = None;
        let mut pattern_ids = Vec::new();
        for record in records {
            if current_key.is_some_and(|key| key != record.key) {
                groups.push(sparse_group(
                    current_key.expect("a key precedes collected pattern ids"),
                    self.pattern_count(),
                    core::mem::take(&mut pattern_ids),
                ));
            }
            current_key = Some(record.key);
            pattern_ids.push(
                u32::try_from(record.pattern_index)
                    .expect("materialized pattern id fits the compact product index"),
            );
        }
        if let Some(key) = current_key {
            groups.push(sparse_group(key, self.pattern_count(), pattern_ids));
        }
        Ok(groups)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackingMultisetBuildError {
    WorkerPanicked,
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
    use crate::{
        hold_automaton::SupplyProvenanceId, pattern_universe::PatternUniverseMaterializer,
        piece_source::PieceSourceId,
    };

    use super::*;

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
}
