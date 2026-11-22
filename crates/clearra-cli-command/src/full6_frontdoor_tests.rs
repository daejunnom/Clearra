//! Public CLI/App lowering for the finite Full6L exhaustive-search fixture.
//!
//! These tests compile the real typed source without starting Geometry or
//! BuildUp. They deliberately preserve the current Opening/Scenario boundary;
//! they do not broaden the native Full6L optimization gates or claim completion.

use clearra_app::{AppCommand, AppRequest, PcResultProjection};
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    PcCountPolicy, PcSolutionProbabilityPolicy, RequestedSearchBackend, SupplyWindowSize,
    WorkerPolicy,
};
use clearra_problem::{
    goal::SearchGoal, PcChanceEvidencePolicy, ProblemCompiler, SearchOutputPolicy, SearchProblem,
    SearchProblemKind, SearchProblemPreset,
};
use clearra_rules::profile::builtin_rules::srs_plus;
use clearra_supply::{
    hold::hold_policy::HoldPolicy, piece_source::PieceSourceKind, QueueObservationPolicy,
};

use crate::CliCommandParser;

const FULL6L_SCENARIO_COMMAND: &str =
    "clearra pc --lines 6 --board-mask 0x0 --height 6 --pieces 15 \
     --source-pieces 15 --hold empty --patterns P1P7P7 --count unique \
     --objective unique --rule srs-plus --queue-knowledge oracle --retained-traces 1 \
     --backend cpu --use-all-cpu-threads --no-tablebase --no-legal-board \
     --no-conditioned-reachability";

fn parse_app(command: &str) -> AppRequest {
    let tokens = command
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let from_argv = CliCommandParser::parse_tokens(&tokens)
        .expect("public argv parser")
        .to_app_request()
        .expect("typed App request");
    let from_text = CliCommandParser::parse(command)
        .expect("public text parser")
        .to_app_request()
        .expect("typed App request from command text");
    assert_eq!(
        from_argv, from_text,
        "native argv and GUI/Web text must agree"
    );
    assert!(from_argv.product_capability_contract().is_none());
    from_argv
}

fn assert_finite_full6l_contract(problem: &SearchProblem) {
    assert_eq!(problem.initial_board().width(), 10);
    assert_eq!(problem.initial_board().occupied_words(), [0; 4]);
    assert_eq!(problem.visible_height(), 6);
    assert_eq!(problem.compact_pc_geometry_height(), 6);
    assert_eq!(problem.piece_window().max_pieces(), 15);
    assert_eq!(problem.exact_pieces(), Some(15));
    assert_eq!(problem.search_goal(), &SearchGoal::ClearToEmpty);
    assert_eq!(problem.rule_profile_value(), srs_plus());
    assert_eq!(problem.count_policy(), PcCountPolicy::CountUnique);
    assert_eq!(problem.objective(), ObjectivePolicy::unique());
    assert_eq!(problem.output_policy(), SearchOutputPolicy::Trace);
    assert_eq!(
        problem.pc_chance_evidence_policy(),
        PcChanceEvidencePolicy::Disabled
    );
    assert!(!problem.build_replay_evidence_requested());
    assert_eq!(
        problem.solution_probability_policy(),
        PcSolutionProbabilityPolicy::Omit
    );
    assert_eq!(
        problem.queue_observation_policy(),
        QueueObservationPolicy::FullQueueOracle
    );
    assert!(problem.allowed_colored_solution_identities().is_none());
    assert_eq!(problem.continuation_policy().min_remaining_queue(), 0);

    let source = problem.piece_source();
    let universe = source
        .materialized_universe()
        .expect("factorized source owner");
    assert!(source.complete());
    assert!(universe.complete());
    assert_eq!(universe.truncation_reason(), None);
    assert_eq!(universe.pattern_count(), 177_811_200);
    assert_eq!(universe.total_possible_pattern_count(), 177_811_200);
    let layout = universe
        .complete_factorized_layout()
        .expect("actual complete factorized layout");
    assert_eq!(layout.full_sequence_len(), 15);
    assert_eq!(layout.visible_sequence_len(), 15);
    assert_eq!(layout.segment_count(), 3);
    for (segment, (start, draws, variants, suffix)) in layout.segments().zip([
        (0, 1, 7, 25_401_600),
        (1, 7, 5_040, 5_040),
        (8, 7, 5_040, 1),
    ]) {
        assert_eq!(segment.choices(), PieceKind::STANDARD_TETROMINOES);
        assert_eq!(segment.draw_start(), start);
        assert_eq!(segment.draw_count(), draws);
        assert_eq!(segment.variant_count(), variants);
        assert_eq!(segment.suffix_variant_count(), suffix);
    }
    assert_eq!(universe.sequence_len_at(0), 15);
    assert_eq!(universe.sequence_len_at(177_811_199), 15);

    let supply = problem.supply();
    assert_eq!(supply.source_sequence_length(), 15);
    assert!(supply.hold_enabled());
    assert_eq!(supply.hold_piece(), None);
    // The finite EOF permits releasing the retained piece, never drawing a
    // complement from a hypothetical next bag.
    assert!(supply.projects_unplaced_lookahead());
    assert!(!supply.projects_standard_bag_lookahead());
    let hold = problem.initial_hold();
    assert_eq!(hold.piece_source_id(), source.id());
    assert_eq!(
        hold.source_kind(),
        PieceSourceKind::MaterializedPatternUniverse
    );
    assert_eq!(hold.cursor(), 0);
    assert_eq!(hold.hold_piece(), None);
    assert!(hold.hold_empty());
    assert_eq!(hold.hold_policy(), HoldPolicy::Allowed);
    assert_eq!(hold.bag_epoch(), 0);
    assert_eq!(hold.bag_remainder_key(), 0);

    let backend = problem.backend_request();
    assert_eq!(backend.requested_backend(), RequestedSearchBackend::Cpu);
    assert_eq!(backend.worker_policy(), WorkerPolicy::Auto);
    assert!(!backend.allow_backend_fallback());
    assert!(!backend.cpu_warmup());
    assert!(!backend.tablebase_requested());
    assert!(!backend.precompute_build_dependencies());
    assert_eq!(backend.max_patterns(), 0);
    assert_eq!(backend.max_nodes(), 0);
    assert_eq!(backend.max_candidates(), 0);
    assert_eq!(backend.max_frontier_states(), 0);
    assert_eq!(backend.max_memory_mib(), None);
    assert_eq!(problem.budget().max_nodes(), 0);
}

#[test]
fn full6_frontdoor_explicit_scenario_matches_exhaustive_harness_contract() {
    let app = parse_app(FULL6L_SCENARIO_COMMAND);
    assert_eq!(
        app.command().exact_accelerator_policy(),
        Some((false, false))
    );
    let AppCommand::Scenario(command) = app.command() else {
        panic!("explicit field/height/pieces must retain Scenario PC identity");
    };
    assert_eq!(command.result_projection(), PcResultProjection::Standard);
    let query = command.query();
    assert!(query.remaining_queue().as_pattern_expression().is_some());
    assert_eq!(query.supply_window_size(), Some(SupplyWindowSize::new(15)));
    assert_eq!(query.retained_trace_limit(), 1);
    assert!(query.execution_policy().use_all_logical_processors());
    let problem =
        ProblemCompiler::compile_scenario_pc(query).expect("exact Scenario PC compilation");
    assert_eq!(problem.preset(), SearchProblemPreset::ScenarioPc);
    assert_eq!(problem.problem_kind(), SearchProblemKind::ScenarioPc);
    assert_eq!(problem.replay_trace_policy().retained_trace_limit(), 1);
    assert!(!problem.backend_request().exact_legal_board_enabled());
    assert!(!problem.backend_request().conditioned_reachability_enabled());
    assert_finite_full6l_contract(&problem);
}

#[test]
fn full6_frontdoor_default_opening_preserves_its_trace_and_accelerator_contract() {
    // The empty-field/empty-Hold GUI workspace emits this Opening form too.
    // Its compiled source/target also admits the native ordinary-PC prefix
    // filters, while the Opening identity, trace cap and accelerator requests
    // remain the public input contract rather than being rewritten for them.
    let app = parse_app("clearra pc --lines 6 --patterns P1P7P7");
    assert_eq!(app.command().exact_accelerator_policy(), Some((true, true)));
    let AppCommand::Pc(command) = app.command() else {
        panic!("ordinary pc --lines/--patterns remains an Opening PC request");
    };
    assert_eq!(command.result_projection(), PcResultProjection::Standard);
    assert!(command.query().queue().as_pattern_expression().is_some());
    assert!(!command
        .query()
        .execution_policy()
        .use_all_logical_processors());
    let problem =
        ProblemCompiler::compile_opening_pc(command.query()).expect("Opening PC compilation");
    assert_eq!(problem.preset(), SearchProblemPreset::OpeningPc);
    assert_eq!(problem.problem_kind(), SearchProblemKind::OpeningPc);
    assert_eq!(problem.replay_trace_policy().retained_trace_limit(), 64);
    assert!(problem.backend_request().exact_legal_board_enabled());
    assert!(problem.backend_request().conditioned_reachability_enabled());
    assert_finite_full6l_contract(&problem);
}

#[test]
fn full6_frontdoor_explicit_scenario_keeps_accelerators_until_explicitly_disabled() {
    let app = parse_app(
        "clearra pc --lines 6 --board-mask 0x0 --height 6 --pieces 15 \
         --source-pieces 15 --patterns P1P7P7",
    );
    let AppCommand::Scenario(command) = app.command() else {
        panic!("explicit field must select Scenario PC");
    };
    assert_eq!(command.result_projection(), PcResultProjection::Standard);
    let problem =
        ProblemCompiler::compile_scenario_pc(command.query()).expect("Scenario PC compilation");
    assert_eq!(problem.preset(), SearchProblemPreset::ScenarioPc);
    assert_eq!(problem.replay_trace_policy().retained_trace_limit(), 1);
    assert!(problem.backend_request().exact_legal_board_enabled());
    assert!(problem.backend_request().conditioned_reachability_enabled());
    assert_finite_full6l_contract(&problem);
}
