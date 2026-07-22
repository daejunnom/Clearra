use clearra_build_coverage::{
    domain::{slot_constraint::SlotConstraint, slot_domain::SlotDomain},
    query::{build_coverage_limits::BuildCoverageLimits, build_coverage_query::BuildCoverageQuery},
    template::{
        build_slot::{BuildSlot, BuildSlotId, SlotOrderConstraint},
        build_template::BuildTemplate,
    },
};
use clearra_core_domain::{
    board::{board_size::BoardSize, cell::CellCoord},
    piece::piece_kind::PieceKind,
};

use super::*;

fn base_template() -> BuildTemplate {
    BuildTemplate::new(
        "test-template",
        vec![BuildSlot::new(
            BuildSlotId::new(1),
            vec![CellCoord::new_unchecked(0, 0)],
        )],
    )
    .with_board_size(BoardSize::standard_10x20())
}

fn base_query() -> BuildCoverageQuery {
    BuildCoverageQuery::new(
        base_template(),
        vec![SlotDomain::new(
            BuildSlotId::new(1),
            vec![PieceKind::I, PieceKind::O],
        )],
        vec![],
        16,
        BuildCoverageLimits::default(),
    )
}

#[test]
fn valid_build_query_is_supported() {
    let report = validate_build_coverage_query(&base_query());

    assert!(!report.has_errors());
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.code() == DiagnosticCode::IBuildQueryMvpSupported));
}

#[test]
fn missing_slot_domain_is_rejected() {
    let query = BuildCoverageQuery::new(
        base_template(),
        vec![],
        vec![],
        16,
        BuildCoverageLimits::default(),
    );

    let report = validate_build_coverage_query(&query);

    assert!(report.has_errors());
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.evidence().iter().any(|evidence| {
            evidence.key() == "reason" && evidence.value() == "missing_slot_domain"
        })));
}

#[test]
fn required_piece_outside_domain_is_rejected() {
    let query = BuildCoverageQuery::new(
        base_template(),
        vec![SlotDomain::new(BuildSlotId::new(1), vec![PieceKind::I])],
        vec![SlotConstraint::required(BuildSlotId::new(1), PieceKind::O)],
        16,
        BuildCoverageLimits::default(),
    );

    let report = validate_build_coverage_query(&query);

    assert!(report.has_errors());
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.evidence().iter().any(|evidence| {
            evidence.key() == "reason" && evidence.value() == "required_piece_not_in_domain"
        })));
}

#[test]
fn invalid_slot_geometry_and_overlapping_slots_are_rejected() {
    let template = BuildTemplate::new(
        "invalid-geometry",
        vec![
            BuildSlot::new(
                BuildSlotId::new(1),
                vec![
                    CellCoord::new_unchecked(0, 0),
                    CellCoord::new_unchecked(0, 0),
                    CellCoord::new_unchecked(10, 0),
                ],
            ),
            BuildSlot::new(BuildSlotId::new(2), vec![CellCoord::new_unchecked(0, 0)]),
        ],
    )
    .with_board_size(BoardSize::standard_10x20());
    let query = BuildCoverageQuery::new(
        template,
        vec![
            SlotDomain::new(BuildSlotId::new(1), vec![PieceKind::I]),
            SlotDomain::new(BuildSlotId::new(2), vec![PieceKind::O]),
        ],
        vec![],
        16,
        BuildCoverageLimits::default(),
    );

    let report = validate_build_coverage_query(&query);

    for reason in [
        "duplicate_slot_cell",
        "slot_cell_out_of_bounds",
        "overlapping_slot_cells",
    ] {
        assert!(
            report.diagnostics().iter().any(|diagnostic| {
                diagnostic
                    .evidence()
                    .iter()
                    .any(|evidence| evidence.key() == "reason" && evidence.value() == reason)
            }),
            "missing reason {reason}"
        );
    }
}

#[test]
fn template_slot_empty_domain_and_external_domain_piece_are_rejected() {
    let template = BuildTemplate::new(
        "invalid-domain",
        vec![
            BuildSlot::new(BuildSlotId::new(1), vec![CellCoord::new_unchecked(0, 0)])
                .with_allowed_pieces(vec![]),
            BuildSlot::new(BuildSlotId::new(2), vec![CellCoord::new_unchecked(1, 0)])
                .with_allowed_pieces(vec![PieceKind::I]),
        ],
    )
    .with_board_size(BoardSize::standard_10x20());
    let query = BuildCoverageQuery::new(
        template,
        vec![
            SlotDomain::new(BuildSlotId::new(1), vec![]),
            SlotDomain::new(BuildSlotId::new(2), vec![PieceKind::O]),
        ],
        vec![],
        16,
        BuildCoverageLimits::default(),
    );

    let report = validate_build_coverage_query(&query);

    for reason in [
        "empty_template_slot_domain",
        "empty_slot_domain",
        "domain_piece_not_allowed_by_template",
    ] {
        assert!(
            report.diagnostics().iter().any(|diagnostic| {
                diagnostic
                    .evidence()
                    .iter()
                    .any(|evidence| evidence.key() == "reason" && evidence.value() == reason)
            }),
            "missing reason {reason}"
        );
    }
}

#[test]
fn impossible_assignment_is_rejected_before_build_coverage_runs() {
    let template = BuildTemplate::new(
        "impossible-assignment",
        vec![
            BuildSlot::new(BuildSlotId::new(1), vec![CellCoord::new_unchecked(0, 0)])
                .with_allowed_pieces(vec![PieceKind::I, PieceKind::O])
                .with_required_piece(PieceKind::I),
        ],
    )
    .with_board_size(BoardSize::standard_10x20());
    let query = BuildCoverageQuery::new(
        template,
        vec![SlotDomain::new(
            BuildSlotId::new(1),
            vec![PieceKind::I, PieceKind::O],
        )],
        vec![SlotConstraint::required(BuildSlotId::new(1), PieceKind::O)],
        16,
        BuildCoverageLimits::default(),
    );

    let report = validate_build_coverage_query(&query);

    assert!(report.diagnostics().iter().any(|diagnostic| {
        diagnostic.evidence().iter().any(|evidence| {
            evidence.key() == "reason" && evidence.value() == "impossible_assignment"
        })
    }));
}

#[test]
fn slot_order_constraint_must_reference_another_template_slot() {
    let template = BuildTemplate::new(
        "invalid-order",
        vec![
            BuildSlot::new(BuildSlotId::new(1), vec![CellCoord::new_unchecked(0, 0)])
                .with_order_constraint(SlotOrderConstraint::Before(BuildSlotId::new(1))),
            BuildSlot::new(BuildSlotId::new(2), vec![CellCoord::new_unchecked(1, 0)])
                .with_order_constraint(SlotOrderConstraint::After(BuildSlotId::new(99))),
        ],
    )
    .with_board_size(BoardSize::standard_10x20());
    let query = BuildCoverageQuery::new(
        template,
        vec![
            SlotDomain::new(BuildSlotId::new(1), vec![PieceKind::I]),
            SlotDomain::new(BuildSlotId::new(2), vec![PieceKind::O]),
        ],
        vec![],
        16,
        BuildCoverageLimits::default(),
    );

    let report = validate_build_coverage_query(&query);

    for reason in [
        "self_referential_slot_order",
        "unknown_slot_order_reference",
    ] {
        assert!(
            report.diagnostics().iter().any(|diagnostic| {
                diagnostic
                    .evidence()
                    .iter()
                    .any(|evidence| evidence.key() == "reason" && evidence.value() == reason)
            }),
            "missing reason {reason}"
        );
    }
}
