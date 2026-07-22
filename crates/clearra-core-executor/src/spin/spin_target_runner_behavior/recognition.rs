use super::*;

mod case_spin_target_runner_uses_build_variant_not_packing_candidate {
    use super::*;

    #[test]
    fn spin_target_runner_uses_build_variant_not_packing_candidate() {
        let result = run_result(&[evidence(0xaaa, 1)]);

        assert_eq!(result.execution_report().build_variant_count(), 1);
        assert_eq!(result.execution_report().evaluated_build_variant_count(), 1);
        assert_eq!(
            result.execution_report().replay_basis(),
            "c-build-variant-operation-replay-basis"
        );
        assert_eq!(result.coverage_matrix().rows()[0].candidate_id(), 0xaaa);
    }
}

mod case_spin_target_runner_uses_all_build_variant_operations {
    use super::*;

    #[test]
    fn spin_target_runner_uses_all_build_variant_operations() {
        let result = SpinTargetRunner::run(
            &SpinTarget::tsd("tsd"),
            &[evidence_with_operations(
                0xaaa,
                1,
                vec![o_operation(0, 0), t_operation(4, 0)],
            )],
            Some(&RequiresPriorBoardClassifier),
            &ScoreProfile::new("guideline", "Guideline"),
            77,
            4,
            PatternUniverseId::new(10),
            PatternWeightModelId::new(20),
        )
        .expect("spin target result");

        assert_eq!(result.execution_report().evaluated_build_variant_count(), 1);
        assert_eq!(result.execution_report().satisfied_build_variant_count(), 1);
    }
}

mod case_spin_target_runner_preserves_variant_board_before_after {
    use super::*;

    #[test]
    fn spin_target_runner_preserves_variant_board_before_after() {
        let replay_evidence =
            evidence_with_operations(0xabc, 1, vec![o_operation(0, 0), t_operation(4, 0)]);
        let replay =
            BuildVariantMapper::to_replay_trace(replay_evidence.variant(), &replay_evidence)
                .expect("replay");
        let spin = replay
            .events()
            .iter()
            .rev()
            .find_map(|event| match event {
                ReplayEvent::SpinBasis(spin) => Some(*spin),
                _ => None,
            })
            .expect("spin basis");

        assert_eq!(spin.piece(), PieceKind::T);
        assert_ne!(spin.board_before(), 0);
        assert_eq!(
            spin.board_after_placement() & spin.board_before(),
            spin.board_before()
        );
    }
}

mod case_native_build_variant_to_replay_trace_uses_operation_set_from_candidate {
    use super::*;

    #[test]
    fn native_build_variant_to_replay_trace_uses_operation_set_from_candidate() {
        let mut candidate =
            candidate_with_operations(vec![o_operation(0, 0), t_right_operation(4, 0)]);
        let native = CNativeBuildVariantView {
            placed_count: 2,
            ..variant(0xabc, 1)
        };
        candidate.candidate_id = native.candidate_id;
        let replay_evidence = BuildVariantReplayEvidence::from_native_build_variant_and_candidate(
            native,
            replay_layout(),
            0,
            &candidate,
        )
        .expect("candidate-backed replay evidence");
        let replay =
            BuildVariantMapper::to_replay_trace(replay_evidence.variant(), &replay_evidence)
                .expect("replay");
        let placements = replay
            .events()
            .iter()
            .filter_map(|event| match event {
                ReplayEvent::Placement(placement) => Some((
                    placement.piece(),
                    placement.rotation(),
                    placement.x(),
                    placement.y(),
                )),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            placements,
            vec![
                (PieceKind::O, RotationState::Zero, 0, 0),
                (PieceKind::T, RotationState::Right, 4, 0),
            ]
        );
    }
}

mod case_spin_target_runner_rejects_missing_operation_basis {
    use super::*;

    #[test]
    fn spin_target_runner_rejects_missing_operation_basis() {
        let candidate = CPackingCandidate {
            operation_count: 0,
            ..Default::default()
        };
        let result = BuildVariantReplayEvidence::from_native_build_variant_and_candidate(
            variant(0xabc, 1),
            replay_layout(),
            0,
            &candidate,
        );

        assert_eq!(
            result,
            Err(BuildVariantReplayEvidenceError::MissingOperationBasis)
        );
    }
}

mod case_missing_operation_basis_is_error {
    use super::*;

    #[test]
    fn missing_operation_basis_is_error() {
        let candidate = CPackingCandidate {
            operation_count: 0,
            ..Default::default()
        };

        let error = BuildVariantReplayEvidence::from_native_build_variant_and_candidate(
            variant(0xabc, 1),
            replay_layout(),
            0,
            &candidate,
        )
        .expect_err("missing operation basis");

        assert_eq!(
            error,
            BuildVariantReplayEvidenceError::MissingOperationBasis
        );
    }
}

mod case_spin_target_runner_rejects_missing_spin_basis_for_exact_query {
    use super::*;

    #[test]
    fn spin_target_runner_rejects_missing_spin_basis_for_exact_query() {
        let result = SpinTargetRunner::run(
            &SpinTarget::tsd("tsd"),
            &[evidence_with_operations(0xaaa, 1, Vec::new())],
            Some(&AlwaysTsdClassifier),
            &ScoreProfile::new("guideline", "Guideline"),
            77,
            4,
            PatternUniverseId::new(10),
            PatternWeightModelId::new(20),
        );

        assert_eq!(result, Err(SpinTargetRunnerError::MissingSpinBasis));
    }
}

mod case_spin_target_runner_does_not_use_stub_t_operation_for_native_variant {
    use super::*;

    #[test]
    fn spin_target_runner_does_not_use_stub_t_operation_for_native_variant() {
        let result = SpinTargetRunner::run(
            &SpinTarget::tsd("tsd"),
            &[evidence_with_operations(0xaaa, 1, vec![o_operation(0, 0)])],
            Some(&InputPieceClassifier),
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
    }
}

mod case_spin_target_runner_rejects_stub_replay_basis {
    use super::*;

    #[test]
    fn spin_target_runner_rejects_stub_replay_basis() {
        let result = SpinTargetRunner::run(
            &SpinTarget::tsd("tsd"),
            &[evidence_with_operations(0xaaa, 1, vec![o_operation(0, 0)])],
            Some(&InputPieceClassifier),
            &ScoreProfile::new("guideline", "Guideline"),
            77,
            4,
            PatternUniverseId::new(10),
            PatternWeightModelId::new(20),
        )
        .expect("spin target result");

        assert_eq!(
            result.execution_report().replay_basis(),
            "c-build-variant-operation-replay-basis"
        );
        assert_eq!(result.execution_report().satisfied_build_variant_count(), 0);
        assert_eq!(result.coverage_matrix().rows().len(), 0);
    }
}
