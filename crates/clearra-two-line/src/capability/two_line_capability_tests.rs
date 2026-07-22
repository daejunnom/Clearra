use clearra_core_domain::pc::pc_target::PcTarget;
use clearra_profiles::{
    board::standard10::standard_10_board_profile,
    pieces::standard_tetrominoes::standard_tetromino_piece_set_profile,
};
use clearra_rules::profile::builtin_rules::{custom_rule, srs_plus};

use super::*;

#[test]
fn accepts_standard_two_line_conditions() {
    let input = TwoLineCapabilityInput::new(
        standard_10_board_profile(),
        standard_tetromino_piece_set_profile(),
        PcTarget::two_lines(),
        srs_plus(),
        true,
        true,
    );

    let capability = TwoLineCapability::evaluate(input);

    assert!(capability.is_capable());
    assert_eq!(capability.fallback_reason(), None);
}

#[test]
fn rejects_non_two_line_target_before_rule_checks() {
    let input = TwoLineCapabilityInput::new(
        standard_10_board_profile(),
        standard_tetromino_piece_set_profile(),
        PcTarget::four_lines(),
        custom_rule(),
        true,
        true,
    );

    let capability = TwoLineCapability::evaluate(input);

    assert_eq!(
        capability.fallback_reason(),
        Some(TwoLineFallbackReason::UnsupportedTargetLines { lines: 4 })
    );
}

#[test]
fn rejects_when_validation_has_failed() {
    let input = TwoLineCapabilityInput::new(
        standard_10_board_profile(),
        standard_tetromino_piece_set_profile(),
        PcTarget::two_lines(),
        srs_plus(),
        true,
        false,
    );

    let capability = TwoLineCapability::evaluate(input);

    assert_eq!(
        capability.fallback_reason(),
        Some(TwoLineFallbackReason::ValidationFailed)
    );
}

#[test]
fn rejects_disabled_hold_before_using_two_line_fast_path() {
    let input = TwoLineCapabilityInput::new(
        standard_10_board_profile(),
        standard_tetromino_piece_set_profile(),
        PcTarget::two_lines(),
        srs_plus(),
        false,
        true,
    );

    let capability = TwoLineCapability::evaluate(input);

    assert_eq!(
        capability.fallback_reason(),
        Some(TwoLineFallbackReason::UnsupportedHoldDisabled)
    );
}
