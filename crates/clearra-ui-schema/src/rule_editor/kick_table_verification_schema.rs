use clearra_rules::kicks::KickProfileVerificationReport;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KickTableVerificationSchema {
    issue_count: usize,
    missing_transition_count: usize,
    duplicate_transition_count: usize,
    unsupported_annotation_count: usize,
    supports_180: bool,
    transition_complete: bool,
}

impl KickTableVerificationSchema {
    pub fn from_report(report: KickProfileVerificationReport) -> Self {
        Self {
            issue_count: report.issue_count(),
            missing_transition_count: report.missing_transition_count(),
            duplicate_transition_count: report.duplicate_transition_count(),
            unsupported_annotation_count: report.unsupported_annotation_count(),
            supports_180: report.supports_180(),
            transition_complete: report.transition_complete(),
        }
    }
}
impl KickTableVerificationSchema {
    pub fn issue_count(&self) -> usize {
        self.issue_count
    }
}
impl KickTableVerificationSchema {
    pub fn missing_transition_count(&self) -> usize {
        self.missing_transition_count
    }
}
impl KickTableVerificationSchema {
    pub fn duplicate_transition_count(&self) -> usize {
        self.duplicate_transition_count
    }
}
impl KickTableVerificationSchema {
    pub fn unsupported_annotation_count(&self) -> usize {
        self.unsupported_annotation_count
    }
}
impl KickTableVerificationSchema {
    pub fn supports_180(&self) -> bool {
        self.supports_180
    }
}
impl KickTableVerificationSchema {
    pub fn transition_complete(&self) -> bool {
        self.transition_complete
    }
}
