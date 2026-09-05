// Preserve the published `diagnostic::diagnostic::Diagnostic` path used by CLI and GUI hosts.
#[allow(clippy::module_inception)]
pub mod diagnostic;
pub mod diagnostic_code;
mod diagnostic_code_string;
#[cfg(test)]
mod diagnostic_code_tests;
pub mod diagnostic_namespace;
pub mod diagnostic_report;
pub mod diagnostic_severity;
mod diagnostic_severity_mapping;
pub mod gpu_worker_diagnostic;
pub mod suggested_next_step;
