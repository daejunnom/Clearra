use super::*;
use crate::cooperative_sort::CooperativeSort;
use std::sync::Arc;

struct Frame {
    occupied: StructureBoard,
    uncovered: StructureBoard,
    remaining: PieceInventory,
    entered: bool,
    cell: usize,
    cell_started: bool,
    operation: usize,
    candidates: Vec<StructureOperation>,
    best: Option<Vec<StructureOperation>>,
    children: Option<Vec<StructureOperation>>,
    child: usize,
}

impl Frame {
    fn new(occupied: StructureBoard, uncovered: StructureBoard, remaining: PieceInventory) -> Self {
        Self {
            occupied,
            uncovered,
            remaining,
            entered: false,
            cell: 0,
            cell_started: false,
            operation: 0,
            candidates: Vec::new(),
            best: None,
            children: None,
            child: 0,
        }
    }
}

enum Phase {
    Search,
    Sort,
    Deduplicate,
    Complete,
}

/// Resumable form of the target-first exact-cover DFS. Candidate-domain
/// scanning, DFS transitions, sorting and deduplication each yield explicitly.
pub(crate) struct FillCursor {
    query: SpinStructureQuery,
    catalog: Arc<LogicalOperationCatalog>,
    target: StructureOperation,
    remaining: PieceInventory,
    initial_full_rows: u32,
    forced_rows: u32,
    variable_rows: u32,
    next_rows: Option<u32>,
    target_rows: u32,
    target_base: StructureBoard,
    new_full_rows: u32,
    target_full_rows: u32,
    stack: Vec<Frame>,
    selected: Vec<StructureOperation>,
    seeds: Vec<FillSeed>,
    metrics: FillSeedMetrics,
    phase: Phase,
    sort: CooperativeSort,
    read: usize,
    write: usize,
}

impl FillCursor {
    pub(crate) fn new(
        query: SpinStructureQuery,
        catalog: Arc<LogicalOperationCatalog>,
        target: StructureOperation,
    ) -> Self {
        let remaining = query.inventory.take(target.piece());
        let initial_full_rows = full_rows(query.initial_board, query.height);
        let eligible = row_range_mask(query.fill_bottom, query.fill_top) & !initial_full_rows;
        let target_base = query.initial_board.union(target.mask());
        let created = full_rows(target_base, query.height) & !initial_full_rows;
        let required = target.need_deleted_rows() & !initial_full_rows;
        let forced = created | required;
        let variable = eligible & !forced;
        let invalid = query.validate().is_err()
            || catalog.height() != query.height
            || catalog.operations().binary_search(&target).is_err()
            || remaining.is_none()
            || query.placement_limit() == 0
            || target.mask().intersects(query.initial_board)
            || forced & !eligible != 0;
        let target_rows = occupied_rows(target.mask(), query.height);
        Self {
            query,
            catalog,
            target,
            remaining: remaining.unwrap_or_default(),
            initial_full_rows,
            forced_rows: forced,
            variable_rows: variable,
            next_rows: (!invalid).then_some(variable),
            target_rows,
            target_base,
            new_full_rows: 0,
            target_full_rows: 0,
            stack: Vec::new(),
            selected: Vec::new(),
            seeds: Vec::new(),
            metrics: FillSeedMetrics {
                targets_considered: 1,
                targets_rejected: u64::from(invalid),
                ..FillSeedMetrics::default()
            },
            phase: Phase::Search,
            sort: CooperativeSort::new(0),
            read: 0,
            write: 0,
        }
    }

    fn pop(&mut self) {
        self.stack.pop();
        if !self.stack.is_empty() {
            self.selected.pop();
        }
    }

    pub(crate) fn step(&mut self) -> bool {
        match self.phase {
            Phase::Sort => {
                if self
                    .sort
                    .step(&mut self.seeds, |a, b| a.operations.cmp(&b.operations))
                {
                    self.phase = Phase::Deduplicate;
                }
                return false;
            }
            Phase::Deduplicate => {
                if self.read == self.seeds.len() {
                    self.metrics.duplicate_seeds += (self.seeds.len() - self.write) as u64;
                    self.seeds.truncate(self.write);
                    self.phase = Phase::Complete;
                    return true;
                }
                if self.write == 0
                    || self.seeds[self.read].operations != self.seeds[self.write - 1].operations
                {
                    self.seeds.swap(self.read, self.write);
                    self.write += 1;
                }
                self.read += 1;
                return false;
            }
            Phase::Complete => return true,
            Phase::Search => {}
        }
        let Some(frame) = self.stack.last_mut() else {
            let Some(selected_rows) = self.next_rows else {
                self.sort = CooperativeSort::new(self.seeds.len());
                self.phase = Phase::Sort;
                return false;
            };
            self.next_rows = (selected_rows != 0).then(|| (selected_rows - 1) & self.variable_rows);
            self.new_full_rows = self.forced_rows | selected_rows;
            self.target_full_rows = self.new_full_rows & self.target_rows;
            self.metrics.row_subsets_considered += 1;
            if !self
                .query
                .line_requirement
                .accepts(self.target_full_rows.count_ones() as u8)
            {
                self.metrics.line_requirement_rejections += 1;
                return false;
            }
            self.metrics.row_subsets_searched += 1;
            let uncovered = difference(
                board_for_rows(self.new_full_rows, self.query.height),
                self.target_base,
            );
            self.stack
                .push(Frame::new(self.target_base, uncovered, self.remaining));
            return false;
        };
        if !frame.entered {
            self.metrics.search_nodes += 1;
            frame.entered = true;
            if frame.uncovered.is_empty() {
                if full_rows(frame.occupied, self.query.height) & !self.initial_full_rows
                    != self.new_full_rows
                {
                    self.metrics.outside_full_row_prunes += 1;
                } else {
                    let mut operations = self.selected.clone();
                    operations.push(self.target);
                    operations.sort_unstable();
                    operations.dedup();
                    self.seeds.push(FillSeed {
                        target: self.target,
                        operations: operations.into_boxed_slice(),
                        new_full_rows: self.new_full_rows,
                        target_full_rows: self.target_full_rows,
                        remaining_inventory: frame.remaining,
                    });
                    self.metrics.exact_covers += 1;
                }
                self.pop();
                return false;
            }
            let minimum = (cell_count(frame.uncovered) as usize).div_ceil(4);
            if minimum
                > usize::from(self.query.placement_limit()).saturating_sub(self.selected.len() + 1)
                || minimum > usize::from(frame.remaining.total())
            {
                self.metrics.area_bound_prunes += 1;
                self.pop();
            }
            return false;
        }
        if let Some(children) = &frame.children {
            if let Some(operation) = children.get(frame.child).copied() {
                frame.child += 1;
                let next = Frame::new(
                    frame.occupied.union(operation.mask()),
                    difference(frame.uncovered, operation.mask()),
                    frame
                        .remaining
                        .take(operation.piece())
                        .expect("domain requires supply"),
                );
                self.selected.push(operation);
                self.stack.push(next);
            } else {
                self.pop();
            }
            return false;
        }
        if frame.cell == usize::from(self.query.height) * 10 {
            if let Some(best) = frame.best.take() {
                frame.children = Some(best);
            } else {
                self.metrics.empty_domain_prunes += 1;
                self.pop();
            }
            return false;
        }
        let x = (frame.cell % 10) as u8;
        let y = (frame.cell / 10) as u8;
        if frame.uncovered.row_bits(y) & (1 << x) == 0 {
            frame.cell += 1;
            return false;
        }
        if !frame.cell_started {
            self.metrics.pivot_cells_scanned += 1;
            frame.cell_started = true;
        }
        let domain = self.catalog.operation_ids_for_cell(x, y);
        let Some(id) = domain.get(frame.operation).copied() else {
            if frame.candidates.is_empty() {
                self.metrics.empty_domain_prunes += 1;
                self.pop();
                return false;
            }
            if frame
                .best
                .as_ref()
                .is_none_or(|best| frame.candidates.len() < best.len())
            {
                frame.best = Some(std::mem::take(&mut frame.candidates));
            } else {
                frame.candidates.clear();
            }
            frame.cell += 1;
            frame.cell_started = false;
            frame.operation = 0;
            return false;
        };
        frame.operation += 1;
        self.metrics.candidate_checks += 1;
        let Some(operation) = self.catalog.operation(id) else {
            return false;
        };
        if frame.remaining.count(operation.piece()) == 0 {
            self.metrics.piece_supply_rejections += 1;
            return false;
        }
        if operation.mask().intersects(frame.occupied) {
            self.metrics.collision_rejections += 1;
            return false;
        }
        if operation.need_deleted_rows() & !(self.initial_full_rows | self.new_full_rows) != 0 {
            self.metrics.deleted_row_rejections += 1;
            return false;
        }
        if full_rows(frame.occupied.union(operation.mask()), self.query.height)
            & !self.initial_full_rows
            & !self.new_full_rows
            != 0
        {
            self.metrics.outside_full_row_prunes += 1;
            return false;
        }
        frame.candidates.push(operation);
        false
    }

    pub(crate) fn finish(self) -> Option<(Vec<FillSeed>, FillSeedMetrics)> {
        matches!(self.phase, Phase::Complete).then_some((self.seeds, self.metrics))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::piece::piece_kind::PieceKind;
    #[test]
    fn cooperative_fill_matches_every_recursive_seed_and_metric() {
        let mut query = SpinStructureQuery::new(
            PieceInventory::from_pieces([PieceKind::T, PieceKind::I, PieceKind::O]).unwrap(),
            crate::SpinStructureMode::TSpins,
        );
        query.height = 4;
        query.fill_top = 4;
        query.max_placements = Some(3);
        query.initial_board = StructureBoard::from_words([0x2d0, 0, 0, 0]);
        let catalog = Arc::new(
            LogicalOperationCatalog::compile(query.height, query.initial_board, query.inventory)
                .unwrap(),
        );
        for target in catalog.operations_for_piece(PieceKind::T).iter().copied() {
            let mut expected_metrics = FillSeedMetrics::default();
            let expected =
                enumerate_fill_seeds(&query, &catalog, target, Some(&mut expected_metrics));
            let mut cursor = FillCursor::new(query.clone(), catalog.clone(), target);
            while !cursor.step() {}
            let (actual, metrics) = cursor.finish().unwrap();
            assert_eq!(actual, expected);
            assert_eq!(metrics, expected_metrics);
        }
    }
}
