use clearra_pc_graph::request::PcExecutionPolicy;

use crate::diagnostic::diagnostic_report::DiagnosticReport;

pub use super::pc_execution_policy_capability_validator::PcBackendCompatibilityContext;
use super::{
    pc_execution_policy_capability_validator::validate_backend_capabilities,
    pc_execution_policy_field_validator::validate_execution_policy_fields,
};

pub fn validate_pc_execution_policy(
    policy: &PcExecutionPolicy,
    context: PcBackendCompatibilityContext,
    location: &'static str,
) -> DiagnosticReport {
    let mut report = DiagnosticReport::new();

    validate_execution_policy_fields(policy, location, &mut report);

    validate_backend_capabilities(policy, context, location, &mut report);

    report
}

#[cfg(test)]
#[path = "pc_execution_policy_validator_tests.rs"]
mod tests;
