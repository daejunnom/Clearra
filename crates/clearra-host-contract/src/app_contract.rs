use crate::{
    AppCommandKind, AppResult, BackendPolicy, BackendReport, CapabilityReport, ContinuationReport,
    DiagnosticsPolicy, LocalePolicy, OutputPolicy, ProductBuildIdentity, QueryEnvelope,
    ResourceBudget, ResourceReport,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum AppStatus {
    Success,
    ValidationFailed,
    Unsupported,
    ExecutionFailed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Diagnostic {
    code: String,
    severity: String,
    message: String,
}

impl Diagnostic {
    pub fn new(
        code: impl Into<String>,
        severity: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity: severity.into(),
            message: message.into(),
        }
    }
}
impl Diagnostic {
    pub fn code(&self) -> &str {
        &self.code
    }
}
impl Diagnostic {
    pub fn severity(&self) -> &str {
        &self.severity
    }
}
impl Diagnostic {
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiagnosticReport {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticReport {
    pub fn new(diagnostics: Vec<Diagnostic>) -> Self {
        Self { diagnostics }
    }
}
impl DiagnosticReport {
    pub fn single(diagnostic: Diagnostic) -> Self {
        Self::new(vec![diagnostic])
    }
}
impl DiagnosticReport {
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AppRequest {
    command: AppCommandKind,
    query: QueryEnvelope,
    backend_policy: BackendPolicy,
    output_policy: OutputPolicy,
    diagnostics_policy: DiagnosticsPolicy,
    locale_policy: LocalePolicy,
    resource_budget: ResourceBudget,
}

impl AppRequest {
    pub fn new(command: AppCommandKind, query: QueryEnvelope) -> Self {
        Self {
            command,
            query,
            backend_policy: BackendPolicy::default(),
            output_policy: OutputPolicy::default(),
            diagnostics_policy: DiagnosticsPolicy::default(),
            locale_policy: LocalePolicy::default(),
            resource_budget: ResourceBudget::default(),
        }
    }
}
impl AppRequest {
    pub fn with_backend_policy(mut self, backend_policy: BackendPolicy) -> Self {
        self.backend_policy = backend_policy;
        self
    }
}
impl AppRequest {
    pub fn with_output_policy(mut self, output_policy: OutputPolicy) -> Self {
        self.output_policy = output_policy;
        self
    }
}
impl AppRequest {
    pub fn with_diagnostics_policy(mut self, diagnostics_policy: DiagnosticsPolicy) -> Self {
        self.diagnostics_policy = diagnostics_policy;
        self
    }
}
impl AppRequest {
    pub fn with_locale_policy(mut self, locale_policy: LocalePolicy) -> Self {
        self.locale_policy = locale_policy;
        self
    }
}
impl AppRequest {
    pub fn with_resource_budget(mut self, resource_budget: ResourceBudget) -> Self {
        self.resource_budget = resource_budget;
        self
    }
}
impl AppRequest {
    pub const fn command(&self) -> AppCommandKind {
        self.command
    }
}
impl AppRequest {
    pub fn query(&self) -> &QueryEnvelope {
        &self.query
    }
}
impl AppRequest {
    pub fn backend_policy(&self) -> &BackendPolicy {
        &self.backend_policy
    }
}
impl AppRequest {
    pub fn output_policy(&self) -> &OutputPolicy {
        &self.output_policy
    }
}
impl AppRequest {
    pub const fn diagnostics_policy(&self) -> DiagnosticsPolicy {
        self.diagnostics_policy
    }
}
impl AppRequest {
    pub fn locale_policy(&self) -> &LocalePolicy {
        &self.locale_policy
    }
}
impl AppRequest {
    pub const fn resource_budget(&self) -> ResourceBudget {
        self.resource_budget
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AppResponse {
    runtime_identity: ProductBuildIdentity,
    command: Option<AppCommandKind>,
    status: AppStatus,
    result: Option<AppResult>,
    diagnostics: Vec<Diagnostic>,
    backend_report: BackendReport,
    resource_report: ResourceReport,
    capability_report: CapabilityReport,
    continuation: Option<ContinuationReport>,
}

impl AppResponse {
    pub fn new(command: Option<AppCommandKind>, status: AppStatus) -> Self {
        Self {
            runtime_identity: ProductBuildIdentity::current(),
            command,
            status,
            result: None,
            diagnostics: Vec::new(),
            backend_report: BackendReport::default(),
            resource_report: ResourceReport::default(),
            capability_report: CapabilityReport::default(),
            continuation: None,
        }
    }
}
impl AppResponse {
    pub fn success(command: AppCommandKind, result: AppResult) -> Self {
        Self::new(Some(command), AppStatus::Success).with_result(Some(result))
    }
}
impl AppResponse {
    pub fn with_result(mut self, result: Option<AppResult>) -> Self {
        self.result = result;
        self
    }
}
impl AppResponse {
    pub fn with_diagnostics(mut self, diagnostics: Vec<Diagnostic>) -> Self {
        self.diagnostics = diagnostics;
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
        self.capability_report = capability_report;
        self
    }
}
impl AppResponse {
    pub fn with_continuation(mut self, continuation: Option<ContinuationReport>) -> Self {
        self.continuation = continuation;
        self
    }
}
impl AppResponse {
    pub fn runtime_identity(&self) -> &ProductBuildIdentity {
        &self.runtime_identity
    }
}
impl AppResponse {
    pub const fn command(&self) -> Option<AppCommandKind> {
        self.command
    }
}
impl AppResponse {
    pub const fn status(&self) -> AppStatus {
        self.status
    }
}
impl AppResponse {
    pub fn result(&self) -> Option<&AppResult> {
        self.result.as_ref()
    }
}
impl AppResponse {
    pub fn diagnostics(&self) -> &[Diagnostic] {
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
