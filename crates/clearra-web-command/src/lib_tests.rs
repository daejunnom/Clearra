use clearra_app::AppCommand;
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_forward_search::{ForwardSearchMode, ForwardSpinCategory};
use clearra_pc_graph::request::SupplyWindowSize;
use clearra_scoring::profile::SpinProfileId;
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
        .expect("parsed command")
        .to_app_request()
        .expect_err("QB mode without observations must fail");

    assert_eq!(error.code(), WebCommandErrorCode::MissingValue);
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
