// SRP rationale: this test module has one behavior-level change reason: verifying the complete public CLI grammar and its typed request projection.

use clearra_i18n::LanguageId;

use super::*;

#[test]
fn top_level_help_lists_both_finesse_modes_and_build_probability_entry() {
    let output = CliHelpTopic::TopLevel.into_output(LanguageId::En);

    assert!(output
        .stdout()
        .contains("finesse search: clearra finesse search"));
    assert!(output
        .stdout()
        .contains("finesse score: clearra finesse score"));
    assert!(output
        .stdout()
        .contains("build-probability finesse: add --finesse inputs"));
}

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
fn tiling_only_rejects_build_dependency_preanalysis() {
    assert_eq!(
        CliParser::parse([
            "clearra",
            "pc",
            "--lines",
            "4",
            "--tiling-only",
            "--build-dependency-dag",
        ]),
        Err(CliParseError::InvalidValue {
            option: "--build-dependency-dag",
            value: "not available with tiling-only search".to_owned(),
        })
    );
}

#[test]
fn tiling_only_preserves_explicit_hold_policy() {
    let with_hold = CliParser::parse(["clearra", "pc", "--lines", "4", "--tiling-only", "--hold"])
        .expect("tiling-only with hold")
        .into_command();
    let ParsedCliCommand::Pc(with_hold) = with_hold else {
        panic!("expected PC command");
    };
    assert!(with_hold.hold_enabled());

    let without_hold = CliParser::parse([
        "clearra",
        "pc",
        "--lines",
        "4",
        "--tiling-only",
        "--no-hold",
    ])
    .expect("tiling-only without hold")
    .into_command();
    let ParsedCliCommand::Pc(without_hold) = without_hold else {
        panic!("expected PC command");
    };
    assert!(!without_hold.hold_enabled());
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
fn parses_automatic_worker_ceiling_without_forcing_parallel_selection() {
    let invocation = CliParser::parse(["clearra", "pc", "--lines", "4", "--auto-workers", "3"])
        .expect("adaptive CPU ceiling");

    let ParsedCliCommand::Pc(args) = invocation.into_command() else {
        panic!("expected pc command");
    };
    assert_eq!(args.workers(), None);
    assert_eq!(args.automatic_worker_limit(), Some(3));
}

#[test]
fn rejects_fixed_and_automatic_worker_counts_together() {
    assert!(matches!(
        CliParser::parse([
            "clearra",
            "pc",
            "--lines",
            "4",
            "--workers",
            "2",
            "--auto-workers",
            "3",
        ]),
        Err(CliParseError::InvalidValue {
            option: "--auto-workers",
            ..
        })
    ));
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
fn strips_solution_data_as_host_output_option() {
    let invocation = CliParser::parse([
        "clearra",
        "pc",
        "--include-solution-data",
        "--format",
        "json",
        "--lines",
        "2",
    ])
    .expect("parsed invocation");

    assert!(invocation.include_solution_data());
    assert!(matches!(invocation.into_command(), ParsedCliCommand::Pc(_)));
}

#[test]
fn solution_data_requires_structured_json_output() {
    assert_eq!(
        CliParser::parse(["clearra", "pc", "--include-solution-data", "--lines", "2",]),
        Err(CliParseError::InvalidValue {
            option: "--include-solution-data",
            value: "requires --format json".to_owned(),
        })
    );
}

#[test]
fn native_pc_backend_fallback_override_is_explicit_and_conflict_checked() {
    for arguments in [
        ["--allow-backend-fallback", "--no-backend-fallback"],
        ["--no-backend-fallback", "--allow-backend-fallback"],
    ] {
        assert_eq!(
            CliParser::parse(["clearra", "pc", "--lines", "2", arguments[0], arguments[1],]),
            Err(CliParseError::InvalidValue {
                option: "--backend-fallback",
                value: "choose exactly one of --allow-backend-fallback or --no-backend-fallback"
                    .to_owned(),
            })
        );
    }

    let ParsedCliCommand::Pc(default_pc) = CliParser::parse(["clearra", "pc"])
        .expect("native PC defaults")
        .into_command()
    else {
        panic!("PC command");
    };
    assert_eq!(default_pc.lines(), 2);
    assert_eq!(default_pc.objective(), "all");
    assert_eq!(default_pc.allow_backend_fallback(), None);
}

#[test]
fn native_pc_initial_b2b_uses_the_cross_surface_u16_domain() {
    let ParsedCliCommand::Pc(maximum) =
        CliParser::parse(["clearra", "pc", "--initial-b2b", "65535"])
            .expect("maximum initial B2B")
            .into_command()
    else {
        panic!("PC command");
    };
    assert_eq!(maximum.initial_b2b(), Some(65_535));

    assert_eq!(
        CliParser::parse(["clearra", "pc", "--initial-b2b", "65536"]),
        Err(CliParseError::InvalidValue {
            option: "--initial-b2b",
            value: "65536".to_owned(),
        })
    );
}

#[test]
fn native_pc_backend_and_fallback_projection_is_order_independent() {
    for (backend, fallback) in [
        ("auto", "--allow-backend-fallback"),
        ("auto", "--no-backend-fallback"),
        ("cpu", "--allow-backend-fallback"),
        ("cpu", "--no-backend-fallback"),
        ("gpu", "--allow-backend-fallback"),
        ("gpu", "--no-backend-fallback"),
        ("hybrid", "--allow-backend-fallback"),
        ("hybrid", "--no-backend-fallback"),
    ] {
        let backend_first = CliParser::parse(["clearra", "pc", "--backend", backend, fallback])
            .expect("backend-first PC invocation");
        let fallback_first = CliParser::parse(["clearra", "pc", fallback, "--backend", backend])
            .expect("fallback-first PC invocation");

        assert_eq!(
            backend_first, fallback_first,
            "backend={backend} {fallback}"
        );
    }
}

#[test]
fn native_pc_scenario_rejects_backend_fallback_conflicts_in_both_orders() {
    for arguments in [
        ["--allow-backend-fallback", "--no-backend-fallback"],
        ["--no-backend-fallback", "--allow-backend-fallback"],
    ] {
        assert_eq!(
            CliParser::parse([
                "clearra",
                "pc-scenario",
                "--fixture",
                "tests/fixtures/pc/example.json",
                arguments[0],
                arguments[1],
            ]),
            Err(CliParseError::InvalidValue {
                option: "--backend-fallback",
                value: "choose exactly one of --allow-backend-fallback or --no-backend-fallback"
                    .to_owned(),
            })
        );
    }
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
fn parses_spin_structure_help_without_executing_the_search() {
    for flag in ["--help", "-h"] {
        let invocation = CliParser::parse(["clearra", "spin-structure", flag]).expect("help");
        let ParsedCliCommand::Help(topic) = invocation.into_command() else {
            panic!("expected spin-structure help");
        };
        assert_eq!(topic, CliHelpTopic::SpinStructure);
        let output = topic.into_output(LanguageId::En);
        assert!(output.stdout().contains("--lines any|0..4|1+..4+"));
        assert!(!output.stdout().contains("0+..4+"));
    }
}

#[test]
fn every_generic_product_command_routes_help_without_executing_the_product() {
    let commands = [
        "build-probability",
        "finesse",
        "damage",
        "spin-finder",
        "spin-structure",
        "chance",
        "minimals",
        "score",
        "special-minimals",
        "special_minimals",
        "special-cover",
        "special_cover",
        "score-minimals",
        "score_minimals",
        "saves",
        "best-save",
        "best_save",
        "score-finder",
        "score_finder",
        "spin-cover",
        "spincover",
        "setup-cover",
        "setupcover",
        "congruent",
        "congruent-cover",
        "congruent_cover",
        "cover-percent",
        "cover_percent",
        "pc-setup",
        "pcsetup",
        "best-setup",
        "bestsetup",
        "dpc-finder",
        "dpcfinder",
        "parity",
        "to-gray",
        "togray",
        "to-fumen",
        "tofumen",
        "render",
    ];

    for command in commands {
        for flag in ["--help", "-h"] {
            let parsed = CliParser::parse(["clearra", command, flag])
                .unwrap_or_else(|error| panic!("{command} {flag}: {error:?}"));
            let ParsedCliCommand::Help(topic) = parsed.into_command() else {
                panic!("{command} {flag} executed instead of returning help");
            };
            let output = topic.into_output(LanguageId::En);
            assert!(output.stderr().is_empty(), "{command} {flag}");
            assert!(!output.stdout().is_empty(), "{command} {flag}");
            assert!(!output.stdout().contains("unsupported"), "{command} {flag}");
        }
    }
}

#[test]
fn pc_help_covers_the_public_b2b_preservation_option() {
    let output = CliHelpTopic::Pc.into_output(LanguageId::En);
    assert!(output.stdout().contains("--preserve-b2b"));
}

#[test]
fn canonical_clearra_commands_match_legacy_aliases() {
    let canonical_path = CliParser::parse(["clearra", "pc-replay", "--lines", "2"])
        .expect("canonical replay command")
        .into_command();
    let legacy_path = CliParser::parse(["clearra", "path", "--lines", "2"])
        .expect("legacy replay alias")
        .into_command();
    assert_eq!(canonical_path, legacy_path);

    let canonical_setup = CliParser::parse(["clearra", "setup-finder", "--remaining", "IOTS"])
        .expect("canonical setup finder command")
        .into_command();
    let legacy_setup = CliParser::parse(["clearra", "setup", "--remaining", "IOTS"])
        .expect("legacy setup alias")
        .into_command();
    assert_eq!(canonical_setup, legacy_setup);

    let canonical_coverage = CliParser::parse(["clearra", "build-coverage", "--template", "pc4"])
        .expect("canonical build coverage command")
        .into_command();
    let legacy_coverage = CliParser::parse(["clearra", "cover", "--template", "pc4"])
        .expect("legacy coverage alias")
        .into_command();
    assert_eq!(canonical_coverage, legacy_coverage);
}

#[test]
fn sfinder_namespace_does_not_collide_with_clearra_legacy_aliases() {
    let clearra_path = CliParser::parse(["clearra", "path", "--lines", "2"])
        .expect("Clearra path alias")
        .into_command();
    assert!(matches!(clearra_path, ParsedCliCommand::Path(_)));

    let sfinder_path =
        CliParser::parse(["clearra", "sfinder", "path", "v115@vhAAgH", "*p7,*p3", "4"])
            .expect("Sfinder path namespace")
            .into_command();
    let ParsedCliCommand::Product(tokens) = sfinder_path else {
        panic!("expected product compatibility command");
    };
    assert_eq!(&tokens[..3], ["clearra", "sfinder", "path"]);

    assert_eq!(
        CliParser::parse(["clearra", "sfinder", "--help"])
            .expect("Sfinder help")
            .into_command(),
        ParsedCliCommand::Help(CliHelpTopic::Sfinder)
    );

    let score_finder = CliParser::parse(["clearra", "score-finder"])
        .expect("score-finder product command")
        .into_command();
    assert!(matches!(score_finder, ParsedCliCommand::Product(_)));
    for retired in ["cat-finder", "cat_finder", "catfinder"] {
        assert_eq!(
            CliParser::parse(["clearra", retired]),
            Err(CliParseError::UnknownCommand {
                command: retired.to_owned()
            })
        );
    }
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
fn parses_setup_worker_allocation() {
    let invocation = CliParser::parse([
        "clearra",
        "setup",
        "--remaining",
        "IOTS",
        "--cpu-threads",
        "4",
        "--use-all-cpu-threads",
    ])
    .expect("setup command");

    let ParsedCliCommand::Setup(args) = invocation.into_command() else {
        panic!("expected setup command");
    };
    assert_eq!(args.workers(), Some(4));
    assert!(args.use_all_logical_processors());
}

#[test]
fn parses_setup_automatic_worker_ceiling() {
    let invocation = CliParser::parse([
        "clearra",
        "setup",
        "--remaining",
        "IOTS",
        "--auto-workers",
        "3",
    ])
    .expect("setup automatic worker ceiling");

    let ParsedCliCommand::Setup(args) = invocation.into_command() else {
        panic!("expected setup command");
    };
    assert_eq!(args.workers(), None);
    assert_eq!(args.automatic_worker_limit(), Some(3));
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

#[test]
fn parses_percent_failed_pattern_limit() {
    let parsed = CliParser::parse([
        "clearra",
        "percent",
        "--queue",
        "IOT",
        "--failed-count",
        "23",
    ])
    .expect("percent")
    .into_command();
    let ParsedCliCommand::Percent(args) = parsed else {
        panic!("expected percent command");
    };
    assert_eq!(args.failed_pattern_limit(), 23);
}

#[test]
fn parses_failed_queue_with_pc_options_and_exact_output_limit() {
    let parsed = CliParser::parse([
        "clearra",
        "failed-queue",
        "--lines",
        "4",
        "--patterns",
        "P7P3",
        "--failed-count",
        "41",
        "--workers",
        "2",
    ])
    .expect("failed-queue")
    .into_command();
    let ParsedCliCommand::FailedQueue(args) = parsed else {
        panic!("expected failed-queue command");
    };
    assert_eq!(args.pc().lines(), 4);
    assert_eq!(args.patterns(), Some("P7P3"));
    assert_eq!(args.pc().workers(), Some(2));
    assert_eq!(args.failed_pattern_limit(), 41);
}
