use super::spin_accuracy::TraceCompleteness;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BoardAnchor {
    pub x: i16,
    pub y: i16,
}

impl BoardAnchor {
    pub fn new(x: i16, y: i16) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RotationRequest {
    #[default]
    None,
    Clockwise,
    CounterClockwise,
    HalfTurn,
}

impl RotationRequest {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Clockwise => "clockwise",
            Self::CounterClockwise => "counter-clockwise",
            Self::HalfTurn => "half-turn",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KickEvidence {
    pub from_rotation: u8,
    pub to_rotation: u8,
    pub rotation_request: RotationRequest,
    pub kick_index: u8,
    pub kick_dx: i16,
    pub kick_dy: i16,
    pub kick_table_id: String,
    pub kick_profile_id: Option<String>,
    pub first_success_confirmed: bool,
    pub predecessor_anchor: BoardAnchor,
    pub result_anchor: BoardAnchor,
}

impl KickEvidence {
    pub fn first_success(
        from_rotation: u8,
        to_rotation: u8,
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
            kick_table_id: kick_table_id.into(),
            kick_profile_id: None,
            first_success_confirmed: true,
            predecessor_anchor: BoardAnchor::default(),
            result_anchor: BoardAnchor::default(),
        }
    }
}
impl KickEvidence {
    pub fn stable_signature(&self) -> String {
        format!(
            "table={};from={};to={};request={};kick={};dx={};dy={}",
            self.kick_table_id,
            self.from_rotation,
            self.to_rotation,
            self.rotation_request.as_str(),
            self.kick_index,
            self.kick_dx,
            self.kick_dy
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MovementInfo {
    pub immobile: bool,
    pub rotation_used: bool,
    pub evidence_complete: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinClassificationInput {
    pub piece: char,
    pub rotation: u8,
    pub x: i16,
    pub y: i16,
    pub board_before: u64,
    pub board_after_placement: u64,
    pub board_after_clear: u64,
    pub cleared_lines: u8,
    pub blocked_corners: u8,
    pub front_corners: u8,
    pub kick_evidence: Option<KickEvidence>,
    pub movement_info: MovementInfo,
    pub trace_completeness: TraceCompleteness,
}

impl SpinClassificationInput {
    pub fn new(piece: char, cleared_lines: u8) -> Self {
        Self {
            piece: piece.to_ascii_uppercase(),
            rotation: 0,
            x: 0,
            y: 0,
            board_before: 0,
            board_after_placement: 0,
            board_after_clear: 0,
            cleared_lines,
            blocked_corners: 0,
            front_corners: 0,
            kick_evidence: None,
            movement_info: MovementInfo::default(),
            trace_completeness: TraceCompleteness::Full,
        }
    }
}
impl SpinClassificationInput {
    pub fn with_blocked_corners(mut self, blocked_corners: u8) -> Self {
        self.blocked_corners = blocked_corners;
        self
    }
}
impl SpinClassificationInput {
    pub fn with_kick_evidence(mut self, kick_evidence: KickEvidence) -> Self {
        self.kick_evidence = Some(kick_evidence);
        self
    }
}
impl SpinClassificationInput {
    pub fn has_kick_evidence(&self) -> bool {
        self.kick_evidence
            .as_ref()
            .is_some_and(|evidence| evidence.first_success_confirmed)
    }
}
