use clearra_app::{
    encode_ctk3_compact, AppCommand, BuildV2AppRequest, Ctk3Color, Ctk3Document, Ctk3Page,
    Ctk3Piece, PcChanceIngressOrigin, PcFailedQueueIngressOrigin, PcMinimalsIngressOrigin,
    PcResultProjection, PcScoreIngressOrigin, PcScoreMinimalsIngressOrigin,
    ProductCapabilityContract,
};
use clearra_core_domain::objective::objective_kind::ObjectiveKind;
use clearra_objectives::policy::{
    objective_policy::ObjectivePolicy,
    score_objective_policy::{ScoreProfileSelection, SpinProfileSelection},
};
use clearra_pc_graph::request::PcCountPolicy;
use clearra_supply::QueueObservationPolicy;

use crate::args::{CliParser, FailedQueueArgs, ParsedCliCommand, PcArgs, SetupArgs};

use super::*;

fn build_v2_colored_document() -> String {
    let mut cells = vec![Ctk3Color::Empty; 40];
    cells[0..4].fill(Ctk3Color::Piece(Ctk3Piece::I));
    encode_ctk3_compact(&Ctk3Document::new(10, vec![Ctk3Page::new(4, cells)]))
        .expect("one-piece Build v2 document")
}

fn build_v2_cli_cases() -> Vec<(String, &'static str)> {
    let document = build_v2_colored_document();
    let target = |path: &str, suffix: &str| {
        format!(
            "clearra build {path} --target-format ctk3 --target-document {document} --queue I --no-hold {suffix}"
        )
    };
    let supplied = |path: &str, suffix: &str| {
        format!(
            "clearra build evaluate {path} --solution-format ctk3 --solution-document {document} --queue I --no-hold {suffix}"
        )
    };
    vec![
        (
            "clearra build cover --base-mask 0 --target-mask 15 --height 4 --queue I --no-hold --objective max-probability-minimum".to_owned(),
            "cover",
        ),
        (target("setup", ""), "setup"),
        (target("congruent", "--objective all"), "congruent"),
        (
            target("congruent-cover", "--objective minimum-cover"),
            "congruent-cover",
        ),
        (
            target(
                "setup-cover",
                "--objective max-probability-minimum --queue-knowledge visible-7",
            ),
            "setup-cover",
        ),
        (
            target("setup-cover-percent", "--objective unique"),
            "setup-cover-percent",
        ),
        (
            target(
                "setup-cover-score",
                "--objective max-score-cover --score-profile guideline --initial-b2b 65535",
            ),
            "setup-cover-score",
        ),
        (
            supplied("cover", "--objective all"),
            "evaluate-cover",
        ),
        (
            supplied("minimals", "--objective min-cover"),
            "evaluate-minimals",
        ),
        (
            supplied(
                "score",
                "--objective max-score-cover --score-profile jstris-ultra --initial-b2b 7",
            ),
            "evaluate-score",
        ),
        (
            supplied("b2b-cover", "--objective all"),
            "evaluate-b2b-cover",
        ),
        (
            supplied("cover-percent", "--objective unique"),
            "evaluate-cover-percent",
        ),
    ]
}

fn build_v2_request_name(request: &BuildV2AppRequest) -> &'static str {
    match request {
        BuildV2AppRequest::BuildCover(_) => "cover",
        BuildV2AppRequest::BuildSetup(_) => "setup",
        BuildV2AppRequest::BuildCongruent(_) => "congruent",
        BuildV2AppRequest::BuildCongruentCover(_) => "congruent-cover",
        BuildV2AppRequest::BuildSetupCover(_) => "setup-cover",
        BuildV2AppRequest::BuildSetupCoverPercent(_) => "setup-cover-percent",
        BuildV2AppRequest::BuildSetupCoverScore(_) => "setup-cover-score",
        BuildV2AppRequest::BuildEvaluateCover(_) => "evaluate-cover",
        BuildV2AppRequest::BuildEvaluateMinimals(_) => "evaluate-minimals",
        BuildV2AppRequest::BuildEvaluateScore(_) => "evaluate-score",
        BuildV2AppRequest::BuildEvaluateB2bCover(_) => "evaluate-b2b-cover",
        BuildV2AppRequest::BuildEvaluateCoverPercent(_) => "evaluate-cover-percent",
    }
}

#[test]
fn every_native_build_v2_form_routes_and_assembles_the_exact_cpu_app_request() {
    for (source, expected) in build_v2_cli_cases() {
        let invocation = CliParser::parse(source.split_whitespace())
            .unwrap_or_else(|error| panic!("parse {source}: {error:?}"));
        let assembly =
            CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
                .unwrap_or_else(|output| panic!("assemble {source}: {}", output.stderr()));
        let request = assembly.request();
        assert_eq!(request.query(), &clearra_app::QueryEnvelope::BuildCoverage);
        assert_eq!(request.backend_policy().backend_requested(), "cpu");
        assert!(!request.backend_policy().allow_backend_fallback());
        assert_eq!(request.resource_budget().memory_mib(), None);
        let AppCommand::BuildV2(command) = request.command() else {
            panic!("{source} did not assemble BuildV2");
        };
        assert_eq!(
            build_v2_request_name(command.request()),
            expected,
            "{source}"
        );
    }
}

#[test]
fn native_build_v2_rejects_ungoverned_memory_authority_on_every_form() {
    for (source, _) in build_v2_cli_cases() {
        let invocation =
            CliParser::parse(format!("{source} --max-memory-mib 64").split_whitespace())
                .unwrap_or_else(|error| {
                    panic!("CLI routing should preserve Product validation: {error:?}")
                });
        let error = CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
            .expect_err("Build v2 max-memory authority must fail closed");
        assert!(
            error.stderr().contains("does not accept max-memory-mib"),
            "{source}: {}",
            error.stderr()
        );
    }
}

#[test]
fn every_native_build_v2_form_requires_exactly_one_queue_source() {
    for (source, expected) in build_v2_cli_cases() {
        let pattern_source = source.replacen(" --queue I", " --patterns I", 1);
        let invocation = CliParser::parse(pattern_source.split_whitespace())
            .unwrap_or_else(|error| panic!("parse {pattern_source}: {error:?}"));
        let assembly =
            CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
                .unwrap_or_else(|output| panic!("assemble {pattern_source}: {}", output.stderr()));
        let request = assembly.request();
        let AppCommand::BuildV2(command) = request.command() else {
            panic!("{pattern_source} did not assemble BuildV2");
        };
        assert_eq!(
            build_v2_request_name(command.request()),
            expected,
            "{pattern_source}"
        );

        for invalid in [
            source.replacen(" --queue I", "", 1),
            format!("{source} --patterns I"),
        ] {
            let invocation = CliParser::parse(invalid.split_whitespace())
                .unwrap_or_else(|error| panic!("preserve Product route for {invalid}: {error:?}"));
            let error =
                CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
                    .expect_err("Build v2 must reject absent or competing queue sources");
            assert!(
                error
                    .stderr()
                    .contains("exactly one of --queue or --patterns"),
                "{invalid}: {}",
                error.stderr()
            );
        }
    }
}

#[test]
fn gui_pc_full_solution_argv_has_native_web_app_request_and_result_family_parity() {
    let canonical_arguments =
        include_str!("../../../../tests/fixtures/contracts/gui_pc_full_solution_argv.tsv")
            .trim_end()
            .split('\t')
            .map(str::to_owned)
            .collect::<Vec<_>>();
    for (count, expected_count, expected_objective) in [
        (
            "unique",
            PcCountPolicy::CountUnique,
            ObjectivePolicy::unique(),
        ),
        ("all", PcCountPolicy::CountAll, ObjectivePolicy::all()),
    ] {
        let mut arguments = canonical_arguments.clone();
        let count_index = arguments
            .iter()
            .position(|argument| argument == "--count")
            .expect("GUI PC argv count option")
            + 1;
        arguments[count_index] = count.to_owned();
        let invocation = CliParser::parse(arguments.iter().map(String::as_str))
            .expect("native CLI accepts the complete GUI PC argv envelope");
        let ParsedCliCommand::Product(forwarded) = invocation.into_command() else {
            panic!("GUI PC argv must reach the shared CLI compiler");
        };

        let native_request = CliAppRequestAssembler::assemble(
            ParsedCliCommand::Product(forwarded.clone()),
            RenderFormat::Json,
        )
        .expect("native CLI GUI PC AppRequest")
        .request();
        let browser_request = clearra_cli_command::CliCommandParser::parse(&forwarded.join(" "))
            .and_then(|request| request.to_app_request())
            .expect("Web command-text GUI PC AppRequest");
        let desktop_request = clearra_cli_command::CliCommandParser::parse_tokens(&forwarded)
            .and_then(|request| request.to_app_request())
            .expect("Desktop argv GUI PC AppRequest");
        assert_eq!(native_request, browser_request);
        assert_eq!(native_request, desktop_request);
        assert_eq!(native_request.product_capability_contract(), None);

        let AppCommand::Scenario(command) = native_request.command() else {
            panic!("field-backed GUI PC argv must preserve the Scenario result family");
        };
        assert_eq!(command.result_projection(), PcResultProjection::Standard);
        assert_eq!(command.query().count_policy(), expected_count);
        assert_eq!(command.query().objective(), expected_objective);
    }
}

#[test]
fn cli_pc_builds_app_request() {
    let assembly =
        CliAppRequestAssembler::assemble(ParsedCliCommand::Pc(PcArgs::new(2)), RenderFormat::Text)
            .expect("pc app request");

    assert!(matches!(assembly.request().command(), AppCommand::Pc(_)));
}

#[test]
fn cli_pc_command_assembles_app_request() {
    let assembly =
        CliAppRequestAssembler::assemble(ParsedCliCommand::Pc(PcArgs::new(2)), RenderFormat::Text)
            .expect("pc app request");
    let (command, _, _, _) = assembly.request().into_parts();
    assert!(matches!(command, AppCommand::Pc(_)));
}

#[test]
fn cli_setup_command_assembles_app_request() {
    let assembly = CliAppRequestAssembler::assemble(
        ParsedCliCommand::Setup(SetupArgs::default()),
        RenderFormat::Text,
    )
    .expect("setup app request");
    let (command, _, _, _) = assembly.request().into_parts();
    assert!(matches!(command, AppCommand::Setup(_)));
}

#[test]
fn cli_setup_assembly_applies_the_shared_host_worker_policy() {
    let hardware = clearra_pc_graph::request::WorkerPolicy::hardware_worker_limit();
    let default_workers = clearra_pc_graph::request::WorkerPolicy::default_worker_limit();

    let default_request = CliAppRequestAssembler::assemble(
        ParsedCliCommand::Setup(SetupArgs::default()),
        RenderFormat::Text,
    )
    .expect("default setup request")
    .request();
    assert_eq!(
        usize::from(default_request.resource_budget().workers()),
        default_workers.min(usize::from(u16::MAX))
    );

    let all_request = CliAppRequestAssembler::assemble(
        ParsedCliCommand::Setup(
            SetupArgs::default()
                .with_workers(Some(0))
                .with_use_all_logical_processors(true),
        ),
        RenderFormat::Text,
    )
    .expect("all-logical-processors setup request")
    .request();
    assert_eq!(
        usize::from(all_request.resource_budget().workers()),
        hardware.min(usize::from(u16::MAX))
    );

    let cap = default_workers.min(3);
    let capped_request = CliAppRequestAssembler::assemble(
        ParsedCliCommand::Setup(SetupArgs::default().with_automatic_worker_limit(Some(cap))),
        RenderFormat::Text,
    )
    .expect("capped setup request")
    .request();
    assert_eq!(usize::from(capped_request.resource_budget().workers()), cap);
}

#[test]
fn cli_failed_queue_command_assembles_coverage_complement_request() {
    let assembly = CliAppRequestAssembler::assemble(
        ParsedCliCommand::FailedQueue(FailedQueueArgs::new(PcArgs::new(2), None, 9)),
        RenderFormat::Text,
    )
    .expect("failed-queue app request");
    let request = assembly.request();
    let AppCommand::Percent(command) = request.command() else {
        panic!("expected percent-backed failed-queue request");
    };
    assert!(command.is_failed_queue());
    assert_eq!(command.pc_failed_queue_origin(), None);
    assert_eq!(command.failed_pattern_limit(), 9);
    assert_eq!(request.product_capability_contract(), None);
}

#[test]
fn cli_pc_failed_queue_product_assembles_the_closed_v2_request() {
    let assembly = CliAppRequestAssembler::assemble(
        ParsedCliCommand::Product(vec![
            "clearra".to_owned(),
            "pc".to_owned(),
            "failed-queue".to_owned(),
            "--lines".to_owned(),
            "2".to_owned(),
            "--patterns".to_owned(),
            "P5".to_owned(),
            "--backend".to_owned(),
            "cpu".to_owned(),
            "--failed-count".to_owned(),
            "9".to_owned(),
        ]),
        RenderFormat::Json,
    )
    .expect("typed pc failed-queue AppRequest");
    let request = assembly.request();
    assert_eq!(
        request.product_capability_contract(),
        Some(ProductCapabilityContract::PcFailedQueue)
    );
    let AppCommand::Percent(command) = request.command() else {
        panic!("typed pc failed-queue remains Percent AppCommand");
    };
    assert_eq!(
        command.pc_failed_queue_origin(),
        Some(PcFailedQueueIngressOrigin::CanonicalFailedQueue)
    );
    assert_eq!(command.failed_pattern_limit(), 9);
}

#[test]
fn cli_pc_failed_queue_underscore_is_rejected_by_the_product_parser() {
    let result = CliAppRequestAssembler::assemble(
        ParsedCliCommand::Product(vec![
            "clearra".to_owned(),
            "pc".to_owned(),
            "failed_queue".to_owned(),
            "--lines".to_owned(),
            "2".to_owned(),
            "--patterns".to_owned(),
            "P5".to_owned(),
        ]),
        RenderFormat::Json,
    );
    assert!(result.is_err());
}

#[test]
fn cli_pc_minimals_product_assembles_the_closed_v2_request_through_the_public_web_parser() {
    let invocation = crate::args::CliParser::parse([
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
        "--hold",
        "empty",
        "--rule",
        "srs-plus",
    ])
    .expect("canonical pc minimals parser route");
    let assembly = CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
        .expect("typed pc minimals AppRequest");
    let request = assembly.request();
    assert_eq!(
        request.product_capability_contract(),
        Some(ProductCapabilityContract::PcMinimals)
    );
    let AppCommand::Scenario(command) = request.command() else {
        panic!("canonical field-backed pc minimals must remain a Scenario AppCommand");
    };
    assert_eq!(
        command.result_projection(),
        PcResultProjection::MinimumCoverV2(PcMinimalsIngressOrigin::CanonicalPcMinimals)
    );
    assert_eq!(
        command.query().objective(),
        ObjectivePolicy::minimum_cover()
    );
    assert_eq!(command.query().count_policy(), PcCountPolicy::CountUnique);
    assert_eq!(
        command.query().queue_observation_policy(),
        QueueObservationPolicy::FullQueueOracle
    );
}

#[test]
fn cli_pc_minimals_rejects_semantic_overrides_and_keeps_public_aliases_generic() {
    for source in [
        "clearra pc minimals --lines 1 --board-mask 0x3f --height 1 --pieces 1 --queue I --objective minimum-cover",
        "clearra pc minimals --lines 1 --board-mask 0x3f --height 1 --pieces 1 --queue I --count all",
        "clearra pc minimals --lines 1 --board-mask 0x3f --height 1 --pieces 1 --queue I --max-memory-mib 64",
    ] {
        let invocation = crate::args::CliParser::parse(source.split_whitespace())
            .unwrap_or_else(|_| panic!("CliParser must route {source} to the public Web boundary"));
        let result =
            CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json);
        assert!(result.is_err(), "{source}");
    }

    let source = concat!(
        "clearra sfinder minimals --field-mask-v1 000000000000003f ",
        "--queue I --lines 1"
    );
    let invocation = crate::args::CliParser::parse(source.split_whitespace())
        .expect("legacy sfinder minimals parser route");
    let assembly = CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
        .expect("generic legacy minimals AppRequest");
    let request = assembly.request();
    assert_eq!(request.product_capability_contract(), None);
    let AppCommand::Scenario(command) = request.command() else {
        panic!("legacy minimals must remain a generic Scenario AppCommand");
    };
    assert_eq!(command.result_projection(), PcResultProjection::Standard);
}

#[test]
fn cli_pc_chance_product_assembles_the_closed_v2_request() {
    let assembly = CliAppRequestAssembler::assemble(
        ParsedCliCommand::Product(vec![
            "clearra".to_owned(),
            "pc".to_owned(),
            "chance".to_owned(),
            "--lines".to_owned(),
            "2".to_owned(),
            "--patterns".to_owned(),
            "[TI]!".to_owned(),
        ]),
        RenderFormat::Json,
    )
    .expect("typed pc chance AppRequest");
    let request = assembly.request();
    assert_eq!(
        request.product_capability_contract(),
        Some(ProductCapabilityContract::PcChance)
    );
    let AppCommand::Pc(command) = request.command() else {
        panic!("canonical opening pc chance must remain a Pc AppCommand");
    };
    assert_eq!(
        command.result_projection(),
        PcResultProjection::ChanceProbabilityV2(PcChanceIngressOrigin::CanonicalPcChance)
    );
}

#[test]
fn cli_pc_chance_product_rejects_an_unaccounted_memory_override() {
    let result = CliAppRequestAssembler::assemble(
        ParsedCliCommand::Product(vec![
            "clearra".to_owned(),
            "pc".to_owned(),
            "chance".to_owned(),
            "--lines".to_owned(),
            "2".to_owned(),
            "--patterns".to_owned(),
            "[TI]!".to_owned(),
            "--max-memory-mib".to_owned(),
            "1".to_owned(),
        ]),
        RenderFormat::Json,
    );
    assert!(result.is_err());
}

#[test]
fn cli_pc_score_product_assembles_the_closed_v2_request_through_the_public_web_parser() {
    let invocation = crate::args::CliParser::parse([
        "clearra",
        "pc",
        "score",
        "--lines",
        "2",
        "--patterns",
        "[TIOSZ]!",
        "--score-profile",
        "tetrio",
    ])
    .expect("canonical pc score parser route");
    let assembly = CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
        .expect("typed pc score AppRequest");
    let request = assembly.request();
    assert_eq!(
        request.product_capability_contract(),
        Some(ProductCapabilityContract::PcScore)
    );
    let AppCommand::Pc(command) = request.command() else {
        panic!("canonical opening pc score must remain a Pc AppCommand");
    };
    assert_eq!(
        command.result_projection(),
        PcResultProjection::ScoreSummaryV2(PcScoreIngressOrigin::CanonicalPcScore)
    );
    assert_eq!(
        command.query().objective().score().profile(),
        ScoreProfileSelection::Tetrio
    );
}

#[test]
fn cli_pc_score_finder_assembles_the_fixed_queue_score_only_request() {
    let invocation = crate::args::CliParser::parse([
        "clearra",
        "pc",
        "score-finder",
        "--board-mask",
        "0x3f0",
        "--height",
        "1",
        "--pieces",
        "1",
        "--lines",
        "1",
        "--queue",
        "I",
        "--initial-b2b",
        "1",
    ])
    .expect("canonical pc score-finder parser route");
    let assembly = CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
        .expect("typed pc score-finder AppRequest");
    let request = assembly.request();
    assert_eq!(
        request.product_capability_contract(),
        Some(ProductCapabilityContract::PcScoreFinder)
    );
    let AppCommand::Scenario(command) = request.command() else {
        panic!("fixed-field pc score-finder must remain a Scenario AppCommand");
    };
    assert_eq!(
        command.result_projection(),
        PcResultProjection::ScoreSummaryV2(PcScoreIngressOrigin::CanonicalPcScoreFinder)
    );
    assert_eq!(
        command.query().objective().score().profile(),
        ScoreProfileSelection::JstrisUltra
    );
    assert_eq!(
        command.query().objective().score().spin_profile(),
        SpinProfileSelection::TSpins
    );
    assert_eq!(command.query().objective().score().initial_b2b(), 1);
}

#[test]
fn cli_pc_score_minimals_assembles_the_closed_score_only_portfolio_request() {
    let invocation = crate::args::CliParser::parse([
        "clearra",
        "pc",
        "score-minimals",
        "--lines",
        "2",
        "--patterns",
        "[TIOSZ]!",
        "--score-profile",
        "tetrio",
    ])
    .expect("canonical pc score-minimals parser route");
    let assembly = CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
        .expect("typed pc score-minimals AppRequest");
    let request = assembly.request();
    assert_eq!(
        request.product_capability_contract(),
        Some(ProductCapabilityContract::PcScoreMinimals)
    );
    let AppCommand::Pc(command) = request.command() else {
        panic!("canonical opening pc score-minimals must remain a Pc AppCommand");
    };
    assert_eq!(
        command.result_projection(),
        PcResultProjection::ScorePortfolioV2(
            PcScoreMinimalsIngressOrigin::CanonicalPcScoreMinimals,
        )
    );
    assert_eq!(
        command.query().objective().kind(),
        ObjectiveKind::MinimumCover
    );
    assert_eq!(
        command.query().objective().score().profile(),
        ScoreProfileSelection::Tetrio
    );
}

#[test]
fn cli_pc_score_product_rejects_authority_memory_and_execution_limit_overrides() {
    for source in [
        "clearra pc score --lines 2 --patterns [TIOSZ]! --objective all",
        "clearra pc score --lines 2 --patterns [TIOSZ]! --score",
        "clearra pc score --lines 2 --patterns [TIOSZ]! --count unique",
        "clearra pc score --lines 2 --patterns [TIOSZ]! --max-memory-mib 64",
        "clearra pc score --lines 2 --patterns [TIOSZ]! --max-patterns 1",
    ] {
        let invocation = crate::args::CliParser::parse(source.split_whitespace())
            .unwrap_or_else(|_| panic!("CliParser must route {source} to the public Web boundary"));
        let result =
            CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json);
        assert!(result.is_err(), "{source}");
    }
}

#[test]
fn cli_public_legacy_chance_and_score_products_assemble_without_typed_product_claims() {
    for (source, expects_jstris_score) in [
        ("clearra chance v115@vhAAgH P7P3 4", false),
        ("clearra sfinder chance v115@vhAAgH P7P3 4", false),
        ("clearra sfinder percent v115@vhAAgH P7P3 4", false),
        ("clearra score v115@vhAAgH P7P3 4", true),
        ("clearra sfinder score v115@vhAAgH P7P3 4", true),
    ] {
        let tokens = source.split_whitespace().map(str::to_owned).collect();
        let assembly =
            CliAppRequestAssembler::assemble(ParsedCliCommand::Product(tokens), RenderFormat::Json)
                .unwrap_or_else(|_| panic!("{source}: product assembly failed"));
        let request = assembly.request();
        assert_eq!(request.product_capability_contract(), None, "{source}");
        let AppCommand::Scenario(command) = request.command() else {
            panic!("expected scenario-backed PC command for {source}");
        };
        assert_eq!(
            command.result_projection(),
            PcResultProjection::Standard,
            "{source}"
        );
        if expects_jstris_score {
            assert_eq!(
                command.query().objective().score().profile(),
                ScoreProfileSelection::JstrisUltra,
                "{source}"
            );
        } else {
            assert!(!command.query().objective().score().requested(), "{source}");
        }
    }
}

#[test]
fn cli_top_level_percent_remains_an_untyped_generic_request() {
    let invocation = crate::args::CliParser::parse([
        "clearra",
        "percent",
        "--queue",
        "IOT",
        "--fixed",
        "--min-len",
        "3",
    ])
    .expect("top-level percent");
    let assembly = CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
        .expect("generic percent AppRequest");
    let request = assembly.request();
    assert_eq!(request.product_capability_contract(), None);
    let AppCommand::Percent(command) = request.command() else {
        panic!("top-level percent must remain a Percent AppCommand");
    };
    assert!(!command.is_failed_queue());
    assert_eq!(command.pc_failed_queue_origin(), None);
}

#[test]
fn cli_pc_allspin_product_assembles_typed_existential_projection() {
    let assembly = CliAppRequestAssembler::assemble(
        ParsedCliCommand::Product(vec![
            "clearra".to_owned(),
            "pc".to_owned(),
            "allspin-sol".to_owned(),
            "--lines".to_owned(),
            "2".to_owned(),
            "--queue".to_owned(),
            "IOTSZ".to_owned(),
            "--spin-profile".to_owned(),
            "all-spin-plus".to_owned(),
        ]),
        RenderFormat::Json,
    )
    .expect("typed PC All-Spin request");
    let request = assembly.request();
    let AppCommand::Pc(command) = request.command() else {
        panic!("expected PC command");
    };

    assert_eq!(
        command.result_projection(),
        PcResultProjection::AllSpinSolution(SpinProfileSelection::AllSpinPlus)
    );
    assert!(command
        .query()
        .objective()
        .execution_constraints()
        .preserves_back_to_back());
    assert_eq!(
        command
            .query()
            .objective()
            .execution_constraints()
            .spin_profile(),
        SpinProfileSelection::AllSpinPlus
    );
    assert!(!command.query().objective().score().requested());
}

#[test]
fn cli_product_profiles_are_request_local_and_fail_closed() {
    let invocation = CliParser::parse([
        "clearra",
        "pc",
        "score",
        "--lines",
        "2",
        "--patterns",
        "[TIOSZ]!",
        "--board-profile",
        "standard-10",
        "--piece-profile",
        "standard-tetrominoes",
        "--bag-profile",
        "standard-7-bag",
        "--rule",
        "srs-x",
        "--spin-profile",
        "all-mini-plus",
        "--score-profile",
        "guideline",
    ])
    .expect("canonical CLI profile command");
    let assembly = CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
        .expect("verified request-local profiles");
    let profiles = assembly.request().request_profiles();
    assert_eq!(profiles.board().as_str(), "standard-10");
    assert_eq!(profiles.piece_set().as_str(), "standard-tetrominoes");
    assert_eq!(profiles.bag().as_str(), "standard-7-bag");
    assert_eq!(profiles.rule().as_str(), "srs-x");
    assert_eq!(profiles.spin().as_str(), "all-mini-plus");
    assert_eq!(profiles.score().as_str(), "guideline");

    for (option, value) in [
        ("--board-profile", "wide-10"),
        ("--piece-profile", "pentominoes"),
        ("--bag-profile", "history-6-rolls"),
        ("--rule", "custom"),
        ("--spin-profile", "unverified-spin"),
        ("--score-profile", "classic-score"),
    ] {
        let invocation = CliParser::parse([
            "clearra",
            "pc",
            "score",
            "--lines",
            "2",
            "--patterns",
            "[TIOSZ]!",
            option,
            value,
        ])
        .expect("canonical CLI syntax remains routed to Product/Web");
        let result =
            CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json);
        assert!(
            result.is_err(),
            "{option}={value} must reject without fallback"
        );
    }
}
