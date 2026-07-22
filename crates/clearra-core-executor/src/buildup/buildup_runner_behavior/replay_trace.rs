use super::*;

#[cfg(feature = "native-c-core")]
mod case_buildup_runner_exports_sample_replay_trace_for_scoring_post_processing {
    use super::*;

    #[test]
    fn buildup_runner_exports_sample_replay_trace_for_scoring_post_processing() {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])))
            .with_hold_policy(PcHoldPolicy::Disabled);
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");
        let packing = PackingRunner::run(&problem).expect("packing");

        let buildup = BuildUpRunner::run(&problem, &packing).expect("buildup");
        let replay = buildup.sample_replay_trace().expect("sample replay trace");

        assert_eq!(replay.trace_steps(), 5);
        assert!(replay
            .events()
            .iter()
            .any(|event| matches!(event, ReplayEvent::Placement(_))));
        assert!(replay
            .events()
            .iter()
            .any(|event| matches!(event, ReplayEvent::LineClear(_))));
        assert!(replay
            .events()
            .iter()
            .any(|event| matches!(event, ReplayEvent::Drop(_))));
        assert!(replay
            .events()
            .iter()
            .any(|event| matches!(event, ReplayEvent::SpinBasis(_))));
    }
}

#[cfg(feature = "native-c-core")]

mod case_native_buildup_result_uses_canonical_trace_key {
    use super::*;

    #[test]
    fn native_buildup_result_uses_canonical_trace_key() {
        let problem = ProblemCompiler::compile_scenario_pc(
            &PcScenarioQuery::new(
                PcScenarioBoard::standard_10(2, 0x3f0),
                PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
                PieceWindow::new(1),
            )
            .with_exact_pieces(Some(1))
            .with_retained_trace_limit(1),
        )
        .expect("problem");
        let candidate = CPackingCandidate {
            candidate_id: 7,
            operation_count: 1,
            operations: [clearra_core_ffi::CPackingOperation {
                piece: clearra_core_ffi::problem::C_PIECE_I,
                rotation: 0,
                x: 0,
                y: 0,
                operation_id: 0,
                required_deleted_row_mask: 0,
                mask: 0x0f,
            }; clearra_core_ffi::packing_problem::C_PACKING_MAX_OPERATIONS],
            ..Default::default()
        };
        let variant = CNativeBuildVariantView {
            candidate_id: 7,
            build_variant_id: 1,
            canonical_operation_set_id: 7,
            operation_set_hash: 0x1234,
            coverage_pattern_id: 0,
            placed_count: 1,
            queue_cursor: 1,
            cleared_lines: 1,
            ..Default::default()
        };

        let material = trace_material_for_execution(
            &problem,
            &[candidate],
            &[owned_build_variant(variant)],
            ScenarioPackingWitness::solved(1, 1, 1),
        );

        assert_eq!(
            material.trace_key.as_deref(),
            Some("bvk2:0000000000000007:00000000:0000000000000001")
        );
        assert_eq!(material.retained_trace_count, 1);
        let replay = material.sample_replay_trace.expect("native replay trace");
        assert_eq!(
            replay.variant_id(),
            "bvk2:0000000000000007:00000000:0000000000000001"
        );
        assert!(replay.representative());
        assert!(replay.sample());
    }
}

mod case_build_variant_trace_key_is_canonical {
    use super::*;

    #[test]
    fn build_variant_trace_key_is_canonical() {
        let variant = CNativeBuildVariantView {
            candidate_id: 7,
            build_variant_id: 1,
            canonical_operation_set_id: 7,
            operation_set_hash: 0x1234,
            ..Default::default()
        };

        let key = trace_key_for_build_variant(&owned_build_variant(variant), 7);

        assert_eq!(key, "bvk2:0000000000000007:00000000:0000000000000001");
    }
}

mod case_zero_operation_set_trace_key_falls_back_to_candidate_id {
    use super::*;

    #[test]
    fn zero_operation_set_trace_key_falls_back_to_candidate_id() {
        let variant = CNativeBuildVariantView::default();

        let key = trace_key_for_build_variant(&owned_build_variant(variant), 42);

        assert_eq!(key, "bvk2:000000000000002a:00000000:0000000000000000");
    }
}
