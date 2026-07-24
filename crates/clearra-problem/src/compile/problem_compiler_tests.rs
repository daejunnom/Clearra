use clearra_core_domain::{
    board::board_size::BoardSize, pc::pc_target::PcTarget, piece::piece_kind::PieceKind,
};
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcCompletionGoal, PcContinuationToken, PcContinuationTokenCodec,
    PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow, SupplyWindowSize,
};
use clearra_supply::{piece_source::PieceSourceKind, queue::fixed_sequence::FixedSequence};

use super::*;
use crate::{
    compile::packing_problem_compiler::PackingProblemCompiler,
    query::{
        BuildProblemLimits, BuildQuery, BuildTemplateBridge, SetupHoldPolicy, SetupQueueInput,
    },
    search_problem::{ExactTargetPolicy, SearchProblemKind},
};

mod case_opening_2l_compiles_to_search_problem {
    use super::*;

    #[test]
    fn opening_2l_compiles_to_search_problem() {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines());
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("2L problem");

        assert_eq!(problem.problem_kind(), SearchProblemKind::OpeningPc);
        assert!(problem.problem_id().as_str().starts_with("opening-pc:"));
        let occupancy = problem.initial_occupancy().expect("occupancy");
        assert_eq!(occupancy.mask, 0);
        assert_eq!(occupancy.height, 2);
        assert_eq!(problem.board_profile().id().as_str(), "standard-10");
        assert_eq!(problem.piece_window().max_pieces(), 5);
        assert_eq!(problem.exact_pieces(), Some(5));
        assert_eq!(problem.goal(), PcCompletionGoal::ClearToEmpty);
        assert_eq!(problem.search_goal().as_str(), "clear-to-empty");
        assert_eq!(problem.labels(), &["2L".to_owned()]);
        assert_eq!(
            problem.exact_target_policy(),
            ExactTargetPolicy::LabelOnly {
                target: PcTarget::two_lines()
            }
        );
        assert!(!problem.exact_target_policy().is_core_success_condition());
    }
}

mod case_opening_4l_compiles_to_search_problem {
    use super::*;

    #[test]
    fn opening_4l_compiles_to_search_problem() {
        let query = OpeningPcSearchQuery::new(PcTarget::four_lines());
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("4L problem");

        assert_eq!(problem.problem_kind(), SearchProblemKind::OpeningPc);
        assert_eq!(problem.initial_occupancy().expect("occupancy").height, 4);
        assert_eq!(problem.piece_window().max_pieces(), 10);
        assert_eq!(problem.exact_pieces(), Some(10));
        assert_eq!(problem.goal(), PcCompletionGoal::ClearToEmpty);
        assert_eq!(problem.labels(), &["2L".to_owned(), "4L".to_owned()]);
    }
}

mod case_opening_6l_compiles_to_search_problem {
    use super::*;

    #[test]
    fn opening_6l_compiles_to_search_problem() {
        let query = OpeningPcSearchQuery::new(PcTarget::six_lines());
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("6L problem");

        assert_eq!(problem.problem_kind(), SearchProblemKind::OpeningPc);
        assert_eq!(problem.initial_occupancy().expect("occupancy").height, 6);
        assert_eq!(problem.piece_window().max_pieces(), 15);
        assert_eq!(problem.exact_pieces(), Some(15));
        assert_eq!(problem.goal(), PcCompletionGoal::ClearToEmpty);
        assert_eq!(
            problem.labels(),
            &["2L".to_owned(), "4L".to_owned(), "6L".to_owned()]
        );
    }
}

mod case_opening_preset_compiles_to_clear_to_empty_problem {
    use super::*;

    #[test]
    fn opening_preset_compiles_to_clear_to_empty_problem() {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines()).with_queue(
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        );
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");

        assert_eq!(problem.preset().as_str(), "opening-pc");
        assert_eq!(problem.initial_board().width(), 10);
        assert_eq!(problem.initial_board().visible_height(), 2);
        assert_eq!(problem.visible_height(), 2);
        assert_eq!(problem.search_height(), 20);
        assert_eq!(problem.initial_board().occupied_mask(), 0);
        assert_eq!(problem.piece_window().max_pieces(), 5);
        assert_eq!(problem.exact_pieces(), Some(5));
        assert_eq!(problem.goal(), PcCompletionGoal::ClearToEmpty);
        assert_eq!(problem.labels(), &["2L".to_owned()]);
        assert_eq!(problem.supply().queue_mode(), "fixed");
        assert!(problem.supply().hold_enabled());
        assert_eq!(
            problem.rule_profile().spawn_profile().id().as_str(),
            "standard-10-spawn"
        );
        assert_eq!(
            problem
                .checkpoint_schedule()
                .expect("opening schedule")
                .partition_labels(),
            vec!["2"]
        );
        assert_eq!(problem.chain_class().as_str(), "opening-2l");
        assert_eq!(problem.core_query().remaining_queue().mode(), "fixed");
    }
}

mod case_opening_four_and_six_line_presets_are_exact_piece_windows {
    use super::*;

    #[test]
    fn opening_four_and_six_line_presets_are_exact_piece_windows() {
        let four =
            ProblemCompiler::compile_opening_pc(&OpeningPcSearchQuery::new(PcTarget::four_lines()))
                .expect("4L problem");
        let six =
            ProblemCompiler::compile_opening_pc(&OpeningPcSearchQuery::new(PcTarget::six_lines()))
                .expect("6L problem");

        assert_eq!(four.piece_window().max_pieces(), 10);
        assert_eq!(four.exact_pieces(), Some(10));
        assert_eq!(four.labels(), &["2L".to_owned(), "4L".to_owned()]);
        assert_eq!(six.piece_window().max_pieces(), 15);
        assert_eq!(six.exact_pieces(), Some(15));
        assert_eq!(
            six.labels(),
            &["2L".to_owned(), "4L".to_owned(), "6L".to_owned()]
        );
    }
}

mod case_scenario_compiles_to_search_problem {
    use super::*;

    #[test]
    fn scenario_compiles_to_search_problem() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I, PieceKind::O])),
            PieceWindow::new(2),
        )
        .with_hold_piece(Some(PieceKind::T))
        .with_exact_pieces(Some(2));
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("scenario problem");

        assert_eq!(problem.problem_kind(), SearchProblemKind::ScenarioPc);
        assert_eq!(problem.initial_occupancy().expect("occupancy").mask, 0x3f0);
        assert_eq!(problem.piece_source().kind(), PieceSourceKind::FixedQueue);
        assert_eq!(problem.initial_hold().hold_piece(), Some(PieceKind::T));
        assert!(problem.supply().hold_enabled());
        assert_eq!(problem.piece_window().max_pieces(), 2);
        assert_eq!(problem.exact_pieces(), Some(2));
        assert_eq!(problem.goal(), PcCompletionGoal::ClearToEmpty);
        assert_eq!(problem.exact_target_policy(), ExactTargetPolicy::None);
    }
}

mod case_occupied_initial_hold_projects_terminal_standard_bag_lookahead {
    use super::*;

    #[test]
    fn occupied_initial_hold_projects_terminal_standard_bag_lookahead() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0x80787),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(8),
        )
        .with_hold_piece(Some(PieceKind::S))
        .with_supply_window_size(SupplyWindowSize::new(7))
        .with_exact_pieces(Some(8));

        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("scenario problem");

        assert_eq!(problem.initial_hold().hold_piece(), Some(PieceKind::S));
        assert_eq!(problem.supply().source_sequence_length(), 7);
        assert!(
            problem.supply().projects_unplaced_lookahead(),
            "the next bag piece replaces the terminal hold but is never placed"
        );
    }
}

mod case_scenario_pc_compiles_to_same_search_problem_type {
    use super::*;

    #[test]
    fn scenario_pc_compiles_to_same_search_problem_type() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0),
            PcQueueInput::default(),
            PieceWindow::new(1),
        );
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");

        assert_eq!(problem.preset().as_str(), "scenario-pc");
        assert_eq!(problem.core_query(), &query);
        assert_eq!(problem.goal(), PcCompletionGoal::ClearToEmpty);
        assert!(problem.labels().is_empty());
        assert!(problem.checkpoint_schedule().is_none());
        assert_eq!(problem.chain_class().as_str(), "scenario");
        assert_eq!(problem.continuation_policy().min_remaining_queue(), 0);
    }
}

mod case_scenario_preset_compiles_to_clear_to_empty_problem {
    use super::*;

    #[test]
    fn scenario_preset_compiles_to_clear_to_empty_problem() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I, PieceKind::O])),
            PieceWindow::new(2),
        )
        .with_hold_piece(Some(PieceKind::T))
        .with_exact_pieces(Some(2));
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");

        assert_eq!(problem.preset(), SearchProblemPreset::ScenarioPc);
        assert_eq!(problem.initial_board(), query.initial_board());
        assert_eq!(problem.supply().hold_piece(), Some(PieceKind::T));
        assert_eq!(problem.piece_window().max_pieces(), 2);
        assert_eq!(problem.exact_pieces(), Some(2));
        assert_eq!(problem.goal(), PcCompletionGoal::ClearToEmpty);
        assert!(problem.scenario().exact_target_policy().is_none());
    }
}

mod case_setup_post_pc_compiles_to_scenario_search_problem {
    use super::*;

    #[test]
    fn setup_post_pc_compiles_to_scenario_search_problem() {
        let query = SetupSearchQuery::default()
            .with_queue(SetupQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
            ])))
            .with_hold_policy(SetupHoldPolicy::EnabledWithPiece(PieceKind::T));
        let problem = ProblemCompiler::compile_setup(&query).expect("setup-post-pc problem");

        assert_eq!(problem.problem_kind(), SearchProblemKind::SetupPostPc);
        assert_eq!(problem.scenario().source().as_str(), "setup-preset");
        assert!(problem.setup_query().is_some());
        assert_eq!(problem.goal(), PcCompletionGoal::ClearToEmpty);
        assert_eq!(problem.initial_hold().hold_piece(), Some(PieceKind::T));
        assert_eq!(problem.piece_window().max_pieces(), 7);
    }
}

mod case_setup_query_compiles_to_search_problem {
    use super::*;

    #[test]
    fn setup_query_compiles_to_search_problem() {
        let query = SetupSearchQuery::default()
            .with_queue(SetupQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
            ])))
            .with_hold_policy(SetupHoldPolicy::EnabledWithPiece(PieceKind::T));
        let problem = ProblemCompiler::compile_setup(&query).expect("setup problem");

        assert_eq!(problem.preset().as_str(), "setup");
        assert!(problem.setup_query().is_some());
        assert_eq!(problem.initial_board().width(), 10);
        assert_eq!(problem.visible_height(), 2);
        assert_eq!(problem.search_height(), 20);
        assert_eq!(problem.piece_window().max_pieces(), 7);
        assert_eq!(problem.supply().hold_piece(), Some(PieceKind::T));
        assert_eq!(problem.budget().max_results(), query.limits().max_results());
    }
}

mod case_build_coverage_bridge_compiles_to_build_problem {
    use super::*;

    #[test]
    fn build_coverage_bridge_compiles_to_build_problem() {
        let query = BuildQuery::coverage_bridge(
            BuildTemplateBridge::new("template-a", BoardSize::new(10, 4).expect("board"), 3),
            16,
            BuildProblemLimits::new(12, 16),
        );
        let problem = ProblemCompiler::compile_build(&query).expect("build problem");

        assert_eq!(problem.preset().as_str(), "build");
        assert!(problem.build_query().is_some());
        assert_eq!(problem.initial_board().visible_height(), 4);
        assert_eq!(problem.search_height(), 4);
        assert_eq!(problem.piece_window().max_pieces(), 3);
        assert_eq!(problem.budget().max_results(), 12);
        assert_eq!(problem.budget().max_patterns(), 16);
    }
}

mod case_continue_token_compiles_to_search_problem {
    use super::*;

    #[test]
    fn continue_token_compiles_to_search_problem() {
        let opening_query = OpeningPcSearchQuery::new(PcTarget::two_lines()).with_queue(
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])),
        );
        let token = PcContinuationTokenCodec::encode_opening_continuation(
            &opening_query,
            None,
            &[
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ],
        );
        let decoded = PcContinuationTokenCodec::parse(&token).expect("continuation token");
        let problem =
            ProblemCompiler::compile_continuation_token(&decoded).expect("continuation problem");

        assert_eq!(problem.preset(), SearchProblemPreset::OpeningPc);
        assert_eq!(problem.goal(), PcCompletionGoal::ClearToEmpty);
        assert_eq!(problem.piece_window().max_pieces(), 5);
        assert_eq!(problem.exact_pieces(), Some(5));

        let scenario_query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        );
        let scenario_token = PcContinuationToken::Scenario(scenario_query.clone());
        let scenario_problem = ProblemCompiler::compile_continuation_token(&scenario_token)
            .expect("scenario continuation problem");

        assert_eq!(scenario_problem.preset(), SearchProblemPreset::ScenarioPc);
        assert_eq!(scenario_problem.core_query(), &scenario_query);
        assert_eq!(scenario_problem.goal(), PcCompletionGoal::ClearToEmpty);
    }
}

mod case_pc_target_remains_label_not_core_success_condition {
    use super::*;

    #[test]
    fn pc_target_remains_label_not_core_success_condition() {
        let problem =
            ProblemCompiler::compile_opening_pc(&OpeningPcSearchQuery::new(PcTarget::six_lines()))
                .expect("6L problem");
        let packing = PackingProblemCompiler::compile(&problem).expect("packing spec");

        assert_eq!(problem.goal(), PcCompletionGoal::ClearToEmpty);
        assert_eq!(problem.search_goal().as_str(), "clear-to-empty");
        assert_eq!(
            problem.scenario().exact_target_policy(),
            Some(PcTarget::six_lines())
        );
        assert_eq!(
            problem.labels(),
            &["2L".to_owned(), "4L".to_owned(), "6L".to_owned()]
        );
        assert_eq!(problem.piece_window().max_pieces(), 15);
        assert_eq!(problem.exact_pieces(), Some(15));
        assert_eq!(packing.max_pieces(), 15);
    }
}
