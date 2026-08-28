// SRP rationale: this module has one behavior-level change reason: assembling and validating the typed application response envelope from governed execution evidence.

use crate::{
    app_error::{AppError, AppErrorCode},
    build_solution_probability_result::build_v2_facade::{BuildCoveragePortfolioV2, BuildSetupV1},
    diagnostics::AppDiagnosticReport,
    objective_contract::objective_diagnostics_from_render_model,
    pc_chance_probability_result::ValidatedPcChanceExecutionEvidence,
    pc_failed_queue_result::ValidatedPcFailedQueueExecutionEvidence,
    pc_save_result::ValidatedPcSaveExecutionEvidence,
    pc_score_minimum_cover_result::ValidatedPcScorePortfolioExecutionEvidence,
    pc_score_summary_result::ValidatedPcScoreExecutionEvidence,
    pc_tiling_family_result::ValidatedPcTilingExecutionEvidence,
    portfolio_alternative_store::ProductPageSourceOwner,
    product_capability_contract::ProductCapabilityContractError,
    product_capability_result::ProductCapabilityResult,
    render::AppRenderModel,
    resource_contract::{
        resource_diagnostics_from_failure, resource_diagnostics_from_render_model,
        resource_report_from_failure, resource_report_from_render_model,
    },
};
use clearra_core_executor::CoreExecutionResult;
use clearra_host_contract::{
    AppCommandKind, AppResponse as HostAppResponse, AppResult, AppStatus as HostAppStatus,
    BackendReport, CapabilityReport, ContinuationReport, Diagnostic, ProductResultPayload,
    RenderCapabilityReport as HostRenderCapabilityReport, ResourceReport,
    SolutionSetArtifactPayload, HOST_SOLUTION_SET_ARTIFACT_MAX_BYTES,
};
#[cfg(feature = "bitmap-render")]
use clearra_output::render::RenderExactOutputGate;
mod finite_build_success;
pub(crate) mod solution_set_artifact;

pub(crate) use finite_build_success::{try_finite_build_success_response, FiniteBuildMemoryPhase};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppStatus {
    Success,
    ValidationFailed,
    Unsupported,
    ExecutionFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitCodeHint {
    Success,
    ValidationFailed,
    Failure,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppEffect {
    None,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppResponse {
    command: Option<AppCommandKind>,
    status: AppStatus,
    result: Option<AppResult>,
    diagnostics: AppDiagnosticReport,
    backend_report: BackendReport,
    resource_report: ResourceReport,
    capability_report: CapabilityReport,
    continuation: Option<ContinuationReport>,
    render_model: Option<AppRenderModel>,
    effects: Vec<AppEffect>,
    exit_code_hint: ExitCodeHint,
    error: Option<AppError>,
    pc_chance_execution_evidence: Option<ValidatedPcChanceExecutionEvidence>,
    pc_failed_queue_execution_evidence: Option<ValidatedPcFailedQueueExecutionEvidence>,
    pc_save_execution_evidence: Option<ValidatedPcSaveExecutionEvidence>,
    pc_score_execution_evidence: Option<ValidatedPcScoreExecutionEvidence>,
    pc_score_portfolio_execution_evidence: Option<ValidatedPcScorePortfolioExecutionEvidence>,
    pc_tiling_execution_evidence: Option<ValidatedPcTilingExecutionEvidence>,
    product_capability_result: Option<ProductCapabilityResult>,
    public_result_payload: Option<ProductResultPayload>,
    public_page_source_owner: Option<ProductPageSourceOwner>,
}

/// Final App response together with the memory authority that admitted it.
///
/// `actual_retained_bytes` is the exact retained size of `AppResponse` itself:
/// its inline value plus every response-owned heap allocation measured from
/// actual allocator capacities. The small wrapper metadata is not part of
/// that value; construction guards account for the live wrapper owner as a
/// separate inline stage before this value is returned.
#[derive(Debug, Eq, PartialEq)]
pub struct GovernedAppResponse {
    response: AppResponse,
    memory_limit_bytes: Option<u128>,
    actual_retained_bytes: u128,
}

impl GovernedAppResponse {
    pub(crate) fn from_memory_authority(
        response: AppResponse,
        memory_limit_bytes: Option<u128>,
        actual_retained_bytes: u128,
    ) -> Self {
        Self {
            response,
            memory_limit_bytes,
            actual_retained_bytes,
        }
    }

    pub fn response(&self) -> &AppResponse {
        &self.response
    }

    pub const fn memory_limit_bytes(&self) -> Option<u128> {
        self.memory_limit_bytes
    }

    pub const fn actual_retained_bytes(&self) -> u128 {
        self.actual_retained_bytes
    }

    /// Consumes the single authority owner without duplicating the retained
    /// response payload. Downstream boundaries receive the response and both
    /// memory facts together, so the authority is not silently discarded.
    pub fn into_parts(self) -> (AppResponse, Option<u128>, u128) {
        (
            self.response,
            self.memory_limit_bytes,
            self.actual_retained_bytes,
        )
    }
}

impl AppResponse {
    pub fn success(render_model: AppRenderModel) -> Self {
        let result = Some(AppResult::new(render_model.kind().as_str()));
        let resource_report = resource_report_from_render_model(&render_model);
        let backend_report = backend_report_from_render_model(&render_model);
        let mut diagnostic_report = resource_diagnostics_from_render_model(&render_model);
        diagnostic_report.append(objective_diagnostics_from_render_model(&render_model));
        let diagnostics = AppDiagnosticReport::new(diagnostic_report);
        Self {
            command: None,
            status: AppStatus::Success,
            result,
            diagnostics,
            backend_report,
            resource_report,
            capability_report: product_capability_report(),
            continuation: None,
            render_model: Some(render_model),
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
        }
    }
}

fn backend_report_from_render_model(render_model: &AppRenderModel) -> BackendReport {
    if render_model.forward_search_result().is_some() {
        return BackendReport::new("cpu", "wasm-cpu-forward-search", None::<String>);
    }
    if render_model.spin_structure_result().is_some() {
        return BackendReport::new("cpu", "cpu-spin-structure", None::<String>);
    }
    let Some(result) = render_model.core_result() else {
        return BackendReport::default();
    };
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
    let failure_class = result
        .field("gpu_failure_class")
        .filter(|value| *value != "none")
        .map(ToOwned::to_owned);
    let failure_stage = result
        .field("gpu_failure_stage")
        .filter(|value| *value != "none")
        .map(ToOwned::to_owned);
    let fallback_backend = result
        .field("fallback_backend")
        .filter(|value| *value != "none")
        .map(ToOwned::to_owned);
    let discarded_partial_gpu_result = result
        .field("discarded_partial_gpu_result")
        .is_some_and(|value| value == "true");
    let gpu_device_requested = result
        .field("gpu_device")
        .filter(|value| *value != "none")
        .map(ToOwned::to_owned);
    let gpu_device_selected_index = result
        .field("gpu_device_selected_index")
        .filter(|value| *value != "none")
        .and_then(|value| value.parse::<u8>().ok());
    let gpu_device_selected_name = result
        .field("gpu_device_selected_name")
        .filter(|value| *value != "none")
        .map(ToOwned::to_owned);
    let gpu_device_selected_type = result
        .field("gpu_device_selected_type")
        .filter(|value| *value != "none")
        .map(ToOwned::to_owned);
    let gpu_device_selected_backend = result
        .field("gpu_device_selected_backend")
        .filter(|value| *value != "none")
        .map(ToOwned::to_owned);
    BackendReport::new(requested, selected, fallback_reason)
        .with_gpu_execution_failure(
            failure_class,
            failure_stage,
            fallback_backend,
            discarded_partial_gpu_result,
        )
        .with_gpu_device(
            gpu_device_requested,
            gpu_device_selected_index,
            gpu_device_selected_name,
            gpu_device_selected_type,
            gpu_device_selected_backend,
        )
}

fn build_resource_truncation_reason(result: &CoreExecutionResult) -> Option<&str> {
    result
        .field("resource_truncation_reason")
        .filter(|reason| *reason != "none")
        .or_else(|| {
            (result.bool_field("count_complete") == Some(false)).then(|| {
                result
                    .field("count_truncated_reason")
                    .unwrap_or("count_incomplete")
            })
        })
        .or_else(|| {
            build_observed_probability_incomplete(result).then_some("observed_universe_truncated")
        })
        .or_else(|| {
            (result.bool_field("resource_truncated") == Some(true)).then_some("resource_truncated")
        })
}

fn build_observed_probability_incomplete(result: &CoreExecutionResult) -> bool {
    result.bool_field("supply_expansion_truncated") == Some(true)
        || result.bool_field("supply_probability_complete") == Some(false)
            && result.field("queue_mode") == Some("observed")
}

fn checked_count_bytes(count: u128, item_size: u128) -> Option<u128> {
    count.checked_mul(item_size)
}
impl AppResponse {
    pub fn validation_failed(
        report: clearra_validation::diagnostic::diagnostic_report::DiagnosticReport,
    ) -> Self {
        Self {
            command: None,
            status: AppStatus::ValidationFailed,
            result: None,
            diagnostics: AppDiagnosticReport::new(report),
            backend_report: BackendReport::default(),
            resource_report: ResourceReport::default(),
            capability_report: product_capability_report(),
            continuation: None,
            render_model: None,
            effects: Vec::new(),
            exit_code_hint: ExitCodeHint::ValidationFailed,
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
        }
    }
}
impl AppResponse {
    pub fn failed(status: AppStatus, error: AppError) -> Self {
        let exit_code_hint = match status {
            AppStatus::Success => ExitCodeHint::Success,
            AppStatus::ValidationFailed => ExitCodeHint::ValidationFailed,
            AppStatus::Unsupported | AppStatus::ExecutionFailed => ExitCodeHint::Failure,
        };
        let resource_report = resource_report_from_failure(status, &error);
        let diagnostics = AppDiagnosticReport::new(resource_diagnostics_from_failure(&error));
        Self {
            command: None,
            status,
            result: None,
            diagnostics,
            backend_report: BackendReport::default(),
            resource_report,
            capability_report: product_capability_report(),
            continuation: None,
            render_model: None,
            effects: Vec::new(),
            exit_code_hint,
            error: Some(error),
            pc_chance_execution_evidence: None,
            pc_failed_queue_execution_evidence: None,
            pc_save_execution_evidence: None,
            pc_score_execution_evidence: None,
            pc_score_portfolio_execution_evidence: None,
            pc_tiling_execution_evidence: None,
            product_capability_result: None,
            public_result_payload: None,
            public_page_source_owner: None,
        }
    }
}
impl AppResponse {
    pub(crate) fn with_pc_chance_execution_evidence(
        mut self,
        evidence: ValidatedPcChanceExecutionEvidence,
    ) -> Self {
        debug_assert!(self.pc_chance_execution_evidence.is_none());
        self.pc_chance_execution_evidence = Some(evidence);
        self
    }

    pub(crate) fn pc_chance_execution_evidence(
        &self,
    ) -> Option<&ValidatedPcChanceExecutionEvidence> {
        self.pc_chance_execution_evidence.as_ref()
    }

    pub(crate) fn with_pc_failed_queue_execution_evidence(
        mut self,
        evidence: ValidatedPcFailedQueueExecutionEvidence,
    ) -> Self {
        debug_assert!(self.pc_failed_queue_execution_evidence.is_none());
        self.pc_failed_queue_execution_evidence = Some(evidence);
        self
    }

    pub(crate) fn pc_failed_queue_execution_evidence(
        &self,
    ) -> Option<&ValidatedPcFailedQueueExecutionEvidence> {
        self.pc_failed_queue_execution_evidence.as_ref()
    }

    pub(crate) fn with_pc_save_execution_evidence(
        mut self,
        evidence: ValidatedPcSaveExecutionEvidence,
    ) -> Self {
        debug_assert!(self.pc_save_execution_evidence.is_none());
        self.pc_save_execution_evidence = Some(evidence);
        self
    }

    pub(crate) fn pc_save_execution_evidence(&self) -> Option<&ValidatedPcSaveExecutionEvidence> {
        self.pc_save_execution_evidence.as_ref()
    }

    pub(crate) fn with_pc_score_execution_evidence(
        mut self,
        evidence: ValidatedPcScoreExecutionEvidence,
    ) -> Self {
        debug_assert!(self.pc_score_execution_evidence.is_none());
        self.pc_score_execution_evidence = Some(evidence);
        self
    }

    pub(crate) fn pc_score_execution_evidence(&self) -> Option<&ValidatedPcScoreExecutionEvidence> {
        self.pc_score_execution_evidence.as_ref()
    }

    pub(crate) fn with_pc_score_portfolio_execution_evidence(
        mut self,
        evidence: ValidatedPcScorePortfolioExecutionEvidence,
    ) -> Self {
        debug_assert!(self.pc_score_execution_evidence.is_none());
        debug_assert!(self.pc_score_portfolio_execution_evidence.is_none());
        self.pc_score_portfolio_execution_evidence = Some(evidence);
        self
    }

    pub(crate) fn pc_score_portfolio_execution_evidence(
        &self,
    ) -> Option<&ValidatedPcScorePortfolioExecutionEvidence> {
        self.pc_score_portfolio_execution_evidence.as_ref()
    }

    pub(crate) fn with_pc_tiling_execution_evidence(
        mut self,
        evidence: ValidatedPcTilingExecutionEvidence,
    ) -> Self {
        debug_assert!(self.pc_tiling_execution_evidence.is_none());
        self.pc_tiling_execution_evidence = Some(evidence);
        self
    }

    pub(crate) fn pc_tiling_execution_evidence(
        &self,
    ) -> Option<&ValidatedPcTilingExecutionEvidence> {
        self.pc_tiling_execution_evidence.as_ref()
    }

    /// Consumes every request-private chance proof after the product wrapper
    /// has been validated. This runs for success and failure responses before
    /// output-policy filtering, so neither App nor Host surfaces retain the
    /// coverage rows or the executed-problem snapshot.
    pub(crate) fn without_product_capability_transients(mut self) -> Self {
        let strip_replay_transients = self.pc_score_execution_evidence.is_some()
            || self.pc_score_portfolio_execution_evidence.is_some()
            || self.pc_save_execution_evidence.is_some();
        self.pc_chance_execution_evidence = None;
        self.pc_failed_queue_execution_evidence = None;
        self.pc_save_execution_evidence = None;
        self.pc_score_execution_evidence = None;
        self.pc_score_portfolio_execution_evidence = None;
        self.pc_tiling_execution_evidence = None;
        self.render_model = self
            .render_model
            .take()
            .map(AppRenderModel::without_pc_chance_transient_evidence)
            .map(AppRenderModel::without_pc_score_problem_evidence)
            .map(|model| {
                if strip_replay_transients {
                    model.without_pc_score_transient_evidence()
                } else {
                    model
                }
            });
        self
    }
}
impl AppResponse {
    pub fn with_build_setup_v1(
        mut self,
        report: BuildSetupV1,
    ) -> Result<Self, ProductCapabilityContractError> {
        if self.status != AppStatus::Success {
            return Err(ProductCapabilityContractError::ResponseStatusNotSuccessful);
        }
        if self.product_capability_result.is_some()
            || self.public_result_payload.is_some()
            || self.public_page_source_owner.is_some()
        {
            return Err(ProductCapabilityContractError::ResponseAlreadyWrapped);
        }
        let result = ProductCapabilityResult::from_build_setup_v1(report)?;
        self.command = Some(AppCommandKind::BuildProbability);
        self.product_capability_result = Some(result);
        Ok(self)
    }

    pub fn with_build_coverage_portfolio_v2(
        mut self,
        report: BuildCoveragePortfolioV2,
    ) -> Result<Self, ProductCapabilityContractError> {
        if self.status != AppStatus::Success {
            return Err(ProductCapabilityContractError::ResponseStatusNotSuccessful);
        }
        if self.product_capability_result.is_some()
            || self.public_result_payload.is_some()
            || self.public_page_source_owner.is_some()
        {
            return Err(ProductCapabilityContractError::ResponseAlreadyWrapped);
        }
        let result = ProductCapabilityResult::from_build_coverage_portfolio_v2(report)?;
        self.command = Some(AppCommandKind::BuildProbability);
        self.product_capability_result = Some(result);
        Ok(self)
    }

    pub(crate) fn with_product_capability_result(
        mut self,
        result: ProductCapabilityResult,
    ) -> Self {
        debug_assert!(self.product_capability_result.is_none());
        self.product_capability_result = Some(result);
        self
    }

    pub(crate) fn with_public_product_result(
        mut self,
        payload: ProductResultPayload,
        page_source_owner: Option<ProductPageSourceOwner>,
    ) -> Self {
        debug_assert!(self.product_capability_result.is_none());
        debug_assert!(self.public_result_payload.is_none());
        debug_assert!(self.public_page_source_owner.is_none());
        self.public_result_payload = Some(payload);
        self.public_page_source_owner = page_source_owner;
        self
    }
}
impl AppResponse {
    pub fn without_render_model(mut self) -> Self {
        self.render_model = None;
        self
    }
}
impl AppResponse {
    pub fn with_contract_context(mut self, command: AppCommandKind) -> Self {
        self.command = Some(command);
        self
    }
}
impl AppResponse {
    pub fn with_backend_report(mut self, backend_report: BackendReport) -> Self {
        self.backend_report = backend_report;
        self
    }
}
impl AppResponse {
    pub fn with_resource_report(mut self, resource_report: ResourceReport) -> Self {
        self.resource_report = resource_report;
        self
    }
}
impl AppResponse {
    pub fn with_capability_report(mut self, capability_report: CapabilityReport) -> Self {
        self.capability_report =
            capability_report.with_render_capability(runtime_render_capability_report());
        self
    }
}

fn product_capability_report() -> CapabilityReport {
    CapabilityReport::default().with_render_capability(runtime_render_capability_report())
}

fn runtime_render_capability_report() -> HostRenderCapabilityReport {
    #[cfg(not(feature = "bitmap-render"))]
    {
        return HostRenderCapabilityReport::new(
            false,
            false,
            false,
            Some("renderer_not_in_wasm_artifact".to_owned()),
        );
    }
    #[cfg(feature = "bitmap-render")]
    {
        let runtime = RenderExactOutputGate::capability_report();
        let png = runtime
            .frame_formats()
            .iter()
            .copied()
            .find(|capability| capability.frame_format().as_str() == "png");
        let gif = runtime
            .frame_formats()
            .iter()
            .copied()
            .find(|capability| capability.frame_format().as_str() == "gif");
        let unsupported_reason = png
            .and_then(|capability| capability.unsupported_reason())
            .or_else(|| gif.and_then(|capability| capability.unsupported_reason()))
            .map(|reason| reason.as_str().to_owned())
            .or_else(|| {
                if png.is_none() || gif.is_none() {
                    Some("runtime_capability_report_incomplete".to_owned())
                } else {
                    None
                }
            });
        let png_supported = png.is_some_and(|capability| capability.supported());
        let gif_supported = gif.is_some_and(|capability| capability.supported());
        let render_exact = png.is_some_and(|capability| capability.render_exact())
            && gif.is_some_and(|capability| capability.render_exact());

        HostRenderCapabilityReport::new(
            png_supported,
            gif_supported,
            render_exact,
            unsupported_reason,
        )
    }
}
impl AppResponse {
    pub fn with_continuation(mut self, continuation: Option<ContinuationReport>) -> Self {
        self.continuation = continuation;
        self
    }
}
impl AppResponse {
    pub fn with_validation_diagnostics(
        mut self,
        report: clearra_validation::diagnostic::diagnostic_report::DiagnosticReport,
    ) -> Self {
        self.diagnostics.append(report);
        self
    }
}
impl AppResponse {
    pub fn status(&self) -> AppStatus {
        self.status
    }
}
impl AppResponse {
    pub fn command(&self) -> Option<AppCommandKind> {
        self.command
    }
}
impl AppResponse {
    pub fn result(&self) -> Option<&AppResult> {
        self.result.as_ref()
    }
}
impl AppResponse {
    pub fn diagnostics(&self) -> &AppDiagnosticReport {
        &self.diagnostics
    }
}
impl AppResponse {
    pub fn backend_report(&self) -> &BackendReport {
        &self.backend_report
    }
}
impl AppResponse {
    pub fn resource_report(&self) -> &ResourceReport {
        &self.resource_report
    }
}
impl AppResponse {
    pub fn capability_report(&self) -> &CapabilityReport {
        &self.capability_report
    }
}
impl AppResponse {
    pub fn continuation(&self) -> Option<&ContinuationReport> {
        self.continuation.as_ref()
    }
}
impl AppResponse {
    pub fn render_model(&self) -> Option<&AppRenderModel> {
        self.render_model.as_ref()
    }
}
impl AppResponse {
    pub fn effects(&self) -> &[AppEffect] {
        &self.effects
    }
}
impl AppResponse {
    pub fn exit_code_hint(&self) -> ExitCodeHint {
        self.exit_code_hint
    }
}
impl AppResponse {
    pub fn error(&self) -> Option<&AppError> {
        self.error.as_ref()
    }
}
impl AppResponse {
    pub fn product_capability_result(&self) -> Option<&ProductCapabilityResult> {
        self.product_capability_result.as_ref()
    }

    pub fn public_result_payload(&self) -> Option<&ProductResultPayload> {
        self.public_result_payload.as_ref()
    }

    pub fn public_page_source_owner(&self) -> Option<ProductPageSourceOwner> {
        self.public_page_source_owner.clone().or_else(|| {
            self.product_capability_result
                .as_ref()
                .and_then(ProductCapabilityResult::public_page_source_owner)
        })
    }

    /// Returns the exact heap payload retained by the memory-authorized
    /// distributed Build response shape, using actual allocator capacities.
    /// The inline `AppResponse`, inline report owners, and inline
    /// `CoreExecutionResult` are excluded.
    ///
    /// This authority is intentionally specialized and fails closed for
    /// non-success responses, non-Build result kinds/render models,
    /// continuations, product-capability wrappers, and request-private PC
    /// evidence. Those owners have separate lifecycle contracts and must not
    /// be silently admitted by the distributed Build terminal path.
    pub(crate) fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        if self.status != AppStatus::Success
            || !matches!(self.command, None | Some(AppCommandKind::BuildProbability))
            || self.exit_code_hint != ExitCodeHint::Success
            || self.error.is_some()
            || self.continuation.is_some()
            || self.pc_chance_execution_evidence.is_some()
            || self.pc_failed_queue_execution_evidence.is_some()
            || self.pc_save_execution_evidence.is_some()
            || self.pc_score_execution_evidence.is_some()
            || self.pc_score_portfolio_execution_evidence.is_some()
            || self.pc_tiling_execution_evidence.is_some()
            || self.product_capability_result.is_some()
            || self.public_result_payload.is_some()
            || self.public_page_source_owner.is_some()
            || self.result.as_ref().map(AppResult::kind) != Some("build-probability")
        {
            return None;
        }

        let mut bytes = self
            .result
            .as_ref()
            .expect("the result kind was checked above")
            .checked_retained_capacity_bytes()?;
        bytes = bytes.checked_add(
            self.diagnostics
                .validation()
                .checked_retained_capacity_bytes()?,
        )?;
        bytes = bytes.checked_add(self.backend_report.checked_retained_capacity_bytes()?)?;
        bytes = bytes.checked_add(self.resource_report.checked_retained_capacity_bytes()?)?;
        bytes = bytes.checked_add(self.capability_report.checked_retained_capacity_bytes()?)?;
        bytes = bytes.checked_add(checked_count_bytes(
            self.effects.capacity() as u128,
            core::mem::size_of::<AppEffect>() as u128,
        )?)?;

        match &self.render_model {
            Some(AppRenderModel::BuildProbability(result)) => {
                let total = result.checked_resource_retained_bytes()?;
                let heap_only =
                    total.checked_sub(core::mem::size_of::<CoreExecutionResult>() as u128)?;
                bytes = bytes.checked_add(heap_only)?;
            }
            None => {}
            Some(_) => return None,
        }
        Some(bytes)
    }

    #[cfg(test)]
    pub(crate) fn with_result_kind_for_test(mut self, kind: &str) -> Self {
        self.result = Some(AppResult::new(kind));
        self
    }
}

impl AppResponse {
    /// Materializes one complete, canonical solution-set owner. Incomplete,
    /// truncated, non-materialized, empty, or identity-inconsistent results
    /// deliberately return `None`; callers must not synthesize a document.
    pub fn complete_solution_set_artifact(
        &self,
    ) -> Option<clearra_output::artifact::SolutionSetArtifact> {
        solution_set_artifact::materialize_response(self)
            .map(solution_set_artifact::BoundSolutionSetArtifact::into_artifact)
    }

    pub fn bounded_solution_set_artifact_payload(
        &self,
        maximum_bytes: u64,
    ) -> Option<SolutionSetArtifactPayload> {
        solution_set_artifact::materialize_response(self)
            .and_then(|source| solution_set_artifact::encode_bound_payload(source, maximum_bytes))
    }

    pub fn to_host_response(&self) -> HostAppResponse {
        self.to_host_response_with_solution_set_artifact(None)
    }

    pub fn to_host_response_with_solution_set_artifact(
        &self,
        maximum_bytes: Option<u64>,
    ) -> HostAppResponse {
        let mut diagnostics = self
            .diagnostics
            .validation()
            .diagnostics()
            .iter()
            .map(|diagnostic| {
                Diagnostic::new(
                    diagnostic.code().as_str(),
                    format!("{:?}", diagnostic.severity()).to_ascii_lowercase(),
                    diagnostic.message(),
                )
            })
            .collect::<Vec<_>>();
        if let Some(error) = &self.error {
            let code = host_error_code(error.code());
            if !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code() == code)
            {
                diagnostics.push(Diagnostic::new(code, "error", error.message()));
            }
        }

        // A validated product wrapper is the App authority for the public
        // result kind. The raw render-model kind remains the compatibility
        // fallback for every generic response.
        let result = self
            .public_result_payload
            .as_ref()
            .map(|payload| AppResult::new(payload.result_kind()))
            .or_else(|| {
                self.product_capability_result
                    .as_ref()
                    .map(|result| AppResult::new(result.result_kind().as_str()))
            })
            .or_else(|| self.result.clone());

        let product_result_payload = self.public_result_payload.clone().or_else(|| {
            self.product_capability_result
                .as_ref()
                .and_then(ProductCapabilityResult::public_result_payload)
        });

        let solution_set_artifact = maximum_bytes
            .filter(|maximum| *maximum <= HOST_SOLUTION_SET_ARTIFACT_MAX_BYTES)
            .and_then(|maximum| self.bounded_solution_set_artifact_payload(maximum));

        HostAppResponse::new(self.command, host_status(self.status))
            .with_result(result)
            .with_diagnostics(diagnostics)
            .with_backend_report(self.backend_report.clone())
            .with_resource_report(self.resource_report.clone())
            .with_capability_report(self.capability_report.clone())
            .with_continuation(self.continuation.clone())
            .with_product_result_payload(product_result_payload)
            .with_solution_set_artifact(solution_set_artifact)
    }
}

fn host_status(status: AppStatus) -> HostAppStatus {
    match status {
        AppStatus::Success => HostAppStatus::Success,
        AppStatus::ValidationFailed => HostAppStatus::ValidationFailed,
        AppStatus::Unsupported => HostAppStatus::Unsupported,
        AppStatus::ExecutionFailed => HostAppStatus::ExecutionFailed,
    }
}

fn host_error_code(code: AppErrorCode) -> &'static str {
    match code {
        AppErrorCode::MissingInput => "E_APP_INPUT_REQUIRED",
        AppErrorCode::InvalidInput => "E_APP_INPUT_INVALID",
        AppErrorCode::ProblemCompileFailed => "E_PROBLEM_COMPILE_FAILED",
        AppErrorCode::ExecutionFailed => "E_APP_EXECUTION_FAILED",
        AppErrorCode::TraceUnavailable => "E_PATH_TRACE_UNAVAILABLE",
        AppErrorCode::NoSolution => "E_PATH_NO_SOLUTION",
        AppErrorCode::Unsupported => "E_PRODUCT_RUNTIME_UNSUPPORTED",
        AppErrorCode::NativeCoreUnavailable => "E_NATIVE_CORE_UNAVAILABLE",
        AppErrorCode::BackendGpuUnavailable => "E_BACKEND_GPU_UNAVAILABLE",
        AppErrorCode::CliCommandUnsupported => "E_CLI_COMMAND_UNSUPPORTED",
        AppErrorCode::PcScenarioExpectedMismatch => "E_PC_SCENARIO_EXPECTED_MISMATCH",
        AppErrorCode::RulesProfileUnknown => "E_RULES_PROFILE_UNKNOWN",
        AppErrorCode::RulesInputRequired => "E_RULES_INPUT_REQUIRED",
        AppErrorCode::RulesInputInvalid => "E_RULES_INPUT_INVALID",
        AppErrorCode::RulesExportUnsupported => "E_RULES_EXPORT_UNSUPPORTED",
        AppErrorCode::ScoringProfileUnknown => "E_SCORING_PROFILE_UNKNOWN",
        AppErrorCode::ScoringInputRequired => "E_SCORING_INPUT_REQUIRED",
        AppErrorCode::ScoringInputInvalid => "E_SCORING_INPUT_INVALID",
        AppErrorCode::ConvertInputRequired => "E_CONVERT_INPUT_REQUIRED",
        AppErrorCode::ConvertDirectionUnsupported => "E_CONVERT_DIRECTION_UNSUPPORTED",
        AppErrorCode::ConvertInputInvalid => "E_CONVERT_INPUT_INVALID",
        AppErrorCode::ContinueTokenRequired => "E_CONTINUE_TOKEN_REQUIRED",
        AppErrorCode::ContinueTokenInvalid => "E_CONTINUE_TOKEN_INVALID",
        AppErrorCode::VerifyTargetUnknown => "E_VERIFY_TARGET_UNKNOWN",
        AppErrorCode::VerifyKicksFailed => "E_VERIFY_KICKS_FAILED",
        AppErrorCode::OperationSequenceInvalid => "E_OPERATION_SEQUENCE_INPUT_INVALID",
        AppErrorCode::OperationSequenceCancelled => "E_OPERATION_SEQUENCE_CANCELLED",
        AppErrorCode::OperationSequenceTimedOut => "E_OPERATION_SEQUENCE_TIMED_OUT",
        AppErrorCode::OperationSequenceIncomplete => "E_OPERATION_SEQUENCE_INCOMPLETE",
        AppErrorCode::SequenceDependenciesInvalid => "E_SEQUENCE_DEPENDENCIES_INPUT_INVALID",
        AppErrorCode::SequenceDependenciesCancelled => "E_SEQUENCE_DEPENDENCIES_CANCELLED",
        AppErrorCode::SequenceDependenciesTimedOut => "E_SEQUENCE_DEPENDENCIES_TIMED_OUT",
        AppErrorCode::SequenceDependenciesIncomplete => "E_SEQUENCE_DEPENDENCIES_INCOMPLETE",
        AppErrorCode::UtilityParityInvalid => "E_UTILITY_PARITY_INPUT_INVALID",
        AppErrorCode::UtilityFumenInvalid => "E_UTILITY_FUMEN_INPUT_INVALID",
        AppErrorCode::UtilityRenderInvalid => "E_UTILITY_RENDER_INPUT_INVALID",
        AppErrorCode::UtilityRenderLimitExceeded => "E_UTILITY_RENDER_LIMIT_EXCEEDED",
        AppErrorCode::UtilityToGrayInvalid => "E_UTILITY_TO_GRAY_INPUT_INVALID",
        AppErrorCode::UtilityMirrorInvalid => "E_UTILITY_MIRROR_INPUT_INVALID",
    }
}

#[cfg(test)]
mod tests {
    use clearra_host_contract::{
        ExecutionAvailabilityReason, ExecutionAvailabilityReport, ExecutionSurface,
        ProductBuildIdentity,
    };
    use clearra_validation::{
        diagnostic::{
            diagnostic::Diagnostic as ValidationDiagnostic, diagnostic_code::DiagnosticCode,
            diagnostic_report::DiagnosticReport as ValidationDiagnosticReport,
            suggested_next_step::SuggestedNextStep,
        },
        evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
    };

    use super::*;

    fn allocated_text(capacity: usize, value: &str) -> String {
        let mut text = String::with_capacity(capacity);
        text.push_str(value);
        text
    }

    fn non_empty_validation_report() -> ValidationDiagnosticReport {
        let diagnostic = ValidationDiagnostic::new(
            DiagnosticCode::EBuildQueryInvalid,
            allocated_text(128, "build fixture diagnostic"),
        )
        .with_location(EvidenceLocation::new(allocated_text(
            80,
            "app_response.resource_report",
        )))
        .with_evidence(ValidationEvidence::new(
            allocated_text(64, "actual"),
            allocated_text(96, "expected"),
        ))
        .with_suggested_next_step(SuggestedNextStep::new(allocated_text(
            144,
            "use the canonical Build contract",
        )));
        let mut report = ValidationDiagnosticReport::new();
        report.push(diagnostic);
        report
    }

    #[test]
    fn host_response_automatically_carries_the_compiled_product_identity() {
        let response = AppResponse::failed(
            AppStatus::Unsupported,
            AppError::new(AppErrorCode::Unsupported, "unsupported in focused test"),
        );

        assert_eq!(
            response.to_host_response().runtime_identity(),
            &ProductBuildIdentity::current()
        );
    }

    #[test]
    fn build_response_actual_capacity_has_exact_peak_and_peak_minus_one_boundary() {
        let core_result = CoreExecutionResult::new(
            vec![
                (
                    allocated_text(64, "search_kind"),
                    allocated_text(96, "build-probability"),
                ),
                (
                    allocated_text(72, "memory_status"),
                    allocated_text(112, "reported"),
                ),
            ],
            Vec::new(),
        );
        let core_heap_bytes = core_result
            .checked_resource_retained_bytes()
            .and_then(|bytes| {
                bytes.checked_sub(core::mem::size_of::<CoreExecutionResult>() as u128)
            })
            .expect("core heap capacity fits u128");

        let diagnostics = AppDiagnosticReport::new(non_empty_validation_report());
        let backend_report = BackendReport::new(
            allocated_text(80, "gpu"),
            allocated_text(96, "wasm-cpu"),
            Some(allocated_text(112, "adapter-unavailable")),
        )
        .with_gpu_execution_failure(
            Some(allocated_text(128, "adapter-request")),
            Some(allocated_text(144, "device-selection")),
            Some(allocated_text(160, "wasm-cpu")),
            true,
        )
        .with_gpu_device(
            Some(allocated_text(176, "discrete")),
            Some(1),
            Some(allocated_text(192, "fixture adapter")),
            Some(allocated_text(208, "discrete-gpu")),
            Some(allocated_text(224, "vulkan")),
        );
        let availability = ExecutionAvailabilityReport::exhausted(
            ExecutionSurface::BrowserWasm32,
            ExecutionAvailabilityReason::MemoryBudgetExceeded,
        )
        .with_pattern_evidence(100, 200, 300)
        .with_required_memory_bytes(400);
        let resource_report = ResourceReport::new(true, allocated_text(128, "reported"))
            .with_truncation(allocated_text(160, "memory_exceeded"))
            .with_execution_availability(availability);
        let capability_report = CapabilityReport::new(
            allocated_text(144, "clearra-app/AppRequest"),
            allocated_text(176, "validation-before-executor"),
        )
        .with_render_capability(HostRenderCapabilityReport::new(
            false,
            false,
            false,
            Some(allocated_text(208, "renderer_not_in_wasm_artifact")),
        ));
        let result = AppResult::new(allocated_text(128, "build-probability"));
        let expected = result
            .checked_retained_capacity_bytes()
            .and_then(|bytes| {
                bytes.checked_add(diagnostics.validation().checked_retained_capacity_bytes()?)
            })
            .and_then(|bytes| bytes.checked_add(backend_report.checked_retained_capacity_bytes()?))
            .and_then(|bytes| bytes.checked_add(resource_report.checked_retained_capacity_bytes()?))
            .and_then(|bytes| {
                bytes.checked_add(capability_report.checked_retained_capacity_bytes()?)
            })
            .and_then(|bytes| bytes.checked_add(core_heap_bytes))
            .expect("response retained capacity fits u128");
        let response = AppResponse {
            command: Some(AppCommandKind::BuildProbability),
            status: AppStatus::Success,
            result: Some(result),
            diagnostics,
            backend_report,
            resource_report,
            capability_report,
            continuation: None,
            render_model: Some(AppRenderModel::BuildProbability(core_result)),
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
        let actual = response
            .checked_retained_capacity_bytes()
            .expect("authorized Build response shape");
        let admits = |limit: u128| actual <= limit;

        assert_eq!(actual, expected);
        assert!(admits(actual), "the exact observed peak must be admitted");
        assert!(actual > 0);
        assert!(
            !admits(actual - 1),
            "one byte below the exact observed peak must be rejected"
        );
    }

    #[test]
    fn build_response_capacity_fails_closed_for_continuation_authority() {
        let mut response = AppResponse::success(AppRenderModel::BuildProbability(
            CoreExecutionResult::new(Vec::new(), Vec::new()),
        ));
        response.continuation = Some(ContinuationReport::new(true, Some("opaque-continuation")));

        assert_eq!(response.checked_retained_capacity_bytes(), None);
    }
}
