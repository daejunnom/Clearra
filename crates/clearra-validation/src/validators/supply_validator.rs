use clearra_supply::{
    bag::bag_profile::BagProfile,
    custom_bag::CustomBagRuntimeGuard,
    mixed::{CustomBagProfile, SupplyProfile},
    queue::{
        bag_aligned_pattern::BagAlignedPattern, fixed_queue::FixedQueue,
        fixed_sequence::FixedSequence, observed_queue::ObservedQueue,
    },
};

use crate::diagnostic::diagnostic_report::DiagnosticReport;

use super::{
    supply_bag_pattern_validator::validate_bag_aligned_pattern_with_profile,
    supply_diagnostic_builder::{
        custom_bag_runtime_guard_diagnostic, custom_bag_runtime_guard_diagnostic_from_guard,
    },
    supply_fixed_queue_validator::validate_fixed_sequence_at,
    supply_observed_queue_validator::validate_observed_queue_with_profile,
};

pub fn validate_fixed_queue(queue: &FixedQueue) -> DiagnosticReport {
    validate_fixed_sequence(queue)
}

pub fn validate_fixed_sequence(sequence: &FixedSequence) -> DiagnosticReport {
    validate_fixed_sequence_at(sequence, "supply.fixed_sequence")
}

pub fn validate_bag_aligned_pattern(pattern: &BagAlignedPattern) -> DiagnosticReport {
    validate_bag_aligned_pattern_with_bag_profile(pattern, &BagProfile::standard_7())
}

pub fn validate_bag_aligned_pattern_with_bag_profile(
    pattern: &BagAlignedPattern,
    bag_profile: &BagProfile,
) -> DiagnosticReport {
    validate_bag_aligned_pattern_with_profile(pattern, bag_profile)
}

pub fn validate_observed_queue(queue: &ObservedQueue) -> DiagnosticReport {
    validate_observed_queue_with_bag_profile(queue, &BagProfile::standard_7())
}

pub fn validate_observed_queue_with_bag_profile(
    queue: &ObservedQueue,
    bag_profile: &BagProfile,
) -> DiagnosticReport {
    validate_observed_queue_with_profile(queue, bag_profile)
}

pub fn validate_custom_bag_profile_mvp3_guard(bag_profile: &CustomBagProfile) -> DiagnosticReport {
    let mut report = DiagnosticReport::new();
    report.push(custom_bag_runtime_guard_diagnostic(bag_profile));
    report
}

pub fn validate_supply_profile_mvp3_guard(profile: &SupplyProfile) -> DiagnosticReport {
    let mut report = DiagnosticReport::new();
    if let Some(guard) = CustomBagRuntimeGuard::from_supply_profile(profile) {
        report.push(custom_bag_runtime_guard_diagnostic_from_guard(
            &guard, true, 1, 1,
        ));
    }
    report
}

#[cfg(test)]
#[path = "supply_validator_tests.rs"]
mod tests;
