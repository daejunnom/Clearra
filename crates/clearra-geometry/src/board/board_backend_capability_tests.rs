use super::*;

#[test]
fn board64_fast_path_unchanged() {
    let capability = board_backend_capability_for_size(BoardSize::new(10, 6).expect("board64"));

    assert_eq!(capability.backend_kind(), BoardBackendKind::Board64);
    assert!(capability.runtime_connected());
    assert!(capability.packing_supported());
    assert_eq!(
        capability.unsupported_reason(),
        BoardRuntimeUnsupportedReason::None
    );
}

#[test]
fn board128_descriptor_validates_while_packing_runtime_is_guarded() {
    let capability = board_backend_capability_for_size(BoardSize::new(10, 12).expect("board128"));

    assert_eq!(capability.backend_kind(), BoardBackendKind::Board128);
    assert!(capability.descriptor_supported());
    assert!(capability.basic_ops_supported());
    assert!(capability.operation_mask_supported());
    assert!(!capability.packing_supported());
    assert_eq!(
        capability.unsupported_reason().as_str(),
        "board_backend_not_connected"
    );
}

#[test]
fn wide_board_descriptor_validates_but_runtime_reports_reason() {
    let capability = board_backend_capability_for_size(BoardSize::new(16, 20).expect("wide"));

    assert_eq!(capability.backend_kind(), BoardBackendKind::Wide);
    assert!(capability.descriptor_supported());
    assert!(!capability.operation_mask_supported());
    assert!(!capability.runtime_connected());
    assert_eq!(
        capability.unsupported_reason().as_str(),
        "wide_board_runtime_not_connected"
    );
}
