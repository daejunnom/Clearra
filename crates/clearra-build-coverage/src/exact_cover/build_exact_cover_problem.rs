use clearra_exact_cover::{
    model::ExactCoverProblem,
    solver::{DlxSearchLimits, DlxSolveReport, DlxSolver, DlxSolverError},
};

use crate::assignment::{assignment_exact_cover::AssignmentExactCoverBridge, SlotAssignment};

pub struct BuildExactCoverProblemBridge;

impl BuildExactCoverProblemBridge {
    pub fn problem_from_assignment_bridge(
        bridge: &AssignmentExactCoverBridge,
    ) -> ExactCoverProblem {
        bridge.problem()
    }
}
impl BuildExactCoverProblemBridge {
    pub fn solve_problem(
        problem: &ExactCoverProblem,
        limits: DlxSearchLimits,
    ) -> Result<DlxSolveReport, BuildExactCoverProblemError> {
        DlxSolver::solve_all_limited(problem, limits).map_err(BuildExactCoverProblemError::Dlx)
    }
}
impl BuildExactCoverProblemBridge {
    pub fn report_from_assignments(
        assignments: Vec<SlotAssignment>,
        report: &DlxSolveReport,
    ) -> BuildExactCoverProblemReport {
        BuildExactCoverProblemReport {
            assignments,
            complete: report.complete(),
            searched_nodes: report.searched_nodes(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildExactCoverProblemReport {
    assignments: Vec<SlotAssignment>,
    complete: bool,
    searched_nodes: usize,
}

impl BuildExactCoverProblemReport {
    pub fn assignments(&self) -> &[SlotAssignment] {
        &self.assignments
    }
}
impl BuildExactCoverProblemReport {
    pub fn complete(&self) -> bool {
        self.complete
    }
}
impl BuildExactCoverProblemReport {
    pub fn searched_nodes(&self) -> usize {
        self.searched_nodes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildExactCoverProblemError {
    Dlx(DlxSolverError),
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_exact_cover::solver::DlxSearchLimits;

    use crate::{
        assignment::assignment_exact_cover::AssignmentExactCoverBridge,
        domain::{slot_constraint::SlotConstraint, slot_domain::SlotDomain},
        template::build_slot::BuildSlotId,
    };

    use super::*;

    #[test]
    fn build_coverage_exact_cover_uses_shared_problem_schema() {
        let slot = BuildSlotId::new(1);
        let bridge = AssignmentExactCoverBridge::new(
            vec![SlotDomain::new(slot, vec![PieceKind::I])],
            vec![SlotConstraint::required(slot, PieceKind::I)],
        );

        let problem = BuildExactCoverProblemBridge::problem_from_assignment_bridge(&bridge);
        let report =
            BuildExactCoverProblemBridge::solve_problem(&problem, DlxSearchLimits::new(4, 32))
                .expect("dlx report");

        assert!(report.complete());
        assert_eq!(report.solution_count(), 1);
    }
}
