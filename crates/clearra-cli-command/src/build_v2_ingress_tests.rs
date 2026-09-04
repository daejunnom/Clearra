use clearra_app::{
    encode_ctk3_compact, AppCommand, BuildObjective, BuildV2AppRequest, Ctk3Color, Ctk3Document,
    Ctk3Page, Ctk3Piece, QueryEnvelope,
};
use clearra_pc_graph::request::RequestedSearchBackend;

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
