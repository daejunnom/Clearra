use super::*;

mod case_spin_target_predicate_applies_after_replay_before_coverage_row {
    use super::*;

    #[test]
    fn spin_target_predicate_applies_after_replay_before_coverage_row() {
        let result = SpinTargetRunner::run(
            &SpinTarget::tsd("tsd"),
            &[evidence(0xaaa, 1)],
            Some(&RequiresKickEvidenceClassifier),
            &ScoreProfile::new("guideline", "Guideline"),
            77,
            4,
            PatternUniverseId::new(10),
            PatternWeightModelId::new(20),
        )
        .expect("spin target result");

        assert_eq!(result.execution_report().evaluated_build_variant_count(), 1);
        assert_eq!(result.execution_report().satisfied_build_variant_count(), 0);
        assert_eq!(result.coverage_matrix().rows().len(), 0);
        assert_eq!(result.probability_result().probability().get(), 0.0);
        assert!(!result.probability_result().probability_complete());
        assert_eq!(
            result.probability_result().truncation_reason(),
            Some("missing-kick-evidence")
        );
        assert!(!result.execution_report().exact());
        assert_eq!(
            result.execution_report().trace_completeness(),
            "missing-kick-evidence"
        );
        assert_eq!(
            result.execution_report().diagnostic_code(),
            Some("W_SPIN_TARGET_PROBABILITY_INCOMPLETE")
        );
    }
}

mod case_spin_target_coverage_bridge_outputs_pattern_bitset_row {
    use super::*;

    #[test]
    fn spin_target_coverage_bridge_outputs_pattern_bitset_row() {
        let owned = CBuildVariantView::from_native(&variant(0xbbb, 2)).expect("variant");
        let row = SpinTargetCoverageBridge::row_from_build_variant(
            SpinTarget::tsd("tsd").id(),
            &owned,
            77,
            4,
            PatternUniverseId::new(10),
            PatternWeightModelId::new(20),
        )
        .expect("spin row");

        assert_eq!(row.row().candidate_id(), 0xbbb);
        assert_eq!(row.row().coverage_bits().covered_patterns()[0].index(), 2);
    }
}

mod case_spin_probability_result_uses_union_probability {
    use super::*;

    #[test]
    fn spin_probability_result_uses_union_probability() {
        let result = run_result(&[evidence(0xaaa, 1), evidence(0xbbb, 1), evidence(0xccc, 3)]);

        assert_eq!(result.execution_report().satisfied_build_variant_count(), 3);
        assert_eq!(result.probability_result().covered_pattern_count(), 2);
        assert_eq!(result.probability_result().pattern_count(), 4);
        assert_eq!(result.probability_result().probability().get(), 0.5);
    }
}

mod case_spin_probability_uses_pattern_bitset_union {
    use super::*;

    #[test]
    pub(crate) fn spin_probability_uses_pattern_bitset_union() {
        let result = run_result(&[evidence(0xaaa, 1), evidence(0xbbb, 1), evidence(0xccc, 3)]);

        assert_eq!(result.execution_report().satisfied_build_variant_count(), 3);
        assert_eq!(result.probability_result().covered_pattern_count(), 2);
        assert_eq!(result.probability_result().probability().get(), 0.5);
    }
}
pub(crate) use case_spin_probability_uses_pattern_bitset_union::spin_probability_uses_pattern_bitset_union;

mod case_spin_target_probability_uses_pattern_bitset_union {
    use super::*;

    #[test]
    fn spin_target_probability_uses_pattern_bitset_union() {
        spin_probability_uses_pattern_bitset_union();
    }
}

mod case_tsd_probability_threshold_query_reports_satisfaction {
    use super::*;

    #[test]
    fn tsd_probability_threshold_query_reports_satisfaction() {
        let threshold =
            clearra_core_domain::probability::probability_value::ProbabilityValue::new(0.95)
                .expect("threshold");
        let target = SpinTarget::tsd("tsd").with_target_probability_threshold(threshold);
        let result = SpinTargetRunner::run(
            &target,
            &[
                evidence(0xaaa, 0),
                evidence(0xbbb, 1),
                evidence(0xccc, 2),
                evidence(0xddd, 3),
            ],
            Some(&AlwaysTsdClassifier),
            &ScoreProfile::new("guideline", "Guideline"),
            77,
            4,
            PatternUniverseId::new(10),
            PatternWeightModelId::new(20),
        )
        .expect("spin target result");

        assert_eq!(result.probability_result().probability().get(), 1.0);
        assert_eq!(result.threshold_satisfied(), Some(true));
    }
}
