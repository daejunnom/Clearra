use crate::{
    bag::bag_boundary::{standard_7_bag_fixed_boundary_report, BagBoundaryReport},
    bag::bag_profile::BagProfile,
    normalize::observed_window_analyzer::{
        analyze_observed_window_with_bag_profile, ObservedWindowAnalysis,
    },
    queue::{
        bag_aligned_pattern::BagAlignedPattern, fixed_queue::FixedQueue,
        fixed_sequence::FixedSequence, observed_queue::ObservedQueue,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NormalizedSupply {
    FixedSequence {
        sequence: FixedSequence,
    },
    BagAlignedPattern {
        pattern: BagAlignedPattern,
        boundary_report: BagBoundaryReport,
    },
    Observed {
        queue: ObservedQueue,
        analysis: ObservedWindowAnalysis,
    },
}

pub fn normalize_fixed_queue(queue: FixedQueue) -> NormalizedSupply {
    normalize_fixed_sequence(queue)
}

pub fn normalize_fixed_sequence(sequence: FixedSequence) -> NormalizedSupply {
    NormalizedSupply::FixedSequence { sequence }
}

pub fn normalize_bag_aligned_pattern(pattern: BagAlignedPattern) -> NormalizedSupply {
    normalize_bag_aligned_pattern_with_bag_profile(pattern, &BagProfile::standard_7())
}

pub fn normalize_bag_aligned_pattern_with_bag_profile(
    pattern: BagAlignedPattern,
    bag_profile: &BagProfile,
) -> NormalizedSupply {
    let boundary_report =
        BagBoundaryReport::analyze_fixed_queue_with_profile(pattern.pieces(), bag_profile);
    NormalizedSupply::BagAlignedPattern {
        pattern,
        boundary_report,
    }
}

pub fn normalize_observed_queue(queue: ObservedQueue) -> NormalizedSupply {
    normalize_observed_queue_with_bag_profile(queue, &BagProfile::standard_7())
}

pub fn normalize_observed_queue_with_bag_profile(
    queue: ObservedQueue,
    bag_profile: &BagProfile,
) -> NormalizedSupply {
    let analysis = analyze_observed_window_with_bag_profile(&queue, bag_profile);
    NormalizedSupply::Observed { queue, analysis }
}

pub fn boundary_report_for_fixed(queue: &FixedQueue) -> BagBoundaryReport {
    standard_7_bag_fixed_boundary_report(queue.pieces())
}

pub fn boundary_report_for_bag_aligned_pattern(pattern: &BagAlignedPattern) -> BagBoundaryReport {
    boundary_report_for_bag_aligned_pattern_with_bag_profile(pattern, &BagProfile::standard_7())
}

pub fn boundary_report_for_bag_aligned_pattern_with_bag_profile(
    pattern: &BagAlignedPattern,
    bag_profile: &BagProfile,
) -> BagBoundaryReport {
    BagBoundaryReport::analyze_fixed_queue_with_profile(pattern.pieces(), bag_profile)
}

pub fn boundary_report_for_observed(queue: &ObservedQueue) -> BagBoundaryReport {
    boundary_report_for_observed_with_bag_profile(queue, &BagProfile::standard_7())
}

pub fn boundary_report_for_observed_with_bag_profile(
    queue: &ObservedQueue,
    bag_profile: &BagProfile,
) -> BagBoundaryReport {
    BagBoundaryReport::analyze_observed_window_with_profile(queue.pieces(), bag_profile)
}
