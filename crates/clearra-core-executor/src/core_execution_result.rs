use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity;
use clearra_replay::{
    ExactScoringExecutionBatch, ReplayTrace as PostProcessReplayTrace, SpinCoverageExecutionBatch,
};

use crate::{
    core_postprocess_execution::CorePostProcessExecution,
    core_postprocess_score_cell::CorePostProcessScoreCell,
    core_postprocess_spin_coverage::CorePostProcessSpinCoverage,
    result_views::SearchExecutionReport,
    setup_finder_report::SetupFinderReport,
    solution_probability::{
        NormalizedSolutionCoverage, SolutionCoverage, SolutionProbabilityReport,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorePathStep {
    piece: PieceKind,
    rotation: u8,
    x: i32,
    y: i32,
    hold: &'static str,
    cleared_lines: u8,
}

impl CorePathStep {
    pub fn new(
        piece: PieceKind,
        rotation: u8,
        x: i32,
        y: i32,
        hold: &'static str,
        cleared_lines: u8,
    ) -> Self {
        Self {
            piece,
            rotation,
            x,
            y,
            hold,
            cleared_lines,
        }
    }
}
impl CorePathStep {
    pub fn piece(&self) -> PieceKind {
        self.piece
    }
}
impl CorePathStep {
    pub fn rotation(&self) -> u8 {
        self.rotation
    }
}
impl CorePathStep {
    pub fn x(&self) -> i32 {
        self.x
    }
}
impl CorePathStep {
    pub fn y(&self) -> i32 {
        self.y
    }
}
impl CorePathStep {
    pub fn hold(&self) -> &'static str {
        self.hold
    }
}
impl CorePathStep {
    pub fn cleared_lines(&self) -> u8 {
        self.cleared_lines
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CoreExecutionResult {
    fields: Vec<(String, String)>,
    execution_report: SearchExecutionReport,
    postprocess_replay_trace: Option<PostProcessReplayTrace>,
    postprocess_executions: Vec<CorePostProcessExecution>,
    postprocess_execution_complete: bool,
    postprocess_pattern_weights: Vec<String>,
    packing_candidate_keys: Vec<String>,
    normalized_solution_keys: Vec<String>,
    normalized_solution_identities: Vec<StandardBoard64TilingIdentity>,
    representative_solution_identity: Option<StandardBoard64TilingIdentity>,
    coverage_pattern_words: Vec<u64>,
    solution_coverages: Vec<SolutionCoverage>,
    normalized_solution_coverages: Vec<NormalizedSolutionCoverage>,
    solution_probabilities: Vec<SolutionProbabilityReport>,
    exact_scoring_execution_batches: Vec<ExactScoringExecutionBatch>,
    spin_coverage_execution_batches: Vec<SpinCoverageExecutionBatch>,
    postprocess_score_cells: Vec<CorePostProcessScoreCell>,
    postprocess_score_cells_complete: bool,
    postprocess_score_profile_id: Option<String>,
    postprocess_spin_coverages: Vec<CorePostProcessSpinCoverage>,
    setup_finder_report: Option<SetupFinderReport>,
}

impl CoreExecutionResult {
    pub fn new(fields: Vec<(String, String)>, path_steps: Vec<CorePathStep>) -> Self {
        let execution_report = SearchExecutionReport::from_summary_fields(&fields, path_steps);
        Self {
            fields,
            execution_report,
            postprocess_replay_trace: None,
            postprocess_executions: Vec::new(),
            postprocess_execution_complete: false,
            postprocess_pattern_weights: Vec::new(),
            packing_candidate_keys: Vec::new(),
            normalized_solution_keys: Vec::new(),
            normalized_solution_identities: Vec::new(),
            representative_solution_identity: None,
            coverage_pattern_words: Vec::new(),
            solution_coverages: Vec::new(),
            normalized_solution_coverages: Vec::new(),
            solution_probabilities: Vec::new(),
            exact_scoring_execution_batches: Vec::new(),
            spin_coverage_execution_batches: Vec::new(),
            postprocess_score_cells: Vec::new(),
            postprocess_score_cells_complete: false,
            postprocess_score_profile_id: None,
            postprocess_spin_coverages: Vec::new(),
            setup_finder_report: None,
        }
    }
}
impl CoreExecutionResult {
    pub fn with_packing_candidate_keys(mut self, keys: Vec<String>) -> Self {
        self.packing_candidate_keys = keys;
        self
    }
}
impl CoreExecutionResult {
    pub fn with_normalized_solution_keys(mut self, keys: Vec<String>) -> Self {
        self.normalized_solution_keys = keys;
        self
    }

    pub fn with_normalized_solution_identities(
        mut self,
        identities: Vec<StandardBoard64TilingIdentity>,
    ) -> Self {
        self.normalized_solution_identities = identities;
        self
    }

    pub fn with_representative_solution_identity(
        mut self,
        identity: Option<StandardBoard64TilingIdentity>,
    ) -> Self {
        self.representative_solution_identity = identity;
        self
    }

    pub fn with_path_steps(mut self, path_steps: Vec<CorePathStep>) -> Self {
        self.execution_report =
            SearchExecutionReport::from_summary_fields(&self.fields, path_steps);
        self
    }

    pub fn with_coverage_pattern_words(mut self, words: Vec<u64>) -> Self {
        self.coverage_pattern_words = words;
        self
    }

    pub fn with_solution_coverages(mut self, coverage: Vec<SolutionCoverage>) -> Self {
        self.solution_coverages = coverage;
        self
    }

    pub fn with_normalized_solution_coverages(
        mut self,
        coverage: Vec<NormalizedSolutionCoverage>,
    ) -> Self {
        self.normalized_solution_coverages = coverage;
        self
    }

    pub fn with_solution_probabilities(
        mut self,
        probabilities: Vec<SolutionProbabilityReport>,
    ) -> Self {
        self.solution_probabilities = probabilities;
        self
    }

    pub fn with_exact_scoring_execution_batch(
        mut self,
        batch: Option<ExactScoringExecutionBatch>,
    ) -> Self {
        self.exact_scoring_execution_batches = batch.into_iter().collect();
        self
    }

    pub fn with_exact_scoring_execution_batches(
        mut self,
        batches: Vec<ExactScoringExecutionBatch>,
    ) -> Self {
        self.exact_scoring_execution_batches = batches;
        self
    }

    pub fn with_spin_coverage_execution_batch(
        mut self,
        batch: Option<SpinCoverageExecutionBatch>,
    ) -> Self {
        self.spin_coverage_execution_batches = batch.into_iter().collect();
        self
    }

    pub fn with_spin_coverage_execution_batches(
        mut self,
        batches: Vec<SpinCoverageExecutionBatch>,
    ) -> Self {
        self.spin_coverage_execution_batches = batches;
        self
    }

    pub fn with_postprocess_score_cells(
        mut self,
        cells: Vec<CorePostProcessScoreCell>,
        complete: bool,
        profile_id: impl Into<String>,
    ) -> Self {
        self.postprocess_score_cells = cells;
        self.postprocess_score_cells_complete = complete;
        self.postprocess_score_profile_id = Some(profile_id.into());
        self
    }

    pub fn with_postprocess_spin_coverages(
        mut self,
        coverages: Vec<CorePostProcessSpinCoverage>,
    ) -> Self {
        self.postprocess_spin_coverages = coverages;
        self
    }

    pub fn with_setup_finder_report(mut self, report: SetupFinderReport) -> Self {
        self.setup_finder_report = Some(report);
        self
    }
}
impl CoreExecutionResult {
    pub fn with_postprocess_execution_batch(
        mut self,
        executions: Vec<CorePostProcessExecution>,
        complete: bool,
        pattern_weights: Vec<String>,
    ) -> Self {
        self.postprocess_executions = executions;
        self.postprocess_execution_complete = complete;
        self.postprocess_pattern_weights = pattern_weights;
        self
    }
}
impl CoreExecutionResult {
    pub fn with_postprocess_replay_trace(
        mut self,
        replay_trace: Option<PostProcessReplayTrace>,
    ) -> Self {
        self.postprocess_replay_trace = replay_trace;
        self
    }
}
impl CoreExecutionResult {
    pub fn with_additional_fields(mut self, fields: Vec<(String, String)>) -> Self {
        let path_steps = self.path_steps().to_vec();
        self.fields.extend(fields);
        self.execution_report =
            SearchExecutionReport::from_summary_fields(&self.fields, path_steps);
        self
    }
}
impl CoreExecutionResult {
    pub fn with_replaced_fields(mut self, fields: Vec<(String, String)>) -> Self {
        let replacement_keys = fields
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        self.fields
            .retain(|(key, _)| !replacement_keys.contains(key.as_str()));
        self.fields.extend(fields);
        let path_steps = self.path_steps().to_vec();
        self.execution_report =
            SearchExecutionReport::from_summary_fields(&self.fields, path_steps);
        self
    }
}
impl CoreExecutionResult {
    pub fn summary_fields(&self) -> Vec<(String, String)> {
        self.fields.clone()
    }
}
impl CoreExecutionResult {
    pub fn execution_report(&self) -> &SearchExecutionReport {
        &self.execution_report
    }
}
impl CoreExecutionResult {
    pub fn path_steps(&self) -> &[CorePathStep] {
        self.execution_report.replay_trace().steps()
    }
}
impl CoreExecutionResult {
    pub fn postprocess_replay_trace(&self) -> Option<&PostProcessReplayTrace> {
        self.postprocess_replay_trace.as_ref()
    }
}
impl CoreExecutionResult {
    pub fn postprocess_executions(&self) -> &[CorePostProcessExecution] {
        &self.postprocess_executions
    }

    pub fn postprocess_execution_complete(&self) -> bool {
        self.postprocess_execution_complete
    }

    pub fn postprocess_pattern_weights(&self) -> &[String] {
        &self.postprocess_pattern_weights
    }

    pub fn packing_candidate_keys(&self) -> &[String] {
        &self.packing_candidate_keys
    }

    pub fn normalized_solution_keys(&self) -> &[String] {
        &self.normalized_solution_keys
    }

    pub fn normalized_solution_identities(&self) -> &[StandardBoard64TilingIdentity] {
        &self.normalized_solution_identities
    }

    pub fn representative_solution_identity(&self) -> Option<StandardBoard64TilingIdentity> {
        self.representative_solution_identity
    }

    pub fn coverage_pattern_words(&self) -> &[u64] {
        &self.coverage_pattern_words
    }

    pub fn solution_coverages(&self) -> &[SolutionCoverage] {
        &self.solution_coverages
    }

    pub fn normalized_solution_coverages(&self) -> &[NormalizedSolutionCoverage] {
        &self.normalized_solution_coverages
    }

    pub fn solution_probabilities(&self) -> &[SolutionProbabilityReport] {
        &self.solution_probabilities
    }

    pub fn exact_scoring_execution_batch(&self) -> Option<&ExactScoringExecutionBatch> {
        self.exact_scoring_execution_batches.first()
    }

    pub fn exact_scoring_execution_batches(&self) -> &[ExactScoringExecutionBatch] {
        &self.exact_scoring_execution_batches
    }

    pub fn spin_coverage_execution_batches(&self) -> &[SpinCoverageExecutionBatch] {
        &self.spin_coverage_execution_batches
    }

    pub fn postprocess_score_cells(&self) -> &[CorePostProcessScoreCell] {
        &self.postprocess_score_cells
    }

    pub const fn postprocess_score_cells_complete(&self) -> bool {
        self.postprocess_score_cells_complete
    }

    pub fn postprocess_score_profile_id(&self) -> Option<&str> {
        self.postprocess_score_profile_id.as_deref()
    }

    pub fn postprocess_spin_coverages(&self) -> &[CorePostProcessSpinCoverage] {
        &self.postprocess_spin_coverages
    }

    pub fn setup_finder_report(&self) -> Option<&SetupFinderReport> {
        self.setup_finder_report.as_ref()
    }

    pub(crate) fn take_exact_scoring_execution_batches(
        &mut self,
    ) -> Vec<ExactScoringExecutionBatch> {
        core::mem::take(&mut self.exact_scoring_execution_batches)
    }

    pub(crate) fn take_spin_coverage_execution_batches(
        &mut self,
    ) -> Vec<SpinCoverageExecutionBatch> {
        core::mem::take(&mut self.spin_coverage_execution_batches)
    }
}
impl CoreExecutionResult {
    pub fn field(&self, key: &str) -> Option<&str> {
        field_value(&self.fields, key)
    }
}
impl CoreExecutionResult {
    pub fn bool_field(&self, key: &str) -> Option<bool> {
        self.field(key).and_then(|value| value.parse().ok())
    }
}
impl CoreExecutionResult {
    pub fn usize_field(&self, key: &str) -> Option<usize> {
        self.field(key).and_then(|value| value.parse().ok())
    }
}
impl CoreExecutionResult {
    pub fn u8_field(&self, key: &str) -> Option<u8> {
        self.field(key).and_then(|value| value.parse().ok())
    }
}
impl CoreExecutionResult {
    pub fn u64_field(&self, key: &str) -> Option<u64> {
        self.field(key).and_then(|value| value.parse().ok())
    }
}
impl CoreExecutionResult {
    pub fn solution_found(&self) -> bool {
        field_value(&self.fields, "solution_found") == Some("true")
    }
}
impl CoreExecutionResult {
    pub fn sample_trace_available(&self) -> bool {
        field_value(&self.fields, "sample_trace_available") == Some("true")
    }
}

fn field_value<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find_map(|(field_key, value)| (field_key == key).then_some(value.as_str()))
}
