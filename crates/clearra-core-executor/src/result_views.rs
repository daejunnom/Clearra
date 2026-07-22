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
        PackingResult, ReplayTrace,
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
            }
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
}
mod summary_fields {
    pub(super) fn field_value<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
        fields
            .iter()
            .find_map(|(field_key, value)| (field_key == key).then_some(value.as_str()))
    }

    pub(super) fn bool_field(fields: &[(String, String)], key: &str) -> bool {
        field_value(fields, key)
            .and_then(|value| value.parse().ok())
            .unwrap_or(false)
    }

    pub(super) fn usize_field(fields: &[(String, String)], key: &str) -> usize {
        field_value(fields, key)
            .and_then(|value| value.parse().ok())
            .unwrap_or(0)
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

#[cfg(test)]
#[path = "result_views_tests.rs"]
mod tests;
