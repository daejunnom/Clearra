use clearra_geometry::area::{AreaFeasibilityDecision, AreaMultisetFeasibility};

use crate::builder::piece_area_constraint::PieceAreaConstraint;

pub struct AreaMultisetExactCoverBridge;

impl AreaMultisetExactCoverBridge {
    pub fn check_before_exact_cover(
        component_area: usize,
        feasibility: &AreaMultisetFeasibility,
    ) -> AreaMultisetExactCoverDecision {
        match feasibility.check_component_area(component_area) {
            AreaFeasibilityDecision::RejectAreaInfeasible { component_area } => {
                AreaMultisetExactCoverDecision::RejectBeforeExactCover { component_area }
            }
            AreaFeasibilityDecision::SearchMayContinue => {
                AreaMultisetExactCoverDecision::NecessaryConditionPassed
            }
        }
    }
}
impl AreaMultisetExactCoverBridge {
    pub fn to_piece_area_constraint(
        component_area: usize,
        feasibility: &AreaMultisetFeasibility,
    ) -> Result<PieceAreaConstraint, crate::builder::piece_area_constraint::PieceAreaConstraintError>
    {
        PieceAreaConstraint::new(
            component_area,
            feasibility.active_piece_area_multiset().iter().copied(),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AreaMultisetExactCoverDecision {
    RejectBeforeExactCover { component_area: usize },
    NecessaryConditionPassed,
}

impl AreaMultisetExactCoverDecision {
    pub fn is_solution_found(self) -> bool {
        false
    }
}

#[cfg(test)]
#[path = "area_multiset_bridge_tests.rs"]
mod tests;
