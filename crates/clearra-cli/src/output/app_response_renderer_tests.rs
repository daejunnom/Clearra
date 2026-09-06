// SRP rationale: this test module has one behavior-level change reason: verifying stable rendering of typed application responses across supported CLI formats.

use clearra_app::{
    encode_ctk3_compact, AppContext, AppCoreExecutorService, AppError, AppResultKind, AppServices,
    AppStatus, Ctk3Color, Ctk3Document, Ctk3Page, Ctk3Piece, FinesseReport, FinesseReportInput,
    FinesseReportPlacement, FinesseRepresentativeWitness, ProductCapabilityContract,
    ProductCapabilityResultKind,
};
use clearra_cli_command::CliCommandParser;
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

use crate::{args::CliParser, assemble::CliAppRequestAssembler, output::RenderFormatSelector};

use super::*;

const DAMAGE_TWO_WORKERS: &str = concat!(
    "clearra damage --board-mask 0xffbfe --height 4 --queue IOTJ --no-hold ",
    "--spin-profile all-mini-plus --minimum-damage 1 --workers 2"
);
const SPIN_TWO_WORKERS: &str = concat!(
    "clearra spin-finder --board-mask 0xffbfe --height 4 --queue IOTJ --no-hold ",
    "--spin-profile t-spins-plus --lines any --workers 2"
);
const STRUCTURE_TWO_WORKERS: &str = concat!(
    "clearra spin-structure --board-mask 0x5000010 --height 4 --pieces T ",
    "--spin-profile t-spins --lines any --fill-top 4 --max-placements 1 --workers 2"
);
const STRUCTURE_WITH_COMPLETED_INPUT_ROW: &str = concat!(
    "clearra spin-structure --board-mask 0x14000043ff --height 4 --pieces T ",
    "--spin-profile t-spins --lines any --fill-top 4 --max-placements 1 --workers 2"
);

fn build_v2_render_document() -> String {
    let mut cells = vec![Ctk3Color::Empty; 40];
    cells[0..4].fill(Ctk3Color::Piece(Ctk3Piece::I));
    encode_ctk3_compact(&Ctk3Document::new(10, vec![Ctk3Page::new(4, cells)]))
        .expect("one-piece Build v2 CTK3 document")
}

fn build_v2_render_cases() -> Vec<(String, &'static str, &'static str, &'static str)> {
    let document = build_v2_render_document();
    let target = |path: &str, suffix: &str| {
        format!(
            "clearra build {path} --target-format ctk3 --target-document {document} --queue I --no-hold {suffix} --workers 2"
        )
    };
    let supplied = |path: &str, suffix: &str| {
        format!(
            "clearra build evaluate {path} --solution-format ctk3 --solution-document {document} --queue I --no-hold {suffix} --workers 2"
        )
    };
    vec![
        (
            "clearra build cover --base-mask 0 --target-mask 15 --height 4 --queue I --no-hold --objective min-cover --workers 2".to_owned(),
            "build.cover",
            "build-coverage-portfolio.v2",
            "portfolio",
        ),
        (
            target("setup", "--objective unique"),
            "build.setup",
            "build-target-family.v2",
            "candidate-family",
        ),
        (
            target("congruent", "--objective all"),
            "build.congruent",
            "build-congruence-family.v1",
            "candidate-family",
        ),
        (
            target("congruent-cover", "--objective min-cover"),
            "build.congruent-cover",
            "build-congruence-coverage.v1",
            "portfolio",
        ),
        (
            target("setup-cover", "--objective min-cover"),
            "build.setup-cover",
            "build-setup-cover.v1",
            "portfolio",
        ),
        (
            target("setup-cover-percent", "--objective unique"),
            "build.setup-cover-percent",
            "build-setup-cover-probability.v1",
            "probability",
        ),
        (
            target(
                "setup-cover-score",
                "--objective max-score-cover --score-profile guideline --initial-b2b 9",
            ),
            "build.setup-cover-score",
            "build-setup-cover-score.v1",
            "score-portfolio",
        ),
        (
            supplied("cover", "--objective all"),
            "build.evaluate.cover",
            "build-supplied-coverage.v1",
            "candidate-family",
        ),
        (
            supplied("minimals", "--objective min-cover"),
            "build.evaluate.minimals",
            "build-supplied-minimum-cover.v1",
            "portfolio",
        ),
        (
            supplied(
                "score",
                "--objective max-score-cover --score-profile tetrio --initial-b2b 0",
            ),
            "build.evaluate.score",
            "build-supplied-score.v1",
            "score-portfolio",
        ),
        (
            supplied("b2b-cover", "--objective all"),
            "build.evaluate.b2b-cover",
            "build-supplied-b2b-coverage.v1",
            "candidate-family",
        ),
        (
            supplied("cover-percent", "--objective unique"),
            "build.evaluate.cover-percent",
            "build-supplied-probability.v1",
            "probability",
        ),
    ]
}

#[test]
fn every_build_v2_app_response_renders_its_closed_public_cli_payload() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let app = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    for (source, capability, result_contract, payload_kind) in build_v2_render_cases() {
        let invocation = CliParser::parse(source.split_whitespace())
            .unwrap_or_else(|error| panic!("parse native CLI {source}: {error:?}"));
        let assembly =
            CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
                .unwrap_or_else(|output| panic!("assemble {source}: {}", output.stderr()));
        let request = assembly.request();
        assert_eq!(request.resource_budget().workers(), 2, "{source}");
        let response = app.run(request);
        assert_eq!(
            response.status(),
            AppStatus::Success,
            "{source}: {:?}",
            response.error()
        );
        assert_eq!(
            response.public_page_source_owner().is_some(),
            matches!(payload_kind, "portfolio" | "score-portfolio"),
            "{source}"
        );
        let output = AppResponseRenderer::render(
            response,
            RenderFormat::Json,
            CliErrorCode::ProductRuntimeUnsupported,
        );
        assert!(output.stderr().is_empty(), "{source}: {}", output.stderr());
        let value: serde_json::Value = serde_json::from_str(output.stdout())
            .unwrap_or_else(|error| panic!("render {source}: {error}"));
        assert_eq!(value["kind"], result_contract, "{source}");
        assert_eq!(
            value["contract"]["command"]["kind"], result_contract,
            "{source}"
        );
        assert_eq!(value["summary"]["capability_id"], capability, "{source}");
        assert_eq!(
            value["summary"]["result_contract"], result_contract,
            "{source}"
        );
        assert_eq!(value["summary"]["payload_kind"], payload_kind, "{source}");
        assert!(value["summary"].get("completeness").is_some(), "{source}");
        if payload_kind == "score-portfolio" {
            assert_eq!(
                value["summary"]["score_equality_basis"], "score-only",
                "{source}"
            );
            assert!(
                value["summary"]["informational_attack_basis"].is_string(),
                "{source}"
            );
        }
    }
}

#[test]
fn native_build_v2_text_output_exposes_only_the_public_product_summary() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let source = "clearra build cover --base-mask 0 --target-mask 15 --height 4 --queue I --no-hold --objective min-cover --workers 2";
    let invocation = CliParser::parse(source.split_whitespace()).expect("native Build v2 CLI");
    let request = CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Text)
        .expect("typed Build v2 app request")
        .request();
    let response = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    )
    .run(request);
    assert_eq!(
        response.status(),
        AppStatus::Success,
        "{:?}",
        response.error()
    );

    let output = AppResponseRenderer::render(
        response,
        RenderFormat::Text,
        CliErrorCode::ProductRuntimeUnsupported,
    );
    assert!(output.stderr().is_empty(), "{}", output.stderr());
    for marker in [
        "kind: minimum build solutions",
        "objective:",
        "source_candidate_count:",
        "selected_candidate_count:",
    ] {
        assert!(output.stdout().contains(marker), "missing {marker}");
    }
    for private_marker in [
        "build-coverage-portfolio.v2",
        "capability_id",
        "result_contract",
        "payload_kind",
        "page_source",
        "candidate_id",
    ] {
        assert!(
            !output.stdout().contains(private_marker),
            "private marker {private_marker}: {}",
            output.stdout()
        );
    }
}

#[test]
fn setup_score_executes_actual_coverage_and_score_and_renders_the_closed_payload() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let mut cells = vec![Ctk3Color::Empty; 20];
    cells[0..4].fill(Ctk3Color::Piece(Ctk3Piece::I));
    let document = encode_ctk3_compact(&Ctk3Document::new(10, vec![Ctk3Page::new(2, cells)]))
        .expect("Setup score CTK3 document");
    let source = format!(
        "clearra setup score --document-format ctk3 --document {document} --setup-queue I --solution-patterns P4 --clear 2 --no-hold --score-profile tetrio --initial-b2b 0 --rule srs-plus --max-patterns 840 --workers 1 --backend cpu --no-backend-fallback"
    );
    let invocation = CliParser::parse(source.split_whitespace())
        .unwrap_or_else(|error| panic!("parse Setup score CLI: {error:?}"));
    let assembly = CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
        .unwrap_or_else(|output| panic!("assemble Setup score: {}", output.stderr()));
    let response = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    )
    .run(assembly.request());
    assert_eq!(
        response.status(),
        AppStatus::Success,
        "Setup score diagnostics: {:?}; error: {:?}",
        response.diagnostics(),
        response.error()
    );
    let output = AppResponseRenderer::render(
        response,
        RenderFormat::Json,
        CliErrorCode::ProductRuntimeUnsupported,
    );
    assert!(output.stderr().is_empty(), "{}", output.stderr());
    let value: serde_json::Value =
        serde_json::from_str(output.stdout()).expect("Setup score CLI JSON");
    assert_eq!(value["kind"], "setup-score-ranking.v1");
    assert_eq!(value["summary"]["capability_id"], "setup.score");
    assert_eq!(value["summary"]["payload_kind"], "ranked-family");
    assert_eq!(value["summary"]["complete"], true);
    assert_eq!(value["summary"]["candidate_count"], "1");
    assert!(value["summary"].get("tie_cursor").is_none());
    assert!(value["summary"].get("informational_attack").is_none());
}

fn render_forward(command: &str, format: RenderFormat) -> String {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let request = CliCommandParser::parse_with_worker_limit(command, 8)
        .expect("forward CLI command")
        .to_app_request()
        .expect("typed app request");
    assert_eq!(request.resource_budget().workers(), 2);
    let response = AppContext::default().run(request);
    assert_eq!(response.status(), AppStatus::Success);

    AppResponseRenderer::render(response, format, CliErrorCode::ProductRuntimeUnsupported)
        .stdout()
        .to_owned()
}

fn failed_queue_public_fields() -> Vec<RenderField> {
    let mut fields = SummaryRenderContract::render_fields(vec![
        ("result_mode".to_owned(), "failed-queue".to_owned()),
        ("failed_pattern_count".to_owned(), "3".to_owned()),
        ("failed_pattern_limit".to_owned(), "9".to_owned()),
        (
            "failed_pattern_examples_materialized".to_owned(),
            "3".to_owned(),
        ),
        (
            "failed_pattern_examples_truncated".to_owned(),
            "false".to_owned(),
        ),
        ("failed_queue_probability".to_owned(), "0.25".to_owned()),
        ("failed_pattern_0".to_owned(), "IOT".to_owned()),
    ]);
    append_solution_data_contract(
        &mut fields,
        SolutionDataStatus::NotRequested,
        RenderFormat::Json,
    );
    fields
}

fn pc_probability_public_fields() -> Vec<RenderField> {
    let mut fields = SummaryRenderContract::render_fields(vec![
        ("covered_pattern_count".to_owned(), "1".to_owned()),
        ("total_pattern_count".to_owned(), "2".to_owned()),
        ("probability".to_owned(), "0.5".to_owned()),
        ("probability_complete".to_owned(), "true".to_owned()),
    ]);
    append_solution_data_contract(
        &mut fields,
        SolutionDataStatus::NotRequested,
        RenderFormat::Json,
    );
    fields
}

fn pc_score_public_fields() -> Vec<RenderField> {
    let mut fields = SummaryRenderContract::render_fields(vec![
        (
            "score_accuracy_level".to_owned(),
            "basic-approximation".to_owned(),
        ),
        (
            "score_accuracy_reason".to_owned(),
            "profile-specific basic score/attack tables with configurable spin detection"
                .to_owned(),
        ),
        (
            "score_profile_specific_exact".to_owned(),
            "false".to_owned(),
        ),
        ("score_evaluation_complete".to_owned(), "true".to_owned()),
        ("score_matrix_complete".to_owned(), "true".to_owned()),
        ("score_summary_complete".to_owned(), "true".to_owned()),
        ("probability_complete".to_owned(), "true".to_owned()),
        ("score_failed_pc_pattern_count".to_owned(), "1".to_owned()),
        ("score_failed_pc_pattern_score".to_owned(), "0".to_owned()),
        ("score_best_score".to_owned(), "800".to_owned()),
        ("score_best_attack".to_owned(), "2".to_owned()),
        (
            "score_unconditional_expected_score".to_owned(),
            "400.5".to_owned(),
        ),
        (
            "score_unconditional_expected_attack".to_owned(),
            "1.25".to_owned(),
        ),
        (
            "score_covered_pattern_conditional_average_score".to_owned(),
            "801".to_owned(),
        ),
    ]);
    append_solution_data_contract(
        &mut fields,
        SolutionDataStatus::NotRequested,
        RenderFormat::Json,
    );
    fields
}

#[test]
fn pc_minimum_cover_v2_json_kind_requires_the_closed_product_pair_and_pc_family() {
    for app_kind in [AppResultKind::Pc, AppResultKind::Scenario] {
        assert_eq!(
            public_success_render_kind(
                Some((
                    ProductCapabilityContract::PcMinimals,
                    ProductCapabilityResultKind::PcMinimumCoverV2,
                )),
                app_kind,
            ),
            "pc-minimum-cover.v2"
        );
    }

    for (identity, app_kind) in [
        (None, AppResultKind::Scenario),
        (
            Some((
                ProductCapabilityContract::PcMinimals,
                ProductCapabilityResultKind::PcProbabilityV2,
            )),
            AppResultKind::Scenario,
        ),
        (
            Some((
                ProductCapabilityContract::PcChance,
                ProductCapabilityResultKind::PcMinimumCoverV2,
            )),
            AppResultKind::Scenario,
        ),
        (
            Some((
                ProductCapabilityContract::PcMinimals,
                ProductCapabilityResultKind::PcMinimumCoverV2,
            )),
            AppResultKind::Percent,
        ),
    ] {
        assert_eq!(
            public_success_render_kind(identity, app_kind),
            app_kind.as_str()
        );
    }
}

#[test]
fn pc_score_portfolio_v2_json_kind_requires_the_closed_product_pair_and_pc_family() {
    for app_kind in [AppResultKind::Pc, AppResultKind::Scenario] {
        assert_eq!(
            public_success_render_kind(
                Some((
                    ProductCapabilityContract::PcScoreMinimals,
                    ProductCapabilityResultKind::PcScorePortfolioV2,
                )),
                app_kind,
            ),
            "pc-score-portfolio.v2"
        );
    }
    assert_eq!(
        public_success_render_kind(
            Some((
                ProductCapabilityContract::PcScoreMinimals,
                ProductCapabilityResultKind::PcScorePortfolioV2,
            )),
            AppResultKind::Percent,
        ),
        "percent"
    );
    assert_eq!(
        public_success_render_kind(
            Some((
                ProductCapabilityContract::PcScore,
                ProductCapabilityResultKind::PcScorePortfolioV2,
            )),
            AppResultKind::Scenario,
        ),
        "pc-scenario"
    );
}

#[test]
fn pc_save_result_kinds_require_their_closed_product_pairs_and_pc_family() {
    for (contract, result_kind, expected) in [
        (
            ProductCapabilityContract::PcSaves,
            ProductCapabilityResultKind::PcSaveGroupsV2,
            "pc-save-groups.v2",
        ),
        (
            ProductCapabilityContract::PcBestSave,
            ProductCapabilityResultKind::PcBestSaveV2,
            "pc-best-save.v2",
        ),
    ] {
        for app_kind in [AppResultKind::Pc, AppResultKind::Scenario] {
            assert_eq!(
                public_success_render_kind(Some((contract, result_kind)), app_kind),
                expected
            );
        }
        assert_eq!(
            public_success_render_kind(Some((contract, result_kind)), AppResultKind::Percent),
            "percent"
        );
    }

    assert_eq!(
        public_success_render_kind(
            Some((
                ProductCapabilityContract::PcSaves,
                ProductCapabilityResultKind::PcBestSaveV2,
            )),
            AppResultKind::Pc,
        ),
        "pc"
    );
    assert_eq!(
        public_success_render_kind(
            Some((
                ProductCapabilityContract::PcBestSave,
                ProductCapabilityResultKind::PcSaveGroupsV2,
            )),
            AppResultKind::Scenario,
        ),
        "pc-scenario"
    );
}

#[test]
fn pc_probability_v2_json_kind_requires_the_closed_product_pair_and_pc_family() {
    for app_kind in [AppResultKind::Pc, AppResultKind::Scenario] {
        assert_eq!(
            public_success_render_kind(
                Some((
                    ProductCapabilityContract::PcChance,
                    ProductCapabilityResultKind::PcProbabilityV2,
                )),
                app_kind,
            ),
            "pc-probability.v2"
        );
    }

    let rendered = CommandRenderer::render(
        public_success_render_kind(
            Some((
                ProductCapabilityContract::PcChance,
                ProductCapabilityResultKind::PcProbabilityV2,
            )),
            AppResultKind::Pc,
        ),
        pc_probability_public_fields(),
        RenderFormat::Json,
    )
    .expect("typed pc probability JSON");
    let value: serde_json::Value =
        serde_json::from_str(&rendered).expect("typed pc probability JSON contract");
    assert_eq!(value["kind"], "pc-probability.v2");
    assert_eq!(value["contract"]["command"]["kind"], "pc-probability.v2");
    assert_eq!(value["summary"]["covered_pattern_count"], 1);
    assert_eq!(value["summary"]["total_pattern_count"], 2);
    assert_eq!(value["summary"]["probability"], 0.5);
    assert_eq!(value["summary"]["probability_complete"], true);

    for (identity, app_kind) in [
        (None, AppResultKind::Pc),
        (
            Some((
                ProductCapabilityContract::PcChance,
                ProductCapabilityResultKind::PcFailedQueueV2,
            )),
            AppResultKind::Pc,
        ),
        (
            Some((
                ProductCapabilityContract::PcFailedQueue,
                ProductCapabilityResultKind::PcProbabilityV2,
            )),
            AppResultKind::Pc,
        ),
        (
            Some((
                ProductCapabilityContract::PcChance,
                ProductCapabilityResultKind::PcProbabilityV2,
            )),
            AppResultKind::Percent,
        ),
    ] {
        assert_eq!(
            public_success_render_kind(identity, app_kind),
            app_kind.as_str()
        );
    }
}

#[test]
fn pc_tiling_family_v1_json_kind_requires_the_closed_product_pair_and_pc_family() {
    for app_kind in [AppResultKind::Pc, AppResultKind::Scenario] {
        assert_eq!(
            public_success_render_kind(
                Some((
                    ProductCapabilityContract::PcTiling,
                    ProductCapabilityResultKind::PcTilingFamilyV1,
                )),
                app_kind,
            ),
            "pc-tiling-family.v1"
        );
    }

    for identity in [
        None,
        Some((
            ProductCapabilityContract::PcTiling,
            ProductCapabilityResultKind::PcProbabilityV2,
        )),
        Some((
            ProductCapabilityContract::PcChance,
            ProductCapabilityResultKind::PcTilingFamilyV1,
        )),
    ] {
        assert_eq!(
            public_success_render_kind(identity, AppResultKind::Pc),
            "pc"
        );
    }
}

#[test]
fn pc_score_summary_v2_json_kind_requires_the_closed_product_pair_and_pc_family() {
    for app_kind in [AppResultKind::Pc, AppResultKind::Scenario] {
        assert_eq!(
            public_success_render_kind(
                Some((
                    ProductCapabilityContract::PcScore,
                    ProductCapabilityResultKind::PcScoreSummaryV2,
                )),
                app_kind,
            ),
            "pc-score-summary.v2"
        );
    }

    let rendered = CommandRenderer::render(
        public_success_render_kind(
            Some((
                ProductCapabilityContract::PcScore,
                ProductCapabilityResultKind::PcScoreSummaryV2,
            )),
            AppResultKind::Pc,
        ),
        pc_score_public_fields(),
        RenderFormat::Json,
    )
    .expect("typed pc score JSON");
    let value: serde_json::Value =
        serde_json::from_str(&rendered).expect("typed pc score JSON contract");
    assert_eq!(value["kind"], "pc-score-summary.v2");
    assert_eq!(value["contract"]["command"]["kind"], "pc-score-summary.v2");
    assert_eq!(
        value["summary"]["score_accuracy_level"],
        "basic-approximation"
    );
    assert_eq!(value["summary"]["score_profile_specific_exact"], false);
    assert_eq!(
        value["summary"]["score_accuracy_reason"],
        "profile-specific basic score/attack tables with configurable spin detection"
    );
    for numeric in [
        "score_failed_pc_pattern_count",
        "score_failed_pc_pattern_score",
        "score_best_score",
        "score_best_attack",
        "score_unconditional_expected_score",
        "score_unconditional_expected_attack",
        "score_covered_pattern_conditional_average_score",
    ] {
        assert!(value["summary"][numeric].is_number(), "{numeric}");
    }
    for completeness in [
        "score_evaluation_complete",
        "score_matrix_complete",
        "score_summary_complete",
        "probability_complete",
    ] {
        assert_eq!(value["summary"][completeness], true, "{completeness}");
    }
    for private_key in [
        "origin",
        "query",
        "problem_owner",
        "execution_authority",
        "memory_evidence",
        "pc_score_problem_evidence",
        "exact_scoring_execution_batches",
        "postprocess_score_cells",
    ] {
        assert!(value["summary"].get(private_key).is_none(), "{private_key}");
    }

    for (identity, app_kind) in [
        (None, AppResultKind::Pc),
        (
            Some((
                ProductCapabilityContract::PcScore,
                ProductCapabilityResultKind::PcProbabilityV2,
            )),
            AppResultKind::Pc,
        ),
        (
            Some((
                ProductCapabilityContract::PcChance,
                ProductCapabilityResultKind::PcScoreSummaryV2,
            )),
            AppResultKind::Pc,
        ),
        (
            Some((
                ProductCapabilityContract::PcScore,
                ProductCapabilityResultKind::PcScoreSummaryV2,
            )),
            AppResultKind::Percent,
        ),
    ] {
        assert_eq!(
            public_success_render_kind(identity, app_kind),
            app_kind.as_str()
        );
    }
}

#[test]
fn pc_fixed_score_witness_v2_json_kind_requires_the_closed_product_pair() {
    for app_kind in [AppResultKind::Pc, AppResultKind::Scenario] {
        assert_eq!(
            public_success_render_kind(
                Some((
                    ProductCapabilityContract::PcScoreFinder,
                    ProductCapabilityResultKind::PcFixedScoreWitnessV2,
                )),
                app_kind,
            ),
            "pc-fixed-score-witness.v2"
        );
    }
    assert_eq!(
        public_success_render_kind(
            Some((
                ProductCapabilityContract::PcScoreFinder,
                ProductCapabilityResultKind::PcScoreSummaryV2,
            )),
            AppResultKind::Scenario,
        ),
        "pc-scenario"
    );
}

#[test]
fn score_finder_renderer_rejects_a_noncanonical_supplied_witness() {
    use clearra_host_contract::{ScorePatternWinnerFamilyPayload, ScorePatternWinnerPayload};

    let candidate_ten = ScorePatternWinnerPayload::new("0", "10", "solution-10", "1200", "0");
    let candidate_two = ScorePatternWinnerPayload::new("1", "2", "solution-2", "1200", "999");
    let family = |canonical| {
        ScorePatternWinnerFamilyPayload::new(
            "pc-score-pattern-winner.v1",
            "pattern-id-ascending-then-candidate-id-ascending",
            "score-only-attack-informational",
            "canonical-equal-score-trace",
            "100",
            "2",
            "smallest-canonical-candidate-id",
            canonical,
            vec![candidate_ten.clone(), candidate_two.clone()],
        )
    };

    assert!(valid_score_pattern_canonical_witness(&family(
        candidate_two.clone()
    )));
    assert!(!valid_score_pattern_canonical_witness(&family(
        candidate_ten.clone()
    )));
}

#[test]
fn pc_failed_queue_v2_json_kind_requires_the_closed_product_pair() {
    let typed_kind = public_success_render_kind(
        Some((
            ProductCapabilityContract::PcFailedQueue,
            ProductCapabilityResultKind::PcFailedQueueV2,
        )),
        AppResultKind::Percent,
    );
    let rendered =
        CommandRenderer::render(typed_kind, failed_queue_public_fields(), RenderFormat::Json)
            .expect("typed pc failed-queue JSON");
    let value: serde_json::Value =
        serde_json::from_str(&rendered).expect("typed pc failed-queue JSON contract");

    assert_eq!(value["schema_version"], 2);
    assert_eq!(value["kind"], "pc-failed-queue.v2");
    assert!(value["summary"].is_object());
    assert_eq!(value["contract"]["command"]["kind"], "pc-failed-queue.v2");
    assert_eq!(value["contract"]["solution_data"]["requested"], false);
    assert_eq!(
        value["contract"]["solution_data"]["status"],
        "not-requested"
    );
    assert!(value["contract"].get("supply").is_none());
    assert!(value["runtime_identity"].is_object());
    assert!(value["summary"]["failed_pattern_count"].is_number());
    assert!(value["summary"]["failed_pattern_limit"].is_number());
    assert!(value["summary"]["failed_queue_probability"].is_number());
    assert_eq!(value["summary"]["failed_pattern_examples_truncated"], false);
    assert_eq!(value["summary"]["failed_pattern_0"], "IOT");
    for private_key in [
        "origin",
        "query",
        "problem_owner",
        "execution_authority",
        "memory_evidence",
    ] {
        assert!(value["summary"].get(private_key).is_none(), "{private_key}");
    }

    for (identity, app_kind) in [
        (None, AppResultKind::Percent),
        (
            Some((
                ProductCapabilityContract::PcFailedQueue,
                ProductCapabilityResultKind::PcProbabilityV2,
            )),
            AppResultKind::Percent,
        ),
        (
            Some((
                ProductCapabilityContract::PcChance,
                ProductCapabilityResultKind::PcFailedQueueV2,
            )),
            AppResultKind::Percent,
        ),
        (
            Some((
                ProductCapabilityContract::PcFailedQueue,
                ProductCapabilityResultKind::PcFailedQueueV2,
            )),
            AppResultKind::Scenario,
        ),
    ] {
        assert_eq!(
            public_success_render_kind(identity, app_kind),
            app_kind.as_str()
        );
    }

    let legacy = CommandRenderer::render(
        public_success_render_kind(None, AppResultKind::Percent),
        failed_queue_public_fields(),
        RenderFormat::Json,
    )
    .expect("legacy failed-queue JSON");
    let legacy: serde_json::Value =
        serde_json::from_str(&legacy).expect("legacy failed-queue JSON contract");
    assert_eq!(legacy["kind"], "percent");
    assert_eq!(legacy["contract"]["command"]["kind"], "percent");
    assert!(legacy["contract"]["supply"].is_object());
}

#[test]
fn canonical_pc_minimals_cli_json_uses_the_typed_v2_result_kind() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let source = concat!(
        "clearra --format json pc minimals --lines 1 --board-mask 0x3f ",
        "--height 1 --pieces 1 --queue I --hold empty --rule srs-plus"
    );
    let invocation = CliParser::parse(source.split_whitespace())
        .expect("canonical pc minimals CLI parser route");
    let assembly = CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
        .unwrap_or_else(|output| panic!("assemble {source}: {}", output.stderr()));
    let render_format = assembly.render_format();
    let default_error = assembly.default_error();
    let response = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    )
    .run(assembly.request());
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    assert_eq!(
        response
            .product_capability_result()
            .expect("typed pc minimals result wrapper")
            .result_kind(),
        ProductCapabilityResultKind::PcMinimumCoverV2
    );

    let output = AppResponseRenderer::render(response, render_format, default_error);
    assert!(output.stderr().is_empty(), "{}", output.stderr());
    let value: serde_json::Value =
        serde_json::from_str(output.stdout()).expect("typed pc minimals CLI JSON");
    assert_eq!(value["kind"], "pc-minimum-cover.v2");
    assert_eq!(value["contract"]["command"]["kind"], "pc-minimum-cover.v2");
    assert!(value.get("portfolio_alternative_page").is_none());
    assert!(value.get("tie_cursor").is_none());
}

#[test]
#[cfg_attr(
    not(feature = "native-c-core"),
    ignore = "requires the native clearra_core static library"
)]
fn canonical_pc_failed_queue_cli_json_uses_v2_while_legacy_stays_percent() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    for (source, expected_kind) in [
        (
            "clearra pc failed-queue --lines 2 --patterns P5 --backend cpu --failed-count 4",
            "pc-failed-queue.v2",
        ),
        (
            "clearra failed-queue --lines 2 --patterns P5 --backend cpu --failed-count 4",
            "percent",
        ),
    ] {
        let request = CliCommandParser::parse_with_worker_limit(source, 2)
            .unwrap_or_else(|_| panic!("parse {source}"))
            .to_app_request()
            .unwrap_or_else(|_| panic!("assemble {source}"));
        let response = AppContext::default().run(request);
        assert_eq!(response.status(), AppStatus::Success, "{source}");
        let output = AppResponseRenderer::render(
            response,
            RenderFormat::Json,
            CliErrorCode::ProductRuntimeUnsupported,
        );
        assert!(output.stderr().is_empty(), "{source}: {}", output.stderr());
        let value: serde_json::Value = serde_json::from_str(output.stdout())
            .unwrap_or_else(|_| panic!("rendered JSON for {source}"));
        assert_eq!(value["kind"], expected_kind, "{source}");
        assert_eq!(
            value["contract"]["command"]["kind"], expected_kind,
            "{source}"
        );
    }
}

#[test]
#[cfg_attr(
    not(feature = "native-c-core"),
    ignore = "requires the native clearra_core static library"
)]
fn canonical_pc_chance_cli_json_uses_v2_while_legacy_routes_stay_generic() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    for (source, expected_kind) in [
        (
            "clearra --format json pc chance --lines 2 --queue IOTJL --backend cpu",
            "pc-probability.v2",
        ),
        (
            "clearra --format json chance v115@vhAAgH --queue IOTJL --lines 2",
            "pc-scenario",
        ),
        (
            "clearra --format json percent --queue IOTJL --fixed --min-len 5",
            "percent",
        ),
    ] {
        let invocation = CliParser::parse(source.split_whitespace())
            .unwrap_or_else(|_| panic!("parse {source}"));
        let assembly =
            CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
                .unwrap_or_else(|output| panic!("assemble {source}: {}", output.stderr()));
        let render_format = assembly.render_format();
        let default_error = assembly.default_error();
        let response = AppContext::default().run(assembly.request());
        assert_eq!(response.status(), AppStatus::Success, "{source}");
        let output = AppResponseRenderer::render(response, render_format, default_error);
        assert!(output.stderr().is_empty(), "{source}: {}", output.stderr());
        let value: serde_json::Value = serde_json::from_str(output.stdout())
            .unwrap_or_else(|_| panic!("rendered JSON for {source}"));
        assert_eq!(value["kind"], expected_kind, "{source}");
        assert_eq!(
            value["contract"]["command"]["kind"], expected_kind,
            "{source}"
        );
    }
}

#[test]
fn execution_failed_json_preserves_the_typed_resource_report() {
    let availability = clearra_host_contract::ExecutionAvailabilityReport::exhausted(
        clearra_host_contract::ExecutionSurface::Native,
        clearra_host_contract::ExecutionAvailabilityReason::MemoryBudgetExceeded,
    )
    .with_pattern_evidence(1_058_400, 1_058_400, 132_304)
    .with_required_memory_bytes(17_066_704);
    let mut resource_report = clearra_host_contract::ResourceReport::new(false, "not-executed")
        .with_execution_availability(availability);
    resource_report
        .set_result_completeness(clearra_host_contract::ExecutionCompletenessState::NotExecuted);
    let response = AppResponse::failed(
        AppStatus::ExecutionFailed,
        AppError::new(
            AppErrorCode::ExecutionFailed,
            "shared memory budget exhausted",
        ),
    )
    .with_resource_report(resource_report);

    let output = AppResponseRenderer::render(
        response,
        RenderFormat::Json,
        CliErrorCode::ProductRuntimeUnsupported,
    );
    assert!(output.stderr().is_empty());
    assert_ne!(output.exit_code(), crate::exit::ExitCode::Success);
    let value: serde_json::Value =
        serde_json::from_str(output.stdout()).expect("typed failure JSON");

    assert_eq!(value["kind"], "execution-failed");
    assert_eq!(value["error"]["message"], "shared memory budget exhausted");
    assert!(value["error"]["code"].as_str().is_some());
    assert_eq!(value["resource_report"]["solver_executed"], false);
    assert_eq!(
        value["resource_report"]["execution_availability"]["state"],
        "exhausted"
    );
    assert_eq!(
        value["resource_report"]["execution_availability"]["reason"],
        "memory-budget-exceeded"
    );
    assert_eq!(
        value["resource_report"]["execution_availability"]["descriptor_pattern_count"],
        "1058400"
    );
    assert_eq!(
        value["resource_report"]["execution_availability"]["dense_pattern_count"],
        "1058400"
    );
    assert_eq!(
        value["resource_report"]["execution_availability"]["required_dense_bytes"],
        "132304"
    );
    assert_eq!(
        value["resource_report"]["execution_availability"]["required_memory_bytes"],
        "17066704"
    );
    assert_eq!(
        value["resource_report"]["result_completeness"],
        "not-executed"
    );
}

#[test]
fn execution_failed_default_text_is_public_while_explicit_text_profiles_keep_authority() {
    let availability = clearra_host_contract::ExecutionAvailabilityReport::exhausted(
        clearra_host_contract::ExecutionSurface::Native,
        clearra_host_contract::ExecutionAvailabilityReason::MemoryBudgetExceeded,
    )
    .with_pattern_evidence(1_058_400, 1_058_400, 132_304)
    .with_required_memory_bytes(17_066_704);
    let mut report = clearra_host_contract::ResourceReport::new(false, "not-executed")
        .with_execution_availability(availability);
    report.set_result_completeness(clearra_host_contract::ExecutionCompletenessState::NotExecuted);

    let expected = concat!(
        "shared memory budget exhausted\n",
        "resource_report.solver_executed: false\n",
        "resource_report.execution_availability.state: exhausted\n",
        "resource_report.execution_availability.reason: memory-budget-exceeded\n",
        "resource_report.execution_availability.surface: native\n",
        "resource_report.execution_availability.descriptor_pattern_count: 1058400\n",
        "resource_report.execution_availability.dense_pattern_count: 1058400\n",
        "resource_report.execution_availability.required_dense_bytes: 132304\n",
        "resource_report.execution_availability.required_memory_bytes: 17066704\n",
        "resource_report.result_completeness: not-executed",
    );
    let default_text = RenderFormatSelector::parse(None).expect("omitted format defaults to text");
    assert_eq!(default_text, RenderFormat::Text);
    let default_response = AppResponse::failed(
        AppStatus::ExecutionFailed,
        AppError::new(
            AppErrorCode::ExecutionFailed,
            "shared memory budget exhausted",
        ),
    )
    .with_resource_report(report.clone());
    let default_output = AppResponseRenderer::render(
        default_response,
        default_text,
        CliErrorCode::ProductRuntimeUnsupported,
    );
    assert_eq!(
        default_output.stderr(),
        "error E_PRODUCT_RUNTIME_UNSUPPORTED the operation could not be completed"
    );
    assert!(!default_output.stderr().contains("resource_report."));
    assert!(!default_output
        .stderr()
        .contains("shared memory budget exhausted"));

    for format in [RenderFormat::TextVerbose, RenderFormat::TextDiagnostics] {
        let response = AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(
                AppErrorCode::ExecutionFailed,
                "shared memory budget exhausted",
            ),
        )
        .with_resource_report(report.clone());
        let output =
            AppResponseRenderer::render(response, format, CliErrorCode::ProductRuntimeUnsupported);
        assert!(output.stdout().is_empty());
        assert_eq!(output.stderr(), expected);
    }
}

#[test]
fn execution_failed_text_does_not_fabricate_an_absent_resource_report() {
    let response = AppResponse::failed(
        AppStatus::ExecutionFailed,
        AppError::new(AppErrorCode::ExecutionFailed, "legacy failure"),
    );
    let output = AppResponseRenderer::render(
        response,
        RenderFormat::Text,
        CliErrorCode::ProductRuntimeUnsupported,
    );

    assert_eq!(
        output.stderr(),
        "error E_PRODUCT_RUNTIME_UNSUPPORTED the operation could not be completed"
    );
    assert!(!output.stderr().contains("resource_report."));
    assert!(!output.stderr().contains("legacy failure"));
}

#[test]
fn app_failure_format_boundary_keeps_private_identity_out_of_default_text() {
    const PRIVATE_SENTINEL: &str =
        "problem_id=private candidate_id=81 trace_identity=private-trace";
    for (status, app_error, default_error, public_message) in [
        (
            AppStatus::Unsupported,
            AppErrorCode::Unsupported,
            CliErrorCode::ProductRuntimeUnsupported,
            "the requested operation is not supported",
        ),
        (
            AppStatus::ExecutionFailed,
            AppErrorCode::ExecutionFailed,
            CliErrorCode::PcSearchInternal,
            "the operation could not be completed",
        ),
    ] {
        for format in [RenderFormat::Text, RenderFormat::FumenLike] {
            let output = AppResponseRenderer::render(
                AppResponse::failed(status, AppError::new(app_error, PRIVATE_SENTINEL)),
                format,
                default_error,
            );
            assert!(output.stdout().is_empty(), "{status:?}:{format:?}");
            assert_eq!(
                output.stderr(),
                format!("error {} {public_message}", default_error.as_str()),
                "{status:?}:{format:?}"
            );
            assert!(!output.stderr().contains(PRIVATE_SENTINEL));
        }

        for format in [RenderFormat::TextVerbose, RenderFormat::TextDiagnostics] {
            let output = AppResponseRenderer::render(
                AppResponse::failed(status, AppError::new(app_error, PRIVATE_SENTINEL)),
                format,
                default_error,
            );
            assert!(output.stdout().is_empty(), "{status:?}:{format:?}");
            assert!(
                output.stderr().contains(PRIVATE_SENTINEL),
                "{status:?}:{format:?}"
            );
        }

        let json = AppResponseRenderer::render(
            AppResponse::failed(status, AppError::new(app_error, PRIVATE_SENTINEL)),
            RenderFormat::Json,
            default_error,
        );
        let developer_output = format!("{}\n{}", json.stdout(), json.stderr());
        assert!(
            developer_output.contains(PRIVATE_SENTINEL),
            "{status:?}:json"
        );
    }
}

#[test]
fn finesse_renderer_preserves_the_typed_representative_witness() {
    let report = FinesseReport::new("search", "oracle", true, Some("3".to_owned()), vec![])
        .with_representative_witness(FinesseRepresentativeWitness::new(
            "oracle",
            Some("solution-a".to_owned()),
            vec![4],
            vec![PieceKind::T],
            3,
            vec![
                FinesseReportInput::TapLeft,
                FinesseReportInput::RotateClockwise,
                FinesseReportInput::HardDrop,
            ],
            vec![FinesseReportPlacement::new(
                PieceKind::T,
                RotationState::Right,
                2,
                0,
            )],
        ));
    let RenderFieldValue::Object(fields) = finesse_report_value(&report) else {
        panic!("finesse report object");
    };
    assert_eq!(
        fields
            .iter()
            .find(|field| field.key() == "exact_total_inputs")
            .map(|field| field.value()),
        Some(&RenderFieldValue::string("3"))
    );
    let witness = fields
        .iter()
        .find(|field| field.key() == "representative_witness")
        .expect("representative witness field");
    assert_eq!(
        witness.value(),
        &RenderFieldValue::object([
            ("policy", RenderFieldValue::string("oracle")),
            ("solution_key", RenderFieldValue::string("solution-a")),
            (
                "pattern_ids",
                RenderFieldValue::array([RenderFieldValue::from(4_usize)]),
            ),
            (
                "queue",
                RenderFieldValue::array([RenderFieldValue::string("T")]),
            ),
            ("total_inputs", RenderFieldValue::from(3_u32)),
            (
                "input_sequence",
                RenderFieldValue::array([
                    RenderFieldValue::string("tap-left"),
                    RenderFieldValue::string("rotate-clockwise"),
                    RenderFieldValue::string("hard-drop"),
                ]),
            ),
            (
                "placements",
                RenderFieldValue::array([RenderFieldValue::object([
                    ("piece", RenderFieldValue::string("T")),
                    ("rotation", RenderFieldValue::from(1_u8)),
                    ("x", RenderFieldValue::from(2_i16)),
                    ("y", RenderFieldValue::from(0_i16)),
                ])]),
            ),
        ])
    );
}

#[test]
fn fixed_queue_finesse_score_cli_json_preserves_the_typed_public_contract() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let request = CliCommandParser::parse_with_worker_limit(
        "clearra finesse score --initial-mask 0 --height 4 \
         --placements O:spawn:4:0 --queue O --no-hold --pattern-knowledge both \
         --rule srs-plus --workers 2",
        4,
    )
    .expect("finesse score CLI command")
    .to_app_request()
    .expect("typed score request");
    let response = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    )
    .run(request);
    assert_eq!(response.status(), AppStatus::Success);

    let rendered = AppResponseRenderer::render_with_solution_data(
        response,
        RenderFormat::Json,
        CliErrorCode::ProductRuntimeUnsupported,
        true,
    )
    .stdout()
    .to_owned();
    let value: serde_json::Value = serde_json::from_str(&rendered).expect("score CLI JSON");

    assert_eq!(value["contract"]["solution_data"]["status"], "complete");
    assert_eq!(value["finesse_report"]["mode"], "score");
    assert_eq!(value["finesse_report"]["exact_total_inputs"], "1");
    assert_eq!(
        value["contract"]["artifacts"]["finesse_report"],
        value["finesse_report"]
    );
    assert_eq!(
        value["contract"]["artifacts"]["finesse_score"]["representative_path"][0]["piece"],
        "O"
    );
    assert!(
        !rendered.contains("wasm-cpu-finesse-score"),
        "the browser adapter fallback must not leak into CLI output"
    );
}

#[test]
fn coverage_summary_solution_data_request_is_explicitly_unavailable_without_artifacts() {
    let status = SolutionDataStatus::for_request(true, false, false);
    assert!(!status.exposes_artifacts());
    let mut fields = SummaryRenderContract::render_fields(vec![
        (
            "unique_solution_count".to_owned(),
            "not-calculated".to_owned(),
        ),
        ("solution_count_calculated".to_owned(), "false".to_owned()),
    ]);
    append_solution_data_contract(&mut fields, status, RenderFormat::Json);
    let rendered = CommandRenderer::render("percent", fields, RenderFormat::Json).expect("JSON");
    let value: serde_json::Value = serde_json::from_str(&rendered).expect("percent CLI JSON");

    assert_eq!(value["summary"]["unique_solution_count"], "not-calculated");
    assert_eq!(value["summary"]["solution_count_calculated"], false);
    assert_eq!(value["contract"]["solution_data"]["requested"], true);
    assert_eq!(value["contract"]["solution_data"]["status"], "unavailable");
    assert_eq!(
        value["contract"]["solution_data"]["reason"],
        "solution-set-not-materialized"
    );
    assert!(value["contract"].get("artifacts").is_none());
}

#[test]
fn coverage_summary_text_preserves_not_calculated_without_json_contract_metadata() {
    let mut fields = SummaryRenderContract::render_fields(vec![(
        "unique_solution_count".to_owned(),
        "not-calculated".to_owned(),
    )]);
    append_solution_data_contract(
        &mut fields,
        SolutionDataStatus::for_request(true, false, false),
        RenderFormat::TextVerbose,
    );
    let rendered =
        CommandRenderer::render("percent", fields, RenderFormat::TextVerbose).expect("text");

    assert!(rendered.contains("unique_solution_count: not-calculated"));
    assert!(!rendered.contains("unique_solution_count: 0"));
    assert!(!rendered.contains("solution_data_status"));
    assert!(!rendered.contains("solution_data_requested"));
}

#[test]
fn damage_and_spin_json_report_the_two_requested_workers() {
    for command in [DAMAGE_TWO_WORKERS, SPIN_TWO_WORKERS] {
        let rendered = render_forward(command, RenderFormat::Json);

        assert!(rendered.contains("\"workers_used\":2"), "{rendered}");
        assert!(
            rendered.contains("\"evidence_path_count\":\""),
            "{rendered}"
        );
        assert!(
            rendered.contains("\"evidence_complete\":true"),
            "{rendered}"
        );
    }
}

#[test]
fn damage_and_spin_text_profiles_report_the_two_requested_workers() {
    for command in [DAMAGE_TWO_WORKERS, SPIN_TWO_WORKERS] {
        for format in [RenderFormat::Text, RenderFormat::TextVerbose] {
            let rendered = render_forward(command, format);

            assert!(rendered.contains("workers_used: 2"), "{rendered}");
        }
    }
}

#[test]
fn spin_structure_json_exposes_the_closed_public_family_payload() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let request = CliCommandParser::parse_with_worker_limit(STRUCTURE_TWO_WORKERS, 8)
        .expect("structure CLI command")
        .to_app_request()
        .expect("typed app request");
    let response = AppContext::default().run(request);
    assert_eq!(response.status(), AppStatus::Success);

    let family_rendered = AppResponseRenderer::render(
        response.clone(),
        RenderFormat::Json,
        CliErrorCode::ProductRuntimeUnsupported,
    )
    .stdout()
    .to_owned();
    let family_value: serde_json::Value =
        serde_json::from_str(&family_rendered).expect("structure family JSON");
    assert_eq!(family_value["kind"], "spin-structure-family.v2");
    assert_eq!(
        family_value["summary"]["capability_id"],
        "spin-structure.search"
    );
    assert_eq!(
        family_value["summary"]["result_contract"],
        "spin-structure-family.v2"
    );
    assert_eq!(
        family_value["summary"]["payload_kind"],
        "spin-structure-family"
    );
    assert_eq!(family_value["summary"]["complete"], true);
    let candidate_ids = family_value["summary"]["candidates"]
        .as_array()
        .expect("closed structure candidates")
        .iter()
        .map(|candidate| {
            candidate["candidate_id"]
                .as_str()
                .expect("canonical structure candidate ID")
        })
        .collect::<Vec<_>>();
    assert!(!candidate_ids.is_empty());
    assert!(candidate_ids
        .iter()
        .all(|candidate_id| candidate_id.starts_with("spin-structure-candidate.v1:")));
    let unique_candidate_ids = candidate_ids
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unique_candidate_ids.len(), candidate_ids.len());

    let rendered = AppResponseRenderer::render_with_solution_data(
        response,
        RenderFormat::Json,
        CliErrorCode::ProductRuntimeUnsupported,
        true,
    )
    .stdout()
    .to_owned();
    let value: serde_json::Value = serde_json::from_str(&rendered).expect("structure JSON");

    assert_eq!(value["kind"], "spin-structure-family.v2");
    assert_eq!(value["summary"]["capability_id"], "spin-structure.search");
    assert!(value["summary"]["candidate_count"]
        .as_str()
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|count| count > 0));
    assert!(value["summary"].get("regular").is_none());
    assert!(value["summary"].get("mini").is_none());
    assert!(value["contract"].get("artifacts").is_none());
}

#[test]
fn spin_structure_ctk3_keys_start_from_the_line_cleared_input_board() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let request = CliCommandParser::parse_with_worker_limit(STRUCTURE_WITH_COMPLETED_INPUT_ROW, 8)
        .expect("structure CLI command")
        .to_app_request()
        .expect("typed app request");
    let response = AppContext::default().run(request);
    assert_eq!(response.status(), AppStatus::Success);
    let report = response
        .render_model()
        .and_then(AppRenderModel::spin_structure_result)
        .expect("spin-structure render model");
    assert_eq!(
        report
            .query
            .as_ref()
            .expect("normalized query")
            .initial_board
            .words(),
        [0x5000010, 0, 0, 0]
    );
    assert!(response.public_result_payload().is_some());
}

#[test]
fn every_spin_structure_product_route_executes_and_renders_its_closed_public_payload() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let cases = [
        (
            "search",
            "spin-structure-family.v2",
            "spin-structure.search",
            "spin-structure-family",
        ),
        (
            "cover --objective min-cover --max-patterns 8",
            "spin-structure-coverage.v1",
            "spin-structure.cover",
            "coverage-portfolio",
        ),
        (
            "guaranteed --final-piece T --max-patterns 8 --no-dependency-report",
            "spin-structure-guaranteed.v1",
            "spin-structure.guaranteed",
            "spin-structure-family",
        ),
    ];
    for (route, result_contract, capability_id, payload_kind) in cases {
        let command = format!(
            "clearra spin-structure {route} --board-mask 0x14000043ff --height 4 --pieces T --spin-profile t-spins --lines any --fill-top 4 --max-placements 1 --workers 2"
        );
        let request = CliCommandParser::parse_with_worker_limit(&command, 8)
            .unwrap_or_else(|error| panic!("parse {command}: {error:?}"))
            .to_app_request()
            .unwrap_or_else(|error| panic!("lower {command}: {error:?}"));
        let response = AppContext::default().run(request);
        assert_eq!(response.status(), AppStatus::Success, "{command}");
        assert_eq!(
            response.public_page_source_owner().is_some(),
            capability_id == "spin-structure.cover",
            "{command}"
        );
        let output = AppResponseRenderer::render(
            response,
            RenderFormat::Json,
            CliErrorCode::ProductRuntimeUnsupported,
        );
        assert!(output.stderr().is_empty(), "{command}: {}", output.stderr());
        let value: serde_json::Value = serde_json::from_str(output.stdout())
            .unwrap_or_else(|error| panic!("render {command}: {error}"));
        assert_eq!(value["kind"], result_contract, "{command}");
        assert_eq!(
            value["summary"]["capability_id"], capability_id,
            "{command}"
        );
        assert_eq!(
            value["summary"]["result_contract"], result_contract,
            "{command}"
        );
        assert_eq!(value["summary"]["payload_kind"], payload_kind, "{command}");
        assert!(
            value["summary"].get("candidates").is_some()
                || value["summary"].get("members").is_some()
        );
    }
}
