use clearra_core_domain::piece::rotation::RotationState;
use clearra_replay::RotationRequest;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KickTableProfileId(String);

impl KickTableProfileId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}
impl KickTableProfileId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedKickTableProfileId(String);

impl VerifiedKickTableProfileId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}
impl VerifiedKickTableProfileId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BoardAnchor {
    pub x: i16,
    pub y: i16,
}

impl BoardAnchor {
    pub const fn new(x: i16, y: i16) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KickEvidence {
    pub from_rotation: RotationState,
    pub to_rotation: RotationState,
    pub rotation_request: RotationRequest,
    pub kick_index: u8,
    pub kick_dx: i16,
    pub kick_dy: i16,
    pub kick_table_id: KickTableProfileId,
    pub kick_profile_id: Option<VerifiedKickTableProfileId>,
    pub first_success_confirmed: bool,
    pub predecessor_anchor: BoardAnchor,
    pub result_anchor: BoardAnchor,
}

impl KickEvidence {
    pub fn first_success(
        from_rotation: RotationState,
        to_rotation: RotationState,
        rotation_request: RotationRequest,
        kick_index: u8,
        kick_dx: i16,
        kick_dy: i16,
        kick_table_id: impl Into<String>,
    ) -> Self {
        Self {
            from_rotation,
            to_rotation,
            rotation_request,
            kick_index,
            kick_dx,
            kick_dy,
            kick_table_id: KickTableProfileId::new(kick_table_id),
            kick_profile_id: None,
            first_success_confirmed: true,
            predecessor_anchor: BoardAnchor::default(),
            result_anchor: BoardAnchor::default(),
        }
    }
}
impl KickEvidence {
    pub fn with_verified_profile(mut self, profile_id: impl Into<String>) -> Self {
        self.kick_profile_id = Some(VerifiedKickTableProfileId::new(profile_id));
        self
    }
}
impl KickEvidence {
    pub fn with_anchors(mut self, predecessor: BoardAnchor, result: BoardAnchor) -> Self {
        self.predecessor_anchor = predecessor;
        self.result_anchor = result;
        self
    }
}
impl KickEvidence {
    pub fn has_exact_first_success(&self) -> bool {
        self.first_success_confirmed && self.kick_profile_id.is_some()
    }
}
