use clearra_app::{
    encode_ctk3_compact, AppCommand, AppContext, AppCoreExecutorService, AppServices, AppStatus,
    BuildObjective, BuildV2AppRequest, Ctk3Color, Ctk3Document, Ctk3Page, Ctk3Piece, QueryEnvelope,
};
use clearra_pc_graph::request::{RequestedSearchBackend, SupplyWindowSize};

use crate::{
    CliCommandErrorCode, CliCommandParser, CliCommandRequest, WebBuildV2Capability, WebBuildV2Input,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExpectedRequest {
    Cover,
    Setup,
    Congruent,
    CongruentCover,
    SetupCover,
    SetupCoverPercent,
    SetupCoverScore,
    EvaluateCover,
    EvaluateMinimals,
    EvaluateScore,
    EvaluateB2bCover,
    EvaluateCoverPercent,
}

fn colored_target_document() -> String {
    let mut cells = vec![Ctk3Color::Empty; 40];
    cells[0..4].fill(Ctk3Color::Piece(Ctk3Piece::I));
    encode_ctk3_compact(&Ctk3Document::new(10, vec![Ctk3Page::new(4, cells)]))
        .expect("one-piece Build v2 CTK3 document")
}

fn canonical_commands() -> Vec<(String, WebBuildV2Capability, ExpectedRequest)> {
    let document = colored_target_document();
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
            WebBuildV2Capability::Cover,
            ExpectedRequest::Cover,
        ),
        (
            format!("clearra build pinned-minimals --base-mask 0 --target-mask 15 --height 4 --queue I --no-hold --required-format ctk3 --required-document {document}"),
            WebBuildV2Capability::PinnedMinimals,
            ExpectedRequest::Cover,
        ),
        (
            target("setup", ""),
            WebBuildV2Capability::Setup,
            ExpectedRequest::Setup,
        ),
        (
            target("congruent", "--objective all"),
            WebBuildV2Capability::Congruent,
            ExpectedRequest::Congruent,
        ),
        (
            target("congruent-cover", "--objective minimum-cover"),
            WebBuildV2Capability::CongruentCover,
            ExpectedRequest::CongruentCover,
        ),
        (
            target(
                "setup-cover",
                "--objective max-probability-minimum --queue-knowledge visible-7",
            ),
            WebBuildV2Capability::SetupCover,
            ExpectedRequest::SetupCover,
        ),
        (
            target("setup-cover-percent", "--objective unique"),
            WebBuildV2Capability::SetupCoverPercent,
            ExpectedRequest::SetupCoverPercent,
        ),
        (
            target(
                "setup-cover-score",
                "--objective max-score-cover --score-profile guideline --initial-b2b 65535",
            ),
            WebBuildV2Capability::SetupCoverScore,
            ExpectedRequest::SetupCoverScore,
        ),
        (
            supplied("cover", "--objective all"),
            WebBuildV2Capability::EvaluateCover,
            ExpectedRequest::EvaluateCover,
        ),
        (
            supplied("minimals", "--objective min-cover"),
            WebBuildV2Capability::EvaluateMinimals,
            ExpectedRequest::EvaluateMinimals,
        ),
        (
            supplied(
                "score",
                "--objective max-score-cover --score-profile jstris-ultra --initial-b2b 7",
            ),
            WebBuildV2Capability::EvaluateScore,
            ExpectedRequest::EvaluateScore,
        ),
        (
            supplied("b2b-cover", "--objective all"),
            WebBuildV2Capability::EvaluateB2bCover,
            ExpectedRequest::EvaluateB2bCover,
        ),
        (
            supplied("cover-percent", "--objective unique"),
            WebBuildV2Capability::EvaluateCoverPercent,
            ExpectedRequest::EvaluateCoverPercent,
        ),
    ]
}

#[test]
fn every_canonical_build_v2_path_lowers_to_its_exact_app_request_variant() {
    for (command_text, expected_capability, expected_request) in canonical_commands() {
        let parsed = CliCommandParser::parse(&command_text)
            .unwrap_or_else(|error| panic!("parse {command_text}: {error:?}"));
        assert_eq!(
            parsed
                .build_v2_input()
                .expect("nominal Build v2 input")
                .capability(),
            expected_capability,
            "{command_text}",
        );
        let request = parsed
            .to_app_request()
            .unwrap_or_else(|error| panic!("lower {command_text}: {error:?}"));
        assert_eq!(request.query(), &QueryEnvelope::BuildCoverage);
        assert_eq!(request.backend_policy().backend_requested(), "cpu");
        assert!(!request.backend_policy().allow_backend_fallback());
        assert_eq!(request.resource_budget().memory_mib(), None);
        let AppCommand::BuildV2(command) = request.command() else {
            panic!("{command_text} did not lower to AppCommand::BuildV2");
        };
        assert_eq!(
            request_kind(command.request()),
            expected_request,
            "{command_text}"
        );
    }
}

#[test]
fn build_minimals_pin_is_bound_to_a_candidate_from_the_supplied_document() {
    let document = colored_target_document();
    let base = format!(
        "clearra build evaluate minimals --solution-format ctk3 \
         --solution-document {document} --queue I --no-hold"
    );
    let ordinary = CliCommandParser::parse(&base)
        .unwrap()
        .to_app_request()
        .unwrap();
    let AppCommand::BuildV2(command) = ordinary.command() else {
        panic!("expected Build v2 command");
    };
    let BuildV2AppRequest::BuildEvaluateMinimals(request) = command.request() else {
        panic!("expected supplied minimum request");
    };
    let pinned_key = &request.supplied().candidate_keys()[0];
    let pinned = CliCommandParser::parse(&format!("{base} --pin-candidate 1"))
        .unwrap()
        .to_app_request()
        .unwrap();
    let AppCommand::BuildV2(command) = pinned.command() else {
        panic!("expected pinned Build v2 command");
    };
    let BuildV2AppRequest::BuildEvaluateMinimals(request) = command.request() else {
        panic!("expected pinned supplied minimum request");
    };
    assert_eq!(request.pinned_candidate_keys(), [pinned_key.clone()]);
    let response = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    )
    .run(pinned);
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    assert!(response.public_result_payload().is_some());
    let duplicate = CliCommandParser::parse(&format!("{base} --pin-candidate 1 --pin-candidate 1"))
        .unwrap()
        .to_app_request()
        .unwrap();
    let AppCommand::BuildV2(command) = duplicate.command() else {
        panic!("expected duplicate-normalized Build v2 command");
    };
    let BuildV2AppRequest::BuildEvaluateMinimals(request) = command.request() else {
        panic!("expected duplicate-normalized supplied minimum request");
    };
    assert_eq!(request.pinned_candidate_keys(), [pinned_key.clone()]);
    assert!(CliCommandParser::parse(&format!("{base} --pin-candidate 999")).is_err());

    let document_pinned = CliCommandParser::parse(&format!(
        "{base} --pin-solution-format ctk3 --pin-solution-document {document}"
    ))
    .unwrap()
    .to_app_request()
    .unwrap();
    let AppCommand::BuildV2(command) = document_pinned.command() else {
        panic!("expected document-pinned Build v2 command");
    };
    let BuildV2AppRequest::BuildEvaluateMinimals(request) = command.request() else {
        panic!("expected document-pinned supplied minimum request");
    };
    assert_eq!(request.pinned_candidate_keys(), [pinned_key.clone()]);
    assert!(CliCommandParser::parse(&format!("{base} --pin-solution-format ctk3")).is_err());
    assert!(CliCommandParser::parse(&format!(
        "{base} --pin-candidate 1 --pin-solution-format ctk3 --pin-solution-document {document}"
    ))
    .is_err());
}

#[test]
fn build_pinned_minimals_replays_the_full_source_with_a_separate_required_document() {
    let document = colored_target_document();
    let base = "clearra build pinned-minimals --base-mask 0 --target-mask 15 \
                --height 4 --queue I --no-hold";
    assert!(CliCommandParser::parse(base).is_err());
    assert!(CliCommandParser::parse(&format!(
        "{base} --required-format ctk3 --required-document {document} --pin-candidate 1"
    ))
    .is_err());
    assert!(CliCommandParser::parse(&format!(
        "{base} --required-format ctk3 --required-document {document} \
         --expected-source-set-hash stale"
    ))
    .is_err());
    assert!(CliCommandParser::parse(&format!(
        "clearra build evaluate minimals --solution-format ctk3 \
         --solution-document {document} --queue I --no-hold \
         --required-format ctk3 --required-document {document}"
    ))
    .is_err());
    let stale = CliCommandParser::parse(&format!(
        "{base} --required-format ctk3 --required-document {document} \
         --expected-source-set-hash {}",
        "cts1:aaaaaaaaaaaaaaaa"
    ))
    .unwrap();
    let parsed = CliCommandParser::parse(&format!(
        "{base} --required-format ctk3 --required-document {document}"
    ))
    .unwrap();
    assert_eq!(
        parsed.build_v2_input().unwrap().capability(),
        WebBuildV2Capability::PinnedMinimals
    );
    let request = parsed.to_app_request().unwrap();
    let AppCommand::BuildV2(command) = request.command() else {
        panic!("expected Build v2 command");
    };
    let BuildV2AppRequest::BuildCover(cover) = command.request() else {
        panic!("pinned Build must run the full source producer");
    };
    assert_eq!(cover.pinned_colored_identities().len(), 1);
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    let response = context.run(request);
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    let product = response.product_capability_result().unwrap();
    assert_eq!(product.contract().as_str(), "build.pinned-minimals");
    assert_eq!(
        product.result_kind().as_str(),
        "build-pinned-minimum-cover.v1"
    );
    let public = product.public_result_payload().unwrap();
    assert_eq!(public.contract(), "build.pinned-minimals");
    assert_eq!(public.result_kind(), "build-pinned-minimum-cover.v1");
    let host = response.to_host_response();
    assert_eq!(
        host.product_result_payload().unwrap().result_kind(),
        "build-pinned-minimum-cover.v1"
    );
    let portfolio = response
        .product_capability_result()
        .and_then(|result| result.build_coverage_portfolio_v2())
        .expect("complete full-source Build portfolio");
    assert_eq!(portfolio.source_candidate_count(), 2);
    assert_eq!(portfolio.selected_candidate_count(), 1);
    assert_eq!(portfolio.pinned_candidate_keys().len(), 1);
    assert!(portfolio.completeness().complete());
    let ordinary = CliCommandParser::parse(&base.replace("pinned-minimals", "cover"))
        .unwrap()
        .to_app_request()
        .unwrap();
    let source_response = context.run(ordinary);
    assert_eq!(source_response.status(), AppStatus::Success);
    let source_hash = source_response
        .product_capability_result()
        .and_then(|result| result.build_coverage_portfolio_v2())
        .unwrap()
        .normalized_solution_set_hash();
    assert_eq!(portfolio.normalized_solution_set_hash(), source_hash);
    let bound_request = CliCommandParser::parse(&format!(
        "{base} --required-format ctk3 --required-document {document} \
         --expected-source-set-hash {source_hash}"
    ))
    .unwrap()
    .to_app_request()
    .unwrap();
    assert_eq!(context.run(bound_request).status(), AppStatus::Success);
    let stale_response = context.run(stale.to_app_request().unwrap());
    assert_eq!(stale_response.status(), AppStatus::ExecutionFailed);
}

#[test]
fn gui_minimum_solutions_finite_bag_is_the_only_supply_window_authority() {
    let request = CliCommandParser::parse(
        "clearra build cover --base-mask 0x0000000000000000 \
         --target-mask 0x000000000000000f --height 4 --hold empty \
         --patterns P2 --queue-knowledge oracle --objective min-cover \
         --rule srs-plus --backend cpu --no-backend-fallback --workers 1",
    )
    .expect("queue-less GUI minimum-solutions command")
    .to_app_request()
    .expect("typed Build cover AppRequest");

    let AppCommand::BuildV2(command) = request.command() else {
        panic!("GUI minimum-solutions did not lower to AppCommand::BuildV2");
    };
    let BuildV2AppRequest::BuildCover(cover) = command.request() else {
        panic!("GUI minimum-solutions did not lower to BuildCover");
    };
    assert_eq!(cover.objective(), BuildObjective::MinCover);
    assert_eq!(cover.query().aggregation().as_str(), "buildability");
    assert_eq!(cover.query().core_query().piece_window().max_pieces(), 1);
    assert_eq!(cover.query().core_query().exact_pieces(), Some(1));
    assert_eq!(
        cover.query().core_query().supply_window_size(),
        Some(SupplyWindowSize::new(2))
    );
    assert!(cover.query().solution_probability_policy().requested());
}

#[test]
fn build_v2_semantic_profiles_bind_to_the_actual_app_request_without_fallback() {
    let document = colored_target_document();
    let command_text = format!(
        "clearra build setup-cover-score --target-format ctk3 \
         --target-document {document} --queue I --no-hold \
         --objective max-score-cover --rule srs-x \
         --score-profile guideline"
    );
    let request = CliCommandParser::parse(&command_text)
        .expect("Build v2 profile command")
        .to_app_request()
        .expect("profile-bound Build v2 AppRequest");
    let profiles = request.request_profiles();
    assert_eq!(profiles.rule().as_str(), "srs-x");
    assert_eq!(profiles.spin().as_str(), "t-spins");
    assert_eq!(profiles.score().as_str(), "guideline");

    let unsupported = format!(
        "clearra build setup-cover-score --target-format ctk3 \
         --target-document {document} --queue I --no-hold \
         --objective max-score-cover --rule custom --score-profile tetrio"
    );
    let error = CliCommandParser::parse(&unsupported)
        .expect("custom remains syntactically recognized")
        .to_app_request()
        .expect_err("unverified Build v2 rule must fail closed at App authority");
    assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
    assert!(error.message().contains("unverified or unsupported"));
}

fn request_kind(request: &BuildV2AppRequest) -> ExpectedRequest {
    match request {
        BuildV2AppRequest::BuildCover(_) => ExpectedRequest::Cover,
        BuildV2AppRequest::BuildSetup(_) => ExpectedRequest::Setup,
        BuildV2AppRequest::BuildCongruent(_) => ExpectedRequest::Congruent,
        BuildV2AppRequest::BuildCongruentCover(_) => ExpectedRequest::CongruentCover,
        BuildV2AppRequest::BuildSetupCover(_) => ExpectedRequest::SetupCover,
        BuildV2AppRequest::BuildSetupCoverPercent(_) => ExpectedRequest::SetupCoverPercent,
        BuildV2AppRequest::BuildSetupCoverScore(_) => ExpectedRequest::SetupCoverScore,
        BuildV2AppRequest::BuildEvaluateCover(_) => ExpectedRequest::EvaluateCover,
        BuildV2AppRequest::BuildEvaluateMinimals(_) => ExpectedRequest::EvaluateMinimals,
        BuildV2AppRequest::BuildEvaluateScore(_) => ExpectedRequest::EvaluateScore,
        BuildV2AppRequest::BuildEvaluateB2bCover(_) => ExpectedRequest::EvaluateB2bCover,
        BuildV2AppRequest::BuildEvaluateCoverPercent(_) => ExpectedRequest::EvaluateCoverPercent,
    }
}

#[test]
fn build_v2_rejects_unowned_memory_authority_on_every_canonical_path() {
    for (command_text, _, _) in canonical_commands() {
        let error = CliCommandParser::parse(&format!("{command_text} --max-memory-mib 64"))
            .expect_err("Build v2 max-memory-mib must fail closed");
        assert_eq!(
            error.code(),
            CliCommandErrorCode::InvalidValue,
            "{command_text}"
        );
        assert!(error.message().contains("does not accept max-memory-mib"));
    }

    let input = WebBuildV2Input::cover([0; 4], [15, 0, 0, 0], 4, BuildObjective::MinCover)
        .expect("nominal programmatic Build v2 input");
    let error = CliCommandRequest::build_v2(input)
        .with_queue("I")
        .with_max_memory_mib(1)
        .to_app_request()
        .expect_err("programmatic max-memory authority must also fail closed");
    assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
}

#[test]
fn build_v2_parser_rejects_cross_capability_documents_and_option_conflicts() {
    let document = colored_target_document();
    let invalid = [
        format!(
            "clearra build setup --solution-format ctk3 --solution-document {document} --queue I"
        ),
        format!(
            "clearra build evaluate cover --target-format ctk3 --target-document {document} --queue I"
        ),
        format!(
            "clearra build setup --target-format ctk3 --target-document {document} --queue I --objective min-cover"
        ),
        format!(
            "clearra build evaluate minimals --solution-format ctk3 --solution-document {document} --queue I --objective unique"
        ),
        format!(
            "clearra build setup --target-format ctk3 --target-document {document} --queue I --score-profile tetrio"
        ),
        format!(
            "clearra build evaluate score --solution-format ctk3 --solution-document {document} --queue I --initial-b2b 65536"
        ),
        format!(
            "clearra build setup --target-format ctk3 --target-document {document} --queue I --patterns I"
        ),
        format!(
            "clearra build setup --target-format ctk3 --target-document {document}"
        ),
        format!(
            "clearra build setup --target-document {document} --queue I"
        ),
        "clearra build cover --base-mask 0 --target-mask 15 --height 4 --queue I --backend gpu"
            .to_owned(),
    ];
    for command_text in invalid {
        let error = CliCommandParser::parse(&command_text).expect_err(&command_text);
        assert!(
            matches!(
                error.code(),
                CliCommandErrorCode::InvalidValue
                    | CliCommandErrorCode::MissingValue
                    | CliCommandErrorCode::UnsupportedCommand
            ),
            "{command_text}: {error:?}",
        );
    }
}

#[test]
fn build_v2_accepts_only_cpu_even_when_the_redundant_backend_is_explicit() {
    let parsed = CliCommandParser::parse(
        "clearra build cover --base-mask 0 --target-mask 15 --height 4 --queue I --backend cpu",
    )
    .expect("explicit fixed CPU backend");
    let request = parsed.to_app_request().expect("CPU Build v2 request");
    assert_eq!(request.backend_policy().backend_requested(), "cpu");

    let input = WebBuildV2Input::cover([0; 4], [15, 0, 0, 0], 4, BuildObjective::MinCover).unwrap();
    let error = CliCommandRequest::build_v2(input)
        .with_queue("I")
        .with_backend(RequestedSearchBackend::Gpu)
        .to_app_request()
        .expect_err("programmatic GPU Build v2 request");
    assert_eq!(error.code(), CliCommandErrorCode::InvalidValue);
}
