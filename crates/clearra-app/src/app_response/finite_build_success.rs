// SRP rationale: this module has one behavior-level change reason: constructing finite Build success responses while enforcing allocation and evidence invariants.

use core::fmt::Write as _;

use clearra_core_executor::{CoreExecutionError, CoreExecutionResult};
use clearra_host_contract::AppResult;
use clearra_host_contract::{
    AppCommandKind, BackendReport, CapabilityReport, ExecutionAvailabilityReason,
    ExecutionAvailabilityReport, ExecutionCompletenessState, ExecutionSurface,
    RenderCapabilityReport as HostRenderCapabilityReport, ResourceReport,
};
use clearra_validation::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        diagnostic_report::DiagnosticReport, suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

use crate::render::AppRenderModel;
use crate::{
    app_context::AppContext,
    app_request::AppOutputPolicy,
    resource_contract::{
        availability_from_truncation_reason, availability_reason_from_str,
        diagnostic_code_for_resource_reason,
    },
};

use super::{
    build_observed_probability_incomplete, build_resource_truncation_reason, AppResponse,
    GovernedAppResponse,
};
use super::{AppStatus, ExitCodeHint};

const ALLOCATION_FAILED: &str = "finite_build_app_response_allocation_failed";
const PROJECTION_OVERFLOW: &str = "finite_build_app_response_memory_projection_overflow";
const ACTUAL_AUTHORITY_MISMATCH: &str =
    "finite_build_app_response_actual_memory_authority_mismatch";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FiniteBuildMemoryPhase {
    ConstructionBaseline,
    AllocationRequested,
    AllocationActual,
    FinalizedResponse,
}

/// Inline owners at the finite Build construction boundary. `AppResponse`
/// stands in for the Core result and every response component that is moved
/// into it, while the eventual wrapper's memory-limit field remains a distinct
/// construction input. The pending report is a distinct source owner until
/// append. A diagnostic scratch owner is admitted only while a diagnostic is
/// actually being built, so diagnostic-free results do not pay a guessed
/// inline cost.
fn checked_finite_build_construction_inline_bytes() -> Option<u128> {
    (core::mem::size_of::<AppResponse>() as u128)
        .checked_add(core::mem::size_of::<AppContext>() as u128)?
        .checked_add(core::mem::size_of::<AppOutputPolicy>() as u128)?
        .checked_add(core::mem::size_of::<Option<u128>>() as u128)?
        .checked_add(core::mem::size_of::<DiagnosticReport>() as u128)
}

struct FiniteBuildMemoryLedger<G> {
    live_heap_bytes: u128,
    inline_bytes: u128,
    caller_inline_bytes: u128,
    finalized_owner_inline_bytes: u128,
    memory_guard: G,
}

impl<G> FiniteBuildMemoryLedger<G>
where
    G: FnMut(FiniteBuildMemoryPhase, u128) -> Result<(), CoreExecutionError>,
{
    fn new(
        live_heap_bytes: u128,
        inline_bytes: u128,
        caller_inline_bytes: u128,
        finalized_owner_inline_bytes: u128,
        memory_guard: G,
    ) -> Result<Self, CoreExecutionError> {
        if finalized_owner_inline_bytes < core::mem::size_of::<GovernedAppResponse>() as u128 {
            return Err(actual_authority_mismatch());
        }
        let inline_bytes = inline_bytes
            .checked_add(caller_inline_bytes)
            .ok_or_else(projection_overflow)?;
        let mut ledger = Self {
            live_heap_bytes,
            inline_bytes,
            caller_inline_bytes,
            finalized_owner_inline_bytes,
            memory_guard,
        };
        ledger.observe(FiniteBuildMemoryPhase::ConstructionBaseline, 0)?;
        Ok(ledger)
    }

    fn authorize_requested(&mut self, requested_bytes: u128) -> Result<(), CoreExecutionError> {
        self.observe(FiniteBuildMemoryPhase::AllocationRequested, requested_bytes)
    }

    fn observe(
        &mut self,
        phase: FiniteBuildMemoryPhase,
        requested_bytes: u128,
    ) -> Result<(), CoreExecutionError> {
        let required = self
            .inline_bytes
            .checked_add(self.live_heap_bytes)
            .and_then(|bytes| bytes.checked_add(requested_bytes))
            .ok_or_else(projection_overflow)?;
        (self.memory_guard)(phase, required)
    }

    fn retain_actual(&mut self, actual_bytes: u128) -> Result<(), CoreExecutionError> {
        self.live_heap_bytes = self
            .live_heap_bytes
            .checked_add(actual_bytes)
            .ok_or_else(projection_overflow)?;
        self.observe(FiniteBuildMemoryPhase::AllocationActual, 0)
    }

    fn replace_actual(
        &mut self,
        old_bytes: u128,
        new_bytes: u128,
    ) -> Result<(), CoreExecutionError> {
        self.live_heap_bytes = self
            .live_heap_bytes
            .checked_sub(old_bytes)
            .and_then(|bytes| bytes.checked_add(new_bytes))
            .ok_or_else(actual_authority_mismatch)?;
        self.observe(FiniteBuildMemoryPhase::AllocationActual, 0)
    }

    fn retain_inline(&mut self, inline_bytes: u128) -> Result<(), CoreExecutionError> {
        self.inline_bytes = self
            .inline_bytes
            .checked_add(inline_bytes)
            .ok_or_else(projection_overflow)?;
        self.observe(FiniteBuildMemoryPhase::ConstructionBaseline, 0)
    }

    fn release_inline(&mut self, inline_bytes: u128) -> Result<(), CoreExecutionError> {
        self.inline_bytes = self
            .inline_bytes
            .checked_sub(inline_bytes)
            .ok_or_else(actual_authority_mismatch)?;
        self.observe(FiniteBuildMemoryPhase::AllocationActual, 0)
    }

    fn release_actual_and_inline(
        &mut self,
        heap_bytes: u128,
        inline_bytes: u128,
    ) -> Result<(), CoreExecutionError> {
        self.live_heap_bytes = self
            .live_heap_bytes
            .checked_sub(heap_bytes)
            .ok_or_else(actual_authority_mismatch)?;
        self.inline_bytes = self
            .inline_bytes
            .checked_sub(inline_bytes)
            .ok_or_else(actual_authority_mismatch)?;
        self.observe(FiniteBuildMemoryPhase::AllocationActual, 0)
    }

    fn retain_actual_and_release_inline(
        &mut self,
        heap_bytes: u128,
        inline_bytes: u128,
    ) -> Result<(), CoreExecutionError> {
        self.live_heap_bytes = self
            .live_heap_bytes
            .checked_add(heap_bytes)
            .ok_or_else(projection_overflow)?;
        self.inline_bytes = self
            .inline_bytes
            .checked_sub(inline_bytes)
            .ok_or_else(actual_authority_mismatch)?;
        self.observe(FiniteBuildMemoryPhase::AllocationActual, 0)
    }

    fn transition_to_finalized_response(
        &mut self,
        released_heap_bytes: u128,
    ) -> Result<(), CoreExecutionError> {
        self.live_heap_bytes = self
            .live_heap_bytes
            .checked_sub(released_heap_bytes)
            .ok_or_else(actual_authority_mismatch)?;
        self.inline_bytes = self
            .finalized_owner_inline_bytes
            .checked_add(self.caller_inline_bytes)
            .ok_or_else(projection_overflow)?;
        self.observe(FiniteBuildMemoryPhase::FinalizedResponse, 0)
    }

    fn live_heap_bytes(&self) -> u128 {
        self.live_heap_bytes
    }
}

/// Builds the finite Build response while retaining the caller that coexists
/// with every construction stage and authorizing the concrete inline owner
/// that will carry the returned governed response across the caller boundary.
pub(crate) fn try_finite_build_success_response(
    result: CoreExecutionResult,
    pending_validation: DiagnosticReport,
    command_kind: AppCommandKind,
    output_policy: AppOutputPolicy,
    context: AppContext,
    memory_limit_bytes: Option<u128>,
    caller_inline_bytes: u128,
    finalized_owner_inline_bytes: u128,
    memory_guard: impl FnMut(FiniteBuildMemoryPhase, u128) -> Result<(), CoreExecutionError>,
) -> Result<GovernedAppResponse, CoreExecutionError> {
    {
        let source_total = result
            .checked_resource_retained_bytes()
            .ok_or_else(projection_overflow)?;
        let source_heap = source_total
            .checked_sub(core::mem::size_of::<CoreExecutionResult>() as u128)
            .ok_or_else(projection_overflow)?;
        let pending_bytes = pending_validation
            .checked_retained_capacity_bytes()
            .ok_or_else(projection_overflow)?;
        let output_bytes = output_policy
            .checked_retained_capacity_bytes()
            .ok_or_else(projection_overflow)?;
        let initial_heap = source_heap
            .checked_add(pending_bytes)
            .and_then(|bytes| bytes.checked_add(output_bytes))
            .ok_or_else(projection_overflow)?;
        let inline_bytes =
            checked_finite_build_construction_inline_bytes().ok_or_else(projection_overflow)?;
        let mut ledger = FiniteBuildMemoryLedger::new(
            initial_heap,
            inline_bytes,
            caller_inline_bytes,
            finalized_owner_inline_bytes,
            memory_guard,
        )?;

        let result_kind = try_owned_string("build-probability", &mut ledger)?;
        let app_result = AppResult::new(result_kind);
        let resource_report = try_resource_report(&result, &mut ledger)?;
        let backend_report = try_backend_report(&result, &mut ledger)?;
        let capability_report = try_capability_report(&mut ledger)?;
        let mut diagnostics = try_generated_diagnostics(&result, &mut ledger)?;
        move_pending_diagnostics(&mut diagnostics, pending_validation, &mut ledger)?;

        let response = AppResponse {
            command: Some(command_kind),
            status: AppStatus::Success,
            result: Some(app_result),
            diagnostics: crate::diagnostics::AppDiagnosticReport::new(diagnostics),
            backend_report,
            resource_report,
            capability_report,
            continuation: None,
            render_model: Some(AppRenderModel::BuildProbability(result)),
            effects: Vec::new(),
            exit_code_hint: ExitCodeHint::Success,
            error: None,
            pc_chance_execution_evidence: None,
            pc_failed_queue_execution_evidence: None,
            pc_save_execution_evidence: None,
            pc_score_execution_evidence: None,
            pc_score_portfolio_execution_evidence: None,
            pc_tiling_execution_evidence: None,
            product_capability_result: None,
            public_result_payload: None,
            public_page_source_owner: None,
        };

        let response_heap = response
            .checked_retained_capacity_bytes()
            .ok_or_else(actual_authority_mismatch)?;
        let expected_live = output_bytes
            .checked_add(response_heap)
            .ok_or_else(projection_overflow)?;
        if ledger.live_heap_bytes() != expected_live {
            return Err(actual_authority_mismatch());
        }
        ledger.observe(FiniteBuildMemoryPhase::AllocationActual, 0)?;

        // Finite Build has already proved that no product/PC
        // transient exists. Observe diagnostics directly and drop the render
        // model in place when requested; do not use the generic take/map
        // finalizer, whose temporary render owner has a different authority.
        context
            .services()
            .diagnostic_sink()
            .observe(response.diagnostics());
        let response = if output_policy.include_render_model() {
            response
        } else {
            response.without_render_model()
        };
        let finalized_heap = response
            .checked_retained_capacity_bytes()
            .ok_or_else(actual_authority_mismatch)?;
        ledger.replace_actual(response_heap, finalized_heap)?;
        let actual_retained_bytes = (core::mem::size_of::<AppResponse>() as u128)
            .checked_add(finalized_heap)
            .ok_or_else(projection_overflow)?;

        // The sink and output-policy transition are complete. Destroy both
        // remaining external owners before the final actual-capacity guard;
        // the ledger then represents only the governed response wrapper.
        drop(context);
        drop(output_policy);
        ledger.transition_to_finalized_response(output_bytes)?;
        if ledger.live_heap_bytes() != finalized_heap {
            return Err(actual_authority_mismatch());
        }

        Ok(GovernedAppResponse::from_memory_authority(
            response,
            memory_limit_bytes,
            actual_retained_bytes,
        ))
    }
}

fn try_owned_string<G>(
    value: &str,
    ledger: &mut FiniteBuildMemoryLedger<G>,
) -> Result<String, CoreExecutionError>
where
    G: FnMut(FiniteBuildMemoryPhase, u128) -> Result<(), CoreExecutionError>,
{
    ledger.authorize_requested(value.len() as u128)?;
    let mut owned = String::new();
    owned
        .try_reserve_exact(value.len())
        .map_err(|_| allocation_failed())?;
    let actual_capacity = owned.capacity();
    ledger.retain_actual(actual_capacity as u128)?;
    if actual_capacity < value.len() {
        return Err(actual_authority_mismatch());
    }
    owned.push_str(value);
    if owned.capacity() != actual_capacity {
        return Err(actual_authority_mismatch());
    }
    Ok(owned)
}

fn try_decimal_string<G>(
    value: u128,
    ledger: &mut FiniteBuildMemoryLedger<G>,
) -> Result<String, CoreExecutionError>
where
    G: FnMut(FiniteBuildMemoryPhase, u128) -> Result<(), CoreExecutionError>,
{
    let requested = decimal_digit_count(value);
    ledger.authorize_requested(requested as u128)?;
    let mut owned = String::new();
    owned
        .try_reserve_exact(requested)
        .map_err(|_| allocation_failed())?;
    let actual_capacity = owned.capacity();
    ledger.retain_actual(actual_capacity as u128)?;
    if actual_capacity < requested {
        return Err(actual_authority_mismatch());
    }
    write!(&mut owned, "{value}").map_err(|_| actual_authority_mismatch())?;
    if owned.len() != requested || owned.capacity() != actual_capacity {
        return Err(actual_authority_mismatch());
    }
    Ok(owned)
}

fn try_optional_string<G>(
    value: Option<&str>,
    ledger: &mut FiniteBuildMemoryLedger<G>,
) -> Result<Option<String>, CoreExecutionError>
where
    G: FnMut(FiniteBuildMemoryPhase, u128) -> Result<(), CoreExecutionError>,
{
    value
        .map(|value| try_owned_string(value, ledger))
        .transpose()
}

fn try_backend_report<G>(
    result: &CoreExecutionResult,
    ledger: &mut FiniteBuildMemoryLedger<G>,
) -> Result<BackendReport, CoreExecutionError>
where
    G: FnMut(FiniteBuildMemoryPhase, u128) -> Result<(), CoreExecutionError>,
{
    let start = ledger.live_heap_bytes();
    let requested = result
        .field("backend_requested")
        .or_else(|| result.field("requested_backend"))
        .unwrap_or("auto");
    let selected = result
        .field("backend_selected")
        .or_else(|| result.field("selected_backend"))
        .unwrap_or("none");
    let fallback_reason = result
        .field("backend_fallback_reason")
        .filter(|reason| *reason != "none");
    let explicit_fallback = result
        .field("fallback_backend")
        .filter(|value| *value != "none");

    let report = BackendReport::from_owned_memory_authorized_parts(
        try_owned_string(requested, ledger)?,
        try_owned_string(selected, ledger)?,
        try_optional_string(fallback_reason, ledger)?,
        try_optional_string(fallback_reason, ledger)?,
        try_optional_string(
            explicit_fallback.or_else(|| fallback_reason.map(|_| selected)),
            ledger,
        )?,
        try_optional_string(
            result
                .field("gpu_failure_class")
                .filter(|value| *value != "none"),
            ledger,
        )?,
        try_optional_string(
            result
                .field("gpu_failure_stage")
                .filter(|value| *value != "none"),
            ledger,
        )?,
        result.bool_field("discarded_partial_gpu_result") == Some(true),
        try_optional_string(
            result.field("gpu_device").filter(|value| *value != "none"),
            ledger,
        )?,
        result
            .field("gpu_device_selected_index")
            .filter(|value| *value != "none")
            .and_then(|value| value.parse::<u8>().ok()),
        try_optional_string(
            result
                .field("gpu_device_selected_name")
                .filter(|value| *value != "none"),
            ledger,
        )?,
        try_optional_string(
            result
                .field("gpu_device_selected_type")
                .filter(|value| *value != "none"),
            ledger,
        )?,
        try_optional_string(
            result
                .field("gpu_device_selected_backend")
                .filter(|value| *value != "none"),
            ledger,
        )?,
    );
    verify_component_actual(start, report.checked_retained_capacity_bytes(), ledger)?;
    Ok(report)
}

fn try_resource_report<G>(
    result: &CoreExecutionResult,
    ledger: &mut FiniteBuildMemoryLedger<G>,
) -> Result<ResourceReport, CoreExecutionError>
where
    G: FnMut(FiniteBuildMemoryPhase, u128) -> Result<(), CoreExecutionError>,
{
    let start = ledger.live_heap_bytes();
    let memory_status =
        try_owned_string(result.field("memory_status").unwrap_or("reported"), ledger)?;
    let truncation_reason_text = build_resource_truncation_reason(result);
    let truncation_reason = try_optional_string(truncation_reason_text, ledger)?;
    let truncated = truncation_reason.is_some();
    let peak_frontier_states = result
        .usize_field("resource_peak_frontier_states")
        .unwrap_or(0);
    let peak_candidate_rows = result
        .usize_field("resource_peak_candidate_rows")
        .unwrap_or(0);
    let peak_hash_buckets = result
        .usize_field("resource_peak_hash_buckets")
        .unwrap_or(0);
    let peak_gpu_bytes = result.usize_field("resource_peak_gpu_bytes").unwrap_or(0);
    let peak_cpu_bytes = result.usize_field("resource_peak_cpu_bytes").unwrap_or(0);
    let build_worker_backlog_peak = result
        .usize_field("resource_build_worker_backlog_peak")
        .unwrap_or(0);
    let coverage_rows_emitted = result
        .usize_field("resource_coverage_rows_emitted")
        .unwrap_or(0);

    let count_complete = result.bool_field("count_complete").unwrap_or(false);
    let semantic_result_complete =
        count_complete || result.bool_field("objective_complete") == Some(true);
    let probability_complete = result
        .bool_field("resource_probability_complete")
        .or_else(|| result.bool_field("probability_complete"))
        .unwrap_or(false)
        && count_complete
        && !truncated;

    let availability_scratch = core::mem::size_of::<ExecutionAvailabilityReport>() as u128;
    ledger.retain_inline(availability_scratch)?;
    let base_availability =
        finite_base_availability(result, truncation_reason_text, semantic_result_complete);
    let pattern_evidence = if let (Some(descriptor), Some(dense), Some(required_dense)) = (
        result
            .field("execution_descriptor_pattern_count")
            .and_then(|value| value.parse::<u128>().ok()),
        result
            .field("execution_dense_pattern_count")
            .and_then(|value| value.parse::<u128>().ok()),
        result
            .field("execution_required_dense_bytes")
            .and_then(|value| value.parse::<u128>().ok()),
    ) {
        Some((
            try_decimal_string(descriptor, ledger)?,
            try_decimal_string(dense, ledger)?,
            try_decimal_string(required_dense, ledger)?,
        ))
    } else {
        None
    };
    let (descriptor_pattern_count, dense_pattern_count, required_dense_bytes) = pattern_evidence
        .map(|(descriptor, dense, required_dense)| {
            (Some(descriptor), Some(dense), Some(required_dense))
        })
        .unwrap_or((None, None, None));
    let required_memory_bytes = result
        .field("execution_required_memory_bytes")
        .and_then(|value| value.parse::<u128>().ok())
        .map(|required_memory| try_decimal_string(required_memory, ledger))
        .transpose()?;
    let availability = ExecutionAvailabilityReport::from_owned_memory_authorized_parts(
        base_availability.state(),
        base_availability.reason(),
        base_availability.surface(),
        descriptor_pattern_count,
        dense_pattern_count,
        required_dense_bytes,
        required_memory_bytes,
    );
    ledger.release_inline(availability_scratch)?;
    let result_completeness = if semantic_result_complete && !truncated {
        ExecutionCompletenessState::Complete
    } else {
        ExecutionCompletenessState::Incomplete
    };
    let report = ResourceReport::from_owned_memory_authorized_parts(
        true,
        memory_status,
        truncated,
        truncation_reason,
        peak_frontier_states,
        peak_candidate_rows,
        peak_hash_buckets,
        peak_gpu_bytes,
        peak_cpu_bytes,
        build_worker_backlog_peak,
        coverage_rows_emitted,
        probability_complete,
        availability,
        result_completeness,
    );
    verify_component_actual(start, report.checked_retained_capacity_bytes(), ledger)?;
    Ok(report)
}

fn finite_base_availability(
    result: &CoreExecutionResult,
    truncation_reason: Option<&str>,
    semantic_result_complete: bool,
) -> ExecutionAvailabilityReport {
    if let Some(reason) = truncation_reason {
        return availability_from_truncation_reason(reason);
    }
    match result.field("execution_availability_state") {
        Some("unavailable") => ExecutionAvailabilityReport::unavailable(
            ExecutionSurface::current(),
            availability_reason_from_str(
                result
                    .field("execution_availability_reason")
                    .unwrap_or("not-executed"),
            ),
        ),
        Some("deferred") => ExecutionAvailabilityReport::deferred(
            ExecutionSurface::current(),
            ExecutionAvailabilityReason::SharedResourceContention,
        ),
        Some("exhausted") => ExecutionAvailabilityReport::exhausted(
            ExecutionSurface::current(),
            availability_reason_from_str(
                result
                    .field("execution_availability_reason")
                    .unwrap_or("memory-budget-exceeded"),
            ),
        ),
        Some("cancelled") => ExecutionAvailabilityReport::cancelled(ExecutionSurface::current()),
        Some("incomplete") => ExecutionAvailabilityReport::incomplete(
            ExecutionSurface::current(),
            ExecutionAvailabilityReason::PartialExecution,
        ),
        Some("available") if semantic_result_complete => {
            ExecutionAvailabilityReport::available(ExecutionSurface::current())
        }
        None if semantic_result_complete => {
            ExecutionAvailabilityReport::available(ExecutionSurface::current())
        }
        _ => ExecutionAvailabilityReport::incomplete(
            ExecutionSurface::current(),
            ExecutionAvailabilityReason::PartialExecution,
        ),
    }
}

fn try_generated_diagnostics<G>(
    result: &CoreExecutionResult,
    ledger: &mut FiniteBuildMemoryLedger<G>,
) -> Result<DiagnosticReport, CoreExecutionError>
where
    G: FnMut(FiniteBuildMemoryPhase, u128) -> Result<(), CoreExecutionError>,
{
    let truncation_reason = build_resource_truncation_reason(result);
    let trace_reason = (result.bool_field("trace_retention_truncated") == Some(true))
        .then(|| result.field("trace_retention_reason").unwrap_or("unknown"));
    let observed_incomplete = build_observed_probability_incomplete(result);
    let objective_reason = result
        .field("objective_incomplete_reason")
        .filter(|reason| {
            matches!(
                *reason,
                "pattern_weight_model_not_materialized" | "pattern_weight_count_mismatch"
            )
        });
    let count = usize::from(truncation_reason.is_some())
        + usize::from(trace_reason.is_some())
        + usize::from(observed_incomplete)
        + usize::from(objective_reason.is_some());
    let requested_outer = checked_slot_bytes::<Diagnostic>(count)?;
    ledger.authorize_requested(requested_outer)?;
    let mut report = DiagnosticReport::try_with_capacity(count).map_err(|_| allocation_failed())?;
    let outer_actual = checked_slot_bytes::<Diagnostic>(report.capacity())?;
    ledger.retain_actual(outer_actual)?;
    if report.capacity() < count {
        return Err(actual_authority_mismatch());
    }

    if let Some(reason) = truncation_reason {
        try_push_one_evidence_diagnostic(
            &mut report,
            diagnostic_code_for_resource_reason(reason),
            "resource cap or native truncation made the product result incomplete",
            "app_response.resource_report",
            "truncation_reason",
            reason,
            Some(
                "Increase the relevant resource budget or treat count and probability fields as incomplete.",
            ),
            ledger,
        )?;
    }
    if let Some(reason) = trace_reason {
        try_push_one_evidence_diagnostic(
            &mut report,
            DiagnosticCode::WTraceRetentionTruncated,
            "trace retention was truncated; counts may still be complete",
            "app_response.resource_report",
            "trace_retention_reason",
            reason,
            None,
            ledger,
        )?;
    }
    if observed_incomplete {
        try_push_one_evidence_diagnostic(
            &mut report,
            DiagnosticCode::WObservedQueueProbabilityIncomplete,
            "observed queue expansion is incomplete and probability was not renormalized",
            "app_response.resource_report",
            "truncation_reason",
            "observed_universe_truncated",
            Some("Increase the observed queue budget or treat the probability as incomplete."),
            ledger,
        )?;
    }
    if let Some(reason) = objective_reason {
        try_push_one_evidence_diagnostic(
            &mut report,
            DiagnosticCode::WObjectivePatternWeightModelNotMaterialized,
            "the objective was not reduced because its pattern weights were unavailable or inconsistent",
            "app_response.objective_result",
            "objective_incomplete_reason",
            reason,
            Some(
                "Materialize the PatternWeightModel for this PieceSource before requesting the objective.",
            ),
            ledger,
        )?;
    }
    if report.diagnostics().len() != count {
        return Err(actual_authority_mismatch());
    }
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
fn try_push_one_evidence_diagnostic<G>(
    report: &mut DiagnosticReport,
    code: DiagnosticCode,
    message: &str,
    location: &str,
    evidence_key: &str,
    evidence_value: &str,
    suggested_next_step: Option<&str>,
    ledger: &mut FiniteBuildMemoryLedger<G>,
) -> Result<(), CoreExecutionError>
where
    G: FnMut(FiniteBuildMemoryPhase, u128) -> Result<(), CoreExecutionError>,
{
    let diagnostic_inline = core::mem::size_of::<Diagnostic>() as u128;
    let evidence_inline = core::mem::size_of::<ValidationEvidence>() as u128;
    ledger.retain_inline(diagnostic_inline)?;
    let nested_start = ledger.live_heap_bytes();
    let message = try_owned_string(message, ledger)?;
    let location = EvidenceLocation::new(try_owned_string(location, ledger)?);
    ledger.retain_inline(evidence_inline)?;
    let evidence = ValidationEvidence::new(
        try_owned_string(evidence_key, ledger)?,
        try_owned_string(evidence_value, ledger)?,
    );
    let suggestion = suggested_next_step
        .map(|text| try_owned_string(text, ledger))
        .transpose()?
        .map(SuggestedNextStep::new);
    let mut diagnostic = Diagnostic::new(code, message).with_location(location);
    if let Some(suggestion) = suggestion {
        diagnostic = diagnostic.with_suggested_next_step(suggestion);
    }
    ledger.authorize_requested(core::mem::size_of::<ValidationEvidence>() as u128)?;
    diagnostic = diagnostic
        .try_with_evidence(evidence)
        .map_err(|_| allocation_failed())?;
    let actual_nested = diagnostic
        .checked_retained_capacity_bytes()
        .ok_or_else(projection_overflow)?;
    let string_bytes = ledger
        .live_heap_bytes()
        .checked_sub(nested_start)
        .ok_or_else(actual_authority_mismatch)?;
    let actual_evidence_outer = actual_nested
        .checked_sub(string_bytes)
        .ok_or_else(actual_authority_mismatch)?;
    ledger.retain_actual_and_release_inline(actual_evidence_outer, evidence_inline)?;

    let capacity = report.capacity();
    if report.diagnostics().len() >= capacity {
        return Err(actual_authority_mismatch());
    }
    report.push(diagnostic);
    if report.capacity() != capacity {
        return Err(actual_authority_mismatch());
    }
    ledger.release_inline(diagnostic_inline)
}

fn move_pending_diagnostics<G>(
    destination: &mut DiagnosticReport,
    pending: DiagnosticReport,
    ledger: &mut FiniteBuildMemoryLedger<G>,
) -> Result<(), CoreExecutionError>
where
    G: FnMut(FiniteBuildMemoryPhase, u128) -> Result<(), CoreExecutionError>,
{
    let pending_inline = core::mem::size_of::<DiagnosticReport>() as u128;
    let pending_total = pending
        .checked_retained_capacity_bytes()
        .ok_or_else(projection_overflow)?;
    let pending_outer = checked_slot_bytes::<Diagnostic>(pending.capacity())?;
    let pending_nested = pending_total
        .checked_sub(pending_outer)
        .ok_or_else(actual_authority_mismatch)?;
    let pending_len = pending.diagnostics().len();
    if pending_len == 0 {
        drop(pending);
        return ledger.release_actual_and_inline(pending_total, pending_inline);
    }

    let destination_total_before = destination
        .checked_retained_capacity_bytes()
        .ok_or_else(projection_overflow)?;
    let destination_len = destination.diagnostics().len();
    let target_len = destination_len
        .checked_add(pending_len)
        .ok_or_else(projection_overflow)?;
    if destination.capacity() < target_len {
        let old_outer = checked_slot_bytes::<Diagnostic>(destination.capacity())?;
        let requested_new_outer = checked_slot_bytes::<Diagnostic>(target_len)?;
        ledger.authorize_requested(requested_new_outer)?;
        destination
            .try_reserve_exact(pending_len)
            .map_err(|_| allocation_failed())?;
        let new_outer = checked_slot_bytes::<Diagnostic>(destination.capacity())?;
        ledger.replace_actual(old_outer, new_outer)?;
        let destination_total_after = destination
            .checked_retained_capacity_bytes()
            .ok_or_else(projection_overflow)?;
        let expected_destination_total = destination_total_before
            .checked_sub(old_outer)
            .and_then(|bytes| bytes.checked_add(new_outer))
            .ok_or_else(actual_authority_mismatch)?;
        if destination_total_after != expected_destination_total {
            return Err(actual_authority_mismatch());
        }
    }
    if destination.capacity() < target_len {
        return Err(actual_authority_mismatch());
    }
    let destination_total_before_append = destination
        .checked_retained_capacity_bytes()
        .ok_or_else(projection_overflow)?;
    destination.append(pending);
    if destination.diagnostics().len() != target_len {
        return Err(actual_authority_mismatch());
    }
    let destination_total_after_append = destination
        .checked_retained_capacity_bytes()
        .ok_or_else(projection_overflow)?;
    let expected_destination_total = destination_total_before_append
        .checked_add(pending_nested)
        .ok_or_else(projection_overflow)?;
    if destination_total_after_append != expected_destination_total {
        return Err(actual_authority_mismatch());
    }
    ledger.release_actual_and_inline(pending_outer, pending_inline)
}

fn try_capability_report<G>(
    ledger: &mut FiniteBuildMemoryLedger<G>,
) -> Result<CapabilityReport, CoreExecutionError>
where
    G: FnMut(FiniteBuildMemoryPhase, u128) -> Result<(), CoreExecutionError>,
{
    let start = ledger.live_heap_bytes();
    let app_boundary = try_owned_string("clearra-app/AppRequest", ledger)?;
    let executor_boundary = try_owned_string("validation-before-executor", ledger)?;
    #[cfg(feature = "bitmap-render")]
    // The bitmap artifact contract fixes both built-in formats as exact. Keep
    // this projection allocation-free: the generic runtime probe constructs
    // and destroys a decoded atlas, whose temporary heap has no finite-ledger
    // ownership seam. The parity test below compares this response with that
    // validated generic probe in every bitmap-render test build.
    let render_capability = HostRenderCapabilityReport::new(true, true, true, None::<String>);
    #[cfg(not(feature = "bitmap-render"))]
    let render_capability = {
        let reason = try_owned_string("renderer_not_in_wasm_artifact", ledger)?;
        HostRenderCapabilityReport::new(false, false, false, Some(reason))
    };
    let report = CapabilityReport::from_owned_memory_authorized_parts(
        app_boundary,
        executor_boundary,
        Some(render_capability),
    );
    verify_component_actual(start, report.checked_retained_capacity_bytes(), ledger)?;
    Ok(report)
}

fn verify_component_actual<G>(
    start: u128,
    actual: Option<u128>,
    ledger: &FiniteBuildMemoryLedger<G>,
) -> Result<(), CoreExecutionError>
where
    G: FnMut(FiniteBuildMemoryPhase, u128) -> Result<(), CoreExecutionError>,
{
    let tracked = ledger
        .live_heap_bytes()
        .checked_sub(start)
        .ok_or_else(actual_authority_mismatch)?;
    if actual != Some(tracked) {
        return Err(actual_authority_mismatch());
    }
    Ok(())
}

fn checked_slot_bytes<T>(count: usize) -> Result<u128, CoreExecutionError> {
    (count as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or_else(projection_overflow)
}

fn decimal_digit_count(mut value: u128) -> usize {
    let mut digits = 1_usize;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

fn allocation_failed() -> CoreExecutionError {
    CoreExecutionError::RuntimeUnavailable {
        component: ALLOCATION_FAILED,
    }
}

fn projection_overflow() -> CoreExecutionError {
    CoreExecutionError::RuntimeUnavailable {
        component: PROJECTION_OVERFLOW,
    }
}

fn actual_authority_mismatch() -> CoreExecutionError {
    CoreExecutionError::RuntimeUnavailable {
        component: ACTUAL_AUTHORITY_MISMATCH,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_LIMIT_EXCEEDED: &str = "test_finite_build_memory_limit_exceeded";

    fn field(key: &str, value: &str) -> (String, String) {
        (key.to_owned(), value.to_owned())
    }

    fn fixture_result(
        resource: bool,
        trace: bool,
        observed: bool,
        objective: bool,
    ) -> CoreExecutionResult {
        let mut fields = vec![
            field("search_kind", "build-probability"),
            field("backend_requested", "gpu"),
            field("backend_selected", "wasm-cpu"),
            field("backend_fallback_reason", "adapter-unavailable"),
            field("fallback_backend", "wasm-cpu"),
            field("gpu_failure_class", "adapter-request"),
            field("gpu_failure_stage", "device-selection"),
            field("gpu_device", "discrete"),
            field("gpu_device_selected_index", "1"),
            field("gpu_device_selected_name", "fixture adapter"),
            field("gpu_device_selected_type", "discrete-gpu"),
            field("gpu_device_selected_backend", "vulkan"),
            field("discarded_partial_gpu_result", "true"),
            field("memory_status", "reported"),
            field("count_complete", "true"),
            field("probability_complete", "true"),
            field("resource_probability_complete", "true"),
            field("objective_complete", "true"),
            field("supply_probability_complete", "true"),
            field("execution_descriptor_pattern_count", "35384428800"),
            field("execution_dense_pattern_count", "35384428800"),
            field("execution_required_dense_bytes", "4423053600"),
            field("execution_required_memory_bytes", "9999999999"),
        ];
        if resource {
            fields.push(field("resource_truncated", "true"));
            fields.push(field("resource_truncation_reason", "memory_exceeded"));
        }
        if trace {
            fields.push(field("trace_retention_truncated", "true"));
            fields.push(field("trace_retention_reason", "retained_trace_limit"));
        }
        if observed {
            fields.push(field("supply_expansion_truncated", "true"));
        }
        if objective {
            fields.push(field(
                "objective_incomplete_reason",
                "pattern_weight_model_not_materialized",
            ));
        }
        CoreExecutionResult::new(fields, Vec::new())
    }

    fn pending_report() -> DiagnosticReport {
        let mut report = DiagnosticReport::try_with_capacity(8)
            .expect("the test can reserve its pending report");
        report.push(
            Diagnostic::new(DiagnosticCode::EBuildQueryInvalid, "pending validation")
                .with_location(EvidenceLocation::new("request.build"))
                .with_evidence(ValidationEvidence::new("source", "validator")),
        );
        report
    }

    fn build_with_guard(
        include_render_model: bool,
        memory_limit_bytes: Option<u128>,
        memory_guard: impl FnMut(FiniteBuildMemoryPhase, u128) -> Result<(), CoreExecutionError>,
    ) -> Result<GovernedAppResponse, CoreExecutionError> {
        build_with_boundary_guard(
            include_render_model,
            memory_limit_bytes,
            0,
            core::mem::size_of::<GovernedAppResponse>() as u128,
            memory_guard,
        )
    }

    fn build_with_boundary_guard(
        include_render_model: bool,
        memory_limit_bytes: Option<u128>,
        caller_inline_bytes: u128,
        finalized_owner_inline_bytes: u128,
        memory_guard: impl FnMut(FiniteBuildMemoryPhase, u128) -> Result<(), CoreExecutionError>,
    ) -> Result<GovernedAppResponse, CoreExecutionError> {
        try_finite_build_success_response(
            fixture_result(true, true, true, true),
            pending_report(),
            AppCommandKind::BuildProbability,
            AppOutputPolicy::new(include_render_model),
            AppContext::default(),
            memory_limit_bytes,
            caller_inline_bytes,
            finalized_owner_inline_bytes,
            memory_guard,
        )
    }

    #[test]
    fn actual_stage_peak_accepts_exact_limit_and_rejects_peak_minus_one() {
        let mut observations = Vec::new();
        let measured = build_with_guard(true, None, |phase, required| {
            observations.push((phase, required));
            Ok(())
        })
        .expect("unbounded measurement succeeds");
        let peak = observations
            .iter()
            .map(|(_, required)| *required)
            .max()
            .expect("the builder emits memory stages");
        let final_owner_actual = observations
            .iter()
            .rev()
            .find_map(|(phase, required)| {
                (*phase == FiniteBuildMemoryPhase::FinalizedResponse).then_some(*required)
            })
            .expect("the builder emits a final actual stage");
        let expected_response_actual = measured
            .response()
            .checked_retained_capacity_bytes()
            .and_then(|heap| (core::mem::size_of::<AppResponse>() as u128).checked_add(heap))
            .expect("the finite response shape has exact retained capacity");
        assert_eq!(measured.actual_retained_bytes(), expected_response_actual);
        assert_eq!(
            final_owner_actual,
            (core::mem::size_of::<GovernedAppResponse>() as u128)
                + measured
                    .actual_retained_bytes()
                    .checked_sub(core::mem::size_of::<AppResponse>() as u128)
                    .expect("response actual includes its inline owner")
        );
        drop(measured);

        let exact = build_with_guard(true, Some(peak), |_, required| {
            if required > peak {
                Err(CoreExecutionError::RuntimeUnavailable {
                    component: TEST_LIMIT_EXCEEDED,
                })
            } else {
                Ok(())
            }
        })
        .expect("the exact observed peak is admitted");
        assert_eq!(exact.memory_limit_bytes(), Some(peak));
        drop(exact);

        assert_eq!(
            build_with_guard(true, Some(peak - 1), |_, required| {
                if required > peak - 1 {
                    Err(CoreExecutionError::RuntimeUnavailable {
                        component: TEST_LIMIT_EXCEEDED,
                    })
                } else {
                    Ok(())
                }
            }),
            Err(CoreExecutionError::RuntimeUnavailable {
                component: TEST_LIMIT_EXCEEDED,
            })
        );

        build_with_guard(true, Some(peak), |phase, required| {
            if phase == FiniteBuildMemoryPhase::FinalizedResponse && required > final_owner_actual {
                Err(CoreExecutionError::RuntimeUnavailable {
                    component: TEST_LIMIT_EXCEEDED,
                })
            } else {
                Ok(())
            }
        })
        .expect("the exact final actual owner is admitted");
        assert_eq!(
            build_with_guard(true, Some(peak), |phase, required| {
                if phase == FiniteBuildMemoryPhase::FinalizedResponse
                    && required > final_owner_actual - 1
                {
                    Err(CoreExecutionError::RuntimeUnavailable {
                        component: TEST_LIMIT_EXCEEDED,
                    })
                } else {
                    Ok(())
                }
            }),
            Err(CoreExecutionError::RuntimeUnavailable {
                component: TEST_LIMIT_EXCEEDED,
            })
        );
    }

    #[test]
    fn cooperative_caller_and_return_enum_are_in_the_exact_peak_and_final_stage() {
        let caller_inline_bytes =
            core::mem::size_of::<crate::cooperative_execution::CooperativeAppExecution>() as u128;
        let finalized_owner_inline_bytes =
            core::mem::size_of::<crate::cooperative_execution::CooperativeAppAdvance>() as u128;
        assert!(
            finalized_owner_inline_bytes >= core::mem::size_of::<GovernedAppResponse>() as u128
        );
        let mut observations = Vec::new();
        let measured = build_with_boundary_guard(
            true,
            None,
            caller_inline_bytes,
            finalized_owner_inline_bytes,
            |phase, required| {
                observations.push((phase, required));
                Ok(())
            },
        )
        .expect("the cooperative boundary measurement succeeds");
        let peak = observations
            .iter()
            .map(|(_, required)| *required)
            .max()
            .expect("the builder emits cooperative memory stages");
        let final_required = observations
            .iter()
            .rev()
            .find_map(|(phase, required)| {
                (*phase == FiniteBuildMemoryPhase::FinalizedResponse).then_some(*required)
            })
            .expect("the builder emits the cooperative final stage");
        let response_heap = measured
            .actual_retained_bytes()
            .checked_sub(core::mem::size_of::<AppResponse>() as u128)
            .expect("the governed actual includes the response inline owner");
        assert_eq!(
            final_required,
            caller_inline_bytes + finalized_owner_inline_bytes + response_heap
        );
        drop(measured);

        build_with_boundary_guard(
            true,
            Some(peak),
            caller_inline_bytes,
            finalized_owner_inline_bytes,
            |_, required| {
                if required > peak {
                    Err(CoreExecutionError::RuntimeUnavailable {
                        component: TEST_LIMIT_EXCEEDED,
                    })
                } else {
                    Ok(())
                }
            },
        )
        .expect("the exact cooperative construction peak is admitted");
        assert_eq!(
            build_with_boundary_guard(
                true,
                Some(peak - 1),
                caller_inline_bytes,
                finalized_owner_inline_bytes,
                |_, required| {
                    if required > peak - 1 {
                        Err(CoreExecutionError::RuntimeUnavailable {
                            component: TEST_LIMIT_EXCEEDED,
                        })
                    } else {
                        Ok(())
                    }
                },
            ),
            Err(CoreExecutionError::RuntimeUnavailable {
                component: TEST_LIMIT_EXCEEDED,
            })
        );
        assert_eq!(
            build_with_boundary_guard(
                true,
                Some(peak),
                caller_inline_bytes,
                finalized_owner_inline_bytes,
                |phase, required| {
                    if phase == FiniteBuildMemoryPhase::FinalizedResponse
                        && required > final_required - 1
                    {
                        Err(CoreExecutionError::RuntimeUnavailable {
                            component: TEST_LIMIT_EXCEEDED,
                        })
                    } else {
                        Ok(())
                    }
                },
            ),
            Err(CoreExecutionError::RuntimeUnavailable {
                component: TEST_LIMIT_EXCEEDED,
            })
        );
    }

    #[test]
    fn finite_builder_matches_generic_diagnostics_for_every_combination_and_output_policy() {
        for include_render_model in [false, true] {
            for mask in 0_u8..16 {
                let resource = mask & 1 != 0;
                let trace = mask & 2 != 0;
                let observed = mask & 4 != 0;
                let objective = mask & 8 != 0;
                let output_policy = AppOutputPolicy::new(include_render_model);
                let expected_context = AppContext::default();
                let expected = expected_context.finalize_response(
                    AppResponse::success(AppRenderModel::BuildProbability(fixture_result(
                        resource, trace, observed, objective,
                    )))
                    .with_validation_diagnostics(pending_report()),
                    AppCommandKind::BuildProbability,
                    &output_policy,
                );
                let actual = try_finite_build_success_response(
                    fixture_result(resource, trace, observed, objective),
                    pending_report(),
                    AppCommandKind::BuildProbability,
                    AppOutputPolicy::new(include_render_model),
                    AppContext::default(),
                    None,
                    0,
                    core::mem::size_of::<GovernedAppResponse>() as u128,
                    |_, _| Ok(()),
                )
                .expect("the finite diagnostic combination succeeds");

                assert_eq!(actual.response(), &expected, "mask={mask}");
                assert_eq!(
                    actual.response().render_model().is_some(),
                    include_render_model,
                    "mask={mask}"
                );
            }
        }
    }

    #[test]
    fn empty_pending_capacity_is_destroyed_before_final_actual_authority() {
        let pending = DiagnosticReport::try_with_capacity(8)
            .expect("the test can reserve an empty pending owner");
        assert!(pending.capacity() >= 8);
        let governed = try_finite_build_success_response(
            fixture_result(false, false, false, false),
            pending,
            AppCommandKind::BuildProbability,
            AppOutputPolicy::new(false),
            AppContext::default(),
            None,
            0,
            core::mem::size_of::<GovernedAppResponse>() as u128,
            |_, _| Ok(()),
        )
        .expect("empty pending capacity is released");

        assert_eq!(governed.response().diagnostics().validation().capacity(), 0);
        assert!(governed.response().render_model().is_none());
    }
}
