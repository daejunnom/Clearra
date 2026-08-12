use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity;
use clearra_replay::{
    ExactScoringExecutionBatch, ReplayTrace as PostProcessReplayTrace, SpinCoverageExecutionBatch,
};
use std::sync::Arc;

use crate::{
    core_postprocess_execution::CorePostProcessExecution,
    core_postprocess_score_cell::CorePostProcessScoreCell,
    core_postprocess_spin_coverage::CorePostProcessSpinCoverage,
    finesse_report::FinesseReport,
    result_views::SearchExecutionReport,
    setup_finder_report::SetupFinderReport,
    solution_probability::{
        NormalizedSolutionCoverage, SolutionAverageScoreReport, SolutionCoverage,
        SolutionProbabilityReport,
    },
    tiling_solution_store::TilingSolutionPageStore,
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
    solution_average_scores: Vec<SolutionAverageScoreReport>,
    exact_scoring_execution_batches: Vec<ExactScoringExecutionBatch>,
    spin_coverage_execution_batches: Vec<SpinCoverageExecutionBatch>,
    postprocess_score_cells: Vec<CorePostProcessScoreCell>,
    postprocess_score_cells_complete: bool,
    postprocess_score_profile_id: Option<String>,
    postprocess_spin_coverages: Vec<CorePostProcessSpinCoverage>,
    setup_finder_report: Option<SetupFinderReport>,
    finesse_report: Option<FinesseReport>,
    tiling_solution_page_store: Option<Arc<TilingSolutionPageStore>>,
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
            solution_average_scores: Vec::new(),
            exact_scoring_execution_batches: Vec::new(),
            spin_coverage_execution_batches: Vec::new(),
            postprocess_score_cells: Vec::new(),
            postprocess_score_cells_complete: false,
            postprocess_score_profile_id: None,
            postprocess_spin_coverages: Vec::new(),
            setup_finder_report: None,
            finesse_report: None,
            tiling_solution_page_store: None,
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

    pub fn with_tiling_solution_page_store(mut self, store: Arc<TilingSolutionPageStore>) -> Self {
        self.tiling_solution_page_store = Some(store);
        self
    }

    pub fn without_tiling_solution_page_store(mut self) -> Self {
        self.tiling_solution_page_store = None;
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

    pub fn with_solution_average_scores(mut self, scores: Vec<SolutionAverageScoreReport>) -> Self {
        self.solution_average_scores = scores;
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

    pub fn without_postprocess_score_cells(mut self) -> Self {
        self.postprocess_score_cells.clear();
        self.postprocess_score_cells_complete = false;
        self.postprocess_score_profile_id = None;
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

    pub fn with_finesse_report(mut self, report: FinesseReport) -> Self {
        self.finesse_report = Some(report);
        self
    }

    /// Removes a finesse-search report whose witnesses and per-solution rows no longer have an
    /// authoritative solution set. Finesse score is independent of a searched solution set and
    /// therefore remains valid.
    pub fn without_finesse_search_report(mut self) -> Self {
        if self
            .finesse_report
            .as_ref()
            .is_some_and(|report| report.mode() != "score")
        {
            self.finesse_report = None;
        }
        self
    }

    /// Canonicalizes invalid declared availability fields and physically removes every private
    /// solution authority before a result crosses a public application boundary.
    pub fn into_fail_closed_public_solution_surface(mut self) -> Self {
        self.fields = self.fail_closed_solution_summary_fields();
        self.postprocess_replay_trace = None;
        self.postprocess_executions.clear();
        self.postprocess_execution_complete = false;
        self.postprocess_pattern_weights.clear();
        self.packing_candidate_keys.clear();
        self.normalized_solution_keys.clear();
        self.normalized_solution_identities.clear();
        self.representative_solution_identity = None;
        self.solution_coverages.clear();
        self.normalized_solution_coverages.clear();
        self.solution_probabilities.clear();
        self.solution_average_scores.clear();
        self.exact_scoring_execution_batches.clear();
        self.spin_coverage_execution_batches.clear();
        self.postprocess_score_cells.clear();
        self.postprocess_score_cells_complete = false;
        self.postprocess_score_profile_id = None;
        self.postprocess_spin_coverages.clear();
        self.tiling_solution_page_store = None;
        self = self.without_finesse_search_report();
        self.execution_report =
            SearchExecutionReport::from_summary_fields(&self.fields, Vec::new());
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

    pub fn fail_closed_solution_summary_fields(&self) -> Vec<(String, String)> {
        let availability = self.execution_report.solution_set_availability();
        let has_declared_policy = self
            .fields
            .iter()
            .any(|(key, _)| key == "search_output_policy");
        let coverage_summary = self
            .fields
            .iter()
            .any(|(key, value)| key == "search_output_policy" && value == "coverage-summary");
        if !coverage_summary
            && ((!availability.uses_explicit_contract() && !has_declared_policy)
                || (availability.contract_valid()
                    && availability
                        .materialized_key_count_matches(self.normalized_solution_keys.len())))
        {
            return self.summary_fields();
        }

        const SOLUTION_AVAILABILITY_KEYS: &[&str] = &[
            "search_output_policy",
            "unique_solution_count",
            "normalized_unique_solution_count",
            "actual_normalized_unique_solution_count",
            "total_solution_count",
            "solution_count_calculated",
            "solution_set_materialized",
            "solution_keys_materialized_count",
            "solution_keys_complete",
            "solution_page_available",
            "normalized_solution_set_hash",
            "actual_normalized_solution_set_hash",
            "mirror_unique_solution_count",
            "mirror_normalized_solution_set_hash",
            "original_unique_solution_count",
            "coverage_row_count",
            "b2b_preserving_candidate_pattern_count",
            "pattern_verified_execution_count",
            "minimum_cover_source_solution_count",
            "minimum_cover_selected_solution_count",
            "solution_trace_count",
            "unique_solution_trace_count",
            "solution_path_count",
            "solution_probability_count",
            "objective_solution_traces",
            "objective_unique_solution_traces",
            "post_pc_solution_count",
            "b2b_preserving_solution_count",
        ];
        let search_output_policy = self
            .fields
            .iter()
            .filter(|(key, _)| key == "search_output_policy")
            .map(|(_, value)| value.as_str())
            .find(|value| *value == "coverage-summary")
            .or_else(|| {
                self.field("search_output_policy")
                    .filter(|value| matches!(*value, "summary" | "trace" | "coverage-rows"))
            })
            .map(ToOwned::to_owned);
        let mut fields = self
            .fields
            .iter()
            .filter(|(key, _)| !SOLUTION_AVAILABILITY_KEYS.contains(&key.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if let Some(search_output_policy) = search_output_policy {
            fields.push(("search_output_policy".to_owned(), search_output_policy));
        }
        fields.extend([
            (
                "unique_solution_count".to_owned(),
                "not-calculated".to_owned(),
            ),
            (
                "normalized_unique_solution_count".to_owned(),
                "not-calculated".to_owned(),
            ),
            ("solution_count_calculated".to_owned(), "false".to_owned()),
            ("solution_set_materialized".to_owned(), "false".to_owned()),
            (
                "solution_keys_materialized_count".to_owned(),
                "0".to_owned(),
            ),
            ("solution_keys_complete".to_owned(), "false".to_owned()),
            ("solution_page_available".to_owned(), "false".to_owned()),
            (
                "normalized_solution_set_hash".to_owned(),
                "not-calculated".to_owned(),
            ),
            (
                "actual_normalized_solution_set_hash".to_owned(),
                "not-calculated".to_owned(),
            ),
        ]);
        for key in [
            "total_solution_count",
            "actual_normalized_unique_solution_count",
            "mirror_unique_solution_count",
            "original_unique_solution_count",
            "mirror_normalized_solution_set_hash",
            "coverage_row_count",
            "b2b_preserving_candidate_pattern_count",
            "pattern_verified_execution_count",
            "minimum_cover_source_solution_count",
            "minimum_cover_selected_solution_count",
            "solution_trace_count",
            "unique_solution_trace_count",
            "solution_path_count",
            "solution_probability_count",
            "objective_solution_traces",
            "objective_unique_solution_traces",
            "post_pc_solution_count",
            "b2b_preserving_solution_count",
        ] {
            if self.field(key).is_some() {
                fields.push((key.to_owned(), "not-calculated".to_owned()));
            }
        }
        fields
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

    pub fn tiling_solution_page_store(&self) -> Option<&Arc<TilingSolutionPageStore>> {
        self.tiling_solution_page_store.as_ref()
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

    pub fn solution_average_scores(&self) -> &[SolutionAverageScoreReport] {
        &self.solution_average_scores
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

    pub fn finesse_report(&self) -> Option<&FinesseReport> {
        self.finesse_report.as_ref()
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{CoreExecutionResult, FinesseReport, TilingSolutionPageStore};

    #[test]
    fn without_tiling_solution_page_store_removes_attached_private_authority() {
        let store = Arc::new(
            TilingSolutionPageStore::new(0, Vec::new(), Vec::new())
                .expect("empty synthetic page store"),
        );
        let result =
            CoreExecutionResult::new(Vec::new(), Vec::new()).with_tiling_solution_page_store(store);

        assert!(result.tiling_solution_page_store().is_some());
        assert!(result
            .without_tiling_solution_page_store()
            .tiling_solution_page_store()
            .is_none());
    }

    #[test]
    fn public_fail_closed_surface_physically_removes_attached_solution_authority() {
        let store = Arc::new(
            TilingSolutionPageStore::new(0, Vec::new(), Vec::new())
                .expect("empty synthetic page store"),
        );
        let result = CoreExecutionResult::new(
            vec![
                ("search_output_policy".to_owned(), "summray".to_owned()),
                ("unique_solution_count".to_owned(), "1".to_owned()),
            ],
            Vec::new(),
        )
        .with_packing_candidate_keys(vec!["private-packing-key".to_owned()])
        .with_normalized_solution_keys(vec!["private-solution-key".to_owned()])
        .with_tiling_solution_page_store(store)
        .with_finesse_report(FinesseReport::new(
            "search",
            "oracle",
            true,
            None,
            Vec::new(),
        ));

        let public = result.into_fail_closed_public_solution_surface();

        assert_eq!(
            public.field("unique_solution_count"),
            Some("not-calculated")
        );
        assert!(public.packing_candidate_keys().is_empty());
        assert!(public.normalized_solution_keys().is_empty());
        assert!(public.tiling_solution_page_store().is_none());
        assert!(public.finesse_report().is_none());
    }

    #[test]
    fn malformed_explicit_availability_is_canonicalized_to_unavailable() {
        let result = CoreExecutionResult::new(
            vec![
                (
                    "search_output_policy".to_owned(),
                    "coverage-summary".to_owned(),
                ),
                ("unique_solution_count".to_owned(), "7".to_owned()),
                (
                    "normalized_unique_solution_count".to_owned(),
                    "not-calculated".to_owned(),
                ),
                (
                    "normalized_solution_set_hash".to_owned(),
                    "not-calculated".to_owned(),
                ),
                (
                    "actual_normalized_solution_set_hash".to_owned(),
                    "not-calculated".to_owned(),
                ),
                ("solution_count_calculated".to_owned(), "true".to_owned()),
                ("solution_set_materialized".to_owned(), "true".to_owned()),
                (
                    "solution_keys_materialized_count".to_owned(),
                    "7".to_owned(),
                ),
                ("solution_keys_complete".to_owned(), "true".to_owned()),
                ("solution_page_available".to_owned(), "true".to_owned()),
            ],
            Vec::new(),
        )
        .with_normalized_solution_keys(vec!["fake".to_owned()]);

        let fields = result.fail_closed_solution_summary_fields();
        let field = |key: &str| {
            fields
                .iter()
                .find_map(|(field_key, value)| (field_key == key).then_some(value.as_str()))
        };
        assert_eq!(field("search_output_policy"), Some("coverage-summary"));
        assert_eq!(field("unique_solution_count"), Some("not-calculated"));
        assert_eq!(field("solution_count_calculated"), Some("false"));
        assert_eq!(field("solution_set_materialized"), Some("false"));
        assert_eq!(field("solution_keys_materialized_count"), Some("0"));
        assert_eq!(field("solution_keys_complete"), Some("false"));
        assert_eq!(field("solution_page_available"), Some("false"));
        assert_eq!(
            fields
                .iter()
                .filter(|(key, _)| key == "solution_count_calculated")
                .count(),
            1
        );
    }

    #[test]
    fn unknown_policy_canonicalizes_numeric_and_hash_authority() {
        let result = CoreExecutionResult::new(
            vec![
                ("search_output_policy".to_owned(), "summray".to_owned()),
                ("unique_solution_count".to_owned(), "9".to_owned()),
                (
                    "normalized_unique_solution_count".to_owned(),
                    "7".to_owned(),
                ),
                ("total_solution_count".to_owned(), "11".to_owned()),
                (
                    "actual_normalized_unique_solution_count".to_owned(),
                    "7".to_owned(),
                ),
                (
                    "normalized_solution_set_hash".to_owned(),
                    "cts1:fake".to_owned(),
                ),
                (
                    "actual_normalized_solution_set_hash".to_owned(),
                    "cts1:fake".to_owned(),
                ),
                (
                    "mirror_normalized_solution_set_hash".to_owned(),
                    "cts1:mirror".to_owned(),
                ),
                ("b2b_preserving_solution_count".to_owned(), "5".to_owned()),
            ],
            Vec::new(),
        );

        let fields = result.fail_closed_solution_summary_fields();
        let field = |key: &str| {
            fields
                .iter()
                .find_map(|(field_key, value)| (field_key == key).then_some(value.as_str()))
        };
        for key in [
            "unique_solution_count",
            "normalized_unique_solution_count",
            "total_solution_count",
            "actual_normalized_unique_solution_count",
            "normalized_solution_set_hash",
            "actual_normalized_solution_set_hash",
            "mirror_normalized_solution_set_hash",
            "b2b_preserving_solution_count",
        ] {
            assert_eq!(field(key), Some("not-calculated"), "{key}");
        }
        assert_eq!(field("solution_count_calculated"), Some("false"));
        assert_eq!(field("solution_set_materialized"), Some("false"));
        assert_eq!(field("solution_page_available"), Some("false"));
        assert_eq!(field("search_output_policy"), None);
    }
}
