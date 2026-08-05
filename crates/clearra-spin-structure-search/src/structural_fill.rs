use crate::{
    board::StructureBoard,
    model::{PieceInventory, SpinStructureQuery, StructureOperation},
    operation_catalog::LogicalOperationCatalog,
};

/// A geometry-complete fill candidate with the reserved target operation
/// included in its canonical operation set.
///
/// This is deliberately not a build witness. Temporal ordering, support and
/// reachability are verified by later exact stages.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FillSeed {
    target: StructureOperation,
    operations: Box<[StructureOperation]>,
    new_full_rows: u32,
    target_full_rows: u32,
    remaining_inventory: PieceInventory,
}

impl FillSeed {
    #[cfg(test)]
    pub(crate) const fn target(&self) -> StructureOperation {
        self.target
    }

    pub(crate) fn operations(&self) -> &[StructureOperation] {
        &self.operations
    }

    #[cfg(test)]
    pub(crate) const fn new_full_rows(&self) -> u32 {
        self.new_full_rows
    }

    #[cfg(test)]
    pub(crate) const fn target_full_rows(&self) -> u32 {
        self.target_full_rows
    }

    #[cfg(test)]
    pub(crate) const fn remaining_inventory(&self) -> PieceInventory {
        self.remaining_inventory
    }
}

/// Fixed, low-overhead counters for the target-first fill stage.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct FillSeedMetrics {
    pub(crate) targets_considered: u64,
    pub(crate) targets_rejected: u64,
    pub(crate) row_subsets_considered: u64,
    pub(crate) row_subsets_searched: u64,
    pub(crate) line_requirement_rejections: u64,
    pub(crate) search_nodes: u64,
    pub(crate) pivot_cells_scanned: u64,
    pub(crate) candidate_checks: u64,
    pub(crate) collision_rejections: u64,
    pub(crate) deleted_row_rejections: u64,
    pub(crate) piece_supply_rejections: u64,
    pub(crate) area_bound_prunes: u64,
    pub(crate) empty_domain_prunes: u64,
    pub(crate) outside_full_row_prunes: u64,
    pub(crate) exact_covers: u64,
    pub(crate) duplicate_seeds: u64,
}

impl FillSeedMetrics {
    fn absorb(&mut self, other: Self) {
        self.targets_considered += other.targets_considered;
        self.targets_rejected += other.targets_rejected;
        self.row_subsets_considered += other.row_subsets_considered;
        self.row_subsets_searched += other.row_subsets_searched;
        self.line_requirement_rejections += other.line_requirement_rejections;
        self.search_nodes += other.search_nodes;
        self.pivot_cells_scanned += other.pivot_cells_scanned;
        self.candidate_checks += other.candidate_checks;
        self.collision_rejections += other.collision_rejections;
        self.deleted_row_rejections += other.deleted_row_rejections;
        self.piece_supply_rejections += other.piece_supply_rejections;
        self.area_bound_prunes += other.area_bound_prunes;
        self.empty_domain_prunes += other.empty_domain_prunes;
        self.outside_full_row_prunes += other.outside_full_row_prunes;
        self.exact_covers += other.exact_covers;
        self.duplicate_seeds += other.duplicate_seeds;
    }
}

/// Enumerates every exact logical fill seed for one reserved target.
///
/// The optional metrics sink is additive so callers can accumulate many
/// target operations without allocating a per-target report.
pub(crate) fn enumerate_fill_seeds(
    query: &SpinStructureQuery,
    catalog: &LogicalOperationCatalog,
    target: StructureOperation,
    metrics: Option<&mut FillSeedMetrics>,
) -> Vec<FillSeed> {
    let mut local_metrics = FillSeedMetrics {
        targets_considered: 1,
        ..FillSeedMetrics::default()
    };
    let mut seeds = enumerate_fill_seeds_inner(query, catalog, target, &mut local_metrics);
    seeds.sort_unstable_by(|left, right| left.operations.cmp(&right.operations));
    let before = seeds.len();
    seeds.dedup_by(|left, right| left.operations == right.operations);
    local_metrics.duplicate_seeds += (before - seeds.len()) as u64;
    if let Some(metrics) = metrics {
        metrics.absorb(local_metrics);
    }
    seeds
}

fn enumerate_fill_seeds_inner(
    query: &SpinStructureQuery,
    catalog: &LogicalOperationCatalog,
    target: StructureOperation,
    metrics: &mut FillSeedMetrics,
) -> Vec<FillSeed> {
    if query.validate().is_err()
        || catalog.height() != query.height
        || catalog.operations().binary_search(&target).is_err()
    {
        metrics.targets_rejected += 1;
        return Vec::new();
    }
    let Some(remaining_inventory) = query.inventory.take(target.piece()) else {
        metrics.targets_rejected += 1;
        return Vec::new();
    };
    let placement_limit = usize::from(query.placement_limit());
    if placement_limit == 0 || target.mask().intersects(query.initial_board) {
        metrics.targets_rejected += 1;
        return Vec::new();
    }

    let initial_full_rows = full_rows(query.initial_board, query.height);
    let fill_window = row_range_mask(query.fill_bottom, query.fill_top);
    let eligible_new_rows = fill_window & !initial_full_rows;
    let target_rows = occupied_rows(target.mask(), query.height);
    let target_base = query.initial_board.union(target.mask());
    let target_created_rows = full_rows(target_base, query.height) & !initial_full_rows;
    if target_created_rows & !eligible_new_rows != 0 {
        metrics.targets_rejected += 1;
        return Vec::new();
    }

    let target_required_new = target.need_deleted_rows() & !initial_full_rows;
    if target_required_new & !eligible_new_rows != 0 {
        metrics.targets_rejected += 1;
        return Vec::new();
    }
    let forced_new_rows = target_created_rows | target_required_new;
    let variable_new_rows = eligible_new_rows & !forced_new_rows;
    let mut selected_variable_rows = variable_new_rows;
    let mut seeds = Vec::new();

    loop {
        let new_full_rows = forced_new_rows | selected_variable_rows;
        metrics.row_subsets_considered += 1;
        let target_full_rows = new_full_rows & target_rows;
        if !query
            .line_requirement
            .accepts(target_full_rows.count_ones() as u8)
        {
            metrics.line_requirement_rejections += 1;
        } else {
            metrics.row_subsets_searched += 1;
            let allowed_full_rows = initial_full_rows | new_full_rows;
            let required_cells =
                difference(board_for_rows(new_full_rows, query.height), target_base);
            let mut selected = Vec::new();
            let mut search = FillSearch {
                query,
                catalog,
                target,
                initial_full_rows,
                new_full_rows,
                target_full_rows,
                allowed_full_rows,
                placement_limit,
                metrics,
                seeds: &mut seeds,
            };
            search.visit(
                target_base,
                required_cells,
                remaining_inventory,
                &mut selected,
            );
        }

        if selected_variable_rows == 0 {
            break;
        }
        selected_variable_rows = (selected_variable_rows - 1) & variable_new_rows;
    }

    seeds
}

struct FillSearch<'a> {
    query: &'a SpinStructureQuery,
    catalog: &'a LogicalOperationCatalog,
    target: StructureOperation,
    initial_full_rows: u32,
    new_full_rows: u32,
    target_full_rows: u32,
    allowed_full_rows: u32,
    placement_limit: usize,
    metrics: &'a mut FillSeedMetrics,
    seeds: &'a mut Vec<FillSeed>,
}

impl FillSearch<'_> {
    fn visit(
        &mut self,
        occupied: StructureBoard,
        uncovered: StructureBoard,
        remaining: PieceInventory,
        selected: &mut Vec<StructureOperation>,
    ) {
        self.metrics.search_nodes += 1;
        if uncovered.is_empty() {
            self.accept_exact_cover(occupied, remaining, selected);
            return;
        }

        let remaining_cell_count = cell_count(uncovered) as usize;
        let minimum_more_operations = remaining_cell_count.div_ceil(4);
        let placement_slots = self
            .placement_limit
            .saturating_sub(selected.len().saturating_add(1));
        if minimum_more_operations > placement_slots
            || minimum_more_operations > usize::from(remaining.total())
        {
            self.metrics.area_bound_prunes += 1;
            return;
        }

        let Some(candidates) = self.minimum_domain(occupied, uncovered, remaining) else {
            self.metrics.empty_domain_prunes += 1;
            return;
        };
        if candidates.is_empty() {
            self.metrics.empty_domain_prunes += 1;
            return;
        }

        for operation in candidates {
            let Some(next_remaining) = remaining.take(operation.piece()) else {
                continue;
            };
            let next_occupied = occupied.union(operation.mask());
            let next_uncovered = difference(uncovered, operation.mask());
            selected.push(operation);
            self.visit(next_occupied, next_uncovered, next_remaining, selected);
            selected.pop();
        }
    }

    fn minimum_domain(
        &mut self,
        occupied: StructureBoard,
        uncovered: StructureBoard,
        remaining: PieceInventory,
    ) -> Option<Vec<StructureOperation>> {
        let mut best: Option<Vec<StructureOperation>> = None;
        for y in 0..self.query.height {
            let row = uncovered.row_bits(y);
            for x in 0..StructureBoard::WIDTH {
                if row & (1_u16 << x) == 0 {
                    continue;
                }
                self.metrics.pivot_cells_scanned += 1;
                let mut candidates = Vec::new();
                for operation_id in self.catalog.operation_ids_for_cell(x, y) {
                    self.metrics.candidate_checks += 1;
                    let Some(operation) = self.catalog.operation(*operation_id) else {
                        continue;
                    };
                    if remaining.count(operation.piece()) == 0 {
                        self.metrics.piece_supply_rejections += 1;
                        continue;
                    }
                    if operation.mask().intersects(occupied) {
                        self.metrics.collision_rejections += 1;
                        continue;
                    }
                    if operation.need_deleted_rows() & !self.allowed_full_rows != 0 {
                        self.metrics.deleted_row_rejections += 1;
                        continue;
                    }
                    let next_occupied = occupied.union(operation.mask());
                    let outside_full_rows = full_rows(next_occupied, self.query.height)
                        & !self.initial_full_rows
                        & !self.new_full_rows;
                    if outside_full_rows != 0 {
                        self.metrics.outside_full_row_prunes += 1;
                        continue;
                    }
                    candidates.push(operation);
                }
                if candidates.is_empty() {
                    return Some(candidates);
                }
                if best
                    .as_ref()
                    .is_none_or(|current| candidates.len() < current.len())
                {
                    best = Some(candidates);
                }
            }
        }
        best
    }

    fn accept_exact_cover(
        &mut self,
        occupied: StructureBoard,
        remaining: PieceInventory,
        selected: &[StructureOperation],
    ) {
        let new_full_rows = full_rows(occupied, self.query.height) & !self.initial_full_rows;
        if new_full_rows != self.new_full_rows {
            self.metrics.outside_full_row_prunes += 1;
            return;
        }

        let mut operations = Vec::with_capacity(selected.len() + 1);
        operations.push(self.target);
        operations.extend_from_slice(selected);
        operations.sort_unstable();
        operations.dedup();
        debug_assert_eq!(operations.len(), selected.len() + 1);
        self.seeds.push(FillSeed {
            target: self.target,
            operations: operations.into_boxed_slice(),
            new_full_rows: self.new_full_rows,
            target_full_rows: self.target_full_rows,
            remaining_inventory: remaining,
        });
        self.metrics.exact_covers += 1;
    }
}

fn row_range_mask(bottom: u8, top: u8) -> u32 {
    let below_top = if top == 32 {
        u32::MAX
    } else {
        (1_u32 << top) - 1
    };
    let below_bottom = (1_u32 << bottom) - 1;
    below_top & !below_bottom
}

fn occupied_rows(board: StructureBoard, height: u8) -> u32 {
    let mut rows = 0_u32;
    for row in 0..height {
        if board.row_bits(row) != 0 {
            rows |= 1_u32 << row;
        }
    }
    rows
}

fn full_rows(board: StructureBoard, height: u8) -> u32 {
    let mut rows = 0_u32;
    for row in 0..height {
        if board.row_bits(row) == 0x03ff {
            rows |= 1_u32 << row;
        }
    }
    rows
}

fn board_for_rows(rows: u32, height: u8) -> StructureBoard {
    let mut board = StructureBoard::EMPTY;
    for row in 0..height {
        if rows & (1_u32 << row) == 0 {
            continue;
        }
        for x in 0..StructureBoard::WIDTH {
            board.insert_index(u16::from(row) * u16::from(StructureBoard::WIDTH) + u16::from(x));
        }
    }
    board
}

fn difference(left: StructureBoard, right: StructureBoard) -> StructureBoard {
    let left = left.words();
    let right = right.words();
    StructureBoard::from_words([
        left[0] & !right[0],
        left[1] & !right[1],
        left[2] & !right[2],
        left[3] & !right[3],
    ])
}

#[cfg(test)]
fn intersection(left: StructureBoard, right: StructureBoard) -> StructureBoard {
    let left = left.words();
    let right = right.words();
    StructureBoard::from_words([
        left[0] & right[0],
        left[1] & right[1],
        left[2] & right[2],
        left[3] & right[3],
    ])
}

fn cell_count(board: StructureBoard) -> u32 {
    board.words().into_iter().map(u64::count_ones).sum()
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

    use super::*;
    use crate::{
        model::{SpinLineRequirement, SpinStructureMode},
        operation_catalog::OperationGeometryKey,
    };

    fn inventory(pieces: &[PieceKind]) -> PieceInventory {
        PieceInventory::from_pieces(pieces.iter().copied()).expect("bounded inventory")
    }

    fn query(
        initial_board: StructureBoard,
        pieces: &[PieceKind],
        line_requirement: SpinLineRequirement,
        fill_bottom: u8,
        fill_top: u8,
    ) -> SpinStructureQuery {
        let mut query = SpinStructureQuery::new(inventory(pieces), SpinStructureMode::AllSpinPlus);
        query.initial_board = initial_board;
        query.height = 4;
        query.fill_bottom = fill_bottom;
        query.fill_top = fill_top;
        query.line_requirement = line_requirement;
        query
    }

    fn target(
        catalog: &LogicalOperationCatalog,
        piece: PieceKind,
        rotation: RotationState,
        x: i8,
        mask: StructureBoard,
    ) -> StructureOperation {
        catalog
            .operations_for_geometry(OperationGeometryKey::new(piece, rotation, x))
            .iter()
            .copied()
            .find(|operation| operation.mask() == mask)
            .expect("target operation")
    }

    #[test]
    fn optimized_fill_matches_a_small_brute_force_oracle() {
        let initial = StructureBoard::from_rows(&[0b0011100000]).expect("field");
        let query = query(
            initial,
            &[PieceKind::T, PieceKind::O, PieceKind::O],
            SpinLineRequirement::Exact(1),
            0,
            1,
        );
        let catalog = LogicalOperationCatalog::compile(query.height, initial, query.inventory)
            .expect("catalog");
        let target = target(
            &catalog,
            PieceKind::T,
            RotationState::Zero,
            0,
            StructureBoard::from_rows(&[0b111, 0b010]).expect("T target"),
        );

        let optimized = enumerate_fill_seeds(&query, &catalog, target, None);
        let brute = brute_force_fill_seeds(&query, &catalog, target);

        assert_eq!(optimized, brute);
        assert_eq!(optimized.len(), 1);
        assert_eq!(optimized[0].operations().len(), 3);
    }

    #[test]
    fn initial_full_row_satisfies_a_gapped_target_dependency() {
        let initial = StructureBoard::from_rows(&[0b1111111000, 0x03ff]).expect("field");
        let query = query(
            initial,
            &[PieceKind::T],
            SpinLineRequirement::Exact(1),
            0,
            1,
        );
        let catalog = LogicalOperationCatalog::compile(query.height, initial, query.inventory)
            .expect("catalog");
        let target = target(
            &catalog,
            PieceKind::T,
            RotationState::Zero,
            0,
            StructureBoard::from_rows(&[0b111, 0, 0b010]).expect("gapped T"),
        );

        let seeds = enumerate_fill_seeds(&query, &catalog, target, None);

        assert_eq!(target.need_deleted_rows(), 1 << 1);
        assert_eq!(seeds.len(), 1);
        assert_eq!(seeds[0].new_full_rows(), 1);
        assert_eq!(seeds[0].target_full_rows(), 1);
        assert_eq!(seeds[0].operations(), &[target]);
    }

    #[test]
    fn unordered_inventory_multiplicity_is_reserved_exactly() {
        let initial = StructureBoard::from_rows(&[0b0011100000]).expect("field");
        let one_query = query(
            initial,
            &[PieceKind::T, PieceKind::O],
            SpinLineRequirement::Exact(1),
            0,
            1,
        );
        let one_catalog =
            LogicalOperationCatalog::compile(4, initial, one_query.inventory).expect("catalog");
        let one_target = target(
            &one_catalog,
            PieceKind::T,
            RotationState::Zero,
            0,
            StructureBoard::from_rows(&[0b111, 0b010]).expect("T target"),
        );
        assert!(enumerate_fill_seeds(&one_query, &one_catalog, one_target, None).is_empty());

        let two_query = query(
            initial,
            &[PieceKind::T, PieceKind::O, PieceKind::O],
            SpinLineRequirement::Exact(1),
            0,
            1,
        );
        let two_catalog =
            LogicalOperationCatalog::compile(4, initial, two_query.inventory).expect("catalog");
        let two_target = target(
            &two_catalog,
            PieceKind::T,
            RotationState::Zero,
            0,
            StructureBoard::from_rows(&[0b111, 0b010]).expect("T target"),
        );
        let seeds = enumerate_fill_seeds(&two_query, &two_catalog, two_target, None);
        assert_eq!(seeds.len(), 1);
        assert_eq!(seeds[0].remaining_inventory().count(PieceKind::O), 0);
    }

    #[test]
    fn line_requirements_count_only_new_rows_occupied_by_the_target() {
        let empty_query = query(
            StructureBoard::EMPTY,
            &[PieceKind::T],
            SpinLineRequirement::Exact(0),
            0,
            2,
        );
        let empty_catalog =
            LogicalOperationCatalog::compile(4, StructureBoard::EMPTY, empty_query.inventory)
                .expect("catalog");
        let empty_target = target(
            &empty_catalog,
            PieceKind::T,
            RotationState::Zero,
            0,
            StructureBoard::from_rows(&[0b111, 0b010]).expect("T target"),
        );
        assert_eq!(
            enumerate_fill_seeds(&empty_query, &empty_catalog, empty_target, None).len(),
            1
        );

        let mut any_query = empty_query.clone();
        any_query.line_requirement = SpinLineRequirement::Any;
        assert_eq!(
            enumerate_fill_seeds(&any_query, &empty_catalog, empty_target, None).len(),
            1
        );

        let mut at_least_query = empty_query.clone();
        at_least_query.line_requirement = SpinLineRequirement::AtLeast(1);
        assert!(
            enumerate_fill_seeds(&at_least_query, &empty_catalog, empty_target, None).is_empty()
        );

        let initial =
            StructureBoard::from_rows(&[0b1111111000, 0b1111111101]).expect("two-row setup");
        let exact_two_query = query(
            initial,
            &[PieceKind::T],
            SpinLineRequirement::Exact(2),
            0,
            2,
        );
        let exact_two_catalog =
            LogicalOperationCatalog::compile(4, initial, exact_two_query.inventory)
                .expect("catalog");
        let exact_two_target = target(
            &exact_two_catalog,
            PieceKind::T,
            RotationState::Zero,
            0,
            StructureBoard::from_rows(&[0b111, 0b010]).expect("T target"),
        );
        let seeds =
            enumerate_fill_seeds(&exact_two_query, &exact_two_catalog, exact_two_target, None);
        assert_eq!(seeds.len(), 1);
        assert_eq!(seeds[0].target_full_rows().count_ones(), 2);
    }

    #[test]
    fn a_new_full_row_outside_the_selected_window_is_rejected() {
        let initial = StructureBoard::from_rows(&[0b0011100000, 0b0011100101])
            .expect("field with an outside near-full row");
        let query = query(
            initial,
            &[PieceKind::T, PieceKind::O, PieceKind::O],
            SpinLineRequirement::Exact(1),
            0,
            1,
        );
        let catalog =
            LogicalOperationCatalog::compile(4, initial, query.inventory).expect("catalog");
        let target = target(
            &catalog,
            PieceKind::T,
            RotationState::Zero,
            0,
            StructureBoard::from_rows(&[0b111, 0b010]).expect("T target"),
        );

        assert!(enumerate_fill_seeds(&query, &catalog, target, None).is_empty());
    }

    #[test]
    fn non_t_target_uses_the_same_exact_fill_contract() {
        let initial = StructureBoard::from_rows(&[0b1111110000]).expect("field");
        let query = query(
            initial,
            &[PieceKind::I],
            SpinLineRequirement::AtLeast(1),
            0,
            1,
        );
        let catalog =
            LogicalOperationCatalog::compile(4, initial, query.inventory).expect("catalog");
        let target = target(
            &catalog,
            PieceKind::I,
            RotationState::Zero,
            0,
            StructureBoard::from_rows(&[0b1111]).expect("I target"),
        );

        let seeds = enumerate_fill_seeds(&query, &catalog, target, None);

        assert_eq!(seeds.len(), 1);
        assert_eq!(seeds[0].target(), target);
        assert_eq!(seeds[0].operations(), &[target]);
    }

    fn brute_force_fill_seeds(
        query: &SpinStructureQuery,
        catalog: &LogicalOperationCatalog,
        target: StructureOperation,
    ) -> Vec<FillSeed> {
        let initial_full_rows = full_rows(query.initial_board, query.height);
        let fill_window = row_range_mask(query.fill_bottom, query.fill_top);
        let target_rows = occupied_rows(target.mask(), query.height);
        let target_base = query.initial_board.union(target.mask());
        let target_created_rows = full_rows(target_base, query.height) & !initial_full_rows;
        let required_new = target.need_deleted_rows() & !initial_full_rows;
        let eligible = fill_window & !initial_full_rows;
        if (target_created_rows | required_new) & !eligible != 0 {
            return Vec::new();
        }
        let forced = target_created_rows | required_new;
        let variable = eligible & !forced;
        let Some(remaining) = query.inventory.take(target.piece()) else {
            return Vec::new();
        };
        let mut candidates = catalog
            .operations()
            .iter()
            .copied()
            .filter(|operation| {
                remaining.count(operation.piece()) != 0 && !operation.mask().intersects(target_base)
            })
            .collect::<Vec<_>>();
        candidates.sort_unstable();

        let mut seeds = Vec::new();
        let mut subset = variable;
        loop {
            let new_full_rows = forced | subset;
            let target_full_rows = new_full_rows & target_rows;
            if query
                .line_requirement
                .accepts(target_full_rows.count_ones() as u8)
            {
                let allowed = initial_full_rows | new_full_rows;
                let required = difference(board_for_rows(new_full_rows, query.height), target_base);
                let mut selected = Vec::new();
                let context = BruteContext {
                    query,
                    target,
                    candidates: &candidates,
                    initial_full_rows,
                    new_full_rows,
                    target_full_rows,
                    allowed_full_rows: allowed,
                    required_cells: required,
                };
                brute_choose(
                    &context,
                    0,
                    target_base,
                    remaining,
                    &mut selected,
                    &mut seeds,
                );
            }
            if subset == 0 {
                break;
            }
            subset = (subset - 1) & variable;
        }
        seeds.sort_unstable_by(|left, right| left.operations.cmp(&right.operations));
        seeds.dedup_by(|left, right| left.operations == right.operations);
        seeds
    }

    struct BruteContext<'a> {
        query: &'a SpinStructureQuery,
        target: StructureOperation,
        candidates: &'a [StructureOperation],
        initial_full_rows: u32,
        new_full_rows: u32,
        target_full_rows: u32,
        allowed_full_rows: u32,
        required_cells: StructureBoard,
    }

    #[allow(clippy::too_many_arguments)]
    fn brute_choose(
        context: &BruteContext<'_>,
        begin: usize,
        occupied: StructureBoard,
        remaining: PieceInventory,
        selected: &mut Vec<StructureOperation>,
        seeds: &mut Vec<FillSeed>,
    ) {
        let covered = intersection(occupied, context.required_cells);
        if covered == context.required_cells
            && full_rows(occupied, context.query.height) & !context.initial_full_rows
                == context.new_full_rows
        {
            let mut operations = selected.clone();
            operations.push(context.target);
            operations.sort_unstable();
            seeds.push(FillSeed {
                target: context.target,
                operations: operations.into_boxed_slice(),
                new_full_rows: context.new_full_rows,
                target_full_rows: context.target_full_rows,
                remaining_inventory: remaining,
            });
        }
        if selected.len() + 1 >= usize::from(context.query.placement_limit()) {
            return;
        }

        for index in begin..context.candidates.len() {
            let operation = context.candidates[index];
            let Some(next_remaining) = remaining.take(operation.piece()) else {
                continue;
            };
            if operation.mask().intersects(occupied)
                || operation.need_deleted_rows() & !context.allowed_full_rows != 0
                || intersection(operation.mask(), context.required_cells).is_empty()
            {
                continue;
            }
            let next_occupied = occupied.union(operation.mask());
            if full_rows(next_occupied, context.query.height)
                & !context.initial_full_rows
                & !context.new_full_rows
                != 0
            {
                continue;
            }
            selected.push(operation);
            brute_choose(
                context,
                index + 1,
                next_occupied,
                next_remaining,
                selected,
                seeds,
            );
            selected.pop();
        }
    }
}
