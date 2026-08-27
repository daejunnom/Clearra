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
#[test]
fn workspace_abi_mismatch_rejects_every_public_raw_c_entry_before_calling_c() {
    use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;

    const INJECTED_ABI_VERSION: i32 = 21;
    let expected_error = NativeCoreError::AbiMismatch {
        expected: CLEARRA_CORE_ABI_VERSION_EXPECTED,
        actual: INJECTED_ABI_VERSION,
    };
    let raw_c_entries = with_test_workspace_abi_override(INJECTED_ABI_VERSION, || {
        let problem = CBuildUpProblem::default();
        let cancellation = ExecutionCancellationToken::new();
        let mut workspace = NativeBuildUpWorkspace::new();

        assert_eq!(workspace.retained_bytes(), workspace.host_buffer_bytes());
        assert!(matches!(
            workspace.raw_handle(),
            Err(error) if error == expected_error
        ));
        assert_eq!(
            workspace.buildup_exists_with_cancellation(&problem, &cancellation),
            Err(expected_error)
        );
        assert!(matches!(
            workspace.verify_first_buildup_problem_with_cancellation(
                &problem,
                &cancellation
            ),
            Err(error) if error == expected_error
        ));
        assert!(matches!(
            workspace.enumerate_buildup_variants_with_cancellation(
                &problem,
                &CNativeBuildUpEnumerationLimits::default(),
                &cancellation,
            ),
            Err(error) if error == expected_error
        ));
        assert!(matches!(
            workspace.export_geometry_language_with_cancellation(&problem, &cancellation),
            Err(error) if error == expected_error
        ));
        assert!(matches!(
            workspace.export_geometry_language_v2_with_cancellation(
                &problem,
                crate::BuildUpGeometryTransitionMode::GeometryOnly,
                &cancellation,
            ),
            Err(error) if error == expected_error
        ));
    });

    assert_eq!(raw_c_entries, 0);
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
    use clearra_core_domain::{
        execution_cancellation::ExecutionCancellationToken, piece::piece_kind::PieceKind,
    };
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use crate::{
        problem::{C_PIECE_O, C_PIECE_S},
        BuildUpGeometryTransitionMode, CBuildUpProblemBuilder, CNativeBuildUpEnumerationLimits,
        CPackingCandidate, CPackingOperation, CoreCNative, NativeBuildUpWorkspace,
        C_BUILDUP_STATUS_OK,
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

        // Geometry v2 intentionally transports the semantic lock target, not
        // scoring evidence. Tie that target to the independently exported
        // first-success kick trace so the adapter boundary cannot drift in
        // rotation, coordinates, or mask.
        let mut workspace = NativeBuildUpWorkspace::new();
        let geometry = workspace
            .export_geometry_language_v2_with_cancellation(
                &buildup,
                BuildUpGeometryTransitionMode::GeometryOnly,
                &ExecutionCancellationToken::new(),
            )
            .expect("prepared geometry v2");
        let kick_target = geometry
            .nodes()
            .iter()
            .filter(|node| node.depth() == 1)
            .flat_map(|node| {
                let start = node.first_edge();
                let end = start + node.edge_count();
                geometry.edges()[start..end].iter()
            })
            .find(|edge| edge.operation_index() == 1)
            .expect("depth-one S target is exported");
        assert_eq!(kick_target.piece(), C_PIECE_S);
        assert_eq!(kick_target.rotation(), evidence.to_rotation);
        assert_eq!(i16::from(kick_target.x()), evidence.result_x);
        assert_eq!(i16::from(kick_target.adjusted_y()), evidence.result_y);
        assert_eq!(kick_target.target_mask(), s_mask);
    }
}

#[cfg(feature = "native-c-core")]
mod case_native_buildup_exports_prepared_geometry_language_v2 {
    use clearra_core_domain::{
        execution_cancellation::ExecutionCancellationToken, piece::piece_kind::PieceKind,
    };
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use crate::{
        problem::C_PIECE_O, BuildUpGeometryTransitionMode, CBuildUpProblemBuilder,
        CPackingCandidate, CPackingOperation, NativeBuildUpWorkspace,
    };

    #[test]
    fn native_buildup_exports_prepared_geometry_language_v2() {
        let first_o = 0x0c03u64;
        let second_o = 0x300cu64;
        let initial_mask = 0x0f_ffffu64 & !(first_o | second_o);
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, initial_mask),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O, PieceKind::O])),
            PieceWindow::new(2),
        )
        .with_exact_pieces(Some(2))
        .with_allow_hold(false);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
        let mut candidate = CPackingCandidate {
            candidate_id: 7,
            canonical_operation_set_id: 9,
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
            mask: first_o,
        };
        candidate.operations[1] = CPackingOperation {
            piece: C_PIECE_O,
            rotation: 0,
            x: 2,
            y: 0,
            operation_id: 4,
            required_deleted_row_mask: 0,
            mask: second_o,
        };
        let buildup = CBuildUpProblemBuilder::from_packing_candidate(&problem, &candidate, 0, 0)
            .expect("buildup");
        let mut workspace = NativeBuildUpWorkspace::new();
        let language = workspace
            .export_geometry_language_v2_with_cancellation(
                &buildup,
                BuildUpGeometryTransitionMode::GeometryOnly,
                &ExecutionCancellationToken::new(),
            )
            .expect("geometry v2");

        assert!(language.complete());
        assert_eq!(
            language.transition_mode(),
            BuildUpGeometryTransitionMode::GeometryOnly
        );
        assert_ne!(language.snapshot_id(), 0);
        let root = language.nodes()[language.root_node_index()];
        assert_eq!(root.board_mask(), initial_mask);
        assert_eq!(root.remaining_operations(), 3);
        assert_eq!(root.depth(), 0);
        assert!(root.edge_count() > 0);
        assert!(language.edges().iter().all(|edge| edge.target_mask() != 0));
        assert!(language
            .edges()
            .iter()
            .any(|edge| edge.cleared_lines() == 2 && edge.cleared_row_mask() == 3));
    }
}

#[cfg(feature = "native-c-core")]
mod case_finite_terminal_projection_matches_native_buildup {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use crate::{
        problem::{
            C_BUILDUP_TERMINAL_PROJECTION_DISABLED,
            C_BUILDUP_TERMINAL_PROJECTION_RELEASE_FINITE_HELD, C_PIECE_O,
        },
        CBuildUpProblemBuilder, CNativeBuildUpEnumerationLimits, CPackingCandidate,
        CPackingOperation, CoreCNative, C_BUILDUP_HOLD_BRANCH_RELEASE_HELD_AT_TERMINAL,
        C_BUILDUP_STATUS_OK,
    };

    #[test]
    fn occupied_initial_hold_is_released_once_without_mutating_problem_policy() {
        let first_o = 0x0c03u64;
        let second_o = 0x300cu64;
        let initial_mask = 0x0f_ffffu64 & !(first_o | second_o);
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, initial_mask),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(2),
        )
        .with_hold_piece(Some(PieceKind::O))
        .with_exact_pieces(Some(2));
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
        assert!(problem.supply().projects_unplaced_lookahead());

        let mut candidate = CPackingCandidate {
            candidate_id: 17,
            canonical_operation_set_id: 23,
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
            mask: first_o,
        };
        candidate.operations[1] = CPackingOperation {
            piece: C_PIECE_O,
            rotation: 0,
            x: 2,
            y: 0,
            operation_id: 4,
            required_deleted_row_mask: 0,
            mask: second_o,
        };

        let buildup = CBuildUpProblemBuilder::from_packing_candidate(&problem, &candidate, 0, 0)
            .expect("buildup");
        assert_eq!(
            buildup.terminal_projection_policy,
            C_BUILDUP_TERMINAL_PROJECTION_RELEASE_FINITE_HELD
        );
        let policy_before = (
            buildup.terminal_projection_policy_version,
            buildup.terminal_projection_policy,
            buildup.terminal_projection_reserved,
        );

        let outcome = CoreCNative::enumerate_buildup_variants(
            &buildup,
            &CNativeBuildUpEnumerationLimits::default(),
        )
        .expect("native enumerate");
        assert_eq!(outcome.status, C_BUILDUP_STATUS_OK);
        assert!(outcome.buffer.count > 0);
        assert_eq!(
            outcome.buffer.trace_step_storage[0][1].hold_branch_kind,
            C_BUILDUP_HOLD_BRANCH_RELEASE_HELD_AT_TERMINAL
        );
        assert_eq!(
            (
                buildup.terminal_projection_policy_version,
                buildup.terminal_projection_policy,
                buildup.terminal_projection_reserved,
            ),
            policy_before,
            "search state must not mutate the immutable problem policy"
        );

        let mut projection_off = buildup;
        projection_off.terminal_projection_policy = C_BUILDUP_TERMINAL_PROJECTION_DISABLED;
        let rejected = CoreCNative::enumerate_buildup_variants(
            &projection_off,
            &CNativeBuildUpEnumerationLimits::default(),
        )
        .expect("native projection-off enumerate");
        assert!(!rejected.accepted());
    }
}
