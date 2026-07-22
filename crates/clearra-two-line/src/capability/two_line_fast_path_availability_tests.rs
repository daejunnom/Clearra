use super::*;

#[test]
fn mvp1_fast_path_availability_reports_missing_tables_separately_from_capability() {
    let availability = TwoLineFastPathAvailability::mvp1();

    assert!(!availability.table_available());
    assert!(!availability.runner_available());
    assert!(!availability.is_available());
    assert_eq!(
        availability.unavailable_reason(),
        Some(TwoLineFastPathUnavailableReason::TableUnavailable)
    );
    assert_eq!(
        availability.fallback_reason(),
        Some(TwoLineFallbackReason::FastPathTableUnavailable)
    );
}

#[test]
fn mvp2_fast_path_stays_unavailable_until_table_runner_trace_bridge_is_wired() {
    let availability = TwoLineFastPathAvailability::mvp2();

    assert_eq!(availability, TwoLineFastPathAvailability::current_scope());
    assert!(!availability.table_available());
    assert!(!availability.runner_available());
    assert!(!availability.is_available());
    assert_eq!(
        availability.unavailable_reason(),
        Some(TwoLineFastPathUnavailableReason::TableUnavailable)
    );
    assert_eq!(
        availability.fallback_reason(),
        Some(TwoLineFallbackReason::FastPathTableUnavailable)
    );
}

#[test]
fn runner_unavailable_is_distinct_from_table_unavailable() {
    let availability = TwoLineFastPathAvailability::new(true, false);

    assert_eq!(
        availability.unavailable_reason(),
        Some(TwoLineFastPathUnavailableReason::RunnerUnavailable)
    );
    assert_eq!(
        availability.fallback_reason(),
        Some(TwoLineFallbackReason::FastPathRunnerUnavailable)
    );
}

#[test]
fn available_fast_path_requires_tables_and_runner() {
    let availability = TwoLineFastPathAvailability::available_for_tests();

    assert!(availability.is_available());
    assert_eq!(availability.unavailable_reason(), None);
    assert_eq!(availability.fallback_reason(), None);
}
