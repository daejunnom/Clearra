use clearra_app::AppCommand;
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_forward_search::{ForwardSearchMode, ForwardSpinCategory};
use clearra_pc_graph::request::SupplyWindowSize;
use clearra_scoring::profile::SpinProfileId;
use clearra_spin_structure_search::{MinimalityPolicy, SpinLineRequirement, SpinStructureMode};
use clearra_supply::QueueObservationPolicy;

use super::*;

#[test]
fn wasm_command_compiles_to_app_request() {
    let request = WebCommandParser::parse("clearra pc --lines 2 --backend cpu")
        .expect("web command")
        .to_app_request()
        .expect("AppRequest");

    match request.command() {
        AppCommand::Pc(command) => {
            assert_eq!(command.query().target().lines(), 2);
            assert_eq!(command.query().execution_policy().backend().as_str(), "cpu");
        }
        _ => panic!("expected AppCommand::Pc"),
    }
}

#[test]
fn bare_web_pc_preserves_the_web_surface_defaults_through_app_projection() {
    let parsed = WebCommandParser::parse("clearra pc").expect("bare web pc command");
    assert_eq!(parsed.lines(), 4);
    assert_eq!(parsed.backend().as_str(), "auto");
    assert!(parsed.allow_backend_fallback());

    let request = parsed.to_app_request().expect("bare web pc AppRequest");
    let AppCommand::Pc(command) = request.command() else {
        panic!("expected AppCommand::Pc");
    };
    assert_eq!(command.query().target().lines(), 4);
    assert_eq!(
        command.query().objective().kind(),
        clearra_core_domain::objective::objective_kind::ObjectiveKind::Unique
    );
    assert_eq!(
        command.query().execution_policy().backend().as_str(),
        "auto"
    );
    assert!(command.query().execution_policy().allow_backend_fallback());
}

#[test]
fn percent_command_compiles_to_coverage_summary_request() {
    let request = WebCommandParser::parse(
        "clearra percent I --fixed --min-len 1 --max-patterns 8 --failed-count 3",
    )
    .expect("web percent command")
    .to_app_request()
    .expect("percent AppRequest");

    let AppCommand::Percent(command) = request.command() else {
        panic!("expected AppCommand::Percent");
    };
    assert_eq!(command.failed_pattern_limit(), 3);
    let query = command.query().expect("scenario percent query");
    assert_eq!(query.initial_board().visible_height(), 1);
    assert_eq!(query.piece_window().max_pieces(), 1);
    assert_eq!(query.exact_pieces(), Some(1));
    assert_eq!(query.supply_window_size(), Some(SupplyWindowSize::new(1)));
    assert_eq!(query.execution_policy().max_patterns(), 8);
}

#[test]
fn percent_command_keeps_queue_materialization_separate_from_one_piece_geometry() {
    let request = WebCommandParser::parse("clearra percent IOT --bag-aligned --min-len 3")
        .expect("web percent command")
        .to_app_request()
        .expect("percent AppRequest");

    let AppCommand::Percent(command) = request.command() else {
        panic!("expected AppCommand::Percent");
    };
    let query = command.query().expect("scenario percent query");
    assert_eq!(query.initial_board().visible_height(), 1);
    assert_eq!(query.piece_window().max_pieces(), 3);
    assert_eq!(query.exact_pieces(), Some(1));
    assert_eq!(query.supply_window_size(), Some(SupplyWindowSize::new(3)));
}

#[test]
fn percent_observed_minimum_len_below_queue_length_still_compiles() {
    let request = WebCommandParser::parse("clearra percent IOT --observed --min-len 1")
        .expect("web percent command")
        .to_app_request()
        .expect("percent AppRequest");

    let AppCommand::Percent(command) = request.command() else {
        panic!("expected AppCommand::Percent");
    };
    let query = command.query().expect("scenario percent query");
    assert_eq!(query.piece_window().max_pieces(), 1);
    assert_eq!(query.supply_window_size(), Some(SupplyWindowSize::new(3)));
    clearra_problem::ProblemCompiler::compile_scenario_percent(query)
        .expect("observed percent problem");
}

#[test]
fn failed_queue_command_reuses_the_reverse_scenario_contract() {
    let request = WebCommandParser::parse(
        "clearra failed-queue --lines 4 --board-mask 0 --height 4 --pieces 10 \
         --patterns P7P3 --hold empty --rule srs-plus --backend cpu --failed-count 17",
    )
    .expect("failed-queue command")
    .to_app_request()
    .expect("failed-queue AppRequest");

    let AppCommand::Percent(command) = request.command() else {
        panic!("expected AppCommand::Percent");
    };
    assert!(command.is_failed_queue());
    assert_eq!(command.failed_pattern_limit(), 17);
    let query = command.query().expect("scenario failed-queue query");
    assert_eq!(query.initial_board().visible_height(), 4);
    assert_eq!(query.piece_window().max_pieces(), 10);
    assert_eq!(
        query.objective().kind(),
        clearra_core_domain::objective::objective_kind::ObjectiveKind::All
    );
    assert!(!query.objective().score().requested());
}

#[test]
fn failed_queue_keeps_b2b_constraints_without_enabling_scoring() {
    let request = WebCommandParser::parse(
        "clearra failed-queue --lines 2 --patterns P5 --preserve-b2b \
         --spin-profile all-mini-plus --backend cpu --failed-count 4",
    )
    .expect("failed-queue command")
    .to_app_request()
    .expect("failed-queue AppRequest");

    let AppCommand::Percent(command) = request.command() else {
        panic!("expected AppCommand::Percent");
    };
    let query = command.opening_query().expect("opening failed-queue query");
    let objective = query.objective();
    assert!(!objective.score().requested());
    assert!(objective.execution_constraints().preserves_back_to_back());
    assert_eq!(
        objective.execution_constraints().spin_profile().as_str(),
        "all-mini-plus"
    );
}

#[test]
fn failed_queue_rejects_scoring_options() {
    for option in [
        "--score",
        "--score-profile guideline",
        "--initial-b2b 1",
        "--solution-probabilities",
        "--spin-profile all-mini-plus",
    ] {
        let command = format!("clearra failed-queue --lines 2 --patterns P5 {option}");
        let error = WebCommandParser::parse(&command).expect_err(option);
        assert_eq!(error.code(), WebCommandErrorCode::InvalidValue);
    }
}

#[test]
fn tiling_only_preserves_hold_supply_projection() {
    let request = WebCommandParser::parse(
        "clearra pc --lines 2 --board-mask 0 --height 2 --pieces 5 \
         --queue IOTZ --hold S --tiling-only --backend cpu",
    )
    .expect("tiling-only command")
    .to_app_request()
    .expect("tiling-only AppRequest");

    let AppCommand::Scenario(command) = request.command() else {
        panic!("expected AppCommand::Scenario");
    };
    assert_eq!(
        command.query().objective().kind(),
        clearra_core_domain::objective::objective_kind::ObjectiveKind::Tiling
    );
    assert_eq!(command.query().hold_state().piece(), Some(PieceKind::S));
    assert!(command.query().allow_hold());
}

#[test]
fn tiling_only_rejects_buildup_and_probability_options() {
    for option in [
        "--rule srs-plus",
        "--score",
        "--preserve-b2b",
        "--solution-probabilities",
        "--queue-knowledge visible-7",
        "--tablebase",
        "--build-dependency-dag",
    ] {
        let command = format!("clearra pc --lines 2 --queue IIOOO --tiling-only {option}");
        let error = WebCommandParser::parse(&command).expect_err(option);
        assert_eq!(error.code(), WebCommandErrorCode::InvalidValue);
    }
}

#[test]
fn browser_tablebase_is_opt_in_for_pc_and_setup() {
    let default_pc = WebCommandParser::parse("clearra pc --lines 4 --backend cpu")
        .expect("default PC command")
        .to_app_request()
        .expect("default PC request");
    let AppCommand::Pc(command) = default_pc.command() else {
        panic!("expected AppCommand::Pc");
    };
    assert!(!command.query().execution_policy().tablebase_requested());

    let enabled_pc = WebCommandParser::parse("clearra pc --lines 4 --backend cpu --tb")
        .expect("TB PC command")
        .to_app_request()
        .expect("TB PC request");
    let AppCommand::Pc(command) = enabled_pc.command() else {
        panic!("expected AppCommand::Pc");
    };
    assert!(command.query().execution_policy().tablebase_requested());

    let enabled_setup = WebCommandParser::parse("clearra setup --remaining IOTSZJL --tablebase")
        .expect("TB setup command")
        .to_app_request()
        .expect("TB setup request");
    let AppCommand::Setup(command) = enabled_setup.command() else {
        panic!("expected AppCommand::Setup");
    };
    assert!(command.query().tablebase_requested());

    let disabled_setup =
        WebCommandParser::parse("clearra setup --remaining IOTSZJL --no-tablebase")
            .expect("disabled TB setup command")
            .to_app_request()
            .expect("disabled TB setup request");
    let AppCommand::Setup(command) = disabled_setup.command() else {
        panic!("expected AppCommand::Setup");
    };
    assert!(!command.query().tablebase_requested());
}

#[test]
fn browser_build_dependency_dag_is_opt_in_for_pc() {
    let default_pc = WebCommandParser::parse("clearra pc --lines 4 --backend cpu")
        .expect("default PC command")
        .to_app_request()
        .expect("default PC request");
    let AppCommand::Pc(command) = default_pc.command() else {
        panic!("expected AppCommand::Pc");
    };
    assert!(!command
        .query()
        .execution_policy()
        .precompute_build_dependencies());

    let enabled_pc =
        WebCommandParser::parse("clearra pc --lines 4 --backend cpu --build-dependency-dag")
            .expect("dependency DAG PC command")
            .to_app_request()
            .expect("dependency DAG PC request");
    let AppCommand::Pc(command) = enabled_pc.command() else {
        panic!("expected AppCommand::Pc");
    };
    assert!(command
        .query()
        .execution_policy()
        .precompute_build_dependencies());

    let disabled_pc =
        WebCommandParser::parse("clearra pc --lines 4 --backend cpu --no-build-dependency-dag")
            .expect("disabled dependency DAG PC command")
            .to_app_request()
            .expect("disabled dependency DAG PC request");
    let AppCommand::Pc(command) = disabled_pc.command() else {
        panic!("expected AppCommand::Pc");
    };
    assert!(!command
        .query()
        .execution_policy()
        .precompute_build_dependencies());
}

#[test]
fn pc_queue_knowledge_defaults_to_full_future_oracle() {
    let request = WebCommandParser::parse("clearra pc --lines 4 --patterns P7P4")
        .expect("web command")
        .to_app_request()
        .expect("AppRequest");
    let AppCommand::Pc(command) = request.command() else {
        panic!("expected AppCommand::Pc");
    };

    assert_eq!(
        command.query().queue_observation_policy(),
        QueueObservationPolicy::FullQueueOracle
    );
}

#[test]
fn pc_command_accepts_visible_seven_queue_knowledge() {
    let request =
        WebCommandParser::parse("clearra pc --lines 4 --patterns P7P4 --queue-knowledge visible-7")
            .expect("web command")
            .to_app_request()
            .expect("AppRequest");
    let AppCommand::Pc(command) = request.command() else {
        panic!("expected AppCommand::Pc");
    };

    assert_eq!(
        command.query().queue_observation_policy(),
        QueueObservationPolicy::VisibleSeven
    );
}

#[test]
fn scenario_pc_command_accepts_visible_seven_queue_knowledge() {
    let request = WebCommandParser::parse(
        "clearra pc --lines 4 --patterns P7P4 --board-mask 0 \
         --height 4 --pieces 10 --queue-knowledge visible-7",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");
    let AppCommand::Scenario(command) = request.command() else {
        panic!("expected AppCommand::Scenario");
    };

    assert_eq!(
        command.query().queue_observation_policy(),
        QueueObservationPolicy::VisibleSeven
    );
}

#[test]
fn pc_and_scenario_commands_reject_visible_seven_minimum_cover() {
    for command in [
        "clearra pc --lines 4 --patterns P7P4 --queue-knowledge visible-7 --objective minimum-cover",
        "clearra pc --lines 4 --patterns P7P4 --board-mask 0 --height 4 --pieces 10 --queue-knowledge visible-7 --objective minimum-cover",
    ] {
        let error = WebCommandParser::parse(command)
            .expect_err("visible-7 minimum-cover must fail before AppRequest construction");
        assert_eq!(error.code(), WebCommandErrorCode::InvalidValue);
        assert!(error
            .message()
            .contains("visible-seven-minimum-cover-unsupported"));
    }
}

#[test]
fn opening_pc_command_preserves_observed_source_piece_count() {
    let request = WebCommandParser::parse("clearra pc --lines 4 --backend cpu --source-pieces 10")
        .expect("web command")
        .to_app_request()
        .expect("AppRequest");

    let AppCommand::Pc(command) = request.command() else {
        panic!("expected AppCommand::Pc");
    };
    assert_eq!(
        command.query().supply_window_size(),
        Some(SupplyWindowSize::new(10))
    );
}

#[test]
fn setup_command_compiles_residue_hold_and_cycle_boundary_policy() {
    let request = WebCommandParser::parse("clearra setup --remaining IOTS")
        .expect("setup command")
        .to_app_request()
        .expect("AppRequest");

    let AppCommand::Setup(command) = request.command() else {
        panic!("expected AppCommand::Setup");
    };
    assert_eq!(command.query().residue().remaining_count(), 4);
    assert_eq!(command.query().residue().cycle(), Some(2));
    assert_eq!(command.query().residue().duplicate_piece(), None);
    assert_eq!(
        command.query().hold_policy(),
        clearra_problem::SetupHoldPolicy::EnabledEmpty
    );
    assert_eq!(
        command.query().candidate_priority(),
        clearra_problem::SetupCandidatePriority::All
    );

    let cycle_boundary =
        WebCommandParser::parse("clearra setup --remaining IOT --allow-post-cycle-borrow")
            .expect("cycle-seven setup command")
            .to_app_request()
            .expect("AppRequest");
    let AppCommand::Setup(command) = cycle_boundary.command() else {
        panic!("expected AppCommand::Setup");
    };
    assert_eq!(command.query().residue().cycle(), Some(7));
    assert_eq!(
        command.query().cycle_reset_borrow_policy(),
        clearra_problem::SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse
    );
}

#[test]
fn setup_command_accepts_the_complete_cycle_one_pattern_domain() {
    let request = WebCommandParser::parse(
        "clearra setup-finder --remaining IOTSZJL --priority pc --setup-length longer --max-setup-pieces 5",
    )
    .expect("cycle one setup command")
    .to_app_request()
    .expect("cycle one AppRequest");
    let AppCommand::Setup(command) = request.command() else {
        panic!("expected AppCommand::Setup");
    };

    let conditions = clearra_problem::compile_setup_search_conditions(command.query())
        .expect("complete cycle one condition");

    assert_eq!(conditions.len(), 1);
    assert_eq!(conditions[0].pattern_expression(), "[IOTSZJL]!P4");
    assert_eq!(
        conditions[0]
            .problem()
            .piece_source()
            .materialized_universe()
            .expect("factorized setup universe")
            .pattern_count(),
        4_233_600
    );
}

#[test]
fn setup_command_accepts_visible_seven_queue_knowledge() {
    let request =
        WebCommandParser::parse("clearra setup --remaining IOTS --queue-knowledge visible-7")
            .expect("setup command")
            .to_app_request()
            .expect("AppRequest");
    let AppCommand::Setup(command) = request.command() else {
        panic!("expected AppCommand::Setup");
    };

    assert_eq!(
        command.query().queue_observation_policy(),
        QueueObservationPolicy::VisibleSeven
    );
}

#[test]
fn queue_knowledge_rejects_unknown_values() {
    for command in [
        "clearra pc --lines 4 --queue-knowledge clairvoyant",
        "clearra setup --remaining IOTS --queue-knowledge clairvoyant",
    ] {
        let error = WebCommandParser::parse(command).expect_err("invalid policy must fail");
        assert_eq!(error.code(), WebCommandErrorCode::InvalidValue);
    }
}

#[test]
fn setup_command_preserves_single_remaining_piece_as_a_guaranteed_prefix() {
    let request = WebCommandParser::parse("clearra setup --remaining I")
        .expect("setup command")
        .to_app_request()
        .expect("AppRequest");
    let AppCommand::Setup(command) = request.command() else {
        panic!("expected AppCommand::Setup");
    };
    let conditions =
        clearra_problem::compile_setup_search_conditions(command.query()).expect("conditions");

    assert_eq!(command.query().residue().pieces(), &[PieceKind::I]);
    assert_eq!(command.query().residue().cycle(), Some(3));
    assert_eq!(conditions.len(), 1);
    assert_eq!(conditions[0].pattern_expression(), "IP7P3");
    assert_eq!(conditions[0].queue_remainder(), &[PieceKind::I]);
}

#[test]
fn setup_command_preserves_candidate_priority() {
    for (keyword, expected) in [
        ("all", clearra_problem::SetupCandidatePriority::All),
        (
            "build",
            clearra_problem::SetupCandidatePriority::BuildProbabilityFirst,
        ),
        (
            "pc",
            clearra_problem::SetupCandidatePriority::PcProbabilityFirst,
        ),
    ] {
        let request = WebCommandParser::parse(&format!(
            "clearra setup --remaining IOTS --priority {keyword}"
        ))
        .expect("setup command")
        .to_app_request()
        .expect("AppRequest");
        let AppCommand::Setup(command) = request.command() else {
            panic!("expected AppCommand::Setup");
        };
        assert_eq!(command.query().candidate_priority(), expected);
    }
}

#[test]
fn setup_command_preserves_setup_length_preference() {
    for (keyword, expected) in [
        ("auto", clearra_problem::SetupLengthPreference::Auto),
        ("longer", clearra_problem::SetupLengthPreference::Longer),
        ("shorter", clearra_problem::SetupLengthPreference::Shorter),
    ] {
        let request = WebCommandParser::parse(&format!(
            "clearra setup --remaining IOTS --setup-length {keyword}"
        ))
        .expect("setup command")
        .to_app_request()
        .expect("AppRequest");
        let AppCommand::Setup(command) = request.command() else {
            panic!("expected AppCommand::Setup");
        };
        assert_eq!(command.query().length_preference(), expected);
    }
}

#[test]
fn setup_command_preserves_selected_kick_table() {
    let request = WebCommandParser::parse("clearra setup --remaining IOTS --rule srs-x")
        .expect("setup command")
        .to_app_request()
        .expect("AppRequest");
    let AppCommand::Setup(command) = request.command() else {
        panic!("expected AppCommand::Setup");
    };

    assert_eq!(
        command.query().rule().id(),
        clearra_rules::profile::rule_profile::RuleProfileId::SrsX
    );
}

#[test]
fn setup_command_preserves_selected_jstris_180_kick_table() {
    let request = WebCommandParser::parse("clearra setup --remaining IOTS --rule jstris-180")
        .expect("setup command")
        .to_app_request()
        .expect("AppRequest");
    let AppCommand::Setup(command) = request.command() else {
        panic!("expected AppCommand::Setup");
    };

    assert_eq!(
        command.query().rule().id(),
        clearra_rules::profile::rule_profile::RuleProfileId::Jstris180
    );
}

#[test]
fn setup_command_preserves_the_complete_pc_piece_limit() {
    let request = WebCommandParser::parse("clearra setup --remaining IOTS --max-setup-pieces 10")
        .expect("setup command")
        .to_app_request()
        .expect("AppRequest");
    let AppCommand::Setup(command) = request.command() else {
        panic!("expected AppCommand::Setup");
    };

    assert_eq!(command.query().max_setup_pieces(), 10);
}

#[test]
fn setup_command_separates_residue_and_observed_qb_pieces() {
    let request = WebCommandParser::parse("clearra setup --remaining TI --mode qb --qb OS")
        .expect("QB setup command")
        .to_app_request()
        .expect("AppRequest");
    let AppCommand::Setup(command) = request.command() else {
        panic!("expected AppCommand::Setup");
    };

    assert_eq!(
        command.query().search_mode(),
        clearra_problem::SetupSearchMode::QueueBased
    );
    assert_eq!(
        command.query().residue().pieces(),
        &[PieceKind::T, PieceKind::I]
    );
    assert_eq!(
        command
            .query()
            .queue()
            .as_fixed_sequence()
            .expect("fixed QB queue")
            .pieces(),
        &[PieceKind::O, PieceKind::S]
    );
    assert!(command.query().next_cycle_remaining_pieces().is_none());
}

#[test]
fn setup_command_accepts_next_cycle_inventory_in_oracle_mode() {
    let request =
        WebCommandParser::parse("clearra setup --remaining TI --next-cycle-remaining OOSITZ")
            .expect("oracle setup command")
            .to_app_request()
            .expect("AppRequest");
    let AppCommand::Setup(command) = request.command() else {
        panic!("expected AppCommand::Setup");
    };

    assert_eq!(
        command.query().search_mode(),
        clearra_problem::SetupSearchMode::ShapeOracle
    );
    assert_eq!(
        command.query().next_cycle_remaining_pieces(),
        Some(
            &[
                PieceKind::O,
                PieceKind::O,
                PieceKind::S,
                PieceKind::I,
                PieceKind::T,
                PieceKind::Z,
            ][..]
        )
    );
}

#[test]
fn setup_command_combines_observed_qb_and_next_cycle_inventory() {
    let request = WebCommandParser::parse(
        "clearra setup --remaining TI --qb OS --next-cycle-remaining OOSITZ",
    )
    .expect("combined setup command")
    .to_app_request()
    .expect("AppRequest");
    let AppCommand::Setup(command) = request.command() else {
        panic!("expected AppCommand::Setup");
    };

    assert_eq!(
        command
            .query()
            .queue()
            .as_fixed_sequence()
            .expect("observed QB queue")
            .pieces(),
        &[PieceKind::O, PieceKind::S]
    );
    assert_eq!(
        command
            .query()
            .next_cycle_remaining_pieces()
            .map(|pieces| pieces.len()),
        Some(6)
    );
}

#[test]
fn setup_command_preserves_exact_path_detail_selection() {
    let request = WebCommandParser::parse(
        "clearra setup --remaining TI --mode qb --qb OS \
         --paths-for setup-00080719e6-0003-000000000000000000000000000001 \
         --condition hold-empty",
    )
    .expect("path detail command")
    .to_app_request()
    .expect("AppRequest");
    let AppCommand::Setup(command) = request.command() else {
        panic!("expected AppCommand::Setup");
    };
    let detail = command.query().path_detail().expect("path detail");

    assert_eq!(detail.board_mask(), 0x0008_0719_e6);
    assert_eq!(detail.deleted_rows(), 3);
    assert_eq!(detail.placement_rows(), 1);
    assert_eq!(detail.condition_id(), "hold-empty");
}

#[test]
fn setup_command_requires_both_path_detail_options() {
    let error = WebCommandParser::parse(
        "clearra setup --remaining TI \
         --paths-for setup-00080719e6-0000-000000000000000000000000000001",
    )
    .expect_err("missing condition must fail");

    assert_eq!(error.code(), WebCommandErrorCode::MissingValue);
}

#[test]
fn setup_command_requires_observed_pieces_in_queue_based_mode() {
    let error = WebCommandParser::parse("clearra setup --remaining TI --mode qb")
        .expect_err("QB mode without observations must fail at the parser boundary");

    assert_eq!(error.code(), WebCommandErrorCode::InvalidValue);
}

#[test]
fn setup_command_separates_one_duplicate_as_automatic_initial_hold_without_changing_cycle() {
    let request = WebCommandParser::parse("clearra setup --remaining IOTSS")
        .expect("setup command")
        .to_app_request()
        .expect("AppRequest");

    let AppCommand::Setup(command) = request.command() else {
        panic!("expected AppCommand::Setup");
    };
    let conditions = clearra_problem::compile_setup_search_conditions(command.query())
        .expect("setup conditions");

    assert_eq!(command.query().residue().remaining_count(), 5);
    assert_eq!(command.query().residue().cycle(), Some(4));
    assert_eq!(conditions.len(), 1);
    assert_eq!(conditions[0].cycle(), 4);
    assert_eq!(conditions[0].initial_hold(), Some(PieceKind::S));
    assert_eq!(
        conditions[0].queue_remainder(),
        &[PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S]
    );
}

#[test]
fn setup_command_rejects_oracle_mode_with_observed_qb_pieces() {
    for command in [
        "clearra setup --remaining TI --mode oracle --qb OS",
        "clearra setup --remaining TI --qb OS --mode oracle",
    ] {
        let error =
            WebCommandParser::parse(command).expect_err("oracle and QB observations must conflict");
        assert_eq!(error.code(), WebCommandErrorCode::InvalidValue);
    }
}

#[test]
fn setup_mode_qb_borrow_and_next_cycle_pairs_fail_closed_at_the_parser_boundary() {
    let invalid = [
        "clearra setup --remaining TI --mode qb",
        "clearra setup --mode qb --remaining TI",
        "clearra setup --remaining TI --mode oracle --qb OS",
        "clearra setup --remaining TI --qb OS --mode oracle",
        "clearra setup --remaining TI --allow-post-cycle-borrow",
        "clearra setup --allow-post-cycle-borrow --remaining TI",
        "clearra setup --remaining TI --next-cycle-remaining O",
        "clearra setup --next-cycle-remaining O --remaining TI",
        "clearra setup --remaining IOT --allow-post-cycle-borrow --next-cycle-remaining I",
        "clearra setup --remaining IOT --next-cycle-remaining I --allow-post-cycle-borrow",
    ];
    for command in invalid {
        let error = WebCommandParser::parse(command).expect_err(command);
        assert_eq!(error.code(), WebCommandErrorCode::InvalidValue, "{command}");
    }

    for command in [
        "clearra setup --remaining TI --mode qb --qb OS --next-cycle-remaining OOSITZ",
        "clearra setup --remaining TI --next-cycle-remaining OOSITZ --qb OS --mode qb",
        "clearra setup --remaining IOT --allow-post-cycle-borrow --next-cycle-remaining IOTSZJL",
        "clearra setup --next-cycle-remaining IOTSZJL --allow-post-cycle-borrow --remaining IOT",
    ] {
        WebCommandParser::parse(command)
            .unwrap_or_else(|error| panic!("failed to parse '{command}': {error:?}"))
            .to_app_request()
            .unwrap_or_else(|error| panic!("failed to project '{command}': {error:?}"));
    }
}

#[test]
fn setup_mode_qb_borrow_and_next_cycle_cartesian_matrix_reaches_the_real_parser() {
    let residues = [("TI", "OOSITZ", "O", false), ("IOT", "IOTSZJL", "I", true)];
    let modes = [None, Some("oracle"), Some("qb")];
    let observed_groups = [None, Some("OS")];
    let next_cycle_groups = [None, Some(true), Some(false)];

    for (remaining, valid_next, invalid_next, borrow_allowed) in residues {
        for mode in modes {
            for observed in observed_groups {
                for borrow in [false, true] {
                    for next_cycle_valid in next_cycle_groups {
                        let mut options = Vec::new();
                        if let Some(mode) = mode {
                            options.push(format!("--mode {mode}"));
                        }
                        if let Some(observed) = observed {
                            options.push(format!("--qb {observed}"));
                        }
                        if borrow {
                            options.push("--allow-post-cycle-borrow".to_owned());
                        }
                        if let Some(valid) = next_cycle_valid {
                            options.push(format!(
                                "--next-cycle-remaining {}",
                                if valid { valid_next } else { invalid_next }
                            ));
                        }

                        let expected_valid = !matches!((mode, observed), (Some("qb"), None))
                            && !matches!((mode, observed), (Some("oracle"), Some(_)))
                            && (!borrow || borrow_allowed)
                            && next_cycle_valid != Some(false);
                        for reverse in [false, true] {
                            let mut ordered = options.clone();
                            if reverse {
                                ordered.reverse();
                            }
                            let command = if reverse {
                                format!(
                                    "clearra setup {} --remaining {remaining}",
                                    ordered.join(" ")
                                )
                            } else {
                                format!(
                                    "clearra setup --remaining {remaining} {}",
                                    ordered.join(" ")
                                )
                            };
                            match WebCommandParser::parse(&command) {
                                Ok(parsed) if expected_valid => {
                                    parsed.to_app_request().unwrap_or_else(|error| {
                                        panic!("failed to project '{command}': {error:?}")
                                    });
                                }
                                Err(error) if !expected_valid => {
                                    assert_eq!(
                                        error.code(),
                                        WebCommandErrorCode::InvalidValue,
                                        "{command}"
                                    );
                                }
                                Ok(_) => panic!("expected parser rejection: {command}"),
                                Err(error) => {
                                    panic!("unexpected parser rejection for '{command}': {error:?}")
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn setup_command_rejects_unknown_product_options() {
    let error = WebCommandParser::parse("clearra setup --remaining IOTSZJL --online-policy")
        .expect_err("unfinished online policy must not enter the product contract");

    assert_eq!(error.code(), WebCommandErrorCode::UnsupportedCommand);
}

#[test]
fn setup_command_keeps_initial_hold_on_the_cli_only_surface() {
    let error = WebCommandParser::parse("clearra setup --remaining SIOS --initial-hold S")
        .expect_err("product web command must not expose the CLI-only initial hold");

    assert_eq!(error.code(), WebCommandErrorCode::UnsupportedCommand);
}

#[test]
fn setup_browser_command_defaults_to_reserved_multithread_worker_count() {
    let request = WebCommandParser::parse_with_worker_limit("clearra setup --remaining IOTS", 12)
        .expect("setup command")
        .to_app_request()
        .expect("AppRequest");

    assert_eq!(request.resource_budget().workers(), 11);
}

#[test]
fn pc_back_to_back_preservation_is_an_execution_constraint_not_a_score_request() {
    let request = WebCommandParser::parse(
        "clearra pc --lines 2 --queue IOTSL --preserve-b2b --spin-profile all-mini-plus",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::Pc(command) = request.command() else {
        panic!("expected AppCommand::Pc");
    };
    let objective = command.query().objective();
    assert!(!objective.score().requested());
    assert!(objective.execution_constraints().preserves_back_to_back());
    assert_eq!(
        objective.execution_constraints().spin_profile().as_str(),
        "all-mini-plus"
    );
}

#[test]
fn pc_score_table_selection_is_independent_from_rotation_rule() {
    let request = WebCommandParser::parse(
        "clearra pc --lines 2 --queue IOTSL --score --score-profile jstris-ultra --rule srs-plus",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::Pc(command) = request.command() else {
        panic!("expected AppCommand::Pc");
    };
    assert_eq!(
        command.query().objective().score().profile().as_str(),
        "jstris-ultra"
    );
    assert_eq!(command.query().rule().id().as_str(), "srs-plus");
}

#[test]
fn build_probability_back_to_back_preservation_reaches_the_core_query() {
    let request = WebCommandParser::parse(
        "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror --preserve-b2b --spin-profile t-spins-plus",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::BuildProbability(command) = request.command() else {
        panic!("expected AppCommand::BuildProbability");
    };
    let objective = command.query().core_query().objective();
    assert!(!objective.score().requested());
    assert!(objective.execution_constraints().preserves_back_to_back());
    assert_eq!(
        objective.execution_constraints().spin_profile().as_str(),
        "t-spins-plus"
    );
}

#[test]
fn build_probability_dependency_dag_is_opt_in_and_reaches_the_core_policy() {
    let enabled = WebCommandParser::parse(
        "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror --build-dependency-dag",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");
    let AppCommand::BuildProbability(enabled) = enabled.command() else {
        panic!("expected AppCommand::BuildProbability");
    };
    assert!(enabled
        .query()
        .core_query()
        .execution_policy()
        .precompute_build_dependencies());

    let disabled = WebCommandParser::parse(
        "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");
    let AppCommand::BuildProbability(disabled) = disabled.command() else {
        panic!("expected AppCommand::BuildProbability");
    };
    assert!(!disabled
        .query()
        .core_query()
        .execution_policy()
        .precompute_build_dependencies());
}

#[test]
fn finesse_search_accepts_the_discord_fixed_queue_contract() {
    let base = "0".repeat(60);
    let target = format!("{}f", "0".repeat(59));
    let request = WebCommandParser::parse(&format!(
        "finesse search --base-mask {base} --target-mask {target} --height 1 \
         --queue I --hold empty --pattern-knowledge both --rule srs-x"
    ))
    .expect("Discord finesse search command")
    .to_app_request()
    .expect("finesse search AppRequest");

    let AppCommand::BuildProbability(command) = request.command() else {
        panic!("expected AppCommand::BuildProbability");
    };
    let query = command.query();
    assert_eq!(query.field().base_words(), [0; 4]);
    assert_eq!(query.field().target_words(), [0xf, 0, 0, 0]);
    assert_eq!(query.field().height(), 1);
    assert!(!query.field().includes_horizontal_mirror());
    assert_eq!(
        query.finesse_metric(),
        clearra_problem::FinesseMetric::Inputs
    );
    assert_eq!(
        query.finesse_pattern_knowledge(),
        clearra_problem::FinessePatternKnowledge::Both
    );
    assert!(query.core_query().allow_hold());
    assert_eq!(query.core_query().hold_state().piece(), None);
    assert_eq!(
        query
            .core_query()
            .remaining_queue()
            .as_fixed_sequence()
            .expect("fixed queue")
            .pieces(),
        &[PieceKind::I]
    );
    assert_eq!(query.core_query().rule().id().as_str(), "srs-x");
}

#[test]
fn finesse_search_accepts_the_discord_pattern_policy_contract() {
    let base = "0".repeat(60);
    let target = format!("{}f", "0".repeat(59));
    let request = WebCommandParser::parse(&format!(
        "finesse search --base-mask {base} --target-mask {target} --height 1 \
         --patterns [TI]! --no-hold --pattern-knowledge visible-7"
    ))
    .expect("Discord pattern finesse search")
    .to_app_request()
    .expect("pattern finesse AppRequest");

    let AppCommand::BuildProbability(command) = request.command() else {
        panic!("expected AppCommand::BuildProbability");
    };
    let query = command.query();
    assert_eq!(
        query.finesse_metric(),
        clearra_problem::FinesseMetric::Inputs
    );
    assert_eq!(
        query.finesse_pattern_knowledge(),
        clearra_problem::FinessePatternKnowledge::VisibleSeven
    );
    assert!(!query.core_query().allow_hold());
    assert_eq!(
        query.core_query().remaining_queue().mode(),
        "materialized-pattern-expression"
    );
}

#[test]
fn finesse_search_preserves_the_declared_spawn_height() {
    let request = WebCommandParser::parse(
        "finesse search --base-mask 0 --target-mask 0xf --height 8 \
         --queue I --no-hold --pattern-knowledge oracle",
    )
    .expect("finesse search command")
    .to_app_request()
    .expect("finesse search AppRequest");

    let AppCommand::BuildProbability(command) = request.command() else {
        panic!("expected AppCommand::BuildProbability");
    };
    assert_eq!(command.query().field().height(), 8);
}

#[test]
fn finesse_score_accepts_the_discord_ctk3_canonical_contract() {
    // Canonical output of the Discord CTK3 document decoder. The source
    // document itself must never cross this parser boundary.
    let initial = format!("{}1", "0".repeat(59));
    let tokens = [
        "finesse".to_owned(),
        "score".to_owned(),
        "--initial-mask".to_owned(),
        initial,
        "--height".to_owned(),
        "4".to_owned(),
        "--placements".to_owned(),
        "T:spawn:3:1,I:right:4:0".to_owned(),
        "--patterns".to_owned(),
        "[TI]!".to_owned(),
        "--hold".to_owned(),
        "empty".to_owned(),
        "--pattern-knowledge".to_owned(),
        "oracle".to_owned(),
    ];
    let request = WebCommandParser::parse_tokens(&tokens)
        .expect("Discord CTK3 finesse score command")
        .to_app_request()
        .expect("CTK3 finesse score AppRequest");

    let AppCommand::BuildProbability(command) = request.command() else {
        panic!("expected AppCommand::BuildProbability");
    };
    let query = command.query();
    assert_eq!(query.field().base_words(), [1, 0, 0, 0]);
    assert_eq!(query.field().target_words(), [0; 4]);
    assert_eq!(query.field().height(), 4);
    assert_eq!(
        query.finesse_pattern_knowledge(),
        clearra_problem::FinessePatternKnowledge::Oracle
    );
    assert!(query.core_query().allow_hold());
    assert_eq!(query.core_query().exact_pieces(), Some(2));
    let placements = query
        .finesse_score()
        .expect("typed finesse score")
        .placements();
    assert_eq!(placements.len(), 2);
    assert_eq!(placements[0].piece(), PieceKind::T);
    assert_eq!(
        placements[0].rotation(),
        clearra_core_domain::piece::rotation::RotationState::Zero
    );
    assert_eq!((placements[0].x(), placements[0].y()), (3, 1));
    assert_eq!(placements[1].piece(), PieceKind::I);
    assert_eq!(
        placements[1].rotation(),
        clearra_core_domain::piece::rotation::RotationState::Right
    );
    assert_eq!((placements[1].x(), placements[1].y()), (4, 0));
}

#[test]
fn finesse_score_command_text_accepts_multiple_comma_separated_placements() {
    let request = WebCommandParser::parse(
        "finesse score --initial-mask 0 --height 4 \
         --placements O:spawn:0:0,O:spawn:2:0 \
         --queue OO --no-hold --pattern-knowledge both",
    )
    .expect("multi-placement finesse score command text")
    .to_app_request()
    .expect("multi-placement finesse score AppRequest");

    let AppCommand::BuildProbability(command) = request.command() else {
        panic!("expected AppCommand::BuildProbability");
    };
    let placements = command
        .query()
        .finesse_score()
        .expect("typed finesse score")
        .placements();
    assert_eq!(placements.len(), 2);
    assert_eq!((placements[0].x(), placements[0].y()), (0, 0));
    assert_eq!((placements[1].x(), placements[1].y()), (2, 0));
}

#[test]
fn finesse_score_accepts_the_discord_fumen_canonical_contract() {
    // Canonical output for v115@bhA8SeaLJKhhWIegWEeAACegWOekmB.
    let initial = format!("{}1", "0".repeat(59));
    let tokens = [
        "finesse".to_owned(),
        "score".to_owned(),
        "--initial-mask".to_owned(),
        initial,
        "--height".to_owned(),
        "3".to_owned(),
        "--placements".to_owned(),
        "L:left:3:0,Z:reverse:4:1".to_owned(),
        "--queue".to_owned(),
        "LZ".to_owned(),
        "--no-hold".to_owned(),
        "--pattern-knowledge".to_owned(),
        "visible-7".to_owned(),
        "--rule".to_owned(),
        "srs-plus".to_owned(),
    ];
    let request = WebCommandParser::parse_tokens(&tokens)
        .expect("Discord Fumen finesse score command")
        .to_app_request()
        .expect("Fumen finesse score AppRequest");

    let AppCommand::BuildProbability(command) = request.command() else {
        panic!("expected AppCommand::BuildProbability");
    };
    let query = command.query();
    assert!(!query.core_query().allow_hold());
    assert_eq!(
        query.finesse_pattern_knowledge(),
        clearra_problem::FinessePatternKnowledge::VisibleSeven
    );
    assert_eq!(
        query
            .core_query()
            .remaining_queue()
            .as_fixed_sequence()
            .expect("fixed queue")
            .pieces(),
        &[PieceKind::L, PieceKind::Z]
    );
    let placements = query
        .finesse_score()
        .expect("typed finesse score")
        .placements();
    assert_eq!(placements.len(), 2);
    assert_eq!(placements[0].piece(), PieceKind::L);
    assert_eq!(
        placements[0].rotation(),
        clearra_core_domain::piece::rotation::RotationState::Left
    );
    assert_eq!((placements[0].x(), placements[0].y()), (3, 0));
    assert_eq!(placements[1].piece(), PieceKind::Z);
    assert_eq!(
        placements[1].rotation(),
        clearra_core_domain::piece::rotation::RotationState::Two
    );
    assert_eq!((placements[1].x(), placements[1].y()), (4, 1));
}

#[test]
fn finesse_parser_rejects_raw_documents_and_malformed_contracts() {
    let initial = "0".repeat(60);
    let valid_placement = "T:spawn:4:1";
    let cases = [
        ("finesse".to_owned(), WebCommandErrorCode::MissingValue),
        (
            "finesse other".to_owned(),
            WebCommandErrorCode::InvalidValue,
        ),
        (
            format!(
                "finesse score --document v115@vhAAgH --initial-mask {initial} \
                 --height 4 --placements {valid_placement} --queue T"
            ),
            WebCommandErrorCode::UnsupportedCommand,
        ),
        (
            format!("finesse score --height 4 --placements {valid_placement} --queue T"),
            WebCommandErrorCode::MissingValue,
        ),
        (
            format!("finesse score --initial-mask {initial} --height 4 --queue T"),
            WebCommandErrorCode::MissingValue,
        ),
        (
            format!(
                "finesse score --initial-mask {initial} --height 4 \
                 --placements T:up:4:1 --queue T"
            ),
            WebCommandErrorCode::InvalidValue,
        ),
        (
            format!(
                "finesse score --initial-mask {initial} --height 4 \
                 --placements {valid_placement} --placements {valid_placement} --queue T"
            ),
            WebCommandErrorCode::InvalidValue,
        ),
        (
            format!(
                "finesse score --initial-mask {initial} --height 4 \
                 --placements O:spawn:0:0|O:spawn:2:0 --queue OO"
            ),
            WebCommandErrorCode::ProcessSemantics,
        ),
        (
            format!(
                "finesse search --base-mask {initial} --target-mask {initial} --height 4 \
                 --queue T --patterns [T]!"
            ),
            WebCommandErrorCode::InvalidValue,
        ),
        (
            format!(
                "finesse search --base-mask {initial} --target-mask {initial} --height 4 \
                 --queue T --pattern-knowledge private"
            ),
            WebCommandErrorCode::InvalidValue,
        ),
    ];

    for (command, expected_code) in cases {
        let error = WebCommandParser::parse(&command).expect_err(&command);
        assert_eq!(error.code(), expected_code, "{command}");
    }
}

#[test]
fn finesse_score_rejects_more_than_sixty_typed_placements() {
    let placements = std::iter::repeat_n("T:spawn:4:1", 61)
        .collect::<Vec<_>>()
        .join(",");
    let tokens = vec![
        "finesse".to_owned(),
        "score".to_owned(),
        "--initial-mask".to_owned(),
        "0".repeat(60),
        "--height".to_owned(),
        "4".to_owned(),
        "--placements".to_owned(),
        placements,
        "--patterns".to_owned(),
        "[T]!".to_owned(),
    ];

    let error = WebCommandParser::parse_tokens(&tokens).expect_err("placement limit");
    assert_eq!(error.code(), WebCommandErrorCode::InvalidValue);
}

#[test]
fn build_probability_preserves_rule_spin_profile_and_initial_hold() {
    let request = WebCommandParser::parse(
        "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --hold T --no-mirror --aggregate spin --rule srs-x --spin-profile all-mini-plus",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::BuildProbability(command) = request.command() else {
        panic!("expected AppCommand::BuildProbability");
    };
    assert_eq!(
        command.query().core_query().rule().id(),
        clearra_rules::profile::rule_profile::RuleProfileId::SrsX
    );
    assert_eq!(
        command.query().core_query().hold_state().piece(),
        Some(PieceKind::T)
    );
    assert!(command.query().core_query().allow_hold());
    assert_eq!(
        command
            .query()
            .aggregation()
            .spin_profile()
            .expect("spin aggregation")
            .as_str(),
        "all-mini-plus"
    );
}

#[test]
fn canonical_setup_finder_matches_legacy_web_alias() {
    let canonical = WebCommandParser::parse("clearra setup-finder --remaining IOTS --workers 1")
        .expect("canonical setup finder command");
    let legacy = WebCommandParser::parse("clearra setup --remaining IOTS --workers 1")
        .expect("legacy setup alias");
    assert_eq!(canonical, legacy);
}

#[test]
fn build_probability_tiling_only_preserves_hold_supply_and_sets_tiling_objective() {
    let request = WebCommandParser::parse(
        "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue IO --hold T --no-mirror --tiling-only",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::BuildProbability(command) = request.command() else {
        panic!("expected AppCommand::BuildProbability");
    };
    assert!(command.query().aggregation().is_tiling_only());
    assert_eq!(
        command.query().core_query().objective().kind(),
        clearra_core_domain::objective::objective_kind::ObjectiveKind::Tiling
    );
    assert!(command.query().core_query().allow_hold());
    assert_eq!(
        command.query().core_query().hold_state().piece(),
        Some(PieceKind::T)
    );
}

#[test]
fn build_probability_tiling_only_rejects_buildup_only_options() {
    for option in [
        "--aggregate spin",
        "--spin-profile t-spins",
        "--preserve-b2b",
        "--build-dependency-dag",
    ] {
        let command = format!(
            "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror --tiling-only {option}"
        );
        assert!(WebCommandParser::parse(&command).is_err(), "{option}");
    }
}

#[test]
fn build_probability_tiling_rejects_explicit_inactive_options_in_either_order() {
    let base = "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror";
    for option in [
        "--rule srs-plus",
        "--spin-profile t-spins",
        "--preserve-b2b",
        "--build-dependency-dag",
        "--no-build-dependency-dag",
        "--finesse off",
        "--pattern-knowledge both",
    ] {
        for command in [
            format!("{base} --tiling-only {option}"),
            format!("{base} {option} --tiling-only"),
        ] {
            let error = WebCommandParser::parse(&command).expect_err(&command);
            assert_eq!(error.code(), WebCommandErrorCode::InvalidValue, "{command}");
        }
    }
}

#[test]
fn build_probability_rejects_conflicting_aggregation_selectors_in_either_order() {
    let base = "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror";
    for (left, right) in [
        ("--tiling-only", "--aggregate buildability"),
        ("--tiling-only", "--aggregate spin"),
        ("--aggregate buildability", "--aggregate spin"),
    ] {
        for command in [
            format!("{base} {left} {right}"),
            format!("{base} {right} {left}"),
        ] {
            let error = WebCommandParser::parse(&command).expect_err(&command);
            assert_eq!(error.code(), WebCommandErrorCode::InvalidValue, "{command}");
        }
    }

    for command in [
        format!("{base} --aggregate tiling --tiling-only"),
        format!("{base} --tiling-only --tiling-only"),
        format!("{base} --aggregate spin --aggregate spin"),
    ] {
        let error = WebCommandParser::parse(&command).expect_err(&command);
        assert_eq!(error.code(), WebCommandErrorCode::InvalidValue, "{command}");
    }
}

#[test]
fn build_probability_rejects_orphan_spin_and_pattern_knowledge_options_in_either_order() {
    let base = "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror";
    for option in ["--spin-profile t-spins", "--pattern-knowledge both"] {
        for command in [
            format!("{base} --aggregate buildability {option}"),
            format!("{base} {option} --aggregate buildability"),
        ] {
            let error = WebCommandParser::parse(&command).expect_err(&command);
            assert_eq!(error.code(), WebCommandErrorCode::InvalidValue, "{command}");
        }
    }

    for command in [
        format!("{base} --aggregate spin --spin-profile all-mini-plus"),
        format!("{base} --spin-profile all-mini-plus --aggregate spin"),
        format!("{base} --preserve-b2b --spin-profile all-mini-plus"),
        format!("{base} --spin-profile all-mini-plus --preserve-b2b"),
        format!("{base} --finesse inputs --pattern-knowledge visible-7"),
        format!("{base} --pattern-knowledge visible-7 --finesse inputs"),
    ] {
        WebCommandParser::parse(&command)
            .unwrap_or_else(|error| panic!("failed to parse '{command}': {error:?}"))
            .to_app_request()
            .unwrap_or_else(|error| panic!("failed to project '{command}': {error:?}"));
    }
}

#[test]
fn browser_command_accepts_completed_group_followed_by_bag_permutation() {
    WebCommandParser::parse(
        "clearra pc --lines 2 --backend cpu --patterns [^TSZ]!P4 --max-patterns 20160",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");
}

#[test]
fn scenario_initial_hold_remains_independent_from_p7_queue() {
    let request = WebCommandParser::parse(
        "clearra pc --lines 4 --board-mask 0x80787 --height 4 --pieces 8 --patterns P7 --hold S --backend cpu",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::Scenario(command) = request.command() else {
        panic!("expected AppCommand::Scenario");
    };
    assert_eq!(command.query().hold_state().piece(), Some(PieceKind::S));
    assert!(matches!(
        command.query().remaining_queue(),
        clearra_pc_graph::request::PcQueueInput::Standard7Bag
    ));
    assert_eq!(
        command.query().supply_window_size(),
        Some(SupplyWindowSize::new(7))
    );
}

#[test]
fn web_command_preserves_cpu_pool_options_in_the_typed_request() {
    let request = WebCommandParser::parse(
        "clearra pc --lines 2 --backend cpu --workers 1 --use-all-cpu-threads --cpu-warmup",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::Pc(command) = request.command() else {
        panic!("expected AppCommand::Pc");
    };
    let policy = command.query().execution_policy();
    assert_eq!(policy.workers(), 1);
    assert!(policy.use_all_logical_processors());
    assert!(policy.cpu_warmup());
}

#[test]
fn sfinder_percent_preserves_adaptive_workers_without_forcing_parallel_search() {
    let request = WebCommandParser::parse_with_worker_limit(
        "clearra sfinder percent v115@vhAAgH P7P3 4 --auto-workers 3",
        8,
    )
    .expect("Sfinder percent command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::Scenario(command) = request.command() else {
        panic!("expected AppCommand::Scenario");
    };
    let policy = command.query().execution_policy();
    assert_eq!(policy.workers(), 3);
    assert_eq!(policy.workers_requested(), None);
}

#[test]
fn sfinder_forward_and_setup_commands_reach_the_shared_worker_budget() {
    let spin = WebCommandParser::parse_with_worker_limit(
        "clearra sfinder spin-cover v115@vhAAgH TI TSS --auto-workers 2",
        8,
    )
    .expect("Sfinder spin-cover command")
    .to_app_request()
    .expect("spin AppRequest");
    assert!(matches!(spin.command(), AppCommand::SpinFinder(_)));
    assert_eq!(spin.resource_budget().workers(), 2);

    let setup = WebCommandParser::parse_with_worker_limit(
        "clearra sfinder pc-setup IOTS --auto-workers 2",
        8,
    )
    .expect("Sfinder setup command")
    .to_app_request()
    .expect("setup AppRequest");
    assert!(matches!(setup.command(), AppCommand::Setup(_)));
    assert_eq!(setup.resource_budget().workers(), 2);
}

#[test]
fn product_worker_modes_are_mutually_exclusive() {
    let error = WebCommandParser::parse_with_worker_limit(
        "clearra pc --lines 4 --workers 2 --auto-workers 3",
        8,
    )
    .expect_err("fixed and adaptive workers conflict");

    assert_eq!(error.code(), WebCommandErrorCode::InvalidValue);
}

#[test]
fn directly_constructed_typed_requests_obey_reserved_and_hardware_worker_limits() {
    let reserved = WebCommandRequest::setup(vec![PieceKind::I], false)
        .with_worker_hardware_limit(8)
        .with_workers(usize::MAX)
        .to_app_request()
        .expect("reserved-core setup request");
    assert_eq!(reserved.resource_budget().workers(), 7);

    let all = WebCommandRequest::setup(vec![PieceKind::I], false)
        .with_worker_hardware_limit(8)
        .with_use_all_logical_processors(true)
        .with_workers(usize::MAX)
        .to_app_request()
        .expect("all-logical-processors setup request");
    assert_eq!(all.resource_budget().workers(), 8);

    let automatically_capped = WebCommandRequest::setup(vec![PieceKind::I], false)
        .with_worker_hardware_limit(8)
        .with_automatic_worker_limit(3)
        .to_app_request()
        .expect("automatically capped setup request");
    assert_eq!(automatically_capped.resource_budget().workers(), 3);
}

#[test]
fn web_command_preserves_gpu_device_and_warmup_in_the_typed_request() {
    let request = WebCommandParser::parse(
        "clearra pc --lines 2 --backend gpu --gpu-device 3 --gpu-warmup --allow-backend-fallback",
    )
    .expect("web command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::Pc(command) = request.command() else {
        panic!("expected AppCommand::Pc");
    };
    let policy = command.query().execution_policy();
    assert_eq!(policy.gpu_device().as_display_string(), "3");
    assert!(policy.gpu_warmup());
    assert!(policy.allow_backend_fallback());
}

#[test]
fn pc_and_build_backend_fallback_are_order_independent_across_the_bounded_matrix() {
    let backends = ["auto", "cpu", "gpu", "hybrid"];
    let overrides = [
        (None, None),
        (Some("--allow-backend-fallback"), Some(true)),
        (Some("--no-backend-fallback"), Some(false)),
    ];

    for family in ["pc", "build"] {
        for backend in backends {
            for (fallback_option, explicit_value) in overrides {
                let expected = explicit_value.unwrap_or(backend == "auto");
                let orders: &[bool] = if fallback_option.is_some() {
                    &[false, true]
                } else {
                    &[false]
                };
                for fallback_first in orders {
                    let backend_arguments = format!("--backend {backend}");
                    let arguments = match (fallback_option, fallback_first) {
                        (Some(option), true) => format!("{option} {backend_arguments}"),
                        (Some(option), false) => format!("{backend_arguments} {option}"),
                        (None, _) => backend_arguments,
                    };
                    assert_backend_fallback_policy(family, &arguments, backend, expected);
                }
            }
        }
    }
}

#[test]
fn pc_and_build_backend_fallback_preserve_surface_defaults() {
    assert_backend_fallback_policy("pc", "", "auto", true);
    assert_backend_fallback_policy("build", "", "cpu", false);
}

#[test]
fn pc_and_build_reject_conflicting_backend_fallback_flags_in_either_order() {
    for family in ["pc", "build"] {
        for arguments in [
            "--allow-backend-fallback --backend gpu --no-backend-fallback",
            "--no-backend-fallback --backend auto --allow-backend-fallback",
            "--allow-backend-fallback --backend gpu --allow-backend-fallback",
            "--no-backend-fallback --backend auto --no-backend-fallback",
        ] {
            let command = backend_fallback_command(family, arguments);
            let error = WebCommandParser::parse(&command).expect_err(&command);
            assert_eq!(error.code(), WebCommandErrorCode::InvalidValue, "{command}");
        }
    }
}

fn assert_backend_fallback_policy(
    family: &str,
    arguments: &str,
    expected_backend: &str,
    expected_fallback: bool,
) {
    let command = backend_fallback_command(family, arguments);
    let parsed = WebCommandParser::parse(&command)
        .unwrap_or_else(|error| panic!("failed to parse '{command}': {error:?}"));
    assert_eq!(
        parsed.allow_backend_fallback(),
        expected_fallback,
        "typed request: {command}"
    );
    let request = parsed
        .to_app_request()
        .unwrap_or_else(|error| panic!("failed to build AppRequest for '{command}': {error:?}"));
    let policy = match request.command() {
        AppCommand::Pc(command) => command.query().execution_policy(),
        AppCommand::BuildProbability(command) => command.query().core_query().execution_policy(),
        _ => panic!("unexpected command family for '{command}'"),
    };
    assert_eq!(policy.backend().as_str(), expected_backend, "{command}");
    assert_eq!(
        policy.allow_backend_fallback(),
        expected_fallback,
        "AppRequest policy: {command}"
    );
}

fn backend_fallback_command(family: &str, arguments: &str) -> String {
    let base = match family {
        "pc" => "clearra pc --lines 2",
        "build" => {
            "clearra build-probability --base-mask 0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror"
        }
        _ => panic!("unsupported backend fallback test family '{family}'"),
    };
    format!("{base} {arguments}")
}

#[test]
fn web_command_requires_opt_in_for_the_reserved_logical_processor() {
    let hardware = clearra_pc_graph::request::WorkerPolicy::hardware_worker_limit();
    if hardware <= 1 {
        return;
    }
    let command = format!("clearra pc --lines 2 --workers {hardware}");
    assert_eq!(
        WebCommandParser::parse(&command)
            .expect_err("reserved processor requires opt-in")
            .code(),
        WebCommandErrorCode::InvalidValue
    );
    WebCommandParser::parse(&format!("{command} --use-all-cpu-threads"))
        .expect("explicit all-CPU command");
}

#[test]
fn wasm_runtime_does_not_use_native_path_semantics() {
    let error = WebCommandParser::parse("clearra pc --input C:\\temp\\field.txt")
        .expect_err("native path is rejected");

    assert_eq!(error.code(), WebCommandErrorCode::NativePathSemantics);
}

#[test]
fn wasm_runtime_does_not_spawn_process() {
    let error = WebCommandParser::parse("clearra pc --lines 2 | clearra verify")
        .expect_err("process syntax is rejected");

    assert_eq!(error.code(), WebCommandErrorCode::ProcessSemantics);
}

#[test]
fn browser_file_input_uses_virtual_file_handle() {
    let handle = WebVirtualFileHandle::new("field-json", "field.json", "application/json", 128)
        .expect("virtual handle");

    assert_eq!(handle.origin_kind(), "browser-file-input");
    assert_eq!(handle.display_name(), "field.json");
}

#[test]
fn virtual_file_handle_rejects_native_paths() {
    let error = WebVirtualFileHandle::new("bad", "C:\\secret\\field.json", "text/plain", 8)
        .expect_err("native paths forbidden");

    assert_eq!(error.code(), WebCommandErrorCode::NativePathSemantics);
}

#[test]
fn damage_command_compiles_to_typed_forward_search() {
    let request = WebCommandParser::parse(
        "clearra damage --board-mask 0x0 --height 4 --queue TI --no-hold --rule srs-plus --spin-profile t-spins --initial-b2b 2",
    )
    .expect("damage command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::Damage(command) = request.command() else {
        panic!("expected AppCommand::Damage");
    };
    assert_eq!(
        command.query().piece_source().fixed_sequence(),
        Some(&[PieceKind::T, PieceKind::I][..])
    );
    assert!(!command.query().hold_enabled());
    assert_eq!(
        command.query().rule_profile(),
        clearra_rules::profile::rule_profile::RuleProfileId::SrsPlus
    );
    assert_eq!(command.query().spin_profile(), SpinProfileId::TSpins);
    assert_eq!(command.query().initial_back_to_back(), Some(1));
    assert_eq!(command.query().mode(), ForwardSearchMode::MaximumDamage);
}

#[test]
fn damage_command_preserves_minimum_damage_enumeration_policy() {
    let request = WebCommandParser::parse(
        "clearra damage --board-mask 0x0 --height 4 --queue TI --minimum-damage 6",
    )
    .expect("damage threshold command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::Damage(command) = request.command() else {
        panic!("expected AppCommand::Damage");
    };
    assert_eq!(command.query().mode(), ForwardSearchMode::DamageAtLeast(6));
}

#[test]
fn damage_command_accepts_the_canonical_empty_board_mask_at_height_eight() {
    let mask = "0".repeat(60);
    let request = WebCommandParser::parse(&format!("damage --board-mask-v1 {mask} --queue I"))
        .expect("canonical empty damage board");
    let query = request.forward_search_query().expect("forward query");

    assert_eq!(query.board().words(), [0; 4]);
    assert_eq!(query.height(), 8);
}

#[test]
fn damage_command_preserves_the_top_bit_of_the_twenty_fourth_row() {
    let mask = format!("8{}", "0".repeat(59));
    let request = WebCommandParser::parse(&format!("damage --board-mask-v1 {mask} --queue I"))
        .expect("canonical 24-row damage board");
    let query = request.forward_search_query().expect("forward query");

    assert_eq!(query.board().words(), [0, 0, 0, 1_u64 << 47]);
    assert_eq!(query.height(), 24);
}

#[test]
fn damage_command_rejects_noncanonical_and_conflicting_board_masks() {
    for mask in ["0".repeat(59), format!("{}A", "0".repeat(59))] {
        let error = WebCommandParser::parse(&format!("damage --board-mask-v1 {mask} --queue I"))
            .expect_err("noncanonical damage mask");
        assert_eq!(error.code(), WebCommandErrorCode::InvalidValue);
    }

    let canonical = "0".repeat(60);
    for command in [
        format!("damage --board-mask 0x0 --board-mask-v1 {canonical} --queue I"),
        format!("damage --board-mask-v1 {canonical} --board-mask-v1 {canonical} --queue I"),
    ] {
        let error = WebCommandParser::parse(&command).expect_err("conflicting damage masks");
        assert_eq!(error.code(), WebCommandErrorCode::InvalidValue);
    }
}

#[test]
fn canonical_damage_mask_rejects_an_explicit_height_below_its_visible_rows() {
    let mask = format!("8{}", "0".repeat(59));
    let error = WebCommandParser::parse(&format!(
        "damage --board-mask-v1 {mask} --height 23 --queue I"
    ))
    .expect_err("height below canonical field");

    assert_eq!(error.code(), WebCommandErrorCode::InvalidValue);
}

#[test]
fn spin_finder_command_preserves_profile_and_target_group() {
    let request = WebCommandParser::parse(
        "clearra spin-finder --board-mask 0x0 --height 8 --queue J --hold --rule srs-x --spin-profile all-mini-plus --lines 2 --spin-category other",
    )
    .expect("spin command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::SpinFinder(command) = request.command() else {
        panic!("expected AppCommand::SpinFinder");
    };
    assert!(command.query().hold_enabled());
    assert_eq!(
        command.query().rule_profile(),
        clearra_rules::profile::rule_profile::RuleProfileId::SrsX
    );
    assert_eq!(command.query().spin_profile(), SpinProfileId::AllMiniPlus);
    let ForwardSearchMode::SpinFinder(target) = command.query().mode() else {
        panic!("expected spin-finder mode");
    };
    assert_eq!(target.lines(), Some(2));
    assert_eq!(target.category(), ForwardSpinCategory::Other);
}

#[test]
fn spin_finder_cli_accepts_patterns_beyond_the_gui_piece_limit() {
    let request = WebCommandParser::parse(
        "clearra spin-finder --patterns P7P1 --spin-profile t-spins --lines any",
    )
    .expect("eight-piece pattern");
    let query = request.forward_search_query().expect("forward query");
    assert_eq!(query.height(), 8);
    assert!(query.piece_source().is_pattern());
    assert_eq!(query.piece_source().sequence_len(), 8);

    let longer =
        WebCommandParser::parse("clearra spin-finder --patterns IOTSZLJIO --spin-profile t-spins")
            .expect("CLI pattern length is not a product limit");
    assert_eq!(
        longer
            .forward_search_query()
            .expect("forward query")
            .piece_source()
            .sequence_len(),
        9
    );

    let damage_pattern = WebCommandParser::parse("clearra damage --patterns [TI]")
        .expect_err("damage pattern must be rejected");
    assert_eq!(
        damage_pattern.code(),
        WebCommandErrorCode::UnsupportedCommand
    );
}

const SEARCH_OPTION_CONTRACT: &str =
    include_str!("../../../tests/fixtures/contracts/search_option_contract.tsv");

#[derive(Clone, Copy, Debug)]
struct ForwardContractSelection {
    option: &'static str,
    value: &'static str,
}

fn forward_contract_values(family: &str, option: &str) -> Vec<&'static str> {
    let row = SEARCH_OPTION_CONTRACT
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .find(|line| {
            let mut columns = line.split('\t');
            columns.next() == Some(family) && columns.next() == Some(option)
        })
        .unwrap_or_else(|| panic!("missing search contract row {family}.{option}"));
    row.split('\t')
        .nth(3)
        .expect("contract valid representatives")
        .split('|')
        .collect()
}

fn forward_contract_invalid_values(family: &str, option: &str) -> Vec<&'static str> {
    let row = SEARCH_OPTION_CONTRACT
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .find(|line| {
            let mut columns = line.split('\t');
            columns.next() == Some(family) && columns.next() == Some(option)
        })
        .unwrap_or_else(|| panic!("missing search contract row {family}.{option}"));
    match row
        .split('\t')
        .nth(4)
        .expect("contract invalid representatives")
    {
        "-" => Vec::new(),
        values => values.split('|').collect(),
    }
}

fn forward_contract_command(family: &str, selections: &[ForwardContractSelection]) -> String {
    let mut tokens = vec![
        "clearra".to_owned(),
        family.to_owned(),
        "--board-mask".to_owned(),
        "0".to_owned(),
    ];
    if !selections
        .iter()
        .any(|selection| selection.option == "source")
    {
        tokens.extend(["--queue".to_owned(), "T".to_owned()]);
    }
    let implicit_minimum_damage = family == "damage"
        && selections
            .iter()
            .any(|selection| selection.option == "damage-mode" && selection.value == "at-least")
        && !selections
            .iter()
            .any(|selection| selection.option == "minimum-damage");
    for selection in selections {
        match (selection.option, selection.value) {
            ("height", value) => tokens.extend(["--height".to_owned(), value.to_owned()]),
            ("source", "fixed") => tokens.extend(["--queue".to_owned(), "T".to_owned()]),
            ("source", "pattern") if family == "spin-finder" => {
                tokens.extend(["--patterns".to_owned(), "P1".to_owned()]);
            }
            ("hold", "on") => tokens.push("--hold".to_owned()),
            ("hold", "off") => tokens.push("--no-hold".to_owned()),
            ("rule", value) => tokens.extend(["--rule".to_owned(), value.to_owned()]),
            ("spin-profile", value) => {
                tokens.extend(["--spin-profile".to_owned(), value.to_owned()]);
            }
            ("damage-mode", "maximum" | "at-least") if family == "damage" => {}
            ("minimum-damage", value) if family == "damage" => {
                tokens.extend(["--minimum-damage".to_owned(), value.to_owned()]);
            }
            ("initial-combo", value) => {
                tokens.extend(["--initial-combo".to_owned(), value.to_owned()]);
            }
            ("initial-b2b", value) => {
                tokens.extend(["--initial-b2b".to_owned(), value.to_owned()]);
            }
            ("preserve-b2b", "on") => tokens.push("--preserve-b2b".to_owned()),
            ("preserve-b2b", "off") => {}
            ("lines", value) if family == "spin-finder" => {
                tokens.extend(["--lines".to_owned(), value.to_owned()]);
            }
            ("category", value) if family == "spin-finder" => {
                tokens.extend(["--spin-category".to_owned(), value.to_owned()]);
            }
            ("workers", value) => tokens.extend(["--workers".to_owned(), value.to_owned()]),
            _ => panic!(
                "unsupported {family} contract selection {}={}",
                selection.option, selection.value
            ),
        }
    }
    if implicit_minimum_damage {
        tokens.extend(["--minimum-damage".to_owned(), "1".to_owned()]);
    }
    tokens.join(" ")
}

fn spin_finder_contract_case_is_valid(selections: &[ForwardContractSelection]) -> bool {
    let profile = selections
        .iter()
        .find(|selection| selection.option == "spin-profile")
        .map_or(SpinProfileId::TSpins, |selection| {
            SpinProfileId::parse(selection.value).expect("fixture spin profile")
        });
    let category = selections
        .iter()
        .find(|selection| selection.option == "category")
        .map_or(ForwardSpinCategory::Any, |selection| {
            match selection.value {
                "any" => ForwardSpinCategory::Any,
                "t" => ForwardSpinCategory::T,
                "other" => ForwardSpinCategory::Other,
                value => panic!("fixture spin category {value}"),
            }
        });
    profile != SpinProfileId::Disabled
        && (category != ForwardSpinCategory::Other || profile.recognizes_non_t_immobile_spins())
}

fn assert_forward_contract_projection(
    family: &str,
    selections: &[ForwardContractSelection],
    expected_valid: bool,
) {
    let command = forward_contract_command(family, selections);
    match WebCommandParser::parse_with_worker_limit(&command, 65) {
        Ok(parsed) if expected_valid => {
            let request = parsed
                .to_app_request()
                .unwrap_or_else(|error| panic!("failed to project '{command}': {error:?}"));
            assert_eq!(request.command_kind().as_str(), family, "{command}");
            if family == "damage" {
                let AppCommand::Damage(damage_command) = request.command() else {
                    panic!("expected AppCommand::Damage for '{command}'");
                };
                let explicit_minimum = selections
                    .iter()
                    .find(|selection| selection.option == "minimum-damage")
                    .map(|selection| {
                        selection
                            .value
                            .parse::<u32>()
                            .expect("fixture minimum damage")
                    });
                let at_least = selections.iter().any(|selection| {
                    selection.option == "damage-mode" && selection.value == "at-least"
                });
                let expected = explicit_minimum
                    .map(ForwardSearchMode::DamageAtLeast)
                    .unwrap_or_else(|| {
                        if at_least {
                            ForwardSearchMode::DamageAtLeast(1)
                        } else {
                            ForwardSearchMode::MaximumDamage
                        }
                    });
                assert_eq!(damage_command.query().mode(), expected, "{command}");
            }
        }
        Err(error) if !expected_valid => {
            assert_eq!(error.code(), WebCommandErrorCode::InvalidValue, "{command}");
        }
        Ok(_) => panic!("expected parser rejection: {command}"),
        Err(error) => panic!("unexpected parser rejection for '{command}': {error:?}"),
    }
}

#[test]
fn damage_fixture_exposed_options_reach_every_single_and_ordered_pair_cartesian_projection() {
    assert_eq!(
        forward_contract_values("damage", "damage-mode"),
        ["maximum", "at-least"]
    );
    let default = WebCommandParser::parse("clearra damage --board-mask 0 --queue T")
        .expect("default maximum-damage command")
        .to_app_request()
        .expect("maximum-damage AppRequest");
    let AppCommand::Damage(command) = default.command() else {
        panic!("expected AppCommand::Damage");
    };
    assert_eq!(command.query().mode(), ForwardSearchMode::MaximumDamage);

    let options = [
        "height",
        "source",
        "hold",
        "rule",
        "spin-profile",
        "damage-mode",
        "minimum-damage",
        "initial-combo",
        "initial-b2b",
        "preserve-b2b",
        "workers",
    ];
    let values = options
        .iter()
        .map(|option| forward_contract_values("damage", option))
        .collect::<Vec<_>>();
    let mut cases = 0_usize;
    for (option, representatives) in options.iter().zip(&values) {
        for value in representatives {
            assert_forward_contract_projection(
                "damage",
                &[ForwardContractSelection { option, value }],
                true,
            );
            cases += 1;
        }
    }
    for left in 0..options.len() {
        for right in (left + 1)..options.len() {
            for left_value in &values[left] {
                for right_value in &values[right] {
                    for selections in [
                        [
                            ForwardContractSelection {
                                option: options[left],
                                value: left_value,
                            },
                            ForwardContractSelection {
                                option: options[right],
                                value: right_value,
                            },
                        ],
                        [
                            ForwardContractSelection {
                                option: options[right],
                                value: right_value,
                            },
                            ForwardContractSelection {
                                option: options[left],
                                value: left_value,
                            },
                        ],
                    ] {
                        if selections.iter().any(|selection| {
                            selection.option == "damage-mode" && selection.value == "maximum"
                        }) && selections
                            .iter()
                            .any(|selection| selection.option == "minimum-damage")
                        {
                            continue;
                        }
                        assert_forward_contract_projection("damage", &selections, true);
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 1_107, "exact damage production parser cases");
}

#[test]
fn spin_finder_fixture_exposed_options_reach_every_single_and_ordered_pair_cartesian_projection() {
    let options = [
        "height",
        "source",
        "hold",
        "rule",
        "spin-profile",
        "lines",
        "category",
        "initial-combo",
        "initial-b2b",
        "preserve-b2b",
        "workers",
    ];
    let values = options
        .iter()
        .map(|option| forward_contract_values("spin-finder", option))
        .collect::<Vec<_>>();
    let mut cases = 0_usize;
    for (option, representatives) in options.iter().zip(&values) {
        for value in representatives {
            let selections = [ForwardContractSelection { option, value }];
            assert_forward_contract_projection(
                "spin-finder",
                &selections,
                spin_finder_contract_case_is_valid(&selections),
            );
            cases += 1;
        }
    }
    for left in 0..options.len() {
        for right in (left + 1)..options.len() {
            for left_value in &values[left] {
                for right_value in &values[right] {
                    for selections in [
                        [
                            ForwardContractSelection {
                                option: options[left],
                                value: left_value,
                            },
                            ForwardContractSelection {
                                option: options[right],
                                value: right_value,
                            },
                        ],
                        [
                            ForwardContractSelection {
                                option: options[right],
                                value: right_value,
                            },
                            ForwardContractSelection {
                                option: options[left],
                                value: left_value,
                            },
                        ],
                    ] {
                        let expected_valid = spin_finder_contract_case_is_valid(&selections);
                        assert_forward_contract_projection(
                            "spin-finder",
                            &selections,
                            expected_valid,
                        );
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 1_661, "exact spin-finder production parser cases");
}

#[test]
fn forward_fixture_dependency_contradictions_are_invalid_in_both_orders() {
    for (profile, category) in [
        ("disabled", "any"),
        ("t-spins", "other"),
        ("t-spins-plus", "other"),
    ] {
        for selections in [
            [
                ForwardContractSelection {
                    option: "spin-profile",
                    value: profile,
                },
                ForwardContractSelection {
                    option: "category",
                    value: category,
                },
            ],
            [
                ForwardContractSelection {
                    option: "category",
                    value: category,
                },
                ForwardContractSelection {
                    option: "spin-profile",
                    value: profile,
                },
            ],
        ] {
            assert_forward_contract_projection("spin-finder", &selections, false);
        }
    }

    for (family, expected_code) in [
        ("damage", WebCommandErrorCode::UnsupportedCommand),
        ("spin-finder", WebCommandErrorCode::InvalidValue),
    ] {
        for command in [
            format!("clearra {family} --board-mask 0 --queue T --patterns P1"),
            format!("clearra {family} --board-mask 0 --patterns P1 --queue T"),
        ] {
            let error = WebCommandParser::parse(&command).expect_err(&command);
            assert_eq!(error.code(), expected_code, "{command}");
        }
    }
}

#[test]
fn forward_fixture_invalid_representatives_fail_at_the_authoritative_parser_boundary() {
    let relevant = [
        ("damage", "height"),
        ("damage", "source"),
        ("damage", "minimum-damage"),
        ("damage", "initial-combo"),
        ("damage", "initial-b2b"),
        ("damage", "workers"),
        ("spin-finder", "height"),
        ("spin-finder", "source"),
        ("spin-finder", "spin-profile"),
        ("spin-finder", "lines"),
        ("spin-finder", "category"),
        ("spin-finder", "initial-combo"),
        ("spin-finder", "initial-b2b"),
        ("spin-finder", "workers"),
    ];
    let mut cases = 0_usize;
    let mut damage_cases = 0_usize;
    let mut spin_cases = 0_usize;
    for (family, option) in relevant {
        for value in forward_contract_invalid_values(family, option) {
            let (command, expected_code) = match (family, option, value) {
                ("damage", "source", "pattern") => (
                    "clearra damage --board-mask 0 --patterns P1".to_owned(),
                    WebCommandErrorCode::UnsupportedCommand,
                ),
                (_, "source", "both") => (
                    format!("clearra {family} --board-mask 0 --queue T --patterns P1"),
                    WebCommandErrorCode::InvalidValue,
                ),
                (_, "source", "empty") => (
                    format!("clearra {family} --board-mask 0 --queue Q"),
                    WebCommandErrorCode::InvalidValue,
                ),
                (_, "height", value) => (
                    format!("clearra {family} --board-mask 0 --queue T --height {value}"),
                    WebCommandErrorCode::InvalidValue,
                ),
                (_, "spin-profile", value) => (
                    format!("clearra {family} --board-mask 0 --queue T --spin-profile {value}"),
                    WebCommandErrorCode::InvalidValue,
                ),
                (_, "minimum-damage", value) => (
                    format!("clearra {family} --board-mask 0 --queue T --minimum-damage {value}"),
                    WebCommandErrorCode::InvalidValue,
                ),
                (_, "initial-combo", value) => (
                    format!("clearra {family} --board-mask 0 --queue T --initial-combo {value}"),
                    WebCommandErrorCode::InvalidValue,
                ),
                (_, "initial-b2b", value) => (
                    format!("clearra {family} --board-mask 0 --queue T --initial-b2b {value}"),
                    WebCommandErrorCode::InvalidValue,
                ),
                (_, "lines", value) => (
                    format!("clearra {family} --board-mask 0 --queue T --lines {value}"),
                    WebCommandErrorCode::InvalidValue,
                ),
                (_, "category", value) => (
                    format!("clearra {family} --board-mask 0 --queue T --spin-category {value}"),
                    WebCommandErrorCode::InvalidValue,
                ),
                (_, "workers", value) => (
                    format!("clearra {family} --board-mask 0 --queue T --workers {value}"),
                    WebCommandErrorCode::InvalidValue,
                ),
                _ => panic!("unmapped invalid fixture representative {family}.{option}={value}"),
            };
            let error = WebCommandParser::parse(&command).expect_err(&command);
            assert_eq!(error.code(), expected_code, "{family}.{option}={value}");
            cases += 1;
            if family == "damage" {
                damage_cases += 1;
            } else {
                spin_cases += 1;
            }
        }
    }
    assert_eq!(damage_cases, 8, "damage invalid fixture representatives");
    assert_eq!(spin_cases, 12, "spin invalid fixture representatives");
    assert_eq!(cases, 20, "all forward invalid fixture representatives");
}

#[derive(Clone, Copy, Debug)]
struct SearchContractSelection<'a> {
    option: &'a str,
    value: &'a str,
}

fn project_contract_command(command: &str) -> Result<clearra_app::AppRequest, WebCommandError> {
    WebCommandParser::parse_with_worker_limit(command, 65)?.to_app_request()
}

fn assert_order_independent_contract_projection(
    family: &str,
    left: SearchContractSelection<'_>,
    right: SearchContractSelection<'_>,
    command: impl Fn(&[SearchContractSelection<'_>]) -> String,
    expected_valid: bool,
) {
    let forward_command = command(&[left, right]);
    let reverse_command = command(&[right, left]);
    let forward = project_contract_command(&forward_command);
    let reverse = project_contract_command(&reverse_command);

    match (forward, reverse) {
        (Ok(forward), Ok(reverse)) => {
            assert!(
                expected_valid,
                "{family} pair should be rejected: {forward_command}"
            );
            assert_eq!(
                forward, reverse,
                "{family}: {forward_command} <> {reverse_command}"
            );
        }
        (Err(forward), Err(reverse)) => {
            assert!(
                !expected_valid,
                "{family} pair should be accepted: {forward_command}"
            );
            assert_eq!(forward.code(), reverse.code(), "{family}: error code");
            assert_eq!(
                forward.message(),
                reverse.message(),
                "{family}: error message"
            );
            assert_eq!(forward.code(), WebCommandErrorCode::InvalidValue);
        }
        (forward, reverse) => panic!(
            "{family} option order changed acceptance: {forward_command} => {forward:?}; \
             {reverse_command} => {reverse:?}"
        ),
    }
}

fn pc_contract_command(selections: &[SearchContractSelection<'_>]) -> String {
    let failed_queue = selections
        .iter()
        .any(|selection| selection.option == "score-mode" && selection.value == "failed-queue");
    let mut tokens = vec![
        "clearra".to_owned(),
        if failed_queue { "failed-queue" } else { "pc" }.to_owned(),
    ];
    if !selections
        .iter()
        .any(|selection| selection.option == "lines")
    {
        tokens.extend(["--lines".to_owned(), "4".to_owned()]);
    }
    if !selections
        .iter()
        .any(|selection| selection.option == "source")
    {
        tokens.extend(["--queue".to_owned(), "IOTSZJLIOTSZJLIOTSZJL".to_owned()]);
    }
    for selection in selections {
        match (selection.option, selection.value) {
            ("lines", value) => tokens.extend(["--lines".to_owned(), value.to_owned()]),
            ("source", "fixed") => {
                tokens.extend(["--queue".to_owned(), "IOTSZJLIOTSZJLIOTSZJL".to_owned()])
            }
            ("source", "pattern") => {
                tokens.extend(["--patterns".to_owned(), "P7P7P7".to_owned()]);
            }
            ("source", "empty") => {}
            // Opening PC represents an empty enabled hold by omission; the
            // valued `--hold empty` spelling belongs to scenario PC input.
            ("hold", "on") => {}
            ("hold", "off") => tokens.push("--no-hold".to_owned()),
            ("queue-knowledge", value) => {
                tokens.extend(["--queue-knowledge".to_owned(), value.to_owned()]);
            }
            ("score-mode", "off" | "failed-queue") => {}
            ("score-mode", "minimum-cover") => {
                tokens.extend(["--objective".to_owned(), "minimum-cover".to_owned()]);
            }
            ("score-mode", "summary") => tokens.push("--score".to_owned()),
            ("score-mode", "tiling") => tokens.push("--tiling-only".to_owned()),
            ("rule", value) => tokens.extend(["--rule".to_owned(), value.to_owned()]),
            ("spin-profile", value) => {
                tokens.extend(["--spin-profile".to_owned(), value.to_owned()]);
            }
            ("preserve-b2b", "on") => tokens.push("--preserve-b2b".to_owned()),
            ("preserve-b2b", "off") => {}
            ("initial-b2b", value) => {
                tokens.extend(["--initial-b2b".to_owned(), value.to_owned()]);
            }
            ("solution-probabilities", "on") => {
                tokens.push("--solution-probabilities".to_owned());
            }
            ("solution-probabilities", "off") => {}
            ("backend", value) => tokens.extend(["--backend".to_owned(), value.to_owned()]),
            ("fallback", "default") => {}
            ("fallback", "allow") => tokens.push("--allow-backend-fallback".to_owned()),
            ("fallback", "deny") => tokens.push("--no-backend-fallback".to_owned()),
            ("workers", value) => tokens.extend(["--workers".to_owned(), value.to_owned()]),
            ("tablebase", "on") => tokens.push("--tablebase".to_owned()),
            ("tablebase", "off") => tokens.push("--no-tablebase".to_owned()),
            ("dependency-dag", "on") => tokens.push("--build-dependency-dag".to_owned()),
            ("dependency-dag", "off") => tokens.push("--no-build-dependency-dag".to_owned()),
            ("gpu-device", value) => {
                tokens.extend(["--gpu-device".to_owned(), value.to_owned()]);
            }
            _ => panic!(
                "unsupported PC contract selection {}={}",
                selection.option, selection.value
            ),
        }
    }
    tokens.join(" ")
}

fn build_contract_command(selections: &[SearchContractSelection<'_>]) -> String {
    let mut tokens = vec![
        "clearra".to_owned(),
        "build-probability".to_owned(),
        "--base-mask".to_owned(),
        "0".to_owned(),
        "--target-mask".to_owned(),
        "15".to_owned(),
    ];
    if !selections
        .iter()
        .any(|selection| selection.option == "height")
    {
        tokens.extend(["--height".to_owned(), "8".to_owned()]);
    }
    if !selections
        .iter()
        .any(|selection| selection.option == "source")
    {
        tokens.extend(["--queue".to_owned(), "I".to_owned()]);
    }
    for selection in selections {
        match (selection.option, selection.value) {
            ("height", value) => tokens.extend(["--height".to_owned(), value.to_owned()]),
            ("source", "fixed") => tokens.extend(["--queue".to_owned(), "I".to_owned()]),
            ("source", "pattern") => {
                tokens.extend(["--patterns".to_owned(), "P1".to_owned()]);
            }
            ("source", "empty") => {}
            ("hold", "on") => tokens.extend(["--hold".to_owned(), "empty".to_owned()]),
            ("hold", "off") => tokens.push("--no-hold".to_owned()),
            ("aggregation", value) => {
                tokens.extend(["--aggregate".to_owned(), value.to_owned()]);
            }
            ("rule", value) => tokens.extend(["--rule".to_owned(), value.to_owned()]),
            ("spin-profile", value) => {
                tokens.extend(["--spin-profile".to_owned(), value.to_owned()]);
            }
            ("preserve-b2b", "on") => tokens.push("--preserve-b2b".to_owned()),
            ("preserve-b2b", "off") => {}
            ("dependency-dag", "on") => tokens.push("--build-dependency-dag".to_owned()),
            ("dependency-dag", "off") => tokens.push("--no-build-dependency-dag".to_owned()),
            ("finesse", value) => tokens.extend(["--finesse".to_owned(), value.to_owned()]),
            ("pattern-knowledge", value) => {
                tokens.extend(["--pattern-knowledge".to_owned(), value.to_owned()]);
            }
            ("mirror", "on") => tokens.push("--include-mirror".to_owned()),
            ("mirror", "off") => tokens.push("--no-mirror".to_owned()),
            ("backend", value) => tokens.extend(["--backend".to_owned(), value.to_owned()]),
            ("fallback", "default") => {}
            ("fallback", "allow") => tokens.push("--allow-backend-fallback".to_owned()),
            ("fallback", "deny") => tokens.push("--no-backend-fallback".to_owned()),
            ("workers", value) => tokens.extend(["--workers".to_owned(), value.to_owned()]),
            _ => panic!(
                "unsupported build contract selection {}={}",
                selection.option, selection.value
            ),
        }
    }
    tokens.join(" ")
}

fn spin_structure_contract_command(selections: &[SearchContractSelection<'_>]) -> String {
    let mut tokens = vec!["clearra".to_owned(), "spin-structure".to_owned()];
    if !selections
        .iter()
        .any(|selection| selection.option == "inventory")
    {
        tokens.extend(["--pieces".to_owned(), "IOTSZJL".to_owned()]);
    }
    tokens.extend(["--height".to_owned(), "24".to_owned()]);
    if selections
        .iter()
        .any(|selection| selection.option == "fill-bottom")
        && !selections
            .iter()
            .any(|selection| selection.option == "fill-top")
    {
        tokens.extend(["--fill-top".to_owned(), "24".to_owned()]);
    }
    for selection in selections {
        match (selection.option, selection.value) {
            ("inventory", value) => tokens.extend(["--pieces".to_owned(), value.to_owned()]),
            ("fill-bottom", value) => {
                tokens.extend(["--fill-bottom".to_owned(), value.to_owned()]);
            }
            ("fill-top", value) => tokens.extend(["--fill-top".to_owned(), value.to_owned()]),
            ("max-placements", value) => {
                tokens.extend(["--max-placements".to_owned(), value.to_owned()]);
            }
            ("minimality", value) => {
                tokens.extend(["--minimality".to_owned(), value.to_owned()]);
            }
            ("rule", value) => tokens.extend(["--rule".to_owned(), value.to_owned()]),
            ("spin-profile", value) => {
                tokens.extend(["--spin-profile".to_owned(), value.to_owned()]);
            }
            ("lines", value) => tokens.extend(["--lines".to_owned(), value.to_owned()]),
            ("workers", value) => tokens.extend(["--workers".to_owned(), value.to_owned()]),
            _ => panic!(
                "unsupported spin-structure contract selection {}={}",
                selection.option, selection.value
            ),
        }
    }
    tokens.join(" ")
}

fn setup_contract_remaining(selections: &[SearchContractSelection<'_>]) -> &'static str {
    if let Some(remaining) = selection_value(selections, "remaining") {
        return match remaining {
            "I" => "I",
            "IOT" => "IOT",
            "IOTS" => "IOTS",
            "IOTSZJL" => "IOTSZJL",
            _ => panic!("unsupported setup remaining fixture value {remaining}"),
        };
    }

    if selection_enabled(selections, "post-cycle-borrow") {
        return "IOT";
    }
    match selection_value(selections, "next-cycle-remaining") {
        Some("I") => "IOTS",
        Some("IOTS") => "IOTSZJL",
        Some("IOTSZJL") => "IOT",
        Some(value) => panic!("unsupported next-cycle fixture value {value}"),
        None => "IOTSZJL",
    }
}

fn setup_contract_command(selections: &[SearchContractSelection<'_>]) -> String {
    let mut tokens = vec!["clearra".to_owned(), "setup".to_owned()];
    if selection_value(selections, "remaining").is_none() {
        tokens.extend([
            "--remaining".to_owned(),
            setup_contract_remaining(selections).to_owned(),
        ]);
    }
    if selection_value(selections, "mode") == Some("qb")
        && selection_value(selections, "qb").is_none()
    {
        tokens.extend(["--qb".to_owned(), "OS".to_owned()]);
    }

    for selection in selections {
        match (selection.option, selection.value) {
            ("mode", value) => tokens.extend(["--mode".to_owned(), value.to_owned()]),
            ("remaining", value) => {
                tokens.extend(["--remaining".to_owned(), value.to_owned()]);
            }
            ("qb", value) => tokens.extend(["--qb".to_owned(), value.to_owned()]),
            // The public full-queue spelling intentionally projects to the
            // authoritative Web parser's equivalent oracle policy.
            ("queue-knowledge", "full-queue") => {
                tokens.extend(["--queue-knowledge".to_owned(), "oracle".to_owned()]);
            }
            ("queue-knowledge", value) => {
                tokens.extend(["--queue-knowledge".to_owned(), value.to_owned()]);
            }
            ("next-cycle-remaining", value) => {
                tokens.extend(["--next-cycle-remaining".to_owned(), value.to_owned()]);
            }
            ("post-cycle-borrow", "on") => {
                tokens.push("--allow-post-cycle-borrow".to_owned());
            }
            ("post-cycle-borrow", "off") => {}
            ("priority", value) => {
                tokens.extend(["--priority".to_owned(), value.to_owned()]);
            }
            ("length", value) => {
                tokens.extend(["--setup-length".to_owned(), value.to_owned()]);
            }
            ("max-setup-pieces", value) => {
                tokens.extend(["--max-setup-pieces".to_owned(), value.to_owned()]);
            }
            ("rule", value) => tokens.extend(["--rule".to_owned(), value.to_owned()]),
            ("tablebase", "on") => tokens.push("--tablebase".to_owned()),
            ("tablebase", "off") => tokens.push("--no-tablebase".to_owned()),
            ("workers", value) => {
                tokens.extend(["--workers".to_owned(), value.to_owned()]);
            }
            _ => panic!(
                "unsupported setup contract selection {}={}",
                selection.option, selection.value
            ),
        }
    }
    tokens.join(" ")
}

fn setup_contract_expected_next_count(remaining_count: usize) -> usize {
    match remaining_count {
        7 => 4,
        4 => 1,
        1 => 5,
        5 => 2,
        2 => 6,
        6 => 3,
        3 => 7,
        _ => panic!("fixture residue count must be one through seven"),
    }
}

fn setup_contract_case_is_valid(selections: &[SearchContractSelection<'_>]) -> bool {
    if selection_value(selections, "mode") == Some("oracle")
        && selection_value(selections, "qb").is_some()
    {
        return false;
    }

    let remaining_count = setup_contract_remaining(selections).len();
    if selection_enabled(selections, "post-cycle-borrow") && remaining_count != 3 {
        return false;
    }
    if let Some(next_cycle) = selection_value(selections, "next-cycle-remaining") {
        if next_cycle.len() != setup_contract_expected_next_count(remaining_count) {
            return false;
        }
    }
    true
}

fn assert_complete_contract_matrix(
    family: &str,
    options: &[&str],
    command: impl Copy + Fn(&[SearchContractSelection<'_>]) -> String,
    expected_valid: impl Copy + Fn(&[SearchContractSelection<'_>]) -> bool,
) -> usize {
    let values = options
        .iter()
        .map(|option| forward_contract_values(family, option))
        .collect::<Vec<_>>();
    let mut cases = 0;
    for (option, representatives) in options.iter().zip(&values) {
        for value in representatives {
            let selections = [SearchContractSelection { option, value }];
            let invocation = command(&selections);
            match (
                project_contract_command(&invocation),
                expected_valid(&selections),
            ) {
                (Ok(_), true) => {}
                (Err(error), false) => assert_eq!(
                    error.code(),
                    WebCommandErrorCode::InvalidValue,
                    "{family}.{option}={value}: {invocation}"
                ),
                (Ok(_), false) => panic!("{family} singleton should be rejected: {invocation}"),
                (Err(error), true) => {
                    panic!("{family} singleton should be accepted: {invocation}: {error:?}")
                }
            }
            cases += 1;
        }
    }
    for left in 0..options.len() {
        for right in (left + 1)..options.len() {
            for left_value in &values[left] {
                for right_value in &values[right] {
                    let selections = [
                        SearchContractSelection {
                            option: options[left],
                            value: left_value,
                        },
                        SearchContractSelection {
                            option: options[right],
                            value: right_value,
                        },
                    ];
                    assert_order_independent_contract_projection(
                        family,
                        selections[0],
                        selections[1],
                        command,
                        expected_valid(&selections),
                    );
                    cases += 2;
                }
            }
        }
    }
    cases
}

fn selection_value<'a>(
    selections: &'a [SearchContractSelection<'a>],
    option: &str,
) -> Option<&'a str> {
    selections
        .iter()
        .find(|selection| selection.option == option)
        .map(|selection| selection.value)
}

fn selection_enabled(selections: &[SearchContractSelection<'_>], option: &str) -> bool {
    selection_value(selections, option) == Some("on")
}

fn pc_contract_case_is_valid(selections: &[SearchContractSelection<'_>]) -> bool {
    let mode = selection_value(selections, "score-mode").unwrap_or("off");
    let preserves_b2b = selection_enabled(selections, "preserve-b2b");
    let has_spin_profile = selection_value(selections, "spin-profile").is_some();
    let has_initial_b2b = selection_value(selections, "initial-b2b").is_some();

    if mode == "tiling" {
        return selection_value(selections, "rule").is_none()
            && !has_spin_profile
            && !preserves_b2b
            && !has_initial_b2b
            && !selection_enabled(selections, "solution-probabilities")
            && !selection_enabled(selections, "tablebase")
            && !selection_enabled(selections, "dependency-dag")
            && selection_value(selections, "queue-knowledge") != Some("visible-7");
    }
    if mode == "failed-queue"
        && (has_initial_b2b
            || selection_enabled(selections, "solution-probabilities")
            || (has_spin_profile && !preserves_b2b))
    {
        return false;
    }
    if has_spin_profile && mode != "summary" && !preserves_b2b {
        return false;
    }
    if has_initial_b2b && mode != "summary" {
        return false;
    }
    true
}

fn build_contract_case_is_valid(selections: &[SearchContractSelection<'_>]) -> bool {
    let aggregation = selection_value(selections, "aggregation").unwrap_or("buildability");
    let preserves_b2b = selection_enabled(selections, "preserve-b2b");
    let has_spin_profile = selection_value(selections, "spin-profile").is_some();
    let finesse = selection_value(selections, "finesse");
    let has_pattern_knowledge = selection_value(selections, "pattern-knowledge").is_some();

    if aggregation == "tiling" {
        return selection_value(selections, "rule").is_none()
            && !has_spin_profile
            && !preserves_b2b
            && selection_value(selections, "dependency-dag").is_none()
            && finesse.is_none()
            && !has_pattern_knowledge;
    }
    if has_spin_profile && aggregation != "spin" && !preserves_b2b {
        return false;
    }
    if has_pattern_knowledge && finesse != Some("inputs") {
        return false;
    }
    true
}

fn spin_structure_contract_case_is_valid(selections: &[SearchContractSelection<'_>]) -> bool {
    if selection_value(selections, "spin-profile") == Some("t-spin-simple") {
        return false;
    }
    if let (Some(bottom), Some(top)) = (
        selection_value(selections, "fill-bottom"),
        selection_value(selections, "fill-top"),
    ) {
        if bottom.parse::<u8>().expect("fixture fill bottom")
            >= top.parse::<u8>().expect("fixture fill top")
        {
            return false;
        }
    }
    if let (Some(inventory), Some(max_placements)) = (
        selection_value(selections, "inventory"),
        selection_value(selections, "max-placements"),
    ) {
        if max_placements
            .parse::<usize>()
            .expect("fixture max placements")
            > inventory.len()
        {
            return false;
        }
    }
    true
}

#[test]
fn pc_fixture_reaches_the_actual_parser_for_every_single_and_ordered_option_pair() {
    let options = [
        "lines",
        "source",
        "hold",
        "queue-knowledge",
        "score-mode",
        "rule",
        "spin-profile",
        "preserve-b2b",
        "initial-b2b",
        "solution-probabilities",
        "backend",
        "fallback",
        "workers",
        "tablebase",
        "dependency-dag",
        "gpu-device",
    ];
    assert_eq!(
        assert_complete_contract_matrix(
            "pc",
            &options,
            pc_contract_command,
            pc_contract_case_is_valid,
        ),
        2_279
    );
}

#[test]
fn build_fixture_reaches_the_actual_parser_for_every_single_and_ordered_option_pair() {
    let options = [
        "height",
        "source",
        "hold",
        "aggregation",
        "rule",
        "spin-profile",
        "preserve-b2b",
        "dependency-dag",
        "finesse",
        "pattern-knowledge",
        "mirror",
        "backend",
        "fallback",
        "workers",
    ];
    assert_eq!(
        assert_complete_contract_matrix(
            "build",
            &options,
            build_contract_command,
            build_contract_case_is_valid,
        ),
        1_664
    );
}

#[test]
fn setup_fixture_reaches_the_actual_parser_for_every_single_and_ordered_option_pair() {
    let options = [
        "mode",
        "remaining",
        "qb",
        "queue-knowledge",
        "next-cycle-remaining",
        "post-cycle-borrow",
        "priority",
        "length",
        "max-setup-pieces",
        "rule",
        "tablebase",
        "workers",
    ];
    assert_eq!(
        assert_complete_contract_matrix(
            "setup",
            &options,
            setup_contract_command,
            setup_contract_case_is_valid,
        ),
        1_216
    );
}

#[test]
fn spin_structure_fixture_reaches_the_actual_parser_for_every_single_and_ordered_option_pair() {
    let options = [
        "inventory",
        "fill-bottom",
        "fill-top",
        "max-placements",
        "minimality",
        "rule",
        "spin-profile",
        "lines",
        "workers",
    ];
    assert_eq!(
        assert_complete_contract_matrix(
            "spin-finder",
            &options,
            spin_structure_contract_command,
            spin_structure_contract_case_is_valid,
        ),
        1_337
    );
}

#[test]
fn pc_build_and_spin_structure_invalid_fixture_values_reach_the_authoritative_parser() {
    let mut pc_cases = 0_usize;
    for option in [
        "lines",
        "source",
        "queue-knowledge",
        "spin-profile",
        "initial-b2b",
        "fallback",
        "workers",
        "gpu-device",
    ] {
        for value in forward_contract_invalid_values("pc", option) {
            let command = match (option, value) {
                ("lines", value) => format!("clearra pc --lines {value}"),
                ("source", "both") => {
                    "clearra pc --lines 4 --queue IOTSZJL --patterns P7".to_owned()
                }
                ("queue-knowledge", value) => {
                    format!("clearra pc --lines 4 --queue-knowledge {value}")
                }
                ("spin-profile", value) => {
                    format!("clearra pc --lines 4 --score --spin-profile {value}")
                }
                ("initial-b2b", value) => {
                    format!("clearra pc --lines 4 --score --initial-b2b {value}")
                }
                ("fallback", "allow+deny") => concat!(
                    "clearra pc --lines 4 --allow-backend-fallback ",
                    "--no-backend-fallback"
                )
                .to_owned(),
                ("workers", value) => format!("clearra pc --lines 4 --workers {value}"),
                ("gpu-device", value) => {
                    format!("clearra pc --lines 4 --gpu-device {value}")
                }
                _ => panic!("unmapped invalid PC fixture value {option}={value}"),
            };
            let error = project_contract_command(&command).expect_err(&command);
            assert_eq!(error.code(), WebCommandErrorCode::InvalidValue, "{command}");
            pc_cases += 1;
        }
    }

    let mut build_cases = 0_usize;
    for option in [
        "height",
        "source",
        "spin-profile",
        "finesse",
        "pattern-knowledge",
        "workers",
    ] {
        for value in forward_contract_invalid_values("build", option) {
            let base = "clearra build-probability --base-mask 0 --target-mask 15";
            let command = match (option, value) {
                ("height", value) => format!("{base} --height {value} --queue I"),
                ("source", "both") => {
                    format!("{base} --height 8 --queue I --patterns P1")
                }
                ("spin-profile", value) => {
                    format!("{base} --height 8 --queue I --aggregate spin --spin-profile {value}")
                }
                ("finesse", value) => {
                    format!("{base} --height 8 --queue I --finesse {value}")
                }
                ("pattern-knowledge", value) => format!(
                    "{base} --height 8 --queue I --finesse inputs --pattern-knowledge {value}"
                ),
                ("workers", value) => {
                    format!("{base} --height 8 --queue I --workers {value}")
                }
                _ => panic!("unmapped invalid build fixture value {option}={value}"),
            };
            let error = project_contract_command(&command).expect_err(&command);
            assert_eq!(error.code(), WebCommandErrorCode::InvalidValue, "{command}");
            build_cases += 1;
        }
    }

    let mut structure_cases = 0_usize;
    for option in [
        "inventory",
        "fill-bottom",
        "fill-top",
        "max-placements",
        "minimality",
        "spin-profile",
        "lines",
        "workers",
    ] {
        for value in forward_contract_invalid_values("spin-finder", option) {
            let command = match option {
                "inventory" => format!("clearra spin-structure --pieces {value}"),
                "fill-bottom" => format!(
                    "clearra spin-structure --pieces IOTSZJL --height 24 --fill-bottom {value} --fill-top 24"
                ),
                "fill-top" => format!(
                    "clearra spin-structure --pieces IOTSZJL --height 24 --fill-top {value}"
                ),
                "max-placements" => format!(
                    "clearra spin-structure --pieces IOTSZJL --max-placements {value}"
                ),
                "minimality" => format!(
                    "clearra spin-structure --pieces IOTSZJL --minimality {value}"
                ),
                "spin-profile" => format!(
                    "clearra spin-structure --pieces IOTSZJL --spin-profile {value}"
                ),
                "lines" => {
                    format!("clearra spin-structure --pieces IOTSZJL --lines {value}")
                }
                "workers" => {
                    format!("clearra spin-structure --pieces IOTSZJL --workers {value}")
                }
                _ => unreachable!(),
            };
            let error = project_contract_command(&command).expect_err(&command);
            assert_eq!(error.code(), WebCommandErrorCode::InvalidValue, "{command}");
            structure_cases += 1;
        }
    }

    assert_eq!(pc_cases, 10);
    assert_eq!(build_cases, 7);
    assert_eq!(structure_cases, 12);
}

#[test]
fn setup_invalid_fixture_values_reach_the_authoritative_parser() {
    let mut cases = 0_usize;
    for option in [
        "remaining",
        "qb",
        "queue-knowledge",
        "next-cycle-remaining",
        "max-setup-pieces",
        "workers",
    ] {
        for value in forward_contract_invalid_values("setup", option) {
            let command = match option {
                "remaining" => format!("clearra setup --remaining {value}"),
                "qb" => format!("clearra setup --mode qb --remaining I --qb {value}"),
                "queue-knowledge" => {
                    format!("clearra setup --remaining IOTSZJL --queue-knowledge {value}")
                }
                "next-cycle-remaining" => {
                    format!("clearra setup --remaining IOTSZJL --next-cycle-remaining {value}")
                }
                "max-setup-pieces" => {
                    format!("clearra setup --remaining IOTSZJL --max-setup-pieces {value}")
                }
                "workers" => format!("clearra setup --remaining IOTSZJL --workers {value}"),
                _ => unreachable!(),
            };
            let error = project_contract_command(&command).expect_err(&command);
            assert_eq!(error.code(), WebCommandErrorCode::InvalidValue, "{command}");
            cases += 1;
        }
    }
    assert_eq!(cases, 7);
}

#[test]
fn spin_structure_compiles_to_an_independent_typed_command() {
    let request = WebCommandParser::parse_with_worker_limit(
        "clearra spin-structure --pieces iOtSz --height 7 --fill-bottom 0 --fill-top 5 --spin-profile t-spins --lines 1+ --rule srs --minimality subset-minimal --workers 8 --use-all-cpu-threads",
        8,
    )
    .expect("spin-structure command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::SpinStructure(command) = request.command() else {
        panic!("expected AppCommand::SpinStructure");
    };
    let query = command.query();
    assert_eq!(query.inventory.total(), 5);
    assert_eq!(query.inventory.count(PieceKind::T), 1);
    assert_eq!(query.mode, SpinStructureMode::TSpins);
    assert_eq!(query.line_requirement, SpinLineRequirement::AtLeast(1));
    assert_eq!(query.height, 7);
    assert_eq!((query.fill_bottom, query.fill_top), (0, 5));
    assert_eq!(query.minimality, MinimalityPolicy::SubsetMinimal);
    assert_eq!(request.resource_budget().workers(), 8);
}

#[test]
fn spin_structure_accepts_all_six_profiles_without_using_forward_mode() {
    for mode in SpinStructureMode::ALL {
        let parsed = WebCommandParser::parse(&format!(
            "spin-structure --pieces TIO --spin-profile {}",
            mode.as_str()
        ))
        .expect("profile");
        assert!(parsed.forward_search_query().is_none());
        assert_eq!(
            parsed.spin_structure_query().expect("structure query").mode,
            mode
        );
    }

    for invalid in ["disabled", "t-spin-simple", "unknown"] {
        let error = WebCommandParser::parse(&format!(
            "spin-structure --pieces T --spin-profile {invalid}"
        ))
        .expect_err("invalid structure profile");
        assert_eq!(error.code(), WebCommandErrorCode::InvalidValue);
    }
}

#[test]
fn spin_structure_preserves_a_canonical_wide_board_and_rejects_conflicts() {
    let mask = format!("8{}", "0".repeat(59));
    let request =
        WebCommandParser::parse(&format!("spin-structure --board-mask-v1 {mask} --pieces T"))
            .expect("wide structure board");
    let query = request.spin_structure_query().expect("structure query");
    assert_eq!(query.height, 24);
    assert_eq!(query.initial_board.words(), [0, 0, 0, 1_u64 << 47]);

    let error = WebCommandParser::parse(&format!(
        "spin-structure --board-mask 0 --board-mask-v1 {} --pieces T",
        "0".repeat(60)
    ))
    .expect_err("conflicting board options");
    assert_eq!(error.code(), WebCommandErrorCode::InvalidValue);
}
// SRP rationale: this module has one behavior-level change reason: verifying the complete public
// web command grammar reaches the intended typed Clearra request contracts.
