use super::*;

#[test]
fn search_operation_defaults_to_target_frame() {
    let operation = Operation::target_frame(
        OperationId(7),
        PieceKind::T,
        RotationState::Right,
        4,
        2,
        0x3c0,
    );

    assert!(operation.is_target_frame());
    assert!(!operation.is_lock_frame());
}

#[test]
fn replay_uses_lock_frame_coordinate() {
    let operation = Operation::target_frame(
        OperationId(3),
        PieceKind::I,
        RotationState::Zero,
        1,
        4,
        0xf000,
    )
    .with_lock_frame_y(3, 0x0f00);

    assert!(operation.is_lock_frame());
    assert_eq!(operation.y, 3);
    assert_eq!(operation.cells_mask, 0x0f00);
}

#[test]
fn operation_target_frame_y_adjusted_in_buildup() {
    replay_uses_lock_frame_coordinate();
}
