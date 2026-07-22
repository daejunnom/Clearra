use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};

use clearra_core_domain::{
    execution_cancellation::ExecutionCancellationToken, pruning::PruningEvidencePolicy,
};
use clearra_core_ffi::{
    CBuildUpProblem, CBuildUpProblemTemplate, CPackingCandidate, CPackingProblem,
    NativeBuildUpWorkspace, NativeCandidateReducer, NativeCoreError, NativeGeometryCatalog,
    NativeGeometrySolutionGraph, NativeGeometrySolutionTask, NativePackingCandidateConsumer,
    NativePackingCandidateContext, NativePackingCandidateSinkError, NativePruningLedger,
    C_BUILDUP_STATUS_OK,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_problem::SearchProblem;

use crate::{
    buildup::buildup_native_bridge::uses_standard_bag_automaton, packing::PackingRunnerError,
};

const PACKING_OK: i32 = 0;
const PACKING_CAPACITY_EXCEEDED: i32 = 6;
const TRUNCATION_CANDIDATE_BUDGET_EXCEEDED: u16 = 2;
const TRUNCATION_MEMORY_EXCEEDED: u16 = 10;

pub(crate) struct BuildableGeometryReduction {
    pub(crate) candidates: clearra_core_ffi::PackingCandidateBatch,
    pub(crate) generated_count: usize,
    pub(crate) buildable_count: usize,
    pub(crate) workspace_bytes: usize,
    pub(crate) reducer_bytes: usize,
    pub(crate) truncation: usize,
    pub(crate) worker_count: usize,
    pub(crate) pruning_ledger: Option<NativePruningLedger>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BuildableGeometryPathError {
    Stopped,
    Invalid,
}

#[derive(Clone)]
enum BuildabilitySourceSelection {
    StandardBagAutomaton,
    ConcretePatterns(Arc<[u32]>),
}

fn buildability_template_and_source(
    search_problem: &SearchProblem,
    source_pattern_bits: Option<&PatternBitSet>,
) -> Result<(Arc<CBuildUpProblemTemplate>, BuildabilitySourceSelection), PackingRunnerError> {
    if uses_standard_bag_automaton(search_problem) {
        return Ok((
            Arc::new(
                CBuildUpProblemTemplate::compile_for_standard_bag_automaton(search_problem)
                    .map_err(PackingRunnerError::Ffi)?,
            ),
            BuildabilitySourceSelection::StandardBagAutomaton,
        ));
    }

    let pattern_bits = source_pattern_bits.ok_or(PackingRunnerError::NoReachablePieceMultiset)?;
    let mut pattern_ids = Vec::with_capacity(pattern_bits.count_ones() as usize);
    for pattern_id in pattern_bits.covered_patterns() {
        pattern_ids.push(
            u32::try_from(pattern_id.index())
                .map_err(|_| PackingRunnerError::PatternGroupCapacityExceeded)?,
        );
    }
    if pattern_ids.is_empty() {
        return Err(PackingRunnerError::NoReachablePieceMultiset);
    }
    Ok((
        Arc::new(
            CBuildUpProblemTemplate::compile(search_problem).map_err(PackingRunnerError::Ffi)?,
        ),
        BuildabilitySourceSelection::ConcretePatterns(pattern_ids.into()),
    ))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn reduce_buildable_geometry_paths<Producer>(
    search_problem: &SearchProblem,
    source_pattern_bits: Option<&PatternBitSet>,
    problem: &CPackingProblem,
    catalog: &NativeGeometryCatalog,
    cancellation: &ExecutionCancellationToken,
    requested_worker_count: usize,
    base_resident_bytes: usize,
    producer: Producer,
) -> Result<BuildableGeometryReduction, PackingRunnerError>
where
    Producer: Fn(usize, usize, &mut BuildableGeometryPathConsumer<'_>) -> Result<(), PackingRunnerError>
        + Sync,
{
    let worker_count = requested_worker_count.max(1);
    let (template, source_selection) =
        buildability_template_and_source(search_problem, source_pattern_bits)?;
    let use_shared_reducer =
        problem.budget.max_results != 0 || problem.budget.has_max_memory_mib != 0;
    let shared_reducer = use_shared_reducer
        .then(|| {
            NativeCandidateReducer::new(problem)
                .map(Mutex::new)
                .map(Arc::new)
                .map_err(PackingRunnerError::CandidateBatch)
        })
        .transpose()?;
    let truncation = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let shared_workspace_bytes = use_shared_reducer.then(|| {
        Arc::new(
            (0..worker_count)
                .map(|_| AtomicUsize::new(0))
                .collect::<Vec<_>>(),
        )
    });

    let mut worker_reducers = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(worker_count);
        for worker_index in 0..worker_count {
            let template = Arc::clone(&template);
            let source_selection = source_selection.clone();
            let reducer = match &shared_reducer {
                Some(reducer) => CandidateReducerStorage::Shared(Arc::clone(reducer)),
                None => CandidateReducerStorage::Local(
                    NativeCandidateReducer::new(problem)
                        .map_err(PackingRunnerError::CandidateBatch)?,
                ),
            };
            let shared_workspace_bytes = shared_workspace_bytes.clone();
            let truncation = &truncation;
            let stop = &stop;
            let producer = &producer;
            handles.push(scope.spawn(move || {
                let mut scratch = match &source_selection {
                    BuildabilitySourceSelection::StandardBagAutomaton => template
                        .new_standard_bag_automaton_scratch()
                        .map_err(PackingRunnerError::Ffi)?,
                    BuildabilitySourceSelection::ConcretePatterns(_) => template.new_scratch(),
                };
                template.attach_geometry_catalog(&mut scratch, catalog);
                let workspace = NativeBuildUpWorkspace::new();
                let host_workspace_bytes = workspace.host_buffer_bytes();
                let initial_workspace_bytes = workspace.retained_bytes();
                if let Some(workspace_bytes) = &shared_workspace_bytes {
                    workspace_bytes[worker_index].store(initial_workspace_bytes, Ordering::Release);
                }
                let sink = BuildableCandidateConsumer {
                    reducer,
                    shared_workspace_bytes,
                    worker_index,
                    base_resident_bytes,
                    catalog_bytes: catalog.resident_bytes(),
                    host_workspace_bytes,
                    workspace_peak_bytes: initial_workspace_bytes,
                };
                let mut consumer = BuildableGeometryPathConsumer {
                    problem,
                    catalog,
                    cancellation,
                    template,
                    source_selection,
                    scratch,
                    workspace,
                    sink,
                    generated_count: 0,
                    buildable_count: 0,
                    truncation,
                    stop,
                    failure: None,
                    pruning_ledger: None,
                };
                producer(worker_index, worker_count, &mut consumer)?;
                if let Some(error) = consumer.failure.take() {
                    return Err(error);
                }
                consumer.finish();
                Ok::<WorkerGeometryReduction, PackingRunnerError>(consumer.into_worker_reduction())
            }));
        }
        let mut worker_reductions = Vec::with_capacity(worker_count);
        for handle in handles {
            worker_reductions.push(
                handle
                    .join()
                    .map_err(|_| PackingRunnerError::ParallelWorkerPanicked)??,
            );
        }
        Ok::<_, PackingRunnerError>(worker_reductions)
    })?;

    if cancellation.is_cancelled() {
        return Err(PackingRunnerError::ExecutionCancelled);
    }
    let generated_count = worker_reducers
        .iter()
        .map(|worker| worker.generated_count)
        .fold(0usize, usize::saturating_add);
    let buildable_count = worker_reducers
        .iter()
        .map(|worker| worker.buildable_count)
        .fold(0usize, usize::saturating_add);
    let retained_workspace_bytes = worker_reducers
        .iter()
        .map(|worker| worker.workspace_peak_bytes)
        .fold(0usize, usize::saturating_add);
    let pruning_ledger = merge_worker_pruning_ledgers(&mut worker_reducers)?;
    let (candidates, reducer_bytes) = match shared_reducer {
        Some(reducer) => {
            let reducer = Arc::try_unwrap(reducer)
                .map_err(|_| PackingRunnerError::ParallelWorkerPanicked)?
                .into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let reducer_bytes = reducer.resident_bytes();
            (reducer.into_candidates(), reducer_bytes)
        }
        None => merge_worker_reducers(
            problem,
            worker_reducers
                .into_iter()
                .filter_map(|worker| worker.reducer)
                .collect(),
        )?,
    };
    Ok(BuildableGeometryReduction {
        candidates,
        generated_count,
        buildable_count,
        workspace_bytes: retained_workspace_bytes,
        reducer_bytes,
        truncation: truncation.load(Ordering::Acquire),
        worker_count,
        pruning_ledger,
    })
}

pub(crate) struct BuildableGeometryPathConsumer<'a> {
    problem: &'a CPackingProblem,
    catalog: &'a NativeGeometryCatalog,
    cancellation: &'a ExecutionCancellationToken,
    template: Arc<CBuildUpProblemTemplate>,
    source_selection: BuildabilitySourceSelection,
    scratch: CBuildUpProblem,
    workspace: NativeBuildUpWorkspace,
    sink: BuildableCandidateConsumer,
    generated_count: usize,
    buildable_count: usize,
    truncation: &'a AtomicUsize,
    stop: &'a AtomicBool,
    failure: Option<PackingRunnerError>,
    pruning_ledger: Option<NativePruningLedger>,
}

impl BuildableGeometryPathConsumer<'_> {
    pub(crate) fn consume_row_ids(
        &mut self,
        row_ids: &[u32],
    ) -> Result<(), BuildableGeometryPathError> {
        if self.should_stop() {
            return Err(BuildableGeometryPathError::Stopped);
        }
        self.generated_count = self.generated_count.saturating_add(1);
        if matches!(
            self.source_selection,
            BuildabilitySourceSelection::StandardBagAutomaton
        ) {
            if self.consume_current_source(row_ids)? {
                self.buildable_count = self.buildable_count.saturating_add(1);
            }
            return Ok(());
        }
        let pattern_count = match &self.source_selection {
            BuildabilitySourceSelection::ConcretePatterns(pattern_ids) => pattern_ids.len(),
            BuildabilitySourceSelection::StandardBagAutomaton => 0,
        };
        for index in 0..pattern_count {
            let pattern_id = match &self.source_selection {
                BuildabilitySourceSelection::ConcretePatterns(pattern_ids) => pattern_ids[index],
                BuildabilitySourceSelection::StandardBagAutomaton => unreachable!(),
            };
            self.template
                .configure_piece_source_pattern(&mut self.scratch, pattern_id)
                .map_err(|error| {
                    self.failure = Some(PackingRunnerError::Ffi(error));
                    self.stop.store(true, Ordering::Release);
                    BuildableGeometryPathError::Invalid
                })?;
            if self.consume_current_source(row_ids)? {
                self.buildable_count = self.buildable_count.saturating_add(1);
                break;
            }
        }
        Ok(())
    }

    pub(crate) fn consume_standard_bag_graph_task(
        &mut self,
        graph: &NativeGeometrySolutionGraph,
        task: &NativeGeometrySolutionTask,
    ) -> Result<(), BuildableGeometryPathError> {
        if self.should_stop() {
            return Err(BuildableGeometryPathError::Stopped);
        }
        if !matches!(
            self.source_selection,
            BuildabilitySourceSelection::StandardBagAutomaton
        ) {
            return Err(BuildableGeometryPathError::Invalid);
        }
        let outcome = graph
            .stream_buildable_task(
                task,
                self.problem,
                &mut self.scratch,
                &mut self.workspace,
                self.cancellation,
                PruningEvidencePolicy::BestEffort,
                &mut self.sink,
            )
            .map_err(|error| {
                self.failure = Some(PackingRunnerError::Native(error));
                self.stop.store(true, Ordering::Release);
                BuildableGeometryPathError::Invalid
            })?;
        let status = outcome.status;
        let report = outcome.report;
        self.merge_pruning_ledger(outcome.pruning_ledger)?;
        self.generated_count = self
            .generated_count
            .saturating_add(usize::try_from(report.generated_count).unwrap_or(usize::MAX));
        self.buildable_count = self
            .buildable_count
            .saturating_add(usize::try_from(report.buildable_count).unwrap_or(usize::MAX));
        self.sink
            .observe_workspace_bytes(report.workspace_retained_bytes);
        if status == PACKING_OK {
            return Ok(());
        }
        if status == PACKING_CAPACITY_EXCEEDED {
            match report.truncation_reason {
                TRUNCATION_CANDIDATE_BUDGET_EXCEEDED => {
                    self.truncation.store(1, Ordering::Release);
                    self.stop.store(true, Ordering::Release);
                    return Err(BuildableGeometryPathError::Stopped);
                }
                TRUNCATION_MEMORY_EXCEEDED => {
                    self.truncation.store(2, Ordering::Release);
                    self.stop.store(true, Ordering::Release);
                    return Err(BuildableGeometryPathError::Stopped);
                }
                _ => {}
            }
        }
        let error = if report.buildup_status != C_BUILDUP_STATUS_OK {
            NativeCoreError::BuildUpStatus(report.buildup_status)
        } else {
            NativeCoreError::PackingStatus(status)
        };
        self.failure = Some(PackingRunnerError::Native(error));
        self.stop.store(true, Ordering::Release);
        Err(BuildableGeometryPathError::Invalid)
    }

    fn consume_current_source(
        &mut self,
        row_ids: &[u32],
    ) -> Result<bool, BuildableGeometryPathError> {
        let outcome = self
            .catalog
            .stream_buildable_rows(
                row_ids,
                self.problem,
                &mut self.scratch,
                &mut self.workspace,
                self.cancellation,
                PruningEvidencePolicy::BestEffort,
                &mut self.sink,
            )
            .map_err(|error| {
                self.failure = Some(PackingRunnerError::Native(error));
                self.stop.store(true, Ordering::Release);
                BuildableGeometryPathError::Invalid
            })?;
        let status = outcome.status;
        let report = outcome.report;
        self.merge_pruning_ledger(outcome.pruning_ledger)?;
        self.sink
            .observe_workspace_bytes(report.workspace_retained_bytes);
        if status == PACKING_OK {
            return Ok(report.candidate_buildable != 0);
        }
        if status == PACKING_CAPACITY_EXCEEDED {
            match report.truncation_reason {
                TRUNCATION_CANDIDATE_BUDGET_EXCEEDED => {
                    self.truncation.store(1, Ordering::Release);
                    self.stop.store(true, Ordering::Release);
                    return Err(BuildableGeometryPathError::Stopped);
                }
                TRUNCATION_MEMORY_EXCEEDED => {
                    self.truncation.store(2, Ordering::Release);
                    self.stop.store(true, Ordering::Release);
                    return Err(BuildableGeometryPathError::Stopped);
                }
                _ => {}
            }
        }
        let error = if report.buildup_status != C_BUILDUP_STATUS_OK {
            NativeCoreError::BuildUpStatus(report.buildup_status)
        } else {
            NativeCoreError::PackingStatus(status)
        };
        self.failure = Some(PackingRunnerError::Native(error));
        self.stop.store(true, Ordering::Release);
        Err(BuildableGeometryPathError::Invalid)
    }

    pub(crate) fn should_stop(&self) -> bool {
        self.stop.load(Ordering::Acquire) || self.cancellation.is_cancelled()
    }

    pub(crate) fn was_truncated(&self) -> bool {
        self.truncation.load(Ordering::Acquire) != 0
    }

    fn merge_pruning_ledger(
        &mut self,
        report: NativePruningLedger,
    ) -> Result<(), BuildableGeometryPathError> {
        if let Some(ledger) = &mut self.pruning_ledger {
            ledger.merge_partition_report(report).map_err(|error| {
                self.failure = Some(PackingRunnerError::Native(
                    NativeCoreError::InvalidPruningLedger(error),
                ));
                self.stop.store(true, Ordering::Release);
                BuildableGeometryPathError::Invalid
            })?;
        } else {
            self.pruning_ledger = Some(report);
        }
        Ok(())
    }

    fn finish(&mut self) {
        self.sink
            .observe_workspace_bytes(self.workspace.retained_bytes());
    }

    fn into_worker_reduction(self) -> WorkerGeometryReduction {
        let generated_count = self.generated_count;
        let buildable_count = self.buildable_count;
        let (reducer, workspace_peak_bytes) = self.sink.into_worker_result();
        WorkerGeometryReduction {
            reducer,
            generated_count,
            buildable_count,
            workspace_peak_bytes,
            pruning_ledger: self.pruning_ledger,
        }
    }
}

struct WorkerGeometryReduction {
    reducer: Option<NativeCandidateReducer>,
    generated_count: usize,
    buildable_count: usize,
    workspace_peak_bytes: usize,
    pruning_ledger: Option<NativePruningLedger>,
}

fn merge_worker_pruning_ledgers(
    workers: &mut [WorkerGeometryReduction],
) -> Result<Option<NativePruningLedger>, PackingRunnerError> {
    let mut merged: Option<NativePruningLedger> = None;
    for report in workers
        .iter_mut()
        .filter_map(|worker| worker.pruning_ledger.take())
    {
        if let Some(ledger) = &mut merged {
            ledger
                .merge_partition_report(report)
                .map_err(NativeCoreError::InvalidPruningLedger)
                .map_err(PackingRunnerError::Native)?;
        } else {
            merged = Some(report);
        }
    }
    Ok(merged)
}

enum CandidateReducerStorage {
    Local(NativeCandidateReducer),
    Shared(Arc<Mutex<NativeCandidateReducer>>),
}

struct BuildableCandidateConsumer {
    reducer: CandidateReducerStorage,
    shared_workspace_bytes: Option<Arc<Vec<AtomicUsize>>>,
    worker_index: usize,
    base_resident_bytes: usize,
    catalog_bytes: usize,
    host_workspace_bytes: usize,
    workspace_peak_bytes: usize,
}

impl BuildableCandidateConsumer {
    fn into_worker_result(self) -> (Option<NativeCandidateReducer>, usize) {
        let reducer = match self.reducer {
            CandidateReducerStorage::Local(reducer) => Some(reducer),
            CandidateReducerStorage::Shared(_) => None,
        };
        (reducer, self.workspace_peak_bytes)
    }

    fn observe_workspace_bytes(&mut self, bytes: usize) {
        self.workspace_peak_bytes = self.workspace_peak_bytes.max(bytes);
        if let Some(workspace_bytes) = &self.shared_workspace_bytes {
            workspace_bytes[self.worker_index].fetch_max(bytes, Ordering::AcqRel);
        }
    }

    fn reducer_resident_bytes(&self) -> usize {
        match &self.reducer {
            CandidateReducerStorage::Local(reducer) => reducer.resident_bytes(),
            CandidateReducerStorage::Shared(reducer) => reducer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .resident_bytes(),
        }
    }
}

impl NativePackingCandidateConsumer for BuildableCandidateConsumer {
    fn consume(
        &mut self,
        candidate: CPackingCandidate,
        context: NativePackingCandidateContext,
    ) -> Result<bool, NativePackingCandidateSinkError> {
        let current_workspace_bytes = context
            .engine_resident_bytes
            .saturating_sub(self.catalog_bytes)
            .saturating_add(self.host_workspace_bytes);
        self.observe_workspace_bytes(current_workspace_bytes);
        let other_workspace_bytes = match &self.shared_workspace_bytes {
            Some(workspace_bytes) => workspace_bytes
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != self.worker_index)
                .map(|(_, bytes)| bytes.load(Ordering::Acquire))
                .fold(0usize, usize::saturating_add),
            None => 0,
        };
        let engine_resident_bytes = self
            .base_resident_bytes
            .saturating_add(current_workspace_bytes)
            .saturating_add(other_workspace_bytes);
        let candidate_context = |accepted_candidate_count| NativePackingCandidateContext {
            accepted_candidate_count,
            engine_resident_bytes,
            max_candidate_rows: context.max_candidate_rows,
            max_total_bytes: context.max_total_bytes,
        };
        match &mut self.reducer {
            CandidateReducerStorage::Local(reducer) => {
                let accepted_candidate_count = reducer.accepted_candidate_count();
                reducer.consume(candidate, candidate_context(accepted_candidate_count))
            }
            CandidateReducerStorage::Shared(reducer) => {
                let mut reducer = reducer
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let accepted_candidate_count = reducer.accepted_candidate_count();
                reducer.consume(candidate, candidate_context(accepted_candidate_count))
            }
        }
    }

    fn resident_bytes(&self) -> usize {
        self.reducer_resident_bytes()
    }
}

fn merge_worker_reducers(
    problem: &CPackingProblem,
    reducers: Vec<NativeCandidateReducer>,
) -> Result<(clearra_core_ffi::PackingCandidateBatch, usize), PackingRunnerError> {
    let source_reducer_bytes = reducers
        .iter()
        .map(NativeCandidateReducer::resident_bytes)
        .fold(0usize, usize::saturating_add);
    let batches = reducers
        .into_iter()
        .map(NativeCandidateReducer::into_uncanonicalized_candidates)
        .collect();
    let board_height = if problem.board.search_height == 0 {
        problem.board.visible_height
    } else {
        problem.board.search_height
    };
    let mut merged = clearra_core_ffi::PackingCandidateBatch::merge_batches(
        batches,
        problem.board.width,
        board_height,
    )
    .map_err(PackingRunnerError::CandidateBatch)?;
    merged.canonicalize_identities();
    // During the ownership transfer, payload allocations are not duplicated.
    // Account conservatively for the one compact global index allocation that
    // may coexist with worker-local reducer hash tables.
    let reducer_bytes = source_reducer_bytes
        .saturating_add(merged.merge_index_resident_bytes())
        .max(merged.resident_bytes());
    Ok((merged, reducer_bytes))
}
