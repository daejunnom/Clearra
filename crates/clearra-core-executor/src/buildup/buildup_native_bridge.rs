use std::{
    borrow::Borrow,
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;
use clearra_core_ffi::{
    CBuildUpProblem, CBuildUpProblemTemplate, CBuildUpResult, CBuildVariantView,
    CNativeBuildUpEnumerationLimits, CNativeBuildVariantBuffer, CPackingCandidate,
    NativeBuildUpWorkspace, NativeCoreError, PackingCandidateView,
    C_BUILDUP_STATUS_CAPACITY_EXCEEDED, C_BUILDUP_STATUS_ENUMERATION_TRUNCATED,
    C_BUILDUP_STATUS_INVALID_ARGUMENT, C_BUILDUP_STATUS_INVALID_ORDER,
    C_BUILDUP_STATUS_INVALID_PROBLEM, C_BUILDUP_STATUS_KICK_EVIDENCE_BUFFER_EXHAUSTED,
    C_BUILDUP_STATUS_LOGICAL_REJECT_MAX, C_BUILDUP_STATUS_LOGICAL_REJECT_MIN, C_BUILDUP_STATUS_OK,
    C_BUILDUP_STATUS_UNSUPPORTED_RUNTIME_SCOPE,
};
use clearra_pc_graph::request::PcQueueInput;
use clearra_problem::SearchProblem;
use clearra_supply::{PatternPiecePositionIndex, PatternPiecePositionIndexError};

use crate::{
    buildup::{
        buildup_candidate_acceptance::BuildUpCandidateAcceptance,
        buildup_coverage_bridge::{
            coverage_pattern_selection_for_problem, coverage_source_for_problem, pattern_count,
            CoveragePatternSelection, PatternCoverageVerification,
            PatternVerifiedCandidateCoverage,
        },
        buildup_error::BuildUpRunnerError,
        buildup_geometry_language_evaluator::GeometryHoldLanguageEvaluator,
        buildup_geometry_language_execution, buildup_parallelism, BuildUpExecutionMode,
        ExecutionVariantSet,
    },
    packing::{packing_runner::PackingRunResult, scenario_packing_witness::ScenarioPackingWitness},
    performance::{ExecutorSearchStage, SearchStageSpan},
};

const CANDIDATE_WORK_CHUNK_SIZE: usize = 32;
const SERIAL_PROGRESS_CHUNK_SIZE: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CBuildUpExecution {
    pub(crate) candidate_acceptance: BuildUpCandidateAcceptance,
    pub(crate) execution_variants: ExecutionVariantSet,
    pub(crate) coverage_verifications: Vec<PatternVerifiedCandidateCoverage>,
    pub(crate) pattern_verified_execution_count: usize,
    pub(crate) total_build_variant_count: Option<usize>,
    pub(crate) execution_mode: BuildUpExecutionMode,
    pub(crate) coverage_source: &'static str,
    pub(crate) count_complete: bool,
    pub(crate) count_truncated_reason: &'static str,
    pub(crate) peak_workspace_bytes: usize,
}

struct CandidateBuildUpExecution {
    candidate_index: usize,
    result: CBuildUpResult,
    retained_variants: Vec<CBuildVariantView>,
    coverage_verifications: Vec<PatternVerifiedCandidateCoverage>,
    pattern_verified_execution_count: usize,
    total_build_variant_count: Option<usize>,
    count_complete: bool,
    count_truncated_reason: &'static str,
}

struct CandidateBuildUpExecutionDetail {
    candidate_index: usize,
    retained_variants: Vec<CBuildVariantView>,
    coverage_verifications: Vec<PatternVerifiedCandidateCoverage>,
    pattern_verified_execution_count: usize,
    total_build_variant_count: Option<usize>,
    count_complete: bool,
    count_truncated_reason: &'static str,
}

struct CandidateBuildUpBatch {
    results: Vec<CBuildUpResult>,
    details: Vec<CandidateBuildUpExecutionDetail>,
    peak_workspace_bytes: usize,
}

#[derive(Clone, Copy)]
struct CandidateBuildUpChunkSpan {
    start_index: usize,
    result_start: usize,
    result_len: usize,
}

struct CandidateBuildUpWorkerOutput {
    chunks: Vec<CandidateBuildUpChunkSpan>,
    results: Vec<CBuildUpResult>,
    details: Vec<CandidateBuildUpExecutionDetail>,
    workspace_bytes: usize,
}

#[derive(Clone, Copy)]
struct CandidateBuildUpChunkOrder {
    start_index: usize,
    worker_index: usize,
    result_start: usize,
    result_len: usize,
}

pub(crate) fn c_buildup_results(
    problem: &SearchProblem,
    packing: &PackingRunResult,
    cancellation: &ExecutionCancellationToken,
    control: &clearra_core_domain::execution_cancellation::ExecutionControl,
) -> Result<CBuildUpExecution, BuildUpRunnerError> {
    let candidate_count = packing.candidate_count();
    let pattern_count = pattern_count(problem);
    let coverage_pattern_selection =
        coverage_pattern_selection_for_problem(problem, pattern_count)?;
    let buildup_template =
        CBuildUpProblemTemplate::compile(problem).map_err(BuildUpRunnerError::Ffi)?;
    let universe = problem.piece_source().materialized_universe().ok_or(
        BuildUpRunnerError::UnsupportedPieceSource {
            reason: "pattern_universe_not_materialized",
        },
    )?;
    let pattern_indices =
        compile_pattern_group_indices(universe, packing, coverage_pattern_selection)?;
    // The typed save producer needs the complete private execution batch in
    // order to derive exact, query-bound terminal evidence. Ordinary Trace
    // output remains governed by `retained_trace_limit`.
    let materialized_variant_limit =
        if score_matrix_requested(problem) || complete_trace_batch_requested(problem) {
            problem.resource_budget().max_results()
        } else {
            problem.trace_policy().retained_trace_limit()
        };
    let candidate_batch = execute_candidates(
        problem,
        packing,
        &buildup_template,
        &pattern_indices,
        coverage_pattern_selection,
        materialized_variant_limit,
        cancellation,
        control,
    )?;

    let CandidateBuildUpBatch {
        results,
        details,
        peak_workspace_bytes: worker_workspace_bytes,
    } = candidate_batch;
    let mut retained_pattern_indices = HashSet::new();
    let pattern_index_bytes = pattern_indices.iter().fold(0usize, |total, index| {
        if retained_pattern_indices.insert(Arc::as_ptr(index)) {
            total.saturating_add(index.retained_bytes())
        } else {
            total
        }
    });
    let peak_workspace_bytes = worker_workspace_bytes.saturating_add(pattern_index_bytes);
    let mut execution_variants = ExecutionVariantSet::default();
    let mut coverage_verifications = Vec::new();
    let mut pattern_verified_execution_count = 0usize;
    let mut total_build_variant_count = Some(0usize);
    let mut count_complete = true;
    let mut count_truncated_reason = "none";
    for detail in details {
        coverage_verifications.extend(detail.coverage_verifications);
        pattern_verified_execution_count = pattern_verified_execution_count
            .checked_add(detail.pattern_verified_execution_count)
            .ok_or(BuildUpRunnerError::PatternVerifiedExecutionCountOverflow)?;
        total_build_variant_count =
            match (total_build_variant_count, detail.total_build_variant_count) {
                (Some(total), Some(candidate_total)) => Some(
                    total
                        .checked_add(candidate_total)
                        .ok_or(BuildUpRunnerError::BuildVariantCountOverflow)?,
                ),
                _ => None,
            };
        if !detail.count_complete {
            count_complete = false;
            count_truncated_reason = detail.count_truncated_reason;
        }
        retain_execution_variants(
            &mut execution_variants,
            detail.retained_variants,
            materialized_variant_limit,
        );
    }
    control.report_progress(
        "buildup",
        candidate_count as u64,
        Some(candidate_count as u64),
    );
    Ok(CBuildUpExecution {
        candidate_acceptance: BuildUpCandidateAcceptance::explicit(results),
        execution_variants,
        coverage_verifications,
        pattern_verified_execution_count,
        total_build_variant_count,
        execution_mode: BuildUpExecutionMode::coverage_producing(),
        coverage_source: coverage_source_for_problem(
            problem,
            "pattern-specific-exact-buildability",
        ),
        count_complete,
        count_truncated_reason,
        peak_workspace_bytes,
    })
}

fn compile_pattern_group_indices(
    universe: &clearra_supply::MaterializedPatternUniverse,
    packing: &PackingRunResult,
    coverage_pattern_selection: CoveragePatternSelection,
) -> Result<Vec<Arc<PatternPiecePositionIndex>>, BuildUpRunnerError> {
    let CoveragePatternSelection::Range { end_exclusive } = coverage_pattern_selection else {
        return Ok(Vec::new());
    };
    let group_count = packing.source_pattern_group_count();
    let mut indices = Vec::new();
    indices
        .try_reserve_exact(group_count)
        .map_err(|_| BuildUpRunnerError::PatternProductStorageUnavailable)?;
    let mut compiled_by_membership = HashMap::<usize, Arc<PatternPiecePositionIndex>>::new();
    for group_index in 0..group_count {
        let patterns = packing
            .source_pattern_group_shared(group_index)
            .ok_or(BuildUpRunnerError::InvalidGeometryLanguage)?;
        let membership_identity = Arc::as_ptr(&patterns) as usize;
        let index = if let Some(index) = compiled_by_membership.get(&membership_identity) {
            Arc::clone(index)
        } else {
            let index = Arc::new(
                PatternPiecePositionIndex::compile_subset_before(
                    universe,
                    patterns.as_ref(),
                    end_exclusive,
                )
                .map_err(|error| match error {
                    PatternPiecePositionIndexError::UniverseMismatch => {
                        BuildUpRunnerError::InvalidGeometryLanguage
                    }
                    _ => BuildUpRunnerError::PatternProductStorageUnavailable,
                })?,
            );
            compiled_by_membership.insert(membership_identity, Arc::clone(&index));
            index
        };
        indices.push(index);
    }
    Ok(indices)
}

fn execute_candidates(
    problem: &SearchProblem,
    packing: &PackingRunResult,
    buildup_template: &CBuildUpProblemTemplate,
    pattern_indices: &[Arc<PatternPiecePositionIndex>],
    coverage_pattern_selection: CoveragePatternSelection,
    materialized_variant_limit: usize,
    cancellation: &ExecutionCancellationToken,
    control: &clearra_core_domain::execution_cancellation::ExecutionControl,
) -> Result<CandidateBuildUpBatch, BuildUpRunnerError> {
    let candidate_count = packing.candidate_count();
    let worker_count = buildup_parallelism::worker_count(problem, candidate_count);
    if worker_count <= 1 {
        let mut workspace = NativeBuildUpWorkspace::new();
        let mut language_evaluator = GeometryHoldLanguageEvaluator::default();
        let mut buildup_scratch = buildup_scratch_with_catalog(buildup_template, packing);
        let mut retained = 0usize;
        let mut results = Vec::with_capacity(candidate_count);
        let mut details = Vec::new();
        for candidate_index in 0..candidate_count {
            let candidate = packing
                .candidate_view_at(candidate_index)
                .ok_or(BuildUpRunnerError::PackingCandidateUnavailable { candidate_index })?;
            let execution = execute_candidate(
                problem,
                packing,
                candidate_index,
                candidate,
                buildup_template,
                pattern_indices,
                &mut buildup_scratch,
                coverage_pattern_selection,
                materialized_variant_limit.saturating_sub(retained),
                cancellation,
                &mut workspace,
                &mut language_evaluator,
            )?;
            retained = retained.saturating_add(execution.retained_variants.len());
            push_candidate_execution(&mut results, &mut details, execution);
            let completed = candidate_index + 1;
            if completed % SERIAL_PROGRESS_CHUNK_SIZE == 0 || completed == candidate_count {
                control.report_progress("buildup", completed as u64, Some(candidate_count as u64));
            }
        }
        return Ok(CandidateBuildUpBatch {
            results,
            details,
            peak_workspace_bytes: workspace
                .retained_bytes()
                .saturating_add(language_evaluator.retained_bytes()),
        });
    }

    let next_candidate = AtomicUsize::new(0);
    let completed = AtomicUsize::new(0);
    let mut worker_outputs = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let next_candidate = &next_candidate;
            let completed = &completed;
            handles.push(scope.spawn(move || {
                let mut workspace = NativeBuildUpWorkspace::new();
                let mut language_evaluator = GeometryHoldLanguageEvaluator::default();
                let mut buildup_scratch = buildup_scratch_with_catalog(buildup_template, packing);
                let mut retained = 0usize;
                let expected_candidates = candidate_count.div_ceil(worker_count);
                let mut local_chunks =
                    Vec::with_capacity(expected_candidates.div_ceil(CANDIDATE_WORK_CHUNK_SIZE));
                let mut local_results = Vec::with_capacity(expected_candidates);
                let mut local_details = Vec::new();
                loop {
                    let chunk_begin =
                        next_candidate.fetch_add(CANDIDATE_WORK_CHUNK_SIZE, Ordering::Relaxed);
                    if chunk_begin >= candidate_count {
                        break;
                    }
                    let chunk_end = chunk_begin
                        .saturating_add(CANDIDATE_WORK_CHUNK_SIZE)
                        .min(candidate_count);
                    let result_start = local_results.len();
                    for candidate_index in chunk_begin..chunk_end {
                        let candidate = packing.candidate_view_at(candidate_index).ok_or(
                            BuildUpRunnerError::PackingCandidateUnavailable { candidate_index },
                        )?;
                        let execution = execute_candidate(
                            problem,
                            packing,
                            candidate_index,
                            candidate,
                            buildup_template,
                            pattern_indices,
                            &mut buildup_scratch,
                            coverage_pattern_selection,
                            materialized_variant_limit.saturating_sub(retained),
                            cancellation,
                            &mut workspace,
                            &mut language_evaluator,
                        )?;
                        retained = retained.saturating_add(execution.retained_variants.len());
                        push_candidate_execution(&mut local_results, &mut local_details, execution);
                    }
                    local_chunks.push(CandidateBuildUpChunkSpan {
                        start_index: chunk_begin,
                        result_start,
                        result_len: chunk_end - chunk_begin,
                    });
                    let chunk_len = chunk_end - chunk_begin;
                    let progress = completed.fetch_add(chunk_len, Ordering::Relaxed) + chunk_len;
                    if progress % SERIAL_PROGRESS_CHUNK_SIZE == 0 || progress == candidate_count {
                        control.report_progress(
                            "buildup",
                            progress as u64,
                            Some(candidate_count as u64),
                        );
                    }
                }
                Ok::<_, BuildUpRunnerError>(CandidateBuildUpWorkerOutput {
                    chunks: local_chunks,
                    results: local_results,
                    details: local_details,
                    workspace_bytes: workspace
                        .retained_bytes()
                        .saturating_add(language_evaluator.retained_bytes()),
                })
            }));
        }

        let mut worker_outputs = Vec::with_capacity(worker_count);
        for handle in handles {
            let output = handle
                .join()
                .map_err(|_| BuildUpRunnerError::ParallelWorkerPanicked)??;
            worker_outputs.push(output);
        }
        Ok::<_, BuildUpRunnerError>(worker_outputs)
    })?;
    let peak_workspace_bytes = worker_outputs.iter().fold(0usize, |total, output| {
        total.saturating_add(output.workspace_bytes)
    });
    let mut chunk_order = Vec::with_capacity(candidate_count.div_ceil(CANDIDATE_WORK_CHUNK_SIZE));
    let mut details = Vec::new();
    for (worker_index, output) in worker_outputs.iter_mut().enumerate() {
        chunk_order.extend(
            output
                .chunks
                .iter()
                .map(|chunk| CandidateBuildUpChunkOrder {
                    start_index: chunk.start_index,
                    worker_index,
                    result_start: chunk.result_start,
                    result_len: chunk.result_len,
                }),
        );
        details.append(&mut output.details);
    }
    chunk_order.sort_unstable_by_key(|chunk| chunk.start_index);
    let mut results = Vec::with_capacity(candidate_count);
    for chunk in chunk_order {
        let end = chunk.result_start + chunk.result_len;
        results.extend_from_slice(
            &worker_outputs[chunk.worker_index].results[chunk.result_start..end],
        );
    }
    details.sort_unstable_by_key(|detail| detail.candidate_index);
    Ok(CandidateBuildUpBatch {
        results,
        details,
        peak_workspace_bytes,
    })
}

pub(super) fn buildup_scratch_with_catalog(
    template: &CBuildUpProblemTemplate,
    packing: &PackingRunResult,
) -> CBuildUpProblem {
    let mut scratch = template.new_scratch();
    if let Some(catalog) = packing.geometry_catalog() {
        template.attach_geometry_catalog(&mut scratch, catalog);
    }
    scratch
}

fn push_candidate_execution(
    results: &mut Vec<CBuildUpResult>,
    details: &mut Vec<CandidateBuildUpExecutionDetail>,
    execution: CandidateBuildUpExecution,
) {
    results.push(execution.result);
    if execution.result.success != 0
        || !execution.retained_variants.is_empty()
        || !execution.coverage_verifications.is_empty()
        || execution.pattern_verified_execution_count != 0
        || !execution.count_complete
    {
        details.push(CandidateBuildUpExecutionDetail {
            candidate_index: execution.candidate_index,
            retained_variants: execution.retained_variants,
            coverage_verifications: execution.coverage_verifications,
            pattern_verified_execution_count: execution.pattern_verified_execution_count,
            total_build_variant_count: execution.total_build_variant_count,
            count_complete: execution.count_complete,
            count_truncated_reason: execution.count_truncated_reason,
        });
    }
}

fn execute_candidate(
    problem: &SearchProblem,
    packing: &PackingRunResult,
    candidate_index: usize,
    candidate: PackingCandidateView<'_>,
    buildup_template: &CBuildUpProblemTemplate,
    pattern_indices: &[Arc<PatternPiecePositionIndex>],
    buildup_scratch: &mut CBuildUpProblem,
    coverage_pattern_selection: CoveragePatternSelection,
    materialized_variant_limit: usize,
    cancellation: &ExecutionCancellationToken,
    native_workspace: &mut NativeBuildUpWorkspace,
    language_evaluator: &mut GeometryHoldLanguageEvaluator,
) -> Result<CandidateBuildUpExecution, BuildUpRunnerError> {
    if cancellation.is_cancelled() {
        return Err(BuildUpRunnerError::ExecutionCancelled);
    }
    let binding_count =
        candidate_pattern_binding_count(packing, candidate_index, coverage_pattern_selection);
    if binding_count == 0 {
        return Ok(candidate_without_verified_patterns(
            candidate_index,
            candidate,
        ));
    }
    let pattern_index = match coverage_pattern_selection {
        CoveragePatternSelection::Single(_) => None,
        CoveragePatternSelection::Range { .. } => {
            let group_index = packing
                .source_pattern_group_index_at(candidate_index)
                .ok_or(BuildUpRunnerError::InvalidGeometryLanguage)?;
            Some(
                pattern_indices
                    .get(group_index)
                    .ok_or(BuildUpRunnerError::InvalidGeometryLanguage)?
                    .as_ref(),
            )
        }
    };
    // The geometry-language product proves per-pattern coverage but deliberately
    // does not retain every concrete BuildVariant.  A complete CountAll+Trace
    // consumer needs those variants as query-bound replay evidence, so it must
    // use the exhaustive per-binding path below.
    if !complete_trace_batch_requested(problem) {
        if let Some(execution) = buildup_geometry_language_execution::try_execute_candidate(
            problem,
            packing,
            candidate_index,
            candidate,
            buildup_template,
            buildup_scratch,
            coverage_pattern_selection,
            materialized_variant_limit,
            cancellation,
            native_workspace,
            language_evaluator,
            pattern_index,
        )? {
            return Ok(candidate_from_geometry_language(
                candidate_index,
                candidate,
                execution,
            ));
        }
    }
    if binding_count > 1
        && !score_matrix_requested(problem)
        && !complete_trace_batch_requested(problem)
    {
        return Err(BuildUpRunnerError::ExactGeometryLanguageRequired {
            candidate_id: candidate.candidate_id(),
            binding_count,
        });
    }
    let mut candidate_success = false;
    let mut candidate_cleared_lines = 0u8;
    let mut retained_variants = Vec::new();
    let mut coverage_verifications = Vec::new();
    let mut pattern_verified_execution_count = 0usize;
    let mut total_build_variant_count = 0usize;
    let mut count_complete = true;
    let mut count_truncated_reason = "none";
    let mut pattern_bindings =
        candidate_pattern_bindings(packing, candidate_index, coverage_pattern_selection);
    loop {
        let binding_span = SearchStageSpan::begin(ExecutorSearchStage::BuildUpPatternBinding);
        let binding = pattern_bindings.next();
        binding_span.finish(binding.is_some() as u64);
        let Some((piece_source_pattern_id, coverage_pattern_id)) = binding else {
            break;
        };
        if cancellation.is_cancelled() {
            return Err(BuildUpRunnerError::ExecutionCancelled);
        }
        let lowering_span = SearchStageSpan::begin(ExecutorSearchStage::BuildUpProblemLowering);
        buildup_template
            .configure_packing_candidate_view(
                buildup_scratch,
                candidate,
                piece_source_pattern_id,
                coverage_pattern_id,
            )
            .map_err(BuildUpRunnerError::Ffi)?;
        lowering_span.finish(1);
        let limits = buildup_enumeration_limits(problem);
        let native_call_span = SearchStageSpan::begin(ExecutorSearchStage::BuildUpNativeCall);
        let native_outcome = native_workspace.enumerate_buildup_variants_with_cancellation(
            buildup_scratch,
            &limits,
            cancellation,
        );
        native_call_span.finish(1);
        match native_outcome {
            Ok(outcome) => {
                if buildup_status_is_fatal(outcome.status) {
                    return Err(BuildUpRunnerError::Native(NativeCoreError::BuildUpStatus(
                        outcome.status,
                    )));
                }
                let retained_variant_count = usize::from(outcome.buffer.count);
                let (pattern_variant_count, pattern_count_complete, pattern_count_reason) =
                    complete_variant_count(outcome.status, outcome.buffer, retained_variant_count)?;
                total_build_variant_count = total_build_variant_count
                    .checked_add(pattern_variant_count)
                    .ok_or(BuildUpRunnerError::BuildVariantCountOverflow)?;
                if !pattern_count_complete {
                    count_complete = false;
                    count_truncated_reason = pattern_count_reason;
                }
                if buildup_status_can_keep_partial_variants(outcome.status)
                    && pattern_variant_count > 0
                {
                    let variant_copy_span =
                        SearchStageSpan::begin(ExecutorSearchStage::BuildUpVariantCopy);
                    let accepted_variants = accepted_build_variants_from_buffer(outcome.buffer)?;
                    variant_copy_span.finish(accepted_variants.len() as u64);
                    candidate_success = true;
                    candidate_cleared_lines = candidate_cleared_lines.max(
                        accepted_variants
                            .iter()
                            .map(CBuildVariantView::cleared_lines)
                            .max()
                            .unwrap_or(0),
                    );
                    coverage_verifications.push(PatternVerifiedCandidateCoverage::new(
                        candidate.candidate_id(),
                        PatternCoverageVerification::pattern_specific_buildup(coverage_pattern_id),
                    ));
                    pattern_verified_execution_count = pattern_verified_execution_count
                        .checked_add(1)
                        .ok_or(BuildUpRunnerError::PatternVerifiedExecutionCountOverflow)?;
                    let remaining =
                        materialized_variant_limit.saturating_sub(retained_variants.len());
                    retained_variants.extend(accepted_variants.into_iter().take(remaining));
                }
            }
            Err(NativeCoreError::Unavailable) => {
                return Err(BuildUpRunnerError::Native(NativeCoreError::Unavailable));
            }
            Err(NativeCoreError::ExecutionCancelled) => {
                return Err(BuildUpRunnerError::ExecutionCancelled);
            }
            Err(error) => return Err(BuildUpRunnerError::Native(error)),
        }
    }

    Ok(CandidateBuildUpExecution {
        candidate_index,
        result: CBuildUpResult {
            candidate_id: candidate.candidate_id(),
            success: candidate_success as u8,
            cleared_lines: candidate_cleared_lines,
            reserved: 0,
        },
        retained_variants,
        coverage_verifications,
        pattern_verified_execution_count,
        total_build_variant_count: Some(total_build_variant_count),
        count_complete,
        count_truncated_reason,
    })
}

fn candidate_from_geometry_language(
    candidate_index: usize,
    candidate: PackingCandidateView<'_>,
    execution: buildup_geometry_language_execution::GeometryLanguageCandidateExecution,
) -> CandidateBuildUpExecution {
    CandidateBuildUpExecution {
        candidate_index,
        result: CBuildUpResult {
            candidate_id: candidate.candidate_id(),
            success: execution.success as u8,
            cleared_lines: execution.cleared_lines,
            reserved: 0,
        },
        retained_variants: execution.retained_variants,
        coverage_verifications: execution.coverage_verifications,
        pattern_verified_execution_count: execution.pattern_verified_execution_count,
        // The language product proves coverage/existence without enumerating
        // every concrete BuildVariant, so it cannot authorize score completeness.
        total_build_variant_count: None,
        count_complete: true,
        count_truncated_reason: "none",
    }
}

fn candidate_without_verified_patterns(
    candidate_index: usize,
    candidate: PackingCandidateView<'_>,
) -> CandidateBuildUpExecution {
    CandidateBuildUpExecution {
        candidate_index,
        result: CBuildUpResult {
            candidate_id: candidate.candidate_id(),
            success: 0,
            cleared_lines: 0,
            reserved: 0,
        },
        retained_variants: Vec::new(),
        coverage_verifications: Vec::new(),
        pattern_verified_execution_count: 0,
        total_build_variant_count: Some(0),
        count_complete: true,
        count_truncated_reason: "none",
    }
}

pub(crate) fn uses_standard_bag_automaton(problem: &SearchProblem) -> bool {
    matches!(problem.supply().queue(), PcQueueInput::Standard7Bag)
        && problem
            .piece_source()
            .bag_universe_descriptor()
            .is_some_and(|bag| {
                bag.pattern()
                    == clearra_core_domain::piece::piece_kind::PieceKind::STANDARD_TETROMINOES
            })
}

pub(crate) enum CandidatePatternBindings<'a> {
    Range(super::super::packing::candidate_pattern_index::CandidatePatternIter<'a>),
    Single(Option<u32>),
}

impl Iterator for CandidatePatternBindings<'_> {
    type Item = (u32, u32);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Range(patterns) => patterns.next().map(|pattern_id| (pattern_id, pattern_id)),
            Self::Single(pattern_id) => {
                pattern_id.take().map(|pattern_id| (pattern_id, pattern_id))
            }
        }
    }
}

pub(crate) fn candidate_pattern_bindings(
    packing: &PackingRunResult,
    candidate_index: usize,
    requested_patterns: CoveragePatternSelection,
) -> CandidatePatternBindings<'_> {
    match requested_patterns {
        CoveragePatternSelection::Range { end_exclusive } => CandidatePatternBindings::Range(
            packing.source_pattern_ids_before_at(candidate_index, end_exclusive),
        ),
        CoveragePatternSelection::Single(pattern_id) => CandidatePatternBindings::Single(
            packing
                .source_pattern_contains(candidate_index, pattern_id)
                .then_some(pattern_id),
        ),
    }
}

pub(crate) fn candidate_pattern_binding_count(
    packing: &PackingRunResult,
    candidate_index: usize,
    requested_patterns: CoveragePatternSelection,
) -> usize {
    match requested_patterns {
        CoveragePatternSelection::Range { end_exclusive } => {
            packing.source_pattern_count_before(candidate_index, end_exclusive)
        }
        CoveragePatternSelection::Single(pattern_id) => {
            usize::from(packing.source_pattern_contains(candidate_index, pattern_id))
        }
    }
}

pub(crate) fn buildup_enumeration_limits(
    problem: &SearchProblem,
) -> CNativeBuildUpEnumerationLimits {
    CNativeBuildUpEnumerationLimits {
        // Coverage needs only one witness plus the exhaustive count. A score
        // matrix needs every legal execution that the bounded native view can
        // materialize; if that view truncates, postprocess remains incomplete.
        max_variants: if score_matrix_requested(problem) || complete_trace_batch_requested(problem)
        {
            0
        } else {
            1
        },
        preserve_hold_branches: 1,
        prefer_highest_t_spin_trace: u8::from(score_matrix_requested(problem)),
        reserved: [0; 6],
    }
}

fn score_matrix_requested(problem: &SearchProblem) -> bool {
    problem.objective().score().requested()
}

fn complete_trace_batch_requested(problem: &SearchProblem) -> bool {
    problem
        .pc_chance_evidence_policy()
        .retains_pc_save_groups_v2_evidence()
}

pub(crate) fn buildup_witness_from_c_results<I>(
    problem: &SearchProblem,
    candidates: I,
    candidate_acceptance: &BuildUpCandidateAcceptance,
    execution_variants: &ExecutionVariantSet,
    accepted_candidate_count: usize,
    normalized_unique_solution_count: usize,
) -> ScenarioPackingWitness
where
    I: IntoIterator,
    I::Item: Borrow<CPackingCandidate>,
{
    let success_count = accepted_candidate_count;
    if success_count == 0 {
        return ScenarioPackingWitness::no_solution();
    }

    let first_success = candidate_acceptance
        .explicit_results()
        .and_then(|results| results.iter().position(|result| result.success != 0))
        .unwrap_or(0);
    let queue_consumed = candidates
        .into_iter()
        .nth(first_success)
        .map(|candidate| usize::from(candidate.borrow().operation_count))
        .unwrap_or_else(|| problem.piece_window().max_pieces());
    let cleared_lines = execution_variants
        .variants()
        .iter()
        .map(CBuildVariantView::cleared_lines)
        .max()
        .unwrap_or(0);

    ScenarioPackingWitness::solved_with_unique(
        cleared_lines,
        success_count,
        normalized_unique_solution_count,
        queue_consumed,
    )
}

pub(crate) fn candidates_for_build_variants(
    packing: &PackingRunResult,
    variants: &[CBuildVariantView],
) -> Vec<CPackingCandidate> {
    let mut required_ids = variants
        .iter()
        .map(CBuildVariantView::candidate_id)
        .collect::<HashSet<_>>();
    if required_ids.is_empty() {
        return Vec::new();
    }
    let mut candidates = Vec::with_capacity(required_ids.len());
    for candidate_index in 0..packing.candidate_count() {
        let Some(candidate_id) = packing.candidate_id_at(candidate_index) else {
            continue;
        };
        if !required_ids.remove(&candidate_id) {
            continue;
        }
        if let Some(candidate) = packing.candidate_at(candidate_index) {
            candidates.push(candidate);
        }
        if required_ids.is_empty() {
            break;
        }
    }
    candidates
}

pub(crate) fn accepted_build_variants_from_buffer(
    buffer: &CNativeBuildVariantBuffer,
) -> Result<Vec<CBuildVariantView>, BuildUpRunnerError> {
    let variant_count = usize::from(buffer.count).min(buffer.variants.len());
    buffer.variants[..variant_count]
        .iter()
        .map(CBuildVariantView::from_native)
        .collect::<Result<Vec<_>, _>>()
        .map_err(BuildUpRunnerError::NativeView)
}

fn complete_variant_count(
    status: i32,
    buffer: &CNativeBuildVariantBuffer,
    retained_variant_count: usize,
) -> Result<(usize, bool, &'static str), BuildUpRunnerError> {
    let total = usize::try_from(buffer.total_variant_count).map_err(|_| {
        BuildUpRunnerError::VariantCountOverflow {
            count: buffer.total_variant_count,
        }
    })?;
    if incomplete_buildup_status_reason(status).is_none() && buffer.count_complete != 0 {
        Ok((total, true, "none"))
    } else {
        Ok((
            total.max(retained_variant_count),
            false,
            incomplete_buildup_status_reason(status).unwrap_or("buildup_count_incomplete"),
        ))
    }
}

pub(crate) fn buildup_status_can_keep_partial_variants(status: i32) -> bool {
    matches!(
        status,
        C_BUILDUP_STATUS_OK
            | C_BUILDUP_STATUS_CAPACITY_EXCEEDED
            | C_BUILDUP_STATUS_KICK_EVIDENCE_BUFFER_EXHAUSTED
            | C_BUILDUP_STATUS_ENUMERATION_TRUNCATED
    )
}

pub(crate) fn incomplete_buildup_status_reason(status: i32) -> Option<&'static str> {
    match status {
        C_BUILDUP_STATUS_CAPACITY_EXCEEDED => Some("buildup_capacity_exceeded"),
        C_BUILDUP_STATUS_KICK_EVIDENCE_BUFFER_EXHAUSTED => Some("kick_evidence_buffer_exhausted"),
        C_BUILDUP_STATUS_ENUMERATION_TRUNCATED => Some("buildup_enumeration_truncated"),
        _ => None,
    }
}

pub(crate) fn buildup_status_is_fatal(status: i32) -> bool {
    matches!(
        status,
        C_BUILDUP_STATUS_INVALID_ARGUMENT
            | C_BUILDUP_STATUS_INVALID_PROBLEM
            | C_BUILDUP_STATUS_INVALID_ORDER
            | C_BUILDUP_STATUS_UNSUPPORTED_RUNTIME_SCOPE
    ) || (status != C_BUILDUP_STATUS_OK
        && !(C_BUILDUP_STATUS_LOGICAL_REJECT_MIN..=C_BUILDUP_STATUS_LOGICAL_REJECT_MAX)
            .contains(&status)
        && incomplete_buildup_status_reason(status).is_none())
}

pub(crate) fn retain_execution_variants(
    execution_variants: &mut ExecutionVariantSet,
    accepted_variants: Vec<CBuildVariantView>,
    retained_trace_limit: usize,
) {
    for variant in accepted_variants {
        if execution_variants.len() >= retained_trace_limit {
            break;
        }
        execution_variants.insert(variant);
    }
}
