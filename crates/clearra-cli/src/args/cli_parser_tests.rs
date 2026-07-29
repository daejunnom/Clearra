use clearra_i18n::LanguageId;

use super::*;

#[test]
fn tablebase_use_is_explicit_for_pc_and_setup() {
    let ParsedCliCommand::Pc(default_pc) = CliParser::parse(["clearra", "pc", "--lines", "4"])
        .expect("default PC invocation")
        .into_command()
    else {
        panic!("expected pc command");
    };
    assert_eq!(default_pc.tablebase_requested(), None);

    let ParsedCliCommand::Pc(enabled_pc) =
        CliParser::parse(["clearra", "pc", "--lines", "4", "--tablebase"])
            .expect("tablebase PC invocation")
            .into_command()
    else {
        panic!("expected pc command");
    };
    assert_eq!(enabled_pc.tablebase_requested(), Some(true));

    let ParsedCliCommand::Setup(enabled_setup) =
        CliParser::parse(["clearra", "setup", "--remaining", "IOTSZJL", "--tb"])
            .expect("tablebase setup invocation")
            .into_command()
    else {
        panic!("expected setup command");
    };
    assert_eq!(enabled_setup.tablebase_requested(), Some(true));

    let ParsedCliCommand::Setup(disabled_setup) = CliParser::parse([
        "clearra",
        "setup",
        "--remaining",
        "IOTSZJL",
        "--no-tablebase",
    ])
    .expect("disabled tablebase setup invocation")
    .into_command() else {
        panic!("expected setup command");
    };
    assert_eq!(disabled_setup.tablebase_requested(), Some(false));
}

#[test]
fn build_dependency_dag_is_explicit_for_pc() {
    let ParsedCliCommand::Pc(default_pc) = CliParser::parse(["clearra", "pc", "--lines", "4"])
        .expect("default PC invocation")
        .into_command()
    else {
        panic!("expected pc command");
    };
    assert_eq!(default_pc.precompute_build_dependencies(), None);

    let ParsedCliCommand::Pc(enabled_pc) =
        CliParser::parse(["clearra", "pc", "--lines", "4", "--build-dependency-dag"])
            .expect("dependency DAG PC invocation")
            .into_command()
    else {
        panic!("expected pc command");
    };
    assert_eq!(enabled_pc.precompute_build_dependencies(), Some(true));

    let ParsedCliCommand::Pc(disabled_pc) =
        CliParser::parse(["clearra", "pc", "--lines", "4", "--no-build-dependency-dag"])
            .expect("disabled dependency DAG PC invocation")
            .into_command()
    else {
        panic!("expected pc command");
    };
    assert_eq!(disabled_pc.precompute_build_dependencies(), Some(false));
}

#[test]
fn parses_pc_args_outside_lib_router() {
    let invocation = CliParser::parse([
        "clearra",
        "pc",
        "--lines",
        "2",
        "--queue",
        "IOT",
        "--fixed",
        "--no-hold",
        "--objective",
        "unique",
        "--rule",
        "srs-x",
        "--kick-profile-json",
        "{}",
    ])
    .expect("parsed invocation");

    assert_eq!(invocation.format(), RenderFormat::Text);
    match invocation.into_command() {
        ParsedCliCommand::Pc(args) => {
            assert_eq!(args.lines(), 2);
            assert_eq!(args.queue(), "IOT");
            assert!(args.fixed_queue());
            assert!(!args.hold_enabled());
            assert_eq!(args.objective(), "unique");
            assert_eq!(args.rule(), Some("srs-x"));
            assert_eq!(args.kick_profile_json(), Some("{}"));
        }
        command => panic!("expected pc command, got {command:?}"),
    }
}

#[test]
fn parses_pc_queue_knowledge_policy() {
    let invocation = CliParser::parse([
        "clearra",
        "pc",
        "--lines",
        "4",
        "--queue-knowledge",
        "visible-7",
    ])
    .expect("visible-seven PC command");

    let ParsedCliCommand::Pc(args) = invocation.into_command() else {
        panic!("expected pc command");
    };
    assert_eq!(
        args.queue_observation_policy(),
        clearra_supply::queue::queue_observation_policy::QueueObservationPolicy::VisibleSeven
    );
}

#[test]
fn rejects_unknown_pc_queue_knowledge_policy() {
    assert!(matches!(
        CliParser::parse([
            "clearra",
            "pc",
            "--lines",
            "4",
            "--queue-knowledge",
            "clairvoyant",
        ]),
        Err(CliParseError::InvalidValue {
            option: "--queue-knowledge",
            ..
        })
    ));
}

#[test]
fn cpu_execution_aliases_select_cpu_and_preserve_thread_count() {
    let invocation = CliParser::parse([
        "clearra",
        "pc",
        "--lines",
        "4",
        "--cpu-threads",
        "6",
        "--no-gpu",
    ])
    .expect("CPU-only invocation");

    let ParsedCliCommand::Pc(args) = invocation.into_command() else {
        panic!("expected pc command");
    };
    assert_eq!(args.backend(), Some("cpu"));
    assert_eq!(args.workers(), Some(6));
    assert!(args.gpu_device().is_none());
}

#[test]
fn parses_explicit_cpu_pool_warmup_and_all_thread_opt_in() {
    let invocation = CliParser::parse([
        "clearra",
        "pc",
        "--lines",
        "4",
        "--workers",
        "1",
        "--use-all-cpu-threads",
        "--cpu-warmup",
    ])
    .expect("CPU pool options");

    let ParsedCliCommand::Pc(args) = invocation.into_command() else {
        panic!("expected pc command");
    };
    assert_eq!(args.workers(), Some(1));
    assert_eq!(args.use_all_logical_processors(), Some(true));
    assert_eq!(args.cpu_warmup(), Some(true));
}

#[test]
fn cpu_execution_aliases_reject_gpu_backend_and_device_conflicts() {
    assert!(matches!(
        CliParser::parse([
            "clearra",
            "pc",
            "--lines",
            "4",
            "--cpu-threads",
            "4",
            "--backend",
            "gpu",
        ]),
        Err(CliParseError::InvalidValue {
            option: "--backend",
            ..
        })
    ));
    assert!(matches!(
        CliParser::parse([
            "clearra",
            "pc-scenario",
            "--field",
            "0",
            "--queue",
            "I",
            "--no-gpu",
            "--gpu-device",
            "0",
        ]),
        Err(CliParseError::InvalidValue {
            option: "--gpu-device",
            ..
        })
    ));
}

#[test]
fn strips_global_format_before_command_parsing() {
    let invocation = CliParser::parse(["clearra", "pc", "--format", "json", "--lines", "2"])
        .expect("parsed invocation");

    assert_eq!(invocation.format(), RenderFormat::Json);
    assert!(matches!(invocation.into_command(), ParsedCliCommand::Pc(_)));
}

#[test]
fn strips_global_language_before_command_parsing() {
    let invocation = CliParser::parse(["clearra", "--lang", "ko-KR", "pc", "--lines", "2"])
        .expect("parsed invocation");

    assert_eq!(invocation.language(), LanguageId::Ko);
    assert!(matches!(invocation.into_command(), ParsedCliCommand::Pc(_)));
}

#[test]
fn rejects_unknown_language() {
    assert_eq!(
        CliParser::parse(["clearra", "--lang", "jp", "pc", "--lines", "2"]),
        Err(CliParseError::InvalidValue {
            option: "--lang",
            value: "jp".to_owned()
        })
    );
}

#[test]
fn strips_verbose_paths_as_global_option() {
    let invocation = CliParser::parse(["clearra", "cover", "--verbose-paths", "--template", "x"])
        .expect("parsed invocation");

    assert!(invocation.verbose_paths());
    assert!(matches!(
        invocation.into_command(),
        ParsedCliCommand::Cover(_)
    ));
}

#[test]
fn parses_text_output_verbosity_as_global_option() {
    let verbose =
        CliParser::parse(["clearra", "pc", "--verbose", "--lines", "2"]).expect("verbose");
    assert_eq!(verbose.output_verbosity(), OutputVerbosity::Verbose);
    assert_eq!(verbose.format(), RenderFormat::Text);

    let diagnostics =
        CliParser::parse(["clearra", "--diagnostics", "pc", "--lines", "2"]).expect("diagnostics");
    assert_eq!(diagnostics.output_verbosity(), OutputVerbosity::Diagnostics);
    assert_eq!(diagnostics.format(), RenderFormat::Text);
}

#[test]
fn parses_pc_scenario_fixture_args() {
    let invocation = CliParser::parse([
        "clearra",
        "pc-scenario",
        "--fixture",
        "tests/fixtures/pc/example.json",
        "--verify-expected",
        "--kick-profile-json",
        "{}",
        "--format",
        "json",
    ])
    .expect("parsed invocation");

    assert_eq!(invocation.format(), RenderFormat::Json);
    match invocation.into_command() {
        ParsedCliCommand::PcScenario(args) => {
            assert_eq!(args.fixture(), Some("tests/fixtures/pc/example.json"));
            assert!(args.verify_expected());
            assert_eq!(args.kick_profile_json(), Some("{}"));
        }
        command => panic!("expected pc-scenario command, got {command:?}"),
    }
}

#[test]
fn parses_cover_native_template_import_export_args() {
    let invocation = CliParser::parse([
        "clearra",
        "cover",
        "--template-file",
        "build-template.json",
        "--export-template-json",
    ])
    .expect("parsed invocation");

    match invocation.into_command() {
        ParsedCliCommand::Cover(args) => {
            assert_eq!(args.template_file(), Some("build-template.json"));
            assert!(args.export_template_json());
        }
        command => panic!("expected cover command, got {command:?}"),
    }
}

#[test]
fn parses_command_specific_help_as_help_topic() {
    let invocation = CliParser::parse(["clearra", "setup", "--help"]).expect("help");

    assert_eq!(
        invocation.into_command(),
        ParsedCliCommand::Help(CliHelpTopic::Setup)
    );
}

#[test]
fn parses_setup_candidate_priority() {
    let invocation = CliParser::parse([
        "clearra",
        "setup",
        "--remaining",
        "IOTS",
        "--priority",
        "build",
    ])
    .expect("setup command");

    let ParsedCliCommand::Setup(args) = invocation.into_command() else {
        panic!("expected setup command");
    };
    assert_eq!(
        args.candidate_priority(),
        clearra_setup_search::query::SetupCandidatePriority::BuildProbabilityFirst
    );
}

#[test]
fn parses_setup_length_preference() {
    let invocation = CliParser::parse([
        "clearra",
        "setup",
        "--remaining",
        "IOTS",
        "--setup-length",
        "shorter",
    ])
    .expect("setup command");

    let ParsedCliCommand::Setup(args) = invocation.into_command() else {
        panic!("expected setup command");
    };
    assert_eq!(
        args.length_preference(),
        clearra_setup_search::query::SetupLengthPreference::Shorter
    );
}

#[test]
fn parses_setup_kick_table_rule() {
    let invocation =
        CliParser::parse(["clearra", "setup", "--remaining", "IOTS", "--rule", "srs-x"])
            .expect("setup command");

    let ParsedCliCommand::Setup(args) = invocation.into_command() else {
        panic!("expected setup command");
    };
    assert_eq!(args.rule(), Some("srs-x"));
}

#[test]
fn parses_jstris_180_setup_kick_table_rule() {
    let invocation = CliParser::parse([
        "clearra",
        "setup",
        "--remaining",
        "IOTS",
        "--rule",
        "jstris-180",
    ])
    .expect("setup command");

    let ParsedCliCommand::Setup(args) = invocation.into_command() else {
        panic!("expected setup command");
    };
    assert_eq!(args.rule(), Some("jstris-180"));
}

#[test]
fn parses_setup_piece_limit_including_the_complete_pc() {
    let invocation = CliParser::parse([
        "clearra",
        "setup",
        "--remaining",
        "IOTS",
        "--max-setup-pieces",
        "10",
    ])
    .expect("setup command");

    let ParsedCliCommand::Setup(args) = invocation.into_command() else {
        panic!("expected setup command");
    };
    assert_eq!(args.max_setup_pieces(), 10);

    for invalid in ["0", "11"] {
        assert!(matches!(
            CliParser::parse([
                "clearra",
                "setup",
                "--remaining",
                "IOTS",
                "--max-setup-pieces",
                invalid,
            ]),
            Err(CliParseError::InvalidValue {
                option: "--max-setup-pieces",
                ..
            })
        ));
    }
}

#[test]
fn parses_queue_based_setup_mode() {
    let invocation = CliParser::parse([
        "clearra",
        "setup",
        "--remaining",
        "TI",
        "--mode",
        "qb",
        "--qb",
        "OS",
    ])
    .expect("QB setup command");

    let ParsedCliCommand::Setup(args) = invocation.into_command() else {
        panic!("expected setup command");
    };
    assert_eq!(
        args.search_mode(),
        clearra_setup_search::query::SetupSearchMode::QueueBased
    );
    assert_eq!(args.remaining(), "TI");
    assert_eq!(args.queue_based_pieces(), Some("OS"));
    assert_eq!(args.next_cycle_remaining_pieces(), None);
}

#[test]
fn parses_setup_queue_knowledge_independently_from_search_mode() {
    let invocation = CliParser::parse([
        "clearra",
        "setup",
        "--remaining",
        "TI",
        "--qb",
        "OS",
        "--queue-knowledge",
        "visible-7",
    ])
    .expect("visible-seven QB setup command");

    let ParsedCliCommand::Setup(args) = invocation.into_command() else {
        panic!("expected setup command");
    };
    assert_eq!(
        args.search_mode(),
        clearra_setup_search::query::SetupSearchMode::QueueBased
    );
    assert_eq!(
        args.queue_observation_policy(),
        clearra_supply::queue::queue_observation_policy::QueueObservationPolicy::VisibleSeven
    );
}

#[test]
fn rejects_unknown_setup_queue_knowledge_policy() {
    assert!(matches!(
        CliParser::parse([
            "clearra",
            "setup",
            "--remaining",
            "TI",
            "--queue-knowledge",
            "clairvoyant",
        ]),
        Err(CliParseError::InvalidValue {
            option: "--queue-knowledge",
            ..
        })
    ));
}

#[test]
fn parses_queue_based_setup_shorthand() {
    let invocation = CliParser::parse(["clearra", "setup", "--remaining", "TI", "--qb", "OS"])
        .expect("QB setup command");

    let ParsedCliCommand::Setup(args) = invocation.into_command() else {
        panic!("expected setup command");
    };
    assert_eq!(
        args.search_mode(),
        clearra_setup_search::query::SetupSearchMode::QueueBased
    );
    assert_eq!(args.queue_based_pieces(), Some("OS"));
}

#[test]
fn parses_next_cycle_inventory_without_enabling_queue_based_mode() {
    let invocation = CliParser::parse([
        "clearra",
        "setup",
        "--remaining",
        "TI",
        "--next-cycle-remaining",
        "OOSITZ",
    ])
    .expect("oracle setup command");

    let ParsedCliCommand::Setup(args) = invocation.into_command() else {
        panic!("expected setup command");
    };
    assert_eq!(
        args.search_mode(),
        clearra_setup_search::query::SetupSearchMode::ShapeOracle
    );
    assert_eq!(args.queue_based_pieces(), None);
    assert_eq!(args.next_cycle_remaining_pieces(), Some("OOSITZ"));
}

#[test]
fn parses_observed_qb_and_next_cycle_inventory_together() {
    let invocation = CliParser::parse([
        "clearra",
        "setup",
        "--remaining",
        "TI",
        "--qb",
        "OS",
        "--next-cycle-remaining",
        "OOSITZ",
    ])
    .expect("combined setup command");

    let ParsedCliCommand::Setup(args) = invocation.into_command() else {
        panic!("expected setup command");
    };
    assert_eq!(args.queue_based_pieces(), Some("OS"));
    assert_eq!(args.next_cycle_remaining_pieces(), Some("OOSITZ"));
}

#[test]
fn parses_setup_initial_hold_as_an_explicit_cli_only_option() {
    let invocation = CliParser::parse([
        "clearra",
        "setup",
        "--remaining",
        "IOTS",
        "--initial-hold",
        "S",
    ])
    .expect("setup command");

    let ParsedCliCommand::Setup(args) = invocation.into_command() else {
        panic!("expected setup command");
    };
    assert_eq!(args.initial_hold(), Some("S"));
}

#[test]
fn parses_setup_path_detail_as_an_atomic_option_pair() {
    let invocation = CliParser::parse([
        "clearra",
        "setup",
        "--remaining",
        "IOTS",
        "--paths-for",
        "setup-004011c4f9-0002-000000000000000000000000000001",
        "--condition",
        "hold-T",
    ])
    .expect("setup path detail command");

    let ParsedCliCommand::Setup(args) = invocation.into_command() else {
        panic!("expected setup command");
    };
    assert_eq!(
        args.path_detail_setup_id(),
        Some("setup-004011c4f9-0002-000000000000000000000000000001")
    );
    assert_eq!(args.path_detail_condition_id(), Some("hold-T"));

    assert!(matches!(
        CliParser::parse([
            "clearra",
            "setup",
            "--remaining",
            "IOTS",
            "--paths-for",
            "setup-004011c4f9-0002-000000000000000000000000000001",
        ]),
        Err(CliParseError::InvalidValue {
            option: "--condition",
            ..
        })
    ));
}

#[test]
fn rejects_oracle_mode_with_observed_qb_regardless_of_option_order() {
    for args in [
        [
            "clearra",
            "setup",
            "--remaining",
            "TI",
            "--mode",
            "oracle",
            "--qb",
            "OS",
        ],
        [
            "clearra",
            "setup",
            "--remaining",
            "TI",
            "--qb",
            "OS",
            "--mode",
            "oracle",
        ],
    ] {
        assert!(matches!(
            CliParser::parse(args),
            Err(CliParseError::InvalidValue {
                option: "--mode",
                ..
            })
        ));
    }
}

#[test]
fn parses_continue_token_as_concrete_command() {
    let token = "pc2:l2:bdstandard-10:psstandard-tetrominoes:bgstandard-7-bag:rsrs-plus:oall:e0:hnone:qIIOOO";
    let invocation = CliParser::parse(["clearra", "continue", token]).expect("continue");

    assert_eq!(invocation.format(), RenderFormat::Text);
    match invocation.into_command() {
        ParsedCliCommand::Continue(args) => {
            assert_eq!(args.token(), Some(token));
        }
        command => panic!("expected continue command, got {command:?}"),
    }
}

#[test]
fn reports_missing_and_invalid_option_values() {
    assert_eq!(
        CliParser::parse(["clearra", "pc", "--lines"]),
        Err(CliParseError::MissingValue { option: "--lines" })
    );
    assert_eq!(
        CliParser::parse(["clearra", "pc", "--lines", "two"]),
        Err(CliParseError::InvalidValue {
            option: "--lines",
            value: "two".to_owned()
        })
    );
}

#[test]
fn classifies_unsupported_and_unknown_commands() {
    let unsupported = CliParser::parse(["clearra", "inspect"]).expect("unsupported command");
    assert_eq!(
        unsupported.into_command(),
        ParsedCliCommand::Unsupported("inspect".to_owned())
    );
    assert_eq!(
        CliParser::parse(["clearra", "wat"]),
        Err(CliParseError::UnknownCommand {
            command: "wat".to_owned()
        })
    );
}

#[test]
fn parses_mvp2_cli_command_surfaces() {
    assert!(matches!(
        CliParser::parse(["clearra", "rules", "list"])
            .expect("rules")
            .into_command(),
        ParsedCliCommand::Rules(_)
    ));
    assert!(matches!(
        CliParser::parse(["clearra", "scoring", "inspect", "--profile", "jstris-ultra"])
            .expect("scoring")
            .into_command(),
        ParsedCliCommand::Scoring(_)
    ));
    assert!(matches!(
        CliParser::parse(["clearra", "path", "--lines", "2"])
            .expect("path")
            .into_command(),
        ParsedCliCommand::Path(_)
    ));
    assert!(matches!(
        CliParser::parse(["clearra", "percent", "--queue", "IOT"])
            .expect("percent")
            .into_command(),
        ParsedCliCommand::Percent(_)
    ));
}
