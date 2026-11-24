//! Exact order tiles for the existing distributed source worker protocol.
//! Candidate identity is always its ordinal in the complete canonical source.
use std::collections::BTreeMap;

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};

use crate::{
    order_coverage::{SpinStructureCoverageAnalysis, SpinStructureCoverageRow},
    structural_verify::{StructuralBuildVerifier, VerificationCursor},
    InventoryOrderSource, PieceInventory, SpinStructureOrderCoverageError, SpinStructureOutcome,
    SpinStructureQuery, SpinStructureReport, StructureOperation,
};

const TILE: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpinStructureParallelMode {
    Search,
    Cover {
        max_patterns: usize,
    },
    Guaranteed {
        final_piece: PieceKind,
        max_patterns: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OrderTask {
    pub mode: SpinStructureParallelMode,
    pub candidate: usize,
    pub first: usize,
    pub count: usize,
    pub target: StructureOperation,
    pub operations: Vec<StructureOperation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OrderCompletion {
    pub task: OrderTask,
    pub covered: u64,
}

fn inventory_for(
    query: &SpinStructureQuery,
    mode: SpinStructureParallelMode,
    target: StructureOperation,
    operations: &[StructureOperation],
) -> Result<InventoryOrderSource, SpinStructureOrderCoverageError> {
    let (inventory, limit) = match mode {
        SpinStructureParallelMode::Search => {
            return Err(SpinStructureOrderCoverageError::WorkerFailed(
                "spin_order_mode_missing",
            ));
        }
        SpinStructureParallelMode::Cover { max_patterns } => (query.inventory, max_patterns),
        SpinStructureParallelMode::Guaranteed {
            final_piece,
            max_patterns,
        } => {
            if target.piece() != final_piece {
                return Err(SpinStructureOrderCoverageError::WorkerFailed(
                    "spin_order_target_mismatch",
                ));
            }
            let inventory = PieceInventory::from_pieces(
                operations
                    .iter()
                    .filter(|operation| **operation != target)
                    .map(|operation| operation.piece()),
            )
            .map_err(|_| SpinStructureOrderCoverageError::PatternCountOverflow)?;
            (inventory, max_patterns)
        }
    };
    InventoryOrderSource::new(inventory, limit)
}

pub(crate) struct OrderWorker {
    query: SpinStructureQuery,
    task: OrderTask,
    source: InventoryOrderSource,
    verifier: StructuralBuildVerifier,
    verification: Option<VerificationCursor>,
    next: usize,
    covered: u64,
    done: bool,
}

impl OrderWorker {
    pub fn new(
        query: SpinStructureQuery,
        task: OrderTask,
        control: &ExecutionControl,
    ) -> Result<Self, SpinStructureOrderCoverageError> {
        if task.operations.is_empty()
            || task.operations.len() > usize::from(query.placement_limit())
            || !task.operations.contains(&task.target)
            || task.operations.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(SpinStructureOrderCoverageError::WorkerFailed(
                "spin_order_source_invalid",
            ));
        }
        let mut inventory = query.inventory;
        for operation in &task.operations {
            inventory = inventory.take(operation.piece()).ok_or(
                SpinStructureOrderCoverageError::WorkerFailed("spin_order_inventory_mismatch"),
            )?;
        }
        let source = inventory_for(&query, task.mode, task.target, &task.operations)?;
        if task.first % TILE != 0
            || task.first >= source.len()
            || task.count != TILE.min(source.len() - task.first)
        {
            return Err(SpinStructureOrderCoverageError::WorkerFailed(
                "spin_order_rank_window_invalid",
            ));
        }
        let next = task.first;
        let verifier = StructuralBuildVerifier::new_with_control(&query, control);
        Ok(Self {
            query,
            task,
            source,
            verifier,
            verification: None,
            next,
            covered: 0,
            done: false,
        })
    }

    pub fn advance(
        &mut self,
        budget: usize,
        control: &ExecutionControl,
    ) -> Result<bool, SpinStructureOrderCoverageError> {
        for _ in 0..budget.max(1) {
            if control.is_cancelled() {
                return Err(SpinStructureOrderCoverageError::Cancelled);
            }
            if self.done {
                return Ok(true);
            }
            if let Some(verification) = self.verification.as_mut() {
                if verification.step(&self.query, &mut self.verifier) {
                    if self
                        .verification
                        .take()
                        .unwrap()
                        .finish()
                        .unwrap()
                        .is_some()
                    {
                        self.covered |= 1 << (self.next - self.task.first);
                    }
                    self.next += 1;
                }
            } else if self.next == self.task.first + self.task.count {
                self.done = true;
            } else {
                let mut order = Vec::new();
                self.source.decode(self.next, &mut order);
                match self.task.mode {
                    SpinStructureParallelMode::Cover { .. } => {
                        order.truncate(self.task.operations.len())
                    }
                    SpinStructureParallelMode::Guaranteed { final_piece, .. } => {
                        order.push(final_piece)
                    }
                    SpinStructureParallelMode::Search => {
                        unreachable!("orders require an analysis mode")
                    }
                }
                self.verification = Some(VerificationCursor::new(
                    &self.query,
                    &self.task.operations,
                    self.task.target,
                    Some(order),
                ));
            }
        }
        Ok(self.done)
    }

    pub fn finish(self) -> Result<OrderCompletion, SpinStructureOrderCoverageError> {
        if !self.done {
            return Err(SpinStructureOrderCoverageError::WorkerFailed(
                "spin_order_tile_not_complete",
            ));
        }
        Ok(OrderCompletion {
            task: self.task,
            covered: self.covered,
        })
    }
}

pub(crate) enum OrderProduce {
    Pending,
    Task(OrderTask),
    Completed,
}

pub(crate) struct OrderCoordinator {
    query: SpinStructureQuery,
    mode: SpinStructureParallelMode,
    report: Option<SpinStructureReport>,
    source: Option<InventoryOrderSource>,
    sources: Vec<Option<InventoryOrderSource>>,
    rows: Vec<BTreeMap<usize, u64>>,
    retained: Vec<bool>,
    prepared: bool,
    candidate: usize,
    rank: usize,
    pending: BTreeMap<(usize, usize), OrderTask>,
    capacity: usize,
    union: BTreeMap<usize, u64>,
    covered_count: usize,
    task_count: usize,
    dispatched: usize,
    completed: usize,
    publishing: bool,
    publish_row: usize,
    publish_indices: Vec<u32>,
    coverage_rows: Vec<SpinStructureCoverageRow>,
    filtered: Option<(
        std::vec::IntoIter<SpinStructureOutcome>,
        std::vec::IntoIter<SpinStructureOutcome>,
    )>,
    result: Option<(SpinStructureReport, Option<SpinStructureCoverageAnalysis>)>,
}

impl OrderCoordinator {
    pub fn new(
        query: SpinStructureQuery,
        report: SpinStructureReport,
        mode: SpinStructureParallelMode,
        workers: usize,
    ) -> Result<Self, SpinStructureOrderCoverageError> {
        if !report.complete {
            return Err(SpinStructureOrderCoverageError::ReportIncomplete);
        }
        if report.query.as_ref() != Some(&query) {
            return Err(SpinStructureOrderCoverageError::QueryIdentityMismatch);
        }
        let source = match mode {
            SpinStructureParallelMode::Cover { max_patterns } => {
                Some(InventoryOrderSource::new(query.inventory, max_patterns)?)
            }
            SpinStructureParallelMode::Guaranteed {
                max_patterns: 0, ..
            } => return Err(SpinStructureOrderCoverageError::InvalidPatternLimit),
            SpinStructureParallelMode::Guaranteed { .. } => None,
            SpinStructureParallelMode::Search => {
                return Err(SpinStructureOrderCoverageError::WorkerFailed(
                    "spin_order_mode_missing",
                ))
            }
        };
        Ok(Self {
            query,
            mode,
            report: Some(report),
            source,
            sources: Vec::new(),
            rows: Vec::new(),
            retained: Vec::new(),
            prepared: false,
            candidate: 0,
            rank: 0,
            pending: BTreeMap::new(),
            capacity: workers.saturating_mul(4).max(1),
            union: BTreeMap::new(),
            covered_count: 0,
            task_count: 0,
            dispatched: 0,
            completed: 0,
            publishing: false,
            publish_row: 0,
            publish_indices: Vec::new(),
            coverage_rows: Vec::new(),
            filtered: None,
            result: None,
        })
    }

    fn outcome(&self, ordinal: usize) -> Option<&SpinStructureOutcome> {
        let report = self.report.as_ref()?;
        if ordinal < report.regular.len() {
            report.regular.get(ordinal)
        } else {
            report.mini.get(ordinal - report.regular.len())
        }
    }

    pub fn progress(&self) -> (usize, usize, usize) {
        (self.dispatched, self.completed, self.task_count)
    }
    pub fn waiting_for_results(&self) -> bool {
        self.prepared
            && !self.publishing
            && self.result.is_none()
            && !self.pending.is_empty()
            && (self.pending.len() >= self.capacity || self.candidate == self.sources.len())
    }

    pub fn produce(
        &mut self,
        budget: usize,
        control: &ExecutionControl,
    ) -> Result<OrderProduce, SpinStructureOrderCoverageError> {
        for _ in 0..budget.max(1) {
            if control.is_cancelled() {
                return Err(SpinStructureOrderCoverageError::Cancelled);
            }
            if self.result.is_some() {
                return Ok(OrderProduce::Completed);
            }
            if !self.prepared {
                let source = if let Some(outcome) = self.outcome(self.sources.len()) {
                    match self.mode {
                        SpinStructureParallelMode::Guaranteed { final_piece, .. }
                            if outcome.logical_spin().piece() != final_piece =>
                        {
                            None
                        }
                        _ => Some(inventory_for(
                            &self.query,
                            self.mode,
                            outcome.logical_spin(),
                            outcome.logical_operations(),
                        )?),
                    }
                } else {
                    self.prepared = true;
                    continue;
                };
                if let Some(source) = source {
                    self.task_count = self
                        .task_count
                        .checked_add(source.len().div_ceil(TILE))
                        .ok_or(SpinStructureOrderCoverageError::PatternCountOverflow)?;
                }
                self.retained.push(source.is_some());
                self.sources.push(source);
                self.rows.push(BTreeMap::new());
                continue;
            }
            if self.publishing {
                self.publish_step();
                continue;
            }
            if self.pending.len() >= self.capacity {
                return Ok(OrderProduce::Pending);
            }
            if self.candidate == self.sources.len() {
                if !self.pending.is_empty() {
                    return Ok(OrderProduce::Pending);
                }
                self.publishing = true;
                if matches!(self.mode, SpinStructureParallelMode::Guaranteed { .. }) {
                    let report = self.report.as_mut().unwrap();
                    report.minimum_placements = None;
                    self.filtered = Some((
                        std::mem::take(&mut report.regular).into_iter(),
                        std::mem::take(&mut report.mini).into_iter(),
                    ));
                }
                continue;
            }
            let Some(source) = self.sources[self.candidate] else {
                self.candidate += 1;
                self.rank = 0;
                continue;
            };
            if !self.retained[self.candidate] || self.rank == source.len() {
                self.candidate += 1;
                self.rank = 0;
                continue;
            }
            let outcome = self.outcome(self.candidate).unwrap();
            let task = OrderTask {
                mode: self.mode,
                candidate: self.candidate,
                first: self.rank,
                count: TILE.min(source.len() - self.rank),
                target: outcome.logical_spin(),
                operations: outcome.logical_operations().to_vec(),
            };
            self.rank += task.count;
            self.dispatched += 1;
            self.pending
                .insert((task.candidate, task.first), task.clone());
            return Ok(OrderProduce::Task(task));
        }
        Ok(OrderProduce::Pending)
    }

    pub fn absorb(
        &mut self,
        completion: OrderCompletion,
    ) -> Result<(), SpinStructureOrderCoverageError> {
        let key = (completion.task.candidate, completion.task.first);
        if self.pending.get(&key) != Some(&completion.task)
            || (completion.task.count < 64 && completion.covered >> completion.task.count != 0)
        {
            return Err(SpinStructureOrderCoverageError::WorkerFailed(
                "spin_order_completion_source_mismatch",
            ));
        }
        self.pending.remove(&key);
        self.completed += 1;
        match self.mode {
            SpinStructureParallelMode::Cover { .. } => {
                if completion.covered != 0 {
                    self.rows[key.0].insert(key.1, completion.covered);
                    let previous = self.union.entry(key.1).or_default();
                    self.covered_count += (completion.covered & !*previous).count_ones() as usize;
                    *previous |= completion.covered;
                }
            }
            SpinStructureParallelMode::Guaranteed { .. } => {
                let all = if completion.task.count == 64 {
                    u64::MAX
                } else {
                    (1 << completion.task.count) - 1
                };
                if completion.covered != all {
                    self.retained[key.0] = false;
                }
            }
            SpinStructureParallelMode::Search => unreachable!(),
        }
        Ok(())
    }

    fn publish_step(&mut self) {
        match self.mode {
            SpinStructureParallelMode::Cover { .. } => {
                if self.publish_row == self.rows.len() {
                    self.result = Some((
                        self.report.take().unwrap(),
                        Some(SpinStructureCoverageAnalysis {
                            pattern_count: self.source.unwrap().len(),
                            covered_pattern_count: self.covered_count,
                            rows: std::mem::take(&mut self.coverage_rows),
                        }),
                    ));
                } else if let Some((first, mut bits)) = self.rows[self.publish_row].pop_first() {
                    while bits != 0 {
                        let bit = bits.trailing_zeros();
                        self.publish_indices.push((first + bit as usize) as u32);
                        bits &= bits - 1;
                    }
                } else {
                    self.coverage_rows.push(SpinStructureCoverageRow {
                        covered_pattern_indices: std::mem::take(&mut self.publish_indices),
                    });
                    self.publish_row += 1;
                }
            }
            SpinStructureParallelMode::Guaranteed { .. } => {
                let (regular, mini) = self.filtered.as_mut().unwrap();
                if let Some(outcome) = regular.next().or_else(|| mini.next()) {
                    if self.retained[self.publish_row] {
                        let report = self.report.as_mut().unwrap();
                        let length = outcome.placement_count() as u8;
                        report.minimum_placements = Some(
                            report
                                .minimum_placements
                                .map_or(length, |minimum| minimum.min(length)),
                        );
                        if outcome.is_mini() {
                            report.mini.push(outcome);
                        } else {
                            report.regular.push(outcome);
                        }
                    }
                    self.publish_row += 1;
                } else {
                    self.result = Some((self.report.take().unwrap(), None));
                }
            }
            SpinStructureParallelMode::Search => unreachable!(),
        }
    }

    pub fn finish(
        self,
    ) -> Result<
        (SpinStructureReport, Option<SpinStructureCoverageAnalysis>),
        SpinStructureOrderCoverageError,
    > {
        self.result
            .ok_or(SpinStructureOrderCoverageError::WorkerFailed(
                "spin_order_analysis_not_complete",
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        analyze_spin_structure_coverage, guaranteed_spin_structure_family, SpinLineRequirement,
        SpinStructureMode, SpinStructureSearcher, StructureBoard,
    };

    fn query() -> SpinStructureQuery {
        let mut query = SpinStructureQuery::new(
            PieceInventory::from_pieces(PieceKind::STANDARD_TETROMINOES).unwrap(),
            SpinStructureMode::TSpins,
        );
        query.height = 4;
        query.fill_top = 4;
        query.max_placements = Some(1);
        query.line_requirement = SpinLineRequirement::Any;
        query.initial_board = [(4, 2), (6, 2), (4, 0)]
            .into_iter()
            .fold(StructureBoard::EMPTY, |board, (x, y)| {
                board.with_cell(x, y).unwrap()
            });
        SpinStructureSearcher::normalize_query(query).unwrap()
    }

    fn run(
        query: SpinStructureQuery,
        report: SpinStructureReport,
        mode: SpinStructureParallelMode,
        workers: usize,
    ) -> (SpinStructureReport, Option<SpinStructureCoverageAnalysis>) {
        let control = ExecutionControl::default();
        let mut coordinator = OrderCoordinator::new(query.clone(), report, mode, workers).unwrap();
        let mut pending = Vec::new();
        loop {
            match coordinator.produce(7, &control).unwrap() {
                OrderProduce::Task(task) => {
                    let mut worker = OrderWorker::new(query.clone(), task, &control).unwrap();
                    assert!(!worker.advance(1, &control).unwrap());
                    while !worker.advance(13, &control).unwrap() {}
                    pending.push(worker.finish().unwrap());
                }
                OrderProduce::Pending => {}
                OrderProduce::Completed => break,
            }
            if pending.len() >= workers || coordinator.waiting_for_results() {
                while let Some(completion) = pending.pop() {
                    coordinator.absorb(completion).unwrap();
                }
            }
        }
        coordinator.finish().unwrap()
    }

    #[test]
    fn bounded_order_tiles_preserve_all_original_pattern_ids_and_candidate_rows() {
        let query = query();
        let report = SpinStructureSearcher::run(query.clone()).unwrap();
        assert!(!report.regular.is_empty());
        let expected = analyze_spin_structure_coverage(&query, &report, 5040).unwrap();
        assert_eq!(expected.pattern_count(), 5040);
        for workers in [1, 2, 4, 8] {
            let (actual_report, actual) = run(
                query.clone(),
                report.clone(),
                SpinStructureParallelMode::Cover { max_patterns: 5040 },
                workers,
            );
            assert_eq!(actual_report, report);
            assert_eq!(actual.unwrap(), expected);
        }
    }

    #[test]
    fn bounded_guaranteed_orders_preserve_reserved_terminal_piece_and_canonical_family() {
        let mut query = query();
        query.inventory =
            PieceInventory::from_pieces([PieceKind::T, PieceKind::I, PieceKind::O]).unwrap();
        query.max_placements = Some(3);
        let report = SpinStructureSearcher::run(query.clone()).unwrap();
        let expected = guaranteed_spin_structure_family(&query, &report, PieceKind::T, 6).unwrap();
        for workers in [1, 2, 4, 8] {
            let (actual, coverage) = run(
                query.clone(),
                report.clone(),
                SpinStructureParallelMode::Guaranteed {
                    final_piece: PieceKind::T,
                    max_patterns: 6,
                },
                workers,
            );
            assert!(coverage.is_none());
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn order_completion_rejects_foreign_source_ordinal_duplicates_and_out_of_range_bits() {
        let query = query();
        let report = SpinStructureSearcher::run(query.clone()).unwrap();
        let control = ExecutionControl::default();
        let mut coordinator = OrderCoordinator::new(
            query.clone(),
            report,
            SpinStructureParallelMode::Cover { max_patterns: 5040 },
            2,
        )
        .unwrap();
        let task = loop {
            if let OrderProduce::Task(task) = coordinator.produce(3, &control).unwrap() {
                break task;
            }
        };
        let mut worker = OrderWorker::new(query, task, &control).unwrap();
        while !worker.advance(17, &control).unwrap() {}
        let completion = worker.finish().unwrap();
        let mut foreign = completion.clone();
        foreign.task.candidate += 1;
        assert!(coordinator.absorb(foreign).is_err());
        coordinator.absorb(completion.clone()).unwrap();
        assert!(coordinator.absorb(completion).is_err());

        let mut query = self::query();
        query.inventory =
            PieceInventory::from_pieces([PieceKind::I, PieceKind::O, PieceKind::T]).unwrap();
        let report = SpinStructureSearcher::run(query.clone()).unwrap();
        let mut coordinator = OrderCoordinator::new(
            query.clone(),
            report,
            SpinStructureParallelMode::Cover { max_patterns: 6 },
            1,
        )
        .unwrap();
        let task = loop {
            if let OrderProduce::Task(task) = coordinator.produce(3, &control).unwrap() {
                break task;
            }
        };
        assert_eq!(task.count, 6);
        assert!(coordinator
            .absorb(OrderCompletion {
                task,
                covered: 1_u64 << 63
            })
            .is_err());
    }
}
