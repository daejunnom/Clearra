use super::*;
use crate::{
    cooperative_sort::CooperativeSort,
    structural_verify::{StructuralVerificationMetrics, VerificationCursor},
};
use std::sync::Arc;

struct Context {
    candidate: Candidate,
    remaining: PieceInventory,
    occupied: StructureBoard,
    non_target: StructureBoard,
    deleted: u32,
    target_ready: bool,
    grounded: bool,
    entry_before: StructuralVerificationMetrics,
}

enum Phase {
    Load,
    Sort,
    Enqueue,
    Pick,
    Accepted(Candidate, usize),
    Inspect(Candidate),
    Necessity(Context, usize, u32),
    Verify(Context, VerificationCursor),
    Support(Context, usize, usize, BTreeSet<StructureOperation>),
    Blocker(
        Context,
        Vec<(u8, u8)>,
        usize,
        usize,
        BTreeSet<StructureOperation>,
    ),
    Roof(Context, usize, BTreeSet<StructureOperation>),
    Emit(Context, BTreeSet<StructureOperation>),
    Complete,
}

/// The structural producer retains exact target/depth/candidate order while
/// exposing every catalog scan and temporal-verification transition to the
/// worker's existing cooperative scheduler.
pub(crate) struct ExpansionCursor {
    query: SpinStructureQuery,
    catalog: Arc<LogicalOperationCatalog>,
    target: StructureOperation,
    seeds: std::vec::IntoIter<Vec<StructureOperation>>,
    initial: Vec<Candidate>,
    initial_sorted: std::vec::IntoIter<Candidate>,
    sort: CooperativeSort,
    visited: BTreeSet<Candidate>,
    queues: Vec<VecDeque<Candidate>>,
    accepted: Vec<AcceptedSet>,
    outcomes: Vec<SpinStructureOutcome>,
    depth: usize,
    metrics: StructuralExpansionMetrics,
    verifier: StructuralBuildVerifier,
    phase: Phase,
}

impl ExpansionCursor {
    pub(crate) fn new(
        query: SpinStructureQuery,
        catalog: Arc<LogicalOperationCatalog>,
        target: StructureOperation,
        seeds: Vec<Vec<StructureOperation>>,
        control: &ExecutionControl,
    ) -> Self {
        let verifier = StructuralBuildVerifier::new_with_control(&query, control);
        let queues = (0..=query.placement_limit())
            .map(|_| VecDeque::new())
            .collect();
        Self {
            query,
            catalog,
            target,
            seeds: seeds.into_iter(),
            initial: Vec::new(),
            initial_sorted: Vec::new().into_iter(),
            sort: CooperativeSort::new(0),
            visited: BTreeSet::new(),
            queues,
            accepted: Vec::new(),
            outcomes: Vec::new(),
            depth: 0,
            metrics: StructuralExpansionMetrics::default(),
            verifier,
            phase: Phase::Load,
        }
    }

    fn record_entry_work(&mut self, before: StructuralVerificationMetrics) {
        let after = self.verifier.metrics();
        let depth = self.depth.min(255);
        self.metrics.entry_states_by_depth[depth] +=
            after.entry_states.saturating_sub(before.entry_states);
        self.metrics.reachable_locks_by_depth[depth] +=
            after.reachable_locks.saturating_sub(before.reachable_locks);
    }

    fn start_blockers(&self, context: Context, additions: BTreeSet<StructureOperation>) -> Phase {
        if !additions.is_empty() {
            return Phase::Emit(context, additions);
        }
        if !context.target_ready {
            let cells = blocker_cells(
                &self.query,
                self.target,
                context.non_target,
                context.deleted,
            );
            Phase::Blocker(context, cells, 0, 0, additions)
        } else if context.grounded {
            Phase::Roof(context, 0, additions)
        } else {
            Phase::Emit(context, additions)
        }
    }

    pub(crate) fn step(&mut self) -> bool {
        let phase = std::mem::replace(&mut self.phase, Phase::Pick);
        self.phase = match phase {
            Phase::Load => {
                if let Some(mut operations) = self.seeds.next() {
                    operations.sort_unstable();
                    operations.dedup();
                    let without = full_rows(
                        self.query
                            .initial_board
                            .union(union_without_target(&operations, self.target)),
                        self.query.height,
                    );
                    let with = full_rows(
                        self.query
                            .initial_board
                            .union(union_operations(&operations)),
                        self.query.height,
                    );
                    let candidate = Candidate {
                        operations,
                        full_rows_without_target: without,
                        full_rows_with_target: with,
                        target_full_rows: with & used_rows(self.target, self.query.height),
                    };
                    if self.visited.insert(candidate.clone()) {
                        self.initial.push(candidate);
                    }
                    Phase::Load
                } else {
                    self.sort = CooperativeSort::new(self.initial.len());
                    Phase::Sort
                }
            }
            Phase::Sort => {
                if self.sort.step(&mut self.initial, |a, b| {
                    a.operations
                        .len()
                        .cmp(&b.operations.len())
                        .then_with(|| a.cmp(b))
                }) {
                    self.initial_sorted = std::mem::take(&mut self.initial).into_iter();
                    Phase::Enqueue
                } else {
                    Phase::Sort
                }
            }
            Phase::Enqueue => {
                if let Some(candidate) = self.initial_sorted.next() {
                    if candidate.operations.len() < self.queues.len() {
                        self.queues[candidate.operations.len()].push_back(candidate);
                    }
                    Phase::Enqueue
                } else {
                    Phase::Pick
                }
            }
            Phase::Pick => {
                if self.depth >= self.queues.len() {
                    Phase::Complete
                } else if let Some(candidate) = self.queues[self.depth].pop_front() {
                    self.metrics.candidates += 1;
                    self.metrics.candidates_by_depth[self.depth.min(255)] += 1;
                    Phase::Accepted(candidate, 0)
                } else {
                    self.depth += 1;
                    Phase::Pick
                }
            }
            Phase::Accepted(candidate, index) => {
                if let Some(known) = self.accepted.get(index) {
                    if known.target_full_rows == candidate.target_full_rows
                        && strict_subset(&known.operations, &candidate.operations)
                    {
                        Phase::Pick
                    } else {
                        Phase::Accepted(candidate, index + 1)
                    }
                } else {
                    Phase::Inspect(candidate)
                }
            }
            Phase::Inspect(candidate) => {
                if let Some(remaining) =
                    remaining_inventory(self.query.inventory, &candidate.operations)
                {
                    let occupied = self
                        .query
                        .initial_board
                        .union(union_operations(&candidate.operations));
                    let non_target = self
                        .query
                        .initial_board
                        .union(union_without_target(&candidate.operations, self.target));
                    let deleted = full_rows(non_target, self.query.height);
                    if deleted != candidate.full_rows_without_target
                        || full_rows(occupied, self.query.height) != candidate.full_rows_with_target
                    {
                        Phase::Pick
                    } else {
                        let target_ready =
                            target_shape_ready(&self.query, self.target, non_target, deleted);
                        let grounded = all_operations_grounded(
                            &self.query,
                            self.target,
                            &candidate.operations,
                            non_target,
                            occupied,
                        );
                        let entry_before = self.verifier.metrics();
                        let entry_ready = grounded
                            && target_ready
                            && target_physical_context(
                                &self.query,
                                self.target,
                                non_target,
                                deleted,
                            )
                            .is_some_and(|(board, mask)| {
                                self.verifier.target_has_scoring_entry(
                                    &self.query,
                                    board,
                                    self.target.piece(),
                                    mask,
                                    candidate.target_full_rows,
                                )
                            });
                        let context = Context {
                            candidate,
                            remaining,
                            occupied,
                            non_target,
                            deleted,
                            target_ready,
                            grounded,
                            entry_before,
                        };
                        if entry_ready {
                            let one_piece = one_piece_full_rows(
                                &context.candidate.operations,
                                full_rows(occupied, self.query.height),
                                self.query.height,
                            ) & !used_rows(self.target, self.query.height);
                            Phase::Necessity(context, 0, one_piece)
                        } else {
                            self.record_entry_work(entry_before);
                            if context.candidate.operations.len() >= self.queues.len() - 1
                                || remaining.total() == 0
                            {
                                Phase::Pick
                            } else if !grounded {
                                Phase::Support(context, 0, 0, BTreeSet::new())
                            } else {
                                self.start_blockers(context, BTreeSet::new())
                            }
                        }
                    }
                } else {
                    Phase::Pick
                }
            }
            Phase::Necessity(context, index, one_piece) => {
                if let Some(removed) = context.candidate.operations.get(index).copied() {
                    let redundant = removal_keeps_structure(
                        &self.query,
                        self.target,
                        &context.candidate.operations,
                        context.non_target,
                        context.candidate.target_full_rows,
                        one_piece,
                        removed,
                        &mut self.verifier,
                        &mut self.metrics,
                    );
                    if redundant {
                        self.record_entry_work(context.entry_before);
                        Phase::Pick
                    } else {
                        Phase::Necessity(context, index + 1, one_piece)
                    }
                } else {
                    self.metrics.verification_candidates += 1;
                    self.metrics.verified_by_depth[self.depth.min(255)] += 1;
                    let cursor = VerificationCursor::new(
                        &self.query,
                        &context.candidate.operations,
                        self.target,
                        None,
                    );
                    Phase::Verify(context, cursor)
                }
            }
            Phase::Verify(context, mut cursor) => {
                if cursor.step(&self.query, &mut self.verifier) {
                    self.record_entry_work(context.entry_before);
                    if let Some(outcome) = cursor.finish().unwrap() {
                        if outcome.is_mini() {
                            self.metrics.accepted_mini_by_depth[self.depth.min(255)] += 1;
                        } else {
                            self.metrics.accepted_regular_by_depth[self.depth.min(255)] += 1;
                        }
                        self.accepted.push(AcceptedSet {
                            operations: context.candidate.operations,
                            target_full_rows: context.candidate.target_full_rows,
                        });
                        self.outcomes.push(outcome);
                    }
                    Phase::Pick
                } else {
                    Phase::Verify(context, cursor)
                }
            }
            Phase::Support(context, selected, index, mut additions) => {
                if let Some(operation) = context.candidate.operations.get(selected).copied() {
                    let grounded = index == 0 && {
                        let all_full = full_rows(context.occupied, self.query.height);
                        let one_piece = one_piece_full_rows(
                            &context.candidate.operations,
                            all_full,
                            self.query.height,
                        ) & !used_rows(self.target, self.query.height);
                        let board = if operation == self.target {
                            context.non_target
                        } else {
                            board_difference(context.non_target, operation.mask())
                        };
                        reference_operation_is_grounded(
                            &self.query,
                            operation,
                            board,
                            all_full,
                            one_piece,
                        )
                    };
                    if grounded {
                        Phase::Support(context, selected + 1, 0, additions)
                    } else if let Some(scaffold) = self.catalog.operations().get(index).copied() {
                        if context.remaining.count(scaffold.piece()) != 0
                            && can_add(
                                &self.query,
                                scaffold,
                                context.occupied,
                                context.non_target,
                                context.deleted,
                                context.candidate.full_rows_with_target,
                            )
                            && scaffold.need_deleted_rows()
                                & used_rows(operation, self.query.height)
                                == 0
                            && scaffold_supports(operation, scaffold, self.query.height)
                        {
                            self.metrics.support_candidates += 1;
                            additions.insert(scaffold);
                        }
                        Phase::Support(context, selected, index + 1, additions)
                    } else {
                        Phase::Support(context, selected + 1, 0, additions)
                    }
                } else {
                    self.start_blockers(context, additions)
                }
            }
            Phase::Blocker(context, cells, cell, index, mut additions) => {
                if let Some((x, y)) = cells.get(cell).copied() {
                    if let Some(id) = self
                        .catalog
                        .operation_ids_for_cell(x, y)
                        .get(index)
                        .copied()
                    {
                        if let Some(operation) = self.catalog.operation(id) {
                            if context.remaining.count(operation.piece()) != 0
                                && can_add(
                                    &self.query,
                                    operation,
                                    context.occupied,
                                    context.non_target,
                                    context.deleted,
                                    context.candidate.full_rows_with_target,
                                )
                            {
                                self.metrics.blocker_candidates += 1;
                                additions.insert(operation);
                            }
                        }
                        Phase::Blocker(context, cells, cell, index + 1, additions)
                    } else {
                        Phase::Blocker(context, cells, cell + 1, 0, additions)
                    }
                } else {
                    Phase::Emit(context, additions)
                }
            }
            Phase::Roof(context, index, mut additions) => {
                if let Some(operation) = self.catalog.operations().get(index).copied() {
                    if context.remaining.count(operation.piece()) != 0
                        && can_add(
                            &self.query,
                            operation,
                            context.occupied,
                            context.non_target,
                            context.deleted,
                            context.candidate.full_rows_with_target,
                        )
                        && operation_is_grounded(
                            operation,
                            context.non_target,
                            operation.need_deleted_rows(),
                            self.query.height,
                        )
                    {
                        self.metrics.roof_candidates += 1;
                        additions.insert(operation);
                    }
                    Phase::Roof(context, index + 1, additions)
                } else {
                    Phase::Emit(context, additions)
                }
            }
            Phase::Emit(context, mut additions) => {
                if let Some(addition) = additions.pop_first() {
                    self.metrics.piece_choices_by_depth[self.depth.min(255)] += 1;
                    let mut operations = context.candidate.operations.clone();
                    operations.push(addition);
                    operations.sort_unstable();
                    let next = Candidate {
                        operations,
                        full_rows_without_target: context.candidate.full_rows_without_target,
                        full_rows_with_target: context.candidate.full_rows_with_target,
                        target_full_rows: context.candidate.target_full_rows,
                    };
                    let depth = next.operations.len();
                    if self.visited.insert(next.clone()) {
                        self.metrics.generated_by_depth[depth.min(255)] += 1;
                        self.queues[depth].push_back(next);
                    } else {
                        self.metrics.exact_duplicates += 1;
                        self.metrics.duplicates_by_depth[depth.min(255)] += 1;
                    }
                    Phase::Emit(context, additions)
                } else {
                    Phase::Pick
                }
            }
            Phase::Complete => Phase::Complete,
        };
        matches!(self.phase, Phase::Complete)
    }

    pub(crate) fn finish(
        self,
    ) -> Option<(
        Vec<SpinStructureOutcome>,
        StructuralExpansionMetrics,
        StructuralVerificationMetrics,
    )> {
        matches!(self.phase, Phase::Complete)
            .then(|| (self.outcomes, self.metrics, self.verifier.metrics()))
    }
}

#[allow(clippy::too_many_arguments)]
fn removal_keeps_structure(
    query: &SpinStructureQuery,
    target: StructureOperation,
    operations: &[StructureOperation],
    non_target: StructureBoard,
    target_rows: u32,
    one_piece: u32,
    removed: StructureOperation,
    verifier: &mut StructuralBuildVerifier,
    metrics: &mut StructuralExpansionMetrics,
) -> bool {
    if removed == target || used_rows(removed, query.height) & target_rows != 0 {
        return false;
    }
    metrics.necessity_checks += 1;
    let field = board_difference(non_target, removed.mask());
    let rows = full_rows(field, query.height);
    if !target_shape_ready(query, target, field, rows) {
        return false;
    }
    let one_piece = one_piece & !used_rows(removed, query.height);
    if !operations.iter().copied().all(|operation| {
        reference_operation_is_grounded(
            query,
            operation,
            board_difference(field, operation.mask()),
            rows,
            one_piece,
        )
    }) {
        return false;
    }
    let remains =
        target_physical_context(query, target, field, rows).is_some_and(|(board, mask)| {
            verifier.target_has_scoring_entry(query, board, target.piece(), mask, target_rows)
        });
    if remains {
        metrics.necessity_rejections += 1;
    }
    remains
}

fn blocker_cells(
    query: &SpinStructureQuery,
    target: StructureOperation,
    non_target: StructureBoard,
    deleted: u32,
) -> Vec<(u8, u8)> {
    let Some(physical) = project_operation(target, deleted, query.height) else {
        return Vec::new();
    };
    let board = compact_board(non_target, deleted, query.height);
    let mut cells = BTreeSet::new();
    if target.piece() == PieceKind::T {
        let (x, y) = t_center(physical);
        let corners = [
            (x - 1, y - 1),
            (x + 1, y - 1),
            (x - 1, y + 1),
            (x + 1, y + 1),
        ];
        if corners
            .iter()
            .filter(|(x, y)| is_blocked(board, *x, *y))
            .count()
            < 3
        {
            for (x, y) in corners {
                if !is_blocked(board, x, y) {
                    cells.insert((x, y));
                }
            }
        }
        if query.mode.plus() {
            collect_open_translation_cells(physical.mask, board, &mut cells);
        }
    } else if !query.mode.t_only() {
        collect_open_translation_cells(physical.mask, board, &mut cells);
    }
    cells
        .into_iter()
        .filter_map(|(x, y)| {
            if x < 0 || x >= 10 || y < 0 {
                return None;
            }
            logical_row_for_physical(deleted, y as u8, query.height).map(|y| (x as u8, y))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structural_fill::enumerate_fill_seeds;
    #[test]
    fn cooperative_expansion_matches_recursive_candidates_and_exact_metrics() {
        let mut query = SpinStructureQuery::new(
            PieceInventory::from_pieces([PieceKind::T, PieceKind::I, PieceKind::O]).unwrap(),
            crate::SpinStructureMode::TSpins,
        );
        query.height = 4;
        query.fill_top = 4;
        query.max_placements = Some(3);
        query.line_requirement = crate::SpinLineRequirement::Any;
        query.initial_board = [(4, 2), (6, 2), (4, 0)]
            .into_iter()
            .fold(StructureBoard::EMPTY, |board, (x, y)| {
                board.with_cell(x, y).unwrap()
            });
        let catalog = Arc::new(
            LogicalOperationCatalog::compile(query.height, query.initial_board, query.inventory)
                .unwrap(),
        );
        for target in catalog.operations_for_piece(PieceKind::T).iter().copied() {
            let seeds = enumerate_fill_seeds(&query, &catalog, target, None)
                .into_iter()
                .map(|seed| seed.operations().to_vec())
                .collect::<Vec<_>>();
            let mut verifier = StructuralBuildVerifier::new(&query);
            let mut expected_metrics = StructuralExpansionMetrics::default();
            let expected = expand_and_verify(
                &query,
                &catalog,
                target,
                seeds.clone(),
                &mut verifier,
                &mut expected_metrics,
            );
            let mut cursor = ExpansionCursor::new(
                query.clone(),
                catalog.clone(),
                target,
                seeds,
                &ExecutionControl::default(),
            );
            while !cursor.step() {}
            let (actual, metrics, verification) = cursor.finish().unwrap();
            expected_metrics.elapsed_ns_by_depth = [0; 256];
            assert_eq!(actual, expected);
            assert_eq!(metrics, expected_metrics);
            assert_eq!(verification, verifier.metrics());
        }
    }
}
