use std::collections::BTreeMap;

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_exact_cover::{
    model::{ExactCoverCandidate, ExactCoverProblem},
    solver::{DlxSearchLimits, DlxSolveReport, DlxSolver, DlxSolverError},
};

use crate::{
    assignment::slot_assignment::{AssignedSlot, SlotAssignment},
    domain::{slot_constraint::SlotConstraint, slot_domain::SlotDomain},
    template::build_slot::BuildSlotId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignmentExactCoverBridge {
    domains: Vec<SlotDomain>,
    constraints: Vec<SlotConstraint>,
}

impl AssignmentExactCoverBridge {
    pub fn new(domains: Vec<SlotDomain>, constraints: Vec<SlotConstraint>) -> Self {
        Self {
            domains,
            constraints,
        }
    }
}
impl AssignmentExactCoverBridge {
    pub fn problem(&self) -> ExactCoverProblem {
        let candidates = self
            .candidate_entries()
            .into_iter()
            .enumerate()
            .map(|(candidate_id, (slot_index, _, _))| {
                ExactCoverCandidate::new(candidate_id, vec![slot_index])
            })
            .collect();
        ExactCoverProblem::new(self.domains.len(), candidates)
    }
}
impl AssignmentExactCoverBridge {
    pub fn solve(
        &self,
        limits: DlxSearchLimits,
    ) -> Result<AssignmentExactCoverResult, AssignmentExactCoverError> {
        let candidate_entries = self.candidate_entries();
        let lookup = candidate_entries
            .iter()
            .enumerate()
            .map(|(candidate_id, (_, slot_id, piece))| (candidate_id, (*slot_id, *piece)))
            .collect::<BTreeMap<_, _>>();
        let report = DlxSolver::solve_all_limited(&self.problem(), limits)
            .map_err(AssignmentExactCoverError::Dlx)?;
        self.assignments_from_report(&report, &lookup)
    }
}
impl AssignmentExactCoverBridge {
    fn candidate_entries(&self) -> Vec<(usize, BuildSlotId, PieceKind)> {
        self.domains
            .iter()
            .enumerate()
            .flat_map(|(slot_index, domain)| {
                domain
                    .pieces()
                    .iter()
                    .copied()
                    .filter(move |piece| self.allows(domain.slot_id(), *piece))
                    .map(move |piece| (slot_index, domain.slot_id(), piece))
            })
            .collect()
    }
}
impl AssignmentExactCoverBridge {
    fn allows(&self, slot_id: BuildSlotId, piece: PieceKind) -> bool {
        self.constraints
            .iter()
            .filter(|constraint| constraint.slot_id() == slot_id)
            .all(|constraint| constraint.allows(piece))
    }
}
impl AssignmentExactCoverBridge {
    fn assignments_from_report(
        &self,
        report: &DlxSolveReport,
        lookup: &BTreeMap<usize, (BuildSlotId, PieceKind)>,
    ) -> Result<AssignmentExactCoverResult, AssignmentExactCoverError> {
        let mut assignments = Vec::new();
        for solution in report.solutions() {
            let mut slots = Vec::new();
            for candidate_id in solution.candidate_ids() {
                let Some((slot_id, piece)) = lookup.get(candidate_id) else {
                    return Err(AssignmentExactCoverError::UnknownCandidateId {
                        candidate_id: *candidate_id,
                    });
                };
                slots.push(AssignedSlot::new(*slot_id, *piece));
            }
            assignments.push(SlotAssignment::new(slots));
        }

        Ok(AssignmentExactCoverResult {
            assignments,
            complete: report.complete(),
            searched_nodes: report.searched_nodes(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignmentExactCoverResult {
    assignments: Vec<SlotAssignment>,
    complete: bool,
    searched_nodes: usize,
}

impl AssignmentExactCoverResult {
    pub fn assignments(&self) -> &[SlotAssignment] {
        &self.assignments
    }
}
impl AssignmentExactCoverResult {
    pub fn complete(&self) -> bool {
        self.complete
    }
}
impl AssignmentExactCoverResult {
    pub fn searched_nodes(&self) -> usize {
        self.searched_nodes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssignmentExactCoverError {
    Dlx(DlxSolverError),
    UnknownCandidateId { candidate_id: usize },
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_exact_cover::solver::DlxSearchLimits;

    use crate::{
        domain::{slot_constraint::SlotConstraint, slot_domain::SlotDomain},
        template::build_slot::BuildSlotId,
    };

    use super::*;

    #[test]
    fn assignment_exact_cover_works() {
        let slot = BuildSlotId::new(1);
        let bridge = AssignmentExactCoverBridge::new(
            vec![SlotDomain::new(slot, vec![PieceKind::I, PieceKind::O])],
            vec![SlotConstraint::required(slot, PieceKind::I)],
        );

        let result = bridge
            .solve(DlxSearchLimits::new(8, 8))
            .expect("exact cover");

        assert!(result.complete());
        assert_eq!(result.assignments().len(), 1);
        assert_eq!(
            result.assignments()[0].assigned_slots()[0].piece(),
            PieceKind::I
        );
    }
}
