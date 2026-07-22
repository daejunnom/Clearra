use clearra_core_ffi::CBuildVariantView;
use clearra_coverage::row::coverage_row::CoverageRow;
use clearra_objectives::reducer::objective_reducer::ObjectiveReductionResult;
use clearra_replay::ReplayTrace;

use crate::{
    buildup::{
        buildup_solution_set_contract::{BuildUpSolutionSetContract, ACTUAL_SOLUTION_SET_CONTRACT},
        BuildUpCandidateAcceptance, BuildUpExecutionMode,
    },
    core_execution_result::CorePathStep,
    core_postprocess_execution::CorePostProcessExecution,
    packing::scenario_packing_witness::ScenarioPackingWitness,
    solution_probability::{SolutionCoverage, SolutionProbabilityReport},
};

#[derive(Clone, Debug, PartialEq)]
pub struct BuildUpRunResult {
    candidate_acceptance: BuildUpCandidateAcceptance,
    build_variants: Vec<CBuildVariantView>,
    coverage_rows: Vec<CoverageRow>,
    objective_result: Option<ObjectiveReductionResult>,
    path_steps: Vec<CorePathStep>,
    sample_replay_trace: Option<ReplayTrace>,
    postprocess_executions: Vec<CorePostProcessExecution>,
    postprocess_execution_complete: bool,
    postprocess_pattern_weights: Vec<String>,
    trace_key: Option<String>,
    solution_found: bool,
    cleared_lines: u8,
    total_solution_count: usize,
    unique_solution_count: usize,
    retained_trace_count: usize,
    pattern_verified_execution_count: usize,
    unique_trace_count: usize,
    count_complete: bool,
    count_truncated_reason: &'static str,
    peak_workspace_bytes: usize,
    trace_retention_truncated: bool,
    trace_retention_reason: &'static str,
    queue_consumed: usize,
    placed_piece_count: usize,
    solution_set_contract: BuildUpSolutionSetContract,
    coverage_probability: String,
    execution_mode: BuildUpExecutionMode,
    coverage_source: &'static str,
    objective_complete: bool,
    objective_incomplete_reason: Option<&'static str>,
    solution_coverages: Vec<SolutionCoverage>,
    solution_probabilities: Vec<SolutionProbabilityReport>,
    solution_probability_complete: bool,
}

impl BuildUpRunResult {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        candidate_acceptance: BuildUpCandidateAcceptance,
        build_variants: Vec<CBuildVariantView>,
        coverage_rows: Vec<CoverageRow>,
        objective_result: Option<ObjectiveReductionResult>,
        path_steps: Vec<CorePathStep>,
        sample_replay_trace: Option<ReplayTrace>,
        postprocess_executions: Vec<CorePostProcessExecution>,
        postprocess_execution_complete: bool,
        postprocess_pattern_weights: Vec<String>,
        trace_key: Option<String>,
        witness: ScenarioPackingWitness,
        retained_trace_count: usize,
        pattern_verified_execution_count: usize,
        unique_trace_count: usize,
        count_complete: bool,
        count_truncated_reason: &'static str,
        peak_workspace_bytes: usize,
        trace_retention_truncated: bool,
        solution_set_contract: BuildUpSolutionSetContract,
        coverage_probability: String,
        execution_mode: BuildUpExecutionMode,
        coverage_source: &'static str,
        objective_complete: bool,
        objective_incomplete_reason: Option<&'static str>,
        solution_coverages: Vec<SolutionCoverage>,
        solution_probabilities: Vec<SolutionProbabilityReport>,
        solution_probability_complete: bool,
    ) -> Self {
        Self {
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
            solution_found: witness.solution_found,
            cleared_lines: witness.cleared_lines,
            total_solution_count: witness.total_solution_count,
            unique_solution_count: witness.unique_solution_count,
            retained_trace_count,
            pattern_verified_execution_count,
            unique_trace_count,
            count_complete,
            count_truncated_reason,
            peak_workspace_bytes,
            trace_retention_truncated,
            trace_retention_reason: if trace_retention_truncated {
                "retained_trace_limit"
            } else {
                "none"
            },
            queue_consumed: witness.queue_consumed,
            placed_piece_count: witness.placed_piece_count,
            solution_set_contract,
            coverage_probability,
            execution_mode,
            coverage_source,
            objective_complete,
            objective_incomplete_reason,
            solution_coverages,
            solution_probabilities,
            solution_probability_complete,
        }
    }
}
impl BuildUpRunResult {
    pub fn candidate_result_count(&self) -> usize {
        self.candidate_acceptance.len()
    }

    pub fn candidate_succeeded(&self, candidate_index: usize, candidate_id: u64) -> Option<bool> {
        self.candidate_acceptance
            .candidate_accepted(candidate_index, candidate_id)
    }
}
impl BuildUpRunResult {
    pub fn build_variants(&self) -> &[CBuildVariantView] {
        &self.build_variants
    }
}
impl BuildUpRunResult {
    pub fn execution_variants(&self) -> &[CBuildVariantView] {
        &self.build_variants
    }
}
impl BuildUpRunResult {
    pub fn coverage_rows(&self) -> &[CoverageRow] {
        &self.coverage_rows
    }
}
impl BuildUpRunResult {
    pub fn objective_result(&self) -> Option<&ObjectiveReductionResult> {
        self.objective_result.as_ref()
    }
}
impl BuildUpRunResult {
    pub fn path_steps(&self) -> &[CorePathStep] {
        &self.path_steps
    }
}
impl BuildUpRunResult {
    pub fn sample_replay_trace(&self) -> Option<&ReplayTrace> {
        self.sample_replay_trace.as_ref()
    }
}
impl BuildUpRunResult {
    pub fn postprocess_executions(&self) -> &[CorePostProcessExecution] {
        &self.postprocess_executions
    }

    pub fn postprocess_execution_complete(&self) -> bool {
        self.postprocess_execution_complete
    }

    pub fn postprocess_pattern_weights(&self) -> &[String] {
        &self.postprocess_pattern_weights
    }
}
impl BuildUpRunResult {
    pub fn trace_key(&self) -> Option<&str> {
        self.trace_key.as_deref()
    }
}
impl BuildUpRunResult {
    pub fn trace_key_source(&self) -> &'static str {
        if self.trace_key.is_none() {
            "none"
        } else {
            "native-c-core"
        }
    }
}
impl BuildUpRunResult {
    pub fn execution_source(&self) -> &'static str {
        "native-cpu-buildup"
    }
}
impl BuildUpRunResult {
    pub fn buildup_backend(&self) -> &'static str {
        "cpu-buildup"
    }
}
impl BuildUpRunResult {
    pub fn solution_found(&self) -> bool {
        self.solution_found
    }
}
impl BuildUpRunResult {
    pub fn cleared_lines(&self) -> u8 {
        self.cleared_lines
    }
}
impl BuildUpRunResult {
    pub fn total_solution_count(&self) -> usize {
        self.total_solution_count
    }
}
impl BuildUpRunResult {
    pub fn unique_solution_count(&self) -> usize {
        self.unique_solution_count
    }
}
impl BuildUpRunResult {
    pub fn retained_trace_count(&self) -> usize {
        self.retained_trace_count
    }
}
impl BuildUpRunResult {
    pub const fn pattern_verified_execution_count(&self) -> usize {
        self.pattern_verified_execution_count
    }
}
impl BuildUpRunResult {
    pub const fn unique_trace_count(&self) -> usize {
        self.unique_trace_count
    }
}
impl BuildUpRunResult {
    pub fn count_complete(&self) -> bool {
        self.count_complete
    }
}
impl BuildUpRunResult {
    pub fn count_truncated_reason(&self) -> &'static str {
        self.count_truncated_reason
    }
}
impl BuildUpRunResult {
    pub const fn peak_workspace_bytes(&self) -> usize {
        self.peak_workspace_bytes
    }
}
impl BuildUpRunResult {
    pub fn trace_retention_truncated(&self) -> bool {
        self.trace_retention_truncated
    }
}
impl BuildUpRunResult {
    pub fn trace_retention_reason(&self) -> &'static str {
        self.trace_retention_reason
    }
}
impl BuildUpRunResult {
    pub fn queue_consumed(&self) -> usize {
        self.queue_consumed
    }
}
impl BuildUpRunResult {
    pub fn placed_piece_count(&self) -> usize {
        self.placed_piece_count
    }
}
impl BuildUpRunResult {
    pub fn actual_solution_set_contract(&self) -> &'static str {
        ACTUAL_SOLUTION_SET_CONTRACT
    }
}
impl BuildUpRunResult {
    pub fn normalized_solution_key_algorithm(&self) -> &'static str {
        self.solution_set_contract.key_algorithm()
    }
}
impl BuildUpRunResult {
    pub fn normalized_solution_set_hash_algorithm(&self) -> &'static str {
        self.solution_set_contract.hash_algorithm()
    }
}
impl BuildUpRunResult {
    pub fn normalized_unique_solution_count(&self) -> usize {
        self.solution_set_contract.unique_solution_count()
    }
}
impl BuildUpRunResult {
    pub fn normalized_solution_set_hash(&self) -> &str {
        self.solution_set_contract.solution_set_hash()
    }
}
impl BuildUpRunResult {
    pub fn normalized_solution_keys(&self) -> Vec<String> {
        self.solution_set_contract.keys()
    }
}
impl BuildUpRunResult {
    pub fn coverage_probability(&self) -> &str {
        &self.coverage_probability
    }
}
impl BuildUpRunResult {
    pub fn coverage_row_count(&self) -> usize {
        self.coverage_rows.len()
    }
}
impl BuildUpRunResult {
    pub fn covered_pattern_count(&self) -> usize {
        self.objective_result
            .as_ref()
            .map(|result| result.coverage().covered_patterns().count_ones() as usize)
            .unwrap_or(0)
    }
}
impl BuildUpRunResult {
    pub fn build_variant_count(&self) -> usize {
        self.build_variants.len()
    }
}
impl BuildUpRunResult {
    pub fn execution_mode(&self) -> BuildUpExecutionMode {
        self.execution_mode
    }
}
impl BuildUpRunResult {
    pub fn coverage_source(&self) -> &'static str {
        self.coverage_source
    }
}
impl BuildUpRunResult {
    pub const fn coverage_complete(&self) -> bool {
        self.execution_mode.can_source_coverage() && self.count_complete
    }
}
impl BuildUpRunResult {
    pub const fn objective_complete(&self) -> bool {
        self.objective_complete
    }
}
impl BuildUpRunResult {
    pub const fn objective_incomplete_reason(&self) -> Option<&'static str> {
        self.objective_incomplete_reason
    }

    pub fn solution_coverages(&self) -> &[SolutionCoverage] {
        &self.solution_coverages
    }

    pub fn solution_probabilities(&self) -> &[SolutionProbabilityReport] {
        &self.solution_probabilities
    }

    pub const fn solution_probability_complete(&self) -> bool {
        self.solution_probability_complete
    }
}
