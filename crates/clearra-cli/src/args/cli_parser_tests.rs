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
fn ordinary_help_never_exposes_versioned_contracts_or_internal_identity_terms() {
    let topics = [
        CliHelpTopic::TopLevel,
        CliHelpTopic::Pc,
        CliHelpTopic::PcScenario,
        CliHelpTopic::Path,
        CliHelpTopic::Percent,
        CliHelpTopic::FailedQueue,
        CliHelpTopic::Setup,
        CliHelpTopic::Cover,
        CliHelpTopic::Rules,
        CliHelpTopic::Scoring,
        CliHelpTopic::Convert,
        CliHelpTopic::Continue,
        CliHelpTopic::SpinStructure,
        CliHelpTopic::Sfinder,
        CliHelpTopic::Product(ProductHelpTopic::PcTiling),
        CliHelpTopic::Product(ProductHelpTopic::PcMinimals),
        CliHelpTopic::Product(ProductHelpTopic::PcPath),
        CliHelpTopic::Product(ProductHelpTopic::PcChance),
        CliHelpTopic::Product(ProductHelpTopic::PcScore),
        CliHelpTopic::Product(ProductHelpTopic::PcScoreFinder),
        CliHelpTopic::Product(ProductHelpTopic::PcScoreMinimals),
        CliHelpTopic::Product(ProductHelpTopic::PcSaves),
        CliHelpTopic::Product(ProductHelpTopic::PcBestSave),
        CliHelpTopic::Product(ProductHelpTopic::PcFailedQueue),
        CliHelpTopic::Product(ProductHelpTopic::PcAllSpinSolution),
        CliHelpTopic::Product(ProductHelpTopic::PcAllSpinPreservationChance),
        CliHelpTopic::Product(ProductHelpTopic::BuildV2),
        CliHelpTopic::Product(ProductHelpTopic::BuildProbability),
        CliHelpTopic::Product(ProductHelpTopic::Finesse),
        CliHelpTopic::Product(ProductHelpTopic::Damage),
        CliHelpTopic::Product(ProductHelpTopic::SpinFinder),
        CliHelpTopic::Product(ProductHelpTopic::Ren),
        CliHelpTopic::Product(ProductHelpTopic::MappedCompatibility),
    ];
    let forbidden_identity_terms = [
        "ctk1",
        "field_id",
        "field id",
        "candidate_id",
        "candidate id",
        "canonical candidate",
        "problem_id",
        "problem id",
        "pattern_id",
        "pattern id",
        "trace_identity",
        "trace identity",
        "trace_key",
        "trace key",
        "operation_id",
        "operation id",
        "group_key",
        "group key",
        "schema_id",
        "schema id",
    ];

    for topic in topics {
        let output = topic.into_output(LanguageId::En);
        let text = output.stdout().to_ascii_lowercase();
        let has_versioned_contract = text
            .as_bytes()
            .windows(3)
            .any(|window| window[0] == b'.' && window[1] == b'v' && window[2].is_ascii_digit());
        assert!(
            !has_versioned_contract,
            "ordinary help exposes a versioned product contract: {topic:?}\n{text}"
        );
        for forbidden in forbidden_identity_terms {
            assert!(
                !text.contains(forbidden),
                "ordinary help exposes internal term {forbidden:?}: {topic:?}\n{text}"
            );
        }
    }
}

#[test]
fn build_probability_help_uses_the_canonical_parser_option_names() {
    let output =
        CliHelpTopic::Product(ProductHelpTopic::BuildProbability).into_output(LanguageId::En);
    for marker in [
        "--base-mask HEX",
        "--aggregate buildability|tiling|spin",
        "--solution-probabilities",
        "--include-mirror|--no-mirror",
        "--max-patterns N",
        "--max-candidates N",
        "--max-memory-mib N",
    ] {
        assert!(output.stdout().contains(marker), "missing marker: {marker}");
    }
    for stale in [
        "--initial-mask HEX",
        "--aggregation",
        "--gpu-warmup",
        "--gpu-device",
    ] {
        assert!(!output.stdout().contains(stale), "stale marker: {stale}");
    }
}

#[test]
fn build_v2_routes_to_the_product_boundary_and_owns_closed_help() {
    let parsed = CliParser::parse([
        "clearra",
        "build",
        "setup",
        "--target-format",
        "ctk3",
        "--target-document",
        "ctk3_test",
        "--queue",
        "I",
    ])
    .expect("Build v2 product command")
    .into_command();
    let ParsedCliCommand::Product(tokens) = parsed else {
        panic!("Build v2 must route through Product/Web");
    };
    assert_eq!(tokens[0..3], ["clearra", "build", "setup"]);

    assert_eq!(
        CliParser::parse(["clearra", "build", "--help"])
            .expect("Build v2 help")
            .into_command(),
        ParsedCliCommand::Help(CliHelpTopic::Product(ProductHelpTopic::BuildV2))
    );
    let help = CliHelpTopic::Product(ProductHelpTopic::BuildV2).into_output(LanguageId::En);
    for marker in [
        "build evaluate",
        "--target-format ctk3|fumen",
        "--solution-format ctk3|fumen",
        "--objective all|unique|min-cover|max-probability-minimum|max-score-cover",
        "--score-profile tetrio|guideline|jstris-ultra",
        "rejects --max-memory-mib",
    ] {
        assert!(help.stdout().contains(marker), "missing marker: {marker}");
    }
}

#[test]
fn pc_allspin_help_keeps_exact_queue_and_pattern_contracts_distinct() {
    let exact =
        CliHelpTopic::Product(ProductHelpTopic::PcAllSpinSolution).into_output(LanguageId::En);
    assert!(exact
        .stdout()
        .contains("pc allspin-sol --lines 2|4|6 --queue QUEUE"));
    assert!(exact
        .stdout()
        .contains("deterministic B2B-preserving witness"));
    assert!(exact.stdout().contains("denominator is exactly one"));
    assert!(exact.stdout().contains("oracle-fixed"));
    assert!(exact.stdout().contains("command-intent only"));
    assert!(exact
        .stdout()
        .contains("--board-mask HEX --height 1..6 --pieces N"));
    assert!(exact.stdout().contains("no target-field input exists"));
    assert!(!exact.stdout().contains("--patterns PATTERN"));

    let chance = CliHelpTopic::Product(ProductHelpTopic::PcAllSpinPreservationChance)
        .into_output(LanguageId::En);
    assert!(chance
        .stdout()
        .contains("pc allspin-pres-chance --lines 2|4|6 --patterns PATTERN"));
    assert!(chance.stdout().contains("original-queue denominator"));
    assert!(chance
        .stdout()
        .contains("Each original queue is counted once"));
    assert!(chance.stdout().contains("command-intent only"));
    assert!(chance
        .stdout()
        .contains("--board-mask HEX --height 1..6 --pieces N"));
    assert!(!chance.stdout().contains("--queue QUEUE"));
}

#[test]
fn pc_allspin_subcommands_route_to_the_shared_product_boundary() {
    for subcommand in ["allspin-sol", "allspin-pres-chance"] {
        let parsed = CliParser::parse([
            "clearra",
            "pc",
            subcommand,
            if subcommand == "allspin-sol" {
                "--queue"
            } else {
                "--patterns"
            },
            "IOTS",
            "--spin-profile",
            "all-spin-plus",
        ])
        .expect("PC All-Spin product command")
        .into_command();
        let ParsedCliCommand::Product(tokens) = parsed else {
            panic!("expected product command");
        };
        assert_eq!(tokens[0..3], ["clearra", "pc", subcommand]);
    }

    assert_eq!(
        CliParser::parse(["clearra", "pc", "allspin-sol", "--help"])
            .expect("exact help")
            .into_command(),
        ParsedCliCommand::Help(CliHelpTopic::Product(ProductHelpTopic::PcAllSpinSolution))
    );
    assert_eq!(
        CliParser::parse(["clearra", "pc", "allspin-pres-chance", "-h"])
            .expect("chance help")
            .into_command(),
        ParsedCliCommand::Help(CliHelpTopic::Product(
            ProductHelpTopic::PcAllSpinPreservationChance
        ))
    );
}

#[test]
fn gui_pc_full_solution_argv_routes_to_the_shared_compiler_and_keeps_legacy_pc_intact() {
    let expected =
        include_str!("../../../../tests/fixtures/contracts/gui_pc_full_solution_argv.tsv")
            .trim_end()
            .split('\t')
            .collect::<Vec<_>>();

    for count in ["unique", "all"] {
        let mut arguments = expected.clone();
        let count_index = arguments
            .iter()
            .position(|argument| *argument == "--count")
            .expect("GUI PC argv count option")
            + 1;
        arguments[count_index] = count;
        let parsed = CliParser::parse(arguments.iter().copied())
            .unwrap_or_else(|error| panic!("canonical GUI PC argv: {error:?}"))
            .into_command();
        let ParsedCliCommand::Product(tokens) = parsed else {
            panic!("GUI PC full-solution argv must use the shared CLI compiler");
        };
        assert_eq!(
            tokens.iter().map(String::as_str).collect::<Vec<_>>(),
            arguments
        );
    }

    assert!(matches!(
        CliParser::parse(["clearra", "pc", "--lines", "2", "--queue", "IOTSZ", "--fixed"])
            .expect("v0.7.4 native PC compatibility invocation")
            .into_command(),
        ParsedCliCommand::Pc(_)
    ));
}

#[test]
fn pc_minimals_routes_only_the_grouped_canonical_spelling_to_product_help_and_tokens() {
    let help = CliParser::parse(["clearra", "pc", "minimals", "--help"])
        .expect("canonical pc minimals help")
        .into_command();
    assert_eq!(
        help,
        ParsedCliCommand::Help(CliHelpTopic::Product(ProductHelpTopic::PcMinimals))
    );
    let output = CliHelpTopic::Product(ProductHelpTopic::PcMinimals).into_output(LanguageId::En);
    assert!(output
        .stdout()
        .contains("usage: clearra pc minimals --lines 2"));
    assert!(output
        .stdout()
        .contains("dedicated minimum-solution search"));
    assert!(output.stdout().contains("exact query-bound minimum cover"));
    for hidden in ["objective", "diagnostic", "verify"] {
        assert!(
            !output.stdout().to_ascii_lowercase().contains(hidden),
            "{hidden}"
        );
    }

    let canonical = CliParser::parse([
        "clearra",
        "pc",
        "minimals",
        "--lines",
        "1",
        "--board-mask",
        "0x3f",
        "--height",
        "1",
        "--pieces",
        "1",
        "--queue",
        "I",
    ])
    .expect("canonical pc minimals")
    .into_command();
    let ParsedCliCommand::Product(tokens) = canonical else {
        panic!("canonical pc minimals must route through Product/Web");
    };
    assert_eq!(tokens[0..3], ["clearra", "pc", "minimals"]);

    let legacy = CliParser::parse([
        "clearra",
        "sfinder",
        "minimals",
        "--field-mask-v1",
        "000000000000003f",
        "--queue",
        "I",
        "--lines",
        "1",
    ])
    .expect("legacy sfinder minimals")
    .into_command();
    let ParsedCliCommand::Product(tokens) = legacy else {
        panic!("legacy sfinder minimals must remain a compatibility product route");
    };
    assert_eq!(tokens[0..3], ["clearra", "sfinder", "minimals"]);
}

#[test]
fn pc_chance_routes_only_the_grouped_canonical_spelling_to_product_help_and_tokens() {
    let help = CliParser::parse(["clearra", "pc", "chance", "--help"])
        .expect("canonical pc chance help")
        .into_command();
    assert_eq!(
        help,
        ParsedCliCommand::Help(CliHelpTopic::Product(ProductHelpTopic::PcChance))
    );
    let output = CliHelpTopic::Product(ProductHelpTopic::PcChance).into_output(LanguageId::En);
    assert!(output
        .stdout()
        .contains("usage: clearra pc chance --lines 2"));
    assert!(output.stdout().contains("dedicated PC probability search"));
    for hidden in ["objective", "diagnostic", "verify"] {
        assert!(
            !output.stdout().to_ascii_lowercase().contains(hidden),
            "{hidden}"
        );
    }
    assert!(!output.stdout().contains("visible-7"));

    let canonical = CliParser::parse([
        "clearra",
        "pc",
        "chance",
        "--lines",
        "2",
        "--patterns",
        "[TI]!",
    ])
    .expect("canonical pc chance")
    .into_command();
    let ParsedCliCommand::Product(tokens) = canonical else {
        panic!("canonical pc chance must route through Product/Web");
    };
    assert_eq!(tokens[0..3], ["clearra", "pc", "chance"]);

    assert!(matches!(
        CliParser::parse(["clearra", "chance", "v115@vhAAgH", "P7P3", "4"])
            .expect("legacy chance")
            .into_command(),
        ParsedCliCommand::Product(_)
    ));
    assert!(matches!(
        CliParser::parse(["clearra", "percent", "--queue", "IOT"])
            .expect("legacy percent")
            .into_command(),
        ParsedCliCommand::Percent(_)
    ));
}

#[test]
fn pc_score_routes_only_the_grouped_canonical_spelling_to_product_help_and_tokens() {
    let help = CliParser::parse(["clearra", "pc", "score", "--help"])
        .expect("canonical pc score help")
        .into_command();
    assert_eq!(
        help,
        ParsedCliCommand::Help(CliHelpTopic::Product(ProductHelpTopic::PcScore))
    );
    let output = CliHelpTopic::Product(ProductHelpTopic::PcScore).into_output(LanguageId::En);
    assert!(output
        .stdout()
        .contains("usage: clearra pc score --lines 2"));
    assert!(output.stdout().contains("PC field-average score result"));
    assert!(output.stdout().contains("basic approximation"));
    assert!(output
        .stdout()
        .contains("not profile-specific exact values"));
    assert!(output
        .stdout()
        .contains("automatic execution reserves one logical processor"));
    assert!(output
        .stdout()
        .contains("Browser execution keeps N on the coordinator"));
    for worker_option in [
        "--workers N|--auto-workers N",
        "--use-all-cpu-threads",
        "--cpu-warmup",
    ] {
        assert!(output.stdout().contains(worker_option), "{worker_option}");
    }
    assert!(output.stdout().contains("16 source pieces"));
    assert!(output
        .stdout()
        .contains("one factorized pattern expression"));
    assert!(output.stdout().contains("P7P7P2"));
    for hidden in [
        "objective",
        "diagnostic",
        "verify",
        "--backend",
        "--gpu-device",
        "--max-patterns",
        "--max-memory-mib",
    ] {
        assert!(
            !output.stdout().to_ascii_lowercase().contains(hidden),
            "{hidden}"
        );
    }

    let canonical = CliParser::parse([
        "clearra",
        "pc",
        "score",
        "--lines",
        "2",
        "--patterns",
        "[TIOSZ]!",
    ])
    .expect("canonical pc score")
    .into_command();
    let ParsedCliCommand::Product(tokens) = canonical else {
        panic!("canonical pc score must route through Product/Web");
    };
    assert_eq!(tokens[0..3], ["clearra", "pc", "score"]);

    for source in [
        &["clearra", "score", "v115@vhAAgH", "P7P3", "4"][..],
        &["clearra", "sfinder", "score", "v115@vhAAgH", "P7P3", "4"][..],
    ] {
        assert!(matches!(
            CliParser::parse(source.iter().copied())
                .expect("legacy score")
                .into_command(),
            ParsedCliCommand::Product(_)
        ));
    }
}

#[test]
fn pc_score_finder_help_and_explicit_family_view_are_closed() {
    let help = CliParser::parse(["clearra", "pc", "score-finder", "--help"])
        .expect("canonical pc score-finder help")
        .into_command();
    assert_eq!(
        help,
        ParsedCliCommand::Help(CliHelpTopic::Product(ProductHelpTopic::PcScoreFinder))
    );
    let output = CliHelpTopic::Product(ProductHelpTopic::PcScoreFinder).into_output(LanguageId::En);
    assert!(output
        .stdout()
        .contains("dedicated fixed-queue maximum-score search"));
    assert!(output.stdout().contains("score only"));
    assert!(output.stdout().contains("attack is informational"));
    assert!(output.stdout().contains("no portfolio tie metadata"));
    assert!(output
        .stdout()
        .contains("automatic execution reserves one logical processor"));
    assert!(output
        .stdout()
        .contains("Browser execution keeps N on the coordinator"));
    assert!(output.stdout().contains("--workers N|--auto-workers N"));

    let invocation = CliParser::parse([
        "clearra",
        "pc",
        "score-finder",
        "--lines",
        "2",
        "--queue",
        "IOTSZ",
        "--ties",
    ])
    .expect("explicit fixed-score winner-family view");
    assert!(invocation.explicit_ties().requested());
    assert!(invocation.explicit_ties().snapshot_path().is_none());
    assert!(CliParser::parse([
        "clearra",
        "pc",
        "score-finder",
        "--lines",
        "2",
        "--queue",
        "IOTSZ",
        "--ties",
        "--tie-snapshot",
        "score-finder.jsonl",
    ])
    .is_err());
}

#[test]
fn pc_save_commands_route_to_distinct_product_help_and_tokens() {
    for (subcommand, topic, product_marker) in [
        ("saves", ProductHelpTopic::PcSaves, "Returns save groups"),
        (
            "best-save",
            ProductHelpTopic::PcBestSave,
            "Returns the best save groups",
        ),
    ] {
        let help = CliParser::parse(["clearra", "pc", subcommand, "--help"])
            .expect("canonical pc save help")
            .into_command();
        assert_eq!(help, ParsedCliCommand::Help(CliHelpTopic::Product(topic)));
        let output = CliHelpTopic::Product(topic).into_output(LanguageId::En);
        assert!(output.stdout().contains(product_marker), "{subcommand}");
        assert!(output.stdout().contains("whole-universe"), "{subcommand}");
        assert!(
            output.stdout().contains("fixed bag-boundary"),
            "{subcommand}"
        );

        let parsed = CliParser::parse([
            "clearra",
            "pc",
            subcommand,
            "--lines",
            "2",
            "--patterns",
            "P7",
        ])
        .expect("canonical pc save command")
        .into_command();
        let ParsedCliCommand::Product(tokens) = parsed else {
            panic!("canonical pc {subcommand} must route through Product/Web");
        };
        assert_eq!(tokens[0..3], ["clearra", "pc", subcommand]);
    }

    let saves = CliHelpTopic::Product(ProductHelpTopic::PcSaves).into_output(LanguageId::En);
    assert!(saves
        .stdout()
        .contains("conditional probability among successful PC queues"));
    let best = CliHelpTopic::Product(ProductHelpTopic::PcBestSave).into_output(LanguageId::En);
    assert!(best.stdout().contains("documented save weights"));
    assert!(best.stdout().contains("ordinary list entry"));
    assert!(best.stdout().contains("never uses portfolio tie semantics"));
}

#[test]
fn search_help_exposes_only_runnable_rule_and_profile_choices() {
    let pc = CliHelpTopic::Pc.into_output(LanguageId::En);
    assert!(pc
        .stdout()
        .contains("--rule srs-plus|srs|srs-x|jstris-180|no-kick"));
    assert!(!pc.stdout().contains("|asc|ars|"));
    assert!(pc
        .stdout()
        .contains("ASC and ARS remain inspectable rule-registry profiles"));

    let damage = CliHelpTopic::Product(ProductHelpTopic::Damage).into_output(LanguageId::En);
    assert!(damage.stdout().contains(
        "--spin-profile disabled|t-spin-simple|t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus"
    ));
    assert!(damage
        .stdout()
        .contains("default spin profile: all-mini-plus"));
    assert!(damage
        .stdout()
        .contains("--minimum-damage selects at-least mode"));

    let spin = CliHelpTopic::Product(ProductHelpTopic::SpinFinder).into_output(LanguageId::En);
    assert!(spin.stdout().contains(
        "--spin-profile t-spin-simple|t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus"
    ));
    assert!(!spin.stdout().contains("--spin-profile disabled|"));
    assert!(spin
        .stdout()
        .contains("--spin-category other requires an all-spin or all-mini profile"));

    let finesse = CliHelpTopic::Product(ProductHelpTopic::Finesse).into_output(LanguageId::En);
    assert!(finesse.stdout().contains("--hold empty|PIECE|--no-hold"));
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
fn solution_artifact_flags_are_global_and_default_to_compact_without_rewriting_allspin() {
    let invocation = CliParser::parse([
        "clearra",
        "pc",
        "allspin-sol",
        "--queue",
        "IOTS",
        "--spin-profile",
        "all-spin",
        "--solution-output",
        "solutions.csa",
    ])
    .expect("AllSpin artifact invocation");
    let artifact = invocation
        .solution_artifact_output()
        .expect("artifact request");
    assert_eq!(artifact.target(), std::path::Path::new("solutions.csa"));
    assert_eq!(artifact.format(), SolutionArtifactOutputFormat::Compact);
    let ParsedCliCommand::Product(tokens) = invocation.into_command() else {
        panic!("AllSpin must keep the product command boundary");
    };
    assert_eq!(tokens[0..3], ["clearra", "pc", "allspin-sol"]);
    assert!(!tokens.iter().any(|token| token == "--solution-output"));
}

#[test]
fn solution_artifact_json_selection_may_appear_before_the_command() {
    let invocation = CliParser::parse([
        "clearra",
        "--solution-artifact-format",
        "json",
        "--solution-output",
        "solutions.json",
        "pc",
        "--lines",
        "2",
    ])
    .expect("JSON artifact invocation");
    let artifact = invocation
        .solution_artifact_output()
        .expect("artifact request");
    assert_eq!(artifact.format(), SolutionArtifactOutputFormat::Json);
    assert!(matches!(invocation.into_command(), ParsedCliCommand::Pc(_)));
}

#[test]
fn artifact_format_without_target_and_legacy_fumen_like_stdout_fail_during_parse() {
    assert_eq!(
        CliParser::parse([
            "clearra",
            "pc",
            "--lines",
            "2",
            "--solution-artifact-format",
            "json",
        ]),
        Err(CliParseError::InvalidValue {
            option: "--solution-artifact-format",
            value: "requires --solution-output".to_owned(),
        })
    );
    assert_eq!(
        CliParser::parse([
            "clearra",
            "pc",
            "--lines",
            "2",
            "--format",
            "fumen-like",
            "--solution-output",
            "solutions.csa",
        ]),
        Err(CliParseError::InvalidValue {
            option: "--solution-output",
            value: "is incompatible with --format fumen-like".to_owned(),
        })
    );
}

#[test]
fn native_document_formats_are_available_for_stdout_and_artifacts() {
    for (format, expected) in [
        ("ctk3", SolutionArtifactOutputFormat::Ctk3),
        ("fumen", SolutionArtifactOutputFormat::Fumen),
    ] {
        let stdout = CliParser::parse(["clearra", "pc", "--lines", "2", "--format", format])
            .expect("native document stdout invocation");
        assert_eq!(stdout.format(), RenderFormat::Text);
        assert_eq!(stdout.solution_stdout_format(), Some(expected));
        assert!(stdout.solution_artifact_output().is_none());

        let artifact = CliParser::parse([
            "clearra",
            "pc",
            "--lines",
            "2",
            "--solution-output",
            "solutions.bin",
            "--solution-artifact-format",
            format,
        ])
        .expect("native document artifact invocation");
        assert_eq!(
            artifact
                .solution_artifact_output()
                .expect("artifact request")
                .format(),
            expected
        );
        assert_eq!(artifact.solution_stdout_format(), None);
    }
}

#[test]
fn native_document_stdout_rejects_conflicting_output_controls() {
    assert_eq!(
        CliParser::parse([
            "clearra",
            "pc",
            "--lines",
            "2",
            "--format",
            "ctk3",
            "--solution-output",
            "solutions.ctk3",
        ]),
        Err(CliParseError::InvalidValue {
            option: "--solution-output",
            value: "is incompatible with native document stdout".to_owned(),
        })
    );
    assert_eq!(
        CliParser::parse([
            "clearra",
            "pc",
            "--lines",
            "2",
            "--format",
            "fumen",
            "--verbose",
        ]),
        Err(CliParseError::InvalidValue {
            option: "--format",
            value: "native document stdout does not accept verbose profiles".to_owned(),
        })
    );
}

#[test]
fn top_level_help_describes_native_solution_document_formats() {
    let output = CliHelpTopic::TopLevel.into_output(LanguageId::En);
    assert!(output.stdout().contains("--solution-output PATH"));
    assert!(output
        .stdout()
        .contains("--solution-artifact-format compact|json|ctk3|fumen"));
    assert!(output
        .stdout()
        .contains("Native CTK3/Fumen encoding has no JavaScript, subprocess, network, or browser runtime dependency"));
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
        CliParser::parse(["clearra", "pc", "--score", "--initial-b2b", "65535"])
            .expect("maximum initial B2B")
            .into_command()
    else {
        panic!("PC command");
    };
    assert_eq!(maximum.initial_b2b(), Some(65_535));

    assert_eq!(
        CliParser::parse(["clearra", "pc", "--score", "--initial-b2b", "65536"]),
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
fn native_pc_parses_b2b_preservation_without_enabling_scoring() {
    let parsed = CliParser::parse([
        "clearra",
        "pc",
        "--lines",
        "4",
        "--preserve-b2b",
        "--spin-profile",
        "all-spin-plus",
    ])
    .expect("B2B-preserving PC")
    .into_command();
    let ParsedCliCommand::Pc(args) = parsed else {
        panic!("expected PC command");
    };

    assert!(args.preserves_back_to_back());
    assert_eq!(args.spin_profile(), Some("all-spin-plus"));
    assert!(!args.score_requested());
}

#[test]
fn native_pc_spin_profile_requires_a_scoring_or_preservation_consumer() {
    assert_eq!(
        CliParser::parse([
            "clearra",
            "pc",
            "--lines",
            "4",
            "--spin-profile",
            "all-spin-plus",
        ]),
        Err(CliParseError::InvalidValue {
            option: "--spin-profile",
            value: "requires --score or --preserve-b2b".to_owned(),
        })
    );
}

#[test]
fn native_pc_initial_b2b_remains_a_scoring_only_option() {
    assert_eq!(
        CliParser::parse(["clearra", "pc", "--lines", "4", "--initial-b2b", "1"]),
        Err(CliParseError::InvalidValue {
            option: "--initial-b2b",
            value: "requires --score".to_owned(),
        })
    );
}

#[test]
fn native_pc_tiling_rejects_b2b_preservation() {
    assert_eq!(
        CliParser::parse([
            "clearra",
            "pc",
            "--lines",
            "4",
            "--tiling-only",
            "--preserve-b2b",
        ]),
        Err(CliParseError::InvalidValue {
            option: "--preserve-b2b",
            value: "not available with tiling-only search".to_owned(),
        })
    );
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
    let sfinder_help = CliHelpTopic::Sfinder.into_output(LanguageId::En);
    assert!(sfinder_help
        .stdout()
        .contains("spin/spincover are unordered structural searches"));
    let mapped_commands = sfinder_help
        .stdout()
        .lines()
        .find(|line| line.starts_with("Clearra-native mappings:"))
        .expect("mapping inventory");
    assert!(!mapped_commands.contains("spin-cover"));
    assert!(!mapped_commands.contains(", spin,"));

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

#[test]
fn parses_canonical_pc_failed_queue_as_product_but_keeps_top_level_spellings_generic() {
    let canonical = CliParser::parse([
        "clearra",
        "pc",
        "failed-queue",
        "--lines",
        "2",
        "--patterns",
        "P5",
    ])
    .expect("canonical pc failed-queue")
    .into_command();
    let ParsedCliCommand::Product(tokens) = canonical else {
        panic!("canonical pc failed-queue must route through Product/Web");
    };
    assert_eq!(tokens[0..3], ["clearra", "pc", "failed-queue"]);

    for spelling in ["failed-queue", "failed_queue"] {
        let parsed = CliParser::parse(["clearra", spelling, "--lines", "2"])
            .unwrap_or_else(|_| panic!("top-level {spelling}"))
            .into_command();
        assert!(
            matches!(parsed, ParsedCliCommand::FailedQueue(_)),
            "{spelling}"
        );
    }

    let grouped_underscore = CliParser::parse(["clearra", "pc", "failed_queue", "--lines", "2"])
        .expect("grouped underscore is routed to the rejecting Product boundary")
        .into_command();
    assert!(matches!(grouped_underscore, ParsedCliCommand::Product(_)));
}

#[test]
fn pc_score_minimals_help_describes_score_only_portfolio_paging() {
    let help = CliParser::parse(["clearra", "pc", "score-minimals", "--help"])
        .expect("canonical pc score-minimals help")
        .into_command();
    assert_eq!(
        help,
        ParsedCliCommand::Help(CliHelpTopic::Product(ProductHelpTopic::PcScoreMinimals))
    );
    let output =
        CliHelpTopic::Product(ProductHelpTopic::PcScoreMinimals).into_output(LanguageId::En);
    assert!(output.stdout().contains("highest-score minimum-set search"));
    assert!(output.stdout().contains("never use attack"));
    assert!(output
        .stdout()
        .contains("automatic execution reserves one logical processor"));
    assert!(output
        .stdout()
        .contains("Browser execution keeps N on the coordinator"));
    assert!(output.stdout().contains("--workers N|--auto-workers N"));
    assert!(output.stdout().contains("--tie-snapshot PATH"));
}

#[test]
fn explicit_pc_minimals_portfolio_options_are_typed_and_not_forwarded() {
    let invocation = CliParser::parse([
        "clearra",
        "--format",
        "json",
        "pc",
        "minimals",
        "--lines",
        "2",
        "--ties",
        "--tie-snapshot",
        "minimum-portfolios.jsonl",
    ])
    .expect("explicit pc minimals portfolio request");
    assert!(invocation.explicit_ties().requested());
    assert_eq!(
        invocation.explicit_ties().snapshot_path(),
        Some("minimum-portfolios.jsonl")
    );
    assert_eq!(invocation.explicit_ties().cursor(), None);
    let ParsedCliCommand::Product(tokens) = invocation.into_command() else {
        panic!("pc minimals must use the product boundary");
    };
    assert_eq!(&tokens[0..3], ["clearra", "pc", "minimals"]);
    assert!(!tokens.iter().any(|token| token.starts_with("--tie")));
}

#[test]
fn explicit_pc_score_winner_family_does_not_accept_a_portfolio_snapshot() {
    let invocation = CliParser::parse(["clearra", "pc", "score", "--lines", "2", "--ties"])
        .expect("explicit score winner-family view");
    assert!(invocation.explicit_ties().requested());
    assert_eq!(invocation.explicit_ties().snapshot_path(), None);
    assert!(matches!(
        invocation.into_command(),
        ParsedCliCommand::Product(_)
    ));

    assert!(CliParser::parse([
        "clearra",
        "pc",
        "score",
        "--ties",
        "--tie-snapshot",
        "score.jsonl",
    ])
    .is_err());
}

#[test]
fn explicit_pc_score_minimals_requires_and_preserves_a_restartable_snapshot_path() {
    let invocation = CliParser::parse([
        "clearra",
        "pc",
        "score-minimals",
        "--lines",
        "2",
        "--ties",
        "--tie-snapshot",
        "score-portfolios.jsonl",
    ])
    .expect("explicit score-minimals portfolio request");
    assert!(invocation.explicit_ties().requested());
    assert_eq!(
        invocation.explicit_ties().snapshot_path(),
        Some("score-portfolios.jsonl")
    );
    let ParsedCliCommand::Product(tokens) = invocation.into_command() else {
        panic!("expected score-minimals product command");
    };
    assert_eq!(&tokens[..3], ["clearra", "pc", "score-minimals"]);
    assert!(!tokens.iter().any(|token| token.starts_with("--tie")));

    assert!(
        CliParser::parse(["clearra", "pc", "score-minimals", "--lines", "2", "--ties",]).is_err()
    );
}

#[test]
fn tie_snapshot_continuation_is_fieldwise_distinct_from_positional_continuation() {
    let invocation = CliParser::parse([
        "clearra",
        "continue",
        "--tie-snapshot",
        "minimum-portfolios.jsonl",
        "--tie-cursor",
        "cpt1.secret.payload.mac",
    ])
    .expect("typed portfolio continuation");
    assert!(!invocation.explicit_ties().requested());
    assert_eq!(
        invocation.explicit_ties().snapshot_path(),
        Some("minimum-portfolios.jsonl")
    );
    assert_eq!(
        invocation.explicit_ties().cursor(),
        Some("cpt1.secret.payload.mac")
    );
    let ParsedCliCommand::Continue(args) = invocation.into_command() else {
        panic!("continue command");
    };
    assert_eq!(args.token(), None);

    assert!(CliParser::parse([
        "clearra",
        "continue",
        "legacy-positional-token",
        "--tie-snapshot",
        "minimum-portfolios.jsonl",
        "--tie-cursor",
        "cpt1.secret.payload.mac",
    ])
    .is_err());
}

#[test]
fn tie_options_are_explicit_only_complete_and_nonduplicated() {
    assert!(CliParser::parse(["clearra", "pc", "chance", "--ties"]).is_err());
    assert!(CliParser::parse([
        "clearra",
        "pc",
        "minimals",
        "--tie-snapshot",
        "minimum-portfolios.jsonl",
    ])
    .is_err());
    assert!(CliParser::parse(["clearra", "pc", "minimals", "--ties"]).is_err());
    assert!(CliParser::parse([
        "clearra",
        "pc",
        "minimals",
        "--ties",
        "--ties",
        "--tie-snapshot",
        "minimum-portfolios.jsonl",
    ])
    .is_err());
}

#[test]
fn exact_build_portfolios_alone_accept_explicit_restartable_tie_paging() {
    for command in [
        &["build", "cover"][..],
        &["build", "congruent-cover"][..],
        &["build", "setup-cover"][..],
        &["build", "setup-cover-score"][..],
        &["build", "evaluate", "minimals"][..],
        &["build", "evaluate", "score"][..],
    ] {
        let mut args = vec!["clearra"];
        args.extend_from_slice(command);
        args.extend_from_slice(&["--ties", "--tie-snapshot", "build-portfolios.jsonl"]);
        let invocation = CliParser::parse(args).expect("exact Build portfolio tie request");
        assert!(invocation.explicit_ties().requested());
        assert_eq!(
            invocation.explicit_ties().snapshot_path(),
            Some("build-portfolios.jsonl")
        );
        let ParsedCliCommand::Product(tokens) = invocation.into_command() else {
            panic!("Build v2 tie route must retain Product tokens");
        };
        assert!(!tokens.iter().any(|token| token.starts_with("--tie")));
    }

    for command in [
        &["build", "setup"][..],
        &["build", "congruent"][..],
        &["build", "setup-cover-percent"][..],
        &["build", "evaluate", "cover"][..],
        &["build", "evaluate", "b2b-cover"][..],
        &["build", "evaluate", "cover-percent"][..],
    ] {
        let mut args = vec!["clearra"];
        args.extend_from_slice(command);
        args.extend_from_slice(&["--ties", "--tie-snapshot", "not-a-portfolio.jsonl"]);
        assert!(CliParser::parse(args).is_err(), "{command:?}");
    }
}

#[test]
fn build_portfolio_ties_require_a_snapshot_and_help_freezes_score_only_equality() {
    assert!(CliParser::parse(["clearra", "build", "cover", "--ties"]).is_err());
    assert!(CliParser::parse(["clearra", "build", "evaluate", "score", "--ties",]).is_err());

    let help = CliHelpTopic::Product(ProductHelpTopic::BuildV2).into_output(LanguageId::En);
    assert!(help
        .stdout()
        .contains("only through explicit --ties --tie-snapshot PATH"));
    assert!(help
        .stdout()
        .contains("Score equality and ordering never use attack"));
    assert!(help.stdout().contains("--base-mask HEX --target-mask HEX"));
    assert!(help.stdout().contains("nominally distinct"));
}

#[test]
fn spin_structure_cover_alone_accepts_restartable_exact_tie_paging() {
    let invocation = CliParser::parse([
        "clearra",
        "spin-structure",
        "cover",
        "--pieces",
        "T",
        "--ties",
        "--tie-snapshot",
        "spin-cover-portfolios.jsonl",
    ])
    .expect("explicit spin cover portfolio request");
    assert!(invocation.explicit_ties().requested());
    assert_eq!(
        invocation.explicit_ties().snapshot_path(),
        Some("spin-cover-portfolios.jsonl")
    );
    let ParsedCliCommand::Product(tokens) = invocation.into_command() else {
        panic!("spin-structure cover must retain Product tokens");
    };
    assert_eq!(&tokens[..3], ["clearra", "spin-structure", "cover"]);
    assert!(!tokens.iter().any(|token| token.starts_with("--tie")));

    assert!(CliParser::parse([
        "clearra",
        "spin-structure",
        "cover",
        "--pieces",
        "T",
        "--ties",
    ])
    .is_err());

    for route in ["search", "guaranteed"] {
        assert!(CliParser::parse([
            "clearra",
            "spin-structure",
            route,
            "--pieces",
            "T",
            "--ties",
            "--tie-snapshot",
            "not-a-portfolio.jsonl",
        ])
        .is_err());
    }
}

#[test]
fn spin_structure_help_freezes_the_three_closed_route_contracts() {
    let output = CliHelpTopic::SpinStructure.into_output(LanguageId::En);
    let help = output.stdout();
    assert!(help.contains("spin-structure search"));
    assert!(help.contains("spin-structure cover"));
    assert!(help.contains("spin-structure guaranteed"));
    assert!(help.contains("--ties --tie-snapshot PATH"));
    assert!(help.contains("unordered no-hold inventory"));
    assert!(help.contains(
        "Queue/pattern, hold, GPU, tablebase, and explicit memory options are unavailable"
    ));
    assert!(help.contains("every equal-cardinality optimum"));
}
