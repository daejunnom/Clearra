use clearra_core_domain::pc::pc_target::PcTarget;
use clearra_profiles::{
    board::standard10::standard_10_board_profile,
    pieces::standard_tetrominoes::standard_tetromino_piece_set_profile,
};
use clearra_rules::profile::builtin_rules::srs;

use super::*;

#[test]
fn dispatches_capable_input_to_core_with_table_fallback_until_fast_path_exists() {
    let input = TwoLineCapabilityInput::new(
        standard_10_board_profile(),
        standard_tetromino_piece_set_profile(),
        PcTarget::two_lines(),
        srs(),
        true,
        true,
    );

    assert_eq!(
        dispatch_two_line(input).reason(),
        Some(TwoLineFallbackReason::FastPathTableUnavailable)
    );
}

#[test]
fn dispatch_separates_capability_from_fast_path_availability() {
    let input = TwoLineCapabilityInput::new(
        standard_10_board_profile(),
        standard_tetromino_piece_set_profile(),
        PcTarget::two_lines(),
        srs(),
        true,
        true,
    );

    let decision = dispatch_two_line_with_availability(input, TwoLineFastPathAvailability::mvp1());

    assert!(decision.capability().is_capable());
    assert_eq!(
        decision.fast_path().fallback_reason(),
        Some(TwoLineFallbackReason::FastPathTableUnavailable)
    );
    assert_eq!(
        decision.reason(),
        Some(TwoLineFallbackReason::FastPathTableUnavailable)
    );
}

#[test]
fn dispatch_never_selects_oracle_solver_path_even_when_fast_path_available() {
    let input = TwoLineCapabilityInput::new(
        standard_10_board_profile(),
        standard_tetromino_piece_set_profile(),
        PcTarget::two_lines(),
        srs(),
        true,
        true,
    );

    let decision = dispatch_two_line_with_availability(
        input,
        TwoLineFastPathAvailability::available_for_tests(),
    );

    assert_eq!(decision.reason(), None);
    assert!(decision.capability().is_capable());
    assert!(decision.fast_path_available());
    assert!(matches!(
        decision,
        TwoLineDispatchDecision::UseCoreSearch { .. }
    ));
}

#[test]
fn dispatches_unavailable_input_to_generic() {
    let input = TwoLineCapabilityInput::new(
        standard_10_board_profile(),
        standard_tetromino_piece_set_profile(),
        PcTarget::six_lines(),
        srs(),
        true,
        true,
    );

    assert_eq!(
        dispatch_two_line(input).reason(),
        Some(TwoLineFallbackReason::UnsupportedTargetLines { lines: 6 })
    );
    assert!(!dispatch_two_line(input).capability().is_capable());
}
