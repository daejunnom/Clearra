use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_scoring::profile::SpinProfileId;

use crate::{
    query::{ForwardSearchMode, ForwardSpinCategory},
    reachability::LockEvidence,
    search::StateKey,
};

#[derive(Clone, Copy)]
pub(crate) struct TSpinAcceleration {
    profile: SpinProfileId,
}

impl TSpinAcceleration {
    pub(crate) const fn for_search(
        mode: ForwardSearchMode,
        profile: SpinProfileId,
    ) -> Option<Self> {
        let ForwardSearchMode::SpinFinder(target) = mode else {
            return None;
        };
        if matches!(target.category(), ForwardSpinCategory::Other) {
            return None;
        }
        if matches!(profile, SpinProfileId::TSpins | SpinProfileId::TSpinsPlus) {
            Some(Self { profile })
        } else {
            None
        }
    }

    pub(crate) fn state_can_reach_target(self, queue: &[PieceKind], state: StateKey) -> bool {
        self.supply_can_reach_target(queue, state.active, state.cursor, state.hold)
    }

    pub(crate) fn supply_can_reach_target(
        self,
        queue: &[PieceKind],
        active: PieceKind,
        cursor: u16,
        hold: Option<PieceKind>,
    ) -> bool {
        active == PieceKind::T
            || hold == Some(PieceKind::T)
            || queue[usize::from(cursor).min(queue.len())..].contains(&PieceKind::T)
    }

    pub(crate) fn needs_corner_counts(self, piece: PieceKind, evidence: LockEvidence) -> bool {
        piece == PieceKind::T && evidence.last_action_was_rotation()
    }

    pub(crate) fn needs_exact_confirmation(
        self,
        piece: PieceKind,
        evidence: LockEvidence,
        immobile: bool,
        blocked_corners: u8,
    ) -> bool {
        if piece != PieceKind::T || !evidence.last_action_was_rotation() {
            return false;
        }
        blocked_corners >= 3 || (matches!(self.profile, SpinProfileId::TSpinsPlus) && immobile)
    }
}
