//! Cooperative source execution over the same canonical target universe as
//! native search. A completion can only be constructed by a finished worker.
use crate::{
    minimal::{self, MinimumCursor},
    operation_catalog::{CatalogCursor, LogicalOperationCatalog},
    structural_expand::ExpansionCursor,
    structural_fill::{FillCursor, FillSeed, FillSeedMetrics},
    structural_search, SpinStructureError, SpinStructureOutcome, SpinStructureQuery,
    SpinStructureReport, SpinStructureSearcher, SpinStructureStageMetrics, StructureOperation,
};
use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use std::{
    cmp::{Ordering, Reverse},
    collections::{BTreeMap, BinaryHeap},
    sync::Arc,
};

pub struct SpinStructureSourcePreparation {
    query: SpinStructureQuery,
    cursor: Option<CatalogCursor>,
    plan: Option<SpinStructureSourcePlan>,
}

#[derive(Clone)]
pub struct SpinStructureSourcePlan {
    query: SpinStructureQuery,
    catalog: Arc<LogicalOperationCatalog>,
}

impl SpinStructureSourcePreparation {
    pub fn new(query: SpinStructureQuery) -> Result<Self, SpinStructureError> {
        let query = SpinStructureSearcher::normalize_query(query)?;
        let cursor = CatalogCursor::new(query.height, query.initial_board, query.inventory)
            .map_err(|_| {
                SpinStructureError::ParallelExecutionFailed("spin_catalog_prepare_failed")
            })?;
        Ok(Self {
            query,
            cursor: Some(cursor),
            plan: None,
        })
    }
    pub fn advance(
        &mut self,
        budget: usize,
        control: &ExecutionControl,
    ) -> Result<bool, SpinStructureError> {
        for _ in 0..budget.max(1) {
            if control.is_cancelled() {
                return Err(SpinStructureError::Cancelled);
            }
            if self.plan.is_some() {
                return Ok(true);
            }
            if self
                .cursor
                .as_mut()
                .expect("preparation cursor")
                .step()
                .map_err(|_| {
                    SpinStructureError::ParallelExecutionFailed("spin_catalog_prepare_failed")
                })?
            {
                let catalog = self
                    .cursor
                    .take()
                    .unwrap()
                    .finish()
                    .expect("completed catalog");
                self.plan = Some(SpinStructureSourcePlan {
                    query: self.query.clone(),
                    catalog: Arc::new(catalog),
                });
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub fn finish(self) -> Result<SpinStructureSourcePlan, SpinStructureError> {
        self.plan.ok_or(SpinStructureError::ParallelExecutionFailed(
            "spin_catalog_not_complete",
        ))
    }
}

impl SpinStructureSourcePlan {
    pub fn query(&self) -> &SpinStructureQuery {
        &self.query
    }
    pub(crate) fn targets(&self) -> &[StructureOperation] {
        if self.query.mode.t_only() {
            self.catalog.operations_for_piece(PieceKind::T)
        } else {
            self.catalog.operations()
        }
    }
    pub fn task_count(&self) -> usize {
        self.targets().len()
    }
    pub fn worker(
        &self,
        ordinal: usize,
        control: &ExecutionControl,
    ) -> Result<SpinStructureSourceWorker, SpinStructureError> {
        let target = *self
            .targets()
            .get(ordinal)
            .ok_or(SpinStructureError::IncompatibleTaskReports)?;
        Ok(SpinStructureSourceWorker {
            plan: self.clone(),
            ordinal,
            target,
            phase: WorkerPhase::Fill(Box::new(FillCursor::new(
                self.query.clone(),
                self.catalog.clone(),
                target,
            ))),
            fill_metrics: FillSeedMetrics::default(),
            control: control.clone(),
        })
    }
    pub fn merger(&self, workers: usize) -> SpinStructureSourceMerger {
        SpinStructureSourceMerger {
            universe: SourceUniverse::Shared(self.clone()),
            reports: BTreeMap::new(),
            workers: workers.max(1).min(u16::MAX as usize) as u16,
        }
    }
}

enum WorkerPhase {
    Fill(Box<FillCursor>),
    Seeds(std::vec::IntoIter<FillSeed>, Vec<Vec<StructureOperation>>),
    Expand(Box<ExpansionCursor>),
    Outcomes(
        Box<SpinStructureReport>,
        std::vec::IntoIter<SpinStructureOutcome>,
    ),
    Minimum(Box<MinimumCursor>),
    Complete(Box<SpinStructureReport>),
    Transition,
}

pub struct SpinStructureSourceWorker {
    plan: SpinStructureSourcePlan,
    ordinal: usize,
    target: StructureOperation,
    phase: WorkerPhase,
    fill_metrics: FillSeedMetrics,
    control: ExecutionControl,
}

pub struct SpinStructureSourceCompletion {
    pub(crate) ordinal: usize,
    pub(crate) target: StructureOperation,
    pub(crate) report: SpinStructureReport,
}

impl SpinStructureSourceCompletion {
    pub fn ordinal(&self) -> usize {
        self.ordinal
    }
}

impl SpinStructureSourceWorker {
    pub fn advance(
        &mut self,
        budget: usize,
        control: &ExecutionControl,
    ) -> Result<bool, SpinStructureError> {
        for _ in 0..budget.max(1) {
            if control.is_cancelled() || self.control.is_cancelled() {
                return Err(SpinStructureError::Cancelled);
            }
            let phase = std::mem::replace(&mut self.phase, WorkerPhase::Transition);
            self.phase = match phase {
                WorkerPhase::Fill(mut cursor) => {
                    if cursor.step() {
                        let (seeds, metrics) = cursor.finish().unwrap();
                        self.fill_metrics = metrics;
                        WorkerPhase::Seeds(seeds.into_iter(), Vec::new())
                    } else {
                        WorkerPhase::Fill(cursor)
                    }
                }
                WorkerPhase::Seeds(mut seeds, mut operations) => {
                    if let Some(seed) = seeds.next() {
                        operations.push(seed.into_operations());
                        WorkerPhase::Seeds(seeds, operations)
                    } else {
                        WorkerPhase::Expand(Box::new(ExpansionCursor::new(
                            self.plan.query.clone(),
                            self.plan.catalog.clone(),
                            self.target,
                            operations,
                            &self.control,
                        )))
                    }
                }
                WorkerPhase::Expand(mut cursor) => {
                    if cursor.step() {
                        let (outcomes, metrics, verification) = cursor.finish().unwrap();
                        let report = SpinStructureReport {
                            query: Some(self.plan.query.clone()),
                            workers_used: 1,
                            complete: true,
                            stages: SpinStructureStageMetrics {
                                build_states: verification.build_states,
                                fill_checks: self.fill_metrics.search_nodes,
                                support_locks: metrics.support_candidates + metrics.roof_candidates,
                                corner_checks: metrics.blocker_candidates,
                                entry_states: verification.entry_states,
                                verification_checks: metrics.verification_candidates,
                                exact_state_deduplications: self.fill_metrics.duplicate_seeds
                                    + metrics.exact_duplicates,
                                exact_outcome_deduplications: 0,
                            },
                            layers: structural_search::layers(&self.plan.query, &metrics),
                            ..SpinStructureReport::default()
                        };
                        WorkerPhase::Outcomes(Box::new(report), outcomes.into_iter())
                    } else {
                        WorkerPhase::Expand(cursor)
                    }
                }
                WorkerPhase::Outcomes(mut report, mut outcomes) => {
                    if let Some(outcome) = outcomes.next() {
                        if outcome.is_mini() {
                            report.mini.push(outcome);
                        } else {
                            report.regular.push(outcome);
                        }
                        WorkerPhase::Outcomes(report, outcomes)
                    } else {
                        WorkerPhase::Minimum(Box::new(MinimumCursor::new(*report)))
                    }
                }
                WorkerPhase::Minimum(mut cursor) => {
                    if cursor.step() {
                        WorkerPhase::Complete(Box::new(cursor.finish().unwrap()))
                    } else {
                        WorkerPhase::Minimum(cursor)
                    }
                }
                WorkerPhase::Complete(report) => {
                    self.phase = WorkerPhase::Complete(report);
                    return Ok(true);
                }
                WorkerPhase::Transition => unreachable!("worker transition is private"),
            };
        }
        Ok(matches!(self.phase, WorkerPhase::Complete(_)))
    }
    pub fn finish(self) -> Result<SpinStructureSourceCompletion, SpinStructureError> {
        let WorkerPhase::Complete(report) = self.phase else {
            return Err(SpinStructureError::ParallelExecutionFailed(
                "spin_source_worker_not_complete",
            ));
        };
        Ok(SpinStructureSourceCompletion {
            ordinal: self.ordinal,
            target: self.target,
            report: *report,
        })
    }
}

pub struct SpinStructureSourceMerger {
    universe: SourceUniverse,
    reports: BTreeMap<usize, SpinStructureReport>,
    workers: u16,
}

enum SourceUniverse {
    Shared(SpinStructureSourcePlan),
    Transferred {
        query: SpinStructureQuery,
        targets: Vec<StructureOperation>,
    },
}

impl SourceUniverse {
    fn query(&self) -> &SpinStructureQuery {
        match self {
            Self::Shared(plan) => plan.query(),
            Self::Transferred { query, .. } => query,
        }
    }
    fn targets(&self) -> &[StructureOperation] {
        match self {
            Self::Shared(plan) => plan.targets(),
            Self::Transferred { targets, .. } => targets,
        }
    }
}

impl SpinStructureSourceMerger {
    pub(crate) fn from_manifest(
        query: SpinStructureQuery,
        targets: Vec<StructureOperation>,
        workers: usize,
    ) -> Self {
        Self {
            universe: SourceUniverse::Transferred { query, targets },
            reports: BTreeMap::new(),
            workers: workers.max(1).min(u16::MAX as usize) as u16,
        }
    }
    pub fn completed_tasks(&self) -> usize {
        self.reports.len()
    }
    pub(crate) fn target_count(&self) -> usize {
        self.universe.targets().len()
    }
    pub(crate) fn target(&self, ordinal: usize) -> Option<StructureOperation> {
        self.universe.targets().get(ordinal).copied()
    }
    pub fn absorb(
        &mut self,
        completion: SpinStructureSourceCompletion,
    ) -> Result<(), SpinStructureError> {
        if self.universe.targets().get(completion.ordinal) != Some(&completion.target)
            || completion.report.query.as_ref() != Some(self.universe.query())
            || !completion.report.complete
            || self.reports.contains_key(&completion.ordinal)
        {
            return Err(SpinStructureError::IncompatibleTaskReports);
        }
        self.reports.insert(completion.ordinal, completion.report);
        Ok(())
    }
    pub fn begin_finish(self) -> Result<SpinStructureSourceMergeCursor, SpinStructureError> {
        if self.reports.len() != self.universe.targets().len() {
            return Err(SpinStructureError::ParallelExecutionFailed(
                "spin_source_completions_missing",
            ));
        }
        let query = self.universe.query().clone();
        Ok(SpinStructureSourceMergeCursor {
            reports: self.reports,
            report: Some(SpinStructureReport {
                query: Some(query),
                workers_used: self.workers,
                complete: true,
                ..SpinStructureReport::default()
            }),
            runs: Vec::new(),
            regular: BinaryHeap::new(),
            mini: BinaryHeap::new(),
            source_minimum: None,
            done: None,
        })
    }
}

pub struct SpinStructureSourceMergeCursor {
    reports: BTreeMap<usize, SpinStructureReport>,
    report: Option<SpinStructureReport>,
    runs: Vec<(
        std::vec::IntoIter<SpinStructureOutcome>,
        std::vec::IntoIter<SpinStructureOutcome>,
    )>,
    regular: BinaryHeap<Reverse<MergeHead>>,
    mini: BinaryHeap<Reverse<MergeHead>>,
    source_minimum: Option<u8>,
    done: Option<SpinStructureReport>,
}

struct MergeHead {
    source: usize,
    outcome: SpinStructureOutcome,
}

impl Ord for MergeHead {
    fn cmp(&self, right: &Self) -> Ordering {
        self.outcome
            .logical_operations
            .cmp(&right.outcome.logical_operations)
            .then_with(|| self.outcome.logical_spin.cmp(&right.outcome.logical_spin))
            .then_with(|| {
                self.outcome
                    .logical_spin_cleared_rows
                    .cmp(&right.outcome.logical_spin_cleared_rows)
            })
            .then_with(|| self.source.cmp(&right.source))
    }
}
impl PartialOrd for MergeHead {
    fn partial_cmp(&self, right: &Self) -> Option<Ordering> {
        Some(self.cmp(right))
    }
}
impl PartialEq for MergeHead {
    fn eq(&self, right: &Self) -> bool {
        self.cmp(right) == Ordering::Equal
    }
}
impl Eq for MergeHead {}

impl SpinStructureSourceMergeCursor {
    pub fn advance(
        &mut self,
        budget: usize,
        control: &ExecutionControl,
    ) -> Result<bool, SpinStructureError> {
        for _ in 0..budget.max(1) {
            if control.is_cancelled() {
                return Err(SpinStructureError::Cancelled);
            }
            if self.done.is_some() {
                return Ok(true);
            }
            let report = self.report.as_mut().expect("merging report");
            if let Some((_, next)) = self.reports.pop_first() {
                minimal::merge_stage_metrics(&mut report.stages, next.stages);
                report.timings.absorb(next.timings);
                if let Some(minimum) = next.minimum_placements {
                    self.source_minimum = Some(
                        self.source_minimum
                            .map_or(minimum, |previous| previous.min(minimum)),
                    );
                }
                for mut right in next.layers {
                    // Each source is already minimal for its unique target.
                    // Only the global minimum-piece policy can remove another
                    // source's outcomes; count final acceptance while merging.
                    right.accepted_regular = 0;
                    right.accepted_mini = 0;
                    if let Some(left) = report
                        .layers
                        .iter_mut()
                        .find(|left| left.depth == right.depth)
                    {
                        left.input_states += right.input_states;
                        left.piece_choices += right.piece_choices;
                        left.reachable_locks += right.reachable_locks;
                        left.generated_states += right.generated_states;
                        left.exact_duplicates += right.exact_duplicates;
                        left.terminal_candidates += right.terminal_candidates;
                        left.accepted_regular += right.accepted_regular;
                        left.accepted_mini += right.accepted_mini;
                    } else {
                        report.layers.push(right);
                    }
                }
                let source = self.runs.len();
                let mut regular = next.regular.into_iter();
                let mut mini = next.mini.into_iter();
                if let Some(outcome) = regular.next() {
                    self.regular.push(Reverse(MergeHead { source, outcome }));
                }
                if let Some(outcome) = mini.next() {
                    self.mini.push(Reverse(MergeHead { source, outcome }));
                }
                self.runs.push((regular, mini));
                continue;
            }
            // Targets are a disjoint, privately completed source partition.
            // Subset comparisons never cross target identity, so repeating
            // the quadratic minimum pass after these workers is unnecessary.
            // Merge their canonical runs one outcome at a time instead.
            let is_mini = self.regular.is_empty();
            let heap = if is_mini {
                &mut self.mini
            } else {
                &mut self.regular
            };
            let Some(Reverse(MergeHead { source, outcome })) = heap.pop() else {
                report.layers.sort_by_key(|layer| layer.depth);
                self.done = self.report.take();
                return Ok(true);
            };
            let run = if is_mini {
                &mut self.runs[source].1
            } else {
                &mut self.runs[source].0
            };
            if let Some(outcome) = run.next() {
                heap.push(Reverse(MergeHead { source, outcome }));
            }
            let placements = outcome.build.len() as u8;
            if report
                .query
                .as_ref()
                .is_some_and(|query| query.minimality == crate::MinimalityPolicy::MinimumPieceCount)
                && Some(placements) != self.source_minimum
            {
                continue;
            }
            report.minimum_placements = Some(
                report
                    .minimum_placements
                    .map_or(placements, |previous| previous.min(placements)),
            );
            if let Some(layer) = report
                .layers
                .iter_mut()
                .find(|layer| layer.depth == placements)
            {
                if is_mini {
                    layer.accepted_mini += 1;
                } else {
                    layer.accepted_regular += 1;
                }
            }
            if is_mini {
                report.mini.push(outcome);
            } else {
                report.regular.push(outcome);
            }
        }
        Ok(self.done.is_some())
    }
    pub fn finish(self) -> Result<SpinStructureReport, SpinStructureError> {
        self.done.ok_or(SpinStructureError::ParallelExecutionFailed(
            "spin_source_merge_not_complete",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PieceInventory, SpinLineRequirement, SpinStructureMode, StructureBoard};
    #[test]
    fn source_workers_yield_and_merge_reversed_ordinals_to_the_native_report() {
        let mut query = SpinStructureQuery::new(
            PieceInventory::from_pieces([PieceKind::T, PieceKind::I, PieceKind::O]).unwrap(),
            SpinStructureMode::TSpins,
        );
        query.height = 4;
        query.fill_top = 4;
        query.max_placements = Some(3);
        query.line_requirement = SpinLineRequirement::Any;
        query.initial_board = [(4, 2), (6, 2), (4, 0)]
            .into_iter()
            .fold(StructureBoard::EMPTY, |board, (x, y)| {
                board.with_cell(x, y).unwrap()
            });
        let control = ExecutionControl::default();
        let mut preparation = SpinStructureSourcePreparation::new(query.clone()).unwrap();
        assert!(!preparation.advance(1, &control).unwrap());
        while !preparation.advance(97, &control).unwrap() {}
        let plan = preparation.finish().unwrap();
        let mut merger = plan.merger(8);
        for ordinal in (0..plan.task_count()).rev() {
            let mut worker = plan.worker(ordinal, &control).unwrap();
            assert!(!worker.advance(1, &control).unwrap());
            while !worker.advance(37, &control).unwrap() {}
            merger.absorb(worker.finish().unwrap()).unwrap();
        }
        let mut merge = merger.begin_finish().unwrap();
        while !merge.advance(11, &control).unwrap() {}
        let actual = merge.finish().unwrap();
        let mut expected = SpinStructureSearcher::run(query)
            .unwrap()
            .with_workers_used(8);
        expected.timings = Default::default();
        assert_eq!(actual, expected);
    }
}
