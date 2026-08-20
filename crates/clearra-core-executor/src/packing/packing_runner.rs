use std::sync::Arc;

use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    piece::piece_kind::PieceKind,
    resource::{ResourceReport, ResourceTruncationReason},
};
use clearra_core_ffi::{
    packing_problem::C_PACKING_MAX_OPERATIONS,
    problem::{
        C_PIECE_I, C_PIECE_J, C_PIECE_L, C_PIECE_MULTISET_FAMILY_CAPACITY, C_PIECE_O, C_PIECE_S,
        C_PIECE_T, C_PIECE_Z,
    },
    CPackingCandidate, CPackingProblem, CPackingState, FfiProblemError, NativeGeometryCatalog,
    NativePruningLedger, PackingCandidateBatch, PackingCandidateIter, PackingCandidateView,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_problem::SearchProblem;
use clearra_supply::{PackingHoldProjection, PackingPatternMembershipKind, PieceMultisetKey};

use crate::{
    backend::{
        execute_selected_buildable_packing, BackendTrustReport, NativePackingExecutorRegistry,
        NativeSearchBackendCapabilityProvider, SearchBackendCapabilityProvider,
        SearchBackendExecutorResolver, SearchBackendReport, SelectedSearchBackend,
    },
    buildup::buildup_native_bridge::uses_standard_bag_automaton,
    packing::{
        candidate_pattern_index::CandidatePatternIndex,
        hybrid_scheduler_report::HybridSchedulerReport,
        packing_error::PackingRunnerError,
        packing_memory_report::PackingMemoryReport,
        packing_metrics::{GpuPackingBackendReport, PackingExecutionSource},
        packing_problem_preparer::{
            prepare_packing_problem_for_multiset_family_with_provider,
            prepare_packing_problem_for_multiset_with_provider, PackingProblemPrepareError,
        },
        PackingExecutionPlan, PackingState,
    },
    performance::{ExecutorSearchStage, SearchStageSpan},
};

#[cfg(test)]
use crate::backend::execute_selected_packing;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackingRunResult {
    plan: PackingExecutionPlan,
    backend_report: SearchBackendReport,
    actual_backend: SelectedSearchBackend,
    trust_report: BackendTrustReport,
    execution_source: PackingExecutionSource,
    candidates: PackingCandidateBatch,
    geometry_catalog: Option<NativeGeometryCatalog>,
    pruning_ledgers: Vec<NativePruningLedger>,
    resource_report: ResourceReport,
    gpu_packing_report: GpuPackingBackendReport,
    hybrid_scheduler_report: HybridSchedulerReport,
    memory_report: PackingMemoryReport,
    candidate_patterns: CandidatePatternIndex,
    multiset_group_count: usize,
    multiset_membership_kind: PackingPatternMembershipKind,
    buildability_preverified: bool,
}

impl PackingRunResult {
    pub(crate) fn new(
        plan: PackingExecutionPlan,
        backend_report: SearchBackendReport,
        actual_backend: SelectedSearchBackend,
        trust_report: BackendTrustReport,
        execution_source: PackingExecutionSource,
        candidates: PackingCandidateBatch,
        geometry_catalog: Option<NativeGeometryCatalog>,
        pruning_ledger: Option<NativePruningLedger>,
        resource_report: ResourceReport,
        gpu_packing_report: GpuPackingBackendReport,
        hybrid_scheduler_report: HybridSchedulerReport,
        candidate_patterns: CandidatePatternIndex,
        multiset_group_count: usize,
        multiset_membership_kind: PackingPatternMembershipKind,
        buildability_preverified: bool,
    ) -> Self {
        let memory_report = PackingMemoryReport::from_execution(
            execution_source,
            &resource_report,
            &candidates,
            &candidate_patterns,
        );
        Self {
            plan,
            backend_report,
            actual_backend,
            trust_report,
            execution_source,
            candidates,
            geometry_catalog,
            pruning_ledgers: pruning_ledger.into_iter().collect(),
            resource_report,
            gpu_packing_report,
            hybrid_scheduler_report,
            memory_report,
            candidate_patterns,
            multiset_group_count,
            multiset_membership_kind,
            buildability_preverified,
        }
    }
}
impl PackingRunResult {
    pub fn compact_problem(&self) -> CPackingProblem {
        self.plan.problem()
    }
}
impl PackingRunResult {
    pub(crate) fn source_pattern_ids_before_at(
        &self,
        candidate_index: usize,
        end_exclusive: usize,
    ) -> super::candidate_pattern_index::CandidatePatternIter<'_> {
        self.candidate_patterns
            .patterns_for_candidate_before(candidate_index, end_exclusive)
    }

    pub(crate) fn source_pattern_group_count(&self) -> usize {
        self.candidate_patterns.pattern_group_count()
    }

    pub(crate) fn source_pattern_group_shared(
        &self,
        group_index: usize,
    ) -> Option<Arc<PatternBitSet>> {
        self.candidate_patterns.shared_pattern_group(group_index)
    }

    pub(crate) fn source_pattern_group_index_at(&self, candidate_index: usize) -> Option<usize> {
        self.candidate_patterns
            .candidate_group_index(candidate_index)
    }

    pub(crate) fn source_pattern_count_before(
        &self,
        candidate_index: usize,
        end_exclusive: usize,
    ) -> usize {
        self.candidate_patterns
            .pattern_count_before(candidate_index, end_exclusive)
    }

    pub(crate) fn source_pattern_contains(&self, candidate_index: usize, pattern_id: u32) -> bool {
        self.candidate_patterns
            .contains_pattern(candidate_index, pattern_id)
    }

    pub fn multiset_group_count(&self) -> usize {
        self.multiset_group_count
    }

    pub const fn multiset_membership_kind(&self) -> PackingPatternMembershipKind {
        self.multiset_membership_kind
    }
}
impl PackingRunResult {
    pub fn backend_report(&self) -> &SearchBackendReport {
        &self.backend_report
    }
}
impl PackingRunResult {
    pub fn actual_backend(&self) -> SelectedSearchBackend {
        self.actual_backend
    }
}
impl PackingRunResult {
    pub fn trust_report(&self) -> BackendTrustReport {
        self.trust_report
    }
}
impl PackingRunResult {
    pub fn execution_source(&self) -> PackingExecutionSource {
        self.execution_source
    }
}
impl PackingRunResult {
    pub fn candidates(&self) -> PackingCandidateIter<'_> {
        self.candidates.iter()
    }

    pub fn candidate_at(&self, index: usize) -> Option<CPackingCandidate> {
        self.candidates.candidate_at(index)
    }

    pub fn candidate_view_at(&self, index: usize) -> Option<PackingCandidateView<'_>> {
        self.candidates.candidate_view(index)
    }

    pub fn candidate_id_at(&self, index: usize) -> Option<u64> {
        self.candidates.candidate_id(index)
    }

    pub fn retained_operation_dictionary_entries(&self) -> usize {
        self.candidates.operation_dictionary_len()
    }

    pub fn retained_operation_references(&self) -> usize {
        self.candidates.operation_reference_count()
    }

    pub fn retained_candidate_metadata_bytes(&self) -> usize {
        self.candidates.candidate_metadata_resident_bytes()
    }

    pub fn retained_operation_reference_bytes(&self) -> usize {
        self.candidates.operation_reference_resident_bytes()
    }

    pub fn geometry_catalog(&self) -> Option<&NativeGeometryCatalog> {
        self.geometry_catalog.as_ref()
    }

    pub fn pruning_ledgers(&self) -> &[NativePruningLedger] {
        &self.pruning_ledgers
    }
}
impl PackingRunResult {
    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    pub(crate) const fn buildability_preverified(&self) -> bool {
        self.buildability_preverified
    }
}
impl PackingRunResult {
    pub fn resource_report(&self) -> &ResourceReport {
        &self.resource_report
    }
}
impl PackingRunResult {
    pub fn count_complete(&self) -> bool {
        !self.resource_report.truncated
    }
}
impl PackingRunResult {
    pub fn truncation_reason(&self) -> Option<ResourceTruncationReason> {
        self.resource_report.truncation_reason
    }
}
impl PackingRunResult {
    pub fn gpu_packing_report(&self) -> GpuPackingBackendReport {
        self.gpu_packing_report
    }
}
impl PackingRunResult {
    pub fn hybrid_scheduler_report(&self) -> HybridSchedulerReport {
        self.hybrid_scheduler_report
    }

    pub fn memory_report(&self) -> PackingMemoryReport {
        self.memory_report
    }

    fn refresh_memory_report(&mut self) {
        self.memory_report = PackingMemoryReport::from_execution(
            self.execution_source,
            &self.resource_report,
            &self.candidates,
            &self.candidate_patterns,
        );
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PackingRunner;

impl PackingRunner {
    pub fn run(problem: &SearchProblem) -> Result<PackingRunResult, PackingRunnerError> {
        Self::run_with_cancellation(problem, &ExecutionCancellationToken::new())
    }

    pub fn run_with_cancellation(
        problem: &SearchProblem,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<PackingRunResult, PackingRunnerError> {
        Self::run_with_control(problem, &ExecutionControl::new(cancellation.clone()))
    }

    pub fn run_with_control(
        problem: &SearchProblem,
        control: &ExecutionControl,
    ) -> Result<PackingRunResult, PackingRunnerError> {
        control.report_progress("packing", 0, None);
        if control.is_cancelled() {
            return Err(PackingRunnerError::ExecutionCancelled);
        }
        Self::run_with_components_and_cancellation(
            problem,
            &NativeSearchBackendCapabilityProvider,
            &NativePackingExecutorRegistry::default(),
            &control.cancellation,
        )
        .inspect(|result| {
            control.report_progress(
                "packing",
                result.candidate_count() as u64,
                Some(result.candidate_count() as u64),
            );
        })
    }

    #[cfg(test)]
    pub(crate) fn run_with_components(
        problem: &SearchProblem,
        capability_provider: &impl SearchBackendCapabilityProvider,
        executors: &impl SearchBackendExecutorResolver,
    ) -> Result<PackingRunResult, PackingRunnerError> {
        Self::run_with_components_and_cancellation(
            problem,
            capability_provider,
            executors,
            &ExecutionCancellationToken::new(),
        )
    }

    pub(crate) fn run_with_components_and_cancellation(
        problem: &SearchProblem,
        capability_provider: &impl SearchBackendCapabilityProvider,
        executors: &impl SearchBackendExecutorResolver,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<PackingRunResult, PackingRunnerError> {
        if cancellation.is_cancelled() {
            return Err(PackingRunnerError::ExecutionCancelled);
        }
        let family_span = SearchStageSpan::begin(ExecutorSearchStage::PackingUniverseAndFamily);
        let universe = problem
            .piece_source()
            .materialized_universe()
            .ok_or(PackingRunnerError::NoReachablePieceMultiset)?;
        let placed_piece_count = problem
            .exact_pieces()
            .unwrap_or_else(|| problem.piece_window().max_pieces());
        if placed_piece_count > C_PACKING_MAX_OPERATIONS {
            return Err(PackingRunnerError::Ffi(
                FfiProblemError::PieceWindowTooLarge {
                    max_pieces: placed_piece_count,
                },
            ));
        }
        let family = universe
            .packing_multiset_family_for_execution_with_workers(
                placed_piece_count,
                problem.initial_hold(),
                problem.supply().hold_enabled(),
                if problem.supply().projects_unplaced_lookahead()
                    && !problem.supply().projects_standard_bag_lookahead()
                {
                    PackingHoldProjection::ReleaseHeldAtTerminal
                } else {
                    PackingHoldProjection::PreserveFinalHoldLanguage
                },
                preprocessing_worker_count(problem),
            )
            .map_err(|_| PackingRunnerError::ParallelWorkerPanicked)?;
        if family.is_empty() {
            return Err(PackingRunnerError::NoReachablePieceMultiset);
        }
        family_span.finish(family.len() as u64);

        let backend_span =
            SearchStageSpan::begin(ExecutorSearchStage::PackingBackendAndPatternIndex);
        let mut result = if uses_standard_bag_automaton(problem)
            && family.len() <= C_PIECE_MULTISET_FAMILY_CAPACITY
        {
            Self::run_multiset_family(
                problem,
                &family,
                capability_provider,
                executors,
                cancellation,
            )?
        } else {
            Self::run_multiset_groups(
                problem,
                family.groups(),
                family.membership_kind(),
                capability_provider,
                executors,
                cancellation,
            )?
        };
        backend_span.finish(result.candidate_count() as u64);
        result.multiset_group_count = family.len();
        if !problem.piece_source().complete() {
            result
                .resource_report
                .mark_truncated(ResourceTruncationReason::ObservedUniverseTruncated);
        }
        result.gpu_packing_report =
            GpuPackingBackendReport::from_execution(result.actual_backend, result.trust_report);
        result.hybrid_scheduler_report = HybridSchedulerReport::from_execution(
            result.actual_backend,
            result.trust_report,
            result.candidates.len(),
        );
        Ok(result)
    }

    fn run_multiset_family(
        problem: &SearchProblem,
        family: &clearra_supply::PackingMultisetFamily,
        capability_provider: &impl SearchBackendCapabilityProvider,
        executors: &impl SearchBackendExecutorResolver,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<PackingRunResult, PackingRunnerError> {
        let prepared = prepare_packing_problem_for_multiset_family_with_provider(
            problem,
            family,
            capability_provider,
        )
        .map_err(PackingRunnerError::from_prepare_error)?;
        let compact_problem = prepared.compact_problem();
        let backend_selection = prepared.backend_selection();
        let initial_state = PackingState::from_raw(CPackingState::empty(
            problem.initial_board().occupied_mask(),
        ));
        let plan = PackingExecutionPlan::new(compact_problem, initial_state);
        let outcome = execute_packing_for_problem(
            problem,
            None,
            &backend_selection,
            &compact_problem,
            cancellation,
            executors,
        )?;
        if cancellation.is_cancelled() {
            return Err(PackingRunnerError::ExecutionCancelled);
        }

        let mut candidate_patterns = CandidatePatternIndex::default();
        let mut group_indices = Vec::with_capacity(family.len());
        for group in family.groups() {
            group_indices
                .push(candidate_patterns.push_shared_pattern_group(group.shared_pattern_bits())?);
        }
        for candidate in outcome.candidates.iter() {
            let key = candidate_piece_multiset(candidate)
                .ok_or(PackingRunnerError::CandidateMultisetOutsideFamily)?;
            let Ok(group_index) = family
                .groups()
                .binary_search_by_key(&key, |group| group.key())
            else {
                return Err(PackingRunnerError::CandidateMultisetOutsideFamily);
            };
            if family.groups()[group_index].pattern_bits().is_empty() {
                return Err(PackingRunnerError::CandidateMultisetOutsideFamily);
            }
            candidate_patterns.bind_candidate(group_indices[group_index]);
        }

        let fallback_reason = outcome.fallback.and_then(|fallback| fallback.reason());
        let backend_report = SearchBackendReport::from_execution(
            backend_selection,
            outcome.actual_backend,
            fallback_reason,
            outcome.gpu_failure,
            outcome.gpu_device.clone(),
            outcome.workers_used,
        );
        let execution_source = PackingExecutionSource::from_actual_backend(outcome.actual_backend);
        let gpu_packing_report =
            GpuPackingBackendReport::from_execution(outcome.actual_backend, outcome.trust_report);
        let hybrid_scheduler_report = HybridSchedulerReport::from_execution(
            outcome.actual_backend,
            outcome.trust_report,
            outcome.candidates.len(),
        );
        Ok(PackingRunResult::new(
            plan,
            backend_report,
            outcome.actual_backend,
            outcome.trust_report,
            execution_source,
            outcome.candidates,
            outcome.geometry_catalog,
            outcome.pruning_ledger,
            outcome.resource_report,
            gpu_packing_report,
            hybrid_scheduler_report,
            candidate_patterns,
            family.len(),
            family.membership_kind(),
            outcome.buildability_preverified,
        ))
    }

    fn run_multiset_groups(
        problem: &SearchProblem,
        groups: &[clearra_supply::PackingMultisetGroup],
        membership_kind: PackingPatternMembershipKind,
        capability_provider: &impl SearchBackendCapabilityProvider,
        executors: &impl SearchBackendExecutorResolver,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<PackingRunResult, PackingRunnerError> {
        let mut merged: Option<PackingRunResult> = None;
        let max_candidates = problem.backend_request().max_candidates();
        for (group_index, group) in groups.iter().enumerate() {
            if cancellation.is_cancelled() {
                return Err(PackingRunnerError::ExecutionCancelled);
            }
            let group_result = Self::run_multiset_group(
                problem,
                group.key(),
                group.shared_pattern_bits(),
                membership_kind,
                capability_provider,
                executors,
                cancellation,
            )?;
            merge_group_result(&mut merged, group_result)?;
            let result = merged
                .as_mut()
                .expect("a merged packing group result was just inserted");
            if max_candidates != 0 && result.candidates.len() > max_candidates {
                truncate_candidates(result, max_candidates);
                result
                    .resource_report
                    .mark_truncated(ResourceTruncationReason::CandidateBudgetExceeded);
                break;
            }
            if max_candidates != 0
                && result.candidates.len() == max_candidates
                && group_index + 1 < groups.len()
            {
                result
                    .resource_report
                    .mark_truncated(ResourceTruncationReason::CandidateBudgetExceeded);
                break;
            }
        }
        merged.ok_or(PackingRunnerError::NoReachablePieceMultiset)
    }

    fn run_multiset_group(
        problem: &SearchProblem,
        piece_multiset: PieceMultisetKey,
        source_pattern_bits: Arc<PatternBitSet>,
        membership_kind: PackingPatternMembershipKind,
        capability_provider: &impl SearchBackendCapabilityProvider,
        executors: &impl SearchBackendExecutorResolver,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<PackingRunResult, PackingRunnerError> {
        let prepared = prepare_packing_problem_for_multiset_with_provider(
            problem,
            piece_multiset,
            capability_provider,
        )
        .map_err(PackingRunnerError::from_prepare_error)?;
        let compact_problem = prepared.compact_problem();
        let backend_selection = prepared.backend_selection();
        let initial_state = PackingState::from_raw(CPackingState::empty(
            problem.initial_board().occupied_mask(),
        ));
        let plan = PackingExecutionPlan::new(compact_problem, initial_state);
        let outcome = execute_packing_for_problem(
            problem,
            Some(source_pattern_bits.as_ref()),
            &backend_selection,
            &compact_problem,
            cancellation,
            executors,
        )?;
        if cancellation.is_cancelled() {
            return Err(PackingRunnerError::ExecutionCancelled);
        }
        let fallback_reason = outcome.fallback.and_then(|fallback| fallback.reason());
        let backend_report = SearchBackendReport::from_execution(
            backend_selection,
            outcome.actual_backend,
            fallback_reason,
            outcome.gpu_failure,
            outcome.gpu_device.clone(),
            outcome.workers_used,
        );
        let execution_source = PackingExecutionSource::from_actual_backend(outcome.actual_backend);
        let gpu_packing_report =
            GpuPackingBackendReport::from_execution(outcome.actual_backend, outcome.trust_report);
        let hybrid_scheduler_report = HybridSchedulerReport::from_execution(
            outcome.actual_backend,
            outcome.trust_report,
            outcome.candidates.len(),
        );

        let mut candidate_patterns = CandidatePatternIndex::default();
        let group_index = candidate_patterns.push_shared_pattern_group(source_pattern_bits)?;
        for _ in 0..outcome.candidates.len() {
            candidate_patterns.bind_candidate(group_index);
        }
        let result = PackingRunResult::new(
            plan,
            backend_report,
            outcome.actual_backend,
            outcome.trust_report,
            execution_source,
            outcome.candidates,
            outcome.geometry_catalog,
            outcome.pruning_ledger,
            outcome.resource_report,
            gpu_packing_report,
            hybrid_scheduler_report,
            candidate_patterns,
            1,
            membership_kind,
            outcome.buildability_preverified,
        );

        debug_assert_eq!(
            result.backend_report().selected_backend(),
            result.actual_backend()
        );

        Ok(result)
    }
}

fn execute_packing_for_problem(
    problem: &SearchProblem,
    source_pattern_bits: Option<&PatternBitSet>,
    backend_selection: &crate::backend::PcBackendSelection,
    compact_problem: &CPackingProblem,
    cancellation: &ExecutionCancellationToken,
    executors: &impl SearchBackendExecutorResolver,
) -> Result<crate::backend::PackingBackendOutcome, PackingRunnerError> {
    if should_stream_buildable_candidates(problem, source_pattern_bits, executors) {
        return execute_selected_buildable_packing(
            problem,
            source_pattern_bits,
            backend_selection,
            compact_problem,
            problem.backend_request(),
            cancellation,
        );
    }
    #[cfg(test)]
    {
        return execute_selected_packing(
            backend_selection,
            compact_problem,
            problem.backend_request(),
            cancellation,
            executors,
        );
    }
    #[cfg(not(test))]
    {
        Err(PackingRunnerError::BackendExecutorUnavailable {
            backend: backend_selection.selected_backend(),
            reason: "buildable_geometry_stream_required",
        })
    }
}

fn should_stream_buildable_candidates(
    problem: &SearchProblem,
    source_pattern_bits: Option<&PatternBitSet>,
    executors: &impl SearchBackendExecutorResolver,
) -> bool {
    executors.supports_native_candidate_streaming()
        && (uses_standard_bag_automaton(problem) || source_pattern_bits.is_some())
}

fn preprocessing_worker_count(problem: &SearchProblem) -> usize {
    problem.backend_request().workers()
}

fn candidate_piece_multiset(candidate: CPackingCandidate) -> Option<PieceMultisetKey> {
    let operation_count = usize::from(candidate.operation_count);
    if operation_count > candidate.operations.len() {
        return None;
    }
    let mut pieces = [PieceKind::I; C_PACKING_MAX_OPERATIONS];
    for (index, operation) in candidate.operations[..operation_count].iter().enumerate() {
        pieces[index] = match operation.piece {
            C_PIECE_I => PieceKind::I,
            C_PIECE_O => PieceKind::O,
            C_PIECE_T => PieceKind::T,
            C_PIECE_S => PieceKind::S,
            C_PIECE_Z => PieceKind::Z,
            C_PIECE_J => PieceKind::J,
            C_PIECE_L => PieceKind::L,
            _ => return None,
        };
    }
    Some(PieceMultisetKey::from_pieces(
        pieces[..operation_count].iter().copied(),
    ))
}

fn merge_group_result(
    merged: &mut Option<PackingRunResult>,
    mut group: PackingRunResult,
) -> Result<(), PackingRunnerError> {
    let Some(result) = merged.as_mut() else {
        *merged = Some(group);
        return Ok(());
    };
    if result.actual_backend != group.actual_backend {
        return Err(PackingRunnerError::BackendExecutionMismatch {
            selected: result.actual_backend,
            actual: group.actual_backend,
        });
    }
    if result.trust_report != group.trust_report {
        return Err(PackingRunnerError::BackendTrustMismatch {
            backend: group.actual_backend,
            trust_state: group.trust_report.state(),
        });
    }
    result.buildability_preverified &= group.buildability_preverified;
    match (&result.geometry_catalog, group.geometry_catalog.take()) {
        (Some(existing), Some(incoming)) if existing != &incoming => {
            return Err(PackingRunnerError::GeometryCatalogMismatch);
        }
        (None, Some(incoming)) => result.geometry_catalog = Some(incoming),
        _ => {}
    }
    result.pruning_ledgers.append(&mut group.pruning_ledgers);

    assign_group_candidate_ids(result.candidates.len(), &mut group)?;
    result
        .candidates
        .append(group.candidates)
        .map_err(PackingRunnerError::CandidateBatch)?;
    result.candidate_patterns.append(group.candidate_patterns)?;
    debug_assert_eq!(
        result.candidates.len(),
        result.candidate_patterns.candidate_count()
    );
    merge_resource_reports(&mut result.resource_report, &group.resource_report);
    result.refresh_memory_report();
    Ok(())
}

fn assign_group_candidate_ids(
    existing_candidate_count: usize,
    group: &mut PackingRunResult,
) -> Result<(), PackingRunnerError> {
    let first_id = u64::try_from(existing_candidate_count)
        .map_err(|_| PackingRunnerError::CandidateIdentityExhausted)?
        .checked_add(1)
        .ok_or(PackingRunnerError::CandidateIdentityExhausted)?;
    for index in 0..group.candidates.len() {
        let resolved_id = first_id
            .checked_add(
                u64::try_from(index).map_err(|_| PackingRunnerError::CandidateIdentityExhausted)?,
            )
            .ok_or(PackingRunnerError::CandidateIdentityExhausted)?;
        group
            .candidates
            .set_identity(index, resolved_id, resolved_id)
            .map_err(PackingRunnerError::CandidateBatch)?;
    }
    Ok(())
}

fn truncate_candidates(result: &mut PackingRunResult, max_candidates: usize) {
    result.candidates.truncate(max_candidates);
    result
        .candidate_patterns
        .truncate_candidates(max_candidates);
    result.refresh_memory_report();
}

fn merge_resource_reports(target: &mut ResourceReport, source: &ResourceReport) {
    if let Some(reason) = source.truncation_reason {
        target.mark_truncated(reason);
    }
    target.observe_frontier_states(source.peak_frontier_states);
    target.observe_candidate_rows(source.peak_candidate_rows);
    target.observe_hash_buckets(source.peak_hash_buckets);
    target.observe_gpu_bytes(source.peak_gpu_bytes);
    target.observe_cpu_bytes(source.peak_cpu_bytes);
    target.observe_build_worker_backlog(source.build_worker_backlog_peak);
    target.coverage_rows_emitted = target
        .coverage_rows_emitted
        .saturating_add(source.coverage_rows_emitted);
    target.probability_complete &= source.probability_complete;
}

impl PackingRunnerError {
    fn from_prepare_error(error: PackingProblemPrepareError) -> Self {
        match error {
            PackingProblemPrepareError::Ffi(error) => Self::Ffi(error),
            PackingProblemPrepareError::Backend(error) => Self::Backend(error),
        }
    }
}

#[cfg(test)]
#[path = "packing_runner_tests.rs"]
mod tests;
