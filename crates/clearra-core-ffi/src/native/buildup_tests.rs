use super::*;

#[test]
fn buildup_enumeration_limits_preserve_variant_budget() {
    let limits = CNativeBuildUpEnumerationLimits {
        max_variants: 17,
        preserve_hold_branches: 1,
        prefer_highest_t_spin_trace: 0,
        reserved: [0; 6],
    };

    assert_eq!(limits.max_variants, 17);
    assert_eq!(limits.preserve_hold_branches, 1);
    assert_eq!(C_BUILDUP_STATUS_CAPACITY_EXCEEDED, 14);
    assert_eq!(C_BUILDUP_STATUS_ENUMERATION_TRUNCATED, 16);
}

#[cfg(feature = "native-c-core")]
mod case_count_zero_solution_report_is_complete {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use crate::{
        problem::C_PIECE_O, CBuildUpProblemBuilder, CPackingCandidate, CPackingOperation,
        CoreCNative, C_BUILDUP_STATUS_HOLD_DISABLED_IMPOSSIBLE,
    };

    use super::*;

    #[test]
    fn count_zero_solution_report_is_complete() {
        let o_mask = 0x0c03;
        let initial_mask = 0x0f_ffff ^ o_mask;
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, initial_mask),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I, PieceKind::T])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_allow_hold(false);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
        let mut candidate = CPackingCandidate {
            candidate_id: 1,
            canonical_operation_set_id: 1,
            operation_count: 1,
            ..Default::default()
        };
        candidate.operations[0] = CPackingOperation {
            piece: C_PIECE_O,
            rotation: 0,
            x: 0,
            y: 0,
            operation_id: 1,
            required_deleted_row_mask: 0,
            mask: o_mask,
        };
        let buildup = CBuildUpProblemBuilder::from_packing_candidate(&problem, &candidate, 0, 0)
            .expect("buildup");
        let outcome =
            CoreCNative::count_buildup_variants(&buildup, &CNativeBuildUpCountLimits::default())
                .expect("native count");

        assert_eq!(outcome.status, C_BUILDUP_STATUS_OK);
        assert_eq!(outcome.report.search_complete, 1);
        assert_eq!(outcome.report.count_complete, 1);
        assert_eq!(outcome.report.total_variant_count, 0);
        assert_eq!(outcome.report.solution_exists, 0);
        assert_eq!(
            outcome.report.no_variant_reason,
            C_BUILDUP_STATUS_HOLD_DISABLED_IMPOSSIBLE as u32
        );
        assert_eq!(outcome.report.truncation_reason, C_BUILDUP_STATUS_OK as u32);
    }
}

#[cfg(feature = "native-c-core")]
mod case_native_buildup_exports_actual_first_success_kick_evidence {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use crate::{
        problem::{C_PIECE_O, C_PIECE_S},
        CBuildUpProblemBuilder, CNativeBuildUpEnumerationLimits, CPackingCandidate,
        CPackingOperation, CoreCNative, C_BUILDUP_STATUS_OK,
    };

    #[test]
    fn native_buildup_exports_actual_first_success_kick_evidence() {
        let o_mask = 0x0c03u64;
        let s_mask = 0x600cu64;
        let initial_mask = 0x0f_93f0u64;
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, initial_mask),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O, PieceKind::S])),
            PieceWindow::new(2),
        )
        .with_exact_pieces(Some(2))
        .with_allow_hold(false);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
        let mut candidate = CPackingCandidate {
            candidate_id: 1,
            canonical_operation_set_id: 1,
            operation_count: 2,
            ..Default::default()
        };
        candidate.operations[0] = CPackingOperation {
            piece: C_PIECE_O,
            rotation: 0,
            x: 0,
            y: 0,
            operation_id: 4,
            required_deleted_row_mask: 0,
            mask: o_mask,
        };
        candidate.operations[1] = CPackingOperation {
            piece: C_PIECE_S,
            rotation: 0,
            x: 2,
            y: 0,
            operation_id: 12,
            required_deleted_row_mask: 0,
            mask: s_mask,
        };
        let buildup = CBuildUpProblemBuilder::from_packing_candidate(&problem, &candidate, 0, 0)
            .expect("buildup");
        let outcome = CoreCNative::enumerate_buildup_variants(
            &buildup,
            &CNativeBuildUpEnumerationLimits::default(),
        )
        .expect("native enumerate");

        assert_eq!(outcome.status, C_BUILDUP_STATUS_OK);
        assert_eq!(outcome.buffer.count, 1);
        assert_eq!(outcome.buffer.variants[0].kick_evidence_count, 1);
        let evidence = outcome.buffer.kick_evidence_storage[0][0];
        assert_eq!(evidence.has_kick_evidence, 1);
        assert_eq!(evidence.from_rotation, 2);
        assert_eq!(evidence.to_rotation, 0);
        assert_eq!(evidence.rotation_request, 3);
        assert_eq!(evidence.kick_index, 1);
        assert_eq!((evidence.kick_dx, evidence.kick_dy), (0, -1));
        assert_eq!(evidence.first_success_confirmed, 1);
        assert_eq!((evidence.predecessor_x, evidence.predecessor_y), (2, 0));
        assert_eq!((evidence.result_x, evidence.result_y), (2, 0));
        assert_eq!(
            outcome.buffer.trace_step_storage[0][0].kick_evidence_index,
            u8::MAX
        );
        assert_eq!(
            outcome.buffer.trace_step_storage[0][1].kick_evidence_index,
            0
        );
        assert_eq!(outcome.buffer.variants[0].trace_completeness_flags, 0);
    }
}
