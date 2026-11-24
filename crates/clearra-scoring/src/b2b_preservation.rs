use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_replay::ScoringExecutionEdge;

use crate::{
    event::{clear_event::ClearEvent, SpinDetector},
    profile::{NonTSpinRecognition, SpinProfile, TSpinRecognition},
};

/// A B2B-save decision using only the piece and the exact lock/clear facts.
///
/// This is not a reachability or spin proof. A consumer may discard a
/// `Rejects` transition before searching movement evidence, but `Preserves`
/// does not discharge physical, trace, score, or other target requirements.
/// `RequiresSpinEvidence` must retain every potentially preserving execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BeforeSpinEvidenceDecision {
    Preserves,
    Rejects,
    RequiresSpinEvidence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackToBackPreservationPolicy {
    spin_profile: SpinProfile,
}

impl BackToBackPreservationPolicy {
    pub const fn new(spin_profile: SpinProfile) -> Self {
        Self { spin_profile }
    }

    pub const fn requires_recognized_spin(edge: ScoringExecutionEdge) -> bool {
        clear_requires_recognized_spin(edge.cleared_lines(), edge.perfect_clear())
    }

    /// Classifies a request-local B2B-save transition without constructing or
    /// searching a movement trace. `perfect_clear` must describe this lock's
    /// actual post-clear board, not the eventual goal board.
    ///
    /// B2B-save is the existing edge predicate: no clear, four lines, any
    /// perfect clear, or a spin recognized by this policy. It is not the
    /// numeric scoring chain transition. Recognition, rather than score-award
    /// mapping, is deliberately the same authority used by `allows`.
    pub const fn before_spin_evidence(
        self,
        piece: PieceKind,
        cleared_lines: u8,
        perfect_clear: bool,
    ) -> BeforeSpinEvidenceDecision {
        if !clear_requires_recognized_spin(cleared_lines, perfect_clear) {
            return BeforeSpinEvidenceDecision::Preserves;
        }
        if matches!(piece, PieceKind::T) {
            match self.spin_profile.t_spin_recognition() {
                TSpinRecognition::Disabled => BeforeSpinEvidenceDecision::Rejects,
                // The existing simple recognizer intentionally recognizes
                // every T lock, independently of movement evidence.
                TSpinRecognition::Simple => BeforeSpinEvidenceDecision::Preserves,
                TSpinRecognition::ThreeCorner | TSpinRecognition::ThreeCornerOrImmobileMini => {
                    BeforeSpinEvidenceDecision::RequiresSpinEvidence
                }
            }
        } else {
            match self.spin_profile.non_t_spin_recognition() {
                NonTSpinRecognition::Disabled => BeforeSpinEvidenceDecision::Rejects,
                NonTSpinRecognition::ImmobileRegular | NonTSpinRecognition::ImmobileMini => {
                    BeforeSpinEvidenceDecision::RequiresSpinEvidence
                }
            }
        }
    }

    pub fn allows(self, edge: ScoringExecutionEdge) -> bool {
        match self.before_spin_evidence(edge.piece(), edge.cleared_lines(), edge.perfect_clear()) {
            BeforeSpinEvidenceDecision::Preserves => true,
            BeforeSpinEvidenceDecision::Rejects => false,
            BeforeSpinEvidenceDecision::RequiresSpinEvidence => {
                SpinDetector::detect_scoring_edge_with_profile(edge, self.spin_profile).is_some()
            }
        }
    }
}

const fn clear_requires_recognized_spin(cleared_lines: u8, perfect_clear: bool) -> bool {
    let clear = ClearEvent::new(cleared_lines, perfect_clear);
    clear.clears_lines() && !clear.is_four_line_clear() && !clear.is_perfect_clear()
}

#[cfg(test)]
#[path = "b2b_preservation_tests.rs"]
mod tests;
