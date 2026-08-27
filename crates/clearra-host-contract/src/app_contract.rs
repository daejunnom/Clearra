use crate::{
    AppCommandKind, AppResult, BackendPolicy, BackendReport, CapabilityReport, ContinuationReport,
    DiagnosticsPolicy, LocalePolicy, OutputPolicy, ProductBuildIdentity, ProductResultPayload,
    QueryEnvelope, ResourceBudget, ResourceReport, SolutionSetArtifactPayload,
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

    /// Returns the heap payload retained by all diagnostic strings, measured
    /// from their actual allocator capacities.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.code.capacity() as u128)
            .checked_add(self.severity.capacity() as u128)?
            .checked_add(self.message.capacity() as u128)
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

    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    /// Returns the outer diagnostic buffer and every nested diagnostic string
    /// using actual allocator capacities.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.diagnostics.capacity() as u128)
            .checked_mul(core::mem::size_of::<Diagnostic>() as u128)?;
        for diagnostic in &self.diagnostics {
            bytes = bytes.checked_add(diagnostic.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
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
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    product_result_payload: Option<ProductResultPayload>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    solution_set_artifact: Option<SolutionSetArtifactPayload>,
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
            product_result_payload: None,
            solution_set_artifact: None,
        }
    }

    /// Allocation-free owned-parts seam for a boundary that has already
    /// authorized every retained component. This intentionally bypasses the
    /// default constructors, which allocate product identity and report
    /// strings for the compatibility API.
    #[allow(clippy::too_many_arguments)]
    pub fn from_owned_memory_authorized_parts(
        runtime_identity: ProductBuildIdentity,
        command: Option<AppCommandKind>,
        status: AppStatus,
        result: Option<AppResult>,
        diagnostics: Vec<Diagnostic>,
        backend_report: BackendReport,
        resource_report: ResourceReport,
        capability_report: CapabilityReport,
        continuation: Option<ContinuationReport>,
    ) -> Self {
        Self {
            runtime_identity,
            command,
            status,
            result,
            diagnostics,
            backend_report,
            resource_report,
            capability_report,
            continuation,
            product_result_payload: None,
            solution_set_artifact: None,
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
    pub fn with_product_result_payload(
        mut self,
        product_result_payload: Option<ProductResultPayload>,
    ) -> Self {
        self.product_result_payload = product_result_payload;
        self
    }

    pub fn with_solution_set_artifact(
        mut self,
        solution_set_artifact: Option<SolutionSetArtifactPayload>,
    ) -> Self {
        self.solution_set_artifact = solution_set_artifact;
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

    pub fn product_result_payload(&self) -> Option<&ProductResultPayload> {
        self.product_result_payload.as_ref()
    }

    pub fn solution_set_artifact(&self) -> Option<&SolutionSetArtifactPayload> {
        self.solution_set_artifact.as_ref()
    }

    /// Returns the complete response-owned heap graph, field by field, using
    /// actual capacities. Inline owners and enum discriminants are excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = self.runtime_identity.checked_retained_capacity_bytes()?;
        if let Some(result) = &self.result {
            bytes = bytes.checked_add(result.checked_retained_capacity_bytes()?)?;
        }
        bytes = bytes.checked_add(
            (self.diagnostics.capacity() as u128)
                .checked_mul(core::mem::size_of::<Diagnostic>() as u128)?,
        )?;
        for diagnostic in &self.diagnostics {
            bytes = bytes.checked_add(diagnostic.checked_retained_capacity_bytes()?)?;
        }
        bytes = bytes.checked_add(self.backend_report.checked_retained_capacity_bytes()?)?;
        bytes = bytes.checked_add(self.resource_report.checked_retained_capacity_bytes()?)?;
        bytes = bytes.checked_add(self.capability_report.checked_retained_capacity_bytes()?)?;
        if let Some(continuation) = &self.continuation {
            bytes = bytes.checked_add(continuation.checked_retained_capacity_bytes()?)?;
        }
        if let Some(payload) = &self.product_result_payload {
            bytes = bytes.checked_add(payload.checked_retained_capacity_bytes()?)?;
        }
        if let Some(artifact) = &self.solution_set_artifact {
            bytes = bytes.checked_add(artifact.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

#[cfg(test)]
mod retained_capacity_tests {
    use super::{AppResponse, AppStatus, Diagnostic, DiagnosticReport};
    use crate::{
        AppResult, BackendReport, CapabilityReport, ContinuationReport, ProductBuildIdentity,
        ResourceReport,
    };

    fn allocated(capacity: usize, value: &str) -> String {
        let mut output = String::with_capacity(capacity);
        output.push_str(value);
        output
    }

    #[test]
    fn diagnostic_report_counts_outer_slots_and_nested_string_slack() {
        let mut diagnostics = Vec::with_capacity(5);
        diagnostics.push(Diagnostic::new(
            allocated(32, "E_TEST"),
            allocated(48, "error"),
            allocated(96, "message"),
        ));
        let report = DiagnosticReport::new(diagnostics);
        let diagnostic = &report.diagnostics()[0];
        let expected = (report.diagnostics.capacity() as u128)
            .checked_mul(core::mem::size_of::<Diagnostic>() as u128)
            .and_then(|bytes| bytes.checked_add(diagnostic.checked_retained_capacity_bytes()?));

        assert_eq!(report.checked_retained_capacity_bytes(), expected);
    }

    #[test]
    fn app_response_counts_actual_nested_capacities_and_exact_boundary() {
        let mut diagnostics = Vec::with_capacity(3);
        diagnostics.push(Diagnostic::new(
            allocated(32, "E_TEST"),
            allocated(48, "error"),
            allocated(96, "message"),
        ));
        let response = AppResponse::from_owned_memory_authorized_parts(
            ProductBuildIdentity::from_owned_memory_authorized_parts(
                allocated(40, "engine"),
                allocated(48, "source"),
                allocated(56, "contract"),
                allocated(64, "supply"),
                allocated(72, "artifact"),
            ),
            None,
            AppStatus::Success,
            Some(AppResult::new(allocated(80, "kind"))),
            diagnostics,
            BackendReport::new(allocated(88, "cpu"), allocated(96, "cpu"), None::<String>),
            ResourceReport::new(true, allocated(104, "reported")),
            CapabilityReport::new(allocated(112, "app"), allocated(120, "executor")),
            Some(ContinuationReport::new(true, Some(allocated(128, "next")))),
        );
        let actual = response
            .checked_retained_capacity_bytes()
            .expect("response capacity fits u128");
        let admits = |limit: u128| actual <= limit;

        assert!(admits(actual));
        assert!(actual > 0);
        assert!(!admits(actual - 1));
    }
}
