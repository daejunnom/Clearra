use super::*;

#[test]
fn mvp1_buildup_15_operation_fast_path_unchanged() {
    assert_eq!(C_BUILDUP_MAX_OPERATIONS, 15);
    assert_eq!(
        buildup_operation_set_runtime_status(15),
        C_BUILDUP_STATUS_OK
    );
}

#[test]
fn unsupported_operation_count_is_rejected_without_speculative_schema() {
    assert_eq!(
        buildup_operation_set_runtime_status(16),
        C_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE
    );
}

#[test]
fn board128_buildup_guard_reports_not_connected() {
    let board = CBoardDescriptor {
        backend_kind: C_BOARD_BACKEND_BOARD128,
        cell_count: 80,
        ..Default::default()
    };

    assert_eq!(
        buildup_runtime_status_for_board(&board),
        C_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE
    );
}
