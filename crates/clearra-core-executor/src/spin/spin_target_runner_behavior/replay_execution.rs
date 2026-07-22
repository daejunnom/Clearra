use super::*;

mod case_alternate_success_order_replay_is_legal {
    use super::*;

    #[test]
    fn alternate_success_order_replay_is_legal() {
        let mut initial_board = 0_u64;
        for x in 5..10 {
            initial_board |= 1_u64 << x;
        }
        initial_board |= 1_u64 << 12;
        for x in 4..10 {
            initial_board |= 1_u64 << (10 + x);
        }

        let mut candidate = candidate_with_operations(vec![o_operation(0, 0), t_operation(2, 0)]);
        candidate.candidate_id = 0xabc;
        let operation_order = [2_u16, 1_u16];
        let trace_steps = [
            trace_step(2, 1, C_PIECE_T, 0, 2, 0, 0),
            trace_step(1, 0, C_PIECE_O, 0, 0, 0, 0b11),
        ];
        let native = CNativeBuildVariantView {
            candidate_id: candidate.candidate_id,
            placed_count: 2,
            operation_order_ids: operation_order.as_ptr(),
            operation_order_count: operation_order.len() as u16,
            trace_steps: trace_steps.as_ptr(),
            trace_step_count: trace_steps.len() as u16,
            trace_identity: 0xfeed,
            ..variant(candidate.candidate_id, 1)
        };

        let replay_evidence = BuildVariantReplayEvidence::from_native_build_variant_and_candidate(
            native,
            replay_layout(),
            initial_board,
            &candidate,
        )
        .expect("actual BuildUp order");
        let replay =
            BuildVariantMapper::to_replay_trace(replay_evidence.variant(), &replay_evidence)
                .expect("legal replay");

        let placements = replay
            .events()
            .iter()
            .filter_map(|event| match event {
                ReplayEvent::Placement(placement) => Some(placement.piece()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(placements, vec![PieceKind::T, PieceKind::O]);
        let operation_ids = replay
            .events()
            .iter()
            .filter_map(|event| match event {
                ReplayEvent::Lock(lock) => Some(lock.operation_id().0),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(operation_ids, vec![2, 1]);
        assert_eq!(
            replay
                .solution_trace()
                .steps()
                .last()
                .expect("last step")
                .board_after()
                .after_line_clear()
                .occupied(),
            0
        );
    }
}

mod case_hold_decision_sequence_is_preserved {
    use super::*;

    #[test]
    fn hold_decision_sequence_is_preserved() {
        let mut candidate = candidate_with_operations(vec![o_operation(0, 0)]);
        candidate.candidate_id = 0x51;
        let operation_order = [1_u16];
        let mut trace_steps = [trace_step(1, 0, C_PIECE_O, 0, 0, 0, 0)];
        trace_steps[0].hold_branch_kind = C_BUILDUP_HOLD_BRANCH_SWAP_HELD;
        trace_steps[0].used_hold = 1;
        trace_steps[0].incoming_piece = C_PIECE_T;
        trace_steps[0].held_piece_before = C_PIECE_O;
        let native = CNativeBuildVariantView {
            candidate_id: candidate.candidate_id,
            operation_order_ids: operation_order.as_ptr(),
            operation_order_count: 1,
            trace_steps: trace_steps.as_ptr(),
            trace_step_count: 1,
            ..variant(candidate.candidate_id, 1)
        };

        let replay_evidence = BuildVariantReplayEvidence::from_native_build_variant_and_candidate(
            native,
            replay_layout(),
            0,
            &candidate,
        )
        .expect("hold-aware replay evidence");
        let replay =
            BuildVariantMapper::to_replay_trace(replay_evidence.variant(), &replay_evidence)
                .expect("replay");

        assert_eq!(
            replay.solution_trace().steps()[0]
                .piece_decision()
                .hold_decision(),
            HoldDecision::SwapWithHold {
                incoming_piece: PieceKind::T,
                held_piece: PieceKind::O,
            }
        );
        assert!(replay
            .events()
            .iter()
            .any(|event| matches!(event, ReplayEvent::HoldSwap(_))));
    }
}
