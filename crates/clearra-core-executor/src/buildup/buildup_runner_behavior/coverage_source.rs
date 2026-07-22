use super::*;

pub(super) fn assert_verify_first_coverage_rejected() {
    let variants = verified_build_variants(vec![CNativeBuildVariantView {
        candidate_id: 0x11,
        canonical_operation_set_id: 0x11,
        operation_set_hash: 0x11,
        coverage_pattern_id: 0,
        ..Default::default()
    }]);
    let identity = CoverageUniverseIdentity {
        piece_source_id: 11,
        pattern_universe_id: 7,
        pattern_weight_model_id: 9,
    };
    let error = coverage_rows_from_build_variants(
        BuildUpExecutionMode::VerifyFirst,
        &variants,
        4,
        identity,
    )
    .expect_err("verify-first cannot source coverage");
    assert_eq!(
        error,
        BuildUpRunnerError::CoverageSourceModeRejected {
            mode: BuildUpExecutionMode::VerifyFirst
        }
    );
}

pub(super) fn verified_build_variants(
    variants: Vec<CNativeBuildVariantView>,
) -> Vec<PatternVerifiedBuildVariant> {
    variants
        .into_iter()
        .map(|variant| {
            let pattern_id = variant.coverage_pattern_id;
            PatternVerifiedBuildVariant::try_new(
                owned_build_variant(variant),
                PatternCoverageVerification::pattern_specific_buildup(pattern_id),
            )
            .expect("variant has matching pattern-specific buildup verification")
        })
        .collect()
}

mod case_coverage_rows_use_accepted_build_variant_pattern_ids {
    use super::*;

    #[test]
    fn coverage_rows_use_accepted_build_variant_pattern_ids() {
        let variants = vec![
            CNativeBuildVariantView {
                candidate_id: 0x11,
                canonical_operation_set_id: 0x11,
                operation_set_hash: 0x11,
                coverage_pattern_id: 2,
                ..Default::default()
            },
            CNativeBuildVariantView {
                candidate_id: 0x22,
                canonical_operation_set_id: 0x22,
                operation_set_hash: 0x22,
                coverage_pattern_id: 5,
                ..Default::default()
            },
        ];

        let identity = CoverageUniverseIdentity {
            piece_source_id: 11,
            pattern_universe_id: 7,
            pattern_weight_model_id: 9,
        };
        let verified = verified_build_variants(variants);
        let rows = coverage_rows_from_build_variants(
            BuildUpExecutionMode::EnumerateVariants,
            &verified,
            8,
            identity,
        )
        .expect("rows");

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].candidate_id(), 0x11);
        assert_eq!(rows[0].piece_source_id(), 11);
        assert_eq!(rows[0].row_kind(), &CoverageRowKind::Build);
        assert_eq!(rows[0].pattern_universe_id(), PatternUniverseId::new(7));
        assert_eq!(
            rows[0].pattern_weight_model_id(),
            PatternWeightModelId::new(9)
        );
        assert!(rows[0].coverage_bits().contains(PatternId::new(2)));
        assert_eq!(rows[1].candidate_id(), 0x22);
        assert!(rows[1].coverage_bits().contains(PatternId::new(5)));
    }
}

mod case_coverage_rows_group_same_candidate_pattern_once {
    use super::*;

    #[test]
    fn coverage_rows_group_same_candidate_pattern_once() {
        let variants = vec![
            CNativeBuildVariantView {
                candidate_id: 0x11,
                build_variant_id: 1,
                canonical_operation_set_id: 0x11,
                operation_set_hash: 0x11,
                coverage_pattern_id: 2,
                queue_cursor: 4,
                ..Default::default()
            },
            CNativeBuildVariantView {
                candidate_id: 0x11,
                build_variant_id: 2,
                canonical_operation_set_id: 0x11,
                operation_set_hash: 0x11,
                coverage_pattern_id: 2,
                queue_cursor: 5,
                ..Default::default()
            },
        ];

        let identity = CoverageUniverseIdentity {
            piece_source_id: 11,
            pattern_universe_id: 7,
            pattern_weight_model_id: 9,
        };
        let verified = verified_build_variants(variants);
        let rows = coverage_rows_from_build_variants(
            BuildUpExecutionMode::EnumerateVariants,
            &verified,
            8,
            identity,
        )
        .expect("rows");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].candidate_id(), 0x11);
        assert!(rows[0].coverage_bits().contains(PatternId::new(2)));
        assert_eq!(rows[0].coverage_bits().count_ones(), 1);
    }
}

mod case_operation_set_hash_collision_does_not_merge_candidates {
    use super::*;

    #[test]
    fn operation_set_hash_collision_does_not_merge_candidates() {
        let variants = verified_build_variants(vec![
            CNativeBuildVariantView {
                candidate_id: 0x11,
                build_variant_id: 1,
                canonical_operation_set_id: 0x11,
                operation_set_hash: 0xdead,
                coverage_pattern_id: 0,
                ..Default::default()
            },
            CNativeBuildVariantView {
                candidate_id: 0x22,
                build_variant_id: 1,
                canonical_operation_set_id: 0x22,
                operation_set_hash: 0xdead,
                coverage_pattern_id: 0,
                ..Default::default()
            },
        ]);
        let rows = coverage_rows_from_build_variants(
            BuildUpExecutionMode::EnumerateVariants,
            &variants,
            1,
            CoverageUniverseIdentity {
                piece_source_id: 1,
                pattern_universe_id: 2,
                pattern_weight_model_id: 3,
            },
        )
        .expect("coverage rows");

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].candidate_id(), 0x11);
        assert_eq!(rows[1].candidate_id(), 0x22);
    }
}

mod case_coverage_groups_by_candidate_id_not_operation_set_hash {
    use super::*;

    #[test]
    fn coverage_groups_by_candidate_id_not_operation_set_hash() {
        let variants = verified_build_variants(vec![
            CNativeBuildVariantView {
                candidate_id: 0x44,
                build_variant_id: 1,
                canonical_operation_set_id: 0x44,
                operation_set_hash: 0x1111,
                coverage_pattern_id: 0,
                ..Default::default()
            },
            CNativeBuildVariantView {
                candidate_id: 0x44,
                build_variant_id: 2,
                canonical_operation_set_id: 0x44,
                operation_set_hash: 0x2222,
                coverage_pattern_id: 1,
                ..Default::default()
            },
        ]);
        let rows = coverage_rows_from_build_variants(
            BuildUpExecutionMode::EnumerateVariants,
            &variants,
            2,
            CoverageUniverseIdentity {
                piece_source_id: 1,
                pattern_universe_id: 2,
                pattern_weight_model_id: 3,
            },
        )
        .expect("coverage rows");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].candidate_id(), 0x44);
        assert_eq!(rows[0].coverage_bits().count_ones(), 2);
    }
}

mod case_same_candidate_covers_two_patterns_when_two_intersections_non_empty {
    use super::*;

    #[test]
    fn same_candidate_covers_two_patterns_when_two_intersections_non_empty() {
        let variants = verified_build_variants(vec![
            CNativeBuildVariantView {
                candidate_id: 0x11,
                canonical_operation_set_id: 0x11,
                operation_set_hash: 0x11,
                coverage_pattern_id: 0,
                queue_cursor: 4,
                ..Default::default()
            },
            CNativeBuildVariantView {
                candidate_id: 0x11,
                canonical_operation_set_id: 0x11,
                operation_set_hash: 0x11,
                coverage_pattern_id: 1,
                queue_cursor: 4,
                ..Default::default()
            },
        ]);
        let identity = CoverageUniverseIdentity {
            piece_source_id: 11,
            pattern_universe_id: 7,
            pattern_weight_model_id: 9,
        };

        let rows = coverage_rows_from_build_variants(
            BuildUpExecutionMode::EnumerateVariants,
            &variants,
            4,
            identity,
        )
        .expect("rows");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].candidate_id(), 0x11);
        assert!(rows[0].coverage_bits().contains(PatternId::new(0)));
        assert!(rows[0].coverage_bits().contains(PatternId::new(1)));
        assert_eq!(rows[0].coverage_bits().count_ones(), 2);
    }
}

mod case_representative_witness_covers_one_pattern_but_enumeration_covers_more {
    use super::*;

    #[test]
    fn representative_witness_covers_one_pattern_but_enumeration_covers_more() {
        let variants = verified_build_variants(vec![
            CNativeBuildVariantView {
                candidate_id: 0x22,
                canonical_operation_set_id: 0x22,
                operation_set_hash: 0x22,
                coverage_pattern_id: 0,
                queue_cursor: 1,
                ..Default::default()
            },
            CNativeBuildVariantView {
                candidate_id: 0x22,
                canonical_operation_set_id: 0x22,
                operation_set_hash: 0x22,
                coverage_pattern_id: 2,
                queue_cursor: 3,
                ..Default::default()
            },
        ]);
        let identity = CoverageUniverseIdentity {
            piece_source_id: 11,
            pattern_universe_id: 7,
            pattern_weight_model_id: 9,
        };

        let rows = coverage_rows_from_build_variants(
            BuildUpExecutionMode::EnumerateVariants,
            &variants,
            4,
            identity,
        )
        .expect("rows");

        assert_eq!(rows.len(), 1);
        assert!(rows[0].coverage_bits().contains(PatternId::new(0)));
        assert!(rows[0].coverage_bits().contains(PatternId::new(2)));
        assert_eq!(rows[0].coverage_bits().count_ones(), 2);
    }
}

mod case_verify_first_cannot_create_coverage_row {
    use super::*;

    #[test]
    fn verify_first_cannot_create_coverage_row() {
        assert_verify_first_coverage_rejected();
    }
}

mod case_verify_first_cannot_source_coverage {
    use super::*;

    #[test]
    fn verify_first_cannot_source_coverage() {
        assert_verify_first_coverage_rejected();
    }
}

mod case_coverage_row_requires_pattern_specific_buildup_verification {
    use super::*;

    #[test]
    fn coverage_row_requires_pattern_specific_buildup_verification() {
        let variant = CNativeBuildVariantView {
            candidate_id: 0x33,
            canonical_operation_set_id: 0x33,
            operation_set_hash: 0x33,
            coverage_pattern_id: 1,
            ..Default::default()
        };
        let variants = vec![PatternVerifiedBuildVariant::try_new(
            owned_build_variant(variant),
            PatternCoverageVerification::pattern_specific_buildup(1),
        )
        .expect("pattern-specific BuildUp verifies pattern")];
        let identity = CoverageUniverseIdentity {
            piece_source_id: 11,
            pattern_universe_id: 7,
            pattern_weight_model_id: 9,
        };

        let rows = coverage_rows_from_build_variants(
            BuildUpExecutionMode::EnumerateVariants,
            &variants,
            4,
            identity,
        )
        .expect("rows");

        assert_eq!(rows.len(), 1);
        assert!(rows[0].coverage_bits().contains(PatternId::new(1)));
    }
}

mod case_coverage_pattern_id_injection_without_pattern_verification_rejected {
    use super::*;

    #[test]
    fn coverage_pattern_id_injection_without_pattern_verification_rejected() {
        let error = PatternVerifiedBuildVariant::try_new(
            owned_build_variant(CNativeBuildVariantView {
                candidate_id: 0x44,
                canonical_operation_set_id: 0x44,
                operation_set_hash: 0x44,
                coverage_pattern_id: 2,
                ..Default::default()
            }),
            PatternCoverageVerification::pattern_specific_buildup(3),
        )
        .expect_err("mismatched injected pattern id is rejected");

        assert_eq!(
            error,
            BuildUpRunnerError::CoveragePatternVerificationMismatch {
                variant_pattern_id: 2,
                verified_pattern_id: 3,
            }
        );
    }
}

mod case_coverage_rows_reject_missing_universe_identity_before_untyped_bridge {
    use super::*;

    #[test]
    fn coverage_rows_reject_missing_universe_identity_before_untyped_bridge() {
        let variants = vec![CNativeBuildVariantView {
            candidate_id: 0x11,
            canonical_operation_set_id: 0x11,
            operation_set_hash: 0x11,
            coverage_pattern_id: 2,
            ..Default::default()
        }];
        let identity = CoverageUniverseIdentity {
            piece_source_id: 11,
            pattern_universe_id: 0,
            pattern_weight_model_id: 9,
        };

        let verified = verified_build_variants(variants);
        let error = coverage_rows_from_build_variants(
            BuildUpExecutionMode::EnumerateVariants,
            &verified,
            8,
            identity,
        )
        .expect_err("missing universe identity");

        assert_eq!(
            error,
            BuildUpRunnerError::CoverageBridge(
                CoverageRowBridgeError::MissingPatternUniverseIdentity
            )
        );
    }
}

mod case_coverage_rows_reject_missing_piece_source_identity {
    use super::*;

    #[test]
    fn coverage_rows_reject_missing_piece_source_identity() {
        let variants = vec![CNativeBuildVariantView {
            candidate_id: 0x11,
            canonical_operation_set_id: 0x11,
            operation_set_hash: 0x11,
            coverage_pattern_id: 2,
            ..Default::default()
        }];
        let identity = CoverageUniverseIdentity {
            piece_source_id: 0,
            pattern_universe_id: 7,
            pattern_weight_model_id: 9,
        };

        let verified = verified_build_variants(variants);
        let error = coverage_rows_from_build_variants(
            BuildUpExecutionMode::EnumerateVariants,
            &verified,
            8,
            identity,
        )
        .expect_err("missing piece source identity");

        assert_eq!(
            error,
            BuildUpRunnerError::CoverageBridge(CoverageRowBridgeError::MissingPieceSourceIdentity)
        );
    }
}

mod case_coverage_universe_identity_is_non_zero_and_source_sensitive {
    use super::*;

    #[test]
    fn coverage_universe_identity_is_non_zero_and_source_sensitive() {
        let opening_query = OpeningPcSearchQuery::new(PcTarget::two_lines()).with_queue(
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])),
        );
        let opening = ProblemCompiler::compile_opening_pc(&opening_query).expect("opening");
        let build_query = clearra_problem::query::BuildQuery::coverage_bridge(
            clearra_problem::query::BuildTemplateBridge::new(
                "template-a",
                clearra_core_domain::board::board_size::BoardSize::new(10, 4).expect("board"),
                2,
            ),
            8,
            clearra_problem::query::BuildProblemLimits::new(12, 8),
        );
        let build = ProblemCompiler::compile_build(&build_query).expect("build");

        let opening_identity = coverage_universe_identity(&opening);
        let build_identity = coverage_universe_identity(&build);

        assert_ne!(opening_identity.piece_source_id, 0);
        assert_ne!(opening_identity.pattern_universe_id, 0);
        assert_ne!(opening_identity.pattern_weight_model_id, 0);
        assert_ne!(build_identity.piece_source_id, 0);
        assert_ne!(build_identity.pattern_universe_id, 0);
        assert_ne!(build_identity.pattern_weight_model_id, 0);
        assert_ne!(
            opening_identity.pattern_universe_id,
            build_identity.pattern_universe_id
        );
        assert_ne!(
            opening_identity.pattern_weight_model_id,
            build_identity.pattern_weight_model_id
        );
    }
}

#[cfg(feature = "native-c-core")]
mod case_build_coverage_can_select_one_materialized_pattern {
    use super::*;

    #[test]
    fn build_coverage_can_select_one_materialized_pattern() {
        let query = clearra_problem::query::BuildQuery::coverage_bridge(
            clearra_problem::query::BuildTemplateBridge::new(
                "template-a",
                clearra_core_domain::board::board_size::BoardSize::new(10, 4).expect("board"),
                2,
            ),
            8,
            clearra_problem::query::BuildProblemLimits::new(12, 8),
        )
        .with_selected_pattern_id(3);
        let problem = ProblemCompiler::compile_build(&query).expect("build problem");
        let packing = PackingRunner::run(&problem).expect("packing");

        let buildup = BuildUpRunner::run(&problem, &packing).expect("buildup");

        assert_eq!(buildup.coverage_row_count(), 0);
    }
}

#[cfg(feature = "native-c-core")]
mod case_build_coverage_pattern_id_out_of_range_is_rejected {
    use super::*;

    #[test]
    fn build_coverage_pattern_id_out_of_range_is_rejected() {
        let query = clearra_problem::query::BuildQuery::coverage_bridge(
            clearra_problem::query::BuildTemplateBridge::new(
                "template-a",
                clearra_core_domain::board::board_size::BoardSize::new(10, 4).expect("board"),
                2,
            ),
            2,
            clearra_problem::query::BuildProblemLimits::new(12, 2),
        )
        .with_selected_pattern_id(3);
        let problem = ProblemCompiler::compile_build(&query).expect("build problem");
        let packing = PackingRunner::run(&problem).expect("packing");

        let error = BuildUpRunner::run(&problem, &packing).expect_err("pattern id error");

        assert_eq!(
            error,
            BuildUpRunnerError::CoveragePatternIdOutOfRange {
                pattern_id: 3,
                pattern_count: 2,
                source: "build-selected-pattern-id",
            }
        );
    }
}
