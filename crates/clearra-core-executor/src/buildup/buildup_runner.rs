use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    resource::ResourceTruncationReason,
};
use clearra_problem::SearchProblem;

use crate::{
    buildup::{
        buildup_coverage_bridge::{
            coverage_rows_from_pattern_verifications_with_cancellation, coverage_universe_identity,
            pattern_count,
        },
        buildup_error::BuildUpRunnerError,
        buildup_native_bridge::{
            buildup_witness_from_c_results, c_buildup_results, candidates_for_build_variants,
        },
        buildup_objective_bridge::reduce_coverage_rows_for_policy,
        buildup_replay_bridge::{
            postprocess_executions_from_build_variants, trace_material_for_execution,
            BuildUpPostProcessBatch, BuildUpTraceMaterial,
        },
        buildup_run_result::BuildUpRunResult,
        buildup_solution_probability::{
            collect_solution_probabilities, BuildUpSolutionProbabilityBatch,
        },
        buildup_solution_set_contract::BuildUpSolutionSetContract,
        buildup_trace_retention::format_probability,
        buildup_unique_solution_search::c_buildup_unique_solution_results,
        objective_incomplete_reason::ObjectiveIncompleteReason,
        objective_reduction_outcome::ObjectiveReductionOutcome,
    },
    packing::PackingRunResult,
    performance::{ExecutorSearchStage, SearchStageSpan},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BuildUpRunner;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum BuildUpExecutionRequest {
    #[default]
    QueryDefault,
    CoverageSummary,
}

impl BuildUpExecutionRequest {
    const fn requires_coverage(self) -> bool {
        matches!(self, Self::CoverageSummary)
    }
}

impl BuildUpRunner {
    pub fn run(
        problem: &SearchProblem,
        packing: &PackingRunResult,
    ) -> Result<BuildUpRunResult, BuildUpRunnerError> {
        Self::run_with_cancellation(problem, packing, &ExecutionCancellationToken::new())
    }

    pub fn run_with_cancellation(
        problem: &SearchProblem,
        packing: &PackingRunResult,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<BuildUpRunResult, BuildUpRunnerError> {
        Self::run_with_control(
            problem,
            packing,
            &ExecutionControl::new(cancellation.clone()),
        )
    }

    pub fn run_for_coverage(
        problem: &SearchProblem,
        packing: &PackingRunResult,
    ) -> Result<BuildUpRunResult, BuildUpRunnerError> {
        Self::run_for_coverage_with_control(
            problem,
            packing,
            &ExecutionControl::new(ExecutionCancellationToken::new()),
        )
    }

    pub fn run_with_control(
        problem: &SearchProblem,
        packing: &PackingRunResult,
        control: &ExecutionControl,
    ) -> Result<BuildUpRunResult, BuildUpRunnerError> {
        Self::run_with_control_for_request(
            problem,
            packing,
            control,
            BuildUpExecutionRequest::QueryDefault,
        )
    }

    pub fn run_for_coverage_with_control(
        problem: &SearchProblem,
        packing: &PackingRunResult,
        control: &ExecutionControl,
    ) -> Result<BuildUpRunResult, BuildUpRunnerError> {
        Self::run_with_control_for_request(
            problem,
            packing,
            control,
            BuildUpExecutionRequest::CoverageSummary,
        )
    }

    fn run_with_control_for_request(
        problem: &SearchProblem,
        packing: &PackingRunResult,
        control: &ExecutionControl,
        request: BuildUpExecutionRequest,
    ) -> Result<BuildUpRunResult, BuildUpRunnerError> {
        control.report_progress("buildup", 0, Some(packing.candidate_count() as u64));
        ensure_not_cancelled(control)?;
        let native_execution_span =
            SearchStageSpan::begin(ExecutorSearchStage::BuildUpNativeExecution);
        let c_execution = if !request.requires_coverage()
            && problem.count_policy() == clearra_pc_graph::request::PcCountPolicy::CountUnique
            && !problem.solution_probability_policy().requested()
        {
            c_buildup_unique_solution_results(problem, packing, &control.cancellation, control)?
        } else {
            c_buildup_results(problem, packing, &control.cancellation, control)?
        };
        native_execution_span.finish(c_execution.pattern_verified_execution_count as u64);
        ensure_not_cancelled(control)?;
        let solution_set_span = SearchStageSpan::begin(ExecutorSearchStage::BuildUpSolutionSet);
        let solution_set_summary = BuildUpSolutionSetContract::from_packing_results(
            problem,
            packing,
            &c_execution.candidate_acceptance,
            &control.cancellation,
        )?;
        solution_set_span.finish(solution_set_summary.accepted_candidate_count as u64);
        let witness_span = SearchStageSpan::begin(ExecutorSearchStage::BuildUpWitness);
        let witness = buildup_witness_from_c_results(
            problem,
            packing.candidates(),
            &c_execution.candidate_acceptance,
            &c_execution.execution_variants,
            solution_set_summary.accepted_candidate_count,
            solution_set_summary.contract.unique_solution_count(),
        );
        witness_span.finish(solution_set_summary.accepted_candidate_count as u64);
        let solution_set_contract = solution_set_summary.contract;
        let candidate_acceptance = c_execution.candidate_acceptance;
        let pattern_verified_execution_count = c_execution.pattern_verified_execution_count;
        let total_build_variant_count = c_execution.total_build_variant_count;
        let unique_trace_count = c_execution.execution_variants.unique_trace_count();
        let build_variants = c_execution.execution_variants.into_variants();
        let accepted_span = SearchStageSpan::begin(ExecutorSearchStage::BuildUpAcceptedCandidates);
        let retained_candidates = candidates_for_build_variants(packing, &build_variants);
        accepted_span.finish(retained_candidates.len() as u64);
        let trace_span = SearchStageSpan::begin(ExecutorSearchStage::BuildUpTraceMaterial);
        let trace_material =
            trace_material_for_execution(problem, &retained_candidates, &build_variants, witness);
        trace_span.finish(build_variants.len() as u64);
        let trace_retention_truncated = trace_material.retained_trace_count
            < witness.total_solution_count
            && witness.solution_found;
        let BuildUpTraceMaterial {
            path_steps,
            sample_replay_trace,
            trace_key,
            retained_trace_count,
        } = trace_material;
        let identity = coverage_universe_identity(problem);
        let objective_pattern_count = pattern_count(problem);
        let coverage_span = SearchStageSpan::begin(ExecutorSearchStage::BuildUpCoverageRows);
        let coverage_rows = if c_execution.execution_mode.can_source_coverage() {
            coverage_rows_from_pattern_verifications_with_cancellation(
                c_execution.execution_mode,
                &c_execution.coverage_verifications,
                objective_pattern_count,
                identity,
                &control.cancellation,
            )?
        } else {
            Vec::new()
        };
        coverage_span.finish(coverage_rows.len() as u64);
        ensure_not_cancelled(control)?;
        control.report_progress(
            "coverage",
            coverage_rows.len() as u64,
            Some(coverage_rows.len() as u64),
        );
        let count_complete = packing.count_complete() && c_execution.count_complete;
        let materialized_coverage_complete = c_execution.execution_mode.can_source_coverage()
            && c_execution.count_complete
            && (packing.count_complete()
                || packing.truncation_reason()
                    == Some(ResourceTruncationReason::ObservedUniverseTruncated));
        let count_truncated_reason = packing
            .truncation_reason()
            .map(|reason| reason.as_str())
            .unwrap_or(c_execution.count_truncated_reason);
        let postprocess_span = SearchStageSpan::begin(ExecutorSearchStage::BuildUpPostProcess);
        let BuildUpPostProcessBatch {
            executions: postprocess_executions,
            all_variants_materialized,
        } = postprocess_executions_from_build_variants(
            problem,
            &retained_candidates,
            &build_variants,
        );
        postprocess_span.finish(postprocess_executions.len() as u64);
        let weights_span = SearchStageSpan::begin(ExecutorSearchStage::BuildUpPatternWeights);
        let postprocess_pattern_weights = if postprocess_executions.is_empty() {
            Vec::new()
        } else {
            problem
                .piece_source()
                .materialized_pattern_weights()
                .filter(|weights| weights.len() == objective_pattern_count)
                .map(|weights| {
                    (0..weights.len())
                        .filter_map(|index| {
                            weights
                                .weight(clearra_coverage::pattern::pattern_id::PatternId::new(
                                    index,
                                ))
                                // PostProcess reconstructs the exact source weight model.
                                // Display rounding here can make a complete model sum above
                                // one (for example 42 uniform patterns), so retain the
                                // round-trippable f64 representation.
                                .map(|weight| weight.get().to_string())
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        weights_span.finish(postprocess_pattern_weights.len() as u64);
        let postprocess_execution_complete = count_complete
            && problem.piece_source().complete()
            && all_variants_materialized
            && total_build_variant_count.is_some_and(|total| postprocess_executions.len() == total)
            && postprocess_pattern_weights.len() == objective_pattern_count;
        let objective_span = SearchStageSpan::begin(ExecutorSearchStage::BuildUpObjective);
        let objective_outcome = if c_execution.execution_mode.can_source_coverage() {
            reduce_coverage_rows_for_policy(
                problem.piece_source(),
                &coverage_rows,
                objective_pattern_count,
                identity,
                witness,
                retained_trace_count,
                count_complete,
                problem.objective().kind(),
            )?
        } else {
            ObjectiveReductionOutcome::incomplete(
                ObjectiveIncompleteReason::CoverageNotRequestedForUniqueSolutionSet,
            )
        };
        objective_span.finish(coverage_rows.len() as u64);
        ensure_not_cancelled(control)?;
        let (objective_result, objective_inputs_complete, objective_incomplete_reason) =
            objective_outcome.into_parts();
        let objective_complete =
            objective_inputs_complete && count_complete && problem.piece_source().complete();
        let objective_incomplete_reason = objective_incomplete_reason
            .map(|reason| reason.as_str())
            .or_else(|| (!problem.piece_source().complete()).then_some("piece_source_incomplete"));
        let coverage_probability = objective_result
            .as_ref()
            .map(|result| format_probability(result.coverage().probability().get()))
            .unwrap_or_else(|| "0.0".to_owned());
        let BuildUpSolutionProbabilityBatch {
            coverage: solution_coverages,
            reports: solution_probabilities,
            complete: solution_probability_complete,
        } = collect_solution_probabilities(
            problem,
            packing,
            &coverage_rows,
            &solution_set_contract,
            c_execution.execution_mode.can_source_coverage() && count_complete,
        )?;
        let result_span = SearchStageSpan::begin(ExecutorSearchStage::BuildUpResultAssembly);
        let result = BuildUpRunResult::new(
            candidate_acceptance,
            build_variants,
            coverage_rows,
            objective_result,
            path_steps,
            sample_replay_trace,
            postprocess_executions,
            postprocess_execution_complete,
            postprocess_pattern_weights,
            trace_key,
            witness,
            retained_trace_count,
            pattern_verified_execution_count,
            unique_trace_count,
            count_complete,
            count_truncated_reason,
            c_execution.peak_workspace_bytes,
            trace_retention_truncated,
            solution_set_contract,
            coverage_probability,
            c_execution.execution_mode,
            c_execution.coverage_source,
            materialized_coverage_complete,
            objective_complete,
            objective_incomplete_reason,
            solution_coverages,
            solution_probabilities,
            solution_probability_complete,
        );
        result_span.finish(1);
        control.report_progress(
            "buildup",
            packing.candidate_count() as u64,
            Some(packing.candidate_count() as u64),
        );
        Ok(result)
    }
}

fn ensure_not_cancelled(control: &ExecutionControl) -> Result<(), BuildUpRunnerError> {
    if control.is_cancelled() {
        Err(BuildUpRunnerError::ExecutionCancelled)
    } else {
        Ok(())
    }
}
