// SRP rationale: this module has one change reason: read-only typed views over core execution results.
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

        pub(crate) fn checked_nested_retained_bytes(&self) -> Option<u128> {
            (self.backend_requested.capacity() as u128)
                .checked_add(self.backend_selected.capacity() as u128)?
                .checked_add(self.backend_fallback_reason.capacity() as u128)
        }

        pub(super) fn checked_clone_nested_bytes(&self) -> Option<u128> {
            (self.backend_requested.len() as u128)
                .checked_add(self.backend_selected.len() as u128)?
                .checked_add(self.backend_fallback_reason.len() as u128)
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

        pub(crate) fn checked_nested_retained_bytes(&self) -> Option<u128> {
            (self.variant_id.capacity() as u128)
                .checked_add(self.coverage_probability.capacity() as u128)?
                .checked_add(self.score_event_basis.capacity() as u128)
        }

        pub(super) fn checked_clone_nested_bytes(&self) -> Option<u128> {
            (self.variant_id.len() as u128)
                .checked_add(self.coverage_probability.len() as u128)?
                .checked_add(self.score_event_basis.len() as u128)
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

        pub(crate) fn checked_nested_retained_bytes(&self) -> Option<u128> {
            let mut bytes = (self.rows.capacity() as u128)
                .checked_mul(core::mem::size_of::<CoverageRowView>() as u128)?
                .checked_add(self.coverage_probability.capacity() as u128)?;
            for row in &self.rows {
                bytes = bytes.checked_add(row.checked_nested_retained_bytes()?)?;
            }
            Some(bytes)
        }

        pub(super) fn checked_clone_nested_bytes(&self) -> Option<u128> {
            let mut bytes = (self.rows.len() as u128)
                .checked_mul(core::mem::size_of::<CoverageRowView>() as u128)?
                .checked_add(self.coverage_probability.len() as u128)?;
            for row in &self.rows {
                bytes = bytes.checked_add(row.checked_clone_nested_bytes()?)?;
            }
            Some(bytes)
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

        pub(crate) fn checked_nested_retained_bytes(&self) -> Option<u128> {
            (self.row_id.capacity() as u128)
                .checked_add(self.coverage_probability.capacity() as u128)
        }

        pub(super) fn checked_clone_nested_bytes(&self) -> Option<u128> {
            (self.row_id.len() as u128).checked_add(self.coverage_probability.len() as u128)
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

        pub(crate) fn checked_nested_retained_bytes(&self) -> Option<u128> {
            Some(self.trace_retention_reason.capacity() as u128)
        }

        pub(super) fn checked_clone_nested_bytes(&self) -> Option<u128> {
            Some(self.trace_retention_reason.len() as u128)
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

        pub(crate) fn checked_nested_retained_bytes(&self) -> Option<u128> {
            Some(self.candidate_id.capacity() as u128)
        }

        pub(super) fn checked_clone_nested_bytes(&self) -> Option<u128> {
            Some(self.candidate_id.len() as u128)
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

        pub(crate) fn checked_nested_retained_bytes(&self) -> Option<u128> {
            let mut bytes = (self.candidates.capacity() as u128)
                .checked_mul(core::mem::size_of::<PackingCandidateView>() as u128)?;
            for candidate in &self.candidates {
                bytes = bytes.checked_add(candidate.checked_nested_retained_bytes()?)?;
            }
            Some(bytes)
        }

        pub(super) fn checked_clone_nested_bytes(&self) -> Option<u128> {
            let mut bytes = (self.candidates.len() as u128)
                .checked_mul(core::mem::size_of::<PackingCandidateView>() as u128)?;
            for candidate in &self.candidates {
                bytes = bytes.checked_add(candidate.checked_clone_nested_bytes()?)?;
            }
            Some(bytes)
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

        pub(crate) fn checked_nested_retained_bytes(&self) -> Option<u128> {
            (self.steps.capacity() as u128)
                .checked_mul(core::mem::size_of::<CorePathStep>() as u128)?
                .checked_add(self.trace_retention_reason.capacity() as u128)
        }

        pub(super) fn checked_clone_nested_bytes(&self) -> Option<u128> {
            (self.steps.len() as u128)
                .checked_mul(core::mem::size_of::<CorePathStep>() as u128)?
                .checked_add(self.trace_retention_reason.len() as u128)
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

    #[derive(Debug)]
    pub(crate) enum SearchExecutionReportBuildError<E> {
        ProjectionOverflow,
        AllocationFailed,
        MemoryGuard(E),
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

        /// Fallible counterpart used by memory-authorized terminal result
        /// projection. Every String allocation is reserved before it is
        /// populated; primitive report views remain allocation-free.
        pub(crate) fn try_from_summary_fields(
            fields: &[(String, String)],
            steps: Vec<CorePathStep>,
        ) -> Result<Self, ()> {
            Self::try_from_summary_fields_with_memory_guard(fields, steps, |_, _| Ok::<_, ()>(()))
                .map_err(|_| ())
        }

        /// Fallible terminal constructor that reports the cumulative actual
        /// String capacity plus the still-unallocated requested payload after
        /// every allocation. The caller can therefore re-authorize an early
        /// allocator overcapacity before the next report String is created.
        pub(crate) fn try_from_summary_fields_with_memory_guard<E>(
            fields: &[(String, String)],
            steps: Vec<CorePathStep>,
            mut memory_guard: impl FnMut(u128, u128) -> Result<(), E>,
        ) -> Result<Self, SearchExecutionReportBuildError<E>> {
            use super::summary_fields::{bool_field, field_value, usize_field};

            fn try_owned<E>(
                value: &str,
                actual_string_bytes: &mut u128,
                remaining_requested_bytes: &mut u128,
                memory_guard: &mut impl FnMut(u128, u128) -> Result<(), E>,
            ) -> Result<String, SearchExecutionReportBuildError<E>> {
                let mut owned = String::new();
                owned
                    .try_reserve_exact(value.len())
                    .map_err(|_| SearchExecutionReportBuildError::AllocationFailed)?;
                *remaining_requested_bytes = remaining_requested_bytes
                    .checked_sub(value.len() as u128)
                    .ok_or(SearchExecutionReportBuildError::ProjectionOverflow)?;
                *actual_string_bytes = actual_string_bytes
                    .checked_add(owned.capacity() as u128)
                    .ok_or(SearchExecutionReportBuildError::ProjectionOverflow)?;
                memory_guard(*actual_string_bytes, *remaining_requested_bytes)
                    .map_err(SearchExecutionReportBuildError::MemoryGuard)?;
                owned.push_str(value);
                Ok(owned)
            }

            let requested_value = field_value(fields, "backend_requested")
                .or_else(|| field_value(fields, "requested_backend"))
                .unwrap_or("none");
            let selected_value = field_value(fields, "backend_selected")
                .or_else(|| field_value(fields, "selected_backend"))
                .unwrap_or("none");
            let fallback_value = field_value(fields, "backend_fallback_reason").unwrap_or("none");
            let coverage_value = field_value(fields, "coverage_probability").unwrap_or("0.0");
            let trace_reason_value =
                field_value(fields, "trace_retention_reason").unwrap_or("none");
            let mut remaining_requested_bytes = [
                requested_value.len() as u128,
                selected_value.len() as u128,
                fallback_value.len() as u128,
                coverage_value.len() as u128,
                trace_reason_value.len() as u128,
                trace_reason_value.len() as u128,
            ]
            .into_iter()
            .try_fold(0_u128, u128::checked_add)
            .ok_or(SearchExecutionReportBuildError::ProjectionOverflow)?;
            let mut actual_string_bytes = 0_u128;
            let requested = try_owned(
                requested_value,
                &mut actual_string_bytes,
                &mut remaining_requested_bytes,
                &mut memory_guard,
            )?;
            let selected = try_owned(
                selected_value,
                &mut actual_string_bytes,
                &mut remaining_requested_bytes,
                &mut memory_guard,
            )?;
            let fallback = try_owned(
                fallback_value,
                &mut actual_string_bytes,
                &mut remaining_requested_bytes,
                &mut memory_guard,
            )?;
            let coverage = try_owned(
                coverage_value,
                &mut actual_string_bytes,
                &mut remaining_requested_bytes,
                &mut memory_guard,
            )?;
            let objective_trace_reason = try_owned(
                trace_reason_value,
                &mut actual_string_bytes,
                &mut remaining_requested_bytes,
                &mut memory_guard,
            )?;
            let replay_trace_reason = try_owned(
                trace_reason_value,
                &mut actual_string_bytes,
                &mut remaining_requested_bytes,
                &mut memory_guard,
            )?;
            debug_assert_eq!(remaining_requested_bytes, 0);

            Ok(Self::new(
                BackendReport::new(requested, selected, fallback),
                PackingResult::from_summary_fields(fields),
                BuildUpResult::from_summary_fields(fields),
                None,
                CoverageResult::new(
                    coverage,
                    usize_field(fields, "coverage_row_count"),
                    Vec::new(),
                ),
                ObjectiveResult::new(
                    usize_field(fields, "total_solution_count"),
                    usize_field(fields, "unique_solution_count"),
                    usize_field(fields, "retained_trace_count"),
                    bool_field(fields, "count_complete"),
                    bool_field(fields, "trace_retention_truncated"),
                    objective_trace_reason,
                ),
                ReplayTrace::new(
                    steps,
                    usize_field(fields, "retained_trace_count"),
                    bool_field(fields, "trace_retention_truncated"),
                    replay_trace_reason,
                ),
            )
            .with_solution_set_availability(SolutionSetAvailability::from_summary_fields(fields)))
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

        /// Heap bytes retained below this already-accounted report slot.
        pub(crate) fn checked_nested_retained_bytes(&self) -> Option<u128> {
            let mut bytes = self.backend_report.checked_nested_retained_bytes()?;
            bytes = bytes.checked_add(self.packing_result.checked_nested_retained_bytes()?)?;
            if let Some(variant) = &self.build_variant_view {
                bytes = bytes.checked_add(variant.checked_nested_retained_bytes()?)?;
            }
            bytes = bytes.checked_add(self.coverage_result.checked_nested_retained_bytes()?)?;
            bytes = bytes.checked_add(self.objective_result.checked_nested_retained_bytes()?)?;
            bytes = bytes.checked_add(self.replay_trace.checked_nested_retained_bytes()?)?;
            Some(bytes)
        }

        pub(crate) fn checked_clone_nested_bytes(&self) -> Option<u128> {
            let mut bytes = self.backend_report.checked_clone_nested_bytes()?;
            bytes = bytes.checked_add(self.packing_result.checked_clone_nested_bytes()?)?;
            if let Some(variant) = &self.build_variant_view {
                bytes = bytes.checked_add(variant.checked_clone_nested_bytes()?)?;
            }
            bytes = bytes.checked_add(self.coverage_result.checked_clone_nested_bytes()?)?;
            bytes = bytes.checked_add(self.objective_result.checked_clone_nested_bytes()?)?;
            bytes = bytes.checked_add(self.replay_trace.checked_clone_nested_bytes()?)?;
            Some(bytes)
        }

        pub(crate) fn checked_clone_peak_bytes(&self) -> Option<u128> {
            (core::mem::size_of::<Self>() as u128)
                .checked_add(self.checked_nested_retained_bytes()?)?
                .checked_add(core::mem::size_of::<Self>() as u128)?
                .checked_add(self.checked_clone_nested_bytes()?)
        }

        /// Allocation-free projection of the nested storage that
        /// `from_summary_fields` will request. The constructor only creates the
        /// six strings enumerated here and takes ownership of a path vector
        /// whose exact target length is supplied by the caller.
        pub(crate) fn checked_from_summary_fields_nested_bytes(
            fields: &[(String, String)],
            path_step_count: usize,
        ) -> Option<u128> {
            use super::summary_fields::field_value;

            let requested = field_value(fields, "backend_requested")
                .or_else(|| field_value(fields, "requested_backend"))
                .unwrap_or("none");
            let selected = field_value(fields, "backend_selected")
                .or_else(|| field_value(fields, "selected_backend"))
                .unwrap_or("none");
            let fallback = field_value(fields, "backend_fallback_reason").unwrap_or("none");
            let coverage = field_value(fields, "coverage_probability").unwrap_or("0.0");
            let trace_reason = field_value(fields, "trace_retention_reason").unwrap_or("none");

            Self::checked_from_summary_fields_nested_bytes_for_values(
                requested,
                selected,
                fallback,
                coverage,
                trace_reason,
                path_step_count,
            )
        }

        pub(crate) fn checked_from_summary_fields_nested_bytes_for_values(
            requested: &str,
            selected: &str,
            fallback: &str,
            coverage: &str,
            trace_reason: &str,
            path_step_count: usize,
        ) -> Option<u128> {
            checked_report_heap_bytes(
                path_step_count as u128,
                [
                    requested.len() as u128,
                    selected.len() as u128,
                    fallback.len() as u128,
                    coverage.len() as u128,
                    trace_reason.len() as u128,
                    trace_reason.len() as u128,
                ],
            )
        }

        #[cfg(test)]
        pub(crate) fn checked_layout_bytes_for_test(
            path_step_count: u128,
            string_bytes: impl IntoIterator<Item = u128>,
        ) -> Option<u128> {
            checked_report_heap_bytes(path_step_count, string_bytes)
        }
    }

    fn checked_report_heap_bytes(
        path_step_count: u128,
        string_bytes: impl IntoIterator<Item = u128>,
    ) -> Option<u128> {
        let mut bytes = path_step_count.checked_mul(core::mem::size_of::<CorePathStep>() as u128)?;
        for value in string_bytes {
            bytes = bytes.checked_add(value)?;
        }
        Some(bytes)
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
        } else if !matches!(
            policy,
            "summary" | "trace" | "tiling-only" | "coverage-rows"
        ) {
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
pub(crate) use search_execution_report::SearchExecutionReportBuildError;
pub use solution_set_availability::SolutionSetAvailability;

#[cfg(test)]
mod resource_projection_tests {
    use crate::core_execution_result::CorePathStep;
    use clearra_core_domain::piece::piece_kind::PieceKind;

    use super::{
        BackendReport, BuildUpResult, BuildVariantView, CoverageResult, ObjectiveResult,
        PackingCandidateView, PackingResult, ReplayTrace, SearchExecutionReport,
    };

    fn reserved(value: &str, capacity: usize) -> String {
        let mut output = String::with_capacity(capacity);
        output.push_str(value);
        output
    }

    #[test]
    fn report_projection_counts_capacity_and_clone_peak_fieldwise() {
        let backend = BackendReport::new(
            reserved("cpu", 19),
            reserved("cpu", 23),
            reserved("none", 29),
        );
        let mut candidates = Vec::with_capacity(4);
        candidates.push(PackingCandidateView::new(reserved("c", 31), 1, 1));
        let packing = PackingResult::new(1, candidates);
        let variant = BuildVariantView::new(reserved("v", 37), reserved("1", 41))
            .with_score_event_basis(reserved("spin", 43));
        let coverage = CoverageResult::new(reserved("0.5", 47), 0, Vec::new());
        let objective = ObjectiveResult::new(0, 0, 0, false, false, reserved("none", 53));
        let mut steps = Vec::with_capacity(3);
        steps.push(CorePathStep::new(PieceKind::T, 0, 0, 0, "none", 0));
        let replay = ReplayTrace::new(steps, 1, false, reserved("none", 59));
        let report = SearchExecutionReport::new(
            backend,
            packing,
            BuildUpResult::new(false, 0, 0, false),
            Some(variant),
            coverage,
            objective,
            replay,
        );

        let retained = 19_u128
            + 23
            + 29
            + (4 * core::mem::size_of::<PackingCandidateView>()) as u128
            + 31
            + 37
            + 41
            + 43
            + 47
            + 53
            + (3 * core::mem::size_of::<CorePathStep>()) as u128
            + 59;
        let clone_nested = 3_u128
            + 3
            + 4
            + core::mem::size_of::<PackingCandidateView>() as u128
            + 1
            + 1
            + 1
            + 4
            + 3
            + 4
            + core::mem::size_of::<CorePathStep>() as u128
            + 4;
        assert_eq!(report.checked_nested_retained_bytes(), Some(retained));
        assert_eq!(report.checked_clone_nested_bytes(), Some(clone_nested));
        assert_eq!(
            report.checked_clone_peak_bytes(),
            Some(
                (core::mem::size_of::<SearchExecutionReport>() as u128) * 2
                    + retained
                    + clone_nested
            )
        );
    }

    #[test]
    fn summary_rebuild_projection_matches_constructed_report_and_overflow_fails_closed() {
        let fields = vec![
            ("backend_requested".to_owned(), "cpu".to_owned()),
            ("backend_selected".to_owned(), "cpu".to_owned()),
            ("backend_fallback_reason".to_owned(), "none".to_owned()),
            ("coverage_probability".to_owned(), "0.5".to_owned()),
            ("trace_retention_reason".to_owned(), "kept".to_owned()),
        ];
        let steps = vec![CorePathStep::new(PieceKind::I, 0, 0, 0, "none", 0)];
        let projection =
            SearchExecutionReport::checked_from_summary_fields_nested_bytes(&fields, steps.len())
                .expect("checked summary projection");
        let report = SearchExecutionReport::from_summary_fields(&fields, steps);
        assert_eq!(report.checked_nested_retained_bytes(), Some(projection));
        assert_eq!(
            SearchExecutionReport::checked_layout_bytes_for_test(u128::MAX, [1]),
            None
        );
    }

    #[test]
    fn guarded_summary_rebuild_reauthorizes_each_actual_string_capacity() {
        let fields = vec![
            ("backend_requested".to_owned(), "requested-value".to_owned()),
            ("backend_selected".to_owned(), "selected-value".to_owned()),
            (
                "backend_fallback_reason".to_owned(),
                "fallback-value".to_owned(),
            ),
            ("coverage_probability".to_owned(), "0.875".to_owned()),
            (
                "trace_retention_reason".to_owned(),
                "retained-for-audit".to_owned(),
            ),
        ];
        let requested_lengths = [
            "requested-value".len() as u128,
            "selected-value".len() as u128,
            "fallback-value".len() as u128,
            "0.875".len() as u128,
            "retained-for-audit".len() as u128,
            "retained-for-audit".len() as u128,
        ];
        let requested_total = requested_lengths.iter().copied().sum::<u128>();
        let mut observations = Vec::new();
        let report = SearchExecutionReport::try_from_summary_fields_with_memory_guard(
            &fields,
            Vec::new(),
            |actual, remaining| {
                observations.push((actual, remaining));
                Ok::<_, ()>(())
            },
        )
        .expect("guarded report rebuild");
        assert_eq!(
            report.backend_report().backend_requested(),
            "requested-value"
        );
        assert_eq!(observations.len(), requested_lengths.len());

        let mut expected_remaining = requested_total;
        let mut prior_actual = 0_u128;
        for ((actual, remaining), requested) in observations
            .iter()
            .copied()
            .zip(requested_lengths.into_iter())
        {
            expected_remaining -= requested;
            assert_eq!(remaining, expected_remaining);
            assert!(actual >= prior_actual + requested);
            assert!(actual + remaining >= requested_total);
            prior_actual = actual;
        }
        assert_eq!(
            observations.last().map(|(_, remaining)| *remaining),
            Some(0)
        );

        let mut guard_calls = 0_usize;
        let error = SearchExecutionReport::try_from_summary_fields_with_memory_guard(
            &fields,
            Vec::new(),
            |_, _| {
                guard_calls += 1;
                (guard_calls < 3).then_some(()).ok_or("report-cap")
            },
        )
        .expect_err("third actual String guard must stop the constructor");
        assert!(matches!(
            error,
            super::SearchExecutionReportBuildError::MemoryGuard("report-cap")
        ));
        assert_eq!(guard_calls, 3);
    }
}

#[cfg(test)]
#[path = "result_views_tests.rs"]
mod tests;
