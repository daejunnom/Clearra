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

#[test]
fn pc4_supply_bound_replay_preserves_cursor_and_hold_without_changing_legacy_geometry() {
    let layout = Board64Layout::standard_10_by_lines(2).unwrap();
    let operation = BuildVariantOperation::new(PieceKind::I, RotationState::Zero, 6, 0);
    for (held, decision, consumed, output_hold) in [
        (None, HoldDecision::None, 1, None),
        (
            Some(PieceKind::O),
            HoldDecision::None,
            1,
            Some(PieceKind::O),
        ),
        (
            None,
            HoldDecision::StoreIncoming {
                stored_piece: PieceKind::O,
                drawn_piece: PieceKind::I,
            },
            2,
            Some(PieceKind::O),
        ),
        (
            Some(PieceKind::I),
            HoldDecision::SwapWithHold {
                incoming_piece: PieceKind::O,
                held_piece: PieceKind::I,
            },
            1,
            Some(PieceKind::O),
        ),
        (
            Some(PieceKind::I),
            HoldDecision::ReleaseHeldAtTerminal {
                held_piece: PieceKind::I,
            },
            1,
            Some(PieceKind::I),
        ),
    ] {
        let builder = SolutionTraceBuilder::new(layout, 0x003f, vec![operation], vec![0])
            .unwrap()
            .with_hold_decisions(vec![decision]);
        let legacy = builder.build().unwrap();
        assert_eq!(legacy.steps()[0].piece_decision().input_cursor(), 0);
        assert_eq!(legacy.steps()[0].piece_decision().output_cursor(), 1);
        assert_eq!(legacy.steps()[0].piece_decision().input_hold_piece(), None);
        let exact = builder.with_initial_supply_state(4, held).build().unwrap();
        let projected = exact.steps()[0].piece_decision();
        assert_eq!(projected.input_cursor(), 4);
        assert_eq!(projected.output_cursor(), 4 + consumed);
        assert_eq!(projected.input_hold_piece(), held);
        assert_eq!(projected.output_hold_piece(), output_hold);
        assert_eq!(projected.hold_decision(), decision);
        assert!(exact.steps()[0].board_after().after_line_clear().is_empty());
    }
}

#[test]
fn pc4_supply_bound_replay_rejects_inconsistent_selected_transitions_and_overflow() {
    for (cursor, held, decision) in [
        (usize::MAX, None, HoldDecision::None),
        (
            usize::MAX - 1,
            None,
            HoldDecision::StoreIncoming {
                stored_piece: PieceKind::O,
                drawn_piece: PieceKind::I,
            },
        ),
        (
            0,
            Some(PieceKind::T),
            HoldDecision::StoreIncoming {
                stored_piece: PieceKind::O,
                drawn_piece: PieceKind::I,
            },
        ),
        (
            0,
            None,
            HoldDecision::StoreIncoming {
                stored_piece: PieceKind::O,
                drawn_piece: PieceKind::T,
            },
        ),
        (
            0,
            None,
            HoldDecision::SwapWithHold {
                incoming_piece: PieceKind::O,
                held_piece: PieceKind::I,
            },
        ),
        (
            0,
            Some(PieceKind::T),
            HoldDecision::SwapWithHold {
                incoming_piece: PieceKind::O,
                held_piece: PieceKind::I,
            },
        ),
        (
            0,
            Some(PieceKind::I),
            HoldDecision::ReleaseHeldAtTerminal {
                held_piece: PieceKind::T,
            },
        ),
    ] {
        let builder = SolutionTraceBuilder::new(
            Board64Layout::standard_10_by_lines(2).unwrap(),
            0x003f,
            vec![BuildVariantOperation::new(
                PieceKind::I,
                RotationState::Zero,
                6,
                0,
            )],
            vec![0],
        )
        .unwrap()
        .with_hold_decisions(vec![decision])
        .with_initial_supply_state(cursor, held);
        assert_eq!(
            builder.build(),
            Err(SolutionTraceBuilderError::InvalidSupplyTransition { step_index: 0 })
        );
    }
}
