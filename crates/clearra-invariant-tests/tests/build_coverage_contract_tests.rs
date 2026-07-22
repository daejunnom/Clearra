use clearra_build_coverage::{
    assignment::{
        assignment_csp::{AssignmentCsp, AssignmentCspLimits},
        slot_assignment::{AssignedSlot, SlotAssignment},
    },
    coverage::{
        build_coverage_matrix::{BuildCoverageMatrix, BuildCoverageMatrixError},
        build_coverage_result::BuildCoverageResult,
        build_union_coverage::BuildUnionCoverage,
    },
    domain::{slot_constraint::SlotConstraint, slot_domain::SlotDomain},
    query::build_coverage_limits::BuildCoverageLimits,
    template::{
        build_slot::{
            BuildSlot, BuildSlotId, SlotCanonicalization, SlotHoldConstraint, SlotOrderConstraint,
            SlotSymmetry,
        },
        build_template::{BuildTemplate, TemplateCanonicalization, TemplateSymmetry},
        template_import::{
            TemplateExport, TemplateExportFormat, TemplateImport, TemplateImportFormat,
            TemplateJsonError,
        },
    },
};
use clearra_core_domain::{
    board::{board_size::BoardSize, cell::CellCoord},
    piece::piece_kind::PieceKind,
};
use clearra_coverage::{
    pattern::{
        pattern_bitset::PatternBitSet, pattern_id::PatternId,
        weighted_pattern_set::WeightedPatternSet,
    },
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};
use clearra_profiles::search::search_defaults::SearchDefaults;

mod case_build_assignment_csp_respects_slot_constraints {
    use super::*;

    #[test]
    fn build_assignment_csp_respects_slot_constraints() {
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
}

mod case_build_coverage_rejects_assignment_coverage_length_mismatch {
    use super::*;

    #[test]
    fn build_coverage_rejects_assignment_coverage_length_mismatch() {
        let slot = BuildSlotId::new(1);
        let assignments = vec![
            SlotAssignment::new(vec![AssignedSlot::new(slot, PieceKind::I)]),
            SlotAssignment::new(vec![AssignedSlot::new(slot, PieceKind::O)]),
            SlotAssignment::new(vec![AssignedSlot::new(slot, PieceKind::T)]),
        ];
        let coverages = vec![
            PatternBitSet::from_patterns(2, [PatternId::new(0)]).expect("coverage 0"),
            PatternBitSet::from_patterns(2, [PatternId::new(1)]).expect("coverage 1"),
        ];

        let result = BuildCoverageMatrix::from_assignments_with_coverages(
            11,
            PatternUniverseId::new(1),
            PatternWeightModelId::new(7),
            2,
            &assignments,
            &coverages,
        );

        assert_eq!(
            result,
            Err(BuildCoverageMatrixError::AssignmentCoverageLengthMismatch {
                assignments: 3,
                coverages: 2
            })
        );
    }
}

mod case_build_coverage_probability_uses_union_not_assignment_sum {
    use super::*;

    #[test]
    fn build_coverage_probability_uses_union_not_assignment_sum() {
        let matrix = BuildCoverageMatrix::from_assignment_coverages(
            11,
            PatternUniverseId::new(1),
            PatternWeightModelId::new(7),
            2,
            vec![
                (
                    0,
                    PatternBitSet::from_patterns(2, [PatternId::new(0), PatternId::new(1)])
                        .expect("coverage 0"),
                ),
                (
                    1,
                    PatternBitSet::from_patterns(2, [PatternId::new(0), PatternId::new(1)])
                        .expect("coverage 1"),
                ),
            ],
        )
        .expect("matrix");
        let union = BuildUnionCoverage::from_matrix(matrix.matrix());
        let weights = WeightedPatternSet::uniform(2).expect("weights");

        let result = BuildCoverageResult::from_union(union, &weights).expect("result");

        assert_eq!(result.probability().get(), 1.0);
    }
}

mod case_build_coverage_limits_come_from_profile_defaults {
    use super::*;

    #[test]
    fn build_coverage_limits_come_from_profile_defaults() {
        let limits = BuildCoverageLimits::from(SearchDefaults::MVP1);

        assert_eq!(limits.max_assignments(), 1024);
        assert_eq!(limits.max_patterns(), 4096);
    }
}

mod case_build_template_mvp2_contract_carries_editor_geometry_domain_and_canonicalization {
    use super::*;

    #[test]
    fn build_template_mvp2_contract_carries_editor_geometry_domain_and_canonicalization() {
        let slot = BuildSlot::new(
            BuildSlotId::new(1),
            vec![
                CellCoord::new_unchecked(0, 0),
                CellCoord::new_unchecked(1, 0),
            ],
        )
        .with_label("left slot")
        .with_allowed_pieces(vec![PieceKind::I, PieceKind::O])
        .with_required_piece(PieceKind::I)
        .with_hold_constraint(SlotHoldConstraint::RequiresHold)
        .with_order_constraint(SlotOrderConstraint::Before(BuildSlotId::new(2)))
        .with_symmetry(SlotSymmetry::MirrorX)
        .with_canonicalization(SlotCanonicalization::CanonicalBySymmetry);
        let template = BuildTemplate::new("editor-template", vec![slot])
            .with_label("Editor template")
            .with_board_size(BoardSize::new(10, 4).expect("board"))
            .with_symmetry(TemplateSymmetry::MirrorX)
            .with_canonicalization(TemplateCanonicalization::CanonicalByGeometry);

        assert_eq!(template.label(), Some("Editor template"));
        assert_eq!(template.board_size().height(), 4);
        assert_eq!(template.symmetry(), TemplateSymmetry::MirrorX);
        assert_eq!(
            template.canonicalization(),
            TemplateCanonicalization::CanonicalByGeometry
        );
        let slot = template.slot(BuildSlotId::new(1)).expect("slot");
        assert_eq!(slot.label(), Some("left slot"));
        assert_eq!(slot.allowed_pieces(), &[PieceKind::I, PieceKind::O]);
        assert_eq!(slot.required_piece(), Some(PieceKind::I));
        assert_eq!(slot.hold_constraint(), SlotHoldConstraint::RequiresHold);
        assert_eq!(
            slot.order_constraint().referenced_slot(),
            Some(BuildSlotId::new(2))
        );
        assert_eq!(slot.symmetry(), SlotSymmetry::MirrorX);
        assert_eq!(
            slot.canonicalization(),
            SlotCanonicalization::CanonicalBySymmetry
        );
    }
}

mod case_build_template_import_export_accepts_interpreted_template_not_raw_format_text {
    use super::*;

    #[test]
    fn build_template_import_export_accepts_interpreted_template_not_raw_format_text() {
        let template = BuildTemplate::new(
            "adapter-template",
            vec![BuildSlot::new(
                BuildSlotId::new(1),
                vec![CellCoord::new_unchecked(0, 0)],
            )],
        );
        let import = TemplateImport::new(
            "output-convert-adapter",
            TemplateImportFormat::Adapter,
            template,
        );

        assert!(!import.format().accepts_raw_text());

        let export = TemplateExport::new(
            "build-editor-json",
            TemplateExportFormat::Json,
            import.into_template(),
        );

        assert_eq!(export.target_name(), "build-editor-json");
        assert_eq!(export.template().id(), "adapter-template");
    }
}

mod case_build_template_native_json_import_export_roundtrips_editor_contract {
    use super::*;

    #[test]
    fn build_template_native_json_import_export_roundtrips_editor_contract() {
        let template = BuildTemplate::new(
            "native-json-template",
            vec![
                BuildSlot::new(
                    BuildSlotId::new(1),
                    vec![
                        CellCoord::new_unchecked(0, 0),
                        CellCoord::new_unchecked(1, 0),
                    ],
                )
                .with_label("left slot")
                .with_allowed_pieces(vec![PieceKind::I, PieceKind::O])
                .with_required_piece(PieceKind::I)
                .with_hold_constraint(SlotHoldConstraint::RequiresHold)
                .with_order_constraint(SlotOrderConstraint::Before(BuildSlotId::new(2)))
                .with_symmetry(SlotSymmetry::MirrorX)
                .with_canonicalization(SlotCanonicalization::CanonicalBySymmetry),
                BuildSlot::new(BuildSlotId::new(2), vec![CellCoord::new_unchecked(2, 0)])
                    .with_allowed_pieces(vec![PieceKind::T]),
            ],
        )
        .with_label("Editor template")
        .with_board_size(BoardSize::new(10, 4).expect("board"))
        .with_symmetry(TemplateSymmetry::MirrorX)
        .with_canonicalization(TemplateCanonicalization::CanonicalByGeometry);

        let export = TemplateExport::new(
            "build-editor-json",
            TemplateExportFormat::Json,
            template.clone(),
        );
        let json = export.to_json().expect("native template JSON export");
        let import = TemplateImport::from_json("build-editor-json", &json)
            .expect("native template JSON import");

        assert_eq!(import.format(), TemplateImportFormat::Json);
        assert_eq!(import.template(), &template);
        assert!(json.contains("\"schema_version\": 2"));
        assert!(json.contains("\"allowed_pieces\""));
        assert!(json.contains("\"order_constraint\""));
    }
}

mod case_build_template_native_json_import_rejects_raw_external_text {
    use super::*;

    #[test]
    fn build_template_native_json_import_rejects_raw_external_text() {
        let error =
            TemplateImport::from_json("external", "not-json").expect_err("native JSON is required");

        assert_eq!(error, TemplateJsonError::InvalidJson);
    }
}

mod case_build_template_native_json_import_rejects_out_of_bounds_cells {
    use super::*;

    #[test]
    fn build_template_native_json_import_rejects_out_of_bounds_cells() {
        let json = r#"{
        "schema_version": 2,
        "id": "bad-cell",
        "board": { "width": 10, "height": 4 },
        "slots": [{
            "id": 1,
            "cells": [{ "x": 10, "y": 0 }],
            "allowed_pieces": ["I"]
        }]
    }"#;

        let error = TemplateImport::from_json("bad-cell", json).expect_err("out of bounds cell");

        assert!(matches!(
            error,
            TemplateJsonError::InvalidField {
                context: "template.slots[].cells[]",
                field: "x/y",
                ..
            }
        ));
    }
}

mod case_build_template_native_json_import_rejects_duplicate_cells {
    use super::*;

    #[test]
    fn build_template_native_json_import_rejects_duplicate_cells() {
        let json = r#"{
        "schema_version": 2,
        "id": "duplicate-cell",
        "board": { "width": 10, "height": 4 },
        "slots": [{
            "id": 1,
            "cells": [{ "x": 0, "y": 0 }, { "x": 0, "y": 0 }],
            "allowed_pieces": ["I"]
        }]
    }"#;

        let error = TemplateImport::from_json("duplicate-cell", json).expect_err("duplicate cell");

        assert_eq!(
            error,
            TemplateJsonError::InvalidField {
                context: "template.slots[]",
                field: "cells",
                reason: "duplicate cell at slot index 0 and cell index 1".to_owned()
            }
        );
    }
}

mod case_build_template_native_json_import_rejects_required_piece_outside_allowed_pieces {
    use super::*;

    #[test]
    fn build_template_native_json_import_rejects_required_piece_outside_allowed_pieces() {
        let json = r#"{
        "schema_version": 2,
        "id": "bad-required-piece",
        "board": { "width": 10, "height": 4 },
        "slots": [{
            "id": 1,
            "cells": [{ "x": 0, "y": 0 }],
            "allowed_pieces": ["I"],
            "required_piece": "O"
        }]
    }"#;

        let error =
            TemplateImport::from_json("bad-required-piece", json).expect_err("required piece");

        assert!(matches!(
            error,
            TemplateJsonError::InvalidField {
                context: "template.slots[]",
                field: "required_piece",
                ..
            }
        ));
    }
}
