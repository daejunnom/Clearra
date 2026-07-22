use crate::backend::gpu_worker::{
    PackingBatchDescriptor, PackingBatchDescriptorBuilder, PackingBatchId, PackingBatchSource,
    PackingBatchSourceError, PackingBatchValidationError,
};
use clearra_core_domain::{pc::pc_target::PcTarget, piece::piece_kind::PieceKind};
use clearra_core_ffi::{
    gpu::CGpuPieceMultisetWindow,
    problem::{
        CBackendRequest, CBoardDescriptor, CPieceMultisetWindow, CPieceWindowDescriptor,
        CProblemBudget, CRuleProfileDescriptor, C_GPU_PIECE_SOURCE_FIXED_SEQUENCE, C_PIECE_I,
        C_PIECE_O, C_PIECE_S, C_PIECE_T, C_PIECE_Z,
    },
    supply::{CPieceSourceDescriptor, C_PIECE_SOURCE_FIXED_QUEUE},
    CPackingProblem, CPackingProblemBuilder,
};
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};
use clearra_problem::query::ScenarioQuery;
use clearra_problem::{ProblemCompiler, SearchProblem, SearchProblemPreset};
use clearra_supply::queue::fixed_sequence::FixedSequence;
use clearra_supply::queue::queue_parser::parse_fixed_sequence;

fn compact_piece_multiset_window() -> CPieceMultisetWindow {
    let mut window = CPieceMultisetWindow {
        total_count: 5,
        exact_count: 5,
        ..Default::default()
    };
    for piece in [C_PIECE_I, C_PIECE_O, C_PIECE_T, C_PIECE_S, C_PIECE_Z] {
        window.counts[usize::from(piece)] += 1;
    }
    window
}

fn gpu_piece_multiset_window() -> CGpuPieceMultisetWindow {
    let compact = compact_piece_multiset_window();
    CGpuPieceMultisetWindow {
        counts: compact.counts,
        total_count: compact.total_count,
        exact_count: compact.exact_count,
        reserved: compact.reserved,
    }
}

fn compact_piece_source() -> CPieceSourceDescriptor {
    CPieceSourceDescriptor {
        piece_source_id: 1,
        source_kind: C_PIECE_SOURCE_FIXED_QUEUE,
        provenance_id: 1,
        fixed_sequence_len: 5,
        piece_set_profile_id: 1,
        complete: 1,
        ..Default::default()
    }
}

fn compact_problem() -> CPackingProblem {
    CPackingProblem {
        problem_kind: CPackingProblem::OPENING_PC,
        board: CBoardDescriptor {
            width: 10,
            visible_height: 2,
            search_height: 2,
            initial_mask: 0,
            cell_count: 20,
            ..Default::default()
        },
        piece_window: CPieceWindowDescriptor {
            max_pieces: 5,
            exact_pieces: 5,
            has_exact_pieces: 1,
            ..Default::default()
        },
        piece_multiset_window: compact_piece_multiset_window(),
        piece_source: compact_piece_source(),
        rule: CRuleProfileDescriptor {
            rule_profile_id: 1,
            kick_profile_id: 3,
            ..Default::default()
        },
        budget: CProblemBudget {
            max_results: 64,
            ..Default::default()
        },
        backend: CBackendRequest {
            requested_backend: 6,
            reserved_flags: 0,
            ..Default::default()
        },
        ..Default::default()
    }
}

mod case_packing_batch_descriptor_rejects_zero_piece_count {
    use super::*;

    #[test]
    fn packing_batch_descriptor_rejects_zero_piece_count() {
        let result = PackingBatchDescriptor::new(
            PackingBatchId::new(1),
            10,
            2,
            2,
            None,
            0,
            5,
            0,
            0,
            C_GPU_PIECE_SOURCE_FIXED_SEQUENCE,
            1,
            gpu_piece_multiset_window(),
            11,
            1,
            3,
            64,
            17,
            1001,
            2001,
        );

        assert_eq!(result, Err(PackingBatchValidationError::ZeroPieceCount));
    }
}

mod case_packing_batch_descriptor_rejects_board_over_board64_limit {
    use super::*;

    #[test]
    fn packing_batch_descriptor_rejects_board_over_board64_limit() {
        let result = PackingBatchDescriptor::new(
            PackingBatchId::new(1),
            10,
            7,
            7,
            None,
            0,
            5,
            5,
            5,
            C_GPU_PIECE_SOURCE_FIXED_SEQUENCE,
            1,
            gpu_piece_multiset_window(),
            11,
            1,
            3,
            64,
            17,
            1001,
            2001,
        );

        assert_eq!(
            result,
            Err(PackingBatchValidationError::BoardExceedsBoard64Limit { cell_count: 70 })
        );
    }
}

mod case_packing_batch_descriptor_rejects_active_rows_over_board_height {
    use super::*;

    #[test]
    fn packing_batch_descriptor_rejects_active_rows_over_board_height() {
        let result = PackingBatchDescriptor::new(
            PackingBatchId::new(1),
            10,
            2,
            3,
            None,
            0,
            5,
            5,
            5,
            C_GPU_PIECE_SOURCE_FIXED_SEQUENCE,
            1,
            gpu_piece_multiset_window(),
            11,
            1,
            3,
            64,
            17,
            1001,
            2001,
        );

        assert_eq!(
            result,
            Err(
                PackingBatchValidationError::ActivePackingRowsExceedBoardHeight {
                    active_packing_rows: 3,
                    board_height: 2,
                }
            )
        );
    }
}

mod case_packing_batch_descriptor_rejects_clear_hint_over_board_height {
    use super::*;

    #[test]
    fn packing_batch_descriptor_rejects_clear_hint_over_board_height() {
        let result = PackingBatchDescriptor::new(
            PackingBatchId::new(1),
            10,
            2,
            2,
            Some(3),
            0,
            5,
            5,
            5,
            C_GPU_PIECE_SOURCE_FIXED_SEQUENCE,
            1,
            gpu_piece_multiset_window(),
            11,
            1,
            3,
            64,
            17,
            1001,
            2001,
        );

        assert_eq!(
            result,
            Err(
                PackingBatchValidationError::GoalClearLinesHintExceedsBoardHeight {
                    goal_clear_lines_hint: 3,
                    board_height: 2,
                }
            )
        );
    }
}

mod case_gpu_batch_descriptor_rejects_piece_count_exceeding_window {
    use super::*;

    #[test]
    fn gpu_batch_descriptor_rejects_piece_count_exceeding_window() {
        let result = PackingBatchDescriptor::new(
            PackingBatchId::new(1),
            10,
            2,
            2,
            None,
            0,
            4,
            5,
            5,
            C_GPU_PIECE_SOURCE_FIXED_SEQUENCE,
            1,
            gpu_piece_multiset_window(),
            11,
            1,
            3,
            64,
            17,
            1001,
            2001,
        );

        assert_eq!(
            result,
            Err(PackingBatchValidationError::PieceCountExceedsPieceWindow {
                piece_count: 5,
                piece_window: 4,
            })
        );
    }
}

mod case_gpu_batch_descriptor_rejects_mask_outside_active_packing_rows {
    use super::*;

    #[test]
    fn gpu_batch_descriptor_rejects_mask_outside_active_packing_rows() {
        let result = PackingBatchDescriptor::new(
            PackingBatchId::new(1),
            10,
            4,
            2,
            None,
            1u64 << 20,
            5,
            5,
            5,
            C_GPU_PIECE_SOURCE_FIXED_SEQUENCE,
            1,
            gpu_piece_multiset_window(),
            11,
            1,
            3,
            64,
            17,
            1001,
            2001,
        );

        assert_eq!(
            result,
            Err(PackingBatchValidationError::InitialBoardMaskOutsideActivePackingRows)
        );
    }
}

mod case_packing_batch_descriptor_preserves_pattern_universe_identity {
    use super::*;

    #[test]
    fn packing_batch_descriptor_preserves_pattern_universe_identity() {
        let descriptor = PackingBatchDescriptorBuilder::new()
            .with_batch_id(PackingBatchId::new(1))
            .from_compact_problem_with_identity(&compact_problem(), 1001, 2001)
            .expect("descriptor");

        assert_eq!(descriptor.pattern_universe_id, 1001);
        assert_eq!(descriptor.pattern_weight_model_id, 2001);
    }
}

mod case_gpu_batch_descriptor_has_piece_source_id {
    use super::*;

    #[test]
    fn gpu_batch_descriptor_has_piece_source_id() {
        let descriptor = PackingBatchDescriptorBuilder::new()
            .with_batch_id(PackingBatchId::new(1))
            .from_compact_problem_with_identity(&compact_problem(), 1001, 2001)
            .expect("descriptor");

        assert_eq!(descriptor.piece_source_id, 1);
    }
}

mod case_gpu_batch_descriptor_has_piece_multiset_window {
    use super::*;

    #[test]
    fn gpu_batch_descriptor_has_piece_multiset_window() {
        let descriptor = PackingBatchDescriptorBuilder::new()
            .with_batch_id(PackingBatchId::new(1))
            .from_compact_problem_with_identity(&compact_problem(), 1001, 2001)
            .expect("descriptor");

        assert_eq!(descriptor.piece_multiset_window.total_count, 5);
        assert_eq!(descriptor.piece_multiset_window.exact_count, 5);
        assert_eq!(
            descriptor.piece_multiset_window.counts[usize::from(C_PIECE_I)],
            1
        );
        assert_eq!(
            descriptor.piece_multiset_window.counts[usize::from(C_PIECE_O)],
            1
        );
        assert_eq!(
            descriptor.piece_multiset_window.counts[usize::from(C_PIECE_T)],
            1
        );
    }
}

mod case_gpu_batch_descriptor_preserves_frontier_budget_and_pattern_count {
    use super::*;

    #[test]
    fn gpu_batch_descriptor_preserves_frontier_budget_and_pattern_count() {
        let descriptor = PackingBatchDescriptorBuilder::new()
            .with_batch_id(PackingBatchId::new(1))
            .from_compact_problem_with_identity(&compact_problem(), 1001, 2001)
            .expect("descriptor");

        assert_eq!(descriptor.max_frontier_states, 2_048);
        assert_eq!(descriptor.pattern_count, 1);

        let c_descriptor = descriptor.to_c_descriptor_view().expect("C descriptor");
        assert_eq!(c_descriptor.max_frontier_states, 2_048);
        assert_eq!(c_descriptor.pattern_count, 1);
    }
}

mod case_packing_batch_descriptor_uses_piece_source_and_multiset_as_source_of_truth {
    use super::*;

    #[test]
    fn packing_batch_descriptor_uses_piece_source_and_multiset_as_source_of_truth() {
        let descriptor = PackingBatchDescriptor::new(
            PackingBatchId::new(1),
            10,
            2,
            2,
            None,
            0,
            5,
            5,
            5,
            C_GPU_PIECE_SOURCE_FIXED_SEQUENCE,
            77,
            gpu_piece_multiset_window(),
            11,
            1,
            3,
            64,
            17,
            1001,
            2001,
        )
        .expect("descriptor");

        let (piece_source_id, pattern_universe_id, pattern_weight_model_id, window) =
            descriptor.product_source_of_truth();

        assert_eq!(piece_source_id, 77);
        assert_eq!(pattern_universe_id, 1001);
        assert_eq!(pattern_weight_model_id, 2001);
        assert_eq!(window.counts[usize::from(C_PIECE_I)], 1);
    }
}

mod case_packing_batch_descriptor_preserves_rule_and_kick_profile_id {
    use super::*;

    #[test]
    fn packing_batch_descriptor_preserves_rule_and_kick_profile_id() {
        let descriptor = PackingBatchDescriptorBuilder::new()
            .with_batch_id(PackingBatchId::new(1))
            .from_compact_problem_with_identity(&compact_problem(), 1001, 2001)
            .expect("descriptor");

        assert_eq!(descriptor.rule_profile_id, 1);
        assert_eq!(descriptor.kick_profile_id, 3);
    }
}

mod case_packing_batch_descriptor_builder_uses_problem_budget {
    use super::*;

    #[test]
    fn packing_batch_descriptor_builder_uses_problem_budget() {
        let descriptor = PackingBatchDescriptorBuilder::new()
            .with_batch_id(PackingBatchId::new(1))
            .from_compact_problem_with_identity(&compact_problem(), 1001, 2001)
            .expect("descriptor");

        assert_eq!(descriptor.candidate_capacity, 64);
    }
}

mod case_packing_batch_descriptor_builder_from_search_problem_uses_problem_budget {
    use super::*;

    #[test]
    fn packing_batch_descriptor_builder_from_search_problem_uses_problem_budget() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0),
            PcQueueInput::fixed_sequence(parse_fixed_sequence("IOTSL").expect("queue")),
            PieceWindow::new(5),
        )
        .with_exact_pieces(Some(5));
        let problem = SearchProblem::new(
            SearchProblemPreset::ScenarioPc,
            ScenarioQuery::scenario_preset(query),
        )
        .expect("test problem materializes its supply");
        let compact = CPackingProblem {
            budget: CProblemBudget {
                max_results: 1,
                ..Default::default()
            },
            ..compact_problem()
        };

        let descriptor = PackingBatchDescriptorBuilder::new()
            .with_batch_id(PackingBatchId::new(9))
            .from_search_problem(&problem, &compact)
            .expect("descriptor");

        assert_eq!(
            descriptor.candidate_capacity,
            problem.budget().max_results() as u32
        );
        assert_ne!(descriptor.candidate_capacity, compact.budget.max_results);
        assert_ne!(descriptor.pattern_universe_id, 0);
        assert_ne!(descriptor.pattern_weight_model_id, 0);
    }
}

mod case_packing_batch_source_from_opening_2l_uses_exact_five_pieces {
    use super::*;

    #[test]
    fn packing_batch_source_from_opening_2l_uses_exact_five_pieces() {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines()).with_queue(
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::O,
                PieceKind::T,
                PieceKind::S,
                PieceKind::Z,
            ])),
        );
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");
        let compact = CPackingProblemBuilder::from_search_problem(&problem).expect("compact");

        let source = PackingBatchSource::from_search_problem(
            &problem,
            &compact,
            Some(PackingBatchId::new(21)),
            None,
            None,
        )
        .expect("source");

        assert_eq!(source.batch_id, PackingBatchId::new(21));
        assert_eq!(source.board_width, 10);
        assert_eq!(source.board_height, 2);
        assert_eq!(source.active_packing_rows, 2);
        assert_eq!(source.piece_window, 5);
        assert_eq!(source.piece_count, 5);
        assert_eq!(source.exact_piece_count, 5);
        assert_ne!(source.piece_source_id, 0);
        assert_eq!(source.piece_source_id, compact.piece_source.piece_source_id);
        assert_eq!(source.piece_multiset_window.total_count, 5);
        assert_eq!(
            source.piece_multiset_window.counts[usize::from(C_PIECE_I)],
            1
        );
        assert_ne!(source.pattern_universe_id, 0);
        assert_ne!(source.pattern_weight_model_id, 0);

        let descriptor = PackingBatchDescriptorBuilder::new()
            .from_source(source)
            .expect("descriptor");
        assert_eq!(descriptor.piece_count, 5);
        assert_eq!(descriptor.exact_piece_count, 5);
        assert_ne!(descriptor.piece_source_id, 0);
        assert_eq!(
            descriptor.piece_source_id,
            compact.piece_source.piece_source_id
        );
        assert_eq!(descriptor.piece_multiset_window.total_count, 5);
    }
}

mod case_packing_batch_source_from_scenario_4l_preserves_initial_mask {
    use super::*;

    #[test]
    fn packing_batch_source_from_scenario_4l_preserves_initial_mask() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::T])),
            PieceWindow::new(1),
        );
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
        let compact = CPackingProblemBuilder::from_search_problem(&problem).expect("compact");

        let source = PackingBatchSource::from_search_problem(
            &problem,
            &compact,
            Some(PackingBatchId::new(22)),
            None,
            None,
        )
        .expect("source");

        assert_eq!(compact.board.visible_height, 1);
        assert_eq!(source.initial_board_mask, 0x3f0);
        assert_eq!(source.board_width, 10);
        assert_eq!(source.board_height, 1);
        assert_eq!(source.active_packing_rows, 1);
        assert_eq!(source.piece_window, 1);
        assert_eq!(source.piece_count, 1);
        assert_eq!(source.piece_multiset_window.total_count, 1);
        assert_eq!(
            source.piece_multiset_window.counts[usize::from(C_PIECE_T)],
            1
        );
    }
}

mod case_same_multiset_different_piece_source_has_different_batch_identity {
    use super::*;

    #[test]
    fn same_multiset_different_piece_source_has_different_batch_identity() {
        let compact_a = compact_problem();
        let mut compact_b = compact_problem();
        compact_b.piece_source.piece_source_id = 2;

        let descriptor_a = PackingBatchDescriptorBuilder::new()
            .from_compact_problem_with_identity(&compact_a, 1001, 2001)
            .expect("descriptor A");
        let descriptor_b = PackingBatchDescriptorBuilder::new()
            .from_compact_problem_with_identity(&compact_b, 1001, 2001)
            .expect("descriptor B");

        assert_eq!(
            descriptor_a.piece_multiset_window,
            descriptor_b.piece_multiset_window
        );
        assert_ne!(descriptor_a.piece_source_id, descriptor_b.piece_source_id);
        assert_ne!(descriptor_a.batch_id, descriptor_b.batch_id);
    }
}

mod case_packing_batch_source_rejects_missing_piece_multiset_window {
    use super::*;

    #[test]
    fn packing_batch_source_rejects_missing_piece_multiset_window() {
        let mut compact = compact_problem();
        compact.piece_multiset_window.total_count = 2;

        let result = PackingBatchSource::from_compact_problem_with_identity(
            &compact,
            Some(PackingBatchId::new(23)),
            1001,
            2001,
            None,
        );

        assert_eq!(
            result,
            Err(PackingBatchSourceError::Validation(
                PackingBatchValidationError::MissingPieceMultisetWindow {
                    piece_count: 5,
                    stored_len: 2
                }
            ))
        );
    }
}
