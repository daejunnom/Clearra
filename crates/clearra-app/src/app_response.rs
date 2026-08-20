use crate::{
    app_error::{AppError, AppErrorCode},
    diagnostics::AppDiagnosticReport,
    objective_contract::objective_diagnostics_from_render_model,
    render::AppRenderModel,
    resource_contract::{
        resource_diagnostics_from_failure, resource_diagnostics_from_render_model,
        resource_report_from_failure, resource_report_from_render_model,
    },
};
use clearra_host_contract::{
    AppCommandKind, AppResponse as HostAppResponse, AppResult, AppStatus as HostAppStatus,
    BackendReport, CapabilityReport, ContinuationReport, Diagnostic,
    RenderCapabilityReport as HostRenderCapabilityReport, ResourceReport,
};
#[cfg(feature = "bitmap-render")]
use clearra_output::render::RenderExactOutputGate;

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
        }
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
    pub fn to_host_response(&self) -> HostAppResponse {
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

        HostAppResponse::new(self.command, host_status(self.status))
            .with_result(self.result.clone())
            .with_diagnostics(diagnostics)
            .with_backend_report(self.backend_report.clone())
            .with_resource_report(self.resource_report.clone())
            .with_capability_report(self.capability_report.clone())
            .with_continuation(self.continuation.clone())
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
    }
}

#[cfg(test)]
mod tests {
    use clearra_host_contract::ProductBuildIdentity;

    use super::*;

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
}
