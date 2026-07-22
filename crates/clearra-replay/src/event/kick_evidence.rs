#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RotationRequest {
    #[default]
    None,
    Clockwise,
    CounterClockwise,
    HalfTurn,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KickEvidenceEvent {
    step_index: usize,
    from_rotation: u8,
    to_rotation: u8,
    rotation_request: RotationRequest,
    kick_index: u8,
    kick_dx: i16,
    kick_dy: i16,
    kick_table_id: u64,
    kick_profile_id: u64,
    first_success_confirmed: bool,
    predecessor: (i16, i16),
    result: (i16, i16),
}

impl KickEvidenceEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        step_index: usize,
        from_rotation: u8,
        to_rotation: u8,
        rotation_request: RotationRequest,
        kick_index: u8,
        kick_dx: i16,
        kick_dy: i16,
    ) -> Self {
        Self {
            step_index,
            from_rotation,
            to_rotation,
            rotation_request,
            kick_index,
            kick_dx,
            kick_dy,
            kick_table_id: 0,
            kick_profile_id: 0,
            first_success_confirmed: true,
            predecessor: (0, 0),
            result: (0, 0),
        }
    }
}
impl KickEvidenceEvent {
    pub fn with_profile_ids(mut self, kick_table_id: u64, kick_profile_id: u64) -> Self {
        self.kick_table_id = kick_table_id;
        self.kick_profile_id = kick_profile_id;
        self
    }
}
impl KickEvidenceEvent {
    pub fn with_anchors(mut self, predecessor: (i16, i16), result: (i16, i16)) -> Self {
        self.predecessor = predecessor;
        self.result = result;
        self
    }
}
impl KickEvidenceEvent {
    pub fn with_first_success_confirmed(mut self, confirmed: bool) -> Self {
        self.first_success_confirmed = confirmed;
        self
    }
}
impl KickEvidenceEvent {
    pub fn step_index(&self) -> usize {
        self.step_index
    }
}
impl KickEvidenceEvent {
    pub fn kick_index(&self) -> u8 {
        self.kick_index
    }
}
impl KickEvidenceEvent {
    pub fn kick_dx(&self) -> i16 {
        self.kick_dx
    }
}
impl KickEvidenceEvent {
    pub fn kick_dy(&self) -> i16 {
        self.kick_dy
    }
}
impl KickEvidenceEvent {
    pub fn first_success_confirmed(&self) -> bool {
        self.first_success_confirmed
    }
}
impl KickEvidenceEvent {
    pub fn from_rotation(&self) -> u8 {
        self.from_rotation
    }
}
impl KickEvidenceEvent {
    pub fn to_rotation(&self) -> u8 {
        self.to_rotation
    }
}
impl KickEvidenceEvent {
    pub fn rotation_request(&self) -> RotationRequest {
        self.rotation_request
    }
}
impl KickEvidenceEvent {
    pub fn kick_table_id(&self) -> u64 {
        self.kick_table_id
    }
}
impl KickEvidenceEvent {
    pub fn kick_profile_id(&self) -> u64 {
        self.kick_profile_id
    }
}
impl KickEvidenceEvent {
    pub fn predecessor(&self) -> (i16, i16) {
        self.predecessor
    }
}
impl KickEvidenceEvent {
    pub fn result(&self) -> (i16, i16) {
        self.result
    }
}
