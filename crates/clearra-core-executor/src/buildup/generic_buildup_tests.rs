use clearra_core_ffi::{
    C_BOARD_BACKEND_BOARD128, C_BOARD_BACKEND_BOARD64, C_BOARD_BACKEND_WIDE,
    C_BUILDUP_MAX_OPERATIONS,
};

use super::*;

#[test]
fn mvp1_buildup_15_operation_fast_path_unchanged() {
    let board = CBoardDescriptor {
        backend_kind: C_BOARD_BACKEND_BOARD64,
        cell_count: 40,
        ..Default::default()
    };

    assert_eq!(C_BUILDUP_MAX_OPERATIONS, 15);
    assert_eq!(
        buildup_capability_for_board(&board),
        BuildUpCapability::ConnectedExact
    );
    assert_eq!(
        buildup_capability_for_operation_count(15),
        BuildUpCapability::ConnectedExact
    );
}

#[test]
fn unsupported_operation_count_does_not_claim_runtime_support() {
    assert_eq!(
        buildup_capability_for_operation_count(16),
        BuildUpCapability::Unsupported {
            reason: BuildUpUnsupportedReason::OperationCount
        }
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
        buildup_capability_for_board(&board),
        BuildUpCapability::Unsupported {
            reason: BuildUpUnsupportedReason::BoardBackend
        }
    );
}

#[test]
fn wide_buildup_guard_reports_not_connected() {
    let board = CBoardDescriptor {
        backend_kind: C_BOARD_BACKEND_WIDE,
        cell_count: 180,
        ..Default::default()
    };

    assert_eq!(
        buildup_capability_for_board(&board).unsupported_reason(),
        Some(BuildUpUnsupportedReason::BoardBackend)
    );
}

#[test]
fn generic_buildup_does_not_claim_solution_before_connected() {
    let board = CBoardDescriptor {
        backend_kind: C_BOARD_BACKEND_BOARD128,
        cell_count: 80,
        ..Default::default()
    };
    let capability = buildup_capability_for_board(&board);

    assert!(!capability.can_claim_solution());
    assert_eq!(
        capability.unsupported_reason(),
        Some(BuildUpUnsupportedReason::BoardBackend)
    );
}
