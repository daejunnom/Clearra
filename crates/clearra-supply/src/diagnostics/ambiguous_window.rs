use crate::normalize::ambiguity_report::AmbiguityReport;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmbiguousWindow {
    report: AmbiguityReport,
}

impl AmbiguousWindow {
    pub fn new(report: AmbiguityReport) -> Self {
        Self { report }
    }
}
impl AmbiguousWindow {
    pub fn report(&self) -> &AmbiguityReport {
        &self.report
    }
}
