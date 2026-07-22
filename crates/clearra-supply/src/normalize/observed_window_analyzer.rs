use crate::{
    bag::{bag_boundary::BagBoundaryReport, bag_profile::BagProfile},
    normalize::ambiguity_report::{AmbiguityReason, AmbiguityReport},
    queue::observed_queue::ObservedQueue,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedWindowAnalysis {
    boundary_report: BagBoundaryReport,
    ambiguity_report: Option<AmbiguityReport>,
}

impl ObservedWindowAnalysis {
    pub fn boundary_report(&self) -> &BagBoundaryReport {
        &self.boundary_report
    }
}
impl ObservedWindowAnalysis {
    pub fn ambiguity_report(&self) -> Option<&AmbiguityReport> {
        self.ambiguity_report.as_ref()
    }
}

pub fn analyze_observed_window(queue: &ObservedQueue) -> ObservedWindowAnalysis {
    analyze_observed_window_with_bag_profile(queue, &BagProfile::standard_7())
}

pub fn analyze_observed_window_with_bag_profile(
    queue: &ObservedQueue,
    bag_profile: &BagProfile,
) -> ObservedWindowAnalysis {
    let boundary_report =
        BagBoundaryReport::analyze_observed_window_with_profile(queue.pieces(), bag_profile);
    let ambiguity_report = if queue.is_empty() {
        Some(AmbiguityReport::new(
            AmbiguityReason::EmptyObservedWindow,
            queue.len(),
            boundary_report.candidates().to_vec(),
        ))
    } else if boundary_report.is_compatible() && boundary_report.is_ambiguous() {
        Some(AmbiguityReport::new(
            AmbiguityReason::MultipleBoundaryCandidates,
            queue.len(),
            boundary_report.candidates().to_vec(),
        ))
    } else {
        None
    };

    ObservedWindowAnalysis {
        boundary_report,
        ambiguity_report,
    }
}
