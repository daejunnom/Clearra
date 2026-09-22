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
use clearra_app::{AppCoreExecutorService, AppServices};
use clearra_i18n::LanguageId;

const TILING_ONLY_WARNING: &str = "WARNING: Tiling-only search skips BuildUp and probability calculation. Results may include solutions that cannot be built.";

pub(crate) fn route_invocation(invocation: ParsedCliInvocation) -> CliOutput {
    let format = invocation
        .output_verbosity()
        .apply_to_format(invocation.format());
    let language = invocation.language();
    let verbose_paths = invocation.verbose_paths();
    let include_solution_data = invocation.include_solution_data();
    let solution_stdout_format = invocation.solution_stdout_format();
    let localize_text = solution_stdout_format.is_none()
        && matches!(
            format,
            crate::output::RenderFormat::Text
                | crate::output::RenderFormat::TextVerbose
                | crate::output::RenderFormat::TextDiagnostics
        );
    let solution_artifact_output = invocation.solution_artifact_output().cloned();
    let explicit_ties = invocation.explicit_ties().clone();
    let output = file_input_guard::with_verbose_paths(verbose_paths, || {
        let command = invocation.into_command();
        if let ParsedCliCommand::LegalBoard(args) = &command {
            if explicit_ties.active()
                || solution_artifact_output.is_some()
                || solution_stdout_format.is_some()
                || include_solution_data
            {
                return CliOutput::error(
                    CliErrorCode::CliInvalidValue,
                    "legal-board management does not return a solution set",
                );
            }
            return crate::legal_board_assets::run(
                args,
                language,
                matches!(format, crate::output::RenderFormat::Json),
            );
        }
        if let ParsedCliCommand::ReachabilityPack(args) = &command {
            if explicit_ties.active()
                || solution_artifact_output.is_some()
                || solution_stdout_format.is_some()
                || include_solution_data
            {
                return CliOutput::error(
                    CliErrorCode::CliInvalidValue,
                    "reachability-pack management does not return a solution set",
                );
            }
            return crate::conditioned_reachability_assets::run(
                args,
                language,
                matches!(format, crate::output::RenderFormat::Json),
            );
        }
        if let ParsedCliCommand::Tablebase(args) = &command {
            if explicit_ties.active()
                || solution_artifact_output.is_some()
                || solution_stdout_format.is_some()
                || include_solution_data
            {
                return CliOutput::error(
                    CliErrorCode::CliInvalidValue,
                    "tablebase management does not return a solution set",
                );
            }
            return crate::tablebase_download::run(
                args,
                language,
                matches!(format, crate::output::RenderFormat::Json),
            );
        }
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
        let tablebase_requested = command_requests_tablebase(&command);
        let offline_fallback_requested = command_requests_offline_fallback(&command);
        if offline_fallback_requested && !tablebase_requested {
            return CliOutput::error(
                CliErrorCode::CliInvalidValue,
                "--offline-fallback requires --tablebase",
            );
        }
        let command = command_without_offline_fallback_marker(command);
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
        let offline_command =
            offline_fallback_requested.then(|| command_without_tablebase(command.clone()));
        #[cfg(not(feature = "online-pc4-tablebase"))]
        let command = if tablebase_requested {
            if !offline_fallback_requested {
                return CliOutput::error(
                    CliErrorCode::TablebaseLookupFailed,
                    tablebase_lookup_failure_message("pc4_online_unavailable", language),
                );
            }
            offline_command
                .clone()
                .expect("explicit fallback command for unavailable tablebase")
        } else {
            command
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
        crate::legal_board_assets::activate_for_request(&request);
        crate::conditioned_reachability_assets::activate_for_request(&request);
        #[cfg(feature = "online-pc4-tablebase")]
        let (response, offline_fallback_reason) = if tablebase_requested {
            let context = product_app_context()
                .with_language(language)
                .with_file_policy(AppFilePolicy::new(verbose_paths));
            match crate::tablebase_download::execute(context, request) {
                Ok(response) => (response, None),
                Err(reason) if offline_fallback_requested && offline_fallback_allowed(reason) => {
                    let fallback = match CliAppRequestAssembler::assemble(
                        offline_command.expect("authorized fallback command"),
                        format,
                    ) {
                        Ok(assembly) => assembly,
                        Err(output) => return output,
                    };
                    let request = fallback
                        .request()
                        .with_language(language)
                        .with_file_policy(AppFilePolicy::new(verbose_paths));
                    crate::legal_board_assets::activate_for_request(&request);
                    crate::conditioned_reachability_assets::activate_for_request(&request);
                    let response = product_app_context()
                        .with_language(language)
                        .with_file_policy(AppFilePolicy::new(verbose_paths))
                        .run(request);
                    (response, Some(reason))
                }
                Err(reason) => {
                    return CliOutput::error(
                        CliErrorCode::TablebaseLookupFailed,
                        tablebase_lookup_failure_message(reason, language),
                    )
                }
            }
        } else {
            (
                product_app_context()
                    .with_language(language)
                    .with_file_policy(AppFilePolicy::new(verbose_paths))
                    .run(request),
                None,
            )
        };
        #[cfg(not(feature = "online-pc4-tablebase"))]
        let (response, offline_fallback_reason) = (
            product_app_context()
                .with_language(language)
                .with_file_policy(AppFilePolicy::new(verbose_paths))
                .run(request),
            (tablebase_requested && offline_fallback_requested).then_some("pc4_online_unavailable"),
        );
        if let Some(plan) = typed_document_plan.as_ref() {
            if response.status() == AppStatus::Success {
                return with_offline_fallback_warning(
                    render_typed_document_utility_success(&response, plan, render_format),
                    offline_fallback_reason,
                    language,
                );
            }
            return with_offline_fallback_warning(
                AppResponseRenderer::render_with_solution_data(
                    response,
                    render_format,
                    default_error,
                    false,
                ),
                offline_fallback_reason,
                language,
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
                return with_offline_fallback_warning(
                    AppResponseRenderer::render_with_solution_data(
                        response,
                        render_format,
                        default_error,
                        false,
                    ),
                    offline_fallback_reason,
                    language,
                );
            }
            let encoded = match explicit_portfolio.as_ref() {
                Some(portfolio) => encode_explicit_portfolio_document(portfolio, document_format),
                None => encode_response_document(&response, document_format),
            };
            return with_offline_fallback_warning(
                match encoded {
                    Ok(document) => CliOutput::success(document),
                    Err(error) => {
                        CliOutput::error(CliErrorCode::CliArtifactInvalid, error.as_str())
                    }
                },
                offline_fallback_reason,
                language,
            );
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
        let output = if tiling_only {
            output.with_surrounding_warning(TILING_ONLY_WARNING)
        } else {
            output
        };
        with_offline_fallback_warning(output, offline_fallback_reason, language)
    });
    if localize_text {
        output.localized_for(language)
    } else {
        output
    }
}

fn tablebase_lookup_failure_message(reason: &str, language: LanguageId) -> &'static str {
    let class = match reason {
        "pc4_online_field_miss" => 0,
        "pc4_online_offline" => 1,
        "pc4_online_rate_limited" => 2,
        "pc4_online_timeout" => 3,
        "pc4_online_unavailable"
        | "pc4_online_generation_unavailable"
        | "pc4_online_profile_or_target_unavailable" => 4,
        _ => 5,
    };
    match (language, class) {
        (LanguageId::Ko, 0) => "선택한 테이블베이스에 이 필드의 항목이 없습니다. 오프라인 탐색은 시작하지 않았습니다. --tablebase를 제거하거나 --no-tablebase로 다시 실행하면 오프라인으로 탐색하며 Ctrl+C로 중단할 수 있습니다.",
        (LanguageId::Ko, 1) => "테이블베이스 서비스에 연결할 수 없습니다. 오프라인 탐색은 시작하지 않았습니다. --tablebase를 제거하거나 --no-tablebase로 다시 실행하면 오프라인으로 탐색하며 Ctrl+C로 중단할 수 있습니다.",
        (LanguageId::Ko, 2) => "테이블베이스 서비스의 요청 한도에 도달했습니다. 오프라인 탐색은 시작하지 않았습니다. 잠시 뒤 다시 시도하거나 --tablebase를 제거하고 오프라인으로 다시 실행해 주세요. 오프라인 탐색은 Ctrl+C로 중단할 수 있습니다.",
        (LanguageId::Ko, 3) => "테이블베이스 요청 시간이 초과되었습니다. 오프라인 탐색은 시작하지 않았습니다. 다시 시도하거나 --tablebase를 제거하고 오프라인으로 다시 실행해 주세요. 오프라인 탐색은 Ctrl+C로 중단할 수 있습니다.",
        (LanguageId::Ko, 4) => "이 규칙과 목표에 사용할 수 있도록 검증된 테이블베이스가 없습니다. 오프라인 탐색은 시작하지 않았습니다. --tablebase를 제거하거나 --no-tablebase로 다시 실행하면 오프라인으로 탐색하며 Ctrl+C로 중단할 수 있습니다.",
        (LanguageId::Ko, _) => "테이블베이스 조회를 완료하지 못했습니다. 오프라인 탐색은 시작하지 않았습니다. --tablebase를 제거하거나 --no-tablebase로 다시 실행하면 오프라인으로 탐색하며 Ctrl+C로 중단할 수 있습니다.",
        (LanguageId::Ja, 0) => "選択したテーブルベースにこのフィールドの項目がありません。オフライン探索は開始していません。--tablebaseを外すか--no-tablebaseで再実行するとオフラインで探索でき、Ctrl+Cで中断できます。",
        (LanguageId::Ja, 1) => "テーブルベースサービスに接続できません。オフライン探索は開始していません。--tablebaseを外すか--no-tablebaseで再実行するとオフラインで探索でき、Ctrl+Cで中断できます。",
        (LanguageId::Ja, 2) => "テーブルベースサービスのリクエスト上限に達しました。オフライン探索は開始していません。しばらく待って再試行するか、--tablebaseを外してオフラインで再実行してください。オフライン探索はCtrl+Cで中断できます。",
        (LanguageId::Ja, 3) => "テーブルベースのリクエストがタイムアウトしました。オフライン探索は開始していません。再試行するか、--tablebaseを外してオフラインで再実行してください。オフライン探索はCtrl+Cで中断できます。",
        (LanguageId::Ja, 4) => "このルールと目標に利用できる検証済みテーブルベースがありません。オフライン探索は開始していません。--tablebaseを外すか--no-tablebaseで再実行するとオフラインで探索でき、Ctrl+Cで中断できます。",
        (LanguageId::Ja, _) => "テーブルベースの照会を完了できませんでした。オフライン探索は開始していません。--tablebaseを外すか--no-tablebaseで再実行するとオフラインで探索でき、Ctrl+Cで中断できます。",
        (LanguageId::En, 0) => "The selected tablebase has no entry for this field. Offline search was not started. Re-run without --tablebase or with --no-tablebase to search offline; press Ctrl+C to stop it.",
        (LanguageId::En, 1) => "The tablebase service could not be reached. Offline search was not started. Re-run without --tablebase or with --no-tablebase to search offline; press Ctrl+C to stop it.",
        (LanguageId::En, 2) => "The tablebase service rate limit was reached. Offline search was not started. Wait and retry, or re-run without --tablebase to search offline; press Ctrl+C to stop it.",
        (LanguageId::En, 3) => "The tablebase request timed out. Offline search was not started. Retry, or re-run without --tablebase to search offline; press Ctrl+C to stop it.",
        (LanguageId::En, 4) => "No qualified tablebase is available for this rule and target. Offline search was not started. Re-run without --tablebase or with --no-tablebase to search offline; press Ctrl+C to stop it.",
        (LanguageId::En, _) => "The tablebase lookup could not be completed. Offline search was not started. Re-run without --tablebase or with --no-tablebase to search offline; press Ctrl+C to stop it.",
    }
}

#[cfg_attr(not(feature = "online-pc4-tablebase"), allow(dead_code))]
fn offline_fallback_allowed(reason: &str) -> bool {
    !reason.eq_ignore_ascii_case("tablebase: search cancelled")
        && !reason.eq_ignore_ascii_case("pc4_online_cancelled")
}

fn with_offline_fallback_warning(
    output: CliOutput,
    reason: Option<&str>,
    language: LanguageId,
) -> CliOutput {
    if reason.is_none() {
        return output;
    }
    let warning = match language {
        LanguageId::Ko => "테이블베이스 조회를 완료하지 못해 명시적으로 요청한 오프라인 exact 탐색을 실행했습니다. Ctrl+C로 중단할 수 있습니다.",
        LanguageId::Ja => "テーブルベースの照会を完了できなかったため、明示的に指定されたオフラインexact探索を実行しました。Ctrl+Cで中断できます。",
        LanguageId::En => "The tablebase lookup did not complete, so the explicitly requested offline exact search was run. Press Ctrl+C to stop it.",
    };
    output.with_surrounding_warning(warning)
}

fn command_requests_offline_fallback(command: &ParsedCliCommand) -> bool {
    match command {
        ParsedCliCommand::Pc(args) => args.offline_fallback_requested(),
        ParsedCliCommand::FailedQueue(args) => args.pc().offline_fallback_requested(),
        ParsedCliCommand::Setup(args) => args.offline_fallback_requested(),
        ParsedCliCommand::Product(tokens) => {
            tokens.iter().any(|token| token == "--offline-fallback")
        }
        _ => false,
    }
}

fn command_without_offline_fallback_marker(command: ParsedCliCommand) -> ParsedCliCommand {
    match command {
        ParsedCliCommand::Pc(args) => {
            ParsedCliCommand::Pc(args.with_offline_fallback_requested(false))
        }
        ParsedCliCommand::FailedQueue(args) => {
            ParsedCliCommand::FailedQueue(crate::args::FailedQueueArgs::new(
                args.pc().clone().with_offline_fallback_requested(false),
                args.patterns().map(ToOwned::to_owned),
                args.failed_pattern_limit(),
            ))
        }
        ParsedCliCommand::Setup(args) => {
            ParsedCliCommand::Setup(args.with_offline_fallback_requested(false))
        }
        ParsedCliCommand::Product(mut tokens) => {
            tokens.retain(|token| token != "--offline-fallback");
            ParsedCliCommand::Product(tokens)
        }
        command => command,
    }
}

#[cfg_attr(not(feature = "online-pc4-tablebase"), allow(dead_code))]
fn command_without_tablebase(command: ParsedCliCommand) -> ParsedCliCommand {
    match command {
        ParsedCliCommand::Pc(args) => ParsedCliCommand::Pc(
            args.with_tablebase_requested(Some(false))
                .with_offline_fallback_requested(false),
        ),
        ParsedCliCommand::FailedQueue(args) => {
            ParsedCliCommand::FailedQueue(crate::args::FailedQueueArgs::new(
                args.pc()
                    .clone()
                    .with_tablebase_requested(Some(false))
                    .with_offline_fallback_requested(false),
                args.patterns().map(ToOwned::to_owned),
                args.failed_pattern_limit(),
            ))
        }
        ParsedCliCommand::Setup(args) => ParsedCliCommand::Setup(
            args.with_tablebase_requested(Some(false))
                .with_offline_fallback_requested(false),
        ),
        ParsedCliCommand::Product(mut tokens) => {
            tokens.retain(|token| {
                !matches!(
                    token.as_str(),
                    "--tablebase" | "--tb" | "--no-tablebase" | "--no-tb" | "--offline-fallback"
                )
            });
            tokens.push("--no-tablebase".to_owned());
            ParsedCliCommand::Product(tokens)
        }
        command => command,
    }
}

fn command_requests_tablebase(command: &ParsedCliCommand) -> bool {
    match command {
        ParsedCliCommand::Pc(args) => args.tablebase_requested() == Some(true),
        ParsedCliCommand::FailedQueue(args) => args.pc().tablebase_requested() == Some(true),
        ParsedCliCommand::Setup(args) => args.tablebase_requested() == Some(true),
        ParsedCliCommand::Product(tokens) => tokens
            .iter()
            .rev()
            .find_map(|token| match token.as_str() {
                "--tablebase" | "--tb" => Some(true),
                "--no-tablebase" | "--no-tb" => Some(false),
                _ => None,
            })
            .unwrap_or(false),
        _ => false,
    }
}

fn product_app_context() -> AppContext {
    #[cfg(feature = "wasm-cpu-runtime")]
    {
        AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        )
    }

    #[cfg(not(feature = "wasm-cpu-runtime"))]
    {
        AppContext::default()
    }
}

#[cfg(test)]
mod offline_fallback_contract_tests {
    use super::{
        command_requests_offline_fallback, command_requests_tablebase,
        command_without_offline_fallback_marker, command_without_tablebase,
        offline_fallback_allowed,
    };
    use crate::args::{CliParser, ParsedCliCommand};

    #[test]
    fn explicit_product_fallback_marker_is_host_only_and_rebuilds_an_offline_command() {
        let source = ParsedCliCommand::Product(vec![
            "pc".to_owned(),
            "minimals".to_owned(),
            "--lines".to_owned(),
            "4".to_owned(),
            "--tablebase".to_owned(),
            "--offline-fallback".to_owned(),
        ]);
        assert!(command_requests_tablebase(&source));
        assert!(command_requests_offline_fallback(&source));

        let online = command_without_offline_fallback_marker(source);
        let ParsedCliCommand::Product(online_tokens) = &online else {
            panic!("expected product command");
        };
        assert!(online_tokens.iter().any(|token| token == "--tablebase"));
        assert!(!online_tokens
            .iter()
            .any(|token| token == "--offline-fallback"));

        let offline = command_without_tablebase(online);
        let ParsedCliCommand::Product(offline_tokens) = offline else {
            panic!("expected product command");
        };
        assert!(!offline_tokens
            .iter()
            .any(|token| matches!(token.as_str(), "--tablebase" | "--tb")));
        assert_eq!(
            offline_tokens
                .iter()
                .filter(|token| token.as_str() == "--no-tablebase")
                .count(),
            1
        );
        assert!(offline_fallback_allowed("pc4_online_offline"));
        assert!(!offline_fallback_allowed("tablebase: search cancelled"));
        assert!(!offline_fallback_allowed("pc4_online_cancelled"));
    }

    #[test]
    fn typed_pc_and_setup_fallback_preserve_the_request_and_disable_only_tablebase() {
        for source in [
            [
                "clearra",
                "pc",
                "--lines",
                "4",
                "--tablebase",
                "--offline-fallback",
            ]
            .as_slice(),
            [
                "clearra",
                "setup",
                "--remaining",
                "IOTSZJL",
                "--tablebase",
                "--offline-fallback",
            ]
            .as_slice(),
        ] {
            let command = CliParser::parse(source.iter().copied())
                .expect("explicit typed fallback")
                .into_command();
            assert!(command_requests_tablebase(&command));
            assert!(command_requests_offline_fallback(&command));

            let stripped = command_without_tablebase(command);
            assert!(!command_requests_tablebase(&stripped));
            assert!(!command_requests_offline_fallback(&stripped));
            match stripped {
                ParsedCliCommand::Pc(args) => assert_eq!(args.lines(), 4),
                ParsedCliCommand::Setup(args) => assert_eq!(args.remaining(), "IOTSZJL"),
                _ => panic!("unexpected rewritten command"),
            }
        }
    }
}

#[cfg(all(test, feature = "wasm-cpu-runtime"))]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{route_invocation, tablebase_lookup_failure_message, LanguageId};
    use crate::args::CliParser;
    use crate::exit::ExitCode;
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
                    value["summary"]["payload_kind"], "pc-score-field-summary",
                    "{source}"
                );
                assert_eq!(
                    value["summary"]["score_solution_field_contract"],
                    "pc-score-solution-field-average.v1",
                    "{source}"
                );
                assert_eq!(
                    value["summary"]["score_solution_field_ordering"],
                    "normalized-solution-field-order",
                    "{source}"
                );
                assert_eq!(
                    value["summary"]["score_solution_field_average_basis"],
                    "whole-materialized-pattern-universe-failed-pc-zero",
                    "{source}"
                );
                assert_eq!(
                    value["summary"]["score_evaluation_basis"],
                    "all-traces",
                    "{source}"
                );
                assert_eq!(value["summary"]["score_evaluation_scope"], "full", "{source}");
                assert_eq!(
                    value["summary"]["score_overall_basis"],
                    "all-materialized-patterns-failed-pc-zero",
                    "{source}"
                );
                assert_eq!(value["summary"]["score_summary_complete"], true, "{source}");
                for decimal in [
                    "materialized_pattern_count",
                    "score_solution_field_count",
                    "score_success_pattern_count",
                    "score_failed_pc_pattern_count",
                    "score_covered_probability",
                    "score_overall_score",
                ] {
                    assert!(value["summary"][decimal].is_string(), "{source}: {decimal}");
                }
                let materialized_count = value["summary"]["materialized_pattern_count"]
                    .as_str()
                    .and_then(|value| value.parse::<usize>().ok())
                    .expect("materialized pattern count");
                let solution_field_count = value["summary"]["score_solution_field_count"]
                    .as_str()
                    .and_then(|value| value.parse::<usize>().ok())
                    .expect("normalized solution field count");
                let materialized_count_decimal = materialized_count.to_string();
                let rows = value["summary"]["score_solution_fields"]
                    .as_array()
                    .expect("ordinary pc.score field rows");
                assert_eq!(rows.len(), solution_field_count, "{source}");
                assert!(rows.iter().all(|row| {
                    row["normalized_field_key"].is_string()
                        && row["average_score"].is_string()
                        && row["covered_pattern_count"].is_string()
                        && row["pattern_count"].as_str()
                            == Some(materialized_count_decimal.as_str())
                        && row["score_complete"] == true
                        && row.get("input_pattern").is_none()
                        && row.get("candidate_id").is_none()
                        && row.get("informational_attack").is_none()
                }));
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
                assert_eq!(
                    summary["score_pattern_canonical_selection"],
                    "smallest-canonical-candidate-id"
                );
                let canonical = summary["score_pattern_canonical_winner"]
                    .as_object()
                    .expect("core-owned score-finder canonical winner");
                let winners = summary["score_pattern_winners"]
                    .as_array()
                    .expect("default fixed-score winner family");
                assert!(!winners.is_empty());
                assert!(winners.iter().any(|winner| winner.as_object() == Some(canonical)));
                let canonical_candidate_id = canonical["candidate_id"]
                    .as_str()
                    .and_then(|value| value.parse::<u64>().ok())
                    .expect("canonical candidate ID");
                assert!(winners.iter().all(|winner| {
                    winner["candidate_id"]
                        .as_str()
                        .and_then(|value| value.parse::<u64>().ok())
                        .is_some_and(|candidate_id| candidate_id >= canonical_candidate_id)
                }));
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
                assert_eq!(summary["alternative_index"], "1");
                assert!(summary["members"].as_array().is_some_and(|members| !members.is_empty()));
                assert_eq!(value["resource_report"]["probability_complete"], true);
                assert_eq!(value["resource_report"]["count_complete"], true);
                assert_eq!(value["resource_report"]["truncated"], false);
                assert_eq!(value["resource_report"]["renormalized"], false);
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
        assert_eq!(
            score_surface["score_pattern_canonical_selection"],
            "smallest-canonical-candidate-id"
        );
        assert!(score_surface["score_pattern_canonical_winner"].is_object());
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
        assert_eq!(score_surface["alternative_index"], "1");
        assert!(score_surface["members"]
            .as_array()
            .is_some_and(|members| !members.is_empty()));
        assert_eq!(
            score_minimals_json["resource_report"]["count_complete"],
            true
        );
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
    fn actual_cli_pc_minimals_returns_first_canonical_then_lazily_continues_every_tie() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "clearra-cli-lazy-minimum-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("lazy minimum test directory");
        let snapshot = directory.join("iiooo-minimum-portfolios.jsonl");
        let snapshot_text = snapshot.to_string_lossy().into_owned();

        let initial = CliParser::parse([
            "clearra".to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
            "pc".to_owned(),
            "minimals".to_owned(),
            "--lines".to_owned(),
            "2".to_owned(),
            "--queue".to_owned(),
            "IIOOO".to_owned(),
            "--backend".to_owned(),
            "cpu".to_owned(),
            "--workers".to_owned(),
            "1".to_owned(),
            "--ties".to_owned(),
            "--tie-snapshot".to_owned(),
            snapshot_text.clone(),
        ])
        .expect("parse tied IIOOO minimum request");
        let initial = route_invocation(initial);
        assert_eq!(
            initial.exit_code(),
            ExitCode::Success,
            "{}",
            initial.stderr()
        );
        let initial: serde_json::Value =
            serde_json::from_str(initial.stdout()).expect("initial tied minimum JSON");
        let initial_surface = initial.get("summary").unwrap_or(&initial);
        let initial_page = &initial_surface["portfolio_alternative_page"];
        assert_eq!(initial_page["alternative_index"], "1");
        assert_eq!(initial_page["known_alternative_count"], "1");
        assert_eq!(
            initial_page["total_alternative_count"],
            serde_json::Value::Null
        );
        assert_eq!(initial_page["enumeration_complete"], false);
        assert_eq!(candidate_ids(initial_page), vec!["1"]);
        let mut cursor = initial_page["tie_cursor"]
            .as_str()
            .expect("lazy continuation cursor")
            .to_owned();

        let mut pages = vec![vec!["1".to_owned()]];
        loop {
            let continuation = CliParser::parse([
                "clearra".to_owned(),
                "--format".to_owned(),
                "json".to_owned(),
                "continue".to_owned(),
                "--tie-snapshot".to_owned(),
                snapshot_text.clone(),
                "--tie-cursor".to_owned(),
                cursor,
            ])
            .expect("parse minimum continuation");
            let continuation = route_invocation(continuation);
            assert_eq!(
                continuation.exit_code(),
                ExitCode::Success,
                "{}",
                continuation.stderr()
            );
            let continuation: serde_json::Value =
                serde_json::from_str(continuation.stdout()).expect("continued minimum JSON");
            let surface = continuation.get("summary").unwrap_or(&continuation);
            let page = &surface["portfolio_alternative_page"];
            pages.push(candidate_ids(page));
            match page["tie_cursor"].as_str() {
                Some(next) => cursor = next.to_owned(),
                None => {
                    assert_eq!(page["enumeration_complete"], true);
                    assert_eq!(page["total_alternative_count"], "4");
                    break;
                }
            }
        }

        assert_eq!(pages, vec![vec!["1"], vec!["2"], vec!["3"], vec!["4"]]);
        fs::remove_file(&snapshot).expect("remove lazy minimum snapshot");
        fs::remove_dir(&directory).expect("remove lazy minimum directory");
    }

    fn candidate_ids(page: &serde_json::Value) -> Vec<String> {
        page["members"]
            .as_array()
            .expect("portfolio members")
            .iter()
            .map(|member| {
                member["candidate_id"]
                    .as_str()
                    .expect("canonical candidate ID")
                    .to_owned()
            })
            .collect()
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
    fn tablebase_failures_are_publicly_typed_without_starting_offline_work() {
        for (reason, expected) in [
            ("pc4_online_field_miss", "no entry"),
            ("pc4_online_offline", "could not be reached"),
            ("pc4_online_rate_limited", "rate limit"),
            ("pc4_online_timeout", "timed out"),
            (
                "pc4_online_profile_or_target_unavailable",
                "No qualified tablebase",
            ),
            ("private_internal_detail", "could not be completed"),
        ] {
            let message = tablebase_lookup_failure_message(reason, LanguageId::En);
            assert!(message.contains(expected), "{reason}: {message}");
            assert!(message.contains("Offline search was not started"));
            assert!(message.contains("--no-tablebase"));
            assert!(message.contains("Ctrl+C"));
            assert!(!message.contains(reason));
        }
        for language in [LanguageId::Ko, LanguageId::Ja] {
            let message = tablebase_lookup_failure_message("pc4_online_timeout", language);
            assert!(message.contains("Ctrl+C"));
            assert!(message.contains("--tablebase"));
            assert!(!message.contains("pc4_online_"));
        }
    }

    #[test]
    fn explicit_cli_fallback_runs_the_same_request_without_tablebase() {
        let invocation = CliParser::parse(
            "clearra --format json pc --lines 1 --board-mask 0x3f0 --height 1 --pieces 1 \
             --queue I --no-hold --backend cpu --workers 1 --tablebase --offline-fallback"
                .split_whitespace(),
        )
        .expect("explicit offline fallback command");
        let output = route_invocation(invocation);
        assert_eq!(
            output.exit_code(),
            ExitCode::Success,
            "stderr={} stdout={}",
            output.stderr(),
            output.stdout()
        );
        assert!(output.stderr().is_empty());
        assert!(output.warning_before().contains("offline exact search"));
        assert_eq!(output.warning_before(), output.warning_after());
        let value: serde_json::Value =
            serde_json::from_str(output.stdout()).expect("fallback JSON response");
        assert!(value["kind"].is_string());
    }
}
