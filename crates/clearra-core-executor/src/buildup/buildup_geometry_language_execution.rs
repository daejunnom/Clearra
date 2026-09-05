use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;
use clearra_core_ffi::{
    CBuildUpProblem, CBuildUpProblemTemplate, CBuildVariantView, NativeBuildUpWorkspace,
    NativeCoreError, PackingCandidateView,
};
use clearra_problem::SearchProblem;
use clearra_supply::PatternPiecePositionIndex;

use crate::{
    buildup::{
        buildup_coverage_bridge::{CoveragePatternSelection, PatternVerifiedCandidateCoverage},
        buildup_error::BuildUpRunnerError,
        buildup_geometry_language_evaluator::{
            GeometryHoldLanguageEvaluator, GeometryLanguageEvaluationError,
        },
        buildup_native_bridge::{
            accepted_build_variants_from_buffer, buildup_status_can_keep_partial_variants,
            buildup_status_is_fatal, candidate_pattern_binding_count, candidate_pattern_bindings,
            incomplete_buildup_status_reason,
        },
    },
    packing::packing_runner::PackingRunResult,
    performance::{ExecutorSearchStage, SearchStageSpan},
};

const MIN_LANGUAGE_INTERSECTION_PATTERNS: usize = 2;

pub(super) struct GeometryLanguageCandidateExecution {
    pub(super) success: bool,
    pub(super) cleared_lines: u8,
    pub(super) retained_variants: Vec<CBuildVariantView>,
    pub(super) coverage_verifications: Vec<PatternVerifiedCandidateCoverage>,
    pub(super) pattern_verified_execution_count: usize,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn try_execute_candidate(
    problem: &SearchProblem,
    packing: &PackingRunResult,
    candidate_index: usize,
    candidate: PackingCandidateView<'_>,
    buildup_template: &CBuildUpProblemTemplate,
    buildup_scratch: &mut CBuildUpProblem,
    coverage_pattern_selection: CoveragePatternSelection,
    materialized_variant_limit: usize,
    cancellation: &ExecutionCancellationToken,
    native_workspace: &mut NativeBuildUpWorkspace,
    evaluator: &mut GeometryHoldLanguageEvaluator,
    pattern_index: Option<&PatternPiecePositionIndex>,
) -> Result<Option<GeometryLanguageCandidateExecution>, BuildUpRunnerError> {
    if matches!(
        coverage_pattern_selection,
        CoveragePatternSelection::Single(_)
    ) {
        return Ok(None);
    }
    let Some(universe) = problem.piece_source().materialized_universe() else {
        return Ok(None);
    };
    let binding_count =
        candidate_pattern_binding_count(packing, candidate_index, coverage_pattern_selection);
    if binding_count < MIN_LANGUAGE_INTERSECTION_PATTERNS {
        return Ok(None);
    }

    let (first_source_pattern_id, first_coverage_pattern_id) =
        candidate_pattern_bindings(packing, candidate_index, coverage_pattern_selection)
            .next()
            .ok_or(BuildUpRunnerError::InvalidGeometryLanguage)?;
    buildup_template
        .configure_packing_candidate_view(
            buildup_scratch,
            candidate,
            first_source_pattern_id,
            first_coverage_pattern_id,
        )
        .map_err(BuildUpRunnerError::Ffi)?;
    let export_span = SearchStageSpan::begin(ExecutorSearchStage::BuildUpGeometryLanguageExport);
    let language = native_workspace
        .export_geometry_language_with_cancellation(buildup_scratch, cancellation)
        .map_err(map_native_error)?;
    export_span.finish(language.nodes().len() as u64);
    if !language.complete() {
        return Ok(None);
    }
    let canonical_operation_set_id = if candidate.canonical_operation_set_id() == 0 {
        candidate.candidate_id()
    } else {
        candidate.canonical_operation_set_id()
    };
    if language.candidate_id() != candidate.candidate_id()
        || language.canonical_operation_set_id() != canonical_operation_set_id
    {
        return Err(BuildUpRunnerError::GeometryLanguageIdentityMismatch {
            candidate_id: candidate.candidate_id(),
            language_candidate_id: language.candidate_id(),
        });
    }

    let pattern_index = pattern_index.ok_or(BuildUpRunnerError::InvalidGeometryLanguage)?;
    if pattern_index.global_pattern_count() != universe.pattern_count() {
        return Err(BuildUpRunnerError::InvalidGeometryLanguage);
    }
    let evaluation_span = SearchStageSpan::begin(ExecutorSearchStage::BuildUpLanguageIntersection);
    let evaluation = match evaluator.evaluate_pattern_words(
        &language,
        pattern_index,
        problem.initial_hold(),
        problem.supply().hold_enabled(),
        cancellation,
    ) {
        Ok(evaluation) => evaluation,
        Err(GeometryLanguageEvaluationError::StorageUnavailable) => {
            return Err(BuildUpRunnerError::PatternProductStorageUnavailable);
        }
        Err(GeometryLanguageEvaluationError::Cancelled) => {
            return Err(BuildUpRunnerError::ExecutionCancelled);
        }
        Err(GeometryLanguageEvaluationError::InvalidLanguage) => {
            return Err(BuildUpRunnerError::InvalidGeometryLanguage);
        }
    };
    evaluation_span.finish(pattern_index.word_count() as u64);

    let success = evaluation.covered();
    let pattern_verified_execution_count = evaluation.covered_pattern_count();
    let mut cleared_lines = if success {
        candidate.cleared_lines()
    } else {
        0
    };
    let mut retained_variants = Vec::new();
    let first_pattern = evaluation.coverage_bits().first_pattern();
    let coverage_verifications = success
        .then(|| {
            PatternVerifiedCandidateCoverage::exact_geometry_hold_product(
                candidate.candidate_id(),
                evaluation.into_coverage_bits(),
            )
        })
        .into_iter()
        .collect::<Vec<_>>();

    if materialized_variant_limit != 0 {
        if let Some(pattern_id) = first_pattern {
            let pattern_id = u32::try_from(pattern_id.index()).map_err(|_| {
                BuildUpRunnerError::CoveragePatternIdOutOfRange {
                    pattern_id: pattern_id.index(),
                    pattern_count: universe.pattern_count(),
                    source: "geometry-hold-language-intersection",
                }
            })?;
            buildup_template
                .configure_packing_candidate_view(
                    buildup_scratch,
                    candidate,
                    pattern_id,
                    pattern_id,
                )
                .map_err(BuildUpRunnerError::Ffi)?;
            let trace_span =
                SearchStageSpan::begin(ExecutorSearchStage::BuildUpTraceMaterialization);
            let trace_outcome = native_workspace
                .verify_first_buildup_problem_with_cancellation(buildup_scratch, cancellation)
                .map_err(map_native_error)?;
            trace_span.finish(1);
            if buildup_status_is_fatal(trace_outcome.status) {
                return Err(BuildUpRunnerError::Native(NativeCoreError::BuildUpStatus(
                    trace_outcome.status,
                )));
            }
            if buildup_status_can_keep_partial_variants(trace_outcome.status)
                && trace_outcome.buffer.count > 0
            {
                let variants = accepted_build_variants_from_buffer(trace_outcome.buffer)?;
                cleared_lines = cleared_lines.max(
                    variants
                        .iter()
                        .map(CBuildVariantView::cleared_lines)
                        .max()
                        .unwrap_or(0),
                );
                retained_variants.extend(variants.into_iter().take(materialized_variant_limit));
            } else if incomplete_buildup_status_reason(trace_outcome.status).is_none() {
                return Err(BuildUpRunnerError::GeometryLanguageTraceMismatch {
                    candidate_id: candidate.candidate_id(),
                    pattern_id,
                });
            }
        }
    }
    Ok(Some(GeometryLanguageCandidateExecution {
        success,
        cleared_lines,
        retained_variants,
        coverage_verifications,
        pattern_verified_execution_count,
    }))
}

fn map_native_error(error: NativeCoreError) -> BuildUpRunnerError {
    match error {
        NativeCoreError::ExecutionCancelled => BuildUpRunnerError::ExecutionCancelled,
        error => BuildUpRunnerError::Native(error),
    }
}
