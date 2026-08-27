use crate::error::CliErrorCode;
use crate::{
    args::{ParsedCliCommand, ParsedCliInvocation},
    assemble::CliAppRequestAssembler,
    input::file_input_guard,
    output::{
        document_utility_output::render_typed_document_utility_success,
        solution_artifact_output::{encode_explicit_portfolio_document, encode_response_document},
        AppResponseRenderer, CliOutput,
    },
    typed_document_utility_cli::prepare_native_typed_utility,
};
use clearra_app::{io::AppFilePolicy, AppContext, AppStatus};
#[cfg(feature = "wasm-cpu-runtime")]
use clearra_app::{AppCoreExecutorService, AppServices, AppTablebaseSession};

#[cfg(feature = "wasm-cpu-runtime")]
const PC4_COMPACT_TABLEBASE: &[u8] =
    include_bytes!("../../../apps/clearra-web/static/tablebase/pc4-compact-exact-v12.bin");

const TILING_ONLY_WARNING: &str = "WARNING: Tiling-only search skips BuildUp and probability calculation. Results may include solutions that cannot be built.";

pub(crate) fn route_invocation(invocation: ParsedCliInvocation) -> CliOutput {
    let format = invocation
        .output_verbosity()
        .apply_to_format(invocation.format());
    let language = invocation.language();
    let verbose_paths = invocation.verbose_paths();
    let include_solution_data = invocation.include_solution_data();
    let solution_stdout_format = invocation.solution_stdout_format();
    let solution_artifact_output = invocation.solution_artifact_output().cloned();
    let explicit_ties = invocation.explicit_ties().clone();
    file_input_guard::with_verbose_paths(verbose_paths, || {
        let command = invocation.into_command();
        if let ParsedCliCommand::Help(topic) = command {
            if solution_artifact_output.is_some() || solution_stdout_format.is_some() {
                return CliOutput::error(
                    CliErrorCode::CliArtifactInvalid,
                    "artifact-help-has-no-solution-set",
                );
            }
            return topic.into_output(language);
        }
        if let Some(cursor) = explicit_ties.cursor() {
            let Some(snapshot_path) = explicit_ties.snapshot_path() else {
                return CliOutput::error(
                    CliErrorCode::TieSnapshotInvalid,
                    "tie-snapshot-continuation-path-missing",
                );
            };
            return match crate::tie_snapshot::continue_snapshot(snapshot_path, cursor) {
                Ok(portfolio) => {
                    let mut output = match solution_stdout_format {
                        Some(document_format) => {
                            match encode_explicit_portfolio_document(&portfolio, document_format) {
                                Ok(document) => CliOutput::success(document),
                                Err(error) => {
                                    return CliOutput::error(
                                        CliErrorCode::CliArtifactInvalid,
                                        error.as_str(),
                                    )
                                }
                            }
                        }
                        None => {
                            AppResponseRenderer::render_portfolio_continuation(&portfolio, format)
                        }
                    };
                    if let Some(request) = solution_artifact_output.as_ref() {
                        let prepared = match request.prepare_explicit_portfolio(&portfolio) {
                            Ok(prepared) => prepared,
                            Err(error) => {
                                return CliOutput::error(
                                    CliErrorCode::CliArtifactInvalid,
                                    error.as_str(),
                                )
                            }
                        };
                        let pending =
                            match prepared.into_pending(output.stdout().to_owned(), format) {
                                Ok(pending) => pending,
                                Err(error) => {
                                    return CliOutput::error(
                                        CliErrorCode::CliArtifactInvalid,
                                        error.as_str(),
                                    )
                                }
                            };
                        output = output.with_pending_solution_artifact(pending);
                    }
                    output
                }
                Err(error) => CliOutput::error(error.code(), error.reason()),
            };
        }
        let (command, typed_document_plan) = match prepare_native_typed_utility(command) {
            Ok(prepared) => prepared,
            Err(output) => return output,
        };
        if typed_document_plan.is_some()
            && (solution_artifact_output.is_some()
                || solution_stdout_format.is_some()
                || include_solution_data
                || explicit_ties.active())
        {
            return CliOutput::error(
                CliErrorCode::CliArtifactInvalid,
                "typed-document utilities do not accept solution artifacts, native solution stdout, solution data, or tie options",
            );
        }
        let tiling_only = matches!(
            &command,
            ParsedCliCommand::Pc(args)
                if matches!(
                    args.objective()
                        .trim()
                        .to_ascii_lowercase()
                        .replace('_', "-")
                        .as_str(),
                    "tiling" | "tiling-only"
                )
        ) || matches!(
            &command,
            ParsedCliCommand::Product(tokens)
                if tokens.iter().any(|token| token == "--tiling-only")
                    || tokens.windows(2).any(|pair| {
                        pair[0] == "--objective"
                            && matches!(pair[1].as_str(), "tiling" | "tiling-only")
                    })
                    || tokens
                        .windows(2)
                        .any(|pair| pair[0] == "pc" && pair[1] == "tiling")
        );
        #[cfg(feature = "wasm-cpu-runtime")]
        let _tablebase_session = match tablebase_session_for_command(&command) {
            Ok(session) => session,
            Err(output) => return output,
        };

        let assembly = match CliAppRequestAssembler::assemble(command, format) {
            Ok(assembly) => assembly,
            Err(output) => return output,
        };
        let render_format = assembly.render_format();
        let default_error = assembly.default_error();
        let request = assembly
            .request()
            .with_language(language)
            .with_file_policy(AppFilePolicy::new(verbose_paths));
        let response = product_app_context()
            .with_language(language)
            .with_file_policy(AppFilePolicy::new(verbose_paths))
            .run(request);
        if let Some(plan) = typed_document_plan.as_ref() {
            if response.status() == AppStatus::Success {
                return render_typed_document_utility_success(&response, plan, render_format);
            }
            return AppResponseRenderer::render_with_solution_data(
                response,
                render_format,
                default_error,
                false,
            );
        }
        let explicit_portfolio = if response.status() == AppStatus::Success {
            match explicit_ties.snapshot_path() {
                Some(snapshot_path) => {
                    match crate::tie_snapshot::initialize_snapshot(&response, snapshot_path) {
                        Ok(portfolio) => Some(portfolio),
                        Err(error) => return CliOutput::error(error.code(), error.reason()),
                    }
                }
                None => None,
            }
        } else {
            None
        };
        let include_score_winner_family =
            explicit_ties.requested() && explicit_ties.snapshot_path().is_none();
        if let Some(document_format) = solution_stdout_format {
            if response.status() != AppStatus::Success {
                return AppResponseRenderer::render_with_solution_data(
                    response,
                    render_format,
                    default_error,
                    false,
                );
            }
            let encoded = match explicit_portfolio.as_ref() {
                Some(portfolio) => encode_explicit_portfolio_document(portfolio, document_format),
                None => encode_response_document(&response, document_format),
            };
            return match encoded {
                Ok(document) => CliOutput::success(document),
                Err(error) => CliOutput::error(CliErrorCode::CliArtifactInvalid, error.as_str()),
            };
        }
        let prepared_artifact = if response.status() == AppStatus::Success {
            match solution_artifact_output.as_ref() {
                Some(request) => {
                    let prepared = match explicit_portfolio.as_ref() {
                        Some(portfolio) => request.prepare_explicit_portfolio(portfolio),
                        None => request.prepare(&response),
                    };
                    match prepared {
                        Ok(prepared) => Some(prepared),
                        Err(error) => {
                            return CliOutput::error(
                                CliErrorCode::CliArtifactInvalid,
                                error.as_str(),
                            )
                        }
                    }
                }
                None => None,
            }
        } else {
            None
        };
        let mut output = AppResponseRenderer::render_with_explicit_result(
            response,
            render_format,
            default_error,
            include_solution_data,
            explicit_portfolio.as_ref(),
            include_score_winner_family,
        );
        if let Some(prepared) = prepared_artifact {
            if output.exit_code() != crate::exit::ExitCode::Success {
                return output;
            }
            let pending = match prepared.into_pending(output.stdout().to_owned(), render_format) {
                Ok(pending) => pending,
                Err(error) => {
                    return CliOutput::error(CliErrorCode::CliArtifactInvalid, error.as_str())
                }
            };
            output = output.with_pending_solution_artifact(pending);
        }
        if tiling_only {
            output.with_surrounding_warning(TILING_ONLY_WARNING)
        } else {
            output
        }
    })
}

#[cfg(feature = "wasm-cpu-runtime")]
fn tablebase_session_for_command(
    command: &ParsedCliCommand,
) -> Result<Option<AppTablebaseSession>, CliOutput> {
    let requested = match command {
        ParsedCliCommand::Pc(args) => args.tablebase_requested() == Some(true),
        ParsedCliCommand::FailedQueue(args) => args.pc().tablebase_requested() == Some(true),
        ParsedCliCommand::Setup(args) => args.tablebase_requested() == Some(true),
        ParsedCliCommand::Product(tokens) => tokens
            .iter()
            .any(|token| matches!(token.as_str(), "--tablebase" | "--tb")),
        _ => false,
    };
    install_requested_tablebase(requested, PC4_COMPACT_TABLEBASE)
}

#[cfg(feature = "wasm-cpu-runtime")]
fn install_requested_tablebase(
    requested: bool,
    artifact: &[u8],
) -> Result<Option<AppTablebaseSession>, CliOutput> {
    if !requested {
        return Ok(None);
    }

    AppTablebaseSession::install_pc4_compact(artifact)
        .map(Some)
        .map_err(|error| {
            CliOutput::error(
                CliErrorCode::TablebaseInstallFailed,
                format!("PC4 tablebase installation failed: {}", error.reason()),
            )
        })
}

fn product_app_context() -> AppContext {
    #[cfg(feature = "wasm-cpu-runtime")]
    {
        return AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
    }

    #[cfg(not(feature = "wasm-cpu-runtime"))]
    AppContext::default()
}

#[cfg(all(test, feature = "wasm-cpu-runtime"))]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{install_requested_tablebase, route_invocation};
    use crate::args::CliParser;
    use crate::{error::CliErrorCode, exit::ExitCode};
    use clearra_app::decode_ctk3_exact;

    #[test]
    fn actual_pc_score_cli_pipeline_emits_v2_while_top_level_score_stays_generic() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        for (source, expected_kind) in [
            (
                "clearra --format json pc score --lines 2 --queue IOTJL",
                "pc-score-summary.v2",
            ),
            (
                "clearra --format json pc score --board-mask 0x3f0 --height 1 --pieces 1 --lines 1 --queue I",
                "pc-score-summary.v2",
            ),
            (
                "clearra --format json pc score-finder --board-mask 0x3f0 --height 1 --pieces 1 --lines 1 --queue I",
                "pc-fixed-score-witness.v2",
            ),
            (
                "clearra --format json pc score-minimals --board-mask 0x3f0 --height 1 --pieces 1 --lines 1 --queue I",
                "pc-score-portfolio.v2",
            ),
            (
                "clearra --format json score v115@vhAAgH IOTJL 2",
                "pc-scenario",
            ),
        ] {
            let invocation = CliParser::parse(source.split_whitespace())
                .unwrap_or_else(|_| panic!("parse {source}"));
            let output = route_invocation(invocation);
            assert_eq!(
                output.exit_code(),
                ExitCode::Success,
                "{source}: stderr={} stdout={}",
                output.stderr(),
                output.stdout()
            );
            assert!(output.stderr().is_empty(), "{source}: {}", output.stderr());
            let value: serde_json::Value = serde_json::from_str(output.stdout())
                .unwrap_or_else(|_| panic!("rendered JSON for {source}"));
            assert_eq!(value["kind"], expected_kind, "{source}");
            assert_eq!(
                value["contract"]["command"]["kind"], expected_kind,
                "{source}"
            );

            if expected_kind == "pc-score-summary.v2" {
                assert_eq!(
                    value["summary"]["score_accuracy_level"], "basic-approximation",
                    "{source}"
                );
                assert_eq!(
                    value["summary"]["score_profile_specific_exact"], false,
                    "{source}"
                );
                assert!(
                    value["summary"]["score_accuracy_reason"].is_string(),
                    "{source}"
                );
                for field in [
                    "score_evaluation_complete",
                    "score_matrix_complete",
                    "score_summary_complete",
                    "probability_complete",
                    "resource_probability_complete",
                ] {
                    assert_eq!(value["summary"][field], true, "{source}: {field}");
                }
                assert!(
                    value["summary"]["score_failed_pc_pattern_count"].is_number(),
                    "{source}"
                );
                assert!(
                    value["summary"]["score_failed_pc_pattern_score"].is_number(),
                    "{source}"
                );
                for numeric in [
                    "score_unconditional_expected_score",
                    "score_unconditional_expected_attack",
                ] {
                    assert!(value["summary"][numeric].is_number(), "{source}: {numeric}");
                }
                let rendered = output.stdout();
                assert!(!rendered.contains("score_pattern_winners"), "{source}");
                assert!(!rendered.contains("portfolio_alternative_page"), "{source}");
                for private in [
                    "pc_score_problem_evidence",
                    "exact_scoring_execution_batches",
                    "postprocess_score_cells",
                    "execution_authority",
                    "memory_evidence",
                ] {
                    assert!(!rendered.contains(private), "{source}: {private}");
                }
            } else if expected_kind == "pc-fixed-score-witness.v2" {
                let summary = &value["summary"];
                assert_eq!(summary["capability_id"], "pc.score-finder");
                assert_eq!(summary["result_contract"], expected_kind);
                assert_eq!(
                    summary["score_pattern_winner_ordering"],
                    "pattern-id-ascending-then-candidate-id-ascending"
                );
                assert_eq!(
                    summary["score_pattern_winner_equality"],
                    "score-only-attack-informational"
                );
                assert_eq!(summary["score_pattern_winner_complete"], true);
                assert!(summary["score_pattern_winner_count"].is_string());
                let winners = summary["score_pattern_winners"]
                    .as_array()
                    .expect("default fixed-score winner family");
                assert!(!winners.is_empty());
                assert!(winners.iter().all(|winner| {
                    winner["candidate_id"].is_string()
                        && winner["score"].is_string()
                        && winner["informational_attack"].is_string()
                }));
                assert!(value.get("portfolio_alternative_page").is_none());
                assert!(!output.stdout().contains("tie_cursor"));
            } else if expected_kind == "pc-score-portfolio.v2" {
                let summary = &value["summary"];
                assert_eq!(summary["score_minimals_score_equality"], "score-only");
                assert_eq!(
                    summary["score_minimals_attack_role"],
                    "informational-only"
                );
                assert_eq!(
                    summary["score_minimals_canonical_selection"],
                    "smallest-canonical-candidate-id"
                );
                assert!(summary["score_minimals_canonical_candidate_id"].is_string());
                assert!(summary["score_minimals_canonical_solution_key"].is_string());
                assert!(value.get("portfolio_alternative_page").is_none());
                assert!(!output.stdout().contains("tie_cursor"));
            } else {
                assert_ne!(value["kind"], "pc-score-summary.v2", "{source}");
            }
        }
    }

    #[test]
    fn explicit_cli_result_surfaces_are_opt_in_on_actual_pc_routes() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();

        let score = CliParser::parse(
            "clearra --format json pc score --board-mask 0x3f0 --height 1 --pieces 1 --lines 1 --queue I --ties"
                .split_whitespace(),
        )
        .expect("explicit score family parse");
        let score = route_invocation(score);
        assert_eq!(score.exit_code(), ExitCode::Success, "{}", score.stderr());
        let score_json: serde_json::Value =
            serde_json::from_str(score.stdout()).expect("explicit score JSON");
        let score_surface = score_json.get("summary").unwrap_or(&score_json);
        assert_eq!(
            score_surface["score_pattern_winner_ordering"],
            "pattern-id-ascending-then-candidate-id-ascending"
        );
        assert_eq!(
            score_surface["score_pattern_winner_equality"],
            "score-only-attack-informational"
        );
        let winners = score_surface["score_pattern_winners"]
            .as_array()
            .expect("explicit score winners");
        assert!(!winners.is_empty());
        assert!(winners
            .iter()
            .all(|winner| winner["candidate_id"].is_string()));

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "clearra-cli-portfolio-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("portfolio test directory");
        let snapshot = directory.join("minimum-portfolios.jsonl");
        let snapshot_text = snapshot.to_string_lossy().into_owned();
        let minimals = CliParser::parse([
            "clearra".to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
            "pc".to_owned(),
            "minimals".to_owned(),
            "--lines".to_owned(),
            "1".to_owned(),
            "--board-mask".to_owned(),
            "0x3f".to_owned(),
            "--height".to_owned(),
            "1".to_owned(),
            "--pieces".to_owned(),
            "1".to_owned(),
            "--queue".to_owned(),
            "I".to_owned(),
            "--hold".to_owned(),
            "empty".to_owned(),
            "--rule".to_owned(),
            "srs-plus".to_owned(),
            "--ties".to_owned(),
            "--tie-snapshot".to_owned(),
            snapshot_text,
        ])
        .expect("explicit minimum portfolio parse");
        let minimals = route_invocation(minimals);
        assert_eq!(
            minimals.exit_code(),
            ExitCode::Success,
            "stderr={} stdout={}",
            minimals.stderr(),
            minimals.stdout()
        );
        let minimals_json: serde_json::Value =
            serde_json::from_str(minimals.stdout()).expect("explicit minimals JSON");
        let minimals_surface = minimals_json.get("summary").unwrap_or(&minimals_json);
        let page = minimals_surface["portfolio_alternative_page"]
            .as_object()
            .expect("explicit portfolio page");
        assert_eq!(page["alternative_index"], "1");
        assert!(page["members"]
            .as_array()
            .expect("portfolio members")
            .iter()
            .all(|member| member["candidate_id"].is_string()));
        assert!(snapshot.is_file());

        let score_snapshot = directory.join("score-portfolios.jsonl");
        let score_minimals = CliParser::parse([
            "clearra".to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
            "pc".to_owned(),
            "score-minimals".to_owned(),
            "--lines".to_owned(),
            "1".to_owned(),
            "--board-mask".to_owned(),
            "0x3f0".to_owned(),
            "--height".to_owned(),
            "1".to_owned(),
            "--pieces".to_owned(),
            "1".to_owned(),
            "--queue".to_owned(),
            "I".to_owned(),
            "--hold".to_owned(),
            "empty".to_owned(),
            "--rule".to_owned(),
            "srs-plus".to_owned(),
            "--ties".to_owned(),
            "--tie-snapshot".to_owned(),
            score_snapshot.to_string_lossy().into_owned(),
        ])
        .expect("explicit score-minimals portfolio parse");
        let score_minimals = route_invocation(score_minimals);
        assert_eq!(
            score_minimals.exit_code(),
            ExitCode::Success,
            "stderr={} stdout={}",
            score_minimals.stderr(),
            score_minimals.stdout()
        );
        let score_minimals_json: serde_json::Value =
            serde_json::from_str(score_minimals.stdout()).expect("explicit score-minimals JSON");
        assert_eq!(score_minimals_json["kind"], "pc-score-portfolio.v2");
        let score_surface = score_minimals_json
            .get("summary")
            .unwrap_or(&score_minimals_json);
        assert_eq!(score_surface["score_minimals_score_equality"], "score-only");
        assert_eq!(
            score_surface["score_minimals_attack_role"],
            "informational-only"
        );
        assert_eq!(
            score_surface["score_minimals_canonical_selection"],
            "smallest-canonical-candidate-id"
        );
        assert!(score_surface["score_minimals_canonical_candidate_id"].is_string());
        let score_page = score_surface["portfolio_alternative_page"]
            .as_object()
            .expect("explicit score-minimals portfolio page");
        assert_eq!(score_page["alternative_index"], "1");
        assert!(score_page["members"]
            .as_array()
            .expect("score portfolio members")
            .iter()
            .all(|member| member["candidate_id"].is_string()));
        assert!(score_snapshot.is_file());

        let document_snapshot = directory.join("minimum-portfolios-document.jsonl");
        let document_invocation = CliParser::parse([
            "clearra".to_owned(),
            "--format".to_owned(),
            "ctk3".to_owned(),
            "pc".to_owned(),
            "minimals".to_owned(),
            "--lines".to_owned(),
            "1".to_owned(),
            "--board-mask".to_owned(),
            "0x3f".to_owned(),
            "--height".to_owned(),
            "1".to_owned(),
            "--pieces".to_owned(),
            "1".to_owned(),
            "--queue".to_owned(),
            "I".to_owned(),
            "--hold".to_owned(),
            "empty".to_owned(),
            "--rule".to_owned(),
            "srs-plus".to_owned(),
            "--ties".to_owned(),
            "--tie-snapshot".to_owned(),
            document_snapshot.to_string_lossy().into_owned(),
        ])
        .expect("explicit minimum portfolio CTK3 parse");
        let document = route_invocation(document_invocation);
        assert_eq!(
            document.exit_code(),
            ExitCode::Success,
            "stderr={} stdout={}",
            document.stderr(),
            document.stdout()
        );
        let decoded = decode_ctk3_exact(document.stdout()).expect("portfolio CTK3 document");
        assert_eq!(
            decoded.pages.len(),
            page["members"].as_array().unwrap().len()
        );

        fs::remove_file(&snapshot).expect("remove portfolio snapshot");
        fs::remove_file(&score_snapshot).expect("remove score portfolio snapshot");
        fs::remove_file(&document_snapshot).expect("remove portfolio document snapshot");
        fs::remove_dir(&directory).expect("remove portfolio directory");
    }

    #[test]
    fn actual_pc_save_cli_pipeline_separates_probabilities_and_uses_plain_winner_lists() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        for (subcommand, expected_kind) in [
            ("saves", "pc-save-groups.v2"),
            ("best-save", "pc-best-save.v2"),
        ] {
            let source = format!(
                "clearra --format json pc {subcommand} --lines 2 --board-mask 0xf3fcf \
                 --height 2 --pieces 1 --patterns P7 --no-hold --backend cpu"
            );
            let invocation = CliParser::parse(source.split_whitespace())
                .unwrap_or_else(|_| panic!("parse {source}"));
            let output = route_invocation(invocation);
            assert_eq!(
                output.exit_code(),
                ExitCode::Success,
                "{source}: stderr={} stdout={}",
                output.stderr(),
                output.stdout()
            );
            assert!(output.stderr().is_empty(), "{source}: {}", output.stderr());
            let value: serde_json::Value = serde_json::from_str(output.stdout())
                .unwrap_or_else(|_| panic!("rendered JSON for {source}"));
            assert_eq!(value["kind"], expected_kind, "{source}");
            assert_eq!(
                value["contract"]["command"]["kind"], expected_kind,
                "{source}"
            );

            if subcommand == "saves" {
                let groups = value["summary"]["save_groups"]
                    .as_array()
                    .expect("save group list");
                assert_eq!(groups.len(), 1);
                assert_eq!(groups[0]["unconditional_probability"], 1.0 / 7.0);
                assert_eq!(groups[0]["conditional_probability_given_pc"], 1.0);
            } else {
                assert_eq!(value["summary"]["best_save_schema"], "clearra-save-v1");
                assert_eq!(
                    value["summary"]["best_save_probability_basis"],
                    "whole-universe-unconditional"
                );
                let winners = value["summary"]["best_save_winners"]
                    .as_array()
                    .expect("ordinary best-save winner list");
                assert_eq!(winners.len(), 1);
                assert_eq!(
                    winners[0]["exact_group_probability"],
                    winners[0]["group"]["unconditional_probability"]
                );
                assert_eq!(winners[0]["group"]["conditional_probability_given_pc"], 1.0);
                assert!(!output.stdout().contains("portfolio"));
            }
        }
    }

    #[test]
    fn explicit_tablebase_request_fails_closed_when_installation_fails() {
        let output = install_requested_tablebase(true, b"not-a-tablebase")
            .expect_err("an explicit request must not silently fall back");

        assert_eq!(output.exit_code(), ExitCode::InternalError);
        assert!(output
            .stderr()
            .contains(CliErrorCode::TablebaseInstallFailed.as_str()));
        assert!(output.stderr().contains("pc4_tablebase_header_invalid"));
    }

    #[test]
    fn unrequested_tablebase_does_not_touch_the_artifact() {
        assert!(install_requested_tablebase(false, b"not-a-tablebase")
            .expect("disabled tablebase must not be installed")
            .is_none());
    }
}
