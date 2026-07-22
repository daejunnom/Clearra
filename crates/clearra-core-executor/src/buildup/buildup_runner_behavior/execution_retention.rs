use super::*;

fn expected_oooo_count_complete() -> bool {
    true
}

fn expected_oooo_count_truncated_reason() -> &'static str {
    "none"
}

fn expected_oooo_scenario_trace_retention_truncated() -> bool {
    true
}

fn expected_oooo_scenario_trace_retention_reason() -> &'static str {
    "retained_trace_limit"
}

#[cfg(feature = "native-c-core")]
mod case_retained_trace_limit_does_not_truncate_solution_count {
    use super::*;

    #[test]
    fn retained_trace_limit_does_not_truncate_solution_count() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0b11 | (0b11 << 10)),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])),
            PieceWindow::new(4),
        )
        .with_retained_trace_limit(1);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
        let packing = PackingRunner::run(&problem).expect("packing");

        let buildup = BuildUpRunner::run(&problem, &packing).expect("buildup");

        assert!(buildup.total_solution_count() > buildup.retained_trace_count());
        assert_eq!(buildup.retained_trace_count(), 1);
        assert_eq!(buildup.count_complete(), expected_oooo_count_complete());
        assert_eq!(
            buildup.count_truncated_reason(),
            expected_oooo_count_truncated_reason()
        );
        assert_eq!(
            buildup.trace_retention_truncated(),
            expected_oooo_scenario_trace_retention_truncated()
        );
        assert_eq!(
            buildup.trace_retention_reason(),
            expected_oooo_scenario_trace_retention_reason()
        );
    }
}

#[cfg(feature = "native-c-core")]

mod case_execution_variant_set_preserves_successes_from_multiple_patterns {
    use super::*;

    #[test]
    fn execution_variant_set_preserves_successes_from_multiple_patterns() {
        let first = owned_build_variant(CNativeBuildVariantView {
            candidate_id: 7,
            build_variant_id: 10,
            canonical_operation_set_id: 7,
            coverage_pattern_id: 0,
            trace_identity: 100,
            ..Default::default()
        });
        let second = owned_build_variant(CNativeBuildVariantView {
            candidate_id: 7,
            build_variant_id: 11,
            canonical_operation_set_id: 7,
            coverage_pattern_id: 1,
            trace_identity: 101,
            ..Default::default()
        });
        let mut executions = ExecutionVariantSet::default();

        retain_execution_variants(&mut executions, vec![first], 2);
        retain_execution_variants(&mut executions, vec![second], 2);

        assert_eq!(executions.len(), 2);
        assert_eq!(executions.variants()[0].coverage_pattern_id(), 0);
        assert_eq!(executions.variants()[1].coverage_pattern_id(), 1);
    }
}

mod case_buildup_witness_counts_all_pattern_verified_executions {
    use super::*;

    #[test]
    fn buildup_witness_counts_all_pattern_verified_executions() {
        let problem = ProblemCompiler::compile_scenario_pc(
            &PcScenarioQuery::new(
                PcScenarioBoard::standard_10(2, 0x3f0),
                PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
                PieceWindow::new(1),
            )
            .with_exact_pieces(Some(1)),
        )
        .expect("problem");
        let mut executions = ExecutionVariantSet::default();
        for pattern_id in [0, 1] {
            executions.insert(owned_build_variant(CNativeBuildVariantView {
                candidate_id: 7,
                build_variant_id: u64::from(pattern_id) + 1,
                canonical_operation_set_id: 7,
                coverage_pattern_id: pattern_id,
                trace_identity: 55,
                ..Default::default()
            }));
        }
        let candidate = CPackingCandidate {
            candidate_id: 7,
            operation_count: 1,
            ..Default::default()
        };
        let result = CBuildUpResult {
            candidate_id: 7,
            success: 1,
            ..Default::default()
        };

        let acceptance =
            crate::buildup::buildup_candidate_acceptance::BuildUpCandidateAcceptance::explicit(
                vec![result],
            );
        let witness =
            buildup_witness_from_c_results(&problem, &[candidate], &acceptance, &executions, 1, 1);

        assert_eq!(witness.total_solution_count, 1);
        assert_eq!(witness.unique_solution_count, 1);
    }
}

mod case_representative_trace_selection_does_not_discard_execution_variants {
    use super::*;

    #[test]
    fn representative_trace_selection_does_not_discard_execution_variants() {
        let variants = vec![
            owned_build_variant(CNativeBuildVariantView {
                candidate_id: 22,
                build_variant_id: 2,
                coverage_pattern_id: 1,
                trace_identity: 200,
                ..Default::default()
            }),
            owned_build_variant(CNativeBuildVariantView {
                candidate_id: 11,
                build_variant_id: 1,
                coverage_pattern_id: 0,
                trace_identity: 100,
                ..Default::default()
            }),
        ];

        let selection = RepresentativeTraceSelection::select(&variants).expect("selection");
        let selected = selection.selected_variant(&variants).expect("variant");

        assert_eq!(selected.candidate_id(), 11);
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0].coverage_pattern_id(), 1);
        assert_eq!(variants[1].coverage_pattern_id(), 0);
    }
}
