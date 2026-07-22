use std::sync::atomic::{AtomicUsize, Ordering};

use clearra_core_domain::execution_cancellation::{ExecutionCancellationToken, ExecutionControl};
use clearra_core_ffi::{
    CBuildUpProblem, CBuildUpProblemTemplate, CBuildUpResult, CBuildVariantView,
    NativeBuildUpWorkspace, NativeCoreError, PackingCandidateView,
};
use clearra_problem::SearchProblem;

use crate::{
    buildup::{
        buildup_candidate_acceptance::BuildUpCandidateAcceptance,
        buildup_coverage_bridge::{
            coverage_pattern_selection_for_problem, pattern_count, CoveragePatternSelection,
        },
        buildup_error::BuildUpRunnerError,
        buildup_native_bridge::{
            accepted_build_variants_from_buffer, buildup_scratch_with_catalog,
            buildup_status_can_keep_partial_variants, buildup_status_is_fatal,
            candidate_pattern_bindings, incomplete_buildup_status_reason,
            uses_standard_bag_automaton, CBuildUpExecution,
        },
        buildup_parallelism, BuildUpExecutionMode, ExecutionVariantSet,
    },
    packing::PackingRunResult,
    performance::{ExecutorSearchStage, SearchStageSpan},
};

const CANDIDATES_PER_CHUNK: usize = 256;

pub(crate) fn c_buildup_unique_solution_results(
    problem: &SearchProblem,
    packing: &PackingRunResult,
    cancellation: &ExecutionCancellationToken,
    control: &ExecutionControl,
) -> Result<CBuildUpExecution, BuildUpRunnerError> {
    let candidate_count = packing.candidate_count();
    let requested_patterns =
        coverage_pattern_selection_for_problem(problem, pattern_count(problem))?;
    let buildup_template =
        CBuildUpProblemTemplate::compile(problem).map_err(BuildUpRunnerError::Ffi)?;
    let candidate_batch = execute_candidates(
        problem,
        packing,
        &buildup_template,
        requested_patterns,
        cancellation,
        control,
    )?;
    let mut execution_variants = ExecutionVariantSet::default();
    for variant in candidate_batch.variants {
        execution_variants.insert(variant);
    }

    control.report_progress(
        "buildup-unique",
        candidate_count as u64,
        Some(candidate_count as u64),
    );
    Ok(CBuildUpExecution {
        candidate_acceptance: if packing.buildability_preverified() {
            BuildUpCandidateAcceptance::all_packing_candidates(candidate_count)
        } else {
            BuildUpCandidateAcceptance::explicit(candidate_batch.results)
        },
        execution_variants,
        coverage_verifications: Vec::new(),
        pattern_verified_execution_count: 0,
        total_build_variant_count: None,
        execution_mode: BuildUpExecutionMode::VerifyFirst,
        coverage_source: "not-produced-count-unique",
        count_complete: candidate_batch.count_complete,
        count_truncated_reason: candidate_batch.count_truncated_reason,
        peak_workspace_bytes: candidate_batch.peak_workspace_bytes,
    })
}

struct UniqueCandidateExecution {
    result: CBuildUpResult,
    variant: Option<CBuildVariantView>,
    count_complete: bool,
    count_truncated_reason: &'static str,
}

struct UniqueCandidateBatch {
    results: Vec<CBuildUpResult>,
    variants: Vec<CBuildVariantView>,
    count_complete: bool,
    count_truncated_reason: &'static str,
    peak_workspace_bytes: usize,
}

struct UniqueWorkerChunk {
    start_index: usize,
    results: Vec<CBuildUpResult>,
    variants: Vec<CBuildVariantView>,
}

struct UniqueWorkerBatch {
    chunks: Vec<UniqueWorkerChunk>,
    count_complete: bool,
    count_truncated_reason: &'static str,
    workspace_bytes: usize,
}

fn execute_candidates(
    problem: &SearchProblem,
    packing: &PackingRunResult,
    buildup_template: &CBuildUpProblemTemplate,
    requested_patterns: CoveragePatternSelection,
    cancellation: &ExecutionCancellationToken,
    control: &ExecutionControl,
) -> Result<UniqueCandidateBatch, BuildUpRunnerError> {
    let candidate_count = if packing.buildability_preverified() {
        packing
            .candidate_count()
            .min(problem.trace_policy().retained_trace_limit())
    } else {
        packing.candidate_count()
    };
    let worker_count = buildup_parallelism::worker_count(problem, candidate_count);
    if worker_count <= 1 {
        let mut workspace = NativeBuildUpWorkspace::new();
        let mut buildup_scratch = buildup_scratch_with_catalog(buildup_template, packing);
        let mut results = Vec::with_capacity(candidate_count);
        let mut variants = Vec::new();
        let mut count_complete = true;
        let mut count_truncated_reason = "none";
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
                &mut buildup_scratch,
                requested_patterns,
                cancellation,
                &mut workspace,
            )?;
            results.push(execution.result);
            if let Some(variant) = execution.variant {
                variants.push(variant);
            }
            if !execution.count_complete {
                count_complete = false;
                count_truncated_reason = execution.count_truncated_reason;
            }
            if results.len() % CANDIDATES_PER_CHUNK == 0 || results.len() == candidate_count {
                control.report_progress(
                    "buildup-unique",
                    results.len() as u64,
                    Some(candidate_count as u64),
                );
            }
        }
        return Ok(UniqueCandidateBatch {
            results,
            variants,
            count_complete,
            count_truncated_reason,
            peak_workspace_bytes: workspace.retained_bytes(),
        });
    }

    let completed = AtomicUsize::new(0);
    let next_candidate = AtomicUsize::new(0);
    let worker_batches = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let completed = &completed;
            let next_candidate = &next_candidate;
            handles.push(scope.spawn(move || {
                let mut workspace = NativeBuildUpWorkspace::new();
                let mut buildup_scratch = buildup_scratch_with_catalog(buildup_template, packing);
                let mut chunks = Vec::new();
                let mut count_complete = true;
                let mut count_truncated_reason = "none";
                loop {
                    let start_index =
                        next_candidate.fetch_add(CANDIDATES_PER_CHUNK, Ordering::Relaxed);
                    if start_index >= candidate_count {
                        break;
                    }
                    let end_index = start_index
                        .saturating_add(CANDIDATES_PER_CHUNK)
                        .min(candidate_count);
                    let mut results = Vec::with_capacity(end_index - start_index);
                    let mut variants = Vec::new();
                    for candidate_index in start_index..end_index {
                        let candidate = packing.candidate_view_at(candidate_index).ok_or(
                            BuildUpRunnerError::PackingCandidateUnavailable { candidate_index },
                        )?;
                        let execution = execute_candidate(
                            problem,
                            packing,
                            candidate_index,
                            candidate,
                            buildup_template,
                            &mut buildup_scratch,
                            requested_patterns,
                            cancellation,
                            &mut workspace,
                        )?;
                        results.push(execution.result);
                        if let Some(variant) = execution.variant {
                            variants.push(variant);
                        }
                        if !execution.count_complete {
                            count_complete = false;
                            count_truncated_reason = execution.count_truncated_reason;
                        }
                    }
                    let progress =
                        completed.fetch_add(results.len(), Ordering::Relaxed) + results.len();
                    control.report_progress(
                        "buildup-unique",
                        progress as u64,
                        Some(candidate_count as u64),
                    );
                    chunks.push(UniqueWorkerChunk {
                        start_index,
                        results,
                        variants,
                    });
                }
                Ok::<_, BuildUpRunnerError>(UniqueWorkerBatch {
                    chunks,
                    count_complete,
                    count_truncated_reason,
                    workspace_bytes: workspace.retained_bytes(),
                })
            }));
        }

        let mut batches = Vec::with_capacity(worker_count);
        for handle in handles {
            batches.push(
                handle
                    .join()
                    .map_err(|_| BuildUpRunnerError::ParallelWorkerPanicked)??,
            );
        }
        Ok::<_, BuildUpRunnerError>(batches)
    })?;

    let mut chunks = Vec::new();
    let mut results = Vec::with_capacity(candidate_count);
    let mut variants = Vec::new();
    let mut count_complete = true;
    let mut count_truncated_reason = "none";
    let mut peak_workspace_bytes = 0usize;
    for batch in worker_batches {
        chunks.extend(batch.chunks);
        if !batch.count_complete {
            count_complete = false;
            count_truncated_reason = batch.count_truncated_reason;
        }
        peak_workspace_bytes = peak_workspace_bytes.saturating_add(batch.workspace_bytes);
    }
    chunks.sort_unstable_by_key(|chunk| chunk.start_index);
    for chunk in chunks {
        results.extend(chunk.results);
        variants.extend(chunk.variants);
    }
    Ok(UniqueCandidateBatch {
        results,
        variants,
        count_complete,
        count_truncated_reason,
        peak_workspace_bytes,
    })
}

fn execute_candidate(
    problem: &SearchProblem,
    packing: &PackingRunResult,
    candidate_index: usize,
    candidate: PackingCandidateView<'_>,
    buildup_template: &CBuildUpProblemTemplate,
    buildup_scratch: &mut CBuildUpProblem,
    requested_patterns: CoveragePatternSelection,
    cancellation: &ExecutionCancellationToken,
    native_workspace: &mut NativeBuildUpWorkspace,
) -> Result<UniqueCandidateExecution, BuildUpRunnerError> {
    ensure_not_cancelled(cancellation)?;
    if packing.buildability_preverified()
        && candidate_index >= problem.trace_policy().retained_trace_limit()
    {
        return Ok(preverified_candidate_without_trace(candidate));
    }
    let verification = if uses_standard_bag_automaton(problem) {
        let lowering_span = SearchStageSpan::begin(ExecutorSearchStage::BuildUpProblemLowering);
        buildup_template
            .configure_packing_candidate_view_with_standard_bag_automaton(
                buildup_scratch,
                candidate,
            )
            .map_err(BuildUpRunnerError::Ffi)?;
        lowering_span.finish(1);
        verify_candidate_problem(buildup_scratch, native_workspace, cancellation)?
    } else {
        verify_candidate_patterns(
            packing,
            candidate_index,
            candidate,
            buildup_template,
            buildup_scratch,
            requested_patterns,
            native_workspace,
            cancellation,
        )?
    };
    let (variant, count_complete, count_truncated_reason) = match verification {
        CandidateVerification::Accepted(variant) => (Some(variant), true, "none"),
        CandidateVerification::Rejected if packing.buildability_preverified() => {
            return Err(BuildUpRunnerError::PreverifiedBuildabilityMismatch {
                candidate_id: candidate.candidate_id(),
            });
        }
        CandidateVerification::Incomplete(_) if packing.buildability_preverified() => {
            return Err(BuildUpRunnerError::PreverifiedBuildabilityMismatch {
                candidate_id: candidate.candidate_id(),
            });
        }
        CandidateVerification::Rejected => (None, true, "none"),
        CandidateVerification::Incomplete(reason) => (None, false, reason),
    };
    Ok(UniqueCandidateExecution {
        result: CBuildUpResult {
            candidate_id: candidate.candidate_id(),
            success: variant.is_some() as u8,
            cleared_lines: variant.as_ref().map_or(0, CBuildVariantView::cleared_lines),
            reserved: 0,
        },
        variant,
        count_complete,
        count_truncated_reason,
    })
}

fn preverified_candidate_without_trace(
    candidate: PackingCandidateView<'_>,
) -> UniqueCandidateExecution {
    UniqueCandidateExecution {
        result: CBuildUpResult {
            candidate_id: candidate.candidate_id(),
            success: 1,
            cleared_lines: candidate.cleared_lines(),
            reserved: 0,
        },
        variant: None,
        count_complete: true,
        count_truncated_reason: "none",
    }
}

enum CandidateVerification {
    Accepted(CBuildVariantView),
    Rejected,
    Incomplete(&'static str),
}

fn verify_candidate_patterns(
    packing: &PackingRunResult,
    candidate_index: usize,
    candidate: PackingCandidateView<'_>,
    buildup_template: &CBuildUpProblemTemplate,
    buildup_scratch: &mut CBuildUpProblem,
    requested_patterns: CoveragePatternSelection,
    native_workspace: &mut NativeBuildUpWorkspace,
    cancellation: &ExecutionCancellationToken,
) -> Result<CandidateVerification, BuildUpRunnerError> {
    let mut incomplete_reason = None;
    for (piece_source_pattern_id, coverage_pattern_id) in
        candidate_pattern_bindings(packing, candidate_index, requested_patterns)
    {
        ensure_not_cancelled(cancellation)?;
        buildup_template
            .configure_packing_candidate_view(
                buildup_scratch,
                candidate,
                piece_source_pattern_id,
                coverage_pattern_id,
            )
            .map_err(BuildUpRunnerError::Ffi)?;
        match verify_candidate_problem(buildup_scratch, native_workspace, cancellation)? {
            accepted @ CandidateVerification::Accepted(_) => return Ok(accepted),
            CandidateVerification::Incomplete(reason) => incomplete_reason = Some(reason),
            CandidateVerification::Rejected => {}
        }
    }
    Ok(incomplete_reason.map_or(
        CandidateVerification::Rejected,
        CandidateVerification::Incomplete,
    ))
}

fn verify_candidate_problem(
    buildup: &CBuildUpProblem,
    native_workspace: &mut NativeBuildUpWorkspace,
    cancellation: &ExecutionCancellationToken,
) -> Result<CandidateVerification, BuildUpRunnerError> {
    let native_call_span = SearchStageSpan::begin(ExecutorSearchStage::BuildUpNativeCall);
    let native_outcome =
        native_workspace.verify_first_buildup_problem_with_cancellation(buildup, cancellation);
    native_call_span.finish(1);
    let outcome = match native_outcome {
        Ok(outcome) => outcome,
        Err(NativeCoreError::Unavailable) => {
            return Err(BuildUpRunnerError::Native(NativeCoreError::Unavailable));
        }
        Err(NativeCoreError::ExecutionCancelled) => {
            return Err(BuildUpRunnerError::ExecutionCancelled);
        }
        Err(error) => return Err(BuildUpRunnerError::Native(error)),
    };
    if buildup_status_is_fatal(outcome.status) {
        return Err(BuildUpRunnerError::Native(NativeCoreError::BuildUpStatus(
            outcome.status,
        )));
    }
    if buildup_status_can_keep_partial_variants(outcome.status) && outcome.buffer.count > 0 {
        let variant_copy_span = SearchStageSpan::begin(ExecutorSearchStage::BuildUpVariantCopy);
        let variant = accepted_build_variants_from_buffer(outcome.buffer)?
            .into_iter()
            .next()
            .ok_or(BuildUpRunnerError::Native(NativeCoreError::BuildUpStatus(
                outcome.status,
            )))?;
        variant_copy_span.finish(1);
        return Ok(CandidateVerification::Accepted(variant));
    }
    Ok(incomplete_buildup_status_reason(outcome.status).map_or(
        CandidateVerification::Rejected,
        CandidateVerification::Incomplete,
    ))
}

fn ensure_not_cancelled(
    cancellation: &ExecutionCancellationToken,
) -> Result<(), BuildUpRunnerError> {
    if cancellation.is_cancelled() {
        Err(BuildUpRunnerError::ExecutionCancelled)
    } else {
        Ok(())
    }
}
