use super::*;

#[test]
fn verify_first_result_is_not_used_for_coverage() {
    assert!(!BuildUpExecutionMode::VerifyFirst.can_source_coverage());
    assert!(BuildUpExecutionMode::EnumerateVariants.can_source_coverage());
    assert_eq!(
        BuildUpExecutionMode::coverage_producing(),
        BuildUpExecutionMode::EnumerateVariants
    );
}

#[test]
fn verify_first_result_not_used_for_min_cover() {
    assert!(!BuildUpExecutionMode::VerifyFirst.can_source_min_cover());
    assert!(BuildUpExecutionMode::EnumerateVariants.can_source_min_cover());
    assert!(!BuildUpExecutionMode::CountVariants.can_source_min_cover());
}

#[test]
fn min_cover_never_uses_verify_first() {
    assert!(!BuildUpExecutionMode::VerifyFirst.can_source_min_cover());
    assert_eq!(
        BuildUpExecutionMode::coverage_producing(),
        BuildUpExecutionMode::EnumerateVariants
    );
}
