use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::{
    assignment::slot_assignment::{AssignedSlot, SlotAssignment},
    domain::{slot_constraint::SlotConstraint, slot_domain::SlotDomain},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssignmentCspLimits {
    max_assignments: usize,
}

impl AssignmentCspLimits {
    pub fn new(max_assignments: usize) -> Self {
        Self { max_assignments }
    }
}
impl AssignmentCspLimits {
    pub fn max_assignments(self) -> usize {
        self.max_assignments
    }
}

impl Default for AssignmentCspLimits {
    fn default() -> Self {
        Self {
            max_assignments: 1024,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignmentCsp {
    domains: Vec<SlotDomain>,
    constraints: Vec<SlotConstraint>,
    limits: AssignmentCspLimits,
}

impl AssignmentCsp {
    pub fn new(
        domains: Vec<SlotDomain>,
        constraints: Vec<SlotConstraint>,
        limits: AssignmentCspLimits,
    ) -> Self {
        Self {
            domains,
            constraints,
            limits,
        }
    }
}
impl AssignmentCsp {
    pub fn solve(&self) -> Vec<SlotAssignment> {
        let mut output = Vec::new();
        let mut current = Vec::new();
        self.solve_at(0, &mut current, &mut output);
        output
    }
}
impl AssignmentCsp {
    fn solve_at(
        &self,
        domain_index: usize,
        current: &mut Vec<AssignedSlot>,
        output: &mut Vec<SlotAssignment>,
    ) {
        if output.len() >= self.limits.max_assignments() {
            return;
        }

        if domain_index == self.domains.len() {
            output.push(SlotAssignment::new(current.clone()));
            return;
        }

        let domain = &self.domains[domain_index];
        for piece in domain.pieces().iter().copied() {
            if self.allows(domain.slot_id(), piece) {
                current.push(AssignedSlot::new(domain.slot_id(), piece));
                self.solve_at(domain_index + 1, current, output);
                current.pop();
            }
        }
    }
}
impl AssignmentCsp {
    fn allows(&self, slot_id: crate::template::build_slot::BuildSlotId, piece: PieceKind) -> bool {
        self.constraints
            .iter()
            .filter(|constraint| constraint.slot_id() == slot_id)
            .all(|constraint| constraint.allows(piece))
    }
}

#[cfg(test)]
#[path = "assignment_csp_tests.rs"]
mod tests;
