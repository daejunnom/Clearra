use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_geometry::layout::board64_layout::Board64Layout;

use crate::replay::replay_engine::BuildVariantOperation;

use super::*;

#[test]
fn builder_preserves_line_clear_events() {
    let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
    let operation =
        BuildVariantOperation::new(PieceKind::I, RotationState::Zero, 6, 0).with_mask(0x03c0);
    let trace = SolutionTraceBuilder::new(layout, 0x003f, vec![operation], vec![0])
        .expect("builder")
        .build()
        .expect("trace");

    assert_eq!(trace.steps().len(), 1);
    assert_eq!(trace.steps()[0].line_clear().cleared_lines(), 1);
    assert!(trace.steps()[0].board_after().after_line_clear().is_empty());
}

#[test]
fn builder_rejects_duplicate_representative_order_indices() {
    let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
    let operations = vec![
        BuildVariantOperation::new(PieceKind::I, RotationState::Zero, 0, 0),
        BuildVariantOperation::new(PieceKind::O, RotationState::Zero, 4, 0),
    ];

    assert_eq!(
        SolutionTraceBuilder::new(layout, 0, operations, vec![0, 0]),
        Err(SolutionTraceBuilderError::RepresentativeOrderDuplicate { index: 0 })
    );
}
