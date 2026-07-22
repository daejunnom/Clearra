use clearra_core_domain::objective::objective_kind::ObjectiveKind;
use clearra_pc_graph::request::{PcCountPolicy, PcExecutionPolicy};

use crate::{
    diagnostic::{diagnostic_code::DiagnosticCode, diagnostic_report::DiagnosticReport},
    validators::core_security_gate::CoreSecurityGate,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcBackendCompatibilityContext {
    OpeningObjective(ObjectiveKind),
    ScenarioCountPolicy(PcCountPolicy),
}

impl PcBackendCompatibilityContext {
    pub fn opening(objective: ObjectiveKind) -> Self {
        Self::OpeningObjective(objective)
    }
}
impl PcBackendCompatibilityContext {
    pub fn scenario(count_policy: PcCountPolicy) -> Self {
        Self::ScenarioCountPolicy(count_policy)
    }
}
pub(crate) fn validate_backend_capabilities(
    policy: &PcExecutionPolicy,
    context: PcBackendCompatibilityContext,
    location: &'static str,
    report: &mut DiagnosticReport,
) {
    let _ = context;

    append_backend_security_gate_diagnostics(policy, location, report);
}

fn append_backend_security_gate_diagnostics(
    policy: &PcExecutionPolicy,
    location: &'static str,
    report: &mut DiagnosticReport,
) {
    if report.contains_code(DiagnosticCode::WPcBackendFallback) {
        report.push(CoreSecurityGate::backend_fallback_used(
            policy.requested_backend().as_str(),
            "cpu-geometry-exact-cover",
            backend_security_reason(report),
            location,
        ));
    }

    if report.contains_code(DiagnosticCode::EBackendGpuFeatureDisabled)
        || report.contains_code(DiagnosticCode::EBackendGpuDeviceNotFound)
    {
        report.push(CoreSecurityGate::gpu_unavailable(
            backend_security_reason(report),
            location,
        ));
    }
}

fn backend_security_reason(report: &DiagnosticReport) -> &'static str {
    if report.contains_code(DiagnosticCode::EBackendGpuFeatureDisabled) {
        "gpu_feature_disabled"
    } else if report.contains_code(DiagnosticCode::EBackendGpuDeviceNotFound) {
        "gpu_device_not_found"
    } else {
        "backend_unsupported"
    }
}
