use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_replay::TraceCompleteness;

use crate::{evidence::KickEvidence, special::SpecialSpinCaseId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LastActionEvidence {
    pub piece: PieceKind,
    pub rotation_used: bool,
}

impl LastActionEvidence {
    pub const fn new(piece: PieceKind, rotation_used: bool) -> Self {
        Self {
            piece,
            rotation_used,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CornerEvidence {
    pub blocked_corners: u8,
}

impl CornerEvidence {
    pub const fn new(blocked_corners: u8) -> Self {
        Self { blocked_corners }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImmobileEvidence {
    pub immobile: bool,
}

impl ImmobileEvidence {
    pub const fn new(immobile: bool) -> Self {
        Self { immobile }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecialSpinEvidence {
    pub case_id: SpecialSpinCaseId,
    pub exact: bool,
}

impl SpecialSpinEvidence {
    pub fn new(case_id: SpecialSpinCaseId, exact: bool) -> Self {
        Self { case_id, exact }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinEvidence {
    pub last_action: LastActionEvidence,
    pub corner: Option<CornerEvidence>,
    pub kick: Option<KickEvidence>,
    pub immobile: Option<ImmobileEvidence>,
    pub special: Vec<SpecialSpinEvidence>,
    pub trace_completeness: TraceCompleteness,
}

impl SpinEvidence {
    pub fn new(last_action: LastActionEvidence) -> Self {
        Self {
            last_action,
            corner: None,
            kick: None,
            immobile: None,
            special: Vec::new(),
            trace_completeness: TraceCompleteness::Complete,
        }
    }
}
impl SpinEvidence {
    pub fn with_corner(mut self, corner: CornerEvidence) -> Self {
        self.corner = Some(corner);
        self
    }
}
impl SpinEvidence {
    pub fn with_kick(mut self, kick: KickEvidence) -> Self {
        self.kick = Some(kick);
        self
    }
}
impl SpinEvidence {
    pub fn with_special(mut self, special: SpecialSpinEvidence) -> Self {
        self.special.push(special);
        self
    }
}
impl SpinEvidence {
    pub fn exact_special_evidence_for(&self, case_id: SpecialSpinCaseId) -> bool {
        self.special
            .iter()
            .any(|special| special.case_id == case_id && special.exact)
    }
}
