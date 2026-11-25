use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::{
    domain::{slot_constraint::SlotConstraint, slot_domain::SlotDomain},
    template::build_slot::BuildSlotId,
};

use super::*;

#[test]
fn default_assignment_enumeration_has_no_implicit_development_budget() {
    let domains: Vec<_> = (0..11)
        .map(|slot| SlotDomain::new(BuildSlotId::new(slot), vec![PieceKind::I, PieceKind::O]))
        .collect();
    let complete =
        AssignmentCsp::new(domains.clone(), Vec::new(), AssignmentCspLimits::default()).solve();
    assert_eq!(complete.len(), 2048);
    let bounded = AssignmentCsp::new(domains, Vec::new(), AssignmentCspLimits::new(1024)).solve();
    assert_eq!(bounded.len(), 1024);
    assert_eq!(bounded, complete[..1024]);
}

#[test]
fn csp_enumerates_assignments_with_constraints() {
    let slot_a = BuildSlotId::new(1);
    let slot_b = BuildSlotId::new(2);
    let csp = AssignmentCsp::new(
        vec![
            SlotDomain::new(slot_a, vec![PieceKind::I, PieceKind::O]),
            SlotDomain::new(slot_b, vec![PieceKind::T, PieceKind::S]),
        ],
        vec![SlotConstraint::required(slot_a, PieceKind::I)],
        AssignmentCspLimits::default(),
    );

    let assignments = csp.solve();

    assert_eq!(assignments.len(), 2);
    assert!(assignments.iter().all(|assignment| {
        assignment
            .assigned_slots()
            .iter()
            .any(|slot| slot.slot_id() == slot_a && slot.piece() == PieceKind::I)
    }));
}

#[test]
fn assignment_csp_works() {
    let slot = BuildSlotId::new(1);
    let csp = AssignmentCsp::new(
        vec![SlotDomain::new(slot, vec![PieceKind::I, PieceKind::O])],
        vec![SlotConstraint::required(slot, PieceKind::O)],
        AssignmentCspLimits::new(8),
    );

    let assignments = csp.solve();

    assert_eq!(assignments.len(), 1);
    assert_eq!(assignments[0].assigned_slots()[0].piece(), PieceKind::O);
}
