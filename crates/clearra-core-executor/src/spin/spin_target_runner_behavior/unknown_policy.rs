use super::*;

mod case_exact_spin_non_match_remains_complete_without_coverage_row {
    use super::*;

    #[test]
    fn exact_spin_non_match_remains_complete_without_coverage_row() {
        let result = SpinTargetRunner::run(
            &SpinTarget::tsd("tsd"),
            &[evidence(0xaaa, 1)],
            Some(&NeverTsdClassifier),
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
        assert!(result.probability_result().probability_complete());
        assert!(result.execution_report().exact());
        assert_eq!(result.execution_report().trace_completeness(), "full");
        assert_eq!(result.execution_report().diagnostic_code(), None);
    }
}

mod case_spin_target_runner_rejects_missing_spin_classifier {
    use super::*;

    #[test]
    pub(crate) fn spin_target_runner_rejects_missing_spin_classifier() {
        let result = SpinTargetRunner::run(
            &SpinTarget::tsd("tsd"),
            &[evidence(0xaaa, 1)],
            None,
            &ScoreProfile::new("guideline", "Guideline"),
            77,
            4,
            PatternUniverseId::new(10),
            PatternWeightModelId::new(20),
        );

        assert_eq!(result, Err(SpinTargetRunnerError::MissingSpinClassifier));
    }
}
pub(crate) use case_spin_target_runner_rejects_missing_spin_classifier::spin_target_runner_rejects_missing_spin_classifier;

mod case_spin_target_requires_classifier {
    use super::*;

    #[test]
    fn spin_target_requires_classifier() {
        spin_target_runner_rejects_missing_spin_classifier();
    }
}

mod case_missing_kick_evidence_is_incomplete_not_exact_spin {
    use super::*;

    #[test]
    pub(crate) fn missing_kick_evidence_is_incomplete_not_exact_spin() {
        let native = CNativeBuildVariantView {
            trace_completeness_flags: CLR_BUILDUP_TRACE_COMPLETENESS_KICK_EVIDENCE_MISSING,
            ..variant(0xaaa, 1)
        };

        let result =
            SpinTargetRunner::run(
                &SpinTarget::tsd("tsd"),
                &[BuildVariantReplayEvidence::new(
                    native,
                    replay_layout(),
                    0,
                    vec![t_operation(0, 0)],
                )
                .expect("owned replay evidence")],
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
    }
}
pub(crate) use case_missing_kick_evidence_is_incomplete_not_exact_spin::missing_kick_evidence_is_incomplete_not_exact_spin;

mod case_missing_kick_evidence_is_incomplete_not_exact {
    use super::*;

    #[test]
    fn missing_kick_evidence_is_incomplete_not_exact() {
        missing_kick_evidence_is_incomplete_not_exact_spin();
    }
}
