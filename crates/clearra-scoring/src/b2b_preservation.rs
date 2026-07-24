use clearra_replay::ScoringExecutionEdge;

use crate::{event::SpinDetector, profile::SpinProfile};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackToBackPreservationPolicy {
    spin_profile: SpinProfile,
}

impl BackToBackPreservationPolicy {
    pub const fn new(spin_profile: SpinProfile) -> Self {
        Self { spin_profile }
    }

    pub const fn requires_recognized_spin(edge: ScoringExecutionEdge) -> bool {
        edge.cleared_lines() > 0 && edge.cleared_lines() != 4 && !edge.perfect_clear()
    }

    pub fn allows(self, edge: ScoringExecutionEdge) -> bool {
        !Self::requires_recognized_spin(edge)
            || SpinDetector::detect_scoring_edge_with_profile(edge, self.spin_profile).is_some()
    }
}
