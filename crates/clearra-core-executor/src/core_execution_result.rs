// SRP rationale: this module has one change reason: the typed outcome contract emitted by one core execution.
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_domain::solution::normalized_tiling_solution::{
    StandardBoard64TilingIdentity, NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
    NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
};
use clearra_replay::{
    ExactScoringExecutionBatch, ReplayTrace as PostProcessReplayTrace, SpinCoverageExecutionBatch,
};
use std::sync::Arc;

use crate::{
    core_postprocess_execution::CorePostProcessExecution,
    core_postprocess_score_cell::CorePostProcessScoreCell,
    core_postprocess_spin_coverage::CorePostProcessSpinCoverage,
    finesse_report::FinesseReport,
    pc_chance_coverage_evidence::{
        DistributedPcChanceCoverageRows, PcChanceCoverageEvidence, PcScoreProblemEvidence,
    },
    result_views::{SearchExecutionReport, SearchExecutionReportBuildError},
    setup_finder_report::SetupFinderReport,
    solution_probability::{
        NormalizedSolutionCoverage, SolutionAverageScoreReport, SolutionCoverage,
        SolutionProbabilityReport,
    },
    solution_set_audit::{SolutionAuditCheckpoint, SolutionSetAuditReport},
    tiling_solution_store::TilingSolutionPageStore,
};

const MAX_EXACT_PATTERN_STORAGE_DEDUP_COMPARISONS: u128 = 4_194_304;
const PC_TILING_PUBLIC_INITIAL_PAGE_LIMIT: usize = 100;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoreResultFieldReplacementProjection {
    pub external_replacement_bytes: u128,
    pub replacement_field_backing_bytes: u128,
    pub path_clone_bytes: u128,
    pub rebuilt_report_bytes: u128,
    pub required_future_bytes: u128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoreResultFieldReplacementError<E> {
    ProjectionOverflow,
    AllocationFailed { required_future_bytes: u128 },
    MemoryGuard(E),
}

/// Unforgeable producer evidence for the memory admission that owns a complete
/// `pc.tiling` family. External callers may inspect the source, but only Core
/// producers can attach it to a result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcTilingMemoryAdmissionEvidence {
    NativeInternal,
    WasmTerminalAuthority,
}

/// Unforgeable evidence that the score-cell family was assembled by Core's
/// verified distributed result merger, not decoded from a worker/wire result.
///
/// The value is readable by App, but the only attaching method is crate
/// private. Public worker-partition setters deliberately clear this marker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcScoreDistributedMergeEvidence {
    WasmVerifiedMerger,
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
    pc_chance_coverage_evidence: Option<PcChanceCoverageEvidence>,
    distributed_pc_chance_coverage_rows: Option<DistributedPcChanceCoverageRows>,
    pc_score_problem_evidence: Option<PcScoreProblemEvidence>,
    solution_coverages: Vec<SolutionCoverage>,
    normalized_solution_coverages: Vec<NormalizedSolutionCoverage>,
    solution_probabilities: Vec<SolutionProbabilityReport>,
    solution_average_scores: Vec<SolutionAverageScoreReport>,
    exact_scoring_execution_batches: Vec<ExactScoringExecutionBatch>,
    spin_coverage_execution_batches: Vec<SpinCoverageExecutionBatch>,
    postprocess_score_cells: Vec<CorePostProcessScoreCell>,
    postprocess_score_cells_complete: bool,
    postprocess_score_profile_id: Option<String>,
    pc_score_distributed_merge_evidence: Option<PcScoreDistributedMergeEvidence>,
    postprocess_spin_coverages: Vec<CorePostProcessSpinCoverage>,
    setup_finder_report: Option<SetupFinderReport>,
    finesse_report: Option<FinesseReport>,
    tiling_solution_page_store: Option<Arc<TilingSolutionPageStore>>,
    pc_tiling_memory_admission_evidence: Option<PcTilingMemoryAdmissionEvidence>,
    pre_b2b_produced_solution_audit_checkpoint: Option<SolutionAuditCheckpoint>,
    pre_b2b_solution_audit_checkpoint: Option<SolutionAuditCheckpoint>,
    solution_set_audit_report: Option<SolutionSetAuditReport>,
}

impl CoreExecutionResult {
    pub fn new(fields: Vec<(String, String)>, path_steps: Vec<CorePathStep>) -> Self {
        let execution_report = SearchExecutionReport::from_summary_fields(&fields, path_steps);
        Self::from_fields_and_execution_report(fields, execution_report)
    }

    /// Fallible constructor for untrusted wire owners. The callback observes
    /// allocator-visible field/path/report capacity immediately after every
    /// reserve and before another report allocation, so an outer authority can
    /// account for sibling results and the borrowed wire at the same time.
    pub fn try_new_with_memory_guard<E>(
        fields: Vec<(String, String)>,
        path_steps: Vec<CorePathStep>,
        mut memory_guard: impl FnMut(u128, u128) -> Result<(), E>,
    ) -> Result<Self, CoreResultFieldReplacementError<E>> {
        let field_bytes = checked_field_storage_bytes(&fields, fields.capacity())
            .ok_or(CoreResultFieldReplacementError::ProjectionOverflow)?;
        let path_bytes = (path_steps.capacity() as u128)
            .checked_mul(core::mem::size_of::<CorePathStep>() as u128)
            .ok_or(CoreResultFieldReplacementError::ProjectionOverflow)?;
        let base_bytes = field_bytes
            .checked_add(path_bytes)
            .ok_or(CoreResultFieldReplacementError::ProjectionOverflow)?;
        let requested_report_bytes = checked_execution_report_string_bytes(&fields)
            .ok_or(CoreResultFieldReplacementError::ProjectionOverflow)?;
        memory_guard(base_bytes, requested_report_bytes)
            .map_err(CoreResultFieldReplacementError::MemoryGuard)?;

        let execution_report = SearchExecutionReport::try_from_summary_fields_with_memory_guard(
            &fields,
            path_steps,
            |actual_string_bytes, remaining_requested_bytes| {
                let actual_bytes = base_bytes
                    .checked_add(actual_string_bytes)
                    .ok_or(CoreResultFieldReplacementError::ProjectionOverflow)?;
                memory_guard(actual_bytes, remaining_requested_bytes)
                    .map_err(CoreResultFieldReplacementError::MemoryGuard)
            },
        )
        .map_err(|error| match error {
            SearchExecutionReportBuildError::ProjectionOverflow => {
                CoreResultFieldReplacementError::ProjectionOverflow
            }
            SearchExecutionReportBuildError::AllocationFailed => {
                CoreResultFieldReplacementError::AllocationFailed {
                    required_future_bytes: requested_report_bytes,
                }
            }
            SearchExecutionReportBuildError::MemoryGuard(error) => error,
        })?;
        Ok(Self::from_fields_and_execution_report(
            fields,
            execution_report,
        ))
    }

    fn from_fields_and_execution_report(
        fields: Vec<(String, String)>,
        execution_report: SearchExecutionReport,
    ) -> Self {
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
            pc_chance_coverage_evidence: None,
            distributed_pc_chance_coverage_rows: None,
            pc_score_problem_evidence: None,
            solution_coverages: Vec::new(),
            normalized_solution_coverages: Vec::new(),
            solution_probabilities: Vec::new(),
            solution_average_scores: Vec::new(),
            exact_scoring_execution_batches: Vec::new(),
            spin_coverage_execution_batches: Vec::new(),
            postprocess_score_cells: Vec::new(),
            postprocess_score_cells_complete: false,
            postprocess_score_profile_id: None,
            pc_score_distributed_merge_evidence: None,
            postprocess_spin_coverages: Vec::new(),
            setup_finder_report: None,
            finesse_report: None,
            tiling_solution_page_store: None,
            pc_tiling_memory_admission_evidence: None,
            pre_b2b_produced_solution_audit_checkpoint: None,
            pre_b2b_solution_audit_checkpoint: None,
            solution_set_audit_report: None,
        }
    }
}

fn checked_execution_report_string_bytes(fields: &[(String, String)]) -> Option<u128> {
    let value = |key: &str| field_value(fields, key);
    [
        value("backend_requested")
            .or_else(|| value("requested_backend"))
            .unwrap_or("none"),
        value("backend_selected")
            .or_else(|| value("selected_backend"))
            .unwrap_or("none"),
        value("backend_fallback_reason").unwrap_or("none"),
        value("coverage_probability").unwrap_or("0.0"),
        value("trace_retention_reason").unwrap_or("none"),
        value("trace_retention_reason").unwrap_or("none"),
    ]
    .into_iter()
    .try_fold(0_u128, |bytes, value| {
        bytes.checked_add(value.len() as u128)
    })
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

    pub(crate) fn with_pc_tiling_memory_admission_evidence(
        mut self,
        evidence: PcTilingMemoryAdmissionEvidence,
    ) -> Self {
        self.pc_tiling_memory_admission_evidence = Some(evidence);
        self
    }

    pub fn without_tiling_solution_page_store(mut self) -> Self {
        self.tiling_solution_page_store = None;
        self.pc_tiling_memory_admission_evidence = None;
        self
    }

    pub fn with_pre_b2b_solution_audit_checkpoint(
        mut self,
        checkpoint: SolutionAuditCheckpoint,
    ) -> Self {
        self.pre_b2b_solution_audit_checkpoint = Some(checkpoint);
        self
    }

    pub fn with_pre_b2b_produced_solution_audit_checkpoint(
        mut self,
        checkpoint: SolutionAuditCheckpoint,
    ) -> Self {
        self.pre_b2b_produced_solution_audit_checkpoint = Some(checkpoint);
        self
    }

    pub fn with_solution_set_audit_report(mut self, report: SolutionSetAuditReport) -> Self {
        self.solution_set_audit_report = Some(report);
        self
    }

    pub fn without_solution_set_audit_report(mut self) -> Self {
        self.solution_set_audit_report = None;
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

    pub(crate) fn with_pc_chance_coverage_evidence(
        self,
        evidence: PcChanceCoverageEvidence,
    ) -> Self {
        self.with_pc_chance_transient_evidence(evidence)
    }

    /// Reattaches an already constructed Core-owned transient batch without
    /// exposing any public evidence constructor.
    pub fn with_pc_chance_transient_evidence(mut self, evidence: PcChanceCoverageEvidence) -> Self {
        self.pc_chance_coverage_evidence = Some(evidence);
        self
    }

    /// Attaches decoded, untrusted distributed PC-chance rows. This transport
    /// is never product authority; the coordinator must validate and rebind it
    /// to the retained problem before constructing terminal chance evidence.
    pub fn with_distributed_pc_chance_coverage_rows(
        mut self,
        rows: DistributedPcChanceCoverageRows,
    ) -> Self {
        self.distributed_pc_chance_coverage_rows = Some(rows);
        self
    }

    pub(crate) fn with_pc_score_problem_evidence(
        mut self,
        evidence: Option<PcScoreProblemEvidence>,
    ) -> Self {
        self.pc_score_problem_evidence = evidence;
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
        self.pc_score_distributed_merge_evidence = None;
        self
    }

    /// Attaches the canonical merger-only provenance after every accepted
    /// worker partition has crossed Core's distributed verifier.
    pub(crate) fn with_verified_distributed_postprocess_score_cells(
        mut self,
        cells: Vec<CorePostProcessScoreCell>,
        complete: bool,
        profile_id: impl Into<String>,
    ) -> Self {
        self.postprocess_score_cells = cells;
        self.postprocess_score_cells_complete = complete;
        self.postprocess_score_profile_id = Some(profile_id.into());
        self.pc_score_distributed_merge_evidence =
            Some(PcScoreDistributedMergeEvidence::WasmVerifiedMerger);
        self
    }

    pub fn without_postprocess_score_cells(mut self) -> Self {
        self.postprocess_score_cells.clear();
        self.postprocess_score_cells_complete = false;
        self.postprocess_score_profile_id = None;
        self.pc_score_distributed_merge_evidence = None;
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
        self.pc_chance_coverage_evidence = None;
        self.distributed_pc_chance_coverage_rows = None;
        self.pc_score_problem_evidence = None;
        self.solution_coverages.clear();
        self.normalized_solution_coverages.clear();
        self.solution_probabilities.clear();
        self.solution_average_scores.clear();
        self.exact_scoring_execution_batches.clear();
        self.spin_coverage_execution_batches.clear();
        self.postprocess_score_cells.clear();
        self.postprocess_score_cells_complete = false;
        self.postprocess_score_profile_id = None;
        self.pc_score_distributed_merge_evidence = None;
        self.postprocess_spin_coverages.clear();
        self.tiling_solution_page_store = None;
        self.pc_tiling_memory_admission_evidence = None;
        self.pre_b2b_produced_solution_audit_checkpoint = None;
        self.pre_b2b_solution_audit_checkpoint = None;
        self.solution_set_audit_report = None;
        self = self.without_finesse_search_report();
        self.execution_report =
            SearchExecutionReport::from_summary_fields(&self.fields, Vec::new());
        self
    }

    /// Fallible terminal projection for a result that remains under a live
    /// shared execution-resource lease.
    ///
    /// The guard runs before the first output field/report allocation with a
    /// checked allocation projection, then runs again against the capacities
    /// actually returned by the allocator. Both measurements cover all heap
    /// bytes that coexist with this still-live source result. A private
    /// solution-set audit must be consumed or skipped by the typed caller
    /// first; carrying it into this public projection fails closed as an
    /// unavailable projection.
    pub fn try_into_fail_closed_public_solution_surface_with_memory_guard<E>(
        mut self,
        mut memory_guard: impl FnMut(&Self, u128) -> Result<(), E>,
    ) -> Result<Self, CoreResultFieldReplacementError<E>> {
        if self.solution_set_audit_report.is_some() {
            return Err(CoreResultFieldReplacementError::ProjectionOverflow);
        }
        let (field_count, required_future_bytes) = self
            .checked_fail_closed_public_surface_projection()
            .ok_or(CoreResultFieldReplacementError::ProjectionOverflow)?;
        memory_guard(&self, required_future_bytes)
            .map_err(CoreResultFieldReplacementError::MemoryGuard)?;

        let allocation_error = || CoreResultFieldReplacementError::AllocationFailed {
            required_future_bytes,
        };
        let mut fields = Vec::new();
        fields
            .try_reserve_exact(field_count)
            .map_err(|_| allocation_error())?;
        self.visit_fail_closed_solution_summary_fields(|key, value| {
            let mut owned_key = String::new();
            owned_key
                .try_reserve_exact(key.len())
                .map_err(|_| allocation_error())?;
            owned_key.push_str(key);
            let mut owned_value = String::new();
            owned_value
                .try_reserve_exact(value.len())
                .map_err(|_| allocation_error())?;
            owned_value.push_str(value);
            fields.push((owned_key, owned_value));
            Ok(())
        })?;
        debug_assert_eq!(fields.len(), field_count);
        let execution_report = SearchExecutionReport::try_from_summary_fields(&fields, Vec::new())
            .map_err(|_| allocation_error())?;
        let actual_field_bytes = checked_field_storage_bytes(&fields, fields.capacity())
            .ok_or(CoreResultFieldReplacementError::ProjectionOverflow)?;
        let actual_report_bytes = execution_report
            .checked_nested_retained_bytes()
            .ok_or(CoreResultFieldReplacementError::ProjectionOverflow)?;
        let actual_future_bytes = actual_field_bytes
            .checked_add(actual_report_bytes)
            .ok_or(CoreResultFieldReplacementError::ProjectionOverflow)?;
        memory_guard(&self, actual_future_bytes)
            .map_err(CoreResultFieldReplacementError::MemoryGuard)?;

        // Drop, rather than merely clear, every private/transient backing store
        // while the terminal authority is still live. This makes the returned
        // result's checked retained-byte report describe its physical owners.
        self.fields = fields;
        self.execution_report = execution_report;
        self.postprocess_replay_trace = None;
        self.postprocess_executions = Vec::new();
        self.postprocess_execution_complete = false;
        self.postprocess_pattern_weights = Vec::new();
        self.packing_candidate_keys = Vec::new();
        self.normalized_solution_keys = Vec::new();
        self.normalized_solution_identities = Vec::new();
        self.representative_solution_identity = None;
        self.pc_chance_coverage_evidence = None;
        self.distributed_pc_chance_coverage_rows = None;
        self.pc_score_problem_evidence = None;
        self.solution_coverages = Vec::new();
        self.normalized_solution_coverages = Vec::new();
        self.solution_probabilities = Vec::new();
        self.solution_average_scores = Vec::new();
        self.exact_scoring_execution_batches = Vec::new();
        self.spin_coverage_execution_batches = Vec::new();
        self.postprocess_score_cells = Vec::new();
        self.postprocess_score_cells_complete = false;
        self.postprocess_score_profile_id = None;
        self.pc_score_distributed_merge_evidence = None;
        self.postprocess_spin_coverages = Vec::new();
        self.tiling_solution_page_store = None;
        self.pc_tiling_memory_admission_evidence = None;
        self.pre_b2b_produced_solution_audit_checkpoint = None;
        self.pre_b2b_solution_audit_checkpoint = None;
        self.solution_set_audit_report = None;
        self = self.without_finesse_search_report();
        Ok(self)
    }

    fn checked_fail_closed_public_surface_projection(&self) -> Option<(usize, u128)> {
        let mut field_count = 0_usize;
        let mut string_bytes = 0_u128;
        self.visit_fail_closed_solution_summary_fields::<()>(|key, value| {
            field_count = field_count.checked_add(1).ok_or(())?;
            string_bytes = string_bytes
                .checked_add(key.len() as u128)
                .and_then(|bytes| bytes.checked_add(value.len() as u128))
                .ok_or(())?;
            Ok(())
        })
        .ok()?;
        let field_slots =
            (field_count as u128).checked_mul(core::mem::size_of::<(String, String)>() as u128)?;
        let requested = self
            .field("backend_requested")
            .or_else(|| self.field("requested_backend"))
            .unwrap_or("none");
        let selected = self
            .field("backend_selected")
            .or_else(|| self.field("selected_backend"))
            .unwrap_or("none");
        let fallback = self.field("backend_fallback_reason").unwrap_or("none");
        let coverage = self.field("coverage_probability").unwrap_or("0.0");
        let trace_reason = self.field("trace_retention_reason").unwrap_or("none");
        let report_bytes =
            SearchExecutionReport::checked_from_summary_fields_nested_bytes_for_values(
                requested,
                selected,
                fallback,
                coverage,
                trace_reason,
                0,
            )?;
        Some((
            field_count,
            field_slots
                .checked_add(string_bytes)?
                .checked_add(report_bytes)?,
        ))
    }

    fn visit_fail_closed_solution_summary_fields<E>(
        &self,
        mut visit: impl FnMut(&str, &str) -> Result<(), E>,
    ) -> Result<(), E> {
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
            for (key, value) in &self.fields {
                visit(key, value)?;
            }
            return Ok(());
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
            "b2b_preservation_witness_available",
            "b2b_preservation_witness_kind",
            "b2b_preservation_witness_candidate_key",
            "b2b_preservation_witness_pattern_index",
        ];
        for (key, value) in &self.fields {
            if !SOLUTION_AVAILABILITY_KEYS.contains(&key.as_str()) {
                visit(key, value)?;
            }
        }
        let search_output_policy = self
            .fields
            .iter()
            .filter(|(key, _)| key == "search_output_policy")
            .map(|(_, value)| value.as_str())
            .find(|value| *value == "coverage-summary")
            .or_else(|| {
                self.field("search_output_policy").filter(|value| {
                    matches!(
                        *value,
                        "summary" | "trace" | "tiling-only" | "coverage-rows"
                    )
                })
            });
        if let Some(policy) = search_output_policy {
            visit("search_output_policy", policy)?;
        }
        for (key, value) in [
            ("unique_solution_count", "not-calculated"),
            ("normalized_unique_solution_count", "not-calculated"),
            ("solution_count_calculated", "false"),
            ("solution_set_materialized", "false"),
            ("solution_keys_materialized_count", "0"),
            ("solution_keys_complete", "false"),
            ("solution_page_available", "false"),
            ("normalized_solution_set_hash", "not-calculated"),
            ("actual_normalized_solution_set_hash", "not-calculated"),
        ] {
            visit(key, value)?;
        }
        if self
            .fields
            .iter()
            .any(|(key, _)| key == "b2b_preservation_witness_available")
        {
            for (key, value) in [
                ("b2b_preservation_witness_available", "false"),
                ("b2b_preservation_witness_kind", "not-materialized"),
                ("b2b_preservation_witness_candidate_key", "not-materialized"),
                ("b2b_preservation_witness_pattern_index", "not-materialized"),
            ] {
                visit(key, value)?;
            }
        }
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
                visit(key, "not-calculated")?;
            }
        }
        Ok(())
    }

    /// Separates product-private PC chance authority from the ordinary result.
    ///
    /// The returned result no longer owns the transient evidence. Callers must
    /// validate or drop the separately returned owner before public projection.
    pub fn into_pc_chance_transient_parts(mut self) -> (Self, Option<PcChanceCoverageEvidence>) {
        let evidence = self.pc_chance_coverage_evidence.take();
        self.distributed_pc_chance_coverage_rows = None;
        (self, evidence)
    }

    pub fn without_pc_chance_transient_evidence(mut self) -> Self {
        self.pc_chance_coverage_evidence = None;
        self.distributed_pc_chance_coverage_rows = None;
        self
    }

    /// Removes every producer-owned score execution input after a typed score
    /// result has consumed and validated it. User-facing score summaries and
    /// per-solution averages remain available; replay graphs, distributed
    /// score cells, and their completeness/profile authorities do not cross
    /// the product response boundary.
    pub fn without_pc_score_transient_evidence(mut self) -> Self {
        self.pc_score_problem_evidence = None;
        self.postprocess_replay_trace = None;
        self.postprocess_executions.clear();
        self.postprocess_execution_complete = false;
        self.postprocess_pattern_weights.clear();
        self.exact_scoring_execution_batches.clear();
        self.spin_coverage_execution_batches.clear();
        self.postprocess_score_cells.clear();
        self.postprocess_score_cells_complete = false;
        self.postprocess_score_profile_id = None;
        self.pc_score_distributed_merge_evidence = None;
        self.postprocess_spin_coverages.clear();
        self
    }

    /// Removes only the executed-problem owner retained by a scoring session.
    /// Generic score output keeps its established replay/score surface, but no
    /// producer-private problem snapshot may cross the App boundary.
    pub fn without_pc_score_problem_evidence(mut self) -> Self {
        self.pc_score_problem_evidence = None;
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
        self.fields
            .retain(|(key, _)| !fields.iter().any(|(replacement, _)| replacement == key));
        self.fields.extend(fields);
        let path_steps = self.path_steps().to_vec();
        self.execution_report =
            SearchExecutionReport::from_summary_fields(&self.fields, path_steps);
        self
    }

    #[cfg(test)]
    pub(crate) fn without_field_for_test(mut self, field: &str) -> Self {
        self.fields.retain(|(key, _)| key != field);
        let path_steps = self.path_steps().to_vec();
        self.execution_report =
            SearchExecutionReport::from_summary_fields(&self.fields, path_steps);
        self
    }

    /// Replaces summary fields while the caller's shared memory authority is
    /// consulted before the first internal reserve, path clone, or report
    /// string allocation. The supplied replacement strings already exist at
    /// entry and are therefore included as external live bytes in the peak.
    pub fn try_with_replaced_fields_with_memory_guard<E>(
        mut self,
        fields: Vec<(String, String)>,
        mut memory_guard: impl FnMut(&Self, u128) -> Result<(), E>,
    ) -> Result<Self, CoreResultFieldReplacementError<E>> {
        let projection = self
            .checked_field_replacement_projection(&fields)
            .ok_or(CoreResultFieldReplacementError::ProjectionOverflow)?;
        memory_guard(&self, projection.required_future_bytes)
            .map_err(CoreResultFieldReplacementError::MemoryGuard)?;

        self.fields
            .retain(|(key, _)| !fields.iter().any(|(replacement, _)| replacement == key));
        let target_len = self
            .fields
            .len()
            .checked_add(fields.len())
            .ok_or(CoreResultFieldReplacementError::ProjectionOverflow)?;
        if self.fields.capacity() < target_len {
            self.fields
                .try_reserve_exact(target_len.saturating_sub(self.fields.len()))
                .map_err(|_| CoreResultFieldReplacementError::AllocationFailed {
                    required_future_bytes: projection.required_future_bytes,
                })?;
        }

        // `try_reserve_exact` may return more capacity than requested. While
        // the external replacement owners and the old execution report are
        // still live, re-authorize the allocator's actual result-field backing
        // plus the report/path allocation that has not started yet.
        let post_reserve_future_bytes = projection
            .external_replacement_bytes
            .checked_add(projection.rebuilt_report_bytes)
            .ok_or(CoreResultFieldReplacementError::ProjectionOverflow)?;
        memory_guard(&self, post_reserve_future_bytes)
            .map_err(CoreResultFieldReplacementError::MemoryGuard)?;

        let path_step_count = self.path_steps().len();
        let mut path_steps = Vec::new();
        path_steps.try_reserve_exact(path_step_count).map_err(|_| {
            CoreResultFieldReplacementError::AllocationFailed {
                required_future_bytes: projection.required_future_bytes,
            }
        })?;
        path_steps.extend_from_slice(self.path_steps());
        let actual_path_bytes = (path_steps.capacity() as u128)
            .checked_mul(core::mem::size_of::<CorePathStep>() as u128)
            .ok_or(CoreResultFieldReplacementError::ProjectionOverflow)?;
        let requested_report_string_bytes = projection
            .rebuilt_report_bytes
            .checked_sub(projection.path_clone_bytes)
            .ok_or(CoreResultFieldReplacementError::ProjectionOverflow)?;
        let post_path_future_bytes = projection
            .external_replacement_bytes
            .checked_add(actual_path_bytes)
            .and_then(|bytes| bytes.checked_add(requested_report_string_bytes))
            .ok_or(CoreResultFieldReplacementError::ProjectionOverflow)?;
        memory_guard(&self, post_path_future_bytes)
            .map_err(CoreResultFieldReplacementError::MemoryGuard)?;
        self.fields.extend(fields);
        let execution_report = SearchExecutionReport::try_from_summary_fields_with_memory_guard(
            &self.fields,
            path_steps,
            |actual_string_bytes, remaining_requested_bytes| {
                let future_bytes = actual_path_bytes
                    .checked_add(actual_string_bytes)
                    .and_then(|bytes| bytes.checked_add(remaining_requested_bytes))
                    .ok_or(CoreResultFieldReplacementError::ProjectionOverflow)?;
                memory_guard(&self, future_bytes)
                    .map_err(CoreResultFieldReplacementError::MemoryGuard)
            },
        )
        .map_err(|error| match error {
            SearchExecutionReportBuildError::ProjectionOverflow => {
                CoreResultFieldReplacementError::ProjectionOverflow
            }
            SearchExecutionReportBuildError::AllocationFailed => {
                CoreResultFieldReplacementError::AllocationFailed {
                    required_future_bytes: projection.required_future_bytes,
                }
            }
            SearchExecutionReportBuildError::MemoryGuard(error) => error,
        })?;
        let actual_report_bytes = execution_report
            .checked_nested_retained_bytes()
            .ok_or(CoreResultFieldReplacementError::ProjectionOverflow)?;
        // `self` now exposes the allocator's actual final field backing while
        // still retaining the old report; the local report is the only future
        // owner. Guard that real coexistence before publishing it.
        memory_guard(&self, actual_report_bytes)
            .map_err(CoreResultFieldReplacementError::MemoryGuard)?;
        self.execution_report = execution_report;
        Ok(self)
    }

    pub fn checked_field_replacement_projection(
        &self,
        fields: &Vec<(String, String)>,
    ) -> Option<CoreResultFieldReplacementProjection> {
        let external_replacement_bytes = checked_field_storage_bytes(fields, fields.capacity())?;
        let survivor_count = self
            .fields
            .iter()
            .filter(|(key, _)| !fields.iter().any(|(replacement, _)| replacement == key))
            .count();
        let target_len = survivor_count.checked_add(fields.len())?;
        let replacement_field_backing_bytes = if self.fields.capacity() < target_len {
            (target_len as u128).checked_mul(core::mem::size_of::<(String, String)>() as u128)?
        } else {
            0
        };
        let path_clone_bytes = (self.path_steps().len() as u128)
            .checked_mul(core::mem::size_of::<CorePathStep>() as u128)?;

        // Report values read from the final field set. Build an allocation-free
        // borrowed view in final lookup order without constructing a B-tree or
        // cloning any string. Replacement fields are searched last because the
        // result appends them after retained fields.
        let final_field_value = |key: &str| {
            self.fields
                .iter()
                .filter(|(existing, _)| {
                    !fields
                        .iter()
                        .any(|(replacement, _)| replacement == existing)
                })
                .chain(fields.iter())
                .find_map(|(field, value)| (field == key).then_some(value.as_str()))
        };
        let requested = final_field_value("backend_requested")
            .or_else(|| final_field_value("requested_backend"))
            .unwrap_or("none");
        let selected = final_field_value("backend_selected")
            .or_else(|| final_field_value("selected_backend"))
            .unwrap_or("none");
        let fallback = final_field_value("backend_fallback_reason").unwrap_or("none");
        let coverage = final_field_value("coverage_probability").unwrap_or("0.0");
        let trace_reason = final_field_value("trace_retention_reason").unwrap_or("none");
        let rebuilt_report_bytes = path_clone_bytes
            .checked_add(requested.len() as u128)?
            .checked_add(selected.len() as u128)?
            .checked_add(fallback.len() as u128)?
            .checked_add(coverage.len() as u128)?
            .checked_add(trace_reason.len() as u128)?
            .checked_add(trace_reason.len() as u128)?;
        debug_assert_eq!(
            rebuilt_report_bytes,
            SearchExecutionReport::checked_from_summary_fields_nested_bytes_for_values(
                requested,
                selected,
                fallback,
                coverage,
                trace_reason,
                self.path_steps().len(),
            )?
        );
        let required_future_bytes = external_replacement_bytes
            .checked_add(replacement_field_backing_bytes)?
            .checked_add(rebuilt_report_bytes)?;
        Some(CoreResultFieldReplacementProjection {
            external_replacement_bytes,
            replacement_field_backing_bytes,
            path_clone_bytes,
            rebuilt_report_bytes,
            required_future_bytes,
        })
    }

    /// Allocation-free replacement projection for callers that have not yet
    /// allocated the replacement `String` owners. String lengths are the exact
    /// requested payload bytes; the later owned projection remains authoritative
    /// if an allocator returns larger capacities.
    pub fn checked_borrowed_field_replacement_projection(
        &self,
        fields: &[(&str, &str)],
    ) -> Option<CoreResultFieldReplacementProjection> {
        let mut external_replacement_bytes =
            (fields.len() as u128).checked_mul(core::mem::size_of::<(String, String)>() as u128)?;
        for (key, value) in fields {
            external_replacement_bytes = external_replacement_bytes
                .checked_add(key.len() as u128)?
                .checked_add(value.len() as u128)?;
        }
        let survivor_count = self
            .fields
            .iter()
            .filter(|(key, _)| {
                !fields
                    .iter()
                    .any(|(replacement, _)| *replacement == key.as_str())
            })
            .count();
        let target_len = survivor_count.checked_add(fields.len())?;
        let replacement_field_backing_bytes = if self.fields.capacity() < target_len {
            (target_len as u128).checked_mul(core::mem::size_of::<(String, String)>() as u128)?
        } else {
            0
        };
        let path_clone_bytes = (self.path_steps().len() as u128)
            .checked_mul(core::mem::size_of::<CorePathStep>() as u128)?;
        let final_field_value = |key: &str| {
            self.fields
                .iter()
                .filter(|(existing, _)| {
                    !fields
                        .iter()
                        .any(|(replacement, _)| *replacement == existing.as_str())
                })
                .find_map(|(field, value)| (field == key).then_some(value.as_str()))
                .or_else(|| {
                    fields
                        .iter()
                        .find_map(|(field, value)| (*field == key).then_some(*value))
                })
        };
        let requested = final_field_value("backend_requested")
            .or_else(|| final_field_value("requested_backend"))
            .unwrap_or("none");
        let selected = final_field_value("backend_selected")
            .or_else(|| final_field_value("selected_backend"))
            .unwrap_or("none");
        let fallback = final_field_value("backend_fallback_reason").unwrap_or("none");
        let coverage = final_field_value("coverage_probability").unwrap_or("0.0");
        let trace_reason = final_field_value("trace_retention_reason").unwrap_or("none");
        let rebuilt_report_bytes =
            SearchExecutionReport::checked_from_summary_fields_nested_bytes_for_values(
                requested,
                selected,
                fallback,
                coverage,
                trace_reason,
                self.path_steps().len(),
            )?;
        debug_assert!(rebuilt_report_bytes >= path_clone_bytes);
        let required_future_bytes = external_replacement_bytes
            .checked_add(replacement_field_backing_bytes)?
            .checked_add(rebuilt_report_bytes)?;
        Some(CoreResultFieldReplacementProjection {
            external_replacement_bytes,
            replacement_field_backing_bytes,
            path_clone_bytes,
            rebuilt_report_bytes,
            required_future_bytes,
        })
    }
}
impl CoreExecutionResult {
    /// Number of summary-field entries, including duplicate keys.
    pub fn summary_field_count(&self) -> usize {
        self.fields.len()
    }

    /// Allocation-free borrowed view of every summary-field entry in producer order.
    /// Consumers that validate an exact field allowlist must still reject duplicate keys.
    pub fn summary_field_entries(
        &self,
    ) -> impl ExactSizeIterator<Item = (&str, &str)> + DoubleEndedIterator + '_ {
        self.fields
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    }

    pub fn summary_fields(&self) -> Vec<(String, String)> {
        self.fields.clone()
    }

    /// Checked retained storage used by the public Build/PC result surface and
    /// by guarded post-processing. Outer vector slots are counted from their
    /// actual capacities; nested string/bitset/spin-shard owners are added
    /// exactly once.
    pub fn checked_resource_retained_bytes(&self) -> Option<u128> {
        let mut bytes = core::mem::size_of::<Self>() as u128;
        bytes = bytes.checked_add(checked_field_storage_bytes(
            &self.fields,
            self.fields.capacity(),
        )?)?;
        bytes = bytes.checked_add(self.execution_report.checked_nested_retained_bytes()?)?;
        if let Some(replay_trace) = &self.postprocess_replay_trace {
            bytes = bytes.checked_add(replay_trace.checked_nested_retained_bytes()?)?;
        }
        bytes = bytes.checked_add(checked_string_vec_storage_bytes(
            &self.postprocess_pattern_weights,
        )?)?;
        bytes = bytes.checked_add(checked_string_vec_storage_bytes(
            &self.packing_candidate_keys,
        )?)?;
        bytes = bytes.checked_add(checked_string_vec_storage_bytes(
            &self.normalized_solution_keys,
        )?)?;
        bytes = bytes.checked_add(
            (self.normalized_solution_identities.capacity() as u128)
                .checked_mul(core::mem::size_of::<StandardBoard64TilingIdentity>() as u128)?,
        )?;
        bytes = bytes.checked_add(
            (self.coverage_pattern_words.capacity() as u128)
                .checked_mul(core::mem::size_of::<u64>() as u128)?,
        )?;
        if let Some(evidence) = &self.pc_chance_coverage_evidence {
            bytes = bytes.checked_add(evidence.checked_non_pattern_storage_retained_bytes()?)?;
        }
        if let Some(rows) = &self.distributed_pc_chance_coverage_rows {
            bytes = bytes.checked_add(rows.checked_non_pattern_storage_retained_bytes()?)?;
        }
        if let Some(evidence) = &self.pc_score_problem_evidence {
            bytes = bytes.checked_add(evidence.checked_storage_retained_bytes()?)?;
        }

        bytes = bytes.checked_add(
            (self.solution_coverages.capacity() as u128)
                .checked_mul(core::mem::size_of::<SolutionCoverage>() as u128)?,
        )?;
        for coverage in &self.solution_coverages {
            bytes = bytes.checked_add(coverage.checked_non_pattern_storage_retained_bytes()?)?;
        }
        bytes = bytes.checked_add(
            (self.normalized_solution_coverages.capacity() as u128)
                .checked_mul(core::mem::size_of::<NormalizedSolutionCoverage>() as u128)?,
        )?;
        for coverage in &self.normalized_solution_coverages {
            bytes = bytes.checked_add(coverage.checked_non_pattern_storage_retained_bytes()?)?;
        }
        bytes = bytes.checked_add(self.checked_unique_result_pattern_storage_bytes()?)?;
        bytes = bytes.checked_add(
            (self.solution_probabilities.capacity() as u128)
                .checked_mul(core::mem::size_of::<SolutionProbabilityReport>() as u128)?,
        )?;
        for report in &self.solution_probabilities {
            bytes = bytes.checked_add(report.checked_nested_retained_bytes()?)?;
        }
        bytes = bytes.checked_add(
            (self.solution_average_scores.capacity() as u128)
                .checked_mul(core::mem::size_of::<SolutionAverageScoreReport>() as u128)?,
        )?;
        for report in &self.solution_average_scores {
            bytes = bytes.checked_add(report.checked_nested_retained_bytes()?)?;
        }

        for (capacity, item_size) in [
            (
                self.postprocess_executions.capacity(),
                core::mem::size_of::<CorePostProcessExecution>(),
            ),
            (
                self.exact_scoring_execution_batches.capacity(),
                core::mem::size_of::<ExactScoringExecutionBatch>(),
            ),
            (
                self.spin_coverage_execution_batches.capacity(),
                core::mem::size_of::<SpinCoverageExecutionBatch>(),
            ),
            (
                self.postprocess_score_cells.capacity(),
                core::mem::size_of::<CorePostProcessScoreCell>(),
            ),
        ] {
            bytes = bytes.checked_add((capacity as u128).checked_mul(item_size as u128)?)?;
        }
        for execution in &self.postprocess_executions {
            bytes = bytes.checked_add(execution.checked_nested_retained_bytes()?)?;
        }
        for batch in &self.exact_scoring_execution_batches {
            bytes = bytes.checked_add(batch.checked_nested_retained_bytes()?)?;
        }
        for batch in &self.spin_coverage_execution_batches {
            bytes = bytes.checked_add(batch.checked_nested_retained_bytes()?)?;
        }
        for cell in &self.postprocess_score_cells {
            bytes = bytes.checked_add(cell.checked_nested_retained_bytes()?)?;
        }
        if let Some(profile) = &self.postprocess_score_profile_id {
            bytes = bytes.checked_add(profile.capacity() as u128)?;
        }
        bytes = bytes.checked_add(
            (self.postprocess_spin_coverages.capacity() as u128)
                .checked_mul(core::mem::size_of::<CorePostProcessSpinCoverage>() as u128)?,
        )?;
        for coverage in &self.postprocess_spin_coverages {
            bytes = bytes.checked_add(coverage.checked_nested_retained_bytes()?)?;
        }
        if let Some(report) = &self.setup_finder_report {
            bytes = bytes.checked_add(report.checked_nested_retained_bytes()?)?;
        }
        if let Some(report) = &self.finesse_report {
            bytes = bytes.checked_add(report.checked_nested_retained_bytes()?)?;
        }
        if let Some(store) = &self.tiling_solution_page_store {
            bytes = bytes.checked_add(store.checked_owned_graph_retained_bytes()?)?;
        }
        for checkpoint in [
            &self.pre_b2b_produced_solution_audit_checkpoint,
            &self.pre_b2b_solution_audit_checkpoint,
        ]
        .into_iter()
        .flatten()
        {
            bytes = bytes.checked_add(checkpoint.checked_nested_retained_bytes()?)?;
        }
        if let Some(report) = &self.solution_set_audit_report {
            bytes = bytes.checked_add(report.checked_non_pattern_storage_retained_bytes()?)?;
        }
        Some(bytes)
    }

    fn coverage_pattern_bitsets(
        &self,
    ) -> impl Iterator<Item = &clearra_coverage::pattern::pattern_bitset::PatternBitSet> {
        self.solution_coverages
            .iter()
            .map(SolutionCoverage::covered_patterns)
            .chain(
                self.normalized_solution_coverages
                    .iter()
                    .map(NormalizedSolutionCoverage::covered_patterns),
            )
            .chain(
                self.pc_chance_coverage_evidence
                    .iter()
                    .flat_map(|evidence| evidence.rows().iter())
                    .map(|row| row.coverage_bits()),
            )
            .chain(
                self.distributed_pc_chance_coverage_rows
                    .iter()
                    .flat_map(|transport| transport.rows().iter())
                    .map(|row| row.coverage_bits()),
            )
    }

    fn result_pattern_storage_components(
        &self,
    ) -> impl Iterator<Item = clearra_coverage::pattern::pattern_bitset::PatternBitSetStorageComponent>
           + '_ {
        self.coverage_pattern_bitsets()
            .flat_map(|bitset| {
                (0..bitset.storage_component_count()).map(move |index| {
                    bitset
                        .storage_component(index)
                        .expect("component index is bounded by the owner count")
                })
            })
            .chain(
                self.solution_set_audit_report
                    .iter()
                    .flat_map(|report| report.pattern_storage_components()),
            )
    }

    /// Allocation-free, pointer-exact deduplication for coverage backing.
    /// Work is capped so adversarial public result graphs fail closed instead
    /// of turning resource admission itself into an unbounded CPU operation.
    fn checked_unique_result_pattern_storage_bytes(&self) -> Option<u128> {
        self.checked_unique_result_pattern_storage_bytes_with_limit(
            MAX_EXACT_PATTERN_STORAGE_DEDUP_COMPARISONS,
        )
    }

    fn checked_unique_result_pattern_storage_bytes_with_limit(
        &self,
        max_comparisons: u128,
    ) -> Option<u128> {
        let mut bytes = 0_u128;
        let mut comparisons = 0_u128;
        for (index, current) in self.result_pattern_storage_components().enumerate() {
            let mut seen = false;
            for prior in self.result_pattern_storage_components().take(index) {
                comparisons = comparisons.checked_add(1)?;
                if comparisons > max_comparisons {
                    return None;
                }
                if current == prior {
                    seen = true;
                    break;
                }
            }
            if !seen {
                bytes = bytes.checked_add(current.retained_bytes())?;
            }
        }
        Some(bytes)
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
        let had_b2b_witness_contract = self
            .fields
            .iter()
            .any(|(key, _)| key == "b2b_preservation_witness_available");
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
            "b2b_preservation_witness_available",
            "b2b_preservation_witness_kind",
            "b2b_preservation_witness_candidate_key",
            "b2b_preservation_witness_pattern_index",
        ];
        let search_output_policy = self
            .fields
            .iter()
            .filter(|(key, _)| key == "search_output_policy")
            .map(|(_, value)| value.as_str())
            .find(|value| *value == "coverage-summary")
            .or_else(|| {
                self.field("search_output_policy").filter(|value| {
                    matches!(
                        *value,
                        "summary" | "trace" | "tiling-only" | "coverage-rows"
                    )
                })
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
        if had_b2b_witness_contract {
            fields.extend([
                (
                    "b2b_preservation_witness_available".to_owned(),
                    "false".to_owned(),
                ),
                (
                    "b2b_preservation_witness_kind".to_owned(),
                    "not-materialized".to_owned(),
                ),
                (
                    "b2b_preservation_witness_candidate_key".to_owned(),
                    "not-materialized".to_owned(),
                ),
                (
                    "b2b_preservation_witness_pattern_index".to_owned(),
                    "not-materialized".to_owned(),
                ),
            ]);
        }
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
        if let Some(report) = &self.solution_set_audit_report {
            let redacted = report.redacted_summary_fields();
            let redacted_keys = redacted
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            fields.retain(|(key, _)| !redacted_keys.contains(key.as_str()));
            fields.extend(redacted);
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

    pub fn pc_tiling_memory_admission_evidence(&self) -> Option<PcTilingMemoryAdmissionEvidence> {
        self.pc_tiling_memory_admission_evidence
    }

    /// Validates the complete positive identity required before a retained
    /// geometry-family owner may cross a public host boundary.
    ///
    /// Summary booleans alone are not authority: the result must also carry
    /// producer-only memory-admission evidence, and every count, hash, initial
    /// page, and completeness marker must agree with the immutable store.
    pub fn pc_tiling_family_publication_contract_is_valid(&self) -> bool {
        let Some(_memory_admission) = self.pc_tiling_memory_admission_evidence else {
            return false;
        };
        let Some(store) = self.tiling_solution_page_store.as_deref() else {
            return false;
        };
        let unique_bool = |key: &str| {
            self.unique_field(key)
                .and_then(|value| value.parse::<bool>().ok())
        };
        let unique_usize = |key: &str| {
            self.unique_field(key)
                .and_then(|value| value.parse::<usize>().ok())
        };
        let has_unique_value = |key: &str, expected: &str| self.unique_field(key) == Some(expected);

        if !matches!(
            self.unique_field("problem_preset"),
            Some("opening-pc") | Some("scenario-pc")
        ) || !has_unique_value("compiled_goal", "clear-to-empty")
            || !has_unique_value("search_output_policy", "tiling-only")
            || !has_unique_value("actual_solution_set_contract", "normalized-tiling-set")
            || !has_unique_value(
                "normalized_solution_key_algorithm",
                NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
            )
            || !has_unique_value(
                "normalized_solution_set_hash_algorithm",
                NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
            )
        {
            return false;
        }
        for key in [
            "packing_source_raw_geometry",
            "tiling_objective_canonical",
            "tiling_materialization_memory_admission_accounted",
            "tiling_materialization_complete",
            "tiling_family_complete",
            "tiling_initial_page_complete",
            "count_complete",
            "solution_count_calculated",
            "solution_set_materialized",
        ] {
            if unique_bool(key) != Some(true) {
                return false;
            }
        }
        for key in [
            "packing_source_buildability_preverified",
            "buildup_executed",
            "additional_buildup_executed",
            "buildability_verified",
            "coverage_calculated",
            "probability_calculated",
            "resource_truncated",
            "solution_probabilities_requested",
        ] {
            if unique_bool(key) != Some(false) {
                return false;
            }
        }
        for key in [
            "tiling_materialization_incomplete_reason",
            "tiling_family_incomplete_reason",
            "resource_truncation_reason",
            "count_truncated_reason",
        ] {
            if !has_unique_value(key, "none") {
                return false;
            }
        }

        let solution_count = store.len();
        let initial_page_count = solution_count.min(PC_TILING_PUBLIC_INITIAL_PAGE_LIMIT);
        let initial_page_covers_family = initial_page_count == solution_count;
        let page_available = initial_page_count < solution_count;
        let availability = self.execution_report.solution_set_availability();

        availability.contract_valid()
            && availability.uses_explicit_contract()
            && availability.solution_count_calculated()
            && availability.solution_set_materialized()
            && availability.solution_keys_materialized_count() == initial_page_count
            && availability.solution_keys_complete() == initial_page_covers_family
            && availability.solution_page_available() == page_available
            && availability.materialized_key_count_matches(self.normalized_solution_keys.len())
            && unique_usize("unique_solution_count") == Some(solution_count)
            && unique_usize("normalized_unique_solution_count") == Some(solution_count)
            && unique_usize("actual_normalized_unique_solution_count") == Some(solution_count)
            && unique_usize("total_solution_count") == Some(solution_count)
            && unique_usize("solution_keys_materialized_count") == Some(initial_page_count)
            && unique_usize("tiling_initial_page_count") == Some(initial_page_count)
            && unique_bool("solution_keys_complete") == Some(initial_page_covers_family)
            && unique_bool("solution_page_available") == Some(page_available)
            && unique_bool("tiling_initial_page_covers_family") == Some(initial_page_covers_family)
            && store.initial_page_keys_match(&self.normalized_solution_keys)
            && self.unique_field("normalized_solution_set_hash") == Some(store.normalized_hash())
            && self.unique_field("actual_normalized_solution_set_hash")
                == Some(store.normalized_hash())
    }

    pub fn pre_b2b_solution_audit_checkpoint(&self) -> Option<&SolutionAuditCheckpoint> {
        self.pre_b2b_solution_audit_checkpoint.as_ref()
    }

    pub fn pre_b2b_produced_solution_audit_checkpoint(&self) -> Option<&SolutionAuditCheckpoint> {
        self.pre_b2b_produced_solution_audit_checkpoint.as_ref()
    }

    pub fn solution_set_audit_report(&self) -> Option<&SolutionSetAuditReport> {
        self.solution_set_audit_report.as_ref()
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

    pub fn pc_chance_coverage_evidence(&self) -> Option<&PcChanceCoverageEvidence> {
        self.pc_chance_coverage_evidence.as_ref()
    }

    pub fn distributed_pc_chance_coverage_rows(&self) -> Option<&DistributedPcChanceCoverageRows> {
        self.distributed_pc_chance_coverage_rows.as_ref()
    }

    pub fn pc_score_problem_evidence(&self) -> Option<&PcScoreProblemEvidence> {
        self.pc_score_problem_evidence.as_ref()
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

    pub const fn pc_score_distributed_merge_evidence(
        &self,
    ) -> Option<PcScoreDistributedMergeEvidence> {
        self.pc_score_distributed_merge_evidence
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

    pub fn field_occurrence_count(&self, key: &str) -> usize {
        self.fields
            .iter()
            .filter(|(field_key, _)| field_key == key)
            .count()
    }

    pub fn unique_field(&self, key: &str) -> Option<&str> {
        let mut matches = self
            .fields
            .iter()
            .filter_map(|(field_key, value)| (field_key == key).then_some(value.as_str()));
        let value = matches.next()?;
        matches.next().is_none().then_some(value)
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

fn checked_field_storage_bytes(fields: &[(String, String)], outer_capacity: usize) -> Option<u128> {
    let mut bytes =
        (outer_capacity as u128).checked_mul(core::mem::size_of::<(String, String)>() as u128)?;
    for (key, value) in fields {
        bytes = bytes
            .checked_add(key.capacity() as u128)?
            .checked_add(value.capacity() as u128)?;
    }
    Some(bytes)
}

fn checked_string_vec_storage_bytes(values: &Vec<String>) -> Option<u128> {
    let mut bytes =
        (values.capacity() as u128).checked_mul(core::mem::size_of::<String>() as u128)?;
    for value in values {
        bytes = bytes.checked_add(value.capacity() as u128)?;
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use clearra_core_domain::{
        piece::piece_kind::PieceKind,
        solution::normalized_tiling_solution::{
            PiecePlacementMask, StandardBoard64TilingIdentity,
            NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
            NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
        },
    };
    use clearra_coverage::{
        pattern::pattern_bitset::PatternBitSet,
        row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
    };
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::{ProblemCompiler, SearchProblem};
    use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

    use crate::{
        pc_chance_coverage_evidence::PcChanceCoverageEvidence,
        solution_set_audit::{
            SolutionAuditCheckpoint, SolutionPortfolioSelectionPolicy, SolutionProductFamily,
            SolutionSetAuditInput, SolutionSetAuditReport,
        },
    };

    use super::{
        CoreExecutionResult, CorePathStep, CorePostProcessScoreCell, CorePostProcessSpinCoverage,
        CoreResultFieldReplacementError, FinesseReport, PcScoreDistributedMergeEvidence,
        PcTilingMemoryAdmissionEvidence, TilingSolutionPageStore,
    };

    fn reserved(value: &str, capacity: usize) -> String {
        let mut output = String::with_capacity(capacity);
        output.push_str(value);
        output
    }

    fn chance_problem() -> SearchProblem {
        let expression = QueuePatternExpression::parse("[IO]", 2).expect("two-pattern expression");
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0),
            PcQueueInput::pattern_expression(expression),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        ProblemCompiler::compile_scenario_pc(&query)
            .expect("chance evidence problem")
            .with_pc_chance_probability_v2_evidence()
    }

    fn chance_evidence(
        problem: &SearchProblem,
        rows: Vec<(u64, PatternBitSet)>,
        complete: bool,
    ) -> PcChanceCoverageEvidence {
        let source = problem.piece_source();
        let universe = source
            .materialized_universe()
            .expect("materialized chance universe");
        PcChanceCoverageEvidence::from_problem_rows(
            problem,
            rows.into_iter()
                .map(|(candidate_id, coverage)| {
                    CoverageRow::new_with_piece_source(
                        candidate_id,
                        CoverageRowKind::Build,
                        source.id().get(),
                        universe.pattern_universe_id(),
                        universe.pattern_weight_model_id(),
                        coverage,
                    )
                })
                .collect(),
            complete,
        )
        .expect("typed chance evidence")
    }

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
    fn distributed_score_marker_survives_clone_but_not_worker_or_public_boundaries() {
        let identity = StandardBoard64TilingIdentity::from_placements(
            0,
            [PiecePlacementMask::new(PieceKind::I, 0xf)],
        )
        .expect("one-piece identity");
        let cell = || CorePostProcessScoreCell::new(identity, 0, "trace", 100, 1);
        let verified = CoreExecutionResult::default()
            .with_verified_distributed_postprocess_score_cells(vec![cell()], true, "tetrio");
        assert_eq!(
            verified.pc_score_distributed_merge_evidence(),
            Some(PcScoreDistributedMergeEvidence::WasmVerifiedMerger)
        );
        assert_eq!(
            verified.clone().pc_score_distributed_merge_evidence(),
            verified.pc_score_distributed_merge_evidence()
        );

        let worker_or_wire =
            verified
                .clone()
                .with_postprocess_score_cells(vec![cell()], true, "tetrio");
        assert_eq!(worker_or_wire.pc_score_distributed_merge_evidence(), None);
        assert_eq!(
            verified
                .into_fail_closed_public_solution_surface()
                .pc_score_distributed_merge_evidence(),
            None
        );
    }

    fn synthetic_pageable_pc_tiling_result() -> CoreExecutionResult {
        let identities = (0..101_usize)
            .map(|index| {
                let low_variable_bit = 2 + index / 16;
                let high_variable_bit = 18 + index % 16;
                let mask = 0b11_u64 | (1_u64 << low_variable_bit) | (1_u64 << high_variable_bit);
                StandardBoard64TilingIdentity::from_placements(
                    0,
                    [PiecePlacementMask::new(PieceKind::T, mask)],
                )
                .expect("synthetic standard identity")
            })
            .collect::<Vec<_>>();
        let store = Arc::new(
            TilingSolutionPageStore::from_standard_identities(0, identities)
                .expect("synthetic complete page store"),
        );
        let initial_page_count = 100;
        let initial_page = store
            .page_keys(0, initial_page_count)
            .expect("synthetic initial page");
        let solution_count = store.len().to_string();
        let hash = store.normalized_hash().to_owned();
        CoreExecutionResult::new(
            vec![
                ("problem_preset".to_owned(), "opening-pc".to_owned()),
                ("compiled_goal".to_owned(), "clear-to-empty".to_owned()),
                ("search_output_policy".to_owned(), "tiling-only".to_owned()),
                (
                    "actual_solution_set_contract".to_owned(),
                    "normalized-tiling-set".to_owned(),
                ),
                (
                    "normalized_solution_key_algorithm".to_owned(),
                    NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM.to_owned(),
                ),
                (
                    "normalized_solution_set_hash_algorithm".to_owned(),
                    NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM.to_owned(),
                ),
                ("packing_source_raw_geometry".to_owned(), "true".to_owned()),
                ("tiling_objective_canonical".to_owned(), "true".to_owned()),
                (
                    "tiling_materialization_memory_admission_accounted".to_owned(),
                    "true".to_owned(),
                ),
                (
                    "tiling_materialization_complete".to_owned(),
                    "true".to_owned(),
                ),
                ("tiling_family_complete".to_owned(), "true".to_owned()),
                ("tiling_initial_page_complete".to_owned(), "true".to_owned()),
                ("count_complete".to_owned(), "true".to_owned()),
                ("solution_count_calculated".to_owned(), "true".to_owned()),
                ("solution_set_materialized".to_owned(), "true".to_owned()),
                (
                    "packing_source_buildability_preverified".to_owned(),
                    "false".to_owned(),
                ),
                ("buildup_executed".to_owned(), "false".to_owned()),
                ("additional_buildup_executed".to_owned(), "false".to_owned()),
                ("buildability_verified".to_owned(), "false".to_owned()),
                ("coverage_calculated".to_owned(), "false".to_owned()),
                ("probability_calculated".to_owned(), "false".to_owned()),
                ("resource_truncated".to_owned(), "false".to_owned()),
                (
                    "solution_probabilities_requested".to_owned(),
                    "false".to_owned(),
                ),
                (
                    "tiling_materialization_incomplete_reason".to_owned(),
                    "none".to_owned(),
                ),
                (
                    "tiling_family_incomplete_reason".to_owned(),
                    "none".to_owned(),
                ),
                ("resource_truncation_reason".to_owned(), "none".to_owned()),
                ("count_truncated_reason".to_owned(), "none".to_owned()),
                ("unique_solution_count".to_owned(), solution_count.clone()),
                (
                    "normalized_unique_solution_count".to_owned(),
                    solution_count.clone(),
                ),
                (
                    "actual_normalized_unique_solution_count".to_owned(),
                    solution_count.clone(),
                ),
                ("total_solution_count".to_owned(), solution_count),
                (
                    "solution_keys_materialized_count".to_owned(),
                    initial_page_count.to_string(),
                ),
                (
                    "tiling_initial_page_count".to_owned(),
                    initial_page_count.to_string(),
                ),
                ("solution_keys_complete".to_owned(), "false".to_owned()),
                ("solution_page_available".to_owned(), "true".to_owned()),
                (
                    "tiling_initial_page_covers_family".to_owned(),
                    "false".to_owned(),
                ),
                ("normalized_solution_set_hash".to_owned(), hash.clone()),
                ("actual_normalized_solution_set_hash".to_owned(), hash),
            ],
            Vec::new(),
        )
        .with_normalized_solution_keys(initial_page)
        .with_tiling_solution_page_store(store)
        .with_pc_tiling_memory_admission_evidence(
            PcTilingMemoryAdmissionEvidence::WasmTerminalAuthority,
        )
    }

    #[test]
    fn pc_tiling_publication_requires_unforgeable_evidence_and_exact_page_store_identity() {
        let result = synthetic_pageable_pc_tiling_result();

        assert_eq!(
            result.pc_tiling_memory_admission_evidence(),
            Some(PcTilingMemoryAdmissionEvidence::WasmTerminalAuthority)
        );
        assert!(result.pc_tiling_family_publication_contract_is_valid());
        assert_eq!(result.normalized_solution_keys().len(), 100);
        assert_eq!(
            result.tiling_solution_page_store().expect("store").len(),
            101
        );

        assert!(!result
            .clone()
            .with_replaced_fields(vec![(
                "search_output_policy".to_owned(),
                "trace".to_owned(),
            )])
            .pc_tiling_family_publication_contract_is_valid());
        assert!(!result
            .clone()
            .with_replaced_fields(vec![(
                "normalized_solution_set_hash".to_owned(),
                "cts1:forged".to_owned(),
            )])
            .pc_tiling_family_publication_contract_is_valid());

        let without_store = result.without_tiling_solution_page_store();
        assert!(without_store
            .pc_tiling_memory_admission_evidence()
            .is_none());
        assert!(!without_store.pc_tiling_family_publication_contract_is_valid());
    }

    #[test]
    fn malformed_tiling_policy_is_preserved_only_as_a_redacted_declared_policy() {
        let result = CoreExecutionResult::new(
            vec![
                ("search_output_policy".to_owned(), "tiling-only".to_owned()),
                ("unique_solution_count".to_owned(), "101".to_owned()),
            ],
            Vec::new(),
        );

        let fields = result.fail_closed_solution_summary_fields();
        let field = |key: &str| {
            fields
                .iter()
                .find_map(|(field_key, value)| (field_key == key).then_some(value.as_str()))
        };
        assert_eq!(field("search_output_policy"), Some("tiling-only"));
        assert_eq!(field("unique_solution_count"), Some("not-calculated"));
        assert_eq!(field("solution_count_calculated"), Some("false"));
        assert_eq!(field("solution_set_materialized"), Some("false"));
        assert_eq!(field("solution_page_available"), Some("false"));
    }

    #[test]
    fn public_fail_closed_surface_physically_removes_attached_solution_authority() {
        let chance_problem = chance_problem();
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
        .with_pre_b2b_produced_solution_audit_checkpoint(SolutionAuditCheckpoint::known(
            1,
            "private-produced-hash",
        ))
        .with_pre_b2b_solution_audit_checkpoint(SolutionAuditCheckpoint::known(
            1,
            "private-pre-b2b-hash",
        ))
        .with_solution_set_audit_report(SolutionSetAuditReport::unavailable(
            SolutionProductFamily::Pc,
            "synthetic-private-audit",
        ))
        .with_pc_chance_coverage_evidence(chance_evidence(&chance_problem, Vec::new(), true))
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
        assert!(public
            .pre_b2b_produced_solution_audit_checkpoint()
            .is_none());
        assert!(public.pre_b2b_solution_audit_checkpoint().is_none());
        assert!(public.solution_set_audit_report().is_none());
        assert!(public.pc_chance_coverage_evidence().is_none());
        assert_eq!(
            public.field("solution_portfolio_snapshot_id"),
            Some("not-materialized")
        );
        assert_eq!(
            public.field("solution_set_audit_private_authority"),
            Some("not-materialized")
        );
        assert!(public.finesse_report().is_none());
    }

    #[test]
    fn chance_transient_evidence_can_be_taken_or_explicitly_stripped_before_publication() {
        let problem = chance_problem();
        let result = CoreExecutionResult::default()
            .with_pc_chance_coverage_evidence(chance_evidence(&problem, Vec::new(), true));

        let (without_evidence, evidence) = result.into_pc_chance_transient_parts();
        assert!(without_evidence.pc_chance_coverage_evidence().is_none());
        assert!(evidence
            .as_ref()
            .is_some_and(|value| value.problem().matches_search_problem(&problem)));

        let reattached =
            without_evidence.with_pc_chance_coverage_evidence(evidence.expect("taken evidence"));
        assert!(reattached.pc_chance_coverage_evidence().is_some());
        assert!(reattached
            .without_pc_chance_transient_evidence()
            .pc_chance_coverage_evidence()
            .is_none());
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

    #[test]
    fn public_result_projection_counts_field_string_and_spin_payload_capacities() {
        let mut fields = Vec::with_capacity(5);
        fields.push((reserved("backend_requested", 31), reserved("cpu", 37)));
        let mut words = Vec::with_capacity(9);
        words.push(1_u64);
        let mut keys = Vec::with_capacity(7);
        keys.push(reserved("후보", 43));
        let spin =
            CorePostProcessSpinCoverage::new(reserved("대상", 41), 0, 64, words, keys, 1, true);
        let result = CoreExecutionResult::new(
            fields,
            vec![CorePathStep::new(PieceKind::T, 0, 0, 0, "none", 0)],
        )
        .with_postprocess_spin_coverages(vec![spin]);
        let retained = result
            .checked_resource_retained_bytes()
            .expect("checked public result storage");
        let mandatory = core::mem::size_of::<CoreExecutionResult>() as u128
            + (5 * core::mem::size_of::<(String, String)>()) as u128
            + 31
            + 37
            + core::mem::size_of::<CorePostProcessSpinCoverage>() as u128
            + 41
            + (9 * core::mem::size_of::<u64>()) as u128
            + (7 * core::mem::size_of::<String>()) as u128
            + 43;
        assert!(retained >= mandatory);
    }

    #[test]
    fn coverage_projection_counts_pointer_identical_sparse_storage_once() {
        let chance_problem = chance_problem();
        let shared = PatternBitSet::from_pattern_indices(2, vec![1])
            .expect("one pattern uses sparse storage");
        let result = CoreExecutionResult::default().with_normalized_solution_coverages(vec![
            crate::solution_probability::NormalizedSolutionCoverage::new("first", shared.clone()),
            crate::solution_probability::NormalizedSolutionCoverage::new("second", shared.clone()),
        ]);
        assert_eq!(
            result.checked_unique_result_pattern_storage_bytes(),
            shared.checked_storage_retained_bytes()
        );

        let distinct = CoreExecutionResult::default().with_normalized_solution_coverages(vec![
            crate::solution_probability::NormalizedSolutionCoverage::new(
                "first",
                PatternBitSet::all(64),
            ),
            crate::solution_probability::NormalizedSolutionCoverage::new(
                "second",
                PatternBitSet::all(64),
            ),
        ]);
        assert_eq!(
            distinct.checked_unique_result_pattern_storage_bytes_with_limit(0),
            None
        );

        let audit = SolutionSetAuditReport::analyze(SolutionSetAuditInput::new(
            SolutionProductFamily::BuildProbability,
            shared.clone(),
            SolutionPortfolioSelectionPolicy::EquivalentCoverageRepresentatives,
        ))
        .expect("empty audit input preserves required-pattern backing");
        let evidence = chance_evidence(&chance_problem, vec![(17, shared.clone())], true);
        let cross_owner = CoreExecutionResult::default()
            .with_normalized_solution_coverages(vec![
                crate::solution_probability::NormalizedSolutionCoverage::new(
                    "shared",
                    shared.clone(),
                ),
            ])
            .with_pc_chance_coverage_evidence(evidence)
            .with_solution_set_audit_report(audit);
        assert_eq!(
            cross_owner.checked_unique_result_pattern_storage_bytes(),
            shared.checked_storage_retained_bytes()
        );
        assert_eq!(cross_owner.clone(), cross_owner);
    }

    #[test]
    fn chance_evidence_resource_projection_counts_row_slots_and_unique_pattern_storage() {
        let chance_problem = chance_problem();
        let shared = PatternBitSet::from_pattern_indices(2, vec![1])
            .expect("one pattern uses sparse storage");
        let evidence = chance_evidence(&chance_problem, vec![(17, shared.clone())], true);
        let evidence_non_pattern_bytes = evidence
            .checked_non_pattern_storage_retained_bytes()
            .expect("problem and row storage");
        let base = CoreExecutionResult::default()
            .checked_resource_retained_bytes()
            .expect("base accounting");
        let retained = CoreExecutionResult::default()
            .with_pc_chance_coverage_evidence(evidence)
            .checked_resource_retained_bytes()
            .expect("evidence accounting");

        assert_eq!(
            retained - base,
            evidence_non_pattern_bytes
                + shared
                    .checked_storage_retained_bytes()
                    .expect("pattern storage")
        );
    }

    #[test]
    fn build_pc_resource_projection_field_inventory_is_exhaustive() {
        let result = CoreExecutionResult::default();
        let CoreExecutionResult {
            fields,
            execution_report,
            postprocess_replay_trace,
            postprocess_executions,
            postprocess_execution_complete,
            postprocess_pattern_weights,
            packing_candidate_keys,
            normalized_solution_keys,
            normalized_solution_identities,
            representative_solution_identity,
            coverage_pattern_words,
            pc_chance_coverage_evidence,
            distributed_pc_chance_coverage_rows,
            pc_score_problem_evidence,
            solution_coverages,
            normalized_solution_coverages,
            solution_probabilities,
            solution_average_scores,
            exact_scoring_execution_batches,
            spin_coverage_execution_batches,
            postprocess_score_cells,
            postprocess_score_cells_complete,
            postprocess_score_profile_id,
            pc_score_distributed_merge_evidence,
            postprocess_spin_coverages,
            setup_finder_report,
            finesse_report,
            tiling_solution_page_store,
            pc_tiling_memory_admission_evidence,
            pre_b2b_produced_solution_audit_checkpoint,
            pre_b2b_solution_audit_checkpoint,
            solution_set_audit_report,
        } = &result;

        // This tuple deliberately names every field without `..`. Adding a new
        // owner therefore requires an explicit projection decision. Primitive
        // values and inline identities live in `size_of::<CoreExecutionResult>()`;
        // Setup Finder has a separate execution family and is not admitted by
        // the Build/PC result contract guarded by this method.
        let inventory = (
            fields,
            execution_report,
            postprocess_replay_trace,
            postprocess_executions,
            postprocess_execution_complete,
            postprocess_pattern_weights,
            packing_candidate_keys,
            normalized_solution_keys,
            normalized_solution_identities,
            representative_solution_identity,
            coverage_pattern_words,
            pc_chance_coverage_evidence,
            distributed_pc_chance_coverage_rows,
            pc_score_problem_evidence,
            solution_coverages,
            normalized_solution_coverages,
            solution_probabilities,
            solution_average_scores,
            exact_scoring_execution_batches,
            spin_coverage_execution_batches,
            postprocess_score_cells,
            postprocess_score_cells_complete,
            postprocess_score_profile_id,
            pc_score_distributed_merge_evidence,
            postprocess_spin_coverages,
            setup_finder_report,
            finesse_report,
            tiling_solution_page_store,
            pc_tiling_memory_admission_evidence,
            pre_b2b_produced_solution_audit_checkpoint,
            pre_b2b_solution_audit_checkpoint,
            solution_set_audit_report,
        );
        assert!(inventory.0.is_empty());
        assert!(inventory.25.is_none());
    }

    #[test]
    fn guarded_field_replacement_rejects_one_byte_short_before_internal_growth() {
        let existing = vec![("keep".to_owned(), "old".to_owned())];
        let result = CoreExecutionResult::new(
            existing,
            vec![CorePathStep::new(PieceKind::I, 0, 0, 0, "none", 0)],
        );
        let replacements = vec![
            ("keep".to_owned(), "new".to_owned()),
            ("added".to_owned(), "value".to_owned()),
        ];
        let projection = result
            .checked_field_replacement_projection(&replacements)
            .expect("checked replacement projection");
        assert_eq!(
            projection.required_future_bytes,
            projection.external_replacement_bytes
                + projection.replacement_field_backing_bytes
                + projection.rebuilt_report_bytes
        );
        assert!(projection.path_clone_bytes > 0);
        let retained = result
            .checked_resource_retained_bytes()
            .expect("checked retained bytes");
        let cap = retained + projection.required_future_bytes - 1;
        let error = result
            .try_with_replaced_fields_with_memory_guard(replacements, |live, future| {
                let required = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .expect("checked guard input");
                (required <= cap).then_some(()).ok_or(required)
            })
            .expect_err("one-byte-short field rebuild must fail closed");
        assert!(matches!(
            error,
            CoreResultFieldReplacementError::MemoryGuard(required) if required == cap + 1
        ));
    }

    #[test]
    fn borrowed_field_projection_guards_before_replacement_strings_exist() {
        let result = CoreExecutionResult::new(
            vec![("backend_selected".to_owned(), "cpu".to_owned())],
            vec![CorePathStep::new(PieceKind::I, 0, 0, 0, "none", 0)],
        );
        let borrowed = [
            ("backend_fallback_used", "true"),
            ("backend_fallback_reason", "gpu_kernel_unavailable"),
        ];
        let projected = result
            .checked_borrowed_field_replacement_projection(&borrowed)
            .expect("borrowed projection");
        let owned = borrowed
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect::<Vec<_>>();
        let actual = result
            .checked_field_replacement_projection(&owned)
            .expect("owned projection");
        assert_eq!(projected, actual);
    }

    #[test]
    fn guarded_field_replacement_reauthorizes_actual_overcapacity_at_exact_peak() {
        fn fixture() -> (CoreExecutionResult, Vec<(String, String)>) {
            let result = CoreExecutionResult::new(
                vec![("keep".to_owned(), "old".to_owned())],
                vec![CorePathStep::new(PieceKind::I, 0, 0, 0, "none", 0)],
            );
            let mut key = String::with_capacity(127);
            key.push_str("added");
            let mut value = String::with_capacity(191);
            value.push_str("value");
            let mut replacements = Vec::with_capacity(13);
            replacements.push((key, value));
            (result, replacements)
        }

        let (projection_result, projection_fields) = fixture();
        let borrowed = [("added", "value")];
        let borrowed_projection = projection_result
            .checked_borrowed_field_replacement_projection(&borrowed)
            .expect("borrowed projection");
        let owned_projection = projection_result
            .checked_field_replacement_projection(&projection_fields)
            .expect("owned projection");
        assert!(
            owned_projection.external_replacement_bytes
                > borrowed_projection.external_replacement_bytes
        );

        let (dry_result, dry_fields) = fixture();
        let mut peak = 0_u128;
        let mut guard_calls = 0_usize;
        dry_result
            .try_with_replaced_fields_with_memory_guard(dry_fields, |live, future| {
                guard_calls += 1;
                let required = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .expect("checked guard input");
                peak = peak.max(required);
                Ok::<_, ()>(())
            })
            .expect("dry replacement");
        // Initial projection, actual field backing, actual path backing, each
        // of the six report Strings, and the completed report are all guarded.
        assert_eq!(guard_calls, 10);

        let (exact_result, exact_fields) = fixture();
        exact_result
            .try_with_replaced_fields_with_memory_guard(exact_fields, |live, future| {
                let required = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .expect("checked guard input");
                (required <= peak).then_some(()).ok_or(required)
            })
            .expect("exact actual-capacity peak");

        let (short_result, short_fields) = fixture();
        let error = short_result
            .try_with_replaced_fields_with_memory_guard(short_fields, |live, future| {
                let required = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .expect("checked guard input");
                (required < peak).then_some(()).ok_or(required)
            })
            .expect_err("peak minus one must fail closed");
        assert!(matches!(
            error,
            CoreResultFieldReplacementError::MemoryGuard(required) if required == peak
        ));
    }

    #[test]
    fn guarded_field_replacement_preserves_order_and_typed_report_semantics() {
        let result = CoreExecutionResult::new(
            vec![
                ("keep".to_owned(), "yes".to_owned()),
                ("backend_requested".to_owned(), "old".to_owned()),
            ],
            vec![CorePathStep::new(PieceKind::O, 0, 0, 0, "none", 0)],
        );
        let replaced = result
            .try_with_replaced_fields_with_memory_guard(
                vec![
                    ("backend_requested".to_owned(), "cpu".to_owned()),
                    ("backend_selected".to_owned(), "cpu".to_owned()),
                ],
                |_, _| Ok::<_, ()>(()),
            )
            .expect("guarded replacement");
        assert_eq!(replaced.field("keep"), Some("yes"));
        assert_eq!(replaced.field("backend_requested"), Some("cpu"));
        assert_eq!(
            replaced
                .execution_report()
                .backend_report()
                .backend_requested(),
            "cpu"
        );
        assert_eq!(replaced.path_steps().len(), 1);
        assert_eq!(
            replaced
                .summary_fields()
                .iter()
                .filter(|(key, _)| key == "backend_requested")
                .count(),
            1
        );
    }

    #[test]
    fn unique_field_distinguishes_absent_unique_and_duplicate_fields_without_allocation() {
        let result = CoreExecutionResult::new(
            vec![
                ("only".to_owned(), "one".to_owned()),
                ("duplicate".to_owned(), "first".to_owned()),
                ("duplicate".to_owned(), "second".to_owned()),
            ],
            Vec::new(),
        );

        assert_eq!(result.field_occurrence_count("missing"), 0);
        assert_eq!(result.unique_field("missing"), None);
        assert_eq!(result.field_occurrence_count("only"), 1);
        assert_eq!(result.unique_field("only"), Some("one"));
        assert_eq!(result.field_occurrence_count("duplicate"), 2);
        assert_eq!(result.unique_field("duplicate"), None);
        assert_eq!(result.field("duplicate"), Some("first"));
        assert_eq!(result.summary_field_count(), 3);
        assert_eq!(
            result.summary_field_entries().collect::<Vec<_>>(),
            vec![
                ("only", "one"),
                ("duplicate", "first"),
                ("duplicate", "second"),
            ]
        );
    }

    fn synthetic_coverage_summary_with_private_solution_storage() -> CoreExecutionResult {
        let mut packing_keys = Vec::with_capacity(8);
        packing_keys.push("private-packing".to_owned());
        let mut normalized_keys = Vec::with_capacity(8);
        normalized_keys.push("private-solution".to_owned());
        CoreExecutionResult::new(
            vec![
                (
                    "search_output_policy".to_owned(),
                    "coverage-summary".to_owned(),
                ),
                ("backend_requested".to_owned(), "cpu".to_owned()),
                ("backend_selected".to_owned(), "wasm-cpu".to_owned()),
                ("coverage_probability".to_owned(), "1".to_owned()),
                ("unique_solution_count".to_owned(), "1".to_owned()),
                (
                    "normalized_unique_solution_count".to_owned(),
                    "1".to_owned(),
                ),
                ("solution_count_calculated".to_owned(), "true".to_owned()),
                ("solution_set_materialized".to_owned(), "true".to_owned()),
                (
                    "solution_keys_materialized_count".to_owned(),
                    "1".to_owned(),
                ),
                ("solution_keys_complete".to_owned(), "true".to_owned()),
                ("solution_page_available".to_owned(), "false".to_owned()),
                (
                    "normalized_solution_set_hash".to_owned(),
                    "private-hash".to_owned(),
                ),
                (
                    "actual_normalized_solution_set_hash".to_owned(),
                    "private-hash".to_owned(),
                ),
            ],
            vec![CorePathStep::new(PieceKind::I, 0, 0, 0, "none", 0)],
        )
        .with_packing_candidate_keys(packing_keys)
        .with_normalized_solution_keys(normalized_keys)
    }

    #[test]
    fn guarded_fail_closed_surface_matches_legacy_fields_and_drops_private_backing() {
        let source = synthetic_coverage_summary_with_private_solution_storage();
        let expected = source.clone().into_fail_closed_public_solution_surface();
        let mut projected_future = 0_u128;
        let mut guard_calls = 0_usize;
        let guarded = source
            .try_into_fail_closed_public_solution_surface_with_memory_guard(|live, future| {
                guard_calls += 1;
                projected_future = future;
                let required = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .expect("checked terminal surface projection");
                assert!(required > future);
                Ok::<_, ()>(())
            })
            .expect("guarded terminal projection");

        assert!(projected_future > 0);
        assert_eq!(guard_calls, 2);
        assert_eq!(guarded.summary_fields(), expected.summary_fields());
        assert_eq!(guarded.packing_candidate_keys.capacity(), 0);
        assert_eq!(guarded.normalized_solution_keys.capacity(), 0);
        assert_eq!(guarded.normalized_solution_identities.capacity(), 0);
        assert_eq!(guarded.exact_scoring_execution_batches.capacity(), 0);
        assert!(guarded.path_steps().is_empty());
        assert_eq!(
            guarded
                .execution_report()
                .backend_report()
                .backend_selected(),
            "wasm-cpu"
        );
    }

    #[test]
    fn guarded_fail_closed_surface_rejects_one_byte_short_before_allocation() {
        let source = synthetic_coverage_summary_with_private_solution_storage();
        let retained = source
            .checked_resource_retained_bytes()
            .expect("checked source bytes");
        let mut projected_future = 0_u128;
        source
            .clone()
            .try_into_fail_closed_public_solution_surface_with_memory_guard(|_, future| {
                projected_future = future;
                Ok::<_, ()>(())
            })
            .expect("projection probe");
        let cap = retained.checked_add(projected_future).expect("checked cap") - 1;

        let error = source
            .try_into_fail_closed_public_solution_surface_with_memory_guard(|live, future| {
                let required = live
                    .checked_resource_retained_bytes()
                    .and_then(|bytes| bytes.checked_add(future))
                    .expect("checked required bytes");
                (required <= cap).then_some(()).ok_or(required)
            })
            .expect_err("one-byte-short surface must fail closed");
        assert!(matches!(
            error,
            CoreResultFieldReplacementError::MemoryGuard(required) if required == cap + 1
        ));
    }

    #[test]
    fn guarded_fail_closed_surface_rejects_unconsumed_private_audit() {
        let source = synthetic_coverage_summary_with_private_solution_storage()
            .with_solution_set_audit_report(SolutionSetAuditReport::unavailable(
                SolutionProductFamily::Pc,
                "private-audit-must-be-consumed",
            ));
        let error = source
            .try_into_fail_closed_public_solution_surface_with_memory_guard(|_, _| Ok::<_, ()>(()))
            .expect_err("private audit is outside guarded public projection");
        assert!(matches!(
            error,
            CoreResultFieldReplacementError::ProjectionOverflow
        ));
    }
}
