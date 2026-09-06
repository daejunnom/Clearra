use clearra_app::{
    AppCommand, BuildProbabilityResultMode, PcChanceIngressOrigin, PcFailedQueueIngressOrigin,
    PcMinimalsIngressOrigin, PcPathIngressOrigin, PcResultProjection, PcSaveIngressOrigin,
    PcScoreIngressOrigin, PcScoreMinimalsIngressOrigin, PcTilingIngressOrigin,
    ProductCapabilityContract, SpinStructureProductMode, PC_SCORE_MAX_PATTERNS,
    PC_SCORE_MAX_PATTERN_BYTES, PC_SCORE_MAX_SOURCE_PIECES,
};
use clearra_core_domain::{
    objective::{objective_kind::ObjectiveKind, tie_policy::TiePolicy, trace_policy::TracePolicy},
    piece::{piece_kind::PieceKind, rotation::RotationState},
    solution::StandardBoard64ColoredTilingIdentity,
};
use clearra_forward_search::{ForwardSearchMode, ForwardSpinCategory, MAX_REN_QUEUE_PIECES};
use clearra_objectives::policy::{
    objective_policy::ObjectivePolicy,
    score_objective_policy::{ScoreObjectiveMode, ScoreProfileSelection, SpinProfileSelection},
};
use clearra_pc_graph::request::{
    PcCountPolicy, RequestedSearchBackend, SupplyWindowSize, WorkerPolicy,
};
use clearra_problem::{
    BuildProbabilityAggregation, BuildSolutionProbabilityPolicy, FinessePlacement,
    FinesseScoreRequest, PcQuery,
};
use clearra_scoring::profile::SpinProfileId;
use clearra_spin_structure_search::{MinimalityPolicy, SpinLineRequirement, SpinStructureMode};
use clearra_supply::QueueObservationPolicy;

use super::*;
use crate::web_command_parser::{PC_SCORE_MAX_ARGUMENT_BYTES, PC_SCORE_MAX_ARGUMENT_TOKENS};

#[test]
fn request_structural_profiles_are_request_local_and_canonical() {
    let parsed = CliCommandParser::parse(
        "clearra pc --lines 2 --backend cpu --board-profile standard-10 --piece-profile standard-tetrominoes --bag-profile standard-7-bag",
    )
    .expect("canonical structural profiles");
    let profiles = parsed.request_structural_profiles();
    assert_eq!(profiles.board().as_str(), "standard-10");
    assert_eq!(profiles.piece_set().as_str(), "standard-tetrominoes");
    assert_eq!(profiles.bag().as_str(), "standard-7-bag");
    let app_request = parsed.to_app_request().expect("typed AppRequest");
    assert_eq!(app_request.request_profiles().structural(), profiles);
}

#[test]
fn request_structural_profiles_reject_unknown_or_duplicate_values_without_fallback() {
    for command in [
        "clearra pc --lines 2 --board-profile wide-10",
        "clearra pc --lines 2 --piece-profile pentominoes",
        "clearra pc --lines 2 --bag-profile history-6-rolls",
        "clearra pc --lines 2 --bag-profile standard-7-bag --bag-profile standard-7-bag",
    ] {
        let error = CliCommandParser::parse(command).expect_err("profile must fail closed");
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
    }
}

#[test]
fn request_semantic_profiles_reject_unverified_or_unknown_values_without_fallback() {
    for command in [
        "clearra pc --lines 2 --rule custom",
        "clearra pc --lines 2 --spin-profile unverified-spin",
        "clearra pc --lines 2 --score-profile classic-score",
    ] {
        let result = CliCommandParser::parse(command).and_then(|request| request.to_app_request());
        assert!(result.is_err(), "{command} must fail closed");
    }
}

#[test]
fn wasm_command_compiles_to_app_request() {
    let request = CliCommandParser::parse("clearra pc --lines 2 --backend cpu")
        .expect("CLI command")
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
fn pc_minimals_canonical_and_internal_alias_bind_one_typed_v2_contract() {
    let canonical = CliCommandParser::parse(
        "clearra pc minimals --lines 1 --board-mask 0x3f --height 1 --pieces 1 --queue I --hold empty --rule srs-plus",
    )
    .expect("canonical pc minimals")
    .to_app_request()
    .expect("typed pc minimals AppRequest");
    assert_eq!(
        canonical.product_capability_contract(),
        Some(ProductCapabilityContract::PcMinimals)
    );
    let AppCommand::Scenario(command) = canonical.command() else {
        panic!("expected typed pc minimals scenario");
    };
    assert_eq!(
        command.result_projection(),
        PcResultProjection::MinimumCoverV2(PcMinimalsIngressOrigin::CanonicalPcMinimals)
    );
    assert_eq!(
        command.query().objective().kind(),
        ObjectiveKind::MinimumCover
    );
    assert!(!command.query().objective().score().requested());
    assert_eq!(
        command.query().queue_observation_policy(),
        QueueObservationPolicy::FullQueueOracle
    );
    assert_eq!(
        command.query().count_policy(),
        PcCountPolicy::CountUnique,
        "pc.minimals owns normalized field/coverage identity, not build-path multiplicity"
    );

    let alias_tokens =
        "clearra sfinder minimals --field-mask-v1 000000000000003f --queue I --lines 1"
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
    let alias = CliCommandParser::parse_tokens_internal_typed_candidate(&alias_tokens)
        .expect("internal minimals alias")
        .to_app_request()
        .expect("typed alias AppRequest");
    assert_eq!(alias, canonical);

    let generic = CliCommandParser::parse(
        "clearra pc --lines 1 --board-mask 0x3f --height 1 --pieces 1 --queue I --hold empty --objective minimum-cover",
    )
    .expect("generic advanced minimum-cover")
    .to_app_request()
    .expect("generic PC AppRequest");
    assert_eq!(generic.product_capability_contract(), None);
}

#[test]
fn pc_minimals_four_line_opening_and_scenario_share_normalized_identity_semantics() {
    let opening = CliCommandParser::parse("clearra pc minimals --lines 4")
        .expect("canonical four-line opening pc minimals")
        .to_app_request()
        .expect("typed opening pc minimals AppRequest");
    let scenario = CliCommandParser::parse(
        "clearra pc minimals --lines 4 --board-mask 0 --height 4 --pieces 10 --hold empty",
    )
    .expect("canonical four-line scenario pc minimals")
    .to_app_request()
    .expect("typed scenario pc minimals AppRequest");

    assert_eq!(
        opening.product_capability_contract(),
        Some(ProductCapabilityContract::PcMinimals)
    );
    assert_eq!(
        scenario.product_capability_contract(),
        Some(ProductCapabilityContract::PcMinimals)
    );

    let AppCommand::Pc(opening) = opening.command() else {
        panic!("expected opening-backed pc.minimals command");
    };
    let AppCommand::Scenario(scenario) = scenario.command() else {
        panic!("expected scenario-backed pc.minimals command");
    };
    let normalized_opening = PcQuery::from_opening_query(opening.query());

    assert_eq!(
        normalized_opening.count_policy(),
        PcCountPolicy::CountUnique,
        "the canonical product count policy must survive opening-preset lowering"
    );
    assert_eq!(scenario.query().count_policy(), PcCountPolicy::CountUnique);
    assert_eq!(
        normalized_opening.objective().kind(),
        ObjectiveKind::MinimumCover
    );
    assert_eq!(
        scenario.query().objective().kind(),
        ObjectiveKind::MinimumCover
    );
    assert_eq!(
        normalized_opening.queue(),
        scenario.query().remaining_queue()
    );
    assert_eq!(normalized_opening.exact_piece_count(), 10);
    assert_eq!(scenario.query().exact_pieces(), Some(10));
}

#[test]
fn pc_minimals_rejects_semantic_overrides_and_unaccounted_memory_caps() {
    for suffix in [
        "--objective minimum-cover",
        "--count all",
        "--score",
        "--tiling-only",
        "--queue-knowledge visible-7",
        "--max-memory-mib 64",
        "--tablebase",
        "--precompute-build-dependencies",
    ] {
        let command = format!(
            "clearra pc minimals --lines 1 --board-mask 0x3f --height 1 --pieces 1 --queue I --hold empty {suffix}"
        );
        let error = CliCommandParser::parse(&command).expect_err(&command);
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
    }
}

#[test]
fn pc_path_binds_the_complete_replay_family_contract_without_score_or_ties() {
    let source = concat!(
        "clearra pc path --lines 1 --board-mask 0x3f0 --height 1 ",
        "--pieces 1 --queue I --hold empty --rule srs-plus"
    );
    let parsed = CliCommandParser::parse(source).expect(source);
    assert_eq!(
        parsed.product_capability_contract(),
        Some(ProductCapabilityContract::PcPath)
    );
    assert_eq!(
        parsed.pc_result_projection(),
        PcResultProjection::PathFamilyV2(PcPathIngressOrigin::CanonicalPcPath)
    );

    let request = parsed.to_app_request().expect("typed pc.path AppRequest");
    assert_eq!(
        request.product_capability_contract(),
        Some(ProductCapabilityContract::PcPath)
    );
    let AppCommand::Scenario(command) = request.command() else {
        panic!("expected scenario-backed pc.path command");
    };
    assert_eq!(command.query().count_policy(), PcCountPolicy::CountAll);
    assert_eq!(command.query().objective(), ObjectivePolicy::all());
    assert!(!command.query().objective().score().requested());
    assert_eq!(command.query().retained_trace_limit(), 1);

    let generic = CliCommandParser::parse(
        "clearra pc --lines 1 --board-mask 0x3f0 --height 1 --pieces 1 --queue I --hold empty --objective all --count all",
    )
    .expect("generic all-path-capable request");
    assert_eq!(generic.product_capability_contract(), None);
    assert_eq!(generic.pc_result_projection(), PcResultProjection::Standard);
}

#[test]
fn pc_path_rejects_semantic_and_resource_overrides() {
    for suffix in [
        "--objective all",
        "--count all",
        "--score",
        "--tiling-only",
        "--solution-probabilities",
        "--queue-knowledge visible-7",
        "--max-memory-mib 64",
        "--tablebase",
        "--precompute-build-dependencies",
    ] {
        let source = format!(
            "clearra pc path --lines 1 --board-mask 0x3f0 --height 1 --pieces 1 --queue I --hold empty {suffix}"
        );
        assert_eq!(
            CliCommandParser::parse(&source).expect_err(&source).code(),
            CliCommandErrorCode::InvalidValue,
            "{source}"
        );
    }
}

#[test]
fn cli_build_queue_shapes_have_measurable_typed_request_memory() {
    for command in [
        "clearra build-probability --base-mask 0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror --max-memory-mib 1",
        "clearra build-probability --base-mask 0 --target-mask 0xf --height 4 --patterns I --no-hold --no-mirror --max-memory-mib 1",
        "clearra build-probability --base-mask 0 --target-mask 0xf --height 4 --no-hold --no-mirror --max-memory-mib 1",
    ] {
        let request = CliCommandParser::parse(command)
            .expect("CLI Build command")
            .to_app_request()
            .expect("typed Build AppRequest");

        assert!(
            request
                .checked_build_probability_retained_capacity_bytes()
                .is_some(),
            "CLI Build ingress must remain within the measured queue contract: {command}"
        );
    }
}

#[test]
fn bare_cli_pc_preserves_shared_surface_defaults_through_app_projection() {
    let parsed = CliCommandParser::parse("clearra pc").expect("bare web pc command");
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
    let request = CliCommandParser::parse(
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
    let request = CliCommandParser::parse("clearra percent IOT --bag-aligned --min-len 3")
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
    let request = CliCommandParser::parse("clearra percent IOT --observed --min-len 1")
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
    let request = CliCommandParser::parse(
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
fn public_failed_queue_spellings_remain_whole_request_equivalent_and_generic() {
    let suffix = "--lines 2 --patterns P5 --backend cpu --failed-count 4";
    let hyphen = CliCommandParser::parse(&format!("clearra failed-queue {suffix}"))
        .expect("public failed-queue")
        .to_app_request()
        .expect("public failed-queue AppRequest");
    let underscore = CliCommandParser::parse(&format!("clearra failed_queue {suffix}"))
        .expect("public failed_queue")
        .to_app_request()
        .expect("public failed_queue AppRequest");
    let internal_hyphen =
        CliCommandParser::parse_internal_typed_candidate(&format!("clearra failed-queue {suffix}"))
            .expect("internal hyphen remains legacy")
            .to_app_request()
            .expect("internal hyphen legacy AppRequest");

    assert_eq!(hyphen, underscore);
    assert_eq!(hyphen, internal_hyphen);
    assert_eq!(hyphen.product_capability_contract(), None);
    let AppCommand::Percent(command) = hyphen.command() else {
        panic!("legacy failed-queue lowers to Percent");
    };
    assert!(command.is_failed_queue());
    assert_eq!(command.pc_failed_queue_origin(), None);
}

#[test]
fn canonical_and_internal_underscore_failed_queue_preserve_closed_typed_origins() {
    let suffix = "--lines 2 --patterns P5 --backend cpu --failed-count 4";
    for (source, internal, expected_origin) in [
        (
            format!("clearra pc failed-queue {suffix}"),
            false,
            PcFailedQueueIngressOrigin::CanonicalFailedQueue,
        ),
        (
            format!("clearra failed_queue {suffix}"),
            true,
            PcFailedQueueIngressOrigin::CompatibilityFailedQueueUnderscore,
        ),
    ] {
        let parsed = if internal {
            CliCommandParser::parse_internal_typed_candidate(&source).expect(&source)
        } else {
            CliCommandParser::parse(&source).expect(&source)
        };
        assert_eq!(parsed.command_kind(), "failed-queue", "{source}");
        assert_eq!(
            parsed.pc_failed_queue_origin(),
            Some(expected_origin),
            "{source}"
        );
        assert_eq!(
            parsed.product_capability_contract(),
            Some(ProductCapabilityContract::PcFailedQueue),
            "{source}"
        );

        let request = parsed.to_app_request().expect(&source);
        assert_eq!(
            request.product_capability_contract(),
            Some(ProductCapabilityContract::PcFailedQueue),
            "{source}"
        );
        let AppCommand::Percent(command) = request.command() else {
            panic!("typed failed-queue lowers to Percent: {source}");
        };
        assert_eq!(command.pc_failed_queue_origin(), Some(expected_origin));
        assert_eq!(command.failed_pattern_limit(), 4);
    }
}

#[test]
fn pc_failed_queue_rejects_underscore_and_unbound_typed_state() {
    let error = CliCommandParser::parse("clearra pc failed_queue --lines 2 --patterns P5")
        .expect_err("pc failed_queue is not canonical");
    assert_eq!(error.code(), CliCommandErrorCode::UnsupportedCommand);
    assert!(error
        .message()
        .contains("not an authorized canonical spelling"));

    let unbound = CliCommandParser::parse("clearra failed-queue --lines 2 --patterns P5")
        .expect("legacy failed-queue")
        .with_product_capability_contract_for_test(ProductCapabilityContract::PcFailedQueue);
    let error = unbound
        .to_app_request()
        .expect_err("unbound failed-queue product claim");
    assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
    assert!(error
        .message()
        .contains("requires a closed failed-queue origin"));
}

#[test]
fn failed_queue_keeps_b2b_constraints_without_enabling_scoring() {
    let request = CliCommandParser::parse(
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
        let error = CliCommandParser::parse(&command).expect_err(option);
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
    }
}

#[test]
fn tiling_only_preserves_hold_supply_projection() {
    let request = CliCommandParser::parse(
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
        let error = CliCommandParser::parse(&command).expect_err(option);
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
    }
}

#[test]
fn shared_cli_tablebase_is_opt_in_for_pc_and_setup() {
    let default_pc = CliCommandParser::parse("clearra pc --lines 4 --backend cpu")
        .expect("default PC command")
        .to_app_request()
        .expect("default PC request");
    let AppCommand::Pc(command) = default_pc.command() else {
        panic!("expected AppCommand::Pc");
    };
    assert!(!command.query().execution_policy().tablebase_requested());

    let enabled_pc = CliCommandParser::parse("clearra pc --lines 4 --backend cpu --tb")
        .expect("TB PC command")
        .to_app_request()
        .expect("TB PC request");
    let AppCommand::Pc(command) = enabled_pc.command() else {
        panic!("expected AppCommand::Pc");
    };
    assert!(command.query().execution_policy().tablebase_requested());

    let enabled_setup = CliCommandParser::parse("clearra setup --remaining IOTSZJL --tablebase")
        .expect("TB setup command")
        .to_app_request()
        .expect("TB setup request");
    let AppCommand::Setup(command) = enabled_setup.command() else {
        panic!("expected AppCommand::Setup");
    };
    assert!(command.query().tablebase_requested());

    let disabled_setup =
        CliCommandParser::parse("clearra setup --remaining IOTSZJL --no-tablebase")
            .expect("disabled TB setup command")
            .to_app_request()
            .expect("disabled TB setup request");
    let AppCommand::Setup(command) = disabled_setup.command() else {
        panic!("expected AppCommand::Setup");
    };
    assert!(!command.query().tablebase_requested());
}

#[test]
fn shared_cli_build_dependency_dag_is_opt_in_for_pc() {
    let default_pc = CliCommandParser::parse("clearra pc --lines 4 --backend cpu")
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
        CliCommandParser::parse("clearra pc --lines 4 --backend cpu --build-dependency-dag")
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
        CliCommandParser::parse("clearra pc --lines 4 --backend cpu --no-build-dependency-dag")
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
    let request = CliCommandParser::parse("clearra pc --lines 4 --patterns P7P4")
        .expect("CLI command")
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
        CliCommandParser::parse("clearra pc --lines 4 --patterns P7P4 --queue-knowledge visible-7")
            .expect("CLI command")
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
    let request = CliCommandParser::parse(
        "clearra pc --lines 4 --patterns P7P4 --board-mask 0 \
         --height 4 --pieces 10 --queue-knowledge visible-7",
    )
    .expect("CLI command")
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
        let error = CliCommandParser::parse(command)
            .expect_err("visible-7 minimum-cover must fail before AppRequest construction");
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
        assert!(error
            .message()
            .contains("visible-seven-minimum-cover-unsupported"));
    }
}

#[test]
fn opening_pc_command_preserves_observed_source_piece_count() {
    let request = CliCommandParser::parse("clearra pc --lines 4 --backend cpu --source-pieces 10")
        .expect("CLI command")
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
    let request = CliCommandParser::parse("clearra setup --remaining IOTS")
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
        CliCommandParser::parse("clearra setup --remaining IOT --allow-post-cycle-borrow")
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
    let request = CliCommandParser::parse(
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
        CliCommandParser::parse("clearra setup --remaining IOTS --queue-knowledge visible-7")
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
        let error = CliCommandParser::parse(command).expect_err("invalid policy must fail");
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
    }
}

#[test]
fn setup_command_preserves_single_remaining_piece_as_a_guaranteed_prefix() {
    let request = CliCommandParser::parse("clearra setup --remaining I")
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
        let request = CliCommandParser::parse(&format!(
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
fn canonical_setup_ranked_family_paths_fix_their_typed_priority() {
    for (family, expected) in [
        ("joint", clearra_problem::SetupCandidatePriority::All),
        (
            "build",
            clearra_problem::SetupCandidatePriority::BuildProbabilityFirst,
        ),
        (
            "pc",
            clearra_problem::SetupCandidatePriority::PcProbabilityFirst,
        ),
    ] {
        let request = CliCommandParser::parse(&format!(
            "clearra setup {family} --remaining IOTS --priority {}",
            expected.keyword()
        ))
        .expect("canonical setup ranked family")
        .to_app_request()
        .expect("typed Setup AppRequest");
        let AppCommand::Setup(command) = request.command() else {
            panic!("expected AppCommand::Setup");
        };
        assert_eq!(command.query().candidate_priority(), expected);
        assert!(command.query().path_detail().is_none());
    }
}

#[test]
fn canonical_setup_ranked_family_paths_reject_cross_family_and_path_detail_options() {
    for command in [
        "clearra setup joint --remaining IOTS --priority build",
        "clearra setup build --remaining IOTS --priority pc",
        "clearra setup pc --remaining IOTS --priority all",
        "clearra setup joint --remaining IOTS --paths-for 0x1 --condition hold-empty",
    ] {
        let error = CliCommandParser::parse(command).expect_err("closed family must reject drift");
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
    }
}

fn setup_score_ctk3_document() -> String {
    let mut cells = vec![clearra_app::Ctk3Color::Empty; 20];
    cells[0..4].fill(clearra_app::Ctk3Color::Piece(clearra_app::Ctk3Piece::I));
    clearra_app::encode_ctk3_compact(&clearra_app::Ctk3Document::new(
        10,
        vec![clearra_app::Ctk3Page::new(2, cells)],
    ))
    .expect("canonical Setup score CTK3 fixture")
}

#[test]
fn canonical_setup_score_lowers_to_the_nominal_app_command() {
    let document = setup_score_ctk3_document();
    let request = CliCommandParser::parse_with_worker_limit(
        &format!(
            "clearra setup score --document-format ctk3 --document {document} --setup-queue I --solution-queue OTSJ --clear 2 --no-hold --score-profile guideline --initial-b2b 2 --rule srs --max-patterns 64 --workers 2 --backend cpu --no-backend-fallback"
        ),
        4,
    )
    .expect("canonical Setup score command")
    .to_app_request()
    .expect("typed Setup score AppRequest");

    let AppCommand::SetupScore(command) = request.command() else {
        panic!("expected AppCommand::SetupScore");
    };
    assert_eq!(
        command.document_format(),
        clearra_app::FieldDocumentFormat::Ctk3
    );
    assert_eq!(command.source_page_count(), 1);
    assert_eq!(command.score_profile(), ScoreProfileSelection::Guideline);
    assert_eq!(command.initial_b2b(), 2);
    assert_eq!(request.resource_budget().workers(), 2);
}

#[test]
fn canonical_setup_score_accepts_the_two_independent_pattern_sources() {
    let document = setup_score_ctk3_document();
    let request = CliCommandParser::parse(&format!(
        "clearra setup score --document-format ctk3 --document {document} --setup-patterns P1 --solution-patterns P4 --clear-height 2 --hold"
    ))
    .expect("pattern Setup score command")
    .to_app_request()
    .expect("typed Setup score AppRequest");
    assert!(matches!(request.command(), AppCommand::SetupScore(_)));
}

#[test]
fn canonical_setup_score_rejects_cross_family_and_ungoverned_options() {
    let document = setup_score_ctk3_document();
    let base = format!(
        "clearra setup score --document-format ctk3 --document {document} --setup-queue I --solution-queue OTSJ --clear 2"
    );
    for suffix in [
        "--setup-patterns P1",
        "--solution-patterns P4",
        "--queue I",
        "--patterns P4",
        "--objective score",
        "--queue-knowledge oracle",
        "--source-pieces 4",
        "--max-memory-mib 64",
        "--backend gpu",
        "--allow-backend-fallback",
        "--gpu-device 0",
        "--hold --no-hold",
    ] {
        let command = format!("{base} {suffix}");
        let error = CliCommandParser::parse(&command).expect_err(&command);
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
    }
}

#[test]
fn setup_command_preserves_setup_length_preference() {
    for (keyword, expected) in [
        ("auto", clearra_problem::SetupLengthPreference::Auto),
        ("longer", clearra_problem::SetupLengthPreference::Longer),
        ("shorter", clearra_problem::SetupLengthPreference::Shorter),
    ] {
        let request = CliCommandParser::parse(&format!(
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
    let request = CliCommandParser::parse("clearra setup --remaining IOTS --rule srs-x")
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
    let request = CliCommandParser::parse("clearra setup --remaining IOTS --rule jstris-180")
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
    let request = CliCommandParser::parse("clearra setup --remaining IOTS --max-setup-pieces 10")
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
    let request = CliCommandParser::parse("clearra setup --remaining TI --mode qb --qb OS")
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
        CliCommandParser::parse("clearra setup --remaining TI --next-cycle-remaining OOSITZ")
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
    let request = CliCommandParser::parse(
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
    let request = CliCommandParser::parse(
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

    assert_eq!(detail.board_mask(), 0x0000_0807_19e6);
    assert_eq!(detail.deleted_rows(), 3);
    assert_eq!(detail.placement_rows(), 1);
    assert_eq!(detail.condition_id(), "hold-empty");
}

#[test]
fn setup_command_requires_both_path_detail_options() {
    let error = CliCommandParser::parse(
        "clearra setup --remaining TI \
         --paths-for setup-00080719e6-0000-000000000000000000000000000001",
    )
    .expect_err("missing condition must fail");

    assert_eq!(error.code(), CliCommandErrorCode::MissingValue);
}

#[test]
fn setup_command_requires_observed_pieces_in_queue_based_mode() {
    let error = CliCommandParser::parse("clearra setup --remaining TI --mode qb")
        .expect_err("QB mode without observations must fail at the parser boundary");

    assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
}

#[test]
fn setup_command_separates_one_duplicate_as_automatic_initial_hold_without_changing_cycle() {
    let request = CliCommandParser::parse("clearra setup --remaining IOTSS")
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
            CliCommandParser::parse(command).expect_err("oracle and QB observations must conflict");
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
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
        let error = CliCommandParser::parse(command).expect_err(command);
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
    }

    for command in [
        "clearra setup --remaining TI --mode qb --qb OS --next-cycle-remaining OOSITZ",
        "clearra setup --remaining TI --next-cycle-remaining OOSITZ --qb OS --mode qb",
        "clearra setup --remaining IOT --allow-post-cycle-borrow --next-cycle-remaining IOTSZJL",
        "clearra setup --next-cycle-remaining IOTSZJL --allow-post-cycle-borrow --remaining IOT",
    ] {
        CliCommandParser::parse(command)
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
                            match CliCommandParser::parse(&command) {
                                Ok(parsed) if expected_valid => {
                                    parsed.to_app_request().unwrap_or_else(|error| {
                                        panic!("failed to project '{command}': {error:?}")
                                    });
                                }
                                Err(error) if !expected_valid => {
                                    assert_eq!(
                                        error.code(),
                                        CliCommandErrorCode::InvalidValue,
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
    let error = CliCommandParser::parse("clearra setup --remaining IOTSZJL --online-policy")
        .expect_err("unfinished online policy must not enter the product contract");

    assert_eq!(error.code(), CliCommandErrorCode::UnsupportedCommand);
}

#[test]
fn setup_command_keeps_initial_hold_on_the_cli_only_surface() {
    let error = CliCommandParser::parse("clearra setup --remaining SIOS --initial-hold S")
        .expect_err("product CLI command must not expose the CLI-only initial hold");

    assert_eq!(error.code(), CliCommandErrorCode::UnsupportedCommand);
}

#[test]
fn setup_cli_command_defaults_to_reserved_multithread_worker_count() {
    let request = CliCommandParser::parse_with_worker_limit("clearra setup --remaining IOTS", 12)
        .expect("setup command")
        .to_app_request()
        .expect("AppRequest");

    assert_eq!(request.resource_budget().workers(), 11);
}

#[test]
fn pc_back_to_back_preservation_is_an_execution_constraint_not_a_score_request() {
    let request = CliCommandParser::parse(
        "clearra pc --lines 2 --queue IOTSL --preserve-b2b --spin-profile all-mini-plus",
    )
    .expect("CLI command")
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
    let request = CliCommandParser::parse(
        "clearra pc --lines 2 --queue IOTSL --score --score-profile jstris-ultra --rule srs-plus",
    )
    .expect("CLI command")
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
    let request = CliCommandParser::parse(
        "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror --preserve-b2b --spin-profile t-spins-plus",
    )
    .expect("CLI command")
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
fn build_probability_result_aggregation_is_cli_owned_and_independent_from_engine_aggregation() {
    let base = "clearra build-probability --base-mask 0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror --aggregate buildability";
    for (suffix, expected) in [
        (
            " --result-mode complete-replay-paths",
            BuildProbabilityResultMode::CompleteReplayPaths,
        ),
        (
            " --result-mode field-average-score --score-profile guideline --initial-b2b 7",
            BuildProbabilityResultMode::FieldAverageScore,
        ),
        (
            " --result-mode fixed-queue-maximum-score --score-profile guideline --initial-b2b 7",
            BuildProbabilityResultMode::FixedQueueMaximumScore,
        ),
        (
            " --result-mode highest-score-minimum-set --score-profile guideline --initial-b2b 7",
            BuildProbabilityResultMode::HighestScoreMinimumSet,
        ),
    ] {
        let request = CliCommandParser::parse_with_worker_limit(&format!("{base}{suffix}"), 8)
            .expect("Build result aggregation command")
            .to_app_request()
            .expect("Build result aggregation AppRequest");
        let AppCommand::BuildProbability(command) = request.command() else {
            panic!("expected AppCommand::BuildProbability");
        };
        assert_eq!(command.result_mode(), expected);
        assert_eq!(
            command.query().aggregation(),
            BuildProbabilityAggregation::Buildability
        );
        assert!(command.query().core_query().objective().score().requested());
        assert_eq!(request.resource_budget().workers(), 7);
        assert_eq!(command.query().core_query().execution_policy().workers(), 7);
    }

    for suffix in [
        "--tiling-only --result-mode complete-replay-paths",
        "--tiling-only --result-mode field-average-score",
        "--result-mode complete-replay-paths --score-profile tetrio",
    ] {
        assert!(
            CliCommandParser::parse(&format!(
                "clearra build-probability --base-mask 0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror {suffix}"
            ))
            .is_err(),
            "{suffix}"
        );
    }
}

#[test]
fn build_replay_and_score_occupied_height_is_bounded_before_execution() {
    for mode in [
        "complete-replay-paths",
        "field-average-score",
        "fixed-queue-maximum-score",
        "highest-score-minimum-set",
    ] {
        let command = format!("clearra build-probability --base-mask 0 --target-mask 0xf --height 8 --queue I --no-hold --no-mirror --result-mode {mode}");
        let request = CliCommandParser::parse(&command)
            .expect("empty display rows are allowed")
            .to_app_request()
            .expect("compact App query");
        let AppCommand::BuildProbability(build) = request.command() else {
            panic!("Build query");
        };
        assert!(build.query().field().is_compact());
        for (base, target) in [("0", "0xf000000000000000"), ("0x1000000000000000", "0xf")] {
            let error = CliCommandParser::parse(&format!("clearra build-probability --base-mask {base} --target-mask {target} --height 8 --queue I --no-hold --no-mirror --result-mode {mode}"))
                .expect_err("occupied seventh row must fail before Geometry");
            assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
            assert!(error.message().contains("bottom six rows"));
        }
    }
}

#[test]
fn build_queue_knowledge_is_closed_and_reaches_the_actual_build_query() {
    let base = "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror";
    for (suffix, expected) in [
        ("", QueueObservationPolicy::FullQueueOracle),
        (
            " --queue-knowledge oracle",
            QueueObservationPolicy::FullQueueOracle,
        ),
        (
            " --queue-knowledge visible-7",
            QueueObservationPolicy::VisibleSeven,
        ),
    ] {
        let request = CliCommandParser::parse(&format!("{base}{suffix}"))
            .expect("Build queue-knowledge command")
            .to_app_request()
            .expect("Build AppRequest");
        let AppCommand::BuildProbability(command) = request.command() else {
            panic!("expected AppCommand::BuildProbability");
        };
        assert_eq!(command.query().queue_observation_policy(), expected);
        assert_eq!(
            command.query().core_query().queue_observation_policy(),
            expected
        );
    }

    for rejected in ["full-future", "seven-visible", "online", "visible-seven"] {
        let error = CliCommandParser::parse(&format!("{base} --queue-knowledge {rejected}"))
            .expect_err(rejected);
        assert_eq!(
            error.code(),
            CliCommandErrorCode::InvalidValue,
            "{rejected}"
        );
    }
    let duplicate = CliCommandParser::parse(&format!(
        "{base} --queue-knowledge oracle --queue-knowledge oracle"
    ))
    .expect_err("duplicate Build queue knowledge");
    assert_eq!(duplicate.code(), CliCommandErrorCode::InvalidValue);
}

#[test]
fn build_visible_seven_rejects_tiling_before_app_execution() {
    let base = "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror";
    for command in [
        format!("{base} --tiling-only --queue-knowledge visible-7"),
        format!("{base} --queue-knowledge visible-7 --tiling-only"),
    ] {
        let error = CliCommandParser::parse(&command).expect_err(&command);
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
        assert!(error.message().contains("tiling-only"));
    }

    let error = CliCommandRequest::build_probability(
        WebBuildProbabilityInput::new(0, 0xf, 4)
            .with_aggregation(BuildProbabilityAggregation::TilingOnly),
    )
    .with_queue("I")
    .with_hold_enabled(false)
    .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven)
    .to_app_request()
    .expect_err("programmatic visible-seven tiling");
    assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
}

#[test]
fn build_probability_dependency_dag_is_opt_in_and_reaches_the_core_policy() {
    let enabled = CliCommandParser::parse(
        "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror --build-dependency-dag",
    )
    .expect("CLI command")
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

    let disabled = CliCommandParser::parse(
        "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror",
    )
    .expect("CLI command")
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
fn build_probability_memory_limit_is_one_exact_query_and_app_request_authority() {
    let finite = CliCommandParser::parse(
        "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 \
         --queue I --no-hold --no-mirror --workers 2 --max-memory-mib 64",
    )
    .expect("finite Build CLI command")
    .to_app_request()
    .expect("finite Build AppRequest");
    let AppCommand::BuildProbability(command) = finite.command() else {
        panic!("expected AppCommand::BuildProbability");
    };
    assert_eq!(
        command
            .query()
            .core_query()
            .execution_policy()
            .max_memory_mib(),
        Some(64)
    );
    assert_eq!(finite.resource_budget().max_memory_mib(), Some(64));
    assert_eq!(finite.resource_budget().memory_mib(), Some(64));

    let unlimited = CliCommandParser::parse(
        "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 \
         --queue I --no-hold --no-mirror",
    )
    .expect("unlimited Build CLI command")
    .to_app_request()
    .expect("unlimited Build AppRequest");
    let AppCommand::BuildProbability(command) = unlimited.command() else {
        panic!("expected AppCommand::BuildProbability");
    };
    assert_eq!(
        command
            .query()
            .core_query()
            .execution_policy()
            .max_memory_mib(),
        None
    );
    assert_eq!(unlimited.resource_budget().max_memory_mib(), None);
    assert_eq!(unlimited.resource_budget().memory_mib(), None);
}

#[test]
fn build_probability_memory_limit_rejects_lossy_app_authority_projection() {
    let command = format!(
        "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 \
         --queue I --no-hold --no-mirror --max-memory-mib {}",
        u64::from(u32::MAX) + 1
    );
    let error = CliCommandParser::parse(&command)
        .expect("the CLI parser accepts the u64 option domain")
        .to_app_request()
        .expect_err("the App request authority must not truncate the memory limit");
    assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
    assert!(error.message().contains("authority range"));
}

#[test]
fn build_probability_solution_probabilities_are_opt_in_and_reach_the_typed_query() {
    let base = "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror";
    let included = CliCommandParser::parse(&format!("{base} --solution-probabilities"))
        .expect("CLI command")
        .to_app_request()
        .expect("AppRequest");
    let AppCommand::BuildProbability(included) = included.command() else {
        panic!("expected AppCommand::BuildProbability");
    };
    assert_eq!(
        included.query().solution_probability_policy(),
        BuildSolutionProbabilityPolicy::Include
    );

    let omitted = CliCommandParser::parse(base)
        .expect("CLI command")
        .to_app_request()
        .expect("AppRequest");
    let AppCommand::BuildProbability(omitted) = omitted.command() else {
        panic!("expected AppCommand::BuildProbability");
    };
    assert_eq!(
        omitted.query().solution_probability_policy(),
        BuildSolutionProbabilityPolicy::Omit
    );
}

#[test]
fn build_probability_solution_probabilities_reject_duplicates_and_tiling() {
    let base = "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror";
    let duplicate = CliCommandParser::parse(&format!(
        "{base} --solution-probabilities --solution-probabilities"
    ))
    .expect_err("duplicate solution probability request");
    assert_eq!(duplicate.code(), CliCommandErrorCode::InvalidValue);

    for command in [
        format!("{base} --tiling-only --solution-probabilities"),
        format!("{base} --solution-probabilities --tiling-only"),
        format!("{base} --aggregate tiling --solution-probabilities"),
        format!("{base} --solution-probabilities --aggregate tiling"),
    ] {
        let error = CliCommandParser::parse(&command).expect_err(&command);
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
    }

    let finesse =
        CliCommandParser::parse(&format!("{base} --finesse inputs --solution-probabilities"))
            .expect("ordinary finesse plus solution probabilities")
            .to_app_request()
            .expect("AppRequest");
    let AppCommand::BuildProbability(finesse) = finesse.command() else {
        panic!("expected AppCommand::BuildProbability");
    };
    assert!(finesse.query().finesse_metric().requested());
    assert_eq!(
        finesse.query().solution_probability_policy(),
        BuildSolutionProbabilityPolicy::Include
    );
}

#[test]
fn programmatic_build_probability_rejects_solution_probabilities_with_finesse_score() {
    let score = FinesseScoreRequest::new(vec![FinessePlacement::new(
        PieceKind::I,
        RotationState::Zero,
        0,
        0,
    )])
    .expect("one placement score request");
    let error = CliCommandRequest::build_probability(WebBuildProbabilityInput::new(0, 0xf, 4))
        .with_queue("I")
        .with_hold_enabled(false)
        .with_finesse_score(score)
        .with_solution_probabilities(true)
        .to_app_request()
        .expect_err("solution probabilities plus finesse score");

    assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
    assert!(error.message().contains("finesse score"));

    let error = CliCommandRequest::build_probability(
        WebBuildProbabilityInput::new(0, 0xf, 4)
            .with_aggregation(BuildProbabilityAggregation::TilingOnly),
    )
    .with_queue("I")
    .with_hold_enabled(false)
    .with_solution_probabilities(true)
    .to_app_request()
    .expect_err("programmatic tiling plus solution probabilities");
    assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
    assert!(error.message().contains("tiling aggregation"));
}

#[test]
fn finesse_search_accepts_the_discord_fixed_queue_contract() {
    let base = "0".repeat(60);
    let target = format!("{}f", "0".repeat(59));
    let request = CliCommandParser::parse(&format!(
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
    let request = CliCommandParser::parse(&format!(
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
    let request = CliCommandParser::parse(
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
    let request = CliCommandParser::parse_tokens(&tokens)
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
    let request = CliCommandParser::parse(
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
    let request = CliCommandParser::parse_tokens(&tokens)
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
        ("finesse".to_owned(), CliCommandErrorCode::MissingValue),
        (
            "finesse other".to_owned(),
            CliCommandErrorCode::InvalidValue,
        ),
        (
            format!(
                "finesse score --document v115@vhAAgH --initial-mask {initial} \
                 --height 4 --placements {valid_placement} --queue T"
            ),
            CliCommandErrorCode::UnsupportedCommand,
        ),
        (
            format!("finesse score --height 4 --placements {valid_placement} --queue T"),
            CliCommandErrorCode::MissingValue,
        ),
        (
            format!("finesse score --initial-mask {initial} --height 4 --queue T"),
            CliCommandErrorCode::MissingValue,
        ),
        (
            format!(
                "finesse score --initial-mask {initial} --height 4 \
                 --placements T:up:4:1 --queue T"
            ),
            CliCommandErrorCode::InvalidValue,
        ),
        (
            format!(
                "finesse score --initial-mask {initial} --height 4 \
                 --placements {valid_placement} --placements {valid_placement} --queue T"
            ),
            CliCommandErrorCode::InvalidValue,
        ),
        (
            format!(
                "finesse score --initial-mask {initial} --height 4 \
                 --placements O:spawn:0:0|O:spawn:2:0 --queue OO"
            ),
            CliCommandErrorCode::ProcessSemantics,
        ),
        (
            format!(
                "finesse search --base-mask {initial} --target-mask {initial} --height 4 \
                 --queue T --patterns [T]!"
            ),
            CliCommandErrorCode::InvalidValue,
        ),
        (
            format!(
                "finesse search --base-mask {initial} --target-mask {initial} --height 4 \
                 --queue T --pattern-knowledge private"
            ),
            CliCommandErrorCode::InvalidValue,
        ),
    ];

    for (command, expected_code) in cases {
        let error = CliCommandParser::parse(&command).expect_err(&command);
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

    let error = CliCommandParser::parse_tokens(&tokens).expect_err("placement limit");
    assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
}

#[test]
fn build_probability_preserves_rule_spin_profile_and_initial_hold() {
    let request = CliCommandParser::parse(
        "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --hold T --no-mirror --aggregate spin --rule srs-x --spin-profile all-mini-plus",
    )
    .expect("CLI command")
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
fn canonical_setup_finder_matches_legacy_compatibility_alias() {
    let canonical = CliCommandParser::parse("clearra setup-finder --remaining IOTS --workers 1")
        .expect("canonical setup finder command");
    let legacy = CliCommandParser::parse("clearra setup --remaining IOTS --workers 1")
        .expect("legacy setup alias");
    assert_eq!(canonical, legacy);
}

#[test]
fn build_probability_tiling_only_preserves_hold_supply_and_sets_tiling_objective() {
    let request = CliCommandParser::parse(
        "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue IO --hold T --no-mirror --tiling-only",
    )
    .expect("CLI command")
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
        "--solution-probabilities",
    ] {
        let command = format!(
            "clearra build-probability --base-mask 0x0 --target-mask 0xf --height 4 --queue I --no-hold --no-mirror --tiling-only {option}"
        );
        assert!(CliCommandParser::parse(&command).is_err(), "{option}");
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
            let error = CliCommandParser::parse(&command).expect_err(&command);
            assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
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
            let error = CliCommandParser::parse(&command).expect_err(&command);
            assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
        }
    }

    for command in [
        format!("{base} --aggregate tiling --tiling-only"),
        format!("{base} --tiling-only --tiling-only"),
        format!("{base} --aggregate spin --aggregate spin"),
    ] {
        let error = CliCommandParser::parse(&command).expect_err(&command);
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
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
            let error = CliCommandParser::parse(&command).expect_err(&command);
            assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
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
        CliCommandParser::parse(&command)
            .unwrap_or_else(|error| panic!("failed to parse '{command}': {error:?}"))
            .to_app_request()
            .unwrap_or_else(|error| panic!("failed to project '{command}': {error:?}"));
    }
}

#[test]
fn cli_command_accepts_completed_group_followed_by_bag_permutation() {
    CliCommandParser::parse(
        "clearra pc --lines 2 --backend cpu --patterns [^TSZ]!P4 --max-patterns 20160",
    )
    .expect("CLI command")
    .to_app_request()
    .expect("AppRequest");
}

#[test]
fn scenario_initial_hold_remains_independent_from_p7_queue() {
    let request = CliCommandParser::parse(
        "clearra pc --lines 4 --board-mask 0x80787 --height 4 --pieces 8 --patterns P7 --hold S --backend cpu",
    )
    .expect("CLI command")
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
fn cli_command_preserves_cpu_pool_options_in_the_typed_request() {
    let request = CliCommandParser::parse(
        "clearra pc --lines 2 --backend cpu --workers 1 --use-all-cpu-threads --cpu-warmup",
    )
    .expect("CLI command")
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
fn public_sfinder_percent_preserves_adaptive_workers_without_a_product_claim() {
    let request = CliCommandParser::parse_with_worker_limit(
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
    assert_eq!(request.product_capability_contract(), None);
    assert_eq!(command.result_projection(), PcResultProjection::Standard);
}

#[test]
fn public_chance_compatibility_spellings_remain_generic() {
    for source in [
        "clearra chance v115@vhAAgH P7P3 4",
        "clearra sfinder chance v115@vhAAgH P7P3 4",
        "clearra sfinder percent v115@vhAAgH P7P3 4",
    ] {
        let parsed = CliCommandParser::parse(source).expect(source);
        assert_eq!(
            parsed.pc_result_projection(),
            PcResultProjection::Standard,
            "{source}"
        );
        assert_eq!(parsed.product_capability_contract(), None, "{source}");

        let request = parsed.to_app_request().expect(source);
        assert_eq!(request.product_capability_contract(), None, "{source}");
        let objective = match request.command() {
            AppCommand::Pc(command) => {
                assert_eq!(command.result_projection(), PcResultProjection::Standard);
                command.query().objective()
            }
            AppCommand::Scenario(command) => {
                assert_eq!(command.result_projection(), PcResultProjection::Standard);
                command.query().objective()
            }
            command => panic!("expected PC command for {source}, got {command:?}"),
        };
        assert_eq!(objective.kind(), ObjectiveKind::Unique, "{source}");
    }

    let canonical = CliCommandParser::parse("clearra pc chance --lines 2 --patterns [TI]!")
        .expect("canonical pc chance");
    assert_eq!(
        canonical.pc_result_projection(),
        PcResultProjection::ChanceProbabilityV2(PcChanceIngressOrigin::CanonicalPcChance)
    );
    assert_eq!(
        canonical.product_capability_contract(),
        Some(ProductCapabilityContract::PcChance)
    );
}

#[test]
fn pc_saves_and_best_save_bind_distinct_typed_contracts_across_native_and_compatibility_ingress() {
    for (source, expected_projection, expected_contract) in [
        (
            "clearra pc saves --lines 2 --patterns P7 --no-hold",
            PcResultProjection::SaveGroupsV2(PcSaveIngressOrigin::CanonicalPcSaves),
            ProductCapabilityContract::PcSaves,
        ),
        (
            "clearra pc best-save --lines 2 --patterns P7 --no-hold",
            PcResultProjection::BestSaveV2(PcSaveIngressOrigin::CanonicalPcBestSave),
            ProductCapabilityContract::PcBestSave,
        ),
        (
            "clearra sfinder saves v115@vhAAgH P7P3 4",
            PcResultProjection::SaveGroupsV2(PcSaveIngressOrigin::CompatibilitySaves),
            ProductCapabilityContract::PcSaves,
        ),
        (
            "clearra sfinder best-save v115@vhAAgH P7P3 4",
            PcResultProjection::BestSaveV2(PcSaveIngressOrigin::CompatibilityBestSave),
            ProductCapabilityContract::PcBestSave,
        ),
    ] {
        let parsed = CliCommandParser::parse(source).expect(source);
        assert_eq!(
            parsed.pc_result_projection(),
            expected_projection,
            "{source}"
        );
        assert_eq!(
            parsed.product_capability_contract(),
            Some(expected_contract),
            "{source}"
        );

        let request = parsed.to_app_request().expect(source);
        assert_eq!(
            request.product_capability_contract(),
            Some(expected_contract),
            "{source}"
        );
        match request.command() {
            AppCommand::Pc(command) => {
                assert_eq!(command.result_projection(), expected_projection, "{source}");
                assert_eq!(
                    command.query().objective().kind(),
                    ObjectiveKind::All,
                    "{source}"
                );
            }
            AppCommand::Scenario(command) => {
                assert_eq!(command.result_projection(), expected_projection, "{source}");
                assert_eq!(
                    command.query().objective().kind(),
                    ObjectiveKind::All,
                    "{source}"
                );
                assert_eq!(
                    command.query().count_policy(),
                    PcCountPolicy::CountAll,
                    "{source}"
                );
            }
            command => panic!("expected PC command for {source}, got {command:?}"),
        }
    }
}

#[test]
fn pc_save_ingress_fails_closed_without_fixed_bag_boundary_authority() {
    for source in [
        "clearra pc saves --lines 2 --queue IOTSZ --no-hold",
        "clearra pc best-save --lines 2 --queue IOTSZ --no-hold",
        "clearra sfinder saves --fumen v115@vhAAgH --queue IOTSZJLIOTS --lines 4",
        "clearra sfinder best-save --fumen v115@vhAAgH --queue IOTSZJLIOTS --lines 4",
    ] {
        let error = CliCommandParser::parse(source).expect_err(source);
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{source}");
        assert!(
            error.message().contains("bag-boundary")
                || error
                    .message()
                    .contains("does not accept an explicit --queue"),
            "{source}: {}",
            error.message()
        );
    }

    for forbidden in [
        "--objective all",
        "--count all",
        "--solution-probabilities",
        "--max-memory-mib 64",
        "--visible-seven",
        "--tablebase",
        "--build-dependency-dag",
    ] {
        let source = format!("clearra pc saves --lines 2 --patterns P7 {forbidden}");
        let error = CliCommandParser::parse(&source).expect_err(&source);
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{source}");
    }
}

#[test]
fn canonical_pc_tiling_binds_the_closed_projection_but_generic_tiling_does_not() {
    let canonical = CliCommandParser::parse(
        "clearra pc tiling --lines 2 --queue IIOOO --no-hold --backend cpu --workers 2 --max-patterns 23 --max-memory-mib 64",
    )
    .expect("canonical pc tiling")
    .to_app_request()
    .expect("canonical pc tiling AppRequest");
    assert_eq!(
        canonical.product_capability_contract(),
        Some(ProductCapabilityContract::PcTiling)
    );
    let AppCommand::Pc(command) = canonical.command() else {
        panic!("expected opening PC command");
    };
    assert_eq!(
        command.result_projection(),
        PcResultProjection::TilingFamilyV1(PcTilingIngressOrigin::CanonicalPcTiling)
    );
    assert_eq!(command.query().objective(), ObjectivePolicy::tiling());
    assert_eq!(
        command.query().execution_policy().max_memory_mib(),
        Some(64)
    );
    assert_eq!(command.query().execution_policy().max_patterns(), 23);

    for source in [
        "clearra pc --lines 2 --queue IIOOO --no-hold --objective tiling",
        "clearra pc --lines 2 --queue IIOOO --no-hold --tiling-only",
    ] {
        let generic = CliCommandParser::parse(source)
            .expect(source)
            .to_app_request()
            .expect(source);
        assert_eq!(generic.product_capability_contract(), None, "{source}");
        let AppCommand::Pc(command) = generic.command() else {
            panic!("expected generic PC command for {source}");
        };
        assert_eq!(command.result_projection(), PcResultProjection::Standard);
    }
}

#[test]
fn canonical_scenario_pc_tiling_preserves_unique_family_counting() {
    let request = CliCommandParser::parse(
        "clearra pc tiling --board-mask 0 --height 2 --pieces 5 --lines 2 --queue IIOOO --no-hold --max-patterns 23",
    )
    .expect("canonical scenario pc tiling")
    .to_app_request()
    .expect("canonical scenario pc tiling AppRequest");
    assert_eq!(
        request.product_capability_contract(),
        Some(ProductCapabilityContract::PcTiling)
    );
    let AppCommand::Scenario(command) = request.command() else {
        panic!("expected scenario PC command");
    };
    assert_eq!(command.query().count_policy(), PcCountPolicy::CountUnique);
    assert_eq!(command.query().execution_policy().max_patterns(), 23);
    assert_eq!(
        command.result_projection(),
        PcResultProjection::TilingFamilyV1(PcTilingIngressOrigin::CanonicalPcTiling)
    );
}

#[test]
fn internal_pc_chance_candidates_preserve_all_closed_origins_and_typed_contract() {
    for (source, expected_origin) in [
        (
            "clearra pc chance --lines 2 --patterns [TI]!",
            PcChanceIngressOrigin::CanonicalPcChance,
        ),
        (
            "clearra chance v115@vhAAgH P7P3 4",
            PcChanceIngressOrigin::CompatibilityChance,
        ),
        (
            "clearra sfinder chance v115@vhAAgH P7P3 4",
            PcChanceIngressOrigin::CompatibilityChance,
        ),
        (
            "clearra sfinder percent v115@vhAAgH P7P3 4",
            PcChanceIngressOrigin::CompatibilityPercent,
        ),
    ] {
        let parsed = CliCommandParser::parse_internal_typed_candidate(source).expect(source);
        assert_eq!(
            parsed.pc_result_projection(),
            PcResultProjection::ChanceProbabilityV2(expected_origin),
            "{source}"
        );
        assert_eq!(
            parsed.product_capability_contract(),
            Some(ProductCapabilityContract::PcChance),
            "{source}"
        );

        let request = parsed.to_app_request().expect(source);
        assert_eq!(
            request.product_capability_contract(),
            Some(ProductCapabilityContract::PcChance),
            "{source}"
        );
        let projection = match request.command() {
            AppCommand::Pc(command) => command.result_projection(),
            AppCommand::Scenario(command) => command.result_projection(),
            command => panic!("expected PC command for {source}, got {command:?}"),
        };
        assert_eq!(
            projection,
            PcResultProjection::ChanceProbabilityV2(expected_origin),
            "{source}"
        );
    }
}

#[test]
fn native_percent_and_ordinary_unique_do_not_inherit_pc_chance() {
    let percent = CliCommandParser::parse("clearra percent I --fixed --min-len 1")
        .expect("native percent")
        .to_app_request()
        .expect("native percent AppRequest");
    assert!(matches!(percent.command(), AppCommand::Percent(_)));
    assert_eq!(percent.product_capability_contract(), None);

    let unique =
        CliCommandParser::parse("clearra pc --lines 2 --patterns [TI]! --objective unique")
            .expect("ordinary unique PC");
    assert_eq!(unique.pc_result_projection(), PcResultProjection::Standard);
    assert_eq!(unique.product_capability_contract(), None);
    let unique = unique.to_app_request().expect("ordinary unique AppRequest");
    assert_eq!(unique.product_capability_contract(), None);
}

#[test]
fn pc_chance_rejects_conflicting_objective_but_accepts_its_matching_fixed_value() {
    let conflicting =
        CliCommandParser::parse("clearra pc chance --lines 2 --patterns [TI]! --objective all")
            .expect("syntactically valid conflicting objective")
            .to_app_request()
            .expect_err("pc chance owns its fixed probability semantics");
    assert_eq!(conflicting.code(), CliCommandErrorCode::InvalidValue);

    let matching =
        CliCommandParser::parse("clearra pc chance --lines 2 --patterns [TI]! --objective unique")
            .expect("matching fixed objective")
            .to_app_request()
            .expect("matching fixed objective AppRequest");
    assert_eq!(
        matching.product_capability_contract(),
        Some(ProductCapabilityContract::PcChance)
    );
}

#[test]
fn public_score_compatibility_spellings_keep_fixed_jstris_semantics_without_a_product_claim() {
    for source in [
        "clearra score v115@vhAAgH P7P3 4",
        "clearra sfinder score v115@vhAAgH P7P3 4",
    ] {
        let parsed = CliCommandParser::parse(source).expect(source);
        assert_eq!(
            parsed.pc_result_projection(),
            PcResultProjection::Standard,
            "{source}"
        );
        assert_eq!(parsed.product_capability_contract(), None, "{source}");

        let request = parsed.to_app_request().expect(source);
        assert_eq!(request.product_capability_contract(), None, "{source}");
        let objective = match request.command() {
            AppCommand::Pc(command) => {
                assert_eq!(command.result_projection(), PcResultProjection::Standard);
                command.query().objective()
            }
            AppCommand::Scenario(command) => {
                assert_eq!(command.result_projection(), PcResultProjection::Standard);
                command.query().objective()
            }
            command => panic!("expected PC command for {source}, got {command:?}"),
        };
        assert_eq!(objective.kind(), ObjectiveKind::All, "{source}");
        assert_eq!(
            objective.score().mode(),
            ScoreObjectiveMode::Summary,
            "{source}"
        );
        assert_eq!(
            objective.score().profile(),
            ScoreProfileSelection::JstrisUltra,
            "{source}"
        );
        assert!(!objective.execution_constraints().requested(), "{source}");
    }

    let canonical = CliCommandParser::parse("clearra pc score --lines 2 --patterns [TIOSZ]!")
        .expect("canonical pc score");
    assert_eq!(
        canonical.pc_result_projection(),
        PcResultProjection::ScoreSummaryV2(PcScoreIngressOrigin::CanonicalPcScore)
    );
    assert_eq!(
        canonical.product_capability_contract(),
        Some(ProductCapabilityContract::PcScore)
    );
}

#[test]
fn internal_pc_score_candidates_preserve_closed_origin_profile_and_typed_contract() {
    for (source, expected_origin, expected_profile) in [
        (
            "clearra pc score --lines 2 --patterns [TIOSZ]!",
            PcScoreIngressOrigin::CanonicalPcScore,
            ScoreProfileSelection::Tetrio,
        ),
        (
            "clearra score v115@vhAAgH P7P3 4",
            PcScoreIngressOrigin::CompatibilityScore,
            ScoreProfileSelection::JstrisUltra,
        ),
        (
            "clearra sfinder score v115@vhAAgH P7P3 4",
            PcScoreIngressOrigin::CompatibilityScore,
            ScoreProfileSelection::JstrisUltra,
        ),
    ] {
        let parsed = CliCommandParser::parse_internal_typed_candidate(source).expect(source);
        assert_eq!(
            parsed.pc_result_projection(),
            PcResultProjection::ScoreSummaryV2(expected_origin),
            "{source}"
        );
        assert_eq!(
            parsed.product_capability_contract(),
            Some(ProductCapabilityContract::PcScore),
            "{source}"
        );

        let request = parsed.to_app_request().expect(source);
        assert_eq!(
            request.product_capability_contract(),
            Some(ProductCapabilityContract::PcScore),
            "{source}"
        );
        let objective = match request.command() {
            AppCommand::Pc(command) => {
                assert_eq!(command.result_projection(), parsed.pc_result_projection());
                let policy = command.query().execution_policy();
                assert_eq!(policy.requested_backend(), RequestedSearchBackend::Cpu);
                assert_eq!(policy.worker_policy(), WorkerPolicy::Auto);
                assert!(!policy.allow_backend_fallback());
                assert_eq!(policy.max_patterns(), PC_SCORE_MAX_PATTERNS);
                command.query().objective()
            }
            AppCommand::Scenario(command) => {
                assert_eq!(command.result_projection(), parsed.pc_result_projection());
                let policy = command.query().execution_policy();
                assert_eq!(policy.requested_backend(), RequestedSearchBackend::Cpu);
                assert_eq!(policy.worker_policy(), WorkerPolicy::Auto);
                assert!(!policy.allow_backend_fallback());
                assert_eq!(policy.max_patterns(), PC_SCORE_MAX_PATTERNS);
                command.query().objective()
            }
            command => panic!("expected PC command for {source}, got {command:?}"),
        };
        assert_eq!(objective.kind(), ObjectiveKind::All, "{source}");
        assert_eq!(
            objective.score().mode(),
            ScoreObjectiveMode::Summary,
            "{source}"
        );
        assert_eq!(objective.score().profile(), expected_profile, "{source}");
        assert!(!objective.execution_constraints().requested(), "{source}");
    }
}

#[test]
fn canonical_pc_score_minimals_binds_the_score_only_exact_portfolio_contract() {
    let source = concat!(
        "clearra pc score-minimals --board-mask 0x3f0 --height 1 --pieces 1 ",
        "--lines 1 --queue I --hold empty --rule srs-plus"
    );
    let parsed = CliCommandParser::parse(source).expect(source);
    assert_eq!(
        parsed.pc_result_projection(),
        PcResultProjection::ScorePortfolioV2(
            PcScoreMinimalsIngressOrigin::CanonicalPcScoreMinimals,
        )
    );
    assert_eq!(
        parsed.product_capability_contract(),
        Some(ProductCapabilityContract::PcScoreMinimals)
    );

    let request = parsed.to_app_request().expect(source);
    assert_eq!(
        request.product_capability_contract(),
        Some(ProductCapabilityContract::PcScoreMinimals)
    );
    let AppCommand::Scenario(command) = request.command() else {
        panic!("expected scenario-backed pc score-minimals command");
    };
    assert_eq!(command.result_projection(), parsed.pc_result_projection());
    let query = command.query();
    assert_eq!(query.count_policy(), PcCountPolicy::CountAll);
    assert_eq!(query.objective().kind(), ObjectiveKind::MinimumCover);
    assert_eq!(
        query.objective().score().mode(),
        ScoreObjectiveMode::Summary
    );
    assert_eq!(
        query.objective().score().profile(),
        ScoreProfileSelection::Tetrio
    );
    let policy = query.execution_policy();
    assert_eq!(policy.requested_backend(), RequestedSearchBackend::Cpu);
    assert_eq!(policy.worker_policy(), WorkerPolicy::Auto);
    assert!(!policy.allow_backend_fallback());
    assert_eq!(policy.max_patterns(), PC_SCORE_MAX_PATTERNS);

    for suffix in [
        "--objective minimum-cover",
        "--score",
        "--count all",
        "--solution-probabilities",
        "--backend cpu",
        "--max-memory-mib 64",
    ] {
        let command = format!("{source} {suffix}");
        assert_eq!(
            CliCommandParser::parse(&command)
                .expect_err(&command)
                .code(),
            CliCommandErrorCode::InvalidValue,
            "{command}"
        );
    }
}

#[test]
fn generic_score_flags_and_neighboring_compatibility_commands_do_not_inherit_pc_score() {
    for source in [
        "clearra pc --lines 2 --patterns [TIOSZ]! --objective all --score",
        "clearra sfinder score-minimals v115@vhAAgH P7P3 4",
    ] {
        let parsed = CliCommandParser::parse(source).expect(source);
        assert_eq!(
            parsed.pc_result_projection(),
            PcResultProjection::Standard,
            "{source}"
        );
        assert_eq!(parsed.product_capability_contract(), None, "{source}");
        let request = parsed.to_app_request().expect(source);
        assert_eq!(request.product_capability_contract(), None, "{source}");
    }
}

#[test]
fn pc_score_rejects_authority_overrides_and_unaccounted_resource_limits() {
    for source in [
        "clearra pc score --lines 2 --patterns [TIOSZ]! --objective all",
        "clearra pc score --lines 2 --patterns [TIOSZ]! --score",
        "clearra pc score --lines 2 --patterns [TIOSZ]! --count unique",
    ] {
        let error = CliCommandParser::parse(source).expect_err(source);
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{source}");
    }

    for suffix in [
        "--backend cpu",
        "--gpu-device auto",
        "--gpu-warmup",
        "--allow-backend-fallback",
        "--no-backend-fallback",
        "--tablebase",
        "--no-tablebase",
        "--build-dependency-dag",
        "--no-build-dependency-dag",
        "--retained-traces 1",
        "--max-patterns 1066867200",
        "--max-nodes 1",
        "--max-frontier-states 1",
        "--max-candidates 1",
        "--max-memory-mib 64",
    ] {
        let source = format!("clearra pc score --lines 2 --patterns [TIOSZ]! {suffix}");
        let error = CliCommandParser::parse(&source).expect_err(&source);
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{source}");
        assert!(error.message().contains("execution override"), "{source}");
    }
}

#[test]
fn pc_score_products_preserve_bounded_cpu_worker_options() {
    for (product, source_input) in [
        ("score", "--patterns [TIOSZ]!"),
        ("score-minimals", "--patterns [TIOSZ]!"),
        (
            "score-finder",
            "--board-mask 0 --height 2 --pieces 5 --queue TIOSZJL --no-hold",
        ),
    ] {
        let source = format!(
            "clearra pc {product} --lines 2 {source_input} \
             --workers 4 --use-all-cpu-threads --cpu-warmup"
        );
        let request = CliCommandParser::parse_with_worker_limit(&source, 4)
            .expect(&source)
            .to_app_request()
            .expect("typed score AppRequest");
        let policy = match request.command() {
            AppCommand::Pc(command) => command.query().execution_policy(),
            AppCommand::Scenario(command) => command.query().execution_policy(),
            command => panic!("expected PC score command, got {command:?}"),
        };
        assert_eq!(policy.requested_backend(), RequestedSearchBackend::Cpu);
        assert_eq!(policy.worker_policy(), WorkerPolicy::Fixed(4));
        assert!(policy.use_all_logical_processors());
        assert!(policy.cpu_warmup());
        assert!(!policy.allow_backend_fallback());

        let automatic = CliCommandParser::parse_with_worker_limit(
            &format!(
                "clearra pc {product} --lines 2 {source_input} \
                 --auto-workers 3"
            ),
            4,
        )
        .expect("automatic score worker policy")
        .to_app_request()
        .expect("automatic typed score AppRequest");
        let automatic_policy = match automatic.command() {
            AppCommand::Pc(command) => command.query().execution_policy(),
            AppCommand::Scenario(command) => command.query().execution_policy(),
            command => panic!("expected PC score command, got {command:?}"),
        };
        assert_eq!(automatic_policy.worker_policy(), WorkerPolicy::Auto);
        assert!(automatic_policy.workers() <= 3);
    }
}

#[test]
fn pc_score_bounds_canonical_tokens_before_compatibility_translation() {
    let mut too_many = vec!["clearra".to_owned(), "pc".to_owned(), "score".to_owned()];
    too_many.extend((0..=PC_SCORE_MAX_ARGUMENT_TOKENS).map(|index| format!("argument-{index}")));
    let token_error = CliCommandParser::parse_tokens(&too_many)
        .expect_err("score argument count must fail before compatibility translation");
    assert_eq!(token_error.code(), CliCommandErrorCode::InvalidValue);
    assert!(token_error.message().contains("argument tokens"));

    let too_large = vec![
        "clearra".to_owned(),
        "pc".to_owned(),
        "score".to_owned(),
        "x".repeat(PC_SCORE_MAX_ARGUMENT_BYTES + 1),
    ];
    let byte_error = CliCommandParser::parse_tokens(&too_large)
        .expect_err("score argument bytes must fail before compatibility translation");
    assert_eq!(byte_error.code(), CliCommandErrorCode::InvalidValue);
    assert!(byte_error.message().contains("argument bytes"));

    let oversized_raw = format!(
        "clearra pc score {}",
        "x".repeat(PC_SCORE_MAX_ARGUMENT_BYTES + PC_SCORE_MAX_ARGUMENT_TOKENS + 1)
    );
    let raw_error = CliCommandParser::parse(&oversized_raw)
        .expect_err("oversized raw score text must fail before tokenization");
    assert_eq!(raw_error.code(), CliCommandErrorCode::InvalidValue);
    assert!(raw_error.message().contains("command text"));
}

#[test]
fn pc_score_bounds_internal_compatibility_before_translation_clones() {
    let mut exact_token_limit = vec!["score".to_owned()];
    exact_token_limit.extend((0..PC_SCORE_MAX_ARGUMENT_TOKENS).map(|_| "x".to_owned()));
    crate::web_command_parser::validate_pretranslation_pc_score_tokens(
        &exact_token_limit,
        WebCompatibilityAuthority::InternalTypedCandidate,
    )
    .expect("exactly 64 internal score arguments remain inside the clone boundary");

    let exact_byte_limit = vec![
        "sfinder".to_owned(),
        "score".to_owned(),
        "x".repeat(PC_SCORE_MAX_ARGUMENT_BYTES),
    ];
    crate::web_command_parser::validate_pretranslation_pc_score_tokens(
        &exact_byte_limit,
        WebCompatibilityAuthority::InternalTypedCandidate,
    )
    .expect("exactly 2048 internal score argument bytes remain inside the clone boundary");

    for prefix in [["score"].as_slice(), ["sfinder", "score"].as_slice()] {
        let mut too_many = prefix
            .iter()
            .map(|token| (*token).to_owned())
            .collect::<Vec<_>>();
        too_many
            .extend((0..=PC_SCORE_MAX_ARGUMENT_TOKENS).map(|index| format!("argument-{index}")));
        let error = CliCommandParser::parse_tokens_internal_typed_candidate(&too_many)
            .expect_err("compatibility score token count must fail before translation clones");
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
        assert!(error.message().contains("argument tokens"));

        let mut too_large = prefix
            .iter()
            .map(|token| (*token).to_owned())
            .collect::<Vec<_>>();
        too_large.push("x".repeat(PC_SCORE_MAX_ARGUMENT_BYTES + 1));
        let error = CliCommandParser::parse_tokens_internal_typed_candidate(&too_large)
            .expect_err("compatibility score bytes must fail before translation clones");
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
        assert!(error.message().contains("argument bytes"));
    }

    let oversized_raw = format!(
        "clearra sfinder score {}",
        "x".repeat(PC_SCORE_MAX_ARGUMENT_BYTES + PC_SCORE_MAX_ARGUMENT_TOKENS + 1)
    );
    let error = CliCommandParser::parse_internal_typed_candidate(&oversized_raw)
        .expect_err("compatibility score raw text must fail before tokenization");
    assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
    assert!(error.message().contains("command text"));

    let oversized_direct_raw = format!(
        "clearra score {}",
        "x".repeat(PC_SCORE_MAX_ARGUMENT_BYTES + PC_SCORE_MAX_ARGUMENT_TOKENS + 1)
    );
    let error = CliCommandParser::parse_internal_typed_candidate(&oversized_direct_raw)
        .expect_err("direct compatibility score raw text must fail before tokenization");
    assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
    assert!(error.message().contains("command text"));
}

#[test]
fn pc_score_bounds_pattern_and_fixed_queue_sources() {
    let pattern_at_limit = format!("P1{}", ",".repeat(PC_SCORE_MAX_PATTERN_BYTES - 2));
    assert_eq!(pattern_at_limit.len(), PC_SCORE_MAX_PATTERN_BYTES);
    CliCommandParser::parse(&format!(
        "clearra pc score --lines 2 --patterns {pattern_at_limit}"
    ))
    .expect("128-byte single factorized expression");

    let pattern_over_limit = format!("{pattern_at_limit},");
    let pattern_error = CliCommandParser::parse(&format!(
        "clearra pc score --lines 2 --patterns {pattern_over_limit}"
    ))
    .expect_err("129-byte pattern must be rejected");
    assert_eq!(pattern_error.code(), CliCommandErrorCode::InvalidValue);
    assert!(pattern_error.message().contains("128 UTF-8 bytes"));

    let alternatives = CliCommandParser::parse("clearra pc score --lines 2 --patterns P7;P7")
        .expect_err("score accepts one factorized expression");
    assert_eq!(alternatives.code(), CliCommandErrorCode::InvalidValue);
    assert!(alternatives.message().contains("without alternatives"));

    let queue_at_limit = "IOTSZJLIOTSZJLIO";
    assert_eq!(queue_at_limit.len(), PC_SCORE_MAX_SOURCE_PIECES);
    CliCommandParser::parse(&format!(
        "clearra pc score --lines 2 --queue {queue_at_limit}"
    ))
    .expect("16-piece fixed source");
    let queue_error = CliCommandParser::parse(&format!(
        "clearra pc score --lines 2 --queue {queue_at_limit}T"
    ))
    .expect_err("17-piece fixed source must be rejected");
    assert_eq!(queue_error.code(), CliCommandErrorCode::InvalidValue);
    assert!(queue_error.message().contains("16 source pieces"));

    let window_error =
        CliCommandParser::parse("clearra pc score --lines 2 --patterns P7 --source-pieces 17")
            .expect_err("17-piece source window must be rejected");
    assert_eq!(window_error.code(), CliCommandErrorCode::InvalidValue);
    assert!(window_error.message().contains("16 source pieces"));
}

#[test]
fn pc_score_accepts_six_line_factorized_source_with_product_cpu_policy() {
    let request = CliCommandParser::parse("clearra pc score --lines 6 --patterns P7P7P2")
        .expect("bounded six-line factorized score source")
        .to_app_request()
        .expect("typed score AppRequest");
    let AppCommand::Pc(command) = request.command() else {
        panic!("expected typed opening PC score command");
    };
    let query = command.query();
    assert_eq!(query.queue().mode(), "standard-7-bag");
    assert_eq!(
        query.supply_window_size(),
        Some(SupplyWindowSize::new(PC_SCORE_MAX_SOURCE_PIECES))
    );
    let policy = query.execution_policy();
    assert_eq!(policy.requested_backend(), RequestedSearchBackend::Cpu);
    assert_eq!(policy.worker_policy(), WorkerPolicy::Auto);
    assert!(!policy.allow_backend_fallback());
    assert_eq!(policy.max_patterns(), PC_SCORE_MAX_PATTERNS);
}

#[test]
fn pc_chance_cli_boundary_rejects_supplied_solution_count_memory_and_observation_overrides() {
    let selected_identity = StandardBoard64ColoredTilingIdentity::from_piece_masks(0, [0; 7])
        .expect("empty colored identity");
    let base = || {
        CliCommandRequest::pc(1, RequestedSearchBackend::Cpu)
            .with_patterns("[I]!")
            .with_scenario(WebPcScenarioInput::new(0x3f0, 1, 1).with_allow_hold(false))
            .with_hold_enabled(false)
            .with_pc_chance_product_capability(PcChanceIngressOrigin::CanonicalPcChance)
    };

    let colored = base()
        .with_scenario(
            WebPcScenarioInput::new(0x3f0, 1, 1)
                .with_allow_hold(false)
                .with_allowed_colored_solution_identities([selected_identity]),
        )
        .to_app_request()
        .expect_err("pc chance must reject supplied colored solutions at CLI ingress");
    assert_eq!(colored.code(), CliCommandErrorCode::InvalidValue);
    assert!(colored.message().contains("supplied colored solution"));

    let count = base()
        .with_scenario(
            WebPcScenarioInput::new(0x3f0, 1, 1)
                .with_allow_hold(false)
                .with_count_policy(PcCountPolicy::CountAll),
        )
        .to_app_request()
        .expect_err("pc chance must reject an inner scenario count override");
    assert_eq!(count.code(), CliCommandErrorCode::InvalidValue);
    assert!(count.message().contains("unique solution counting"));

    let scenario_memory = base()
        .with_max_memory_mib(64)
        .to_app_request()
        .expect_err("scenario pc chance must reject unaccounted transient proof memory");
    assert_eq!(scenario_memory.code(), CliCommandErrorCode::InvalidValue);
    assert!(scenario_memory
        .message()
        .contains("transient proof memory is accounted"));

    let opening_memory = CliCommandRequest::pc(2, RequestedSearchBackend::Cpu)
        .with_patterns("[TIOSZ]!")
        .with_max_memory_mib(64)
        .with_pc_chance_product_capability(PcChanceIngressOrigin::CanonicalPcChance)
        .to_app_request()
        .expect_err("opening pc chance must reject unaccounted transient proof memory");
    assert_eq!(opening_memory.code(), CliCommandErrorCode::InvalidValue);
    assert!(opening_memory
        .message()
        .contains("transient proof memory is accounted"));

    let scenario_observation = base()
        .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven)
        .to_app_request()
        .expect_err("scenario pc chance must reject visible-seven semantics");
    assert_eq!(
        scenario_observation.code(),
        CliCommandErrorCode::InvalidValue
    );
    assert!(scenario_observation.message().contains("full-queue oracle"));

    let opening_observation = CliCommandRequest::pc(2, RequestedSearchBackend::Cpu)
        .with_patterns("[TIOSZ]!")
        .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven)
        .with_pc_chance_product_capability(PcChanceIngressOrigin::CanonicalPcChance)
        .to_app_request()
        .expect_err("opening pc chance must reject visible-seven semantics");
    assert_eq!(
        opening_observation.code(),
        CliCommandErrorCode::InvalidValue
    );
    assert!(opening_observation.message().contains("full-queue oracle"));
}

#[test]
fn sfinder_structural_spin_fails_closed_while_setup_reaches_the_worker_budget() {
    for command in ["spin", "spin-cover", "spincover"] {
        let error = CliCommandParser::parse_with_worker_limit(
            &format!("clearra sfinder {command} v115@vhAAgH TI TSS --auto-workers 2"),
            8,
        )
        .expect_err("structural Sfinder spin must not become ordered forward spin");
        assert_eq!(error.code(), CliCommandErrorCode::UnsupportedCommand);
        assert!(error.message().contains("distinct result contract"));
    }

    let setup = CliCommandParser::parse_with_worker_limit(
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
    let error = CliCommandParser::parse_with_worker_limit(
        "clearra pc --lines 4 --workers 2 --auto-workers 3",
        8,
    )
    .expect_err("fixed and adaptive workers conflict");

    assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
}

#[test]
fn directly_constructed_typed_requests_obey_reserved_and_hardware_worker_limits() {
    let reserved = CliCommandRequest::setup(vec![PieceKind::I], false)
        .with_worker_hardware_limit(8)
        .with_workers(usize::MAX)
        .to_app_request()
        .expect("reserved-core setup request");
    assert_eq!(reserved.resource_budget().workers(), 7);

    let all = CliCommandRequest::setup(vec![PieceKind::I], false)
        .with_worker_hardware_limit(8)
        .with_use_all_logical_processors(true)
        .with_workers(usize::MAX)
        .to_app_request()
        .expect("all-logical-processors setup request");
    assert_eq!(all.resource_budget().workers(), 8);

    let automatically_capped = CliCommandRequest::setup(vec![PieceKind::I], false)
        .with_worker_hardware_limit(8)
        .with_automatic_worker_limit(3)
        .to_app_request()
        .expect("automatically capped setup request");
    assert_eq!(automatically_capped.resource_budget().workers(), 3);
}

#[test]
fn cli_command_preserves_gpu_device_and_warmup_in_the_typed_request() {
    let request = CliCommandParser::parse(
        "clearra pc --lines 2 --backend gpu --gpu-device 3 --gpu-warmup --allow-backend-fallback",
    )
    .expect("CLI command")
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
            let error = CliCommandParser::parse(&command).expect_err(&command);
            assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
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
    let parsed = CliCommandParser::parse(&command)
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
fn cli_command_requires_opt_in_for_the_reserved_logical_processor() {
    let hardware = clearra_pc_graph::request::WorkerPolicy::hardware_worker_limit();
    if hardware <= 1 {
        return;
    }
    let command = format!("clearra pc --lines 2 --workers {hardware}");
    assert_eq!(
        CliCommandParser::parse(&command)
            .expect_err("reserved processor requires opt-in")
            .code(),
        CliCommandErrorCode::InvalidValue
    );
    CliCommandParser::parse(&format!("{command} --use-all-cpu-threads"))
        .expect("explicit all-CPU command");
}

#[test]
fn wasm_runtime_does_not_use_native_path_semantics() {
    let error = CliCommandParser::parse("clearra pc --input C:\\temp\\field.txt")
        .expect_err("native path is rejected");

    assert_eq!(error.code(), CliCommandErrorCode::NativePathSemantics);
}

#[test]
fn wasm_runtime_does_not_spawn_process() {
    let error = CliCommandParser::parse("clearra pc --lines 2 | clearra verify")
        .expect_err("process syntax is rejected");

    assert_eq!(error.code(), CliCommandErrorCode::ProcessSemantics);
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

    assert_eq!(error.code(), CliCommandErrorCode::NativePathSemantics);
}

#[test]
fn damage_command_compiles_to_typed_forward_search() {
    let request = CliCommandParser::parse(
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
fn ren_command_compiles_to_its_isolated_typed_forward_search() {
    let request = CliCommandParser::parse(
        "clearra ren --board-mask 0x3f --height 4 --queue TI --no-hold --rule srs-plus",
    )
    .expect("REN command")
    .to_app_request()
    .expect("AppRequest");

    let AppCommand::Ren(command) = request.command() else {
        panic!("expected AppCommand::Ren");
    };
    assert_eq!(
        command.query().piece_source().fixed_sequence(),
        Some(&[PieceKind::T, PieceKind::I][..])
    );
    assert!(!command.query().hold_enabled());
    assert_eq!(command.query().spin_profile(), SpinProfileId::Disabled);
    assert_eq!(command.query().initial_combo(), None);
    assert_eq!(command.query().initial_back_to_back(), None);
    assert_eq!(command.query().mode(), ForwardSearchMode::MaximumRen);
}

#[test]
fn ren_command_enforces_the_fixed_queue_and_twenty_two_piece_boundary() {
    let maximum_queue = "I".repeat(MAX_REN_QUEUE_PIECES);
    CliCommandParser::parse(&format!(
        "clearra ren --board-mask 0 --height 4 --queue {maximum_queue}"
    ))
    .expect("22-piece REN queue");

    let overlong_queue = "I".repeat(MAX_REN_QUEUE_PIECES + 1);
    let overlong = CliCommandParser::parse(&format!(
        "clearra ren --board-mask 0 --height 4 --queue {overlong_queue}"
    ))
    .expect_err("23-piece REN queue");
    assert_eq!(overlong.code(), CliCommandErrorCode::InvalidValue);

    for unsupported in [
        "--patterns [TI]",
        "--spin-profile t-spins",
        "--minimum-damage 1",
        "--initial-combo 1",
        "--initial-b2b 1",
        "--preserve-b2b",
    ] {
        let error = CliCommandParser::parse(&format!(
            "clearra ren --board-mask 0 --height 4 --queue TI {unsupported}"
        ))
        .expect_err("REN must reject damage and spin semantics");
        assert_eq!(error.code(), CliCommandErrorCode::UnsupportedCommand);
    }
}

#[test]
fn damage_command_preserves_minimum_damage_enumeration_policy() {
    let request = CliCommandParser::parse(
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
fn damage_command_default_spin_profile_matches_the_gui_contract() {
    let request = CliCommandParser::parse("clearra damage --board-mask 0 --queue T")
        .expect("default damage command")
        .to_app_request()
        .expect("default damage AppRequest");
    let AppCommand::Damage(command) = request.command() else {
        panic!("expected AppCommand::Damage");
    };

    assert_eq!(command.query().spin_profile(), SpinProfileId::AllMiniPlus);
    assert_eq!(command.query().mode(), ForwardSearchMode::MaximumDamage);
}

#[test]
fn damage_command_accepts_the_canonical_empty_board_mask_at_height_eight() {
    let mask = "0".repeat(60);
    let request = CliCommandParser::parse(&format!("damage --board-mask-v1 {mask} --queue I"))
        .expect("canonical empty damage board");
    let query = request.forward_search_query().expect("forward query");

    assert_eq!(query.board().words(), [0; 4]);
    assert_eq!(query.height(), 8);
}

#[test]
fn damage_command_preserves_the_top_bit_of_the_twenty_fourth_row() {
    let mask = format!("8{}", "0".repeat(59));
    let request = CliCommandParser::parse(&format!("damage --board-mask-v1 {mask} --queue I"))
        .expect("canonical 24-row damage board");
    let query = request.forward_search_query().expect("forward query");

    assert_eq!(query.board().words(), [0, 0, 0, 1_u64 << 47]);
    assert_eq!(query.height(), 24);
}

#[test]
fn damage_command_rejects_noncanonical_and_conflicting_board_masks() {
    for mask in ["0".repeat(59), format!("{}A", "0".repeat(59))] {
        let error = CliCommandParser::parse(&format!("damage --board-mask-v1 {mask} --queue I"))
            .expect_err("noncanonical damage mask");
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
    }

    let canonical = "0".repeat(60);
    for command in [
        format!("damage --board-mask 0x0 --board-mask-v1 {canonical} --queue I"),
        format!("damage --board-mask-v1 {canonical} --board-mask-v1 {canonical} --queue I"),
    ] {
        let error = CliCommandParser::parse(&command).expect_err("conflicting damage masks");
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
    }
}

#[test]
fn canonical_damage_mask_rejects_an_explicit_height_below_its_visible_rows() {
    let mask = format!("8{}", "0".repeat(59));
    let error = CliCommandParser::parse(&format!(
        "damage --board-mask-v1 {mask} --height 23 --queue I"
    ))
    .expect_err("height below canonical field");

    assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
}

#[test]
fn spin_finder_command_preserves_profile_and_target_group() {
    let request = CliCommandParser::parse(
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
    let request = CliCommandParser::parse(
        "clearra spin-finder --patterns P7P1 --spin-profile t-spins --lines any",
    )
    .expect("eight-piece pattern");
    let query = request.forward_search_query().expect("forward query");
    assert_eq!(query.height(), 8);
    assert!(query.piece_source().is_pattern());
    assert_eq!(query.piece_source().sequence_len(), 8);

    let longer =
        CliCommandParser::parse("clearra spin-finder --patterns IOTSZLJIO --spin-profile t-spins")
            .expect("CLI pattern length is not a product limit");
    assert_eq!(
        longer
            .forward_search_query()
            .expect("forward query")
            .piece_source()
            .sequence_len(),
        9
    );

    let damage_pattern = CliCommandParser::parse("clearra damage --patterns [TI]")
        .expect_err("damage pattern must be rejected");
    assert_eq!(
        damage_pattern.code(),
        CliCommandErrorCode::UnsupportedCommand
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
    let family = fixture_family(family);
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
    let family = fixture_family(family);
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

fn fixture_family(family: &str) -> &str {
    match family {
        "damage" => "forward-damage",
        "spin-finder" => "forward-spin",
        family => family,
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
    match CliCommandParser::parse_with_worker_limit(&command, 65) {
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
            assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
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
    let default = CliCommandParser::parse("clearra damage --board-mask 0 --queue T")
        .expect("default maximum-damage command")
        .to_app_request()
        .expect("maximum-damage AppRequest");
    let AppCommand::Damage(command) = default.command() else {
        panic!("expected AppCommand::Damage");
    };
    assert_eq!(command.query().mode(), ForwardSearchMode::MaximumDamage);
    assert_eq!(command.query().spin_profile(), SpinProfileId::AllMiniPlus);

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
    let expected = contract_matrix_case_count(&values)
        - 2 * forward_contract_values("damage", "damage-mode")
            .into_iter()
            .filter(|value| *value == "maximum")
            .count()
            * forward_contract_values("damage", "minimum-damage").len();
    assert_eq!(cases, expected, "exact damage production parser cases");
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
    assert_eq!(
        cases,
        contract_matrix_case_count(&values),
        "exact spin-finder production parser cases"
    );
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
        ("damage", CliCommandErrorCode::UnsupportedCommand),
        ("spin-finder", CliCommandErrorCode::InvalidValue),
    ] {
        for command in [
            format!("clearra {family} --board-mask 0 --queue T --patterns P1"),
            format!("clearra {family} --board-mask 0 --patterns P1 --queue T"),
        ] {
            let error = CliCommandParser::parse(&command).expect_err(&command);
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
    let expected_damage_cases = relevant
        .iter()
        .filter(|(family, _)| *family == "damage")
        .map(|(family, option)| forward_contract_invalid_values(family, option).len())
        .sum::<usize>();
    let expected_spin_cases = relevant
        .iter()
        .filter(|(family, _)| *family == "spin-finder")
        .map(|(family, option)| forward_contract_invalid_values(family, option).len())
        .sum::<usize>();
    let mut cases = 0_usize;
    let mut damage_cases = 0_usize;
    let mut spin_cases = 0_usize;
    for (family, option) in relevant {
        for value in forward_contract_invalid_values(family, option) {
            let (command, expected_code) = match (family, option, value) {
                ("damage", "source", "pattern") => (
                    "clearra damage --board-mask 0 --patterns P1".to_owned(),
                    CliCommandErrorCode::UnsupportedCommand,
                ),
                (_, "source", "both") => (
                    format!("clearra {family} --board-mask 0 --queue T --patterns P1"),
                    CliCommandErrorCode::InvalidValue,
                ),
                (_, "source", "empty") => (
                    format!("clearra {family} --board-mask 0 --queue Q"),
                    CliCommandErrorCode::InvalidValue,
                ),
                (_, "height", value) => (
                    format!("clearra {family} --board-mask 0 --queue T --height {value}"),
                    CliCommandErrorCode::InvalidValue,
                ),
                (_, "spin-profile", value) => (
                    format!("clearra {family} --board-mask 0 --queue T --spin-profile {value}"),
                    CliCommandErrorCode::InvalidValue,
                ),
                (_, "minimum-damage", value) => (
                    format!("clearra {family} --board-mask 0 --queue T --minimum-damage {value}"),
                    CliCommandErrorCode::InvalidValue,
                ),
                (_, "initial-combo", value) => (
                    format!("clearra {family} --board-mask 0 --queue T --initial-combo {value}"),
                    CliCommandErrorCode::InvalidValue,
                ),
                (_, "initial-b2b", value) => (
                    format!("clearra {family} --board-mask 0 --queue T --initial-b2b {value}"),
                    CliCommandErrorCode::InvalidValue,
                ),
                (_, "lines", value) => (
                    format!("clearra {family} --board-mask 0 --queue T --lines {value}"),
                    CliCommandErrorCode::InvalidValue,
                ),
                (_, "category", value) => (
                    format!("clearra {family} --board-mask 0 --queue T --spin-category {value}"),
                    CliCommandErrorCode::InvalidValue,
                ),
                (_, "workers", value) => (
                    format!("clearra {family} --board-mask 0 --queue T --workers {value}"),
                    CliCommandErrorCode::InvalidValue,
                ),
                _ => panic!("unmapped invalid fixture representative {family}.{option}={value}"),
            };
            let error = CliCommandParser::parse(&command).expect_err(&command);
            assert_eq!(error.code(), expected_code, "{family}.{option}={value}");
            cases += 1;
            if family == "damage" {
                damage_cases += 1;
            } else {
                spin_cases += 1;
            }
        }
    }
    assert_eq!(
        damage_cases, expected_damage_cases,
        "damage invalid fixture representatives"
    );
    assert_eq!(
        spin_cases, expected_spin_cases,
        "spin invalid fixture representatives"
    );
    assert_eq!(
        cases,
        expected_damage_cases + expected_spin_cases,
        "all forward invalid fixture representatives"
    );
}

#[derive(Clone, Copy, Debug)]
struct SearchContractSelection<'a> {
    option: &'a str,
    value: &'a str,
}

fn project_contract_command(command: &str) -> Result<clearra_app::AppRequest, CliCommandError> {
    CliCommandParser::parse_with_worker_limit(command, 65)?.to_app_request()
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
            assert_eq!(forward.code(), CliCommandErrorCode::InvalidValue);
        }
        (forward, reverse) => panic!(
            "{family} option order changed acceptance: {forward_command} => {forward:?}; \
             {reverse_command} => {reverse:?}"
        ),
    }
}

fn pc_contract_command(selections: &[SearchContractSelection<'_>]) -> String {
    let score_product = matches!(
        selection_value(selections, "score-mode"),
        Some("score-only-summary" | "score-only-minimum-cover")
    );
    let mut tokens = match selection_value(selections, "score-mode") {
        Some("failed-queue") => vec!["clearra".to_owned(), "failed-queue".to_owned()],
        Some("score-only-summary") => {
            vec!["clearra".to_owned(), "pc".to_owned(), "score".to_owned()]
        }
        Some("score-only-minimum-cover") => vec![
            "clearra".to_owned(),
            "pc".to_owned(),
            "score-minimals".to_owned(),
        ],
        _ => vec!["clearra".to_owned(), "pc".to_owned()],
    };
    let scenario_height = selection_value(selections, "lines")
        .filter(|lines| *lines == "1")
        .or_else(|| {
            selection_value(selections, "hold")
                .filter(|hold| matches!(*hold, "empty" | "I"))
                .map(|_| "4")
        });
    if let Some(height) = scenario_height {
        let (board_mask, pieces) = if height == "1" {
            ("0x387", "1")
        } else {
            ("0", "10")
        };
        tokens.extend([
            "--board-mask".to_owned(),
            board_mask.to_owned(),
            "--height".to_owned(),
            height.to_owned(),
            "--pieces".to_owned(),
            pieces.to_owned(),
        ]);
    }
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
        tokens.extend([
            "--queue".to_owned(),
            if score_product {
                "IOTSZJLIOTSZJLIO"
            } else {
                "IOTSZJLIOTSZJLIOTSZJL"
            }
            .to_owned(),
        ]);
    }
    for selection in selections {
        match (selection.option, selection.value) {
            ("lines", value) => tokens.extend(["--lines".to_owned(), value.to_owned()]),
            ("source", "fixed") => tokens.extend([
                "--queue".to_owned(),
                if score_product {
                    "IOTSZJLIOTSZJLIO"
                } else {
                    "IOTSZJLIOTSZJLIOTSZJL"
                }
                .to_owned(),
            ]),
            ("source", "pattern") => {
                tokens.extend([
                    "--patterns".to_owned(),
                    if score_product { "P7P7P2" } else { "P7P7P7" }.to_owned(),
                ]);
            }
            ("source", "empty") => {}
            ("hold", "disabled") => tokens.push("--no-hold".to_owned()),
            ("hold", "empty") => {
                tokens.extend(["--hold".to_owned(), "empty".to_owned()]);
            }
            ("hold", "I") => tokens.extend(["--hold".to_owned(), "I".to_owned()]),
            ("queue-knowledge", value) => {
                tokens.extend(["--queue-knowledge".to_owned(), value.to_owned()]);
            }
            (
                "score-mode",
                "off" | "failed-queue" | "score-only-summary" | "score-only-minimum-cover",
            ) => {}
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
            ("hold", "disabled") => tokens.push("--no-hold".to_owned()),
            ("hold", "empty") => {
                tokens.extend(["--hold".to_owned(), "empty".to_owned()]);
            }
            ("hold", "I") => tokens.extend(["--hold".to_owned(), "I".to_owned()]),
            ("aggregation", value) => {
                tokens.extend(["--aggregate".to_owned(), value.to_owned()]);
            }
            ("solution-probabilities", "on") => {
                tokens.push("--solution-probabilities".to_owned());
            }
            ("solution-probabilities", "off") => {}
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
            ("mirror", "auto") => {}
            ("mirror", "include") => tokens.push("--include-mirror".to_owned()),
            ("mirror", "exclude") => tokens.push("--no-mirror".to_owned()),
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
            // authoritative CLI parser's equivalent oracle policy.
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
                    CliCommandErrorCode::InvalidValue,
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

fn contract_matrix_case_count(values: &[Vec<&str>]) -> usize {
    let singles = values.iter().map(Vec::len).sum::<usize>();
    let ordered_pairs = (0..values.len())
        .flat_map(|left| ((left + 1)..values.len()).map(move |right| (left, right)))
        .map(|(left, right)| 2 * values[left].len() * values[right].len())
        .sum::<usize>();
    singles + ordered_pairs
}

fn fixture_contract_matrix_case_count(family: &str, options: &[&str]) -> usize {
    let values = options
        .iter()
        .map(|option| forward_contract_values(family, option))
        .collect::<Vec<_>>();
    contract_matrix_case_count(&values)
}

fn invalid_fixture_case_count(family: &str, options: &[&str]) -> usize {
    options
        .iter()
        .map(|option| forward_contract_invalid_values(family, option).len())
        .sum()
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

    if matches!(mode, "score-only-summary" | "score-only-minimum-cover") {
        if selection_enabled(selections, "preserve-b2b")
            || selection_enabled(selections, "solution-probabilities")
            || selection_value(selections, "backend").is_some()
            || selection_value(selections, "workers").is_some()
            || selection_value(selections, "tablebase").is_some()
            || selection_value(selections, "dependency-dag").is_some()
            || selection_value(selections, "gpu-device").is_some()
            || matches!(
                selection_value(selections, "fallback"),
                Some("allow" | "deny")
            )
        {
            return false;
        }
        return selection_value(selections, "queue-knowledge") != Some("visible-7");
    }

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
    if has_spin_profile && mode != "score" && !preserves_b2b {
        return false;
    }
    if has_initial_b2b && mode != "score" {
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
            && !has_pattern_knowledge
            && !selection_enabled(selections, "solution-probabilities");
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
    let expected = fixture_contract_matrix_case_count("pc", &options);
    assert_eq!(
        assert_complete_contract_matrix(
            "pc",
            &options,
            pc_contract_command,
            pc_contract_case_is_valid,
        ),
        expected
    );
}

#[test]
fn build_fixture_reaches_the_actual_parser_for_every_single_and_ordered_option_pair() {
    let options = [
        "height",
        "source",
        "hold",
        "aggregation",
        "solution-probabilities",
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
    let expected = fixture_contract_matrix_case_count("build", &options);
    assert_eq!(
        assert_complete_contract_matrix(
            "build",
            &options,
            build_contract_command,
            build_contract_case_is_valid,
        ),
        expected
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
    let expected = fixture_contract_matrix_case_count("setup", &options);
    assert_eq!(
        assert_complete_contract_matrix(
            "setup",
            &options,
            setup_contract_command,
            setup_contract_case_is_valid,
        ),
        expected
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
    let expected = fixture_contract_matrix_case_count("spin-structure", &options);
    assert_eq!(
        assert_complete_contract_matrix(
            "spin-structure",
            &options,
            spin_structure_contract_command,
            spin_structure_contract_case_is_valid,
        ),
        expected
    );
}

#[test]
fn pc_build_and_spin_structure_invalid_fixture_values_reach_the_authoritative_parser() {
    let pc_options = [
        "lines",
        "source",
        "queue-knowledge",
        "spin-profile",
        "initial-b2b",
        "fallback",
        "workers",
        "gpu-device",
    ];
    let mut pc_cases = 0_usize;
    for option in pc_options {
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
            assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
            pc_cases += 1;
        }
    }

    let build_options = [
        "height",
        "source",
        "solution-probabilities",
        "spin-profile",
        "finesse",
        "pattern-knowledge",
        "workers",
    ];
    let mut build_cases = 0_usize;
    for option in build_options {
        for value in forward_contract_invalid_values("build", option) {
            let base = "clearra build-probability --base-mask 0 --target-mask 15";
            let command = match (option, value) {
                ("height", value) => format!("{base} --height {value} --queue I"),
                ("source", "both") => {
                    format!("{base} --height 8 --queue I --patterns P1")
                }
                ("solution-probabilities", value) => {
                    format!("{base} --height 8 --queue I --solution-probabilities {value}")
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
            assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
            build_cases += 1;
        }
    }

    let structure_options = [
        "inventory",
        "fill-bottom",
        "fill-top",
        "max-placements",
        "minimality",
        "spin-profile",
        "lines",
        "workers",
    ];
    let mut structure_cases = 0_usize;
    for option in structure_options {
        for value in forward_contract_invalid_values("spin-structure", option) {
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
            assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
            structure_cases += 1;
        }
    }

    assert_eq!(pc_cases, invalid_fixture_case_count("pc", &pc_options));
    assert_eq!(
        build_cases,
        invalid_fixture_case_count("build", &build_options)
    );
    assert_eq!(
        structure_cases,
        invalid_fixture_case_count("spin-structure", &structure_options)
    );
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
            assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
            cases += 1;
        }
    }
    assert_eq!(cases, 7);
}

#[test]
fn spin_structure_compiles_to_an_independent_typed_command() {
    let request = CliCommandParser::parse_with_worker_limit(
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
fn canonical_spin_structure_search_path_lowers_to_the_existing_typed_search_command() {
    let canonical =
        CliCommandParser::parse("clearra spin-structure search --pieces IOT --height 7 --lines 1+")
            .expect("canonical structure search")
            .to_app_request()
            .expect("canonical AppRequest");
    let legacy =
        CliCommandParser::parse("clearra spin-structure --pieces IOT --height 7 --lines 1+")
            .expect("legacy structure search")
            .to_app_request()
            .expect("legacy AppRequest");

    assert_eq!(canonical.command(), legacy.command());
    assert_eq!(canonical.resource_budget(), legacy.resource_budget());
    let AppCommand::SpinStructure(command) = canonical.command() else {
        panic!("expected canonical SpinStructure command");
    };
    assert_eq!(command.product_mode(), SpinStructureProductMode::Search);
}

#[test]
fn canonical_spin_structure_cover_and_guaranteed_lower_to_distinct_typed_modes() {
    let cover = CliCommandParser::parse(
        "clearra spin-structure cover --pieces IOT --height 7 --lines 1+ --objective min-cover --max-patterns 42",
    )
    .expect("canonical structure cover")
    .to_app_request()
    .expect("cover AppRequest");
    let AppCommand::SpinStructure(cover) = cover.command() else {
        panic!("expected SpinStructure cover command");
    };
    assert_eq!(
        cover.product_mode(),
        SpinStructureProductMode::Cover { max_patterns: 42 }
    );

    let guaranteed = CliCommandParser::parse(
        "clearra spin-structure guaranteed --pieces IOT --height 7 --lines 1+ --final-piece T --max-patterns 43 --dependency-report",
    )
    .expect("canonical structure guaranteed")
    .to_app_request()
    .expect("guaranteed AppRequest");
    let AppCommand::SpinStructure(guaranteed) = guaranteed.command() else {
        panic!("expected SpinStructure guaranteed command");
    };
    assert_eq!(
        guaranteed.product_mode(),
        SpinStructureProductMode::Guaranteed {
            final_piece: PieceKind::T,
            max_patterns: 43,
            dependency_report: true,
        }
    );
}

#[test]
fn spin_structure_product_routes_reject_cross_route_and_ungoverned_options() {
    for command in [
        "clearra spin-structure search --pieces T --max-patterns 2",
        "clearra spin-structure search --pieces T --final-piece T",
        "clearra spin-structure search --pieces T --dependency-report",
        "clearra spin-structure cover --pieces T --final-piece T",
        "clearra spin-structure cover --pieces T --dependency-report",
        "clearra spin-structure cover --pieces T --objective score",
        "clearra spin-structure cover --pieces T --max-patterns 0",
        "clearra spin-structure cover --pieces T --max-patterns 100001",
        "clearra spin-structure guaranteed --pieces T --objective min-cover",
        "clearra spin-structure guaranteed --pieces I --final-piece T",
        "clearra spin-structure guaranteed --pieces IO --spin-profile t-spins --final-piece I",
        "clearra spin-structure guaranteed --pieces T --dependency-report --no-dependency-report",
    ] {
        let error = CliCommandParser::parse(command).expect_err(command);
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
    }
}

#[test]
fn spin_structure_accepts_all_six_profiles_without_using_forward_mode() {
    for mode in SpinStructureMode::ALL {
        let parsed = CliCommandParser::parse(&format!(
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
        let error = CliCommandParser::parse(&format!(
            "spin-structure --pieces T --spin-profile {invalid}"
        ))
        .expect_err("invalid structure profile");
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
    }
}

#[test]
fn spin_structure_preserves_a_canonical_wide_board_and_rejects_conflicts() {
    let mask = format!("8{}", "0".repeat(59));
    let request =
        CliCommandParser::parse(&format!("spin-structure --board-mask-v1 {mask} --pieces T"))
            .expect("wide structure board");
    let query = request.spin_structure_query().expect("structure query");
    assert_eq!(query.height, 24);
    assert_eq!(query.initial_board.words(), [0, 0, 0, 1_u64 << 47]);

    let error = CliCommandParser::parse(&format!(
        "spin-structure --board-mask 0 --board-mask-v1 {} --pieces T",
        "0".repeat(60)
    ))
    .expect_err("conflicting board options");
    assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
}

#[test]
fn pc_allspin_exact_queue_lowers_to_typed_existential_b2b_without_scoring() {
    let parsed = CliCommandParser::parse_with_worker_limit(
        "clearra pc allspin-sol --lines 2 --queue IOTSZ --spin-profile all-spin-plus --no-hold --rule srs --workers 3",
        8,
    )
    .expect("PC All-Spin exact queue");
    assert_eq!(parsed.command_kind(), "pc");
    assert_eq!(
        parsed.pc_result_projection(),
        PcResultProjection::AllSpinSolution(SpinProfileSelection::AllSpinPlus)
    );

    let request = parsed.to_app_request().expect("typed PC AppRequest");
    let AppCommand::Pc(command) = request.command() else {
        panic!("expected PC command");
    };
    let query = command.query();
    assert_eq!(query.target().lines(), 2);
    assert_eq!(query.queue().mode(), "fixed");
    assert_eq!(
        query.queue_observation_policy(),
        QueueObservationPolicy::FullQueueOracle
    );
    assert_eq!(
        query.objective().execution_constraints().spin_profile(),
        SpinProfileSelection::AllSpinPlus
    );
    assert!(query
        .objective()
        .execution_constraints()
        .preserves_back_to_back());
    assert!(!query.objective().score().requested());
    assert_eq!(command.result_projection(), parsed.pc_result_projection());
    assert_eq!(query.execution_policy().workers(), 3);
}

#[test]
fn pc_allspin_pattern_chance_preserves_pattern_universe_projection() {
    let parsed = CliCommandParser::parse(
        "clearra pc allspin-pres-chance --lines 4 --patterns [TI]! --spin-profile all-mini-plus --max-patterns 5040",
    )
    .expect("PC All-Spin preservation chance");
    assert_eq!(
        parsed.pc_result_projection(),
        PcResultProjection::AllSpinPreservationChance(SpinProfileSelection::AllMiniPlus)
    );

    let request = parsed.to_app_request().expect("typed PC AppRequest");
    let AppCommand::Pc(command) = request.command() else {
        panic!("expected PC command");
    };
    assert_eq!(
        command.query().queue().mode(),
        "materialized-pattern-expression"
    );
    assert_eq!(
        command
            .query()
            .objective()
            .execution_constraints()
            .spin_profile(),
        SpinProfileSelection::AllMiniPlus
    );
    assert!(!command.query().objective().score().requested());
}

#[test]
fn pc_allspin_forms_require_their_distinct_input_and_explicit_profile() {
    for command in [
        "clearra pc allspin-sol --queue IOTSZ",
        "clearra pc allspin-sol --spin-profile all-spin-plus",
        "clearra pc allspin-pres-chance --patterns P7",
        "clearra pc allspin-pres-chance --spin-profile all-spin-plus",
    ] {
        let error = CliCommandParser::parse(command).expect_err(command);
        assert_eq!(error.code(), CliCommandErrorCode::MissingValue, "{command}");
    }

    for command in [
        "clearra pc allspin-sol --queue IOTS --queue SZJ --spin-profile all-spin-plus",
        "clearra pc allspin-pres-chance --patterns P7 --pattern P4 --spin-profile all-spin-plus",
        "clearra pc allspin-sol --queue IOTS --spin-profile all-spin --spin-profile all-spin-plus",
        "clearra pc allspin-sol --patterns P7 --spin-profile all-spin-plus",
        "clearra pc allspin-pres-chance --queue IOTS --spin-profile all-spin-plus",
        "clearra pc allspin-sol --queue IOTS --spin-profile all-spins",
    ] {
        let error = CliCommandParser::parse(command).expect_err(command);
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
    }
}

#[test]
fn pc_allspin_forms_accept_each_explicit_clearra_spin_profile() {
    for profile in [
        "t-spins",
        "t-spins-plus",
        "all-spin",
        "all-spin-plus",
        "all-mini",
        "all-mini-plus",
    ] {
        let parsed = CliCommandParser::parse(&format!(
            "clearra pc allspin-sol --queue IOTS --spin-profile {profile}"
        ))
        .expect(profile);
        assert_eq!(
            parsed.pc_result_projection().spin_profile(),
            SpinProfileSelection::parse(profile)
        );
    }
}

#[test]
fn pc_allspin_exact_and_pattern_forms_preserve_a_nonempty_initial_field() {
    let exact = CliCommandParser::parse(
        "clearra pc allspin-sol --lines 2 --board-mask 1 --height 2 --pieces 5 --queue IOTSZ --spin-profile all-spin-plus --no-hold",
    )
    .expect("exact initial-field All-Spin")
    .to_app_request()
    .expect("exact initial-field AppRequest");
    let AppCommand::Scenario(exact) = exact.command() else {
        panic!("expected scenario PC command");
    };
    assert_eq!(
        exact.result_projection(),
        PcResultProjection::AllSpinSolution(SpinProfileSelection::AllSpinPlus)
    );
    assert_eq!(exact.query().initial_board().occupied_mask(), 1);
    assert_eq!(exact.query().initial_board().visible_height(), 2);
    assert_eq!(exact.query().piece_window().max_pieces(), 5);
    assert_eq!(exact.query().exact_pieces(), Some(5));
    assert!(!exact.query().allow_hold());
    assert_eq!(exact.query().completion_goal().as_str(), "clear-to-empty");

    let chance = CliCommandParser::parse(
        "clearra pc allspin-pres-chance --board-mask 1 --height 1 --pieces 1 --patterns [TI]! --spin-profile all-mini-plus",
    )
    .expect("pattern initial-field All-Spin")
    .to_app_request()
    .expect("pattern initial-field AppRequest");
    let AppCommand::Scenario(chance) = chance.command() else {
        panic!("expected scenario PC command");
    };
    assert_eq!(
        chance.result_projection(),
        PcResultProjection::AllSpinPreservationChance(SpinProfileSelection::AllMiniPlus)
    );
    assert_eq!(chance.query().initial_board().occupied_mask(), 1);
    assert_eq!(chance.query().initial_board().visible_height(), 1);
    assert_eq!(chance.query().piece_window().max_pieces(), 1);
    assert_eq!(chance.query().completion_goal().as_str(), "clear-to-empty");
}

#[test]
fn pc_allspin_parser_preserves_typed_product_identity_into_app_request() {
    let exact = CliCommandParser::parse(
        "clearra pc allspin-sol --lines 2 --queue IOTSZ --spin-profile all-spin-plus",
    )
    .expect("exact All-Spin web request");
    assert_eq!(
        exact.product_capability_contract(),
        Some(ProductCapabilityContract::PcAllSpinSolution)
    );
    let exact = exact.to_app_request().expect("exact All-Spin AppRequest");
    assert_eq!(
        exact.product_capability_contract(),
        Some(ProductCapabilityContract::PcAllSpinSolution)
    );

    let chance = CliCommandParser::parse(
        "clearra pc allspin-pres-chance --lines 2 --patterns [TI]! --spin-profile all-mini-plus",
    )
    .expect("chance All-Spin web request");
    assert_eq!(
        chance.product_capability_contract(),
        Some(ProductCapabilityContract::PcAllSpinPreservationChance)
    );
    let chance = chance.to_app_request().expect("chance All-Spin AppRequest");
    assert_eq!(
        chance.product_capability_contract(),
        Some(ProductCapabilityContract::PcAllSpinPreservationChance)
    );
}

#[test]
fn cli_typed_identity_mismatch_is_rejected_and_ordinary_pc_cannot_inherit_it() {
    let profile = SpinProfileSelection::AllSpinPlus;
    let mismatch = CliCommandRequest::pc(2, RequestedSearchBackend::Cpu)
        .with_patterns("[TI]!")
        .with_objective(ObjectivePolicy::unique().with_back_to_back_preservation(profile))
        .with_pc_result_projection(PcResultProjection::AllSpinPreservationChance(profile))
        .with_product_capability_contract_for_test(ProductCapabilityContract::PcAllSpinSolution)
        .to_app_request()
        .expect_err("typed identity cannot bypass its projection validator");
    assert_eq!(mismatch.code(), CliCommandErrorCode::InvalidValue);

    let standard_with_identity = CliCommandRequest::pc(2, RequestedSearchBackend::Cpu)
        .with_product_capability_contract_for_test(ProductCapabilityContract::PcAllSpinSolution)
        .to_app_request()
        .expect_err("standard projection cannot inherit a typed target identity");
    assert_eq!(
        standard_with_identity.code(),
        CliCommandErrorCode::InvalidValue
    );

    let ordinary = CliCommandParser::parse("clearra pc --lines 2 --backend cpu")
        .expect("ordinary PC web request");
    assert_eq!(ordinary.product_capability_contract(), None);
    assert_eq!(
        ordinary
            .to_app_request()
            .expect("ordinary PC AppRequest")
            .product_capability_contract(),
        None
    );
}

#[test]
fn pc_allspin_initial_field_is_atomic_unique_and_line_height_consistent() {
    for suffix in [
        "--board-mask 1",
        "--height 2",
        "--pieces 5",
        "--board-mask 1 --height 2",
        "--board-mask 1 --pieces 5",
        "--height 2 --pieces 5",
        "--board-mask 1 --board-mask 2 --height 2 --pieces 5",
        "--board-mask 1 --height 2 --height 2 --pieces 5",
        "--board-mask 1 --height 2 --pieces 5 --pieces 5",
        "--lines 4 --board-mask 1 --height 2 --pieces 5",
        "--board-mask 1 --height 7 --pieces 5",
    ] {
        let command =
            format!("clearra pc allspin-sol --queue IOTSZ --spin-profile all-spin-plus {suffix}");
        let error = CliCommandParser::parse(&command).expect_err(&command);
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
    }

    let target = CliCommandParser::parse(
        "clearra pc allspin-sol --queue IOTSZ --spin-profile all-spin-plus --target-mask 1",
    )
    .expect_err("target fields are not accepted");
    assert_eq!(target.code(), CliCommandErrorCode::UnsupportedCommand);
}

#[test]
fn pc_allspin_forms_reject_target_file_score_multiplicity_and_observation_overrides() {
    for forbidden in [
        "--hold empty",
        "--source-pieces 10",
        "--count all",
        "--objective minimum-cover",
        "--tiling-only",
        "--score",
        "--score-profile tetrio",
        "--initial-b2b 1",
        "--retained-traces 2",
        "--solution-probabilities",
        "--queue-knowledge visible-7",
        "--queue-knowledge oracle",
        "--preserve-b2b",
        "--solution-fumen v115@test",
        "--input local.json",
        "--file local.json",
        "--fixture local.json",
        "--output local.json",
    ] {
        let command =
            format!("clearra pc allspin-sol --queue IOTS --spin-profile all-spin-plus {forbidden}");
        let error = CliCommandParser::parse(&command).expect_err(&command);
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{command}");
    }
}

#[test]
fn direct_typed_pc_allspin_requests_fail_closed_before_or_after_lowering() {
    let profile = SpinProfileSelection::AllSpinPlus;
    let chance = PcResultProjection::AllSpinPreservationChance(profile);
    let b2b = || ObjectivePolicy::unique().with_back_to_back_preservation(profile);
    let exact_request = || {
        CliCommandRequest::pc(2, RequestedSearchBackend::Cpu)
            .with_queue("IOTSZ")
            .with_objective(b2b())
            .with_pc_allspin_product_capability(
                ProductCapabilityContract::PcAllSpinSolution,
                profile,
            )
    };
    let chance_request = || {
        CliCommandRequest::pc(4, RequestedSearchBackend::Cpu)
            .with_patterns("[TI]!")
            .with_objective(b2b())
            .with_pc_allspin_product_capability(
                ProductCapabilityContract::PcAllSpinPreservationChance,
                profile,
            )
    };

    assert!(exact_request().to_app_request().is_ok());
    assert!(chance_request().to_app_request().is_ok());
    assert!(CliCommandRequest::pc(2, RequestedSearchBackend::Cpu)
        .with_patterns("P7")
        .with_objective(b2b())
        .with_pc_allspin_product_capability(
            ProductCapabilityContract::PcAllSpinPreservationChance,
            profile,
        )
        .to_app_request()
        .is_ok());
    assert!(CliCommandRequest::pc(2, RequestedSearchBackend::Cpu)
        .with_queue("IOTSZ")
        .with_scenario(WebPcScenarioInput::new(1, 2, 5))
        .with_objective(b2b())
        .with_pc_allspin_product_capability(ProductCapabilityContract::PcAllSpinSolution, profile,)
        .to_app_request()
        .is_ok());
    assert!(CliCommandRequest::pc(1, RequestedSearchBackend::Cpu)
        .with_patterns("[TI]!")
        .with_scenario(WebPcScenarioInput::new(1, 1, 1).with_allow_hold(false))
        .with_hold_enabled(false)
        .with_objective(b2b())
        .with_pc_allspin_product_capability(
            ProductCapabilityContract::PcAllSpinPreservationChance,
            profile,
        )
        .to_app_request()
        .is_ok());

    let missing_contract = CliCommandRequest::pc(2, RequestedSearchBackend::Cpu)
        .with_patterns("[TI]!")
        .with_objective(b2b())
        .with_pc_result_projection(chance)
        .to_app_request()
        .expect_err("public projection-only builder cannot bypass typed identity");
    assert_eq!(missing_contract.code(), CliCommandErrorCode::InvalidValue);

    let selected_identity = StandardBoard64ColoredTilingIdentity::from_piece_masks(0, [0; 7])
        .expect("empty colored identity");
    let virtual_file = WebVirtualFileHandle::new("input", "input.json", "application/json", 1)
        .expect("browser virtual file");
    let invalid = vec![
        (
            "exact-pattern-supply",
            CliCommandRequest::pc(2, RequestedSearchBackend::Cpu)
                .with_patterns("[TI]!")
                .with_objective(b2b())
                .with_pc_allspin_product_capability(
                    ProductCapabilityContract::PcAllSpinSolution,
                    profile,
                ),
        ),
        (
            "chance-fixed-supply",
            CliCommandRequest::pc(2, RequestedSearchBackend::Cpu)
                .with_queue("IOTSZ")
                .with_objective(b2b())
                .with_pc_allspin_product_capability(
                    ProductCapabilityContract::PcAllSpinPreservationChance,
                    profile,
                ),
        ),
        (
            "duplicate-supply-kinds",
            chance_request().with_queue("IOTSZ"),
        ),
        (
            "missing-supply",
            CliCommandRequest::pc(2, RequestedSearchBackend::Cpu)
                .with_objective(b2b())
                .with_pc_allspin_product_capability(
                    ProductCapabilityContract::PcAllSpinSolution,
                    profile,
                ),
        ),
        (
            "opening-line-domain",
            CliCommandRequest::pc(8, RequestedSearchBackend::Cpu)
                .with_queue("IOTSZ")
                .with_objective(b2b())
                .with_pc_allspin_product_capability(
                    ProductCapabilityContract::PcAllSpinSolution,
                    profile,
                ),
        ),
        ("source-window", exact_request().with_source_piece_count(5)),
        (
            "build-base-target-payload",
            exact_request().with_build_probability(WebBuildProbabilityInput::new(0, 1, 2)),
        ),
        (
            "count-policy",
            exact_request().with_count_policy(PcCountPolicy::CountAll),
        ),
        (
            "objective-kind",
            exact_request()
                .with_objective(ObjectivePolicy::all().with_back_to_back_preservation(profile)),
        ),
        (
            "score-selection",
            exact_request().with_objective(
                ObjectivePolicy::unique()
                    .with_score_summary()
                    .with_back_to_back_preservation(profile),
            ),
        ),
        (
            "missing-preservation",
            exact_request().with_objective(ObjectivePolicy::unique()),
        ),
        (
            "profile-mismatch",
            exact_request().with_objective(
                ObjectivePolicy::unique()
                    .with_back_to_back_preservation(SpinProfileSelection::AllMiniPlus),
            ),
        ),
        (
            "objective-tie-override",
            exact_request().with_objective(
                ObjectivePolicy::new(
                    ObjectiveKind::Unique,
                    TiePolicy::LowestCandidateId,
                    TracePolicy::Keep,
                )
                .with_back_to_back_preservation(profile),
            ),
        ),
        (
            "solution-probabilities",
            exact_request().with_solution_probabilities(true),
        ),
        (
            "visible-seven",
            exact_request().with_queue_observation_policy(QueueObservationPolicy::VisibleSeven),
        ),
        (
            "virtual-file",
            exact_request().with_virtual_file(virtual_file),
        ),
        (
            "empty-initial-field",
            exact_request().with_scenario(WebPcScenarioInput::new(0, 2, 5)),
        ),
        (
            "initial-height-domain",
            exact_request().with_scenario(WebPcScenarioInput::new(1, 7, 5)),
        ),
        (
            "exact-scenario-lines-height-mismatch",
            CliCommandRequest::pc(4, RequestedSearchBackend::Cpu)
                .with_queue("IOTSZ")
                .with_scenario(WebPcScenarioInput::new(1, 2, 5))
                .with_objective(b2b())
                .with_pc_allspin_product_capability(
                    ProductCapabilityContract::PcAllSpinSolution,
                    profile,
                ),
        ),
        (
            "chance-scenario-lines-height-mismatch",
            CliCommandRequest::pc(4, RequestedSearchBackend::Cpu)
                .with_patterns("[TI]!")
                .with_scenario(WebPcScenarioInput::new(1, 1, 1).with_allow_hold(false))
                .with_hold_enabled(false)
                .with_objective(b2b())
                .with_pc_allspin_product_capability(
                    ProductCapabilityContract::PcAllSpinPreservationChance,
                    profile,
                ),
        ),
        (
            "outer-no-hold-inner-hold",
            CliCommandRequest::pc(2, RequestedSearchBackend::Cpu)
                .with_queue("IOTSZ")
                .with_scenario(WebPcScenarioInput::new(1, 2, 5))
                .with_hold_enabled(false)
                .with_objective(b2b())
                .with_pc_allspin_product_capability(
                    ProductCapabilityContract::PcAllSpinSolution,
                    profile,
                ),
        ),
        (
            "outer-hold-inner-no-hold",
            CliCommandRequest::pc(2, RequestedSearchBackend::Cpu)
                .with_queue("IOTSZ")
                .with_scenario(WebPcScenarioInput::new(1, 2, 5).with_allow_hold(false))
                .with_objective(b2b())
                .with_pc_allspin_product_capability(
                    ProductCapabilityContract::PcAllSpinSolution,
                    profile,
                ),
        ),
        (
            "occupied-scenario-hold",
            exact_request().with_scenario(
                WebPcScenarioInput::new(1, 2, 5).with_hold_piece(Some(PieceKind::T)),
            ),
        ),
        (
            "scenario-source-window",
            exact_request()
                .with_scenario(WebPcScenarioInput::new(1, 2, 5).with_source_piece_count(5)),
        ),
        (
            "scenario-count-policy",
            exact_request().with_scenario(
                WebPcScenarioInput::new(1, 2, 5).with_count_policy(PcCountPolicy::CountAll),
            ),
        ),
        (
            "scenario-target-identities",
            exact_request().with_scenario(
                WebPcScenarioInput::new(1, 2, 5)
                    .with_allowed_colored_solution_identities([selected_identity]),
            ),
        ),
        (
            "non-pc-command",
            CliCommandRequest::setup(vec![PieceKind::I], false).with_pc_allspin_product_capability(
                ProductCapabilityContract::PcAllSpinSolution,
                profile,
            ),
        ),
    ];
    for (name, request) in invalid {
        let error = request.to_app_request().expect_err(name);
        assert_eq!(error.code(), CliCommandErrorCode::InvalidValue, "{name}");
    }
}
#[test]
fn legacy_alias_fixture_parses_30_surface_pairs_to_identical_public_app_requests() {
    const FIXTURE: &str =
        include_str!("../../../tests/fixtures/contracts/legacy_alias_equivalence.v1.json");

    let fixture = FIXTURE.replace("\r\n", "\n");
    let mut checked = 0usize;
    for remainder in fixture.split("\n    {\n      \"id\": \"").skip(1) {
        let block = remainder
            .split("\n    },\n    {")
            .next()
            .expect("fixture case block");
        if !block.contains("\"canonical_web_command\"") {
            continue;
        }
        let id = block.split('"').next().expect("fixture case id");
        let capability_id = fixture_string(block, "capability_id");
        let problem_contract_id = fixture_string(block, "problem_contract_id");
        let result_contract_id = fixture_string(block, "result_contract_id");
        let (expected_problem, expected_result) = expected_alias_families(capability_id);
        assert_eq!(
            problem_contract_id, expected_problem,
            "{id}: input family drift"
        );
        assert_eq!(
            result_contract_id, expected_result,
            "{id}: result family drift"
        );

        for (surface, canonical_field, alias_field) in [
            (
                "discord-slash",
                "canonical_web_command",
                "alias_web_command",
            ),
            (
                "discord-text",
                "canonical_text_web_command",
                "alias_text_web_command",
            ),
        ] {
            let canonical_source = fixture_string(block, canonical_field);
            let alias_source = fixture_string(block, alias_field);
            let canonical = parse_fixture_app_request(id, surface, "canonical", canonical_source);
            let alias = parse_fixture_app_request(id, surface, "alias", alias_source);

            assert_eq!(
                canonical.command_kind(),
                alias.command_kind(),
                "{id}/{surface}: typed AppCommand family drift"
            );
            assert_eq!(
                canonical.query(),
                alias.query(),
                "{id}/{surface}: typed query/problem envelope drift"
            );
            assert_eq!(
                canonical.command(),
                alias.command(),
                "{id}/{surface}: normalized AppCommand fields drift"
            );
            assert_eq!(
                canonical, alias,
                "{id}/{surface}: normalized AppRequest policy fields drift"
            );
            checked += 1;
        }
    }
    assert_eq!(
        checked, 30,
        "legacy alias fixture must cover 15 logical cases on both ingress surfaces"
    );
}

fn parse_fixture_app_request(
    id: &str,
    surface: &str,
    variant: &str,
    source: &str,
) -> clearra_app::app_request::AppRequest {
    // Discord text owns the representation-only `--format text` suffix.  The
    // CLI parser owns the semantic argv which precedes it, so remove exactly
    // that frozen terminal transport option before constructing AppRequest.
    let semantic_source = if surface == "discord-text" {
        source.strip_suffix(" --format text").unwrap_or_else(|| {
            panic!("{id}/{surface}/{variant}: text argv lacks frozen format suffix")
        })
    } else {
        assert!(
            !source.contains(" --format "),
            "{id}/{surface}/{variant}: slash argv contains a text-only format option"
        );
        source
    };
    // The redesigned Discord text route no longer injects these execution
    // flags into closed PC product subcommands. The frozen compatibility
    // fixture still records them, so project the same retired transport-era
    // flags away as the Discord fixture test before asking the CLI authority
    // to validate semantic argv.
    let semantic_source = semantic_source
        .split_whitespace()
        .filter(|token| !matches!(*token, "--no-tablebase" | "--no-build-dependency-dag"))
        .collect::<Vec<_>>()
        .join(" ");
    CliCommandParser::parse(&semantic_source)
        .unwrap_or_else(|error| {
            panic!("{id}/{surface}/{variant}: CliCommandParser rejected fixture: {error}")
        })
        .to_app_request()
        .unwrap_or_else(|error| {
            panic!("{id}/{surface}/{variant}: AppRequest construction failed: {error}")
        })
}

fn fixture_string<'a>(block: &'a str, field: &str) -> &'a str {
    let marker = format!("      \"{field}\": \"");
    let value = block
        .split_once(marker.as_str())
        .unwrap_or_else(|| panic!("fixture case lacks string field '{field}'"))
        .1
        .split_once('"')
        .expect("fixture string terminator")
        .0;
    assert!(
        !value.contains('\\'),
        "fixture command strings must not require ad-hoc JSON unescaping"
    );
    value
}

fn expected_alias_families(capability_id: &str) -> (&'static str, &'static str) {
    match capability_id {
        "pc.path" | "pc.score-finder" => ("pc-clear-to-empty", "pc-scenario"),
        "pc.minimals" => ("pc-clear-to-empty.v2", "pc-minimum-cover.v2"),
        "pc.score-minimals" => ("pc-clear-to-empty.v2", "pc-score-portfolio.v2"),
        "pc.allspin-sol" => ("pc-b2b-preservation.v1", "pc-b2b-preserving-witness.v1"),
        "pc.allspin-pres-chance" => (
            "pc-b2b-preservation.v1",
            "pc-b2b-preservation-probability.v1",
        ),
        "build.cover" => ("build-base-target", "build-probability"),
        "build.finesse-score" => ("fixed-placement-finesse-score.v2", "finesse-input-score.v2"),
        "setup.joint" => ("setup-ranking-joint", "setup-ranking"),
        "setup.build" => ("setup-ranking-build", "setup-ranking"),
        "setup.pc" => ("setup-ranking-pc", "setup-ranking"),
        "forward.spin" => ("ordered-forward-spin-search", "forward-spin"),
        "forward.damage" => ("ordered-forward-damage-search", "forward-damage"),
        "forward.ren" => ("ordered-forward-ren-search", "forward-ren"),
        _ => panic!("fixture contains an ungoverned capability family: {capability_id}"),
    }
}

#[test]
fn typed_document_utilities_require_explicit_format_and_lower_to_closed_app_commands() {
    let document = clearra_app::encode_ctk3_compact(&clearra_app::Ctk3Document::new(
        2,
        vec![clearra_app::Ctk3Page::new(
            1,
            vec![clearra_app::Ctk3Color::Gray, clearra_app::Ctk3Color::Empty],
        )],
    ))
    .unwrap();
    let parity = CliCommandParser::parse(&format!(
        "clearra utility parity --format ctk3 --document {document}"
    ))
    .unwrap();
    assert!(matches!(
        parity.to_app_request().unwrap().command(),
        clearra_app::AppCommand::UtilityParity(_)
    ));
    let render = CliCommandParser::parse(&format!(
        "clearra utility render --format ctk3 --document {document} --artifact-format png --page 1"
    ))
    .unwrap();
    assert!(matches!(
        render.to_app_request().unwrap().command(),
        clearra_app::AppCommand::UtilityRender(_)
    ));
    let to_gray = CliCommandParser::parse(&format!(
        "clearra utility to-gray --format ctk3 --document {document}"
    ))
    .unwrap();
    assert_eq!(to_gray.command_kind(), "utility-to-gray");
    assert!(matches!(
        to_gray.to_app_request().unwrap().command(),
        clearra_app::AppCommand::UtilityToGray(_)
    ));
    let mirror = CliCommandParser::parse(&format!(
        "clearra utility mirror --format ctk3 --document {document}"
    ))
    .unwrap();
    assert_eq!(mirror.command_kind(), "utility-mirror");
    assert!(matches!(
        mirror.to_app_request().unwrap().command(),
        clearra_app::AppCommand::UtilityMirror(_)
    ));
    assert!(
        CliCommandParser::parse(&format!("clearra utility parity --document {document}")).is_err()
    );
    assert!(CliCommandParser::parse(&format!(
        "clearra utility parity --format ctk3 --document {document} --workers 2"
    ))
    .is_err());
    assert!(
        CliCommandParser::parse(&format!("clearra utility to-gray --document {document}")).is_err()
    );
    assert!(CliCommandParser::parse(&format!(
        "clearra utility mirror --format ctk3 --document {document} --queue T"
    ))
    .is_err());
}

#[test]
fn typed_document_text_comments_preserve_quoted_spaces_without_shell_semantics() {
    let request = CliCommandParser::parse(
        "clearra utility fumen text-to-fumen --format fumen --comment \"first comment\" --comment second",
    )
    .unwrap()
    .to_app_request()
    .unwrap();
    let clearra_app::AppCommand::UtilityFumen(command) = request.command() else {
        panic!("expected utility fumen command")
    };
    assert_eq!(command.comments(), ["first comment", "second"]);
    assert!(CliCommandParser::parse(
        "clearra utility fumen text-to-fumen --format fumen --comment \"unterminated",
    )
    .is_err());
}

#[test]
fn quoted_cli_literals_preserve_process_markers_and_c0_whitespace_fieldwise() {
    let comment = "literal | && ` $(x) > < ; &\tline\nquote\" slash\\ bell\u{0007}";
    let encoded_comment = comment.replace('\\', "\\\\").replace('"', "\\\"");
    let command_text = format!(
        "clearra utility fumen text-to-fumen --format fumen --comment \"{encoded_comment}\""
    );
    let from_text = CliCommandParser::parse(&command_text).expect("quoted browser command text");
    let arguments = [
        "clearra",
        "utility",
        "fumen",
        "text-to-fumen",
        "--format",
        "fumen",
        "--comment",
        comment,
    ]
    .map(str::to_owned);
    let from_arguments =
        CliCommandParser::parse_tokens(&arguments).expect("exact Desktop/native CLI argv");

    assert_eq!(from_text, from_arguments);
    let app_request = from_text.to_app_request().expect("typed fumen request");
    let AppCommand::UtilityFumen(command) = app_request.command() else {
        panic!("expected utility fumen command")
    };
    assert_eq!(command.comments(), [comment]);
}

#[test]
fn unquoted_process_semantics_and_nul_fail_closed_before_cli_lowering() {
    for marker in ["|", "&", ";", "`", "$(", ">", "<", "\r", "\n"] {
        let command_text = format!(
            "clearra utility fumen text-to-fumen --format fumen --comment left{marker}right"
        );
        let error = CliCommandParser::parse(&command_text)
            .expect_err("unquoted shell/process syntax must fail closed");
        assert_eq!(
            error.code(),
            CliCommandErrorCode::ProcessSemantics,
            "{marker:?}"
        );
    }

    CliCommandParser::parse(
        "clearra pc tiling --lines 4 --board-mask 0xfc3f --height 4 --pieces 7 \
         --patterns IOOOOOO;OOOOOOO --no-hold --backend cpu",
    )
    .expect("an in-token semicolon is the public queue-pattern alternative grammar");
    let error = CliCommandParser::parse(
        "clearra pc tiling --lines 4 --board-mask 0xfc3f --height 4 --pieces 7 \
         --patterns IOOOOOO; clearra verify",
    )
    .expect_err("a separated semicolon remains process syntax");
    assert_eq!(error.code(), CliCommandErrorCode::ProcessSemantics);

    let error = CliCommandParser::parse(
        "clearra utility fumen text-to-fumen --format fumen --comment \"left\0right\"",
    )
    .expect_err("NUL must not cross the command-text boundary");
    assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);

    let arguments = [
        "clearra",
        "utility",
        "fumen",
        "text-to-fumen",
        "--format",
        "fumen",
        "--comment",
        "left\0right",
    ]
    .map(str::to_owned);
    let error = CliCommandParser::parse_tokens(&arguments)
        .expect_err("NUL must not cross the exact-argv boundary");
    assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
}
// SRP rationale: this module has one behavior-level change reason: verifying the complete public
// CLI command grammar reaches the intended typed Clearra request contracts.
