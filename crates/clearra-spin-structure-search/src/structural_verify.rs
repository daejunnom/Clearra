use std::collections::{BTreeMap, BTreeSet, VecDeque};

use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::{
    board::{place_and_clear, StructureBoard},
    entry::{EntryCatalog, EntryResult},
    logical::{apply_physical_lock, DeletedLogicalRows, LogicalBoard},
    model::{SpinStructureOutcome, SpinStructureQuery, StructureOperation, StructurePlacement},
    verify,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct StructuralVerificationMetrics {
    pub(crate) build_states: u64,
    pub(crate) entry_requests: u64,
    pub(crate) entry_states: u64,
    pub(crate) reachable_locks: u64,
    pub(crate) entry_cache_hits: u64,
    pub(crate) entry_cache_misses: u64,
    pub(crate) entry_cache_evictions: u64,
}

const ENTRY_RESULT_CACHE_CAPACITY: usize = 4096;

/// Every input that changes [`EntryCatalog::reachable_locks`] output belongs
/// in this key. Height and rule profile are fixed by the owning verifier.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EntryCacheKey {
    board: StructureBoard,
    piece: PieceKind,
    retain_scoring_evidence: bool,
    measure_immobility: bool,
}

impl EntryCacheKey {
    const fn new(
        board: StructureBoard,
        piece: PieceKind,
        retain_scoring_evidence: bool,
        measure_immobility: bool,
    ) -> Self {
        Self {
            board,
            piece,
            retain_scoring_evidence,
            measure_immobility,
        }
    }
}

/// A bounded FIFO cache. Hits deliberately do not refresh insertion order;
/// this keeps eviction deterministic and avoids a second ordered structure.
struct EntryResultCache {
    capacity: usize,
    entries: BTreeMap<EntryCacheKey, EntryResult>,
    insertion_order: VecDeque<EntryCacheKey>,
}

impl EntryResultCache {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: BTreeMap::new(),
            insertion_order: VecDeque::with_capacity(capacity),
        }
    }

    fn get(&self, key: EntryCacheKey) -> Option<EntryResult> {
        self.entries.get(&key).cloned()
    }

    /// Inserts a key known to be absent and reports whether an older entry
    /// was evicted.
    fn insert(&mut self, key: EntryCacheKey, result: EntryResult) -> bool {
        if self.capacity == 0 {
            return false;
        }
        debug_assert!(!self.entries.contains_key(&key));
        let evicted = if self.entries.len() == self.capacity {
            let oldest = self
                .insertion_order
                .pop_front()
                .expect("non-empty bounded cache has an insertion key");
            let removed = self.entries.remove(&oldest);
            debug_assert!(removed.is_some());
            true
        } else {
            false
        };
        self.entries.insert(key, result);
        self.insertion_order.push_back(key);
        evicted
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct BuildMemoKey {
    logical_board: LogicalBoard,
    deleted_rows: DeletedLogicalRows,
    remaining: Vec<StructureOperation>,
}

#[derive(Clone, Copy, Debug)]
struct BuildPosition {
    board: StructureBoard,
    logical_board: LogicalBoard,
    deleted_rows: DeletedLogicalRows,
}

/// Exact static-set verifier used after the structural producer stages.
///
/// Every non-target operation is replayed in some legal order through the
/// same entry graph as the exhaustive oracle. The target is always locked
/// last and retains every scoring-relevant rotation class. A failed order is
/// memoized only by its complete logical state and exact remaining operation
/// multiset; failure of one order never rejects a different state.
pub(crate) struct StructuralBuildVerifier {
    entry: EntryCatalog,
    entry_results: EntryResultCache,
    metrics: StructuralVerificationMetrics,
}

impl StructuralBuildVerifier {
    pub(crate) fn new(query: &SpinStructureQuery) -> Self {
        Self {
            entry: EntryCatalog::new(query.height, query.rule_profile),
            entry_results: EntryResultCache::new(ENTRY_RESULT_CACHE_CAPACITY),
            metrics: StructuralVerificationMetrics::default(),
        }
    }

    #[cfg(test)]
    fn with_entry_cache_capacity(query: &SpinStructureQuery, capacity: usize) -> Self {
        Self {
            entry: EntryCatalog::new(query.height, query.rule_profile),
            entry_results: EntryResultCache::new(capacity),
            metrics: StructuralVerificationMetrics::default(),
        }
    }

    pub(crate) const fn metrics(&self) -> StructuralVerificationMetrics {
        self.metrics
    }

    /// Cheap exact terminal preflight for a completed static structure.
    ///
    /// This proves that the reserved target geometry has at least one
    /// rotation-ending scoring entry before the factorial non-target build
    /// order search begins. A negative result is safe for the current static
    /// board; callers may still add roof operations and retry.
    pub(crate) fn target_has_scoring_entry(
        &mut self,
        query: &SpinStructureQuery,
        board_before: StructureBoard,
        piece: PieceKind,
        target_mask: StructureBoard,
        logical_cleared_rows: u32,
    ) -> bool {
        self.reachable_locks(board_before, piece, true, true)
            .locks
            .into_iter()
            .filter(|lock| lock.mask == target_mask)
            .any(|lock| {
                let occupied = board_before.union(lock.mask);
                let (board_after, cleared_rows, cleared_lines) =
                    place_and_clear(query.height, occupied);
                verify::classify_lock(
                    query,
                    board_before,
                    board_after,
                    piece,
                    lock,
                    cleared_rows,
                    logical_cleared_rows,
                    cleared_lines,
                )
                .is_some()
            })
    }

    fn reachable_locks(
        &mut self,
        board: StructureBoard,
        piece: PieceKind,
        retain_scoring_evidence: bool,
        measure_immobility: bool,
    ) -> EntryResult {
        self.metrics.entry_requests += 1;
        let key = EntryCacheKey::new(board, piece, retain_scoring_evidence, measure_immobility);
        if let Some(result) = self.entry_results.get(key) {
            self.metrics.entry_cache_hits += 1;
            self.metrics.reachable_locks += result.locks.len() as u64;
            return result;
        }

        self.metrics.entry_cache_misses += 1;
        let result =
            self.entry
                .reachable_locks(board, piece, retain_scoring_evidence, measure_immobility);
        self.metrics.entry_states += result.visited_states;
        self.metrics.reachable_locks += result.locks.len() as u64;
        if self.entry_results.insert(key, result.clone()) {
            self.metrics.entry_cache_evictions += 1;
        }
        result
    }

    pub(crate) fn verify(
        &mut self,
        query: &SpinStructureQuery,
        operations: &[StructureOperation],
        target: StructureOperation,
    ) -> Option<SpinStructureOutcome> {
        let target_index = operations
            .iter()
            .position(|operation| *operation == target)?;
        let mut remaining = operations.to_vec();
        remaining.remove(target_index);
        remaining.sort_unstable();

        let logical_board = LogicalBoard::from_initial(query.initial_board);
        let deleted_rows = logical_board.initial_deleted_rows(query.height);
        let position = BuildPosition {
            board: logical_board.compact(deleted_rows, query.height),
            logical_board,
            deleted_rows,
        };
        let mut failed = BTreeSet::new();
        let mut witness = Vec::with_capacity(operations.len());
        self.build_non_target(
            query,
            position,
            &remaining,
            target,
            operations,
            &mut witness,
            &mut failed,
        )
    }

    /// Replays one exact no-hold piece order against a validated static
    /// structure. Operations with the same piece kind remain distinct and are
    /// matched exhaustively; the reserved scoring operation must be the final
    /// element of the supplied order.
    pub(crate) fn accepts_piece_order(
        &mut self,
        query: &SpinStructureQuery,
        operations: &[StructureOperation],
        target: StructureOperation,
        piece_order: &[PieceKind],
    ) -> bool {
        if piece_order.len() != operations.len()
            || piece_order.last().copied() != Some(target.piece())
        {
            return false;
        }
        let Some(target_index) = operations.iter().position(|operation| *operation == target)
        else {
            return false;
        };
        let mut remaining = operations.to_vec();
        remaining.remove(target_index);
        remaining.sort_unstable();
        let logical_board = LogicalBoard::from_initial(query.initial_board);
        let deleted_rows = logical_board.initial_deleted_rows(query.height);
        let position = BuildPosition {
            board: logical_board.compact(deleted_rows, query.height),
            logical_board,
            deleted_rows,
        };
        let mut failed = BTreeSet::new();
        self.build_ordered_non_target(
            query,
            position,
            &remaining,
            target,
            operations,
            &piece_order[..piece_order.len() - 1],
            0,
            &mut failed,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build_ordered_non_target(
        &mut self,
        query: &SpinStructureQuery,
        position: BuildPosition,
        remaining: &[StructureOperation],
        target: StructureOperation,
        canonical_operations: &[StructureOperation],
        piece_order: &[PieceKind],
        cursor: usize,
        failed: &mut BTreeSet<BuildMemoKey>,
    ) -> bool {
        self.metrics.build_states += 1;
        if remaining.is_empty() {
            return cursor == piece_order.len()
                && self
                    .lock_target(query, position, target, canonical_operations, &[])
                    .is_some();
        }
        let Some(next_piece) = piece_order.get(cursor).copied() else {
            return false;
        };
        let key = BuildMemoKey {
            logical_board: position.logical_board,
            deleted_rows: position.deleted_rows,
            remaining: remaining.to_vec(),
        };
        if failed.contains(&key) {
            return false;
        }
        for operation_index in 0..remaining.len() {
            if remaining[operation_index].piece() != next_piece
                || (operation_index != 0
                    && remaining[operation_index] == remaining[operation_index - 1])
            {
                continue;
            }
            let operation = remaining[operation_index];
            let entry_result = self.reachable_locks(position.board, next_piece, false, false);
            for lock in entry_result.locks {
                let occupied = position.board.union(lock.mask);
                let (board_after, _, _) = place_and_clear(query.height, occupied);
                let Some(logical_lock) = apply_physical_lock(
                    position.logical_board,
                    position.deleted_rows,
                    query.height,
                    next_piece,
                    lock.rotation,
                    lock.x,
                    lock.mask,
                ) else {
                    continue;
                };
                if logical_lock.identity != operation {
                    continue;
                }
                debug_assert_eq!(
                    logical_lock
                        .board_after
                        .compact(logical_lock.deleted_after, query.height),
                    board_after
                );
                let mut next_remaining = remaining.to_vec();
                next_remaining.remove(operation_index);
                if self.build_ordered_non_target(
                    query,
                    BuildPosition {
                        board: board_after,
                        logical_board: logical_lock.board_after,
                        deleted_rows: logical_lock.deleted_after,
                    },
                    &next_remaining,
                    target,
                    canonical_operations,
                    piece_order,
                    cursor + 1,
                    failed,
                ) {
                    return true;
                }
            }
        }
        failed.insert(key);
        false
    }

    #[allow(clippy::too_many_arguments)]
    fn build_non_target(
        &mut self,
        query: &SpinStructureQuery,
        position: BuildPosition,
        remaining: &[StructureOperation],
        target: StructureOperation,
        canonical_operations: &[StructureOperation],
        witness: &mut Vec<StructurePlacement>,
        failed: &mut BTreeSet<BuildMemoKey>,
    ) -> Option<SpinStructureOutcome> {
        self.metrics.build_states += 1;
        if remaining.is_empty() {
            return self.lock_target(query, position, target, canonical_operations, witness);
        }

        let key = BuildMemoKey {
            logical_board: position.logical_board,
            deleted_rows: position.deleted_rows,
            remaining: remaining.to_vec(),
        };
        if failed.contains(&key) {
            return None;
        }

        for operation_index in 0..remaining.len() {
            if operation_index != 0 && remaining[operation_index] == remaining[operation_index - 1]
            {
                continue;
            }
            let operation = remaining[operation_index];
            let entry_result =
                self.reachable_locks(position.board, operation.piece(), false, false);
            for lock in entry_result.locks {
                let occupied = position.board.union(lock.mask);
                let (board_after, cleared_rows, cleared_lines) =
                    place_and_clear(query.height, occupied);
                let Some(logical_lock) = apply_physical_lock(
                    position.logical_board,
                    position.deleted_rows,
                    query.height,
                    operation.piece(),
                    lock.rotation,
                    lock.x,
                    lock.mask,
                ) else {
                    continue;
                };
                if logical_lock.identity != operation {
                    continue;
                }
                debug_assert_eq!(
                    logical_lock
                        .board_after
                        .compact(logical_lock.deleted_after, query.height),
                    board_after
                );

                let mut next_remaining = remaining.to_vec();
                next_remaining.remove(operation_index);
                witness.push(verify::placement_from_lock(
                    operation.piece(),
                    lock,
                    cleared_rows,
                    cleared_lines,
                ));
                let found = self.build_non_target(
                    query,
                    BuildPosition {
                        board: board_after,
                        logical_board: logical_lock.board_after,
                        deleted_rows: logical_lock.deleted_after,
                    },
                    &next_remaining,
                    target,
                    canonical_operations,
                    witness,
                    failed,
                );
                witness.pop();
                if found.is_some() {
                    return found;
                }
            }
        }

        failed.insert(key);
        None
    }

    fn lock_target(
        &mut self,
        query: &SpinStructureQuery,
        position: BuildPosition,
        target: StructureOperation,
        canonical_operations: &[StructureOperation],
        witness: &[StructurePlacement],
    ) -> Option<SpinStructureOutcome> {
        let entry_result = self.reachable_locks(position.board, target.piece(), true, true);

        let mut best: Option<SpinStructureOutcome> = None;
        for lock in entry_result.locks {
            let occupied = position.board.union(lock.mask);
            let (board_after, cleared_rows, cleared_lines) =
                place_and_clear(query.height, occupied);
            let Some(logical_lock) = apply_physical_lock(
                position.logical_board,
                position.deleted_rows,
                query.height,
                target.piece(),
                lock.rotation,
                lock.x,
                lock.mask,
            ) else {
                continue;
            };
            if logical_lock.identity != target {
                continue;
            }
            let Some(event) = verify::classify_lock(
                query,
                position.board,
                board_after,
                target.piece(),
                lock,
                cleared_rows,
                logical_lock.newly_deleted_rows,
                cleared_lines,
            ) else {
                continue;
            };

            let target_placement =
                verify::placement_from_lock(target.piece(), lock, cleared_rows, cleared_lines);
            let mut build = witness.to_vec();
            build.push(target_placement);
            let outcome = SpinStructureOutcome {
                board_before_spin: position.board,
                final_board: board_after,
                spin: target_placement,
                build,
                mini: event.is_mini(),
                logical_operations: canonical_operations.to_vec(),
                logical_spin: target,
                logical_spin_cleared_rows: logical_lock.newly_deleted_rows,
            };
            let replace = best
                .as_ref()
                .is_none_or(|known| known.is_mini() && !outcome.is_mini());
            if replace {
                best = Some(outcome);
            }
            if best.as_ref().is_some_and(|outcome| !outcome.is_mini()) {
                break;
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    use super::*;
    use crate::{
        model::{PieceInventory, SpinLineRequirement, SpinStructureMode},
        SpinStructureSearcher,
    };

    #[test]
    fn one_piece_static_verification_matches_the_exhaustive_oracle() {
        let mut query = SpinStructureQuery::new(
            PieceInventory::from_pieces([PieceKind::T]).expect("inventory"),
            SpinStructureMode::TSpins,
        );
        query.height = 4;
        query.fill_top = 4;
        query.line_requirement = SpinLineRequirement::Any;
        query.max_placements = Some(1);
        query.initial_board = [(4, 2), (6, 2), (4, 0)]
            .into_iter()
            .fold(StructureBoard::EMPTY, |board, (x, y)| {
                board.with_cell(x, y).expect("fixture cell")
            });

        let oracle = SpinStructureSearcher::run(query.clone()).expect("oracle");
        let expected = oracle.outcomes().next().expect("oracle outcome");
        let mut verifier = StructuralBuildVerifier::new(&query);
        let actual = verifier
            .verify(
                &query,
                expected.logical_operations(),
                expected.logical_spin(),
            )
            .expect("verified structure");
        let first_metrics = verifier.metrics();
        let repeated = verifier
            .verify(
                &query,
                expected.logical_operations(),
                expected.logical_spin(),
            )
            .expect("cached verified structure");
        let repeated_metrics = verifier.metrics();
        assert_eq!(actual.logical_operations(), expected.logical_operations());
        assert_eq!(actual.logical_spin(), expected.logical_spin());
        assert_eq!(actual.is_mini(), expected.is_mini());
        assert_eq!(actual, repeated);
        assert!(repeated_metrics.entry_cache_hits > first_metrics.entry_cache_hits);
        assert_eq!(
            repeated_metrics.entry_states, first_metrics.entry_states,
            "a cache hit must not report a second entry-graph traversal"
        );
    }

    #[test]
    fn cache_key_separates_geometry_and_terminal_evidence_contracts() {
        let mut query = SpinStructureQuery::new(
            PieceInventory::from_pieces([PieceKind::T]).expect("inventory"),
            SpinStructureMode::TSpinsPlus,
        );
        query.height = 4;
        query.fill_top = 4;
        query.line_requirement = SpinLineRequirement::Any;
        let mut verifier = StructuralBuildVerifier::with_entry_cache_capacity(&query, 8);

        let geometry = verifier.reachable_locks(StructureBoard::EMPTY, PieceKind::T, false, false);
        let after_geometry_miss = verifier.metrics();
        let geometry_cached =
            verifier.reachable_locks(StructureBoard::EMPTY, PieceKind::T, false, false);
        assert_eq!(geometry.locks, geometry_cached.locks);
        assert_eq!(geometry.visited_states, geometry_cached.visited_states);
        assert_eq!(after_geometry_miss.entry_cache_misses, 1);
        assert_eq!(verifier.metrics().entry_cache_hits, 1);

        let terminal = verifier.reachable_locks(StructureBoard::EMPTY, PieceKind::T, true, true);
        assert_eq!(verifier.metrics().entry_cache_misses, 2);
        assert!(terminal
            .locks
            .iter()
            .any(|lock| lock.evidence.last_action_was_rotation()));
        let terminal_cached =
            verifier.reachable_locks(StructureBoard::EMPTY, PieceKind::T, true, true);
        assert_eq!(terminal.locks, terminal_cached.locks);
        assert_eq!(terminal.visited_states, terminal_cached.visited_states);
        assert_eq!(verifier.metrics().entry_cache_hits, 2);

        verifier.reachable_locks(StructureBoard::EMPTY, PieceKind::O, false, false);
        assert_eq!(verifier.metrics().entry_cache_misses, 3);
        verifier.reachable_locks(StructureBoard::EMPTY, PieceKind::O, false, false);
        assert_eq!(verifier.metrics().entry_cache_hits, 3);
        assert_eq!(verifier.entry_results.entries.len(), 3);
    }

    #[test]
    fn bounded_cache_evicts_in_fifo_order_without_refreshing_hits() {
        let mut query = SpinStructureQuery::new(
            PieceInventory::from_pieces([PieceKind::O]).expect("inventory"),
            SpinStructureMode::AllSpin,
        );
        query.height = 4;
        query.fill_top = 4;
        query.line_requirement = SpinLineRequirement::Any;
        let mut verifier = StructuralBuildVerifier::with_entry_cache_capacity(&query, 2);
        let first = StructureBoard::EMPTY;
        let second = StructureBoard::EMPTY.with_cell(0, 0).expect("cell");
        let third = StructureBoard::EMPTY.with_cell(1, 0).expect("cell");

        verifier.reachable_locks(first, PieceKind::O, false, false);
        verifier.reachable_locks(second, PieceKind::O, false, false);
        verifier.reachable_locks(first, PieceKind::O, false, false);
        verifier.reachable_locks(third, PieceKind::O, false, false);
        assert_eq!(verifier.metrics().entry_cache_hits, 1);
        assert_eq!(verifier.metrics().entry_cache_misses, 3);
        assert_eq!(verifier.metrics().entry_cache_evictions, 1);
        assert!(!verifier
            .entry_results
            .entries
            .contains_key(&EntryCacheKey::new(first, PieceKind::O, false, false,)));

        verifier.reachable_locks(first, PieceKind::O, false, false);
        assert_eq!(verifier.metrics().entry_cache_hits, 1);
        assert_eq!(verifier.metrics().entry_cache_misses, 4);
        assert_eq!(verifier.metrics().entry_cache_evictions, 2);
        assert_eq!(verifier.entry_results.entries.len(), 2);
    }
}
