use clearra_core_domain::piece::rotation::RotationState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RotationTransitionKind {
    None,
    Clockwise90,
    CounterClockwise90,
    OneEighty,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RotationRequest {
    from: RotationState,
    to: RotationState,
}

impl RotationRequest {
    pub fn new(from: RotationState, to: RotationState) -> Self {
        Self { from, to }
    }
}
impl RotationRequest {
    pub fn from(self) -> RotationState {
        self.from
    }
}
impl RotationRequest {
    pub fn to(self) -> RotationState {
        self.to
    }
}
impl RotationRequest {
    pub fn transition_kind(self) -> RotationTransitionKind {
        if self.from == self.to {
            return RotationTransitionKind::None;
        }
        if self.from.clockwise() == self.to {
            return RotationTransitionKind::Clockwise90;
        }
        if self.from.counter_clockwise() == self.to {
            return RotationTransitionKind::CounterClockwise90;
        }
        RotationTransitionKind::OneEighty
    }
}
impl RotationRequest {
    pub fn is_90(self) -> bool {
        matches!(
            self.transition_kind(),
            RotationTransitionKind::Clockwise90 | RotationTransitionKind::CounterClockwise90
        )
    }
}
impl RotationRequest {
    pub fn is_180(self) -> bool {
        self.transition_kind() == RotationTransitionKind::OneEighty
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::rotation::RotationState;

    use super::*;

    #[test]
    fn rotation_request_classifies_90_and_180_transitions() {
        assert_eq!(
            RotationRequest::new(RotationState::Zero, RotationState::Right).transition_kind(),
            RotationTransitionKind::Clockwise90
        );
        assert_eq!(
            RotationRequest::new(RotationState::Zero, RotationState::Left).transition_kind(),
            RotationTransitionKind::CounterClockwise90
        );
        assert_eq!(
            RotationRequest::new(RotationState::Zero, RotationState::Two).transition_kind(),
            RotationTransitionKind::OneEighty
        );
        assert!(RotationRequest::new(RotationState::Right, RotationState::Left).is_180());
    }
}
