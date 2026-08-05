use std::{
    collections::{BTreeSet, VecDeque},
    time::Instant,
};

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;

use crate::{
    board::StructureBoard,
    model::{PieceInventory, SpinStructureOutcome, SpinStructureQuery, StructureOperation},
    operation_catalog::LogicalOperationCatalog,
    structural_verify::StructuralBuildVerifier,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StructuralExpansionMetrics {
    pub(crate) candidates: u64,
    pub(crate) exact_duplicates: u64,
    pub(crate) support_candidates: u64,
    pub(crate) blocker_candidates: u64,
    pub(crate) roof_candidates: u64,
    pub(crate) necessity_checks: u64,
    pub(crate) necessity_rejections: u64,
    pub(crate) verification_candidates: u64,
    pub(crate) candidates_by_depth: [u64; 256],
    pub(crate) generated_by_depth: [u64; 256],
    pub(crate) duplicates_by_depth: [u64; 256],
    pub(crate) verified_by_depth: [u64; 256],
    pub(crate) accepted_regular_by_depth: [u64; 256],
    pub(crate) accepted_mini_by_depth: [u64; 256],
    pub(crate) piece_choices_by_depth: [u64; 256],
    pub(crate) entry_states_by_depth: [u64; 256],
    pub(crate) reachable_locks_by_depth: [u64; 256],
    pub(crate) elapsed_ns_by_depth: [u64; 256],
}

impl Default for StructuralExpansionMetrics {
    fn default() -> Self {
        Self {
            candidates: 0,
            exact_duplicates: 0,
            support_candidates: 0,
            blocker_candidates: 0,
            roof_candidates: 0,
            necessity_checks: 0,
            necessity_rejections: 0,
            verification_candidates: 0,
            candidates_by_depth: [0; 256],
            generated_by_depth: [0; 256],
            duplicates_by_depth: [0; 256],
            verified_by_depth: [0; 256],
            accepted_regular_by_depth: [0; 256],
            accepted_mini_by_depth: [0; 256],
            piece_choices_by_depth: [0; 256],
            entry_states_by_depth: [0; 256],
            reachable_locks_by_depth: [0; 256],
            elapsed_ns_by_depth: [0; 256],
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Candidate {
    operations: Vec<StructureOperation>,
    full_rows_without_target: u32,
    full_rows_with_target: u32,
    target_full_rows: u32,
}

#[derive(Clone, Debug)]
struct AcceptedSet {
    operations: Vec<StructureOperation>,
    target_full_rows: u32,
}

pub(crate) fn expand_and_verify(
    query: &SpinStructureQuery,
    catalog: &LogicalOperationCatalog,
    target: StructureOperation,
    seeds: impl IntoIterator<Item = Vec<StructureOperation>>,
    verifier: &mut StructuralBuildVerifier,
    metrics: &mut StructuralExpansionMetrics,
) -> Vec<SpinStructureOutcome> {
    let mut initial_candidates = Vec::new();
    let mut visited = BTreeSet::new();
    for mut operations in seeds {
        operations.sort_unstable();
        operations.dedup();
        let full_rows_without_target = full_rows(
            query
                .initial_board
                .union(union_without_target(&operations, target)),
            query.height,
        );
        let full_rows_with_target = full_rows(
            query.initial_board.union(union_operations(&operations)),
            query.height,
        );
        let target_full_rows = full_rows_with_target & used_rows(target, query.height);
        let candidate = Candidate {
            operations,
            full_rows_without_target,
            full_rows_with_target,
            target_full_rows,
        };
        if visited.insert(candidate.clone()) {
            initial_candidates.push(candidate);
        }
    }
    initial_candidates.sort_by(|left, right| {
        left.operations
            .len()
            .cmp(&right.operations.len())
            .then_with(|| left.cmp(right))
    });
    let placement_limit = usize::from(query.placement_limit());
    let mut queues = (0..=placement_limit)
        .map(|_| VecDeque::new())
        .collect::<Vec<VecDeque<Candidate>>>();
    for candidate in initial_candidates {
        if candidate.operations.len() <= placement_limit {
            queues[candidate.operations.len()].push_back(candidate);
        }
    }

    let mut accepted_sets: Vec<AcceptedSet> = Vec::new();
    let mut outcomes = Vec::new();
    for depth in 0..=placement_limit {
        let layer_started = Instant::now();
        while let Some(candidate) = queues[depth].pop_front() {
            metrics.candidates += 1;
            let metric_depth = depth.min(255);
            metrics.candidates_by_depth[metric_depth] += 1;
            if accepted_sets.iter().any(|known| {
                known.target_full_rows == candidate.target_full_rows
                    && strict_subset(&known.operations, &candidate.operations)
            }) {
                continue;
            }

            let Some(remaining) = remaining_inventory(query.inventory, &candidate.operations)
            else {
                continue;
            };
            let occupied = query
                .initial_board
                .union(union_operations(&candidate.operations));
            let non_target = query
                .initial_board
                .union(union_without_target(&candidate.operations, target));
            let current_full_rows = full_rows(non_target, query.height);
            if current_full_rows != candidate.full_rows_without_target
                || full_rows(occupied, query.height) != candidate.full_rows_with_target
            {
                continue;
            }

            let target_ready = target_shape_ready(query, target, non_target, current_full_rows);
            let all_grounded =
                all_operations_grounded(query, target, &candidate.operations, non_target, occupied);
            let entry_before = verifier.metrics();
            let target_entry_ready = all_grounded
                && target_ready
                && target_physical_context(query, target, non_target, current_full_rows)
                    .is_some_and(|(board_before, target_mask)| {
                        verifier.target_has_scoring_entry(
                            query,
                            board_before,
                            target.piece(),
                            target_mask,
                            candidate.target_full_rows,
                        )
                    });

            let structurally_necessary = !target_entry_ready
                || structure_is_irredundant(
                    query,
                    target,
                    &candidate.operations,
                    non_target,
                    occupied,
                    candidate.target_full_rows,
                    verifier,
                    metrics,
                );

            // Corner/immobility is an exact terminal requirement. Avoid the much
            // more expensive temporal build/entry proof until that static
            // prerequisite exists. Grounding only orders expansion; a target-ready
            // candidate is still sent to the exact verifier so this optimization
            // cannot reject a legal temporal support arrangement.
            let verified = if target_entry_ready && structurally_necessary {
                metrics.verification_candidates += 1;
                metrics.verified_by_depth[metric_depth] += 1;
                verifier.verify(query, &candidate.operations, target)
            } else {
                None
            };
            let entry_after = verifier.metrics();
            metrics.entry_states_by_depth[metric_depth] += entry_after
                .entry_states
                .saturating_sub(entry_before.entry_states);
            metrics.reachable_locks_by_depth[metric_depth] += entry_after
                .reachable_locks
                .saturating_sub(entry_before.reachable_locks);
            if let Some(outcome) = verified {
                if outcome.is_mini() {
                    metrics.accepted_mini_by_depth[metric_depth] += 1;
                } else {
                    metrics.accepted_regular_by_depth[metric_depth] += 1;
                }
                accepted_sets.push(AcceptedSet {
                    operations: candidate.operations,
                    target_full_rows: candidate.target_full_rows,
                });
                outcomes.push(outcome);
                continue;
            }
            if target_entry_ready && !structurally_necessary {
                continue;
            }
            // At this point every structural prerequisite and the terminal entry
            // are already present. A remaining failure is the exact non-target
            // build-order contract; extra pieces cannot repair that candidate in
            // the staged structural search and would only create non-minimal
            // supersets.
            if target_entry_ready {
                continue;
            }
            if candidate.operations.len() >= usize::from(query.placement_limit())
                || remaining.total() == 0
            {
                continue;
            }

            let mut additions = BTreeSet::new();
            if !all_grounded {
                collect_support_candidates(
                    query,
                    catalog,
                    target,
                    &candidate.operations,
                    remaining,
                    non_target,
                    current_full_rows,
                    candidate.full_rows_with_target,
                    occupied,
                    &mut additions,
                    metrics,
                );
            }
            if additions.is_empty() && !target_ready {
                collect_blocker_candidates(
                    query,
                    catalog,
                    target,
                    remaining,
                    non_target,
                    current_full_rows,
                    candidate.full_rows_with_target,
                    occupied,
                    &mut additions,
                    metrics,
                );
            }
            if additions.is_empty() && all_grounded && target_ready {
                collect_roof_candidates(
                    query,
                    catalog,
                    remaining,
                    non_target,
                    current_full_rows,
                    candidate.full_rows_with_target,
                    occupied,
                    &mut additions,
                    metrics,
                );
            }
            metrics.piece_choices_by_depth[metric_depth] += additions.len() as u64;

            for addition in additions {
                let mut operations = candidate.operations.clone();
                operations.push(addition);
                operations.sort_unstable();
                let next = Candidate {
                    operations,
                    full_rows_without_target: candidate.full_rows_without_target,
                    full_rows_with_target: candidate.full_rows_with_target,
                    target_full_rows: candidate.target_full_rows,
                };
                if visited.insert(next.clone()) {
                    metrics.generated_by_depth[next.operations.len().min(255)] += 1;
                    queues[next.operations.len()].push_back(next);
                } else {
                    metrics.exact_duplicates += 1;
                    metrics.duplicates_by_depth[next.operations.len().min(255)] += 1;
                }
            }
        }
        let metric_depth = depth.min(255);
        metrics.elapsed_ns_by_depth[metric_depth] = metrics.elapsed_ns_by_depth[metric_depth]
            .saturating_add(duration_ns(layer_started.elapsed()));
    }
    outcomes
}

fn duration_ns(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

#[allow(clippy::too_many_arguments)]
fn collect_support_candidates(
    query: &SpinStructureQuery,
    catalog: &LogicalOperationCatalog,
    target: StructureOperation,
    selected: &[StructureOperation],
    remaining: PieceInventory,
    non_target: StructureBoard,
    full_rows_without_target: u32,
    full_rows_with_target: u32,
    occupied: StructureBoard,
    output: &mut BTreeSet<StructureOperation>,
    metrics: &mut StructuralExpansionMetrics,
) {
    let all_full_rows = full_rows(occupied, query.height);
    let one_piece_full_rows = one_piece_full_rows(selected, all_full_rows, query.height)
        & !used_rows(target, query.height);
    for operation in selected.iter().copied() {
        let board_without_operation = if operation == target {
            non_target
        } else {
            board_difference(non_target, operation.mask())
        };
        if reference_operation_is_grounded(
            query,
            operation,
            board_without_operation,
            all_full_rows,
            one_piece_full_rows,
        ) {
            continue;
        }
        for piece in remaining.available() {
            for scaffold in catalog.operations_for_piece(piece).iter().copied() {
                if !can_add(
                    query,
                    scaffold,
                    occupied,
                    non_target,
                    full_rows_without_target,
                    full_rows_with_target,
                ) || scaffold.need_deleted_rows() & used_rows(operation, query.height) != 0
                    || !scaffold_supports(operation, scaffold, query.height)
                {
                    continue;
                }
                metrics.support_candidates += 1;
                output.insert(scaffold);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_blocker_candidates(
    query: &SpinStructureQuery,
    catalog: &LogicalOperationCatalog,
    target: StructureOperation,
    remaining: PieceInventory,
    non_target: StructureBoard,
    deleted: u32,
    full_rows_with_target: u32,
    occupied: StructureBoard,
    output: &mut BTreeSet<StructureOperation>,
    metrics: &mut StructuralExpansionMetrics,
) {
    let Some(target_physical) = project_operation(target, deleted, query.height) else {
        return;
    };
    let board_physical = compact_board(non_target, deleted, query.height);
    let mut blocker_cells = BTreeSet::new();

    if target.piece() == PieceKind::T {
        let (center_x, center_y) = t_center(target_physical);
        let corners = [
            (center_x - 1, center_y - 1),
            (center_x + 1, center_y - 1),
            (center_x - 1, center_y + 1),
            (center_x + 1, center_y + 1),
        ];
        let blocked = corners
            .iter()
            .filter(|(x, y)| is_blocked(board_physical, *x, *y))
            .count();
        if blocked < 3 {
            for (x, y) in corners {
                if !is_blocked(board_physical, x, y) {
                    blocker_cells.insert((x, y));
                }
            }
        }
        if query.mode.plus() {
            collect_open_translation_cells(
                target_physical.mask,
                board_physical,
                &mut blocker_cells,
            );
        }
    } else if !query.mode.t_only() {
        collect_open_translation_cells(target_physical.mask, board_physical, &mut blocker_cells);
    }

    for (physical_x, physical_y) in blocker_cells {
        if physical_x < 0 || physical_x >= i16::from(StructureBoard::WIDTH) || physical_y < 0 {
            continue;
        }
        let Some(logical_y) = logical_row_for_physical(deleted, physical_y as u8, query.height)
        else {
            continue;
        };
        for operation_id in catalog.operation_ids_for_cell(physical_x as u8, logical_y) {
            let Some(operation) = catalog.operation(*operation_id) else {
                continue;
            };
            if remaining.count(operation.piece()) == 0
                || !can_add(
                    query,
                    operation,
                    occupied,
                    non_target,
                    deleted,
                    full_rows_with_target,
                )
            {
                continue;
            }
            metrics.blocker_candidates += 1;
            output.insert(operation);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_roof_candidates(
    query: &SpinStructureQuery,
    catalog: &LogicalOperationCatalog,
    remaining: PieceInventory,
    non_target: StructureBoard,
    deleted: u32,
    full_rows_with_target: u32,
    occupied: StructureBoard,
    output: &mut BTreeSet<StructureOperation>,
    metrics: &mut StructuralExpansionMetrics,
) {
    for piece in remaining.available() {
        for operation in catalog.operations_for_piece(piece).iter().copied() {
            if !can_add(
                query,
                operation,
                occupied,
                non_target,
                deleted,
                full_rows_with_target,
            ) || !operation_is_grounded(
                operation,
                non_target,
                operation.need_deleted_rows(),
                query.height,
            ) {
                continue;
            }
            metrics.roof_candidates += 1;
            output.insert(operation);
        }
    }
}

fn can_add(
    query: &SpinStructureQuery,
    operation: StructureOperation,
    occupied: StructureBoard,
    non_target: StructureBoard,
    full_rows_without_target: u32,
    full_rows_with_target: u32,
) -> bool {
    if occupied.intersects(operation.mask())
        || operation.need_deleted_rows() & !full_rows_without_target != 0
    {
        return false;
    }
    full_rows(non_target.union(operation.mask()), query.height) == full_rows_without_target
        && full_rows(occupied.union(operation.mask()), query.height) == full_rows_with_target
}

fn scaffold_supports(
    operation: StructureOperation,
    scaffold: StructureOperation,
    height: u8,
) -> bool {
    let deleted = operation.need_deleted_rows();
    operation_is_grounded(operation, scaffold.mask(), deleted, height)
}

fn operation_is_grounded(
    operation: StructureOperation,
    board_without_operation: StructureBoard,
    deleted: u32,
    height: u8,
) -> bool {
    if operation.need_deleted_rows() & !deleted != 0 {
        return false;
    }
    let Some(projected) = project_operation(operation, deleted, height) else {
        return false;
    };
    let board = compact_board(board_without_operation, deleted, height);
    mask_is_grounded(projected.mask, board)
}

fn all_operations_grounded(
    query: &SpinStructureQuery,
    target: StructureOperation,
    operations: &[StructureOperation],
    non_target: StructureBoard,
    occupied: StructureBoard,
) -> bool {
    let all_full_rows = full_rows(occupied, query.height);
    let one_piece_full_rows = one_piece_full_rows(operations, all_full_rows, query.height)
        & !used_rows(target, query.height);
    operations.iter().copied().all(|operation| {
        let board_without_operation = if operation == target {
            non_target
        } else {
            board_difference(non_target, operation.mask())
        };
        reference_operation_is_grounded(
            query,
            operation,
            board_without_operation,
            all_full_rows,
            one_piece_full_rows,
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn structure_is_irredundant(
    query: &SpinStructureQuery,
    target: StructureOperation,
    operations: &[StructureOperation],
    non_target: StructureBoard,
    occupied: StructureBoard,
    target_full_rows: u32,
    verifier: &mut StructuralBuildVerifier,
    metrics: &mut StructuralExpansionMetrics,
) -> bool {
    let original_full_rows = full_rows(occupied, query.height);
    let original_one_piece_rows = one_piece_full_rows(operations, original_full_rows, query.height)
        & !used_rows(target, query.height);

    for removed in operations.iter().copied() {
        if removed == target || used_rows(removed, query.height) & target_full_rows != 0 {
            continue;
        }
        metrics.necessity_checks += 1;

        let field_without = board_difference(non_target, removed.mask());
        let full_rows_without = full_rows(field_without, query.height);
        if !target_shape_ready(query, target, field_without, full_rows_without) {
            continue;
        }

        let one_piece_rows_without = original_one_piece_rows & !used_rows(removed, query.height);
        let all_still_grounded = operations.iter().copied().all(|operation| {
            let board_without_operation = board_difference(field_without, operation.mask());
            reference_operation_is_grounded(
                query,
                operation,
                board_without_operation,
                full_rows_without,
                one_piece_rows_without,
            )
        });
        if !all_still_grounded {
            continue;
        }

        let target_entry_remains =
            target_physical_context(query, target, field_without, full_rows_without).is_some_and(
                |(board_before, target_mask)| {
                    verifier.target_has_scoring_entry(
                        query,
                        board_before,
                        target.piece(),
                        target_mask,
                        target_full_rows,
                    )
                },
            );
        if target_entry_remains {
            metrics.necessity_rejections += 1;
            return false;
        }
    }
    true
}

fn reference_operation_is_grounded(
    query: &SpinStructureQuery,
    operation: StructureOperation,
    merged_without_operation_and_target: StructureBoard,
    all_full_rows: u32,
    one_piece_full_rows_without_target: u32,
) -> bool {
    let using_rows = used_rows(operation, query.height);
    let fill_rows_not_using_operation = all_full_rows & !using_rows;
    let need_deleted = operation.need_deleted_rows();
    if need_deleted & !fill_rows_not_using_operation != 0 {
        return false;
    }

    if operation_is_grounded(operation, query.initial_board, need_deleted, query.height) {
        return true;
    }

    let below_operation = if using_rows == 0 {
        0
    } else {
        (1_u32 << using_rows.trailing_zeros()) - 1
    };
    let one_piece_below = below_operation & one_piece_full_rows_without_target;
    operation_is_grounded(
        operation,
        merged_without_operation_and_target,
        need_deleted | one_piece_below,
        query.height,
    )
}

fn one_piece_full_rows(operations: &[StructureOperation], full_rows: u32, height: u8) -> u32 {
    let mut one_piece = 0_u32;
    for row in 0..height {
        let bit = 1_u32 << row;
        if full_rows & bit == 0 {
            continue;
        }
        let users = operations
            .iter()
            .filter(|operation| operation.mask().row_bits(row) != 0)
            .take(2)
            .count();
        if users == 1 {
            one_piece |= bit;
        }
    }
    one_piece
}

fn target_shape_ready(
    query: &SpinStructureQuery,
    target: StructureOperation,
    non_target: StructureBoard,
    deleted: u32,
) -> bool {
    let Some(target_physical) = project_operation(target, deleted, query.height) else {
        return false;
    };
    let board_physical = compact_board(non_target, deleted, query.height);
    let immobile = mask_is_immobile(target_physical.mask, board_physical);

    if target.piece() != PieceKind::T {
        return !query.mode.t_only() && immobile;
    }

    let (center_x, center_y) = t_center(target_physical);
    let blocked_corners = [
        (center_x - 1, center_y - 1),
        (center_x + 1, center_y - 1),
        (center_x - 1, center_y + 1),
        (center_x + 1, center_y + 1),
    ]
    .into_iter()
    .filter(|(x, y)| is_blocked(board_physical, *x, *y))
    .count();
    blocked_corners >= 3 || (query.mode.plus() && immobile)
}

fn target_physical_context(
    query: &SpinStructureQuery,
    target: StructureOperation,
    non_target: StructureBoard,
    deleted: u32,
) -> Option<(StructureBoard, StructureBoard)> {
    let target_physical = project_operation(target, deleted, query.height)?;
    Some((
        compact_board(non_target, deleted, query.height),
        target_physical.mask,
    ))
}

#[derive(Clone, Copy, Debug)]
struct PhysicalOperation {
    mask: StructureBoard,
    rotation: RotationState,
    x: i8,
    y: i8,
}

fn project_operation(
    operation: StructureOperation,
    deleted: u32,
    height: u8,
) -> Option<PhysicalOperation> {
    if operation.need_deleted_rows() & !deleted != 0 || used_rows(operation, height) & deleted != 0
    {
        return None;
    }
    let mask = compact_board(operation.mask(), deleted, height);
    let shape = standard_tetromino_registry()
        .get(operation.piece())?
        .shape(operation.rotation());
    let minimum_shape_y = shape.cells().iter().map(|cell| cell.y()).min()?;
    let minimum_physical_y = (0..height).find(|row| mask.row_bits(*row) != 0)?;
    Some(PhysicalOperation {
        mask,
        rotation: operation.rotation(),
        x: operation.x(),
        y: i8::try_from(i16::from(minimum_physical_y) - i16::from(minimum_shape_y)).ok()?,
    })
}

fn collect_open_translation_cells(
    target: StructureBoard,
    board: StructureBoard,
    output: &mut BTreeSet<(i16, i16)>,
) {
    for (dx, dy) in [(0_i16, -1_i16), (-1, 0), (1, 0), (0, 1)] {
        let mut translated = Vec::with_capacity(4);
        let mut blocked = false;
        for y in 0..StructureBoard::MAX_HEIGHT {
            for x in 0..StructureBoard::WIDTH {
                if !target.contains(x, y) {
                    continue;
                }
                let next_x = i16::from(x) + dx;
                let next_y = i16::from(y) + dy;
                if next_x < 0 || next_x >= i16::from(StructureBoard::WIDTH) || next_y < 0 {
                    blocked = true;
                    break;
                }
                if next_y < i16::from(StructureBoard::MAX_HEIGHT)
                    && board.contains(next_x as u8, next_y as u8)
                {
                    blocked = true;
                    break;
                }
                translated.push((next_x, next_y));
            }
            if blocked {
                break;
            }
        }
        if !blocked {
            output.extend(translated);
        }
    }
}

fn t_center(operation: PhysicalOperation) -> (i16, i16) {
    match operation.rotation {
        RotationState::Zero => (i16::from(operation.x) + 1, i16::from(operation.y)),
        RotationState::Right => (i16::from(operation.x), i16::from(operation.y) + 1),
        RotationState::Two | RotationState::Left => {
            (i16::from(operation.x) + 1, i16::from(operation.y) + 1)
        }
    }
}

fn is_blocked(board: StructureBoard, x: i16, y: i16) -> bool {
    x < 0
        || x >= i16::from(StructureBoard::WIDTH)
        || y < 0
        || (y < i16::from(StructureBoard::MAX_HEIGHT) && board.contains(x as u8, y as u8))
}

fn mask_is_grounded(mask: StructureBoard, board: StructureBoard) -> bool {
    for y in 0..StructureBoard::MAX_HEIGHT {
        for x in 0..StructureBoard::WIDTH {
            if !mask.contains(x, y) {
                continue;
            }
            if y == 0 || board.contains(x, y - 1) {
                return true;
            }
        }
    }
    false
}

fn mask_is_immobile(mask: StructureBoard, board: StructureBoard) -> bool {
    [(0_i16, -1_i16), (-1, 0), (1, 0), (0, 1)]
        .into_iter()
        .all(|(dx, dy)| translation_is_blocked(mask, board, dx, dy))
}

fn translation_is_blocked(mask: StructureBoard, board: StructureBoard, dx: i16, dy: i16) -> bool {
    for y in 0..StructureBoard::MAX_HEIGHT {
        for x in 0..StructureBoard::WIDTH {
            if !mask.contains(x, y) {
                continue;
            }
            let next_x = i16::from(x) + dx;
            let next_y = i16::from(y) + dy;
            if next_x < 0 || next_x >= i16::from(StructureBoard::WIDTH) || next_y < 0 {
                return true;
            }
            if next_y < i16::from(StructureBoard::MAX_HEIGHT)
                && board.contains(next_x as u8, next_y as u8)
            {
                return true;
            }
        }
    }
    false
}

fn logical_row_for_physical(deleted: u32, physical: u8, height: u8) -> Option<u8> {
    let mut alive = 0_u8;
    for logical in 0..height {
        if deleted & (1_u32 << logical) != 0 {
            continue;
        }
        if alive == physical {
            return Some(logical);
        }
        alive += 1;
    }
    None
}

fn compact_board(board: StructureBoard, deleted: u32, height: u8) -> StructureBoard {
    let mut rows = Vec::with_capacity(usize::from(height));
    for logical in 0..height {
        if deleted & (1_u32 << logical) == 0 {
            rows.push(board.row_bits(logical));
        }
    }
    StructureBoard::from_rows(&rows).expect("bounded compact board")
}

fn remaining_inventory(
    inventory: PieceInventory,
    operations: &[StructureOperation],
) -> Option<PieceInventory> {
    operations
        .iter()
        .try_fold(inventory, |remaining, operation| {
            remaining.take(operation.piece())
        })
}

fn union_operations(operations: &[StructureOperation]) -> StructureBoard {
    operations
        .iter()
        .fold(StructureBoard::EMPTY, |board, operation| {
            board.union(operation.mask())
        })
}

fn union_without_target(
    operations: &[StructureOperation],
    target: StructureOperation,
) -> StructureBoard {
    let mut removed = false;
    operations
        .iter()
        .filter(|operation| {
            if !removed && **operation == target {
                removed = true;
                false
            } else {
                true
            }
        })
        .fold(StructureBoard::EMPTY, |board, operation| {
            board.union(operation.mask())
        })
}

fn board_difference(left: StructureBoard, right: StructureBoard) -> StructureBoard {
    let left = left.words();
    let right = right.words();
    StructureBoard::from_words([
        left[0] & !right[0],
        left[1] & !right[1],
        left[2] & !right[2],
        left[3] & !right[3],
    ])
}

fn full_rows(board: StructureBoard, height: u8) -> u32 {
    (0..height).fold(0_u32, |rows, row| {
        rows | u32::from(board.row_bits(row) == 0x03ff) << row
    })
}

fn used_rows(operation: StructureOperation, height: u8) -> u32 {
    (0..height).fold(0_u32, |rows, row| {
        rows | u32::from(operation.mask().row_bits(row) != 0) << row
    })
}

fn strict_subset(left: &[StructureOperation], right: &[StructureOperation]) -> bool {
    if left.len() >= right.len() {
        return false;
    }
    let mut left_index = 0;
    let mut right_index = 0;
    while left_index < left.len() && right_index < right.len() {
        match left[left_index].cmp(&right[right_index]) {
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => {
                left_index += 1;
                right_index += 1;
            }
            std::cmp::Ordering::Greater => right_index += 1,
        }
    }
    left_index == left.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_board_removes_exact_logical_rows() {
        let board = StructureBoard::from_rows(&[1, 2, 4, 8]).expect("field");
        let compact = compact_board(board, (1 << 1) | (1 << 3), 4);
        assert_eq!(compact.row_bits(0), 1);
        assert_eq!(compact.row_bits(1), 4);
        assert_eq!(compact.row_bits(2), 0);
    }

    #[test]
    fn operation_set_subset_is_exact() {
        let operation = |piece, x| {
            StructureOperation::new(
                piece,
                clearra_core_domain::piece::rotation::RotationState::Zero,
                x,
                0,
                StructureBoard::from_rows(&[0b1111]).expect("mask"),
                0,
            )
        };
        let a = operation(PieceKind::I, 0);
        let b = operation(PieceKind::T, 1);
        let c = operation(PieceKind::Z, 2);
        assert!(strict_subset(&[a], &[a, b]));
        assert!(!strict_subset(&[b], &[a, c]));
    }

    #[test]
    fn declared_search_ceiling_is_not_an_immobility_wall() {
        let top = StructureBoard::MAX_HEIGHT - 1;
        let mask = StructureBoard::EMPTY.with_cell(0, top).expect("mask");
        let side_and_floor = StructureBoard::EMPTY
            .with_cell(1, top)
            .expect("right blocker")
            .with_cell(0, top - 1)
            .expect("floor blocker");
        assert!(!mask_is_immobile(mask, side_and_floor));
    }

    #[test]
    fn immobility_requires_all_four_translations_to_be_blocked() {
        let mask = StructureBoard::from_rows(&[0b0110]).expect("mask");
        let board = StructureBoard::from_rows(&[0b1001, 0b0110]).expect("blockers");
        assert!(mask_is_immobile(mask, board));
    }
}
