use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};

use clearra_core_domain::{
    execution_cancellation::ExecutionCancellationToken,
    pruning::PruningEvidencePolicy,
    resource::{ExecutionAvailability, ExecutionAvailabilityReason, ResourceReport},
};
use clearra_core_ffi::{
    CBuildUpProblem, CBuildUpProblemTemplate, CPackingCandidate, CPackingProblem,
    NativeBuildUpWorkspace, NativeCandidateReducer, NativeCoreError, NativeGeometryCatalog,
    NativeGeometrySolutionGraph, NativeGeometrySolutionTask, NativePackingCandidateConsumer,
    NativePackingCandidateContext, NativePackingCandidateSinkError, NativePruningLedger,
    PackingCandidateBatch, C_BUILDUP_STATUS_OK,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_problem::SearchProblem;

use crate::{
    buildup::buildup_native_bridge::uses_standard_bag_automaton, packing::PackingRunnerError,
    resource::ExecutionMemoryBound,
};

const PACKING_OK: i32 = 0;
const PACKING_CAPACITY_EXCEEDED: i32 = 6;
const TRUNCATION_CANDIDATE_BUDGET_EXCEEDED: u16 = 2;
const TRUNCATION_MEMORY_EXCEEDED: u16 = 10;
const BUILDABLE_WORKER_STACK_BYTES: usize = 2 * 1024 * 1024;

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

impl BuildabilitySourceSelection {
    fn checked_retained_bytes(&self) -> Option<u128> {
        match self {
            Self::StandardBagAutomaton => Some(0),
            Self::ConcretePatterns(pattern_ids) => {
                let arc_header_bytes = (2 * core::mem::size_of::<usize>()) as u128;
                arc_header_bytes.checked_add(
                    (pattern_ids.len() as u128).checked_mul(core::mem::size_of::<u32>() as u128)?,
                )
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BuildableGeometryAllocationPlan {
    template_bytes: u128,
    source_ids_bytes: u128,
    worker_owner_bytes: u128,
    worker_stack_bytes: u128,
}

impl BuildableGeometryAllocationPlan {
    fn checked(
        problem: &SearchProblem,
        standard_bag_automaton: bool,
        source_pattern_count: usize,
        worker_count: usize,
        shared_reducer: bool,
    ) -> Option<Self> {
        let template_bytes = CBuildUpProblemTemplate::checked_compile_retained_bytes(
            problem,
            standard_bag_automaton,
        )?;
        let source_ids_bytes = if standard_bag_automaton {
            0
        } else {
            ((2 * core::mem::size_of::<usize>()) as u128).checked_add(
                (source_pattern_count as u128).checked_mul(core::mem::size_of::<u32>() as u128)?,
            )?
        };
        let per_worker_owner_bytes = (core::mem::size_of::<NativeBuildUpWorkspace>() as u128)
            .checked_add(core::mem::size_of::<WorkerGeometryReduction>() as u128)?
            .checked_add(core::mem::size_of::<NativeCandidateReducer>() as u128)?
            .checked_add(core::mem::size_of::<PackingCandidateBatch>() as u128)?
            .checked_add(core::mem::size_of::<AtomicUsize>() as u128)?
            .checked_add(core::mem::size_of::<
                std::thread::ScopedJoinHandle<
                    'static,
                    Result<WorkerGeometryReduction, PackingRunnerError>,
                >,
            >() as u128)?;
        let shared_owner_bytes = if shared_reducer {
            (core::mem::size_of::<Mutex<NativeCandidateReducer>>() as u128)
                .checked_add(core::mem::size_of::<Vec<AtomicUsize>>() as u128)?
                .checked_add((4 * core::mem::size_of::<usize>()) as u128)?
        } else {
            0
        };
        let workers = worker_count as u128;
        Some(Self {
            template_bytes,
            source_ids_bytes,
            worker_owner_bytes: per_worker_owner_bytes
                .checked_mul(workers)?
                .checked_add(shared_owner_bytes)?,
            worker_stack_bytes: (BUILDABLE_WORKER_STACK_BYTES as u128).checked_mul(workers)?,
        })
    }

    fn checked_retained_before_workspace_bytes(self) -> Option<u128> {
        self.template_bytes
            .checked_add(self.source_ids_bytes)?
            .checked_add(self.worker_owner_bytes)?
            .checked_add(self.worker_stack_bytes)
    }
}

fn buildability_template_and_source(
    search_problem: &SearchProblem,
    source_pattern_bits: Option<&PatternBitSet>,
    max_memory_bytes: u128,
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
    let pattern_count = usize::try_from(pattern_bits.count_ones())
        .map_err(|_| buildable_projection_overflow(search_problem))?;
    let mut pattern_ids = Vec::new();
    pattern_ids
        .try_reserve_exact(pattern_count)
        .map_err(|_| buildable_memory_exhausted(search_problem, max_memory_bytes))?;
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
// Local admission composition preserves the domain `ResourceReport` until the runner boundary.
#[allow(clippy::result_large_err)]
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
    let standard_bag_automaton = uses_standard_bag_automaton(search_problem);
    let source_pattern_count = if standard_bag_automaton {
        0
    } else {
        usize::try_from(
            source_pattern_bits
                .ok_or(PackingRunnerError::NoReachablePieceMultiset)?
                .count_ones(),
        )
        .map_err(|_| buildable_projection_overflow(search_problem))?
    };
    let use_shared_reducer =
        problem.budget.max_results != 0 || problem.budget.has_max_memory_mib != 0;
    let max_memory_bytes = configured_memory_limit_bytes(problem);
    let memory_bound = ExecutionMemoryBound::unbounded_for_problem(search_problem)
        .and_then(|bound| bound.with_cap(max_memory_bytes))
        .map_err(buildable_resource_error)?;
    let allocation_plan = BuildableGeometryAllocationPlan::checked(
        search_problem,
        standard_bag_automaton,
        source_pattern_count,
        worker_count,
        use_shared_reducer,
    )
    .ok_or_else(|| buildable_projection_overflow(search_problem))?;
    let planned_before_workspace = allocation_plan
        .checked_retained_before_workspace_bytes()
        .ok_or_else(|| buildable_projection_overflow(search_problem))?;
    memory_bound
        .ensure(base_resident_bytes as u128, planned_before_workspace)
        .map_err(buildable_resource_error)?;

    let (template, source_selection) =
        buildability_template_and_source(search_problem, source_pattern_bits, max_memory_bytes)?;
    let actual_template_bytes = template
        .checked_retained_bytes()
        .ok_or_else(|| buildable_projection_overflow(search_problem))?;
    let actual_source_bytes = source_selection
        .checked_retained_bytes()
        .ok_or_else(|| buildable_projection_overflow(search_problem))?;
    debug_assert!(allocation_plan.template_bytes >= actual_template_bytes);
    debug_assert!(allocation_plan.source_ids_bytes >= actual_source_bytes);
    let retained_before_workspace = actual_template_bytes
        .checked_add(actual_source_bytes)
        .and_then(|bytes| bytes.checked_add(allocation_plan.worker_owner_bytes))
        .and_then(|bytes| bytes.checked_add(allocation_plan.worker_stack_bytes))
        .ok_or_else(|| buildable_projection_overflow(search_problem))?;
    memory_bound
        .ensure(base_resident_bytes as u128, retained_before_workspace)
        .map_err(buildable_resource_error)?;

    let mut workspaces = Vec::new();
    workspaces
        .try_reserve_exact(worker_count)
        .map_err(|_| buildable_memory_exhausted(search_problem, max_memory_bytes))?;
    let mut workspace_retained_total = 0u128;
    for _ in 0..worker_count {
        let already_retained = (base_resident_bytes as u128)
            .checked_add(retained_before_workspace)
            .and_then(|bytes| bytes.checked_add(workspace_retained_total))
            .ok_or_else(|| buildable_projection_overflow(search_problem))?;
        let workspace =
            NativeBuildUpWorkspace::try_new_with_memory_limit(already_retained, max_memory_bytes)
                .map_err(|error| enrich_native_memory_error(search_problem, error))?;
        let retained = workspace
            .checked_retained_bytes()
            .ok_or_else(|| buildable_projection_overflow(search_problem))?;
        workspace_retained_total = workspace_retained_total
            .checked_add(retained)
            .ok_or_else(|| buildable_projection_overflow(search_problem))?;
        workspaces.push(workspace);
    }
    memory_bound
        .ensure(
            (base_resident_bytes as u128)
                .checked_add(retained_before_workspace)
                .ok_or_else(|| buildable_projection_overflow(search_problem))?,
            workspace_retained_total,
        )
        .map_err(buildable_resource_error)?;
    let execution_base_resident_bytes = usize::try_from(
        (base_resident_bytes as u128)
            .checked_add(retained_before_workspace)
            .ok_or_else(|| buildable_projection_overflow(search_problem))?,
    )
    .map_err(|_| buildable_projection_overflow(search_problem))?;

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
            workspaces
                .iter()
                .map(|workspace| {
                    AtomicUsize::new(
                        usize::try_from(
                            workspace
                                .checked_retained_bytes()
                                .expect("workspace bytes were checked above"),
                        )
                        .expect("native workspace retained bytes fit usize"),
                    )
                })
                .collect::<Vec<_>>(),
        )
    });

    let mut worker_reducers = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        handles
            .try_reserve_exact(worker_count)
            .map_err(|_| buildable_memory_exhausted(search_problem, max_memory_bytes))?;
        for (worker_index, workspace) in workspaces.into_iter().enumerate() {
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
            let handle = std::thread::Builder::new()
                .stack_size(BUILDABLE_WORKER_STACK_BYTES)
                .spawn_scoped(scope, move || {
                    let mut scratch = match &source_selection {
                        BuildabilitySourceSelection::StandardBagAutomaton => template
                            .new_standard_bag_automaton_scratch()
                            .map_err(PackingRunnerError::Ffi)?,
                        BuildabilitySourceSelection::ConcretePatterns(_) => template.new_scratch(),
                    };
                    template.attach_geometry_catalog(&mut scratch, catalog);
                    let host_workspace_bytes = workspace.host_buffer_bytes();
                    let initial_workspace_bytes = workspace.retained_bytes();
                    if let Some(workspace_bytes) = &shared_workspace_bytes {
                        workspace_bytes[worker_index]
                            .store(initial_workspace_bytes, Ordering::Release);
                    }
                    let sink = BuildableCandidateConsumer {
                        reducer,
                        shared_workspace_bytes,
                        worker_index,
                        base_resident_bytes: execution_base_resident_bytes,
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
                    Ok::<WorkerGeometryReduction, PackingRunnerError>(
                        consumer.into_worker_reduction(),
                    )
                })
                .map_err(|_| PackingRunnerError::ParallelWorkerPanicked)?;
            handles.push(handle);
        }
        let mut worker_reductions = Vec::new();
        worker_reductions
            .try_reserve_exact(worker_count)
            .map_err(|_| buildable_memory_exhausted(search_problem, max_memory_bytes))?;
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
        .try_fold(0usize, usize::checked_add)
        .ok_or_else(|| buildable_projection_overflow(search_problem))?;
    let retained_without_reducer = (base_resident_bytes as u128)
        .checked_add(retained_before_workspace)
        .and_then(|bytes| bytes.checked_add(retained_workspace_bytes as u128))
        .ok_or_else(|| buildable_projection_overflow(search_problem))?;
    let pruning_ledger = merge_worker_pruning_ledgers(&mut worker_reducers)?;
    let (candidates, reducer_bytes) = match shared_reducer {
        Some(reducer) => {
            let observed_reducer_bytes = reducer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .resident_bytes() as u128;
            memory_bound
                .ensure(retained_without_reducer, observed_reducer_bytes)
                .map_err(buildable_resource_error)?;
            let reducer = Arc::try_unwrap(reducer)
                .map_err(|_| PackingRunnerError::ParallelWorkerPanicked)?
                .into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let reducer_bytes = reducer.resident_bytes();
            (reducer.into_candidates(), reducer_bytes)
        }
        None => merge_worker_reducers(
            search_problem,
            problem,
            worker_reducers
                .into_iter()
                .filter_map(|worker| worker.reducer)
                .collect(),
            memory_bound,
            retained_without_reducer,
        )?,
    };
    Ok(BuildableGeometryReduction {
        candidates,
        generated_count,
        buildable_count,
        workspace_bytes: usize::try_from(
            retained_before_workspace
                .checked_add(retained_workspace_bytes as u128)
                .ok_or_else(|| buildable_projection_overflow(search_problem))?,
        )
        .map_err(|_| buildable_projection_overflow(search_problem))?,
        reducer_bytes,
        truncation: truncation.load(Ordering::Acquire),
        worker_count,
        pruning_ledger,
    })
}

fn configured_memory_limit_bytes(problem: &CPackingProblem) -> u128 {
    if problem.budget.has_max_memory_mib == 0 {
        u128::MAX
    } else {
        u128::from(problem.budget.max_memory_mib) * 1024 * 1024
    }
}

fn buildable_resource_error(resource_report: ResourceReport) -> PackingRunnerError {
    PackingRunnerError::Native(NativeCoreError::packing_incomplete(
        PACKING_CAPACITY_EXCEEDED,
        resource_report,
    ))
}

fn buildable_resource_report(
    search_problem: &SearchProblem,
    reason: ExecutionAvailabilityReason,
    required_memory_bytes: u128,
) -> ResourceReport {
    let Some(universe) = search_problem.piece_source().materialized_universe() else {
        return ResourceReport::admission_failure(ExecutionAvailability::unavailable(
            ExecutionAvailabilityReason::CapabilityUnavailable,
        ));
    };
    let descriptor_pattern_count = universe.total_possible_pattern_count();
    let dense_pattern_count = universe.pattern_count() as u128;
    let required_dense_bytes = dense_pattern_count
        .checked_add(63)
        .and_then(|count| count.checked_div(64))
        .and_then(|words| words.checked_mul(core::mem::size_of::<u64>() as u128))
        .unwrap_or(u128::MAX);
    let availability = match reason {
        ExecutionAvailabilityReason::MemoryBudgetExceeded => {
            ExecutionAvailability::exhausted(reason)
        }
        _ => ExecutionAvailability::unavailable(reason),
    }
    .with_pattern_evidence(
        descriptor_pattern_count,
        dense_pattern_count,
        required_dense_bytes,
    )
    .with_required_memory_bytes(required_memory_bytes);
    ResourceReport::admission_failure(availability)
}

fn buildable_projection_overflow(search_problem: &SearchProblem) -> PackingRunnerError {
    buildable_resource_error(buildable_resource_report(
        search_problem,
        ExecutionAvailabilityReason::PatternCountAddressSpaceExceeded,
        u128::MAX,
    ))
}

fn buildable_memory_exhausted(
    search_problem: &SearchProblem,
    max_memory_bytes: u128,
) -> PackingRunnerError {
    buildable_resource_error(buildable_resource_report(
        search_problem,
        ExecutionAvailabilityReason::MemoryBudgetExceeded,
        max_memory_bytes.saturating_add(1),
    ))
}

fn enrich_native_memory_error(
    search_problem: &SearchProblem,
    error: NativeCoreError,
) -> PackingRunnerError {
    if let NativeCoreError::PackingIncomplete {
        status,
        resource_report,
    } = error
    {
        let availability = resource_report.execution_availability();
        if availability.reason() == Some(ExecutionAvailabilityReason::MemoryBudgetExceeded)
            && availability.descriptor_pattern_count().is_none()
        {
            return PackingRunnerError::Native(NativeCoreError::packing_incomplete(
                status,
                buildable_resource_report(
                    search_problem,
                    ExecutionAvailabilityReason::MemoryBudgetExceeded,
                    availability.required_memory_bytes().unwrap_or(u128::MAX),
                ),
            ));
        }
        return PackingRunnerError::Native(NativeCoreError::PackingIncomplete {
            status,
            resource_report,
        });
    }
    PackingRunnerError::Native(error)
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
            .checked_sub(self.catalog_bytes)
            .and_then(|bytes| bytes.checked_add(self.host_workspace_bytes))
            .ok_or(NativePackingCandidateSinkError::MemoryExceeded)?;
        self.observe_workspace_bytes(current_workspace_bytes);
        let other_workspace_bytes = match &self.shared_workspace_bytes {
            Some(workspace_bytes) => workspace_bytes
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != self.worker_index)
                .map(|(_, bytes)| bytes.load(Ordering::Acquire))
                .try_fold(0usize, usize::checked_add)
                .ok_or(NativePackingCandidateSinkError::MemoryExceeded)?,
            None => 0,
        };
        let engine_resident_bytes = self
            .base_resident_bytes
            .checked_add(current_workspace_bytes)
            .and_then(|bytes| bytes.checked_add(other_workspace_bytes))
            .ok_or(NativePackingCandidateSinkError::MemoryExceeded)?;
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
    search_problem: &SearchProblem,
    problem: &CPackingProblem,
    reducers: Vec<NativeCandidateReducer>,
    memory_bound: ExecutionMemoryBound,
    retained_without_reducer: u128,
) -> Result<(clearra_core_ffi::PackingCandidateBatch, usize), PackingRunnerError> {
    let source_reducer_bytes = reducers
        .iter()
        .map(NativeCandidateReducer::resident_bytes)
        .try_fold(0usize, usize::checked_add)
        .ok_or({
            PackingRunnerError::CandidateReducer(NativePackingCandidateSinkError::MemoryExceeded)
        })?;
    memory_bound
        .ensure(retained_without_reducer, source_reducer_bytes as u128)
        .map_err(buildable_resource_error)?;
    let batches = reducers
        .into_iter()
        .map(NativeCandidateReducer::into_uncanonicalized_candidates)
        .collect::<Vec<_>>();
    let merge_peak_bytes = PackingCandidateBatch::checked_merge_batches_transient_bytes(&batches)
        .ok_or_else(|| buildable_projection_overflow(search_problem))?;
    memory_bound
        .ensure(retained_without_reducer, merge_peak_bytes)
        .map_err(buildable_resource_error)?;
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
    memory_bound
        .ensure(
            retained_without_reducer,
            merged
                .checked_retained_bytes()
                .ok_or_else(|| buildable_projection_overflow(search_problem))?,
        )
        .map_err(buildable_resource_error)?;
    // During the ownership transfer, payload allocations are not duplicated.
    // Account conservatively for the one compact global index allocation that
    // may coexist with worker-local reducer hash tables.
    let reducer_bytes = source_reducer_bytes
        .checked_add(merged.merge_index_resident_bytes())
        .ok_or({
            PackingRunnerError::CandidateReducer(NativePackingCandidateSinkError::MemoryExceeded)
        })?
        .max(merged.resident_bytes());
    Ok((merged, reducer_bytes))
}

#[cfg(test)]
mod resource_projection_tests {
    use super::*;

    #[test]
    fn buildable_owner_components_are_added_once() {
        let plan = BuildableGeometryAllocationPlan {
            template_bytes: 11,
            source_ids_bytes: 13,
            worker_owner_bytes: 17,
            worker_stack_bytes: 19,
        };
        assert_eq!(plan.checked_retained_before_workspace_bytes(), Some(60));
    }

    #[test]
    fn explicit_worker_stack_projection_matches_the_builder_stack_size() {
        let workers = 4u128;
        assert_eq!(
            (BUILDABLE_WORKER_STACK_BYTES as u128).checked_mul(workers),
            Some(8 * 1024 * 1024)
        );
    }

    #[test]
    fn concrete_source_projection_separates_arc_header_from_piece_ids() {
        let ids: Arc<[u32]> = Arc::from([1u32, 2, 3]);
        let selection = BuildabilitySourceSelection::ConcretePatterns(ids);
        assert_eq!(
            selection.checked_retained_bytes(),
            Some((2 * core::mem::size_of::<usize>() + 3 * core::mem::size_of::<u32>()) as u128)
        );
    }
}
