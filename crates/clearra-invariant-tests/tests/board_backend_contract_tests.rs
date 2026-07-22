use clearra_core_domain::board::board_size::BoardSize;
use clearra_core_executor::board::{
    board128_state::Board128State,
    board64_state::Board64State,
    board_state_backend::BoardStateBackend,
    wide_board_state::{WideBoardMask, WideBoardState},
};
use clearra_geometry::board::{board_backend_capability_for_size, BoardRuntimeUnsupportedReason};
use clearra_geometry::layout::{
    board128_layout::Board128Layout,
    board64_layout::Board64Layout,
    board_backend::{backend_kind_for_size, BoardBackendKind, BoardLayoutBackend},
    wide_board_layout::WideBoardLayout,
};
use clearra_validation::{
    diagnostic::diagnostic_code::DiagnosticCode,
    validators::board_validator::validate_board_backend_mvp3_guard,
};

#[test]
fn board_backend_kind_selects_board64_board128_and_wide_by_area() {
    assert_eq!(
        backend_kind_for_size(BoardSize::new(10, 6).expect("board64")),
        BoardBackendKind::Board64
    );
    assert_eq!(
        backend_kind_for_size(BoardSize::new(10, 12).expect("board128")),
        BoardBackendKind::Board128
    );
    assert_eq!(
        backend_kind_for_size(BoardSize::new(16, 20).expect("wide")),
        BoardBackendKind::Wide
    );
}

#[test]
fn board_state_backend_trait_covers_collision_place_clear_row_mask_and_occupied_count() {
    let layout = Board128Layout::standard_10_by_lines(12).expect("board128");
    let board = Board128State::empty(layout);
    let placed = board.place_mask(&0x03ff).expect("place bottom row");

    assert_eq!(placed.backend_kind(), BoardBackendKind::Board128);
    assert_eq!(placed.row_mask(0), Some(0x03ff));
    assert!(placed.collides_mask(&1));
    assert_eq!(placed.occupied_count(), 10);

    let (cleared, lines) = placed.clear_full_rows();
    assert_eq!(lines, 1);
    assert!(cleared.is_empty());
}

#[test]
fn rust_geometry_metadata_bridge_tracks_board_backend_identity() {
    let board128 = Board128Layout::standard_10_by_lines(12).expect("board128");
    let wide = WideBoardLayout::new(BoardSize::new(16, 20).expect("wide"));

    assert_eq!(
        BoardLayoutBackend::backend_kind(board128),
        BoardBackendKind::Board128
    );
    assert_eq!(BoardLayoutBackend::cell_count(board128), 120);
    assert_eq!(
        BoardLayoutBackend::backend_kind(wide),
        BoardBackendKind::Wide
    );
    assert_eq!(BoardLayoutBackend::cell_count(wide), 320);
}

#[test]
fn board64_state_remains_available_for_replay_and_render_contracts_only() {
    let layout = Board64Layout::standard_10_by_lines(6).expect("board64");
    let board = Board64State::empty(layout);

    assert_eq!(board.layout(), layout);
    assert_eq!(board.occupied(), 0);
}

#[test]
fn wide_board_backend_is_a_dynamic_fallback_scaffold_for_custom_widths() {
    let layout = WideBoardLayout::new(BoardSize::new(16, 4).expect("wide"));
    let board = WideBoardState::empty(layout);
    let row = WideBoardMask::new(0..16);
    let placed = board.place_mask(&row).expect("place full custom-width row");

    assert_eq!(placed.backend_kind(), BoardBackendKind::Wide);
    assert_eq!(placed.occupied_count(), 16);
    assert_eq!(placed.clear_full_rows().1, 1);
}

#[test]
fn board128_and_wide_backends_are_guarded_until_search_runtime_is_generic() {
    let board128_report =
        validate_board_backend_mvp3_guard(BoardSize::new(10, 12).expect("board128"));
    let wide_report = validate_board_backend_mvp3_guard(BoardSize::new(16, 20).expect("wide"));

    assert!(board128_report.contains_code(DiagnosticCode::ECustomBoardUnsupportedMvp));
    assert!(board128_report.contains_code(DiagnosticCode::EBoardBackendNotConnected));
    assert!(wide_report.contains_code(DiagnosticCode::ECustomBoardUnsupportedMvp));
    assert!(wide_report.contains_code(DiagnosticCode::EWideBoardRuntimeNotConnected));
}

#[test]
fn board_backend_capability_reports_g3_runtime_boundaries() {
    let board64 = board_backend_capability_for_size(BoardSize::new(10, 6).expect("board64"));
    let board128 = board_backend_capability_for_size(BoardSize::new(10, 12).expect("board128"));
    let wide = board_backend_capability_for_size(BoardSize::new(16, 20).expect("wide"));

    assert_eq!(board64.backend_kind(), BoardBackendKind::Board64);
    assert!(board64.runtime_connected());
    assert!(board64.packing_supported());

    assert_eq!(board128.backend_kind(), BoardBackendKind::Board128);
    assert!(board128.descriptor_supported());
    assert!(board128.basic_ops_supported());
    assert!(!board128.packing_supported());
    assert_eq!(
        board128.unsupported_reason(),
        BoardRuntimeUnsupportedReason::BoardBackendNotConnected
    );

    assert_eq!(wide.backend_kind(), BoardBackendKind::Wide);
    assert!(wide.descriptor_supported());
    assert!(!wide.operation_mask_supported());
    assert!(!wide.runtime_connected());
    assert_eq!(
        wide.unsupported_reason(),
        BoardRuntimeUnsupportedReason::WideBoardRuntimeNotConnected
    );
}

#[test]
fn board128_descriptor_tests() {
    let capability = board_backend_capability_for_size(BoardSize::new(10, 12).expect("board128"));
    let layout = Board128Layout::standard_10_by_lines(12).expect("board128");
    let board = Board128State::empty(layout);

    assert_eq!(capability.backend_kind(), BoardBackendKind::Board128);
    assert!(capability.descriptor_supported());
    assert!(capability.basic_ops_supported());
    assert!(!capability.packing_supported());
    assert_eq!(board.backend_kind(), BoardBackendKind::Board128);
    assert_eq!(BoardLayoutBackend::cell_count(layout), 120);
}

#[test]
fn wide_board_runtime_not_connected() {
    let capability = board_backend_capability_for_size(BoardSize::new(16, 20).expect("wide"));
    let report = validate_board_backend_mvp3_guard(BoardSize::new(16, 20).expect("wide"));

    assert_eq!(capability.backend_kind(), BoardBackendKind::Wide);
    assert!(capability.descriptor_supported());
    assert!(!capability.runtime_connected());
    assert_eq!(
        capability.unsupported_reason(),
        BoardRuntimeUnsupportedReason::WideBoardRuntimeNotConnected
    );
    assert!(report.contains_code(DiagnosticCode::EWideBoardRuntimeNotConnected));
}
