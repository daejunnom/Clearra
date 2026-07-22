#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MovementEvidenceEvent {
    step_index: usize,
    path_complete: bool,
    last_action_was_rotation: bool,
    used_kick: bool,
    used_180: bool,
    rotation_evidence_complete: bool,
}

impl MovementEvidenceEvent {
    pub const fn new(
        step_index: usize,
        path_complete: bool,
        last_action_was_rotation: bool,
        used_kick: bool,
        used_180: bool,
        rotation_evidence_complete: bool,
    ) -> Self {
        Self {
            step_index,
            path_complete,
            last_action_was_rotation,
            used_kick,
            used_180,
            rotation_evidence_complete,
        }
    }

    pub const fn step_index(self) -> usize {
        self.step_index
    }

    pub const fn path_complete(self) -> bool {
        self.path_complete
    }

    pub const fn last_action_was_rotation(self) -> bool {
        self.last_action_was_rotation
    }

    pub const fn used_kick(self) -> bool {
        self.used_kick
    }

    pub const fn used_180(self) -> bool {
        self.used_180
    }

    pub const fn rotation_evidence_complete(self) -> bool {
        self.rotation_evidence_complete
    }
}
