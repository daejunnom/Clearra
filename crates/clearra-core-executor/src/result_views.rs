mod backend_report {
    use super::summary_fields::field_value;

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct BackendReport {
        backend_requested: String,
        backend_selected: String,
        backend_fallback_reason: String,
    }

    impl BackendReport {
        pub fn new(
            backend_requested: impl Into<String>,
            backend_selected: impl Into<String>,
            backend_fallback_reason: impl Into<String>,
        ) -> Self {
            Self {
                backend_requested: backend_requested.into(),
                backend_selected: backend_selected.into(),
                backend_fallback_reason: backend_fallback_reason.into(),
            }
        }
    }
    impl BackendReport {
        pub fn from_summary_fields(fields: &[(String, String)]) -> Self {
            Self::new(
                field_value(fields, "backend_requested")
                    .or_else(|| field_value(fields, "requested_backend"))
                    .unwrap_or("none"),
                field_value(fields, "backend_selected")
                    .or_else(|| field_value(fields, "selected_backend"))
                    .unwrap_or("none"),
                field_value(fields, "backend_fallback_reason").unwrap_or("none"),
            )
        }
    }
    impl BackendReport {
        pub fn backend_requested(&self) -> &str {
            &self.backend_requested
        }
    }
    impl BackendReport {
        pub fn backend_selected(&self) -> &str {
            &self.backend_selected
        }
    }
    impl BackendReport {
        pub fn backend_fallback_reason(&self) -> &str {
            &self.backend_fallback_reason
        }
    }
}
mod build_variant_view {
    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct BuildVariantView {
        variant_id: String,
        coverage_probability: String,
        score_event_basis: String,
        kick_evidence_count: usize,
    }

    impl BuildVariantView {
        pub fn new(variant_id: impl Into<String>, coverage_probability: impl Into<String>) -> Self {
            Self {
                variant_id: variant_id.into(),
                coverage_probability: coverage_probability.into(),
                score_event_basis: "none".to_owned(),
                kick_evidence_count: 0,
            }
        }
    }
    impl BuildVariantView {
        pub fn with_score_event_basis(mut self, score_event_basis: impl Into<String>) -> Self {
            self.score_event_basis = score_event_basis.into();
            self
        }
    }
    impl BuildVariantView {
        pub fn with_kick_evidence_count(mut self, kick_evidence_count: usize) -> Self {
            self.kick_evidence_count = kick_evidence_count;
            self
        }
    }
    impl BuildVariantView {
        pub fn variant_id(&self) -> &str {
            &self.variant_id
        }
    }
    impl BuildVariantView {
        pub fn coverage_probability(&self) -> &str {
            &self.coverage_probability
        }
    }
    impl BuildVariantView {
        pub fn score_event_basis(&self) -> &str {
            &self.score_event_basis
        }
    }
    impl BuildVariantView {
        pub fn kick_evidence_count(&self) -> usize {
            self.kick_evidence_count
        }
    }
}
mod buildup_result {
    use super::summary_fields::{bool_field, usize_field};

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct BuildUpResult {
        solution_found: bool,
        total_solution_count: usize,
        unique_solution_count: usize,
        count_complete: bool,
    }

    impl BuildUpResult {
        pub fn new(
            solution_found: bool,
            total_solution_count: usize,
            unique_solution_count: usize,
            count_complete: bool,
        ) -> Self {
            Self {
                solution_found,
                total_solution_count,
                unique_solution_count,
                count_complete,
            }
        }
    }
    impl BuildUpResult {
        pub fn from_summary_fields(fields: &[(String, String)]) -> Self {
            Self::new(
                bool_field(fields, "solution_found"),
                usize_field(fields, "total_solution_count"),
                usize_field(fields, "unique_solution_count"),
                bool_field(fields, "count_complete"),
            )
        }
    }
    impl BuildUpResult {
        pub fn solution_found(&self) -> bool {
            self.solution_found
        }
    }
    impl BuildUpResult {
        pub fn total_solution_count(&self) -> usize {
            self.total_solution_count
        }
    }
    impl BuildUpResult {
        pub fn unique_solution_count(&self) -> usize {
            self.unique_solution_count
        }
    }
    impl BuildUpResult {
        pub fn count_complete(&self) -> bool {
            self.count_complete
        }
    }
}
mod coverage_result {
    use super::{
        summary_fields::{field_value, usize_field},
        CoverageRowView,
    };

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct CoverageResult {
        coverage_probability: String,
        coverage_row_count: usize,
        rows: Vec<CoverageRowView>,
    }

    impl CoverageResult {
        pub fn new(
            coverage_probability: impl Into<String>,
            coverage_row_count: usize,
            rows: Vec<CoverageRowView>,
        ) -> Self {
            Self {
                coverage_probability: coverage_probability.into(),
                coverage_row_count,
                rows,
            }
        }
    }
    impl CoverageResult {
        pub fn from_summary_fields(fields: &[(String, String)]) -> Self {
            Self::new(
                field_value(fields, "coverage_probability").unwrap_or("0.0"),
                usize_field(fields, "coverage_row_count"),
                Vec::new(),
            )
        }
    }
    impl CoverageResult {
        pub fn coverage_probability(&self) -> &str {
            &self.coverage_probability
        }
    }
    impl CoverageResult {
        pub fn coverage_row_count(&self) -> usize {
            self.coverage_row_count
        }
    }
    impl CoverageResult {
        pub fn rows(&self) -> &[CoverageRowView] {
            &self.rows
        }
    }
}
mod coverage_row_view {
    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct CoverageRowView {
        row_id: String,
        covered_pattern_count: usize,
        coverage_probability: String,
    }

    impl CoverageRowView {
        pub fn new(
            row_id: impl Into<String>,
            covered_pattern_count: usize,
            coverage_probability: impl Into<String>,
        ) -> Self {
            Self {
                row_id: row_id.into(),
                covered_pattern_count,
                coverage_probability: coverage_probability.into(),
            }
        }
    }
    impl CoverageRowView {
        pub fn row_id(&self) -> &str {
            &self.row_id
        }
    }
    impl CoverageRowView {
        pub fn covered_pattern_count(&self) -> usize {
            self.covered_pattern_count
        }
    }
    impl CoverageRowView {
        pub fn coverage_probability(&self) -> &str {
            &self.coverage_probability
        }
    }
}
mod objective_result {
    use super::summary_fields::{bool_field, field_value, usize_field};

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct ObjectiveResult {
        total_solution_count: usize,
        unique_solution_count: usize,
        retained_trace_count: usize,
        count_complete: bool,
        trace_retention_truncated: bool,
        trace_retention_reason: String,
    }

    impl ObjectiveResult {
        pub fn new(
            total_solution_count: usize,
            unique_solution_count: usize,
            retained_trace_count: usize,
            count_complete: bool,
            trace_retention_truncated: bool,
            trace_retention_reason: impl Into<String>,
        ) -> Self {
            Self {
                total_solution_count,
                unique_solution_count,
                retained_trace_count,
                count_complete,
                trace_retention_truncated,
                trace_retention_reason: trace_retention_reason.into(),
            }
        }
    }
    impl ObjectiveResult {
        pub fn from_summary_fields(fields: &[(String, String)]) -> Self {
            Self::new(
                usize_field(fields, "total_solution_count"),
                usize_field(fields, "unique_solution_count"),
                usize_field(fields, "retained_trace_count"),
                bool_field(fields, "count_complete"),
                bool_field(fields, "trace_retention_truncated"),
                field_value(fields, "trace_retention_reason").unwrap_or("none"),
            )
        }
    }
    impl ObjectiveResult {
        pub fn total_solution_count(&self) -> usize {
            self.total_solution_count
        }
    }
    impl ObjectiveResult {
        pub fn unique_solution_count(&self) -> usize {
            self.unique_solution_count
        }
    }
    impl ObjectiveResult {
        pub fn retained_trace_count(&self) -> usize {
            self.retained_trace_count
        }
    }
    impl ObjectiveResult {
        pub fn count_complete(&self) -> bool {
            self.count_complete
        }
    }
    impl ObjectiveResult {
        pub fn trace_retention_truncated(&self) -> bool {
            self.trace_retention_truncated
        }
    }
    impl ObjectiveResult {
        pub fn trace_retention_reason(&self) -> &str {
            &self.trace_retention_reason
        }
    }
}
mod packing_candidate_view {
    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct PackingCandidateView {
        candidate_id: String,
        queue_consumed: usize,
        placed_piece_count: usize,
    }

    impl PackingCandidateView {
        pub fn new(
            candidate_id: impl Into<String>,
            queue_consumed: usize,
            placed_piece_count: usize,
        ) -> Self {
            Self {
                candidate_id: candidate_id.into(),
                queue_consumed,
                placed_piece_count,
            }
        }
    }
    impl PackingCandidateView {
        pub fn candidate_id(&self) -> &str {
            &self.candidate_id
        }
    }
    impl PackingCandidateView {
        pub fn queue_consumed(&self) -> usize {
            self.queue_consumed
        }
    }
    impl PackingCandidateView {
        pub fn placed_piece_count(&self) -> usize {
            self.placed_piece_count
        }
    }
}
mod packing_result {
    use super::{summary_fields::usize_field, PackingCandidateView};

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct PackingResult {
        candidate_count: usize,
        candidates: Vec<PackingCandidateView>,
    }

    impl PackingResult {
        pub fn new(candidate_count: usize, candidates: Vec<PackingCandidateView>) -> Self {
            Self {
                candidate_count,
                candidates,
            }
        }
    }
    impl PackingResult {
        pub fn from_summary_fields(fields: &[(String, String)]) -> Self {
            Self::new(usize_field(fields, "packing_candidate_count"), Vec::new())
        }
    }
    impl PackingResult {
        pub fn candidate_count(&self) -> usize {
            self.candidate_count
        }
    }
    impl PackingResult {
        pub fn candidates(&self) -> &[PackingCandidateView] {
            &self.candidates
        }
    }
}
mod replay_trace {
    use crate::core_execution_result::CorePathStep;

    use super::summary_fields::{bool_field, field_value, usize_field};

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct ReplayTrace {
        steps: Vec<CorePathStep>,
        retained_trace_count: usize,
        trace_retention_truncated: bool,
        trace_retention_reason: String,
    }

    impl ReplayTrace {
        pub fn new(
            steps: Vec<CorePathStep>,
            retained_trace_count: usize,
            trace_retention_truncated: bool,
            trace_retention_reason: impl Into<String>,
        ) -> Self {
            Self {
                steps,
                retained_trace_count,
                trace_retention_truncated,
                trace_retention_reason: trace_retention_reason.into(),
            }
        }
    }
    impl ReplayTrace {
        pub fn from_summary_fields(fields: &[(String, String)], steps: Vec<CorePathStep>) -> Self {
            Self::new(
                steps,
                usize_field(fields, "retained_trace_count"),
                bool_field(fields, "trace_retention_truncated"),
                field_value(fields, "trace_retention_reason").unwrap_or("none"),
            )
        }
    }
    impl ReplayTrace {
        pub fn steps(&self) -> &[CorePathStep] {
            &self.steps
        }
    }
    impl ReplayTrace {
        pub fn retained_trace_count(&self) -> usize {
            self.retained_trace_count
        }
    }
    impl ReplayTrace {
        pub fn trace_retention_truncated(&self) -> bool {
            self.trace_retention_truncated
        }
    }
    impl ReplayTrace {
        pub fn trace_retention_reason(&self) -> &str {
            &self.trace_retention_reason
        }
    }
}
mod search_execution_report {
    use crate::core_execution_result::CorePathStep;

    use super::{
        BackendReport, BuildUpResult, BuildVariantView, CoverageResult, ObjectiveResult,
        PackingResult, ReplayTrace, SolutionSetAvailability,
    };

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct SearchExecutionReport {
        backend_report: BackendReport,
        packing_result: PackingResult,
        buildup_result: BuildUpResult,
        build_variant_view: Option<BuildVariantView>,
        coverage_result: CoverageResult,
        objective_result: ObjectiveResult,
        replay_trace: ReplayTrace,
        solution_set_availability: SolutionSetAvailability,
    }

    impl SearchExecutionReport {
        pub fn new(
            backend_report: BackendReport,
            packing_result: PackingResult,
            buildup_result: BuildUpResult,
            build_variant_view: Option<BuildVariantView>,
            coverage_result: CoverageResult,
            objective_result: ObjectiveResult,
            replay_trace: ReplayTrace,
        ) -> Self {
            Self {
                backend_report,
                packing_result,
                buildup_result,
                build_variant_view,
                coverage_result,
                objective_result,
                replay_trace,
                solution_set_availability: SolutionSetAvailability::default(),
            }
        }

        pub fn with_solution_set_availability(
            mut self,
            solution_set_availability: SolutionSetAvailability,
        ) -> Self {
            self.solution_set_availability = solution_set_availability;
            self
        }
    }
    impl SearchExecutionReport {
        pub fn from_summary_fields(fields: &[(String, String)], steps: Vec<CorePathStep>) -> Self {
            Self::new(
                BackendReport::from_summary_fields(fields),
                PackingResult::from_summary_fields(fields),
                BuildUpResult::from_summary_fields(fields),
                None,
                CoverageResult::from_summary_fields(fields),
                ObjectiveResult::from_summary_fields(fields),
                ReplayTrace::from_summary_fields(fields, steps),
            )
            .with_solution_set_availability(SolutionSetAvailability::from_summary_fields(fields))
        }
    }
    impl SearchExecutionReport {
        pub fn backend_report(&self) -> &BackendReport {
            &self.backend_report
        }
    }
    impl SearchExecutionReport {
        pub fn packing_result(&self) -> &PackingResult {
            &self.packing_result
        }
    }
    impl SearchExecutionReport {
        pub fn buildup_result(&self) -> &BuildUpResult {
            &self.buildup_result
        }
    }
    impl SearchExecutionReport {
        pub fn build_variant_view(&self) -> Option<&BuildVariantView> {
            self.build_variant_view.as_ref()
        }
    }
    impl SearchExecutionReport {
        pub fn coverage_result(&self) -> &CoverageResult {
            &self.coverage_result
        }
    }
    impl SearchExecutionReport {
        pub fn objective_result(&self) -> &ObjectiveResult {
            &self.objective_result
        }
    }
    impl SearchExecutionReport {
        pub fn replay_trace(&self) -> &ReplayTrace {
            &self.replay_trace
        }
    }
    impl SearchExecutionReport {
        pub fn solution_set_availability(&self) -> &SolutionSetAvailability {
            &self.solution_set_availability
        }
    }
}
mod solution_set_availability {
    use super::summary_fields::{field_value, optional_usize_field};

    const EXPLICIT_MARKER_KEYS: [&str; 5] = [
        "solution_count_calculated",
        "solution_set_materialized",
        "solution_keys_materialized_count",
        "solution_keys_complete",
        "solution_page_available",
    ];

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    enum AvailabilityContractSource {
        #[default]
        Unavailable,
        Explicit,
        Legacy,
    }

    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct SolutionSetAvailability {
        solution_count_calculated: bool,
        solution_set_materialized: bool,
        solution_keys_materialized_count: usize,
        solution_keys_complete: bool,
        solution_page_available: bool,
        contract_source: AvailabilityContractSource,
        contract_valid: bool,
    }

    impl SolutionSetAvailability {
        pub fn new(
            solution_count_calculated: bool,
            solution_set_materialized: bool,
            solution_keys_materialized_count: usize,
            solution_keys_complete: bool,
            solution_page_available: bool,
        ) -> Self {
            Self {
                solution_count_calculated,
                solution_set_materialized,
                solution_keys_materialized_count,
                solution_keys_complete,
                solution_page_available,
                contract_source: AvailabilityContractSource::Explicit,
                contract_valid: true,
            }
        }

        pub fn from_summary_fields(fields: &[(String, String)]) -> Self {
            if fields
                .iter()
                .any(|(key, value)| key == "search_output_policy" && value == "coverage-summary")
            {
                return explicit_availability(fields, true).unwrap_or_else(explicit_unavailable);
            }

            if EXPLICIT_MARKER_KEYS
                .iter()
                .any(|key| field_value(fields, key).is_some())
            {
                return explicit_availability(fields, false).unwrap_or_else(explicit_unavailable);
            }

            legacy_availability(fields)
        }

        pub fn solution_count_calculated(&self) -> bool {
            self.solution_count_calculated
        }

        pub fn solution_set_materialized(&self) -> bool {
            self.solution_set_materialized
        }

        pub fn solution_keys_materialized_count(&self) -> usize {
            self.solution_keys_materialized_count
        }

        pub fn solution_keys_complete(&self) -> bool {
            self.solution_keys_complete
        }

        pub fn solution_page_available(&self) -> bool {
            self.solution_page_available
        }

        pub fn contract_valid(&self) -> bool {
            self.contract_valid
        }

        pub fn uses_explicit_contract(&self) -> bool {
            self.contract_source == AvailabilityContractSource::Explicit
        }

        pub fn uses_legacy_inference(&self) -> bool {
            self.contract_source == AvailabilityContractSource::Legacy
        }

        pub fn materialized_key_count_matches(&self, actual_key_count: usize) -> bool {
            self.uses_legacy_inference()
                || self.solution_keys_materialized_count == actual_key_count
        }
    }

    fn explicit_availability(
        fields: &[(String, String)],
        coverage_summary: bool,
    ) -> Option<SolutionSetAvailability> {
        let policy = single_field_value(fields, "search_output_policy")?;
        if coverage_summary {
            if policy != "coverage-summary"
                || single_field_value(fields, "unique_solution_count")? != "not-calculated"
                || single_field_value(fields, "normalized_unique_solution_count")?
                    != "not-calculated"
                || single_field_value(fields, "normalized_solution_set_hash")? != "not-calculated"
                || single_field_value(fields, "actual_normalized_solution_set_hash")?
                    != "not-calculated"
                || optional_existing_count_is_not_calculated(
                    fields,
                    "mirror_normalized_solution_set_hash",
                )? == Some(false)
                || optional_existing_count_is_not_calculated(fields, "total_solution_count")?
                    == Some(false)
            {
                return None;
            }
        } else if !matches!(policy, "summary" | "trace" | "coverage-rows") {
            return None;
        }

        let solution_count_calculated = single_bool_field(fields, "solution_count_calculated")?;
        let solution_set_materialized = single_bool_field(fields, "solution_set_materialized")?;
        let solution_keys_materialized_count =
            single_usize_field(fields, "solution_keys_materialized_count")?;
        let solution_keys_complete = single_bool_field(fields, "solution_keys_complete")?;
        let solution_page_available = single_bool_field(fields, "solution_page_available")?;

        if !solution_count_calculated {
            if solution_set_materialized
                || solution_keys_materialized_count != 0
                || solution_keys_complete
                || solution_page_available
                || single_field_value(fields, "unique_solution_count")? != "not-calculated"
                || optional_existing_count_is_not_calculated(
                    fields,
                    "normalized_unique_solution_count",
                )? == Some(false)
                || optional_existing_count_is_not_calculated(fields, "total_solution_count")?
                    == Some(false)
            {
                return None;
            }
            return Some(SolutionSetAvailability::new(false, false, 0, false, false));
        }

        if coverage_summary || !solution_set_materialized {
            return None;
        }
        let unique_solution_count = single_field_value(fields, "unique_solution_count")
            .and_then(|value| value.parse::<usize>().ok())?;
        let normalized_solution_count =
            optional_single_usize_field(fields, "normalized_unique_solution_count")?
                .unwrap_or(unique_solution_count);
        let _total_solution_count = optional_single_usize_field(fields, "total_solution_count")?;
        if solution_keys_materialized_count > normalized_solution_count
            || (solution_keys_complete
                && solution_keys_materialized_count != normalized_solution_count)
            || (solution_page_available
                && (solution_keys_complete
                    || solution_keys_materialized_count >= normalized_solution_count))
        {
            return None;
        }

        Some(SolutionSetAvailability::new(
            true,
            true,
            solution_keys_materialized_count,
            solution_keys_complete,
            solution_page_available,
        ))
    }

    fn legacy_availability(fields: &[(String, String)]) -> SolutionSetAvailability {
        let policy_is_legacy_compatible =
            match single_optional_field_value(fields, "search_output_policy") {
                Ok(None) => true,
                Ok(Some("summary" | "trace" | "coverage-rows")) => true,
                Ok(Some(_)) | Err(()) => false,
            };
        if !policy_is_legacy_compatible {
            return SolutionSetAvailability::default();
        }
        let solution_count_calculated = optional_usize_field(fields, "unique_solution_count")
            .is_some()
            || optional_usize_field(fields, "total_solution_count").is_some();
        SolutionSetAvailability {
            solution_count_calculated,
            solution_set_materialized: solution_count_calculated,
            solution_keys_materialized_count: 0,
            solution_keys_complete: solution_count_calculated,
            solution_page_available: false,
            contract_source: AvailabilityContractSource::Legacy,
            contract_valid: true,
        }
    }

    fn explicit_unavailable() -> SolutionSetAvailability {
        SolutionSetAvailability {
            contract_source: AvailabilityContractSource::Explicit,
            ..SolutionSetAvailability::default()
        }
    }

    fn single_field_value<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
        single_optional_field_value(fields, key).ok().flatten()
    }

    fn single_optional_field_value<'a>(
        fields: &'a [(String, String)],
        key: &str,
    ) -> Result<Option<&'a str>, ()> {
        let mut values = fields
            .iter()
            .filter_map(|(field_key, value)| (field_key == key).then_some(value.as_str()));
        let value = values.next();
        if values.next().is_some() {
            Err(())
        } else {
            Ok(value)
        }
    }

    fn single_bool_field(fields: &[(String, String)], key: &str) -> Option<bool> {
        match single_field_value(fields, key)? {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }

    fn single_usize_field(fields: &[(String, String)], key: &str) -> Option<usize> {
        single_field_value(fields, key)?.parse().ok()
    }

    fn optional_single_usize_field(
        fields: &[(String, String)],
        key: &str,
    ) -> Option<Option<usize>> {
        match single_optional_field_value(fields, key).ok()? {
            Some(value) => Some(Some(value.parse().ok()?)),
            None => Some(None),
        }
    }

    fn optional_existing_count_is_not_calculated(
        fields: &[(String, String)],
        key: &str,
    ) -> Option<Option<bool>> {
        match single_optional_field_value(fields, key).ok()? {
            Some(value) => Some(Some(value == "not-calculated")),
            None => Some(None),
        }
    }
}
mod summary_fields {
    pub(super) fn field_value<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
        fields
            .iter()
            .find_map(|(field_key, value)| (field_key == key).then_some(value.as_str()))
    }

    pub(super) fn bool_field(fields: &[(String, String)], key: &str) -> bool {
        optional_bool_field(fields, key).unwrap_or(false)
    }

    pub(super) fn usize_field(fields: &[(String, String)], key: &str) -> usize {
        optional_usize_field(fields, key).unwrap_or(0)
    }

    pub(super) fn optional_bool_field(fields: &[(String, String)], key: &str) -> Option<bool> {
        field_value(fields, key).and_then(|value| value.parse().ok())
    }

    pub(super) fn optional_usize_field(fields: &[(String, String)], key: &str) -> Option<usize> {
        field_value(fields, key).and_then(|value| value.parse().ok())
    }
}

pub use backend_report::BackendReport;
pub use build_variant_view::BuildVariantView;
pub use buildup_result::BuildUpResult;
pub use coverage_result::CoverageResult;
pub use coverage_row_view::CoverageRowView;
pub use objective_result::ObjectiveResult;
pub use packing_candidate_view::PackingCandidateView;
pub use packing_result::PackingResult;
pub use replay_trace::ReplayTrace;
pub use search_execution_report::SearchExecutionReport;
pub use solution_set_availability::SolutionSetAvailability;

#[cfg(test)]
#[path = "result_views_tests.rs"]
mod tests;
