use clearra_scoring::{
    profile::ScoreProfile,
    spin::{
        RequiredClearLines, RequiredSpinKind, SpinAccuracy, SpinKind, SpinResult, SpinTarget,
        SpinTargetId, SpinTargetPredicate,
    },
};

#[test]
fn spin_target_predicate_matches_tsd_result() {
    let profile = ScoreProfile::new("tetrio", "TETR.IO");
    let target = SpinTarget::tsd("tsd");
    let predicate = SpinTargetPredicate::new(target);
    let result = SpinResult::new('T', SpinKind::TSpin, false, 2, true, SpinAccuracy::Exact);

    assert!(predicate.evaluate_result_only(&result, &profile));
}

#[test]
fn spin_target_predicate_rejects_wrong_lines_and_profile() {
    let profile = ScoreProfile::new("other", "Other");
    let target = SpinTarget::new(
        SpinTargetId::new("all-spin-double"),
        RequiredSpinKind::AllSpin,
    )
    .with_clear_lines(RequiredClearLines::Exactly(2))
    .with_required_score_profile("all-spin-profile");
    let predicate = SpinTargetPredicate::new(target);
    let result = SpinResult::new('L', SpinKind::AllSpin, false, 1, true, SpinAccuracy::Exact);

    assert!(!predicate.evaluate_result_only(&result, &profile));
}
