use super::*;

#[test]
fn board64_ffi_layout_matches_c_surface_shape() {
    let layout = CBoard64Layout {
        width: 10,
        height: 4,
        cell_count: 40,
        all_cells_mask: (1_u64 << 40) - 1,
    };

    assert_eq!(layout.width, 10);
    assert_eq!(layout.cell_count, 40);
    assert_eq!(CBoard64Status::Collision as u8, 4);
}

#[test]
fn board128_and_wide_ffi_layouts_match_c_dispatch_surface() {
    let board128_capability = CBoardBackendCapability {
        backend_kind: C_BOARD_BACKEND_BOARD128,
        descriptor_supported: 1,
        basic_ops_supported: 1,
        operation_mask_supported: 1,
        runtime_connected: 0,
        packing_supported: 0,
        reserved: [0, 0, 0],
        unsupported_reason: C_BOARD_UNSUPPORTED_REASON_BOARD_BACKEND_NOT_CONNECTED,
    };
    let board128 = CBoard128Descriptor {
        width: 10,
        height: 12,
        cell_count: 120,
        reserved: 0,
        all_cells_mask_lo: u64::MAX,
        all_cells_mask_hi: (1_u64 << 56) - 1,
    };
    let wide = CWideBoardDescriptor {
        width: 16,
        height: 20,
        cell_count: 320,
    };
    let mask = CGenericBoardMask {
        backend_kind: C_BOARD_BACKEND_WIDE,
        word_count: 0,
        words: [0, 0, 0, 0],
        wide_start: 48,
        wide_len: 16,
    };

    assert_eq!(CBoardStatus::UnsupportedBackend as u8, 5);
    assert_eq!(
        board128_capability.unsupported_reason,
        C_BOARD_UNSUPPORTED_REASON_BOARD_BACKEND_NOT_CONNECTED
    );
    assert_eq!(board128.all_cells_mask_hi, (1_u64 << 56) - 1);
    assert_eq!(wide.cell_count, 320);
    assert_eq!(mask.backend_kind, C_BOARD_BACKEND_WIDE);
}

#[test]
fn wide_board_runtime_not_connected_reports_reason_in_ffi_surface() {
    let capability = CBoardBackendCapability {
        backend_kind: C_BOARD_BACKEND_WIDE,
        descriptor_supported: 1,
        basic_ops_supported: 0,
        operation_mask_supported: 0,
        runtime_connected: 0,
        packing_supported: 0,
        reserved: [0, 0, 0],
        unsupported_reason: C_BOARD_UNSUPPORTED_REASON_WIDE_BOARD_RUNTIME_NOT_CONNECTED,
    };

    assert_eq!(capability.backend_kind, C_BOARD_BACKEND_WIDE);
    assert_eq!(capability.operation_mask_supported, 0);
    assert_eq!(
        capability.unsupported_reason,
        C_BOARD_UNSUPPORTED_REASON_WIDE_BOARD_RUNTIME_NOT_CONNECTED
    );
}
