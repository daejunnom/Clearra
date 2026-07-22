use crate::{
    diagnostic::{diagnostic_code::DiagnosticSeverity, diagnostic_report::DiagnosticReport},
    scope::disabled_feature_reason::DisabledFeatureReason,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MvpScopeGuard {
    disabled_reasons: Vec<DisabledFeatureReason>,
}

impl MvpScopeGuard {
    pub fn from_report(report: &DiagnosticReport) -> Self {
        let disabled_reasons = report
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.severity() == DiagnosticSeverity::Error)
            .map(|diagnostic| {
                DisabledFeatureReason::new(diagnostic.code(), diagnostic.message().to_string())
            })
            .collect();
        Self { disabled_reasons }
    }
}
impl MvpScopeGuard {
    pub fn allow() -> Self {
        Self::default()
    }
}
impl MvpScopeGuard {
    pub fn is_allowed(&self) -> bool {
        self.disabled_reasons.is_empty()
    }
}
impl MvpScopeGuard {
    pub fn disabled_reasons(&self) -> &[DisabledFeatureReason] {
        &self.disabled_reasons
    }
}

#[cfg(test)]
#[path = "mvp_scope_guard_tests.rs"]
mod tests;
