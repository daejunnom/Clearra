use super::*;

#[test]
fn standard_10_profile_has_expected_dimensions() {
    let profile = standard_10_board_profile();

    assert_eq!(profile.size().width(), STANDARD_10_WIDTH);
    assert_eq!(profile.size().height(), STANDARD_VISIBLE_HEIGHT);
    assert!(profile.is_standard_10());
}

#[test]
fn standard_analysis_size_uses_requested_line_count() {
    let size = standard_10_analysis_size(6).expect("6-line analysis board is valid");

    assert_eq!(size.width(), STANDARD_10_WIDTH);
    assert_eq!(size.height(), 6);
}
