use super::*;

#[test]
fn piece_area_constraint_rejects_area_infeasible_shape_before_dlx() {
    let feasible = PieceAreaConstraint::new(7, [4, 3]).expect("constraint");
    let infeasible = PieceAreaConstraint::new(5, [4, 3]).expect("constraint");

    assert!(feasible.can_fill_target());
    assert!(!infeasible.can_fill_target());
}

#[test]
fn piece_area_constraint_rejects_zero_area_inputs() {
    assert_eq!(
        PieceAreaConstraint::new(0, [4]),
        Err(PieceAreaConstraintError::ZeroTargetArea)
    );
    assert_eq!(
        PieceAreaConstraint::new(4, [0]),
        Err(PieceAreaConstraintError::ZeroPieceArea)
    );
}
