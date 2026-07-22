use clearra_core_domain::{
    ids::{ScoreProfileId, SpinTargetId},
    probability::probability_value::ProbabilityValue,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpinPieceSelector {
    TOnly,
    AnyPiece,
    PieceSet(String),
}

impl SpinPieceSelector {
    pub fn matches(&self, piece: char) -> bool {
        match self {
            Self::TOnly => piece.eq_ignore_ascii_case(&'T'),
            Self::AnyPiece => true,
            Self::PieceSet(pieces) => pieces
                .chars()
                .any(|candidate| candidate.eq_ignore_ascii_case(&piece)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequiredSpinKind {
    RegularSpin,
    MiniSpin,
    TSpin,
    TSpinMini,
    AllSpin,
    AllSpinMini,
    ProfileSpecific(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequiredClearLines {
    Any,
    Exactly(u8),
    AtLeast(u8),
}

impl RequiredClearLines {
    pub fn matches(self, lines: u8) -> bool {
        match self {
            Self::Any => true,
            Self::Exactly(expected) => lines == expected,
            Self::AtLeast(minimum) => lines >= minimum,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpinMiniPolicy {
    RegularOnly,
    MiniAllowed,
    MiniOnly,
    AllSpinAsMini,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RequiredClearKind {
    #[default]
    Any,
    LineClear,
    PerfectClear,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpinTargetRequest {
    id: SpinTargetId,
    spin_piece_selector: SpinPieceSelector,
    spin_kind: RequiredSpinKind,
    clear_lines: RequiredClearLines,
    mini_policy: SpinMiniPolicy,
    required_clear_kind: RequiredClearKind,
    required_score_profile_id: Option<ScoreProfileId>,
    target_probability_threshold: Option<ProbabilityValue>,
}

impl SpinTargetRequest {
    pub fn new(id: SpinTargetId, spin_kind: RequiredSpinKind) -> Self {
        Self {
            id,
            spin_piece_selector: SpinPieceSelector::AnyPiece,
            spin_kind,
            clear_lines: RequiredClearLines::Any,
            mini_policy: SpinMiniPolicy::MiniAllowed,
            required_clear_kind: RequiredClearKind::Any,
            required_score_profile_id: None,
            target_probability_threshold: None,
        }
    }
}
impl SpinTargetRequest {
    pub fn tsd(id: impl Into<String>) -> Self {
        Self::new(SpinTargetId::new(id), RequiredSpinKind::TSpin)
            .with_piece_selector(SpinPieceSelector::TOnly)
            .with_clear_lines(RequiredClearLines::Exactly(2))
            .with_mini_policy(SpinMiniPolicy::RegularOnly)
    }
}
impl SpinTargetRequest {
    pub fn with_piece_selector(mut self, selector: SpinPieceSelector) -> Self {
        self.spin_piece_selector = selector;
        self
    }
}
impl SpinTargetRequest {
    pub fn with_clear_lines(mut self, clear_lines: RequiredClearLines) -> Self {
        self.clear_lines = clear_lines;
        self
    }
}
impl SpinTargetRequest {
    pub fn with_mini_policy(mut self, mini_policy: SpinMiniPolicy) -> Self {
        self.mini_policy = mini_policy;
        self
    }
}
impl SpinTargetRequest {
    pub fn with_required_score_profile(mut self, profile_id: impl Into<String>) -> Self {
        self.required_score_profile_id = Some(ScoreProfileId::new(profile_id));
        self
    }
}
impl SpinTargetRequest {
    pub fn with_target_probability_threshold(mut self, threshold: ProbabilityValue) -> Self {
        self.target_probability_threshold = Some(threshold);
        self
    }
}
impl SpinTargetRequest {
    pub fn id(&self) -> &SpinTargetId {
        &self.id
    }
}
impl SpinTargetRequest {
    pub fn spin_piece_selector(&self) -> &SpinPieceSelector {
        &self.spin_piece_selector
    }
}
impl SpinTargetRequest {
    pub fn spin_kind(&self) -> RequiredSpinKind {
        self.spin_kind
    }
}
impl SpinTargetRequest {
    pub fn clear_lines(&self) -> RequiredClearLines {
        self.clear_lines
    }
}
impl SpinTargetRequest {
    pub fn mini_policy(&self) -> SpinMiniPolicy {
        self.mini_policy
    }
}
impl SpinTargetRequest {
    pub fn required_clear_kind(&self) -> RequiredClearKind {
        self.required_clear_kind
    }
}
impl SpinTargetRequest {
    pub fn required_score_profile_id(&self) -> Option<&ScoreProfileId> {
        self.required_score_profile_id.as_ref()
    }
}
impl SpinTargetRequest {
    pub fn required_score_profile(&self) -> Option<&str> {
        self.required_score_profile_id
            .as_ref()
            .map(ScoreProfileId::as_str)
    }
}
impl SpinTargetRequest {
    pub fn target_probability_threshold(&self) -> Option<ProbabilityValue> {
        self.target_probability_threshold
    }
}

pub fn spin_target_requires_score_profile(target: &SpinTargetRequest) -> bool {
    matches!(target.spin_kind(), RequiredSpinKind::ProfileSpecific(_))
}
