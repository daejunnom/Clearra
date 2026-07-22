use super::*;

mod case_c_build_variant_kick_evidence_reaches_replay_event {
    use super::*;

    #[test]
    fn c_build_variant_kick_evidence_reaches_replay_event() {
        let mut kick = [CKickEvidenceView::first_success(0, 1, 1, 2, 1, -1)];
        kick[0].kick_table_id = 11;
        kick[0].kick_profile_id = 22;
        kick[0].predecessor_x = -1;
        kick[0].predecessor_y = 4;
        kick[0].result_x = 0;
        kick[0].result_y = 0;
        let native = CNativeBuildVariantView {
            kick_evidence: kick.as_ptr(),
            kick_evidence_count: 1,
            ..variant(0xaaa, 1)
        };
        let replay_evidence = BuildVariantReplayEvidence::new(
            native,
            replay_layout(),
            0,
            vec![t_right_operation(0, 0)],
        )
        .expect("owned replay evidence");

        let replay =
            BuildVariantMapper::to_replay_trace(replay_evidence.variant(), &replay_evidence)
                .expect("replay");

        assert!(replay.events().iter().any(|event| {
            matches!(event, ReplayEvent::KickEvidence(evidence)
            if evidence.kick_index() == 2
                && evidence.kick_table_id() == 11
            && evidence.predecessor() == (-1, 4)
                && evidence.result() == (0, 0))
        }));
    }
}

mod case_kick_evidence_flows_from_build_variant_to_spin_classifier {
    use super::*;

    #[test]
    fn kick_evidence_flows_from_build_variant_to_spin_classifier() {
        let kick = [CKickEvidenceView::first_success(0, 1, 1, 2, 1, -1)];
        let native = CNativeBuildVariantView {
            kick_evidence: kick.as_ptr(),
            kick_evidence_count: 1,
            ..variant(0xaaa, 1)
        };
        let replay_evidence = BuildVariantReplayEvidence::new(
            native,
            replay_layout(),
            0,
            vec![t_right_operation(0, 0)],
        )
        .expect("owned replay evidence");

        let result = SpinTargetRunner::run(
            &SpinTarget::tsd("tsd"),
            &[replay_evidence],
            Some(&RequiresKickEvidenceClassifier),
            &ScoreProfile::new("guideline", "Guideline"),
            77,
            4,
            PatternUniverseId::new(10),
            PatternWeightModelId::new(20),
        )
        .expect("spin target result");

        assert_eq!(result.execution_report().satisfied_build_variant_count(), 1);
        assert_eq!(result.coverage_matrix().rows().len(), 1);
    }
}

mod case_spin_target_runner_uses_real_kick_evidence_count {
    use super::*;

    #[test]
    fn spin_target_runner_uses_real_kick_evidence_count() {
        let mut kick = [
            CKickEvidenceView::first_success(0, 1, 1, 2, 1, -1),
            CKickEvidenceView::first_success(0, 1, 1, 5, -1, 1),
        ];
        kick[0].result_x = 0;
        kick[0].result_y = 0;
        kick[1].result_x = 4;
        kick[1].result_y = 0;
        let native = CNativeBuildVariantView {
            kick_evidence: kick.as_ptr(),
            kick_evidence_count: kick.len() as u32,
            placed_count: 2,
            ..variant(0xaaa, 1)
        };
        let mut candidate =
            candidate_with_operations(vec![t_right_operation(0, 0), t_right_operation(4, 0)]);
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

        assert_eq!(
            replay
                .events()
                .iter()
                .filter(|event| matches!(event, ReplayEvent::KickEvidence(_)))
                .count(),
            2
        );

        let result = SpinTargetRunner::run(
            &SpinTarget::tsd("tsd"),
            &[replay_evidence],
            Some(&RequiresKickEvidenceClassifier),
            &ScoreProfile::new("guideline", "Guideline"),
            77,
            4,
            PatternUniverseId::new(10),
            PatternWeightModelId::new(20),
        )
        .expect("spin target result");

        assert_eq!(result.execution_report().satisfied_build_variant_count(), 1);
    }
}

mod case_kick_evidence_attaches_to_actual_step {
    use super::*;

    #[test]
    fn kick_evidence_attaches_to_actual_step() {
        let mut candidate = candidate_with_operations(vec![o_operation(0, 0), o_operation(2, 0)]);
        candidate.candidate_id = 0x61;
        let operation_order = [1_u16, 2_u16];
        let mut trace_steps = [
            trace_step(1, 0, C_PIECE_O, 0, 0, 0, 0),
            trace_step(2, 1, C_PIECE_O, 0, 2, 0, 0),
        ];
        trace_steps[1].kick_evidence_index = 0;
        let mut kick_evidence = [CKickEvidenceView::first_success(0, 1, 1, 2, 1, -1)];
        kick_evidence[0].result_x = 0;
        kick_evidence[0].result_y = 0;
        let native = CNativeBuildVariantView {
            candidate_id: candidate.candidate_id,
            placed_count: 2,
            operation_order_ids: operation_order.as_ptr(),
            operation_order_count: 2,
            trace_steps: trace_steps.as_ptr(),
            trace_step_count: 2,
            kick_evidence: kick_evidence.as_ptr(),
            kick_evidence_count: 1,
            ..variant(candidate.candidate_id, 1)
        };

        let replay_evidence = BuildVariantReplayEvidence::from_native_build_variant_and_candidate(
            native,
            replay_layout(),
            0,
            &candidate,
        )
        .expect("step-linked kick evidence");
        let replay =
            BuildVariantMapper::to_replay_trace(replay_evidence.variant(), &replay_evidence)
                .expect("replay");

        let step_indices = replay
            .events()
            .iter()
            .filter_map(|event| match event {
                ReplayEvent::KickEvidence(evidence) => Some(evidence.step_index()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(step_indices, vec![1]);
    }
}
