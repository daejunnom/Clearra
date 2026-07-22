use super::*;

#[test]
fn accepts_only_four_rotation_states() {
    assert_eq!(
        RotationState::from_quarter_turns(0),
        Ok(RotationState::Zero)
    );
    assert_eq!(
        RotationState::from_quarter_turns(3),
        Ok(RotationState::Left)
    );
    assert_eq!(
        RotationState::from_quarter_turns(4),
        Err(InvalidRotationQuarterTurns)
    );
}

#[test]
fn rotates_deterministically() {
    assert_eq!(RotationState::Zero.clockwise(), RotationState::Right);
    assert_eq!(RotationState::Zero.counter_clockwise(), RotationState::Left);
    assert_eq!(RotationState::Right.rotated_180(), RotationState::Left);
}
