mod bridge {
    use clearra_app::{AppContext, ProductPageStore};
    #[cfg(feature = "wasm-cpu-runtime")]
    use clearra_app::{AppCoreExecutorService, AppServices};

    use crate::{GuiJobHandle, GuiJobId, GuiJobQueue};

    #[derive(Debug)]
    pub struct DesktopTauriCommandBridge {
        pub(super) app_context: AppContext,
        pub(super) queue: GuiJobQueue,
        pub(super) active_job: Option<GuiJobHandle>,
        pub(super) active_job_id: Option<GuiJobId>,
        pub(super) product_page_store: Option<ProductPageStore>,
    }

    impl DesktopTauriCommandBridge {
        pub fn new(app_context: AppContext) -> Self {
            Self {
                app_context,
                queue: GuiJobQueue::new(),
                active_job: None,
                active_job_id: None,
                product_page_store: None,
            }
        }
    }

    impl Default for DesktopTauriCommandBridge {
        fn default() -> Self {
            Self::new(product_app_context())
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
        AppContext::default()
    }
}
mod cancel_job {
    use crate::GuiJobId;

    use super::{bridge::DesktopTauriCommandBridge, error::DesktopTauriCommandError};

    impl DesktopTauriCommandBridge {
        pub fn cancel_job(&mut self, job_id: u64) -> Result<(), DesktopTauriCommandError> {
            if self.active_job_id.map(GuiJobId::get) != Some(job_id) {
                return Err(DesktopTauriCommandError::job("desktop active job mismatch"));
            }
            self.active_job
                .as_ref()
                .ok_or_else(|| DesktopTauriCommandError::job("desktop job handle missing"))?
                .cancel();
            self.product_page_store = None;
            Ok(())
        }
    }
}
mod error {
    use std::fmt;

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct DesktopTauriCommandError {
        code: &'static str,
        message: String,
    }

    impl DesktopTauriCommandError {
        pub(super) fn invalid_request(message: impl Into<String>) -> Self {
            Self::new("desktop-invalid-request", message)
        }
    }
    impl DesktopTauriCommandError {
        pub(super) fn validation(message: impl Into<String>) -> Self {
            Self::new("desktop-validation-failed", message)
        }
    }
    impl DesktopTauriCommandError {
        pub(super) fn job(message: impl Into<String>) -> Self {
            Self::new("desktop-job-error", message)
        }
    }
    impl DesktopTauriCommandError {
        fn new(code: &'static str, message: impl Into<String>) -> Self {
            Self {
                code,
                message: message.into(),
            }
        }
    }
    impl DesktopTauriCommandError {
        pub const fn code(&self) -> &'static str {
            self.code
        }
    }
    impl DesktopTauriCommandError {
        pub fn message(&self) -> &str {
            &self.message
        }
    }

    impl fmt::Display for DesktopTauriCommandError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "{}: {}", self.code, self.message)
        }
    }

    impl std::error::Error for DesktopTauriCommandError {}
}
mod cli_request_parser {
    use clearra_app::AppRequest;
    use clearra_cli_command::CliCommandParser;
    use clearra_i18n::LanguageId;
    use serde_json::Value;

    use super::error::DesktopTauriCommandError;

    const DESKTOP_CLI_COMMAND_FIELDS: &[&str] =
        &["app_request_model", "command", "language", "arguments"];
    const DESKTOP_CLI_ARGUMENT_TOKEN_LIMIT: usize = 512;
    const DESKTOP_CLI_ARGUMENT_BYTES_LIMIT: usize = 64 << 20;
    const DESKTOP_CLI_SINGLE_ARGUMENT_BYTES_LIMIT: usize = 16 << 20;

    /// Compiles the complete canonical argv emitted by the GUI through the
    /// frontend-neutral CLI grammar. No other desktop request model is a
    /// production ingress path.
    pub(super) fn desktop_request_builds_app_request(
        request_json: &str,
    ) -> Result<AppRequest, DesktopTauriCommandError> {
        let value: Value = serde_json::from_str(request_json)
            .map_err(|error| DesktopTauriCommandError::invalid_request(error.to_string()))?;
        build_cli_command_app_request(&value)
    }

    fn build_cli_command_app_request(
        value: &Value,
    ) -> Result<AppRequest, DesktopTauriCommandError> {
        let object = value.as_object().ok_or_else(|| {
            DesktopTauriCommandError::invalid_request("desktop CLI request must be a JSON object")
        })?;
        if let Some(field) = object
            .keys()
            .find(|field| !DESKTOP_CLI_COMMAND_FIELDS.contains(&field.as_str()))
        {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop CLI request does not accept field '{field}'"
            )));
        }
        if object.get("app_request_model").and_then(Value::as_str)
            != Some("clearra-cli/CommandRequest")
        {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop CLI request requires clearra-cli/CommandRequest",
            ));
        }
        if object.get("command").and_then(Value::as_str) != Some("cli") {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop CLI request command must be 'cli'",
            ));
        }
        let language = object
            .get("language")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(
                    "desktop CLI request requires a string language",
                )
            })
            .and_then(|language| {
                LanguageId::parse(language).ok_or_else(|| {
                    DesktopTauriCommandError::invalid_request(format!(
                        "invalid desktop language '{language}'"
                    ))
                })
            })?;
        let argument_values = object
            .get("arguments")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(
                    "desktop CLI request requires a string arguments array",
                )
            })?;
        if argument_values.len() < 2 || argument_values.len() > DESKTOP_CLI_ARGUMENT_TOKEN_LIMIT {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop CLI arguments must contain between 2 and {DESKTOP_CLI_ARGUMENT_TOKEN_LIMIT} tokens"
            )));
        }
        let mut arguments = Vec::with_capacity(argument_values.len());
        let mut argument_bytes = 0usize;
        for value in argument_values {
            let argument = value.as_str().ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(
                    "desktop CLI arguments must contain only strings",
                )
            })?;
            if argument.contains('\0') || argument.len() > DESKTOP_CLI_SINGLE_ARGUMENT_BYTES_LIMIT {
                return Err(DesktopTauriCommandError::invalid_request(
                    "desktop CLI argument exceeds its size limit or contains NUL",
                ));
            }
            argument_bytes = argument_bytes.checked_add(argument.len()).ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(
                    "desktop CLI argument byte count overflowed",
                )
            })?;
            arguments.push(argument.to_owned());
        }
        if argument_bytes > DESKTOP_CLI_ARGUMENT_BYTES_LIMIT {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop CLI arguments exceed {DESKTOP_CLI_ARGUMENT_BYTES_LIMIT} bytes"
            )));
        }
        if arguments.first().map(String::as_str) != Some("clearra") {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop CLI arguments must begin with the canonical 'clearra' token",
            ));
        }
        let root = arguments.get(1).map(String::as_str).unwrap_or_default();
        if !matches!(
            root,
            "pc" | "failed-queue"
                | "build-probability"
                | "build"
                | "finesse"
                | "setup-finder"
                | "setup"
                | "damage"
                | "spin-finder"
                | "ren"
                | "spin-structure"
                | "utility"
        ) {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop GUI does not expose CLI command root '{root}'"
            )));
        }
        if root == "pc"
            && matches!(
                arguments.get(2).map(String::as_str),
                Some("saves" | "best-save")
            )
        {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop GUI does not expose pc saves or pc best-save",
            ));
        }

        let parsed = CliCommandParser::parse_tokens(&arguments).map_err(|error| {
            DesktopTauriCommandError::invalid_request(format!(
                "invalid canonical desktop CLI arguments: {error}"
            ))
        })?;
        parsed
            .to_app_request()
            .map(|request| request.with_language(language))
            .map_err(|error| {
                DesktopTauriCommandError::validation(format!(
                    "desktop CLI request failed typed lowering: {error}"
                ))
            })
    }

    #[cfg(test)]
    mod tests {
        use clearra_app::{
            AppCommand, PcMinimalsIngressOrigin, PcResultProjection, PcScoreIngressOrigin,
            ProductCapabilityContract,
        };
        use clearra_cli_command::CliCommandParser;
        use clearra_core_domain::objective::objective_kind::ObjectiveKind;
        use clearra_i18n::LanguageId;
        use clearra_pc_graph::request::{PcCountPolicy, RequestedSearchBackend, WorkerPolicy};
        use serde_json::json;

        use super::desktop_request_builds_app_request;

        #[test]
        fn production_desktop_parser_is_identical_to_shared_cli_lowering() {
            let arguments = vec![
                "clearra".to_owned(),
                "pc".to_owned(),
                "tiling".to_owned(),
                "--lines".to_owned(),
                "2".to_owned(),
                "--patterns".to_owned(),
                "P7".to_owned(),
            ];
            let desktop = desktop_request_builds_app_request(
                &json!({
                    "app_request_model": "clearra-cli/CommandRequest",
                    "command": "cli",
                    "language": "ko",
                    "arguments": arguments,
                })
                .to_string(),
            )
            .expect("canonical desktop CLI request");
            let direct = CliCommandParser::parse_tokens(&arguments)
                .expect("shared CLI parser")
                .to_app_request()
                .expect("shared CLI lowering")
                .with_language(LanguageId::Ko);

            assert_eq!(desktop, direct);
        }

        #[test]
        fn production_minimals_web_text_desktop_argv_and_cli_tokens_have_one_typed_authority() {
            let arguments = vec![
                "clearra".to_owned(),
                "pc".to_owned(),
                "minimals".to_owned(),
                "--lines".to_owned(),
                "2".to_owned(),
                "--board-mask".to_owned(),
                "0".to_owned(),
                "--height".to_owned(),
                "2".to_owned(),
                "--pieces".to_owned(),
                "5".to_owned(),
                "--queue".to_owned(),
                "IIOOO".to_owned(),
                "--no-hold".to_owned(),
                "--rule".to_owned(),
                "srs-plus".to_owned(),
                "--backend".to_owned(),
                "cpu".to_owned(),
                "--workers".to_owned(),
                "1".to_owned(),
            ];
            let desktop = desktop_request_builds_app_request(
                &json!({
                    "app_request_model": "clearra-cli/CommandRequest",
                    "command": "cli",
                    "language": "en",
                    "arguments": arguments,
                })
                .to_string(),
            )
            .expect("canonical Desktop minimals argv");
            let cli = CliCommandParser::parse_tokens(&arguments)
                .expect("canonical CLI minimals tokens")
                .to_app_request()
                .expect("typed CLI minimals request")
                .with_language(LanguageId::En);
            let web = CliCommandParser::parse(&arguments.join(" "))
                .expect("canonical Web minimals command text")
                .to_app_request()
                .expect("typed Web minimals request")
                .with_language(LanguageId::En);

            assert_eq!(desktop, cli);
            assert_eq!(web, cli);
            assert_eq!(
                cli.product_capability_contract(),
                Some(ProductCapabilityContract::PcMinimals)
            );
            let AppCommand::Scenario(command) = cli.command() else {
                panic!("field-backed pc minimals must be the shared Scenario command")
            };
            assert_eq!(command.query().count_policy(), PcCountPolicy::CountUnique);
            assert_eq!(
                command.result_projection(),
                PcResultProjection::MinimumCoverV2(PcMinimalsIngressOrigin::CanonicalPcMinimals)
            );

            let extra_dto_authority = json!({
                "app_request_model": "clearra-cli/CommandRequest",
                "command": "cli",
                "language": "en",
                "arguments": arguments,
                "count_policy": "unique",
            });
            assert!(desktop_request_builds_app_request(&extra_dto_authority.to_string()).is_err());
        }

        #[test]
        fn production_pc_score_opening_and_scenario_ingresses_share_all_solution_authority() {
            let cases = [
                vec![
                    "clearra",
                    "pc",
                    "score",
                    "--lines",
                    "2",
                    "--rule",
                    "srs-plus",
                    "--score-profile",
                    "tetrio",
                    "--spin-profile",
                    "t-spins",
                    "--initial-b2b",
                    "0",
                ],
                vec![
                    "clearra",
                    "pc",
                    "score",
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
                    "--score-profile",
                    "tetrio",
                    "--spin-profile",
                    "t-spins",
                    "--initial-b2b",
                    "0",
                ],
            ];

            for arguments in
                cases.map(|arguments| arguments.into_iter().map(str::to_owned).collect::<Vec<_>>())
            {
                let desktop = desktop_request_builds_app_request(
                    &json!({
                        "app_request_model": "clearra-cli/CommandRequest",
                        "command": "cli",
                        "language": "en",
                        "arguments": arguments,
                    })
                    .to_string(),
                )
                .expect("canonical Desktop pc score argv");
                let cli = CliCommandParser::parse_tokens(&arguments)
                    .expect("canonical CLI pc score tokens")
                    .to_app_request()
                    .expect("typed CLI pc score request")
                    .with_language(LanguageId::En);
                let web = CliCommandParser::parse(&arguments.join(" "))
                    .expect("canonical Web pc score text")
                    .to_app_request()
                    .expect("typed Web pc score request")
                    .with_language(LanguageId::En);

                assert_eq!(desktop, cli);
                assert_eq!(web, cli);
                assert_eq!(
                    cli.product_capability_contract(),
                    Some(ProductCapabilityContract::PcScore)
                );
                match cli.command() {
                    AppCommand::Pc(command) => {
                        assert_eq!(
                            command.result_projection(),
                            PcResultProjection::ScoreSummaryV2(
                                PcScoreIngressOrigin::CanonicalPcScore
                            )
                        );
                        assert_eq!(command.query().objective().kind(), ObjectiveKind::All);
                        assert_eq!(command.query().count_policy(), PcCountPolicy::CountAll);
                        command
                            .validate_result_projection()
                            .expect("opening pc score projection");
                    }
                    AppCommand::Scenario(command) => {
                        assert_eq!(
                            command.result_projection(),
                            PcResultProjection::ScoreSummaryV2(
                                PcScoreIngressOrigin::CanonicalPcScore
                            )
                        );
                        assert_eq!(command.query().objective().kind(), ObjectiveKind::All);
                        assert_eq!(command.query().count_policy(), PcCountPolicy::CountAll);
                        command
                            .validate_result_projection()
                            .expect("scenario pc score projection");
                    }
                    other => panic!("unexpected pc score command: {other:?}"),
                }
            }
        }

        #[test]
        fn production_desktop_pc_score_argv_preserves_cpu_worker_options() {
            let arguments = [
                "clearra",
                "pc",
                "score",
                "--lines",
                "2",
                "--patterns",
                "[TIOSZ]!",
                "--workers",
                "2",
                "--use-all-cpu-threads",
                "--cpu-warmup",
            ]
            .map(str::to_owned)
            .to_vec();
            let request = desktop_request_builds_app_request(
                &json!({
                    "app_request_model": "clearra-cli/CommandRequest",
                    "command": "cli",
                    "language": "en",
                    "arguments": arguments,
                })
                .to_string(),
            )
            .expect("Desktop pc score worker argv");
            let policy = match request.command() {
                AppCommand::Pc(command) => command.query().execution_policy(),
                AppCommand::Scenario(command) => command.query().execution_policy(),
                command => panic!("expected PC score command, got {command:?}"),
            };
            assert_eq!(policy.requested_backend(), RequestedSearchBackend::Cpu);
            assert_eq!(policy.worker_policy(), WorkerPolicy::Fixed(2));
            assert!(policy.use_all_logical_processors());
            assert!(policy.cpu_warmup());
            assert!(!policy.allow_backend_fallback());
        }

        #[test]
        fn generic_pc_score_flag_does_not_gain_the_named_all_solution_product_contract() {
            let request = CliCommandParser::parse(
                "clearra pc --lines 2 --score --score-profile tetrio --spin-profile t-spins",
            )
            .expect("generic score-aware PC command")
            .to_app_request()
            .expect("generic score-aware PC request");

            assert_eq!(request.product_capability_contract(), None);
            let AppCommand::Pc(command) = request.command() else {
                panic!("expected opening PC command");
            };
            assert_eq!(command.result_projection(), PcResultProjection::Standard);
            assert_eq!(command.query().objective().kind(), ObjectiveKind::Unique);
            assert_eq!(command.query().count_policy(), PcCountPolicy::CountAll);
        }

        #[cfg(feature = "wasm-cpu-runtime")]
        #[test]
        fn production_minimals_ingresses_execute_the_same_complete_iiooo_portfolio() {
            use clearra_app::{AppContext, AppCoreExecutorService, AppServices, AppStatus};

            let arguments = vec![
                "clearra",
                "pc",
                "minimals",
                "--lines",
                "2",
                "--board-mask",
                "0",
                "--height",
                "2",
                "--pieces",
                "5",
                "--queue",
                "IIOOO",
                "--no-hold",
                "--rule",
                "srs-plus",
                "--backend",
                "cpu",
                "--workers",
                "1",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
            let desktop = desktop_request_builds_app_request(
                &json!({
                    "app_request_model": "clearra-cli/CommandRequest",
                    "command": "cli",
                    "language": "en",
                    "arguments": arguments,
                })
                .to_string(),
            )
            .expect("canonical Desktop minimals argv");
            let cli = CliCommandParser::parse_tokens(&arguments)
                .expect("canonical CLI minimals tokens")
                .to_app_request()
                .expect("typed CLI minimals request")
                .with_language(LanguageId::En);
            let web = CliCommandParser::parse(&arguments.join(" "))
                .expect("canonical Web minimals text")
                .to_app_request()
                .expect("typed Web minimals request")
                .with_language(LanguageId::En);
            let context = AppContext::new(
                AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
            );
            let responses = [context.run(web), context.run(desktop), context.run(cli)];
            assert!(responses
                .iter()
                .all(|response| response.status() == AppStatus::Success));
            let sets = responses
                .iter()
                .map(|response| {
                    response
                        .product_capability_result()
                        .and_then(|result| result.pc_minimum_cover_v2())
                        .expect("complete typed minimum-cover result")
                        .portfolio_alternatives()
                })
                .collect::<Vec<_>>();
            assert_eq!(sets[0], sets[1]);
            assert_eq!(sets[1], sets[2]);
            assert_eq!(sets[0].candidates().len(), 4);
            assert_eq!(sets[0].coverage_rows().len(), 4);
            assert_eq!(sets[0].optimal_cardinality(), 1);

            for set in sets {
                let mut store = set.open_store().expect("exact alternative store");
                let mut portfolios =
                    vec![set.canonical_page().portfolio().candidate_ids().to_vec()];
                loop {
                    let advance = store
                        .next_page(u64::MAX, &mut || false)
                        .expect("next exact alternative");
                    if let Some(page) = advance.page() {
                        portfolios.push(page.portfolio().candidate_ids().to_vec());
                    }
                    if advance.checkpoint().enumeration_complete() {
                        assert_eq!(advance.checkpoint().known_alternative_count_decimal(), "4");
                        break;
                    }
                }
                assert_eq!(portfolios, [vec![1], vec![2], vec![3], vec![4]]);
            }
        }

        #[test]
        fn production_desktop_parser_rejects_legacy_partial_and_extra_envelopes() {
            for request in [
                json!({
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "pc",
                    "lines": 2,
                }),
                json!({
                    "app_request_model": "clearra-cli/CommandRequest",
                    "command": "cli",
                    "language": "en",
                }),
                json!({
                    "app_request_model": "clearra-cli/CommandRequest",
                    "command": "cli",
                    "language": "en",
                    "arguments": ["clearra", "pc", "tiling", "--lines", "2"],
                    "lines": 2,
                }),
                json!({
                    "app_request_model": "clearra-cli/CommandRequest",
                    "command": "cli",
                    "language": "en",
                    "arguments": ["clearra", "pc", "minimals", "--lines", "2"],
                    "count_policy": "unique",
                }),
            ] {
                assert!(desktop_request_builds_app_request(&request.to_string()).is_err());
            }
        }
    }
}

#[cfg(not(test))]
mod active_request_parser {
    pub(super) use super::cli_request_parser::desktop_request_builds_app_request;
}

#[cfg(test)]
mod active_request_parser {
    // The legacy form compiler survives only to preserve its historical unit
    // coverage. Product builds cannot name or reach this module.
    pub(super) use super::legacy_form_parser::desktop_request_builds_app_request;
}

#[cfg(test)]
mod legacy_form_parser {
    use clearra_app::{
        AppCommand, AppRequest, BuildObjective, BuildQueueKnowledge, BuildScoreProfile,
        FieldDocumentFormat, FieldDocumentTransformAppCommand, FieldDocumentTransformKind,
        FumenAppCommand, FumenTransformKind, ParityAppCommand, RenderAppCommand,
        RenderArtifactFormat, RequestStructuralProfiles,
    };
    use clearra_cli_command::{
        operation_sequence_request_from_document, sequence_dependencies_request_from_document,
        CliCommandParser, CliCommandRequest, WebBuildProbabilityInput, WebBuildV2Capability,
        WebBuildV2Input, WebSetupScoreInput, WebSetupScoreQueueInput,
    };
    use clearra_core_domain::{
        board::standard_pc_board::Board256Mask, objective::objective_kind::ObjectiveKind,
        piece::piece_kind::PieceKind,
    };
    use clearra_forward_search::{
        ForwardLineClearPolicy, ForwardPieceSource, ForwardSearchMode, ForwardSearchQuery,
        ForwardSpinCategory, ForwardSpinLineRequirement, ForwardSpinTarget,
    };
    use clearra_i18n::LanguageId;
    use clearra_objectives::policy::{
        objective_policy::ObjectivePolicy,
        score_objective_policy::{ScoreProfileSelection, SpinProfileSelection},
    };
    use clearra_pc_graph::request::{
        validate_pc_observation_objective, GpuDeviceSelection, RequestedSearchBackend, WorkerPolicy,
    };
    use clearra_problem::{
        BuildProbabilityAggregation, FinesseMetric, FinessePatternKnowledge,
        SetupCandidatePriority, SetupLengthPreference, SetupPathDetail, SetupSearchMode,
    };
    use clearra_scoring::profile::SpinProfileId;
    use clearra_supply::queue::queue_observation_policy::QueueObservationPolicy;
    use serde_json::Value;

    use crate::{
        request::{
            parse_piece_sequence, parse_queue_pattern, parse_rule_profile,
            score_mode_objective_kind,
        },
        GuiAppState, GuiBackendForm, GuiOpeningPcForm, GuiProblemForm, GuiToAppRequest,
    };

    use super::error::DesktopTauriCommandError;

    pub(super) fn desktop_request_builds_app_request(
        request_json: &str,
    ) -> Result<AppRequest, DesktopTauriCommandError> {
        let value: Value = serde_json::from_str(request_json)
            .map_err(|error| DesktopTauriCommandError::invalid_request(error.to_string()))?;
        if value.get("app_request_model").and_then(Value::as_str)
            == Some("clearra-cli/CommandRequest")
        {
            return build_cli_command_app_request(&value);
        }
        validate_app_request_envelope(&value)?;
        let request_structural_profiles = parse_request_structural_profiles(&value)?;
        let command = text_or_default(&value, &["command"], "pc")?;
        let request = match command {
            "pc" | "pc-scenario" => {
                let state = desktop_form_builds_app_request(request_json)?;
                GuiToAppRequest::build(&state)
                    .map_err(|error| DesktopTauriCommandError::validation(error.to_string()))?
                    .into_app_request()
            }
            "setup" => build_setup_app_request(&value)?,
            "setup-score" => build_setup_score_app_request(&value)?,
            "build-probability" => build_probability_app_request(&value)?,
            "build-v2" => build_v2_app_request(&value)?,
            "spin-structure" => build_spin_structure_app_request(&value)?,
            "damage" | "spin-finder" | "ren" => build_forward_app_request(&value, command)?,
            "utility-sequence" => build_operation_sequence_app_request(&value)?,
            "utility-sequence-dependencies" => build_sequence_dependencies_app_request(&value)?,
            "utility-parity" => build_parity_app_request(&value)?,
            "utility-fumen" => build_fumen_app_request(&value)?,
            "utility-render" => build_render_app_request(&value)?,
            "utility-to-gray" | "utility-mirror" => {
                build_field_document_transform_app_request(&value, command)?
            }
            _ => unreachable!("desktop command allowlist validated before dispatch"),
        };
        let language = optional_text(&value, &["language"])?
            .map(|language| {
                LanguageId::parse(language).ok_or_else(|| {
                    DesktopTauriCommandError::invalid_request(format!(
                        "invalid desktop language '{language}'"
                    ))
                })
            })
            .transpose()?
            .unwrap_or(LanguageId::En);
        request
            .with_language(language)
            .with_request_structural_profiles(request_structural_profiles)
            .map_err(|error| {
                DesktopTauriCommandError::invalid_request(format!(
                    "invalid desktop request profile selection: {error}"
                ))
            })
    }

    const DESKTOP_CLI_COMMAND_FIELDS: &[&str] =
        &["app_request_model", "command", "language", "arguments"];
    const DESKTOP_CLI_ARGUMENT_TOKEN_LIMIT: usize = 512;
    const DESKTOP_CLI_ARGUMENT_BYTES_LIMIT: usize = 64 << 20;
    const DESKTOP_CLI_SINGLE_ARGUMENT_BYTES_LIMIT: usize = 16 << 20;

    /// Compiles the exact canonical argv emitted by the GUI through the same
    /// frontend-neutral grammar used by native CLI and browser WASM. This
    /// transport owns only a closed JSON envelope; it must not reconstruct
    /// product defaults or split an escaped command string.
    fn build_cli_command_app_request(
        value: &Value,
    ) -> Result<AppRequest, DesktopTauriCommandError> {
        let object = value.as_object().ok_or_else(|| {
            DesktopTauriCommandError::invalid_request("desktop CLI request must be a JSON object")
        })?;
        if let Some(field) = object
            .keys()
            .find(|field| !DESKTOP_CLI_COMMAND_FIELDS.contains(&field.as_str()))
        {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop CLI request does not accept field '{field}'"
            )));
        }
        if object.get("app_request_model").and_then(Value::as_str)
            != Some("clearra-cli/CommandRequest")
        {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop CLI request requires clearra-cli/CommandRequest",
            ));
        }
        if object.get("command").and_then(Value::as_str) != Some("cli") {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop CLI request command must be 'cli'",
            ));
        }
        let language = object
            .get("language")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(
                    "desktop CLI request requires a string language",
                )
            })
            .and_then(|language| {
                LanguageId::parse(language).ok_or_else(|| {
                    DesktopTauriCommandError::invalid_request(format!(
                        "invalid desktop language '{language}'"
                    ))
                })
            })?;
        let argument_values = object
            .get("arguments")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(
                    "desktop CLI request requires a string arguments array",
                )
            })?;
        if argument_values.len() < 2 || argument_values.len() > DESKTOP_CLI_ARGUMENT_TOKEN_LIMIT {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop CLI arguments must contain between 2 and {DESKTOP_CLI_ARGUMENT_TOKEN_LIMIT} tokens"
            )));
        }
        let mut arguments = Vec::with_capacity(argument_values.len());
        let mut argument_bytes = 0usize;
        for value in argument_values {
            let argument = value.as_str().ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(
                    "desktop CLI arguments must contain only strings",
                )
            })?;
            if argument.contains('\0') || argument.len() > DESKTOP_CLI_SINGLE_ARGUMENT_BYTES_LIMIT {
                return Err(DesktopTauriCommandError::invalid_request(
                    "desktop CLI argument is empty-safe text but exceeds its size limit or contains NUL",
                ));
            }
            argument_bytes = argument_bytes.checked_add(argument.len()).ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(
                    "desktop CLI argument byte count overflowed",
                )
            })?;
            arguments.push(argument.to_owned());
        }
        if argument_bytes > DESKTOP_CLI_ARGUMENT_BYTES_LIMIT {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop CLI arguments exceed {DESKTOP_CLI_ARGUMENT_BYTES_LIMIT} bytes"
            )));
        }
        if arguments.first().map(String::as_str) != Some("clearra") {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop CLI arguments must begin with the canonical 'clearra' token",
            ));
        }
        let root = arguments.get(1).map(String::as_str).unwrap_or_default();
        if !matches!(
            root,
            "pc" | "failed-queue"
                | "build-probability"
                | "build"
                | "finesse"
                | "setup-finder"
                | "setup"
                | "damage"
                | "spin-finder"
                | "ren"
                | "spin-structure"
                | "utility"
        ) {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop GUI does not expose CLI command root '{root}'"
            )));
        }
        if root == "pc"
            && matches!(
                arguments.get(2).map(String::as_str),
                Some("saves" | "best-save")
            )
        {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop GUI does not expose pc saves or pc best-save",
            ));
        }

        let parsed = CliCommandParser::parse_tokens(&arguments).map_err(|error| {
            DesktopTauriCommandError::invalid_request(format!(
                "invalid canonical desktop CLI arguments: {error}"
            ))
        })?;
        parsed
            .to_app_request()
            .map(|request| request.with_language(language))
            .map_err(|error| {
                DesktopTauriCommandError::validation(format!(
                    "desktop CLI request failed typed lowering: {error}"
                ))
            })
    }

    const DESKTOP_SETUP_SCORE_FIELDS: &[&str] = &[
        "app_request_model",
        "command",
        "language",
        "document_format",
        "document",
        "setup_queue",
        "setup_patterns",
        "solution_queue",
        "solution_patterns",
        "clear_height",
        "hold_enabled",
        "score_profile",
        "initial_b2b",
        "rule",
        "max_patterns",
        "workers",
        "use_all_logical_processors",
        "backend",
        "allow_backend_fallback",
    ];

    const DESKTOP_SPIN_STRUCTURE_FIELDS: &[&str] = &[
        "app_request_model",
        "command",
        "language",
        "capability_id",
        "board_mask_v1",
        "visible_height",
        "inventory",
        "spin_profile",
        "lines",
        "fill_bottom",
        "fill_top",
        "rule",
        "max_placements",
        "minimality",
        "objective",
        "max_patterns",
        "final_piece",
        "dependency_report",
        "workers",
        "use_all_logical_processors",
        "backend",
        "allow_backend_fallback",
    ];

    fn build_spin_structure_app_request(
        value: &Value,
    ) -> Result<AppRequest, DesktopTauriCommandError> {
        let object = value.as_object().ok_or_else(|| {
            DesktopTauriCommandError::invalid_request("desktop request must be a JSON object")
        })?;
        if let Some(field) = object.keys().find(|field| {
            field.as_str() != "profiles" && !DESKTOP_SPIN_STRUCTURE_FIELDS.contains(&field.as_str())
        }) {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop spin-structure does not accept field '{field}'"
            )));
        }
        let required_string = |key: &'static str| {
            value
                .get(key)
                .ok_or_else(|| {
                    DesktopTauriCommandError::invalid_request(format!(
                        "desktop spin-structure requires {key}"
                    ))
                })?
                .as_str()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    DesktopTauriCommandError::invalid_request(format!(
                        "desktop spin-structure {key} must be a nonempty string"
                    ))
                })
        };
        if required_string("app_request_model")? != "clearra-app/AppRequest"
            || required_string("command")? != "spin-structure"
        {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop spin-structure requires the clearra-app/AppRequest model and spin-structure command",
            ));
        }
        if required_string("backend")? != "cpu" {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop spin-structure is CPU-only",
            ));
        }
        if optional_bool(value, &["allow_backend_fallback"])?.ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(
                "desktop spin-structure requires boolean allow_backend_fallback",
            )
        })? {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop spin-structure requires allow_backend_fallback=false",
            ));
        }

        let capability_id = required_string("capability_id")?;
        let route = match capability_id {
            "spin-structure.search" => "search",
            "spin-structure.cover" => "cover",
            "spin-structure.guaranteed" => "guaranteed",
            _ => {
                return Err(DesktopTauriCommandError::invalid_request(format!(
                    "unsupported desktop spin-structure capability_id '{capability_id}'"
                )))
            }
        };
        let required_u8 = |key: &'static str| {
            optional_u8_any(value, &[key])?.ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(format!(
                    "desktop spin-structure requires integer {key}"
                ))
            })
        };
        let required_usize = |key: &'static str| {
            optional_usize_any(value, &[key])?.ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(format!(
                    "desktop spin-structure requires integer {key}"
                ))
            })
        };
        let height = required_u8("visible_height")?;
        let fill_bottom = required_u8("fill_bottom")?;
        let fill_top = required_u8("fill_top")?;
        let max_placements = required_u8("max_placements")?;
        let workers = required_usize("workers")?;
        let use_all = optional_bool(value, &["use_all_logical_processors"])?.ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(
                "desktop spin-structure requires boolean use_all_logical_processors",
            )
        })?;
        if workers == 0 {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop spin-structure workers must be positive",
            ));
        }

        let mut tokens = vec![
            "clearra".to_owned(),
            "spin-structure".to_owned(),
            route.to_owned(),
            "--board-mask-v1".to_owned(),
            required_string("board_mask_v1")?.to_owned(),
            "--height".to_owned(),
            height.to_string(),
            "--pieces".to_owned(),
            required_string("inventory")?.to_owned(),
            "--spin-profile".to_owned(),
            required_string("spin_profile")?.to_owned(),
            "--lines".to_owned(),
            required_string("lines")?.to_owned(),
            "--fill-bottom".to_owned(),
            fill_bottom.to_string(),
            "--fill-top".to_owned(),
            fill_top.to_string(),
            "--rule".to_owned(),
            required_string("rule")?.to_owned(),
            "--max-placements".to_owned(),
            max_placements.to_string(),
            "--minimality".to_owned(),
            required_string("minimality")?.to_owned(),
        ];
        if use_all {
            tokens.push("--use-all-logical-processors".to_owned());
        } else {
            tokens.push("--workers".to_owned());
            tokens.push(workers.to_string());
        }

        match route {
            "search" => reject_spin_structure_fields(
                value,
                capability_id,
                &[
                    "objective",
                    "max_patterns",
                    "final_piece",
                    "dependency_report",
                ],
            )?,
            "cover" => {
                reject_spin_structure_fields(
                    value,
                    capability_id,
                    &["final_piece", "dependency_report"],
                )?;
                if required_string("objective")? != "min-cover" {
                    return Err(DesktopTauriCommandError::invalid_request(
                        "desktop spin-structure.cover objective must be min-cover",
                    ));
                }
                tokens.extend([
                    "--objective".to_owned(),
                    "min-cover".to_owned(),
                    "--max-patterns".to_owned(),
                    required_usize("max_patterns")?.to_string(),
                ]);
            }
            "guaranteed" => {
                reject_spin_structure_fields(value, capability_id, &["objective"])?;
                tokens.extend([
                    "--final-piece".to_owned(),
                    required_string("final_piece")?.to_owned(),
                    "--max-patterns".to_owned(),
                    required_usize("max_patterns")?.to_string(),
                ]);
                let dependency_report =
                    optional_bool(value, &["dependency_report"])?.ok_or_else(|| {
                        DesktopTauriCommandError::invalid_request(
                            "desktop spin-structure.guaranteed requires boolean dependency_report",
                        )
                    })?;
                tokens.push(
                    if dependency_report {
                        "--dependency-report"
                    } else {
                        "--no-dependency-report"
                    }
                    .to_owned(),
                );
            }
            _ => unreachable!("closed spin-structure route"),
        }

        CliCommandParser::parse_tokens_with_worker_limit(
            &tokens,
            WorkerPolicy::hardware_worker_limit(),
        )
        .and_then(|request| request.to_app_request())
        .map_err(|error| {
            DesktopTauriCommandError::invalid_request(format!(
                "invalid desktop spin-structure request: {error}"
            ))
        })
    }

    fn reject_spin_structure_fields(
        value: &Value,
        capability_id: &str,
        fields: &[&str],
    ) -> Result<(), DesktopTauriCommandError> {
        if let Some(field) = fields.iter().find(|field| value.get(**field).is_some()) {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop {capability_id} does not accept field '{field}'"
            )));
        }
        Ok(())
    }

    fn build_setup_score_app_request(
        value: &Value,
    ) -> Result<AppRequest, DesktopTauriCommandError> {
        let object = value.as_object().ok_or_else(|| {
            DesktopTauriCommandError::invalid_request("desktop request must be a JSON object")
        })?;
        if let Some(field) = object.keys().find(|field| {
            field.as_str() != "profiles" && !DESKTOP_SETUP_SCORE_FIELDS.contains(&field.as_str())
        }) {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop setup-score does not accept field '{field}'"
            )));
        }
        let required_string = |key: &'static str| {
            value
                .get(key)
                .ok_or_else(|| {
                    DesktopTauriCommandError::invalid_request(format!(
                        "desktop setup-score requires {key}"
                    ))
                })?
                .as_str()
                .ok_or_else(|| {
                    DesktopTauriCommandError::invalid_request(format!(
                        "desktop setup-score {key} must be a string"
                    ))
                })
        };
        if required_string("app_request_model")? != "clearra-app/AppRequest"
            || required_string("command")? != "setup-score"
        {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop setup-score requires the clearra-app/AppRequest model and setup-score command",
            ));
        }
        if required_string("backend")? != "cpu" {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop setup-score is CPU-only",
            ));
        }
        if optional_bool(value, &["allow_backend_fallback"])?.ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(
                "desktop setup-score requires boolean allow_backend_fallback",
            )
        })? {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop setup-score requires allow_backend_fallback=false",
            ));
        }
        let format = match required_string("document_format")? {
            "ctk3" => FieldDocumentFormat::Ctk3,
            "fumen" => FieldDocumentFormat::Fumen,
            format => {
                return Err(DesktopTauriCommandError::invalid_request(format!(
                    "invalid desktop setup-score document_format '{format}'"
                )));
            }
        };
        let document = required_string("document")?;
        if document.is_empty() {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop setup-score document must not be empty",
            ));
        }
        let queue_source = |queue_key: &'static str,
                            pattern_key: &'static str|
         -> Result<WebSetupScoreQueueInput, DesktopTauriCommandError> {
            let queue = required_string(queue_key)?;
            let patterns = required_string(pattern_key)?;
            match (queue.is_empty(), patterns.is_empty()) {
                (false, true) => Ok(WebSetupScoreQueueInput::queue(queue)),
                (true, false) => Ok(WebSetupScoreQueueInput::patterns(patterns)),
                _ => Err(DesktopTauriCommandError::invalid_request(format!(
                    "desktop setup-score requires exactly one nonempty {queue_key} or {pattern_key}"
                ))),
            }
        };
        let setup_source = queue_source("setup_queue", "setup_patterns")?;
        let solution_source = queue_source("solution_queue", "solution_patterns")?;
        let clear_height = optional_u8_any(value, &["clear_height"])?.ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(
                "desktop setup-score requires integer clear_height",
            )
        })?;
        if !(1..=6).contains(&clear_height) {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop setup-score clear_height must be between 1 and 6",
            ));
        }
        let hold_enabled = optional_bool(value, &["hold_enabled"])?.ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(
                "desktop setup-score requires boolean hold_enabled",
            )
        })?;
        let score_profile = ScoreProfileSelection::parse(required_string("score_profile")?)
            .ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(
                    "desktop setup-score score_profile must be tetrio, guideline, or jstris-ultra",
                )
            })?;
        let initial_b2b = optional_usize_any(value, &["initial_b2b"])?.ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(
                "desktop setup-score requires integer initial_b2b",
            )
        })?;
        let initial_b2b = u32::try_from(initial_b2b).map_err(|_| {
            DesktopTauriCommandError::invalid_request(
                "desktop setup-score initial_b2b exceeds the supported range",
            )
        })?;
        let max_patterns = optional_usize_any(value, &["max_patterns"])?.ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(
                "desktop setup-score requires integer max_patterns",
            )
        })?;
        if max_patterns == 0 {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop setup-score max_patterns must be positive",
            ));
        }
        let workers = optional_usize_any(value, &["workers"])?.ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(
                "desktop setup-score requires integer workers",
            )
        })?;
        let use_all_logical_processors = optional_bool(value, &["use_all_logical_processors"])?
            .ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(
                    "desktop setup-score requires boolean use_all_logical_processors",
                )
            })?;
        if workers > 0 && use_all_logical_processors {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop setup-score workers and use_all_logical_processors are mutually exclusive",
            ));
        }

        let input = WebSetupScoreInput::new(format, document, setup_source, solution_source)
            .with_clear_height(clear_height)
            .with_setup_hold_enabled(hold_enabled)
            .with_score_profile(score_profile)
            .with_initial_b2b(initial_b2b);
        let rule = parse_rule_profile(required_string("rule")?)
            .map_err(|error| DesktopTauriCommandError::invalid_request(error.to_string()))?;
        let mut request = CliCommandRequest::setup_score(input)
            .with_rule(rule)
            .with_hold_enabled(hold_enabled)
            .with_backend(RequestedSearchBackend::Cpu)
            .with_allow_backend_fallback(false)
            .with_worker_hardware_limit(WorkerPolicy::hardware_worker_limit())
            .with_max_patterns(max_patterns)
            .with_use_all_logical_processors(use_all_logical_processors);
        if workers > 0 {
            request = request.with_workers(workers);
        }
        typed_cli_request_to_app_request("setup-score", request)
    }

    fn build_parity_app_request(value: &Value) -> Result<AppRequest, DesktopTauriCommandError> {
        validate_utility_fields(
            value,
            "utility-parity",
            &[
                "app_request_model",
                "command",
                "language",
                "format",
                "document",
            ],
        )?;
        let format = parse_field_document_format(value, "utility-parity")?;
        let document = required_text(value, &["document"], "document")?;
        let command = ParityAppCommand::new(format, document.to_owned()).map_err(|error| {
            DesktopTauriCommandError::validation(format!(
                "invalid desktop utility-parity request: {error}"
            ))
        })?;
        Ok(AppRequest::new(AppCommand::UtilityParity(command)))
    }

    fn build_fumen_app_request(value: &Value) -> Result<AppRequest, DesktopTauriCommandError> {
        validate_utility_fields(
            value,
            "utility-fumen",
            &[
                "app_request_model",
                "command",
                "language",
                "format",
                "transform",
                "documents",
                "page_number",
                "page_shift",
                "comments",
            ],
        )?;
        let format = parse_field_document_format(value, "utility-fumen")?;
        let transform =
            FumenTransformKind::parse(required_text(value, &["transform"], "transform")?).map_err(
                |error| {
                    DesktopTauriCommandError::validation(format!(
                        "invalid desktop utility-fumen transform: {error}"
                    ))
                },
            )?;
        let documents = optional_string_array(value, "documents")?.unwrap_or_default();
        let comments = optional_string_array(value, "comments")?.unwrap_or_default();
        let page_number = optional_usize_any(value, &["page_number"])?;
        let page_shift = optional_isize(value, "page_shift")?;
        let command = FumenAppCommand::new(
            format,
            transform,
            documents,
            page_number,
            page_shift,
            comments,
        )
        .map_err(|error| {
            DesktopTauriCommandError::validation(format!(
                "invalid desktop utility-fumen request: {error}"
            ))
        })?;
        Ok(AppRequest::new(AppCommand::UtilityFumen(command)))
    }

    fn build_render_app_request(value: &Value) -> Result<AppRequest, DesktopTauriCommandError> {
        validate_utility_fields(
            value,
            "utility-render",
            &[
                "app_request_model",
                "command",
                "language",
                "format",
                "document",
                "artifact_format",
                "page_number",
            ],
        )?;
        let format = parse_field_document_format(value, "utility-render")?;
        let document = required_text(value, &["document"], "document")?;
        let artifact_format = RenderArtifactFormat::parse(required_text(
            value,
            &["artifact_format"],
            "artifact_format",
        )?)
        .map_err(|error| {
            DesktopTauriCommandError::validation(format!(
                "invalid desktop utility-render artifact format: {error}"
            ))
        })?;
        let page_number = optional_usize_any(value, &["page_number"])?;
        let command =
            RenderAppCommand::new(format, document.to_owned(), artifact_format, page_number)
                .map_err(|error| {
                    DesktopTauriCommandError::validation(format!(
                        "invalid desktop utility-render request: {error}"
                    ))
                })?;
        Ok(AppRequest::new(AppCommand::UtilityRender(command)))
    }

    fn build_field_document_transform_app_request(
        value: &Value,
        command_name: &str,
    ) -> Result<AppRequest, DesktopTauriCommandError> {
        validate_utility_fields(
            value,
            command_name,
            &[
                "app_request_model",
                "command",
                "language",
                "format",
                "document",
            ],
        )?;
        let format = parse_field_document_format(value, command_name)?;
        let document = required_text(value, &["document"], "document")?;
        let transform = match command_name {
            "utility-to-gray" => FieldDocumentTransformKind::ToGray,
            "utility-mirror" => FieldDocumentTransformKind::Mirror,
            _ => unreachable!("closed desktop field-document transform"),
        };
        let command = FieldDocumentTransformAppCommand::new(transform, format, document.to_owned())
            .map_err(|error| {
                DesktopTauriCommandError::validation(format!(
                    "invalid desktop {command_name} request: {error}"
                ))
            })?;
        Ok(AppRequest::new(match transform {
            FieldDocumentTransformKind::ToGray => AppCommand::UtilityToGray(command),
            FieldDocumentTransformKind::Mirror => AppCommand::UtilityMirror(command),
        }))
    }

    fn validate_utility_fields(
        value: &Value,
        command: &str,
        allowed_fields: &[&str],
    ) -> Result<(), DesktopTauriCommandError> {
        let object = value.as_object().ok_or_else(|| {
            DesktopTauriCommandError::invalid_request("desktop request must be a JSON object")
        })?;
        if let Some(field) = object
            .keys()
            .find(|key| key.as_str() != "profiles" && !allowed_fields.contains(&key.as_str()))
        {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop {command} does not accept field '{field}'"
            )));
        }
        Ok(())
    }

    fn parse_field_document_format(
        value: &Value,
        command: &str,
    ) -> Result<FieldDocumentFormat, DesktopTauriCommandError> {
        FieldDocumentFormat::parse(required_text(value, &["format"], "format")?).map_err(|error| {
            DesktopTauriCommandError::validation(format!(
                "invalid desktop {command} document format: {error}"
            ))
        })
    }

    fn optional_string_array(
        value: &Value,
        key: &str,
    ) -> Result<Option<Vec<String>>, DesktopTauriCommandError> {
        let Some(entry) = value.get(key) else {
            return Ok(None);
        };
        let values = entry.as_array().ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(format!(
                "desktop {key} must be an array of strings"
            ))
        })?;
        let mut result = Vec::new();
        result.try_reserve_exact(values.len()).map_err(|_| {
            DesktopTauriCommandError::invalid_request(format!(
                "desktop {key} array exceeds available capacity"
            ))
        })?;
        for item in values {
            let text = item.as_str().ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(format!(
                    "desktop {key} must be an array of strings"
                ))
            })?;
            result.push(text.to_owned());
        }
        Ok(Some(result))
    }

    fn optional_isize(value: &Value, key: &str) -> Result<Option<isize>, DesktopTauriCommandError> {
        let Some(entry) = value.get(key) else {
            return Ok(None);
        };
        entry
            .as_i64()
            .and_then(|number| isize::try_from(number).ok())
            .map(Some)
            .ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(format!(
                    "desktop {key} must be a signed integer that fits isize"
                ))
            })
    }

    fn build_sequence_dependencies_app_request(
        value: &Value,
    ) -> Result<AppRequest, DesktopTauriCommandError> {
        const ALLOWED_FIELDS: &[&str] = &[
            "app_request_model",
            "command",
            "language",
            "document",
            "rule_profile",
            "kick_profile",
            "timeout_seconds",
        ];
        let object = value.as_object().ok_or_else(|| {
            DesktopTauriCommandError::invalid_request("desktop request must be a JSON object")
        })?;
        if let Some(field) = object
            .keys()
            .find(|key| key.as_str() != "profiles" && !ALLOWED_FIELDS.contains(&key.as_str()))
        {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop utility-sequence-dependencies does not accept field '{field}'"
            )));
        }
        let document = required_text(value, &["document"], "document")?;
        let rule_profile = optional_text(value, &["rule_profile"])?;
        let kick_profile = optional_text(value, &["kick_profile"])?;
        let timeout_seconds = optional_u16(value, "timeout_seconds")?;
        sequence_dependencies_request_from_document(
            document,
            rule_profile,
            kick_profile,
            timeout_seconds,
        )
        .and_then(|request| request.to_app_request())
        .map_err(|error| DesktopTauriCommandError::validation(error.to_string()))
    }

    fn build_operation_sequence_app_request(
        value: &Value,
    ) -> Result<AppRequest, DesktopTauriCommandError> {
        const ALLOWED_FIELDS: &[&str] = &[
            "app_request_model",
            "command",
            "language",
            "document",
            "rule_profile",
            "kick_profile",
            "timeout_seconds",
        ];
        let object = value.as_object().ok_or_else(|| {
            DesktopTauriCommandError::invalid_request("desktop request must be a JSON object")
        })?;
        if let Some(field) = object
            .keys()
            .find(|key| key.as_str() != "profiles" && !ALLOWED_FIELDS.contains(&key.as_str()))
        {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop utility-sequence does not accept field '{field}'"
            )));
        }
        let document = required_text(value, &["document"], "document")?;
        let rule_profile = optional_text(value, &["rule_profile"])?;
        let kick_profile = optional_text(value, &["kick_profile"])?;
        let timeout_seconds = optional_u16(value, "timeout_seconds")?;
        operation_sequence_request_from_document(
            document,
            rule_profile,
            kick_profile,
            timeout_seconds,
        )
        .and_then(|request| request.to_app_request())
        .map_err(|error| DesktopTauriCommandError::validation(error.to_string()))
    }

    pub(super) fn desktop_form_builds_app_request(
        request_json: &str,
    ) -> Result<GuiAppState, DesktopTauriCommandError> {
        let value: Value = serde_json::from_str(request_json)
            .map_err(|error| DesktopTauriCommandError::invalid_request(error.to_string()))?;
        validate_app_request_envelope(&value)?;

        let command = text_or_default(&value, &["command"], "pc")?;
        let language = text_or_default(&value, &["language"], "en")?;
        let lines = optional_u8_any(&value, &["lines"])?.unwrap_or(2);
        let rule = text_or_default(&value, &["rule"], "srs-plus")?;
        let backend = text_or_default(&value, &["backend"], "auto")?;
        let queue = text_or_default(&value, &["queue"], "")?;
        let patterns = text_or_default(&value, &["patterns"], "")?;
        if !queue.trim().is_empty() && !patterns.trim().is_empty() {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop queue and patterns are mutually exclusive",
            ));
        }
        let hold_enabled = bool_or_default(&value, &["hold_enabled"], true)?;
        let count_policy = text_or_default(&value, &["count_policy"], "unique")?;
        let queue_observation_policy = optional_text(&value, &["queue_knowledge"])?
            .map(|value| {
                QueueObservationPolicy::from_keyword(value).ok_or_else(|| {
                    DesktopTauriCommandError::invalid_request(format!(
                        "invalid desktop queue_knowledge '{value}'"
                    ))
                })
            })
            .transpose()?
            .unwrap_or_default();

        let problem_form = match command {
            "pc" if !patterns.trim().is_empty() => {
                GuiProblemForm::opening_pc_queue_pattern(lines, rule, patterns, hold_enabled)
            }
            "pc" if queue.trim().is_empty() => GuiProblemForm::OpeningPc(
                GuiOpeningPcForm::new(lines, rule).with_hold_enabled(hold_enabled),
            ),
            "pc" => GuiProblemForm::opening_pc_fixed_queue(lines, rule, queue, hold_enabled),
            "pc-scenario" => {
                let visible_height = optional_u8_any(&value, &["visible_height"])?.unwrap_or(lines);
                let board_mask = parse_board_mask(value.get("board_mask"))?;
                let piece_window = optional_usize_any(&value, &["piece_window"])?
                    .filter(|piece_window| *piece_window > 0)
                    .ok_or_else(|| {
                        DesktopTauriCommandError::invalid_request(
                            "desktop scenario PC requires a positive piece_window",
                        )
                    })?;
                let hold_piece = parse_hold_piece(value.get("hold_piece"))?;
                if queue.trim().is_empty() && patterns.trim().is_empty() {
                    GuiProblemForm::scenario_pc_standard_bag_with_execution_input(
                        visible_height,
                        board_mask,
                        rule,
                        piece_window,
                        hold_piece,
                        hold_enabled,
                        count_policy,
                    )
                } else if patterns.trim().is_empty() {
                    GuiProblemForm::scenario_pc_with_execution_input(
                        visible_height,
                        board_mask,
                        queue,
                        rule,
                        piece_window,
                        hold_piece,
                        hold_enabled,
                        count_policy,
                    )
                } else {
                    GuiProblemForm::scenario_pc_pattern_with_execution_input(
                        visible_height,
                        board_mask,
                        patterns,
                        rule,
                        piece_window,
                        hold_piece,
                        hold_enabled,
                        count_policy,
                    )
                }
            }
            _ => {
                return Err(DesktopTauriCommandError::invalid_request(
                    "desktop host supports pc and pc-scenario commands",
                ))
            }
        };
        let score_mode = text_or_default(&value, &["score_mode"], "off")?;
        let canonical_pc_path = score_mode == "path";
        let canonical_pc_save = matches!(score_mode, "saves" | "best-save");
        let canonical_pc_score_finder = score_mode == "score-finder";
        let canonical_pc_score_minimals = score_mode == "score-minimals";
        let base_objective_kind = match command {
            "pc" if score_mode == "failed-queue" || canonical_pc_save || canonical_pc_path => {
                ObjectiveKind::All
            }
            "pc-scenario" if matches!(count_policy, "all" | "count-all") => ObjectiveKind::All,
            _ => ObjectiveKind::Unique,
        };
        let objective_kind = score_mode_objective_kind(score_mode, base_objective_kind)
            .map_err(|error| DesktopTauriCommandError::invalid_request(error.to_string()))?;
        validate_pc_observation_objective(queue_observation_policy, objective_kind).map_err(
            |error| {
                DesktopTauriCommandError::invalid_request(format!(
                    "{}: {}",
                    error.code(),
                    error.message()
                ))
            },
        )?;
        let initial_b2b = optional_u16(&value, "initial_b2b")?
            .map(u32::from)
            .unwrap_or(0);
        let score_profile = text_or_default(&value, &["score_profile"], "tetrio")?;
        let spin_profile = text_or_default(&value, &["spin_profile"], "t-spins")?;
        let solution_probabilities = bool_or_default(&value, &["solution_probabilities"], false)?;
        let preserve_b2b = bool_or_default(&value, &["preserve_b2b"], false)?;
        let precompute_build_dependencies =
            bool_or_default(&value, &["precompute_build_dependencies"], false)?;
        let tablebase_requested =
            bool_or_default(&value, &["tablebase_requested", "tablebase_enabled"], false)?;
        let finesse = text_or_default(&value, &["finesse"], "off")?;
        let pattern_knowledge = text_or_default(&value, &["pattern_knowledge"], "both")?;
        let score_active = matches!(score_mode, "summary" | "score-finder" | "score-minimals");
        if (!score_active
            && (initial_b2b != 0
                || score_profile != "tetrio"
                || (!preserve_b2b && spin_profile != "t-spins")))
            || (score_mode == "failed-queue" && solution_probabilities)
        {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop PC request contains an option that is inactive for its score mode",
            ));
        }
        if score_mode == "tiling"
            && (rule != "srs-plus"
                || count_policy != "unique"
                || score_profile != "tetrio"
                || spin_profile != "t-spins"
                || initial_b2b != 0
                || preserve_b2b
                || solution_probabilities
                || precompute_build_dependencies
                || tablebase_requested
                || queue_observation_policy.requires_observation_policy()
                || finesse != "off"
                || pattern_knowledge != "both")
        {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop tiling-only PC request contains a noncanonical inactive option",
            ));
        }
        if canonical_pc_save
            && (count_policy != "all"
                || !queue.trim().is_empty()
                || preserve_b2b
                || solution_probabilities
                || precompute_build_dependencies
                || tablebase_requested
                || queue_observation_policy.requires_observation_policy()
                || finesse != "off"
                || pattern_knowledge != "both")
        {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop pc save request contains a noncanonical inactive option or lacks bag provenance",
            ));
        }
        if canonical_pc_path
            && (count_policy != "all"
                || score_profile != "tetrio"
                || spin_profile != "t-spins"
                || initial_b2b != 0
                || preserve_b2b
                || solution_probabilities
                || precompute_build_dependencies
                || tablebase_requested
                || queue_observation_policy.requires_observation_policy()
                || finesse != "off"
                || pattern_knowledge != "both")
        {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop pc path requires objective-all/count-all and no score, probability, observation, tablebase, finesse, or dependency override",
            ));
        }
        if canonical_pc_score_minimals
            && (count_policy != "all"
                || preserve_b2b
                || solution_probabilities
                || precompute_build_dependencies
                || tablebase_requested
                || queue_observation_policy.requires_observation_policy()
                || finesse != "off"
                || pattern_knowledge != "both")
        {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop pc score-minimals request requires count-all score-only semantics and contains no constraint, probability, observation, tablebase, finesse, or dependency override",
            ));
        }
        if canonical_pc_score_finder
            && (command != "pc-scenario"
                || count_policy != "all"
                || queue.trim().is_empty()
                || !patterns.trim().is_empty()
                || score_profile != "jstris-ultra"
                || spin_profile != "t-spins"
                || initial_b2b > 1
                || preserve_b2b
                || solution_probabilities
                || precompute_build_dependencies
                || tablebase_requested
                || queue_observation_policy.requires_observation_policy()
                || finesse != "off"
                || pattern_knowledge != "both")
        {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop pc score-finder requires an explicit scenario, one fixed queue, jstris-ultra/t-spins, initial B2B 0 or 1, count-all, and no inactive semantic override",
            ));
        }
        let problem_form = problem_form
            .with_queue_observation_policy(queue_observation_policy)
            .with_score_input(score_mode, initial_b2b)
            .with_score_profiles(score_profile, spin_profile)
            .with_back_to_back_preservation(preserve_b2b)
            .with_solution_probabilities(solution_probabilities);

        let allow_backend_fallback =
            bool_or_default(&value, &["allow_backend_fallback"], backend == "auto")?;
        let mut backend_form =
            GuiBackendForm::from_backend_id(backend).with_allow_fallback(allow_backend_fallback);
        if let Some(workers) = optional_u16(&value, "workers")? {
            backend_form = backend_form.with_workers(workers);
        }
        backend_form = backend_form.with_use_all_logical_processors(
            optional_bool(
                &value,
                &["use_all_logical_processors", "use_all_cpu_threads"],
            )?
            .unwrap_or(false),
        );
        backend_form =
            backend_form.with_precompute_build_dependencies(precompute_build_dependencies);
        backend_form = backend_form.with_tablebase_requested(tablebase_requested);
        if let Some(device) = optional_text(&value, &["gpu_device"])? {
            backend_form = backend_form.with_gpu_device(device);
        }
        if let Some(memory) = optional_u32(&value, "memory_budget_mb")? {
            backend_form = backend_form.with_memory_budget_mb(memory);
        }
        if let Some(candidates) = optional_u32(&value, "candidate_budget")? {
            backend_form = backend_form.with_candidate_budget(candidates);
        }
        if let Some(patterns) = optional_u32(&value, "pattern_budget")? {
            backend_form = backend_form.with_pattern_budget(patterns);
        }

        Ok(GuiAppState::default()
            .with_current_language(language)
            .with_problem_form(problem_form)
            .with_backend_form(backend_form))
    }

    fn build_setup_app_request(value: &Value) -> Result<AppRequest, DesktopTauriCommandError> {
        let remaining = parse_pieces(
            required_text(
                value,
                &["setup_remaining", "remaining", "remaining_pieces"],
                "setup_remaining",
            )?,
            "setup remaining",
        )?;
        let allow_post_cycle_borrow = optional_bool(
            value,
            &["setup_allow_post_cycle_borrow", "allow_post_cycle_borrow"],
        )?
        .unwrap_or(false);
        let mut request = CliCommandRequest::setup(remaining, allow_post_cycle_borrow)
            .with_rule(parse_desktop_rule(value)?);

        let search_mode = optional_text(value, &["setup_mode", "search_mode", "mode"])?
            .map(|mode| {
                SetupSearchMode::from_keyword(mode).ok_or_else(|| {
                    DesktopTauriCommandError::invalid_request(format!(
                        "invalid desktop setup search_mode '{mode}'"
                    ))
                })
            })
            .transpose()?
            .unwrap_or_default();
        request = request.with_setup_search_mode(search_mode);

        if let Some(queue_based) =
            optional_nonempty_text(value, &["setup_qb", "qb_queue", "queue_based_pieces"])?
        {
            request = request.with_setup_queue_based_pieces(parse_pieces(
                queue_based,
                "setup queue-based pieces",
            )?);
        }
        if let Some(next_cycle) = optional_nonempty_text(
            value,
            &[
                "setup_next_cycle_remaining",
                "next_cycle_remaining",
                "next_cycle_remaining_pieces",
            ],
        )? {
            request = request.with_setup_next_cycle_remaining_pieces(parse_pieces(
                next_cycle,
                "setup next-cycle remaining pieces",
            )?);
        }

        let candidate_priority =
            optional_text(value, &["setup_priority", "candidate_priority", "priority"])?
                .map(|priority| {
                    SetupCandidatePriority::from_keyword(priority).ok_or_else(|| {
                        DesktopTauriCommandError::invalid_request(format!(
                            "invalid desktop setup candidate_priority '{priority}'"
                        ))
                    })
                })
                .transpose()?
                .unwrap_or_default();
        let length_preference = optional_text(value, &["setup_length", "length_preference"])?
            .map(|preference| {
                SetupLengthPreference::from_keyword(preference).ok_or_else(|| {
                    DesktopTauriCommandError::invalid_request(format!(
                        "invalid desktop setup length_preference '{preference}'"
                    ))
                })
            })
            .transpose()?
            .unwrap_or_default();
        request = request
            .with_setup_candidate_priority(candidate_priority)
            .with_setup_length_preference(length_preference)
            .with_setup_max_pieces(
                optional_u8_any(value, &["setup_max_pieces", "max_setup_pieces"])?.unwrap_or(9),
            );

        let queue_observation = optional_text(value, &["queue_knowledge"])?
            .map(|policy| {
                QueueObservationPolicy::from_keyword(policy).ok_or_else(|| {
                    DesktopTauriCommandError::invalid_request(format!(
                        "invalid desktop queue_knowledge '{policy}'"
                    ))
                })
            })
            .transpose()?
            .unwrap_or_default();
        request = request
            .with_queue_observation_policy(queue_observation)
            .with_tablebase_requested(
                optional_bool(value, &["tablebase_requested", "tablebase_enabled"])?
                    .unwrap_or(false),
            );

        let setup_id =
            optional_nonempty_text(value, &["setup_path_setup_id", "paths_for", "setup_id"])?;
        let condition_id = optional_nonempty_text(
            value,
            &["setup_path_condition_id", "condition", "condition_id"],
        )?;
        match (setup_id, condition_id) {
            (Some(setup_id), Some(condition_id)) => {
                let detail =
                    SetupPathDetail::from_setup_id(setup_id, condition_id).ok_or_else(|| {
                        DesktopTauriCommandError::invalid_request(
                            "invalid desktop setup path detail request",
                        )
                    })?;
                request = request.with_setup_path_detail(detail);
            }
            (Some(_), None) | (None, Some(_)) => {
                return Err(DesktopTauriCommandError::invalid_request(
                    "desktop setup path detail requires both setup_id and condition_id",
                ));
            }
            (None, None) => {}
        }

        request = apply_worker_policy(request, value)?;
        typed_cli_request_to_app_request("setup", request)
    }

    fn build_probability_app_request(
        value: &Value,
    ) -> Result<AppRequest, DesktopTauriCommandError> {
        if bool_or_default(value, &["tablebase_requested", "tablebase_enabled"], false)? {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop build-probability does not support tablebase",
            ));
        }
        let base_words = parse_board_words(
            optional_value(value, &["base_mask", "existing_mask"])?,
            "base_mask",
        )?;
        let target_words = parse_board_words(value.get("target_mask"), "target_mask")?;
        let height = required_u16(value, &["height", "visible_height"], "height")?;
        if !(1..=24).contains(&height) {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop build-probability height must be between 1 and 24",
            ));
        }

        let spin_profile_text = optional_text(value, &["spin_profile"])?;
        let spin_profile = spin_profile_text
            .map(|profile| {
                SpinProfileSelection::parse(profile).ok_or_else(|| {
                    DesktopTauriCommandError::invalid_request(format!(
                        "invalid desktop build-probability spin_profile '{profile}'"
                    ))
                })
            })
            .transpose()?
            .unwrap_or(SpinProfileSelection::TSpins);
        let aggregation_text = text_or_default(
            value,
            &["build_aggregation", "aggregation", "aggregate"],
            "buildability",
        )?;
        let aggregation = match aggregation_text {
            "buildability" | "build" => BuildProbabilityAggregation::Buildability,
            "tiling" | "tiling-only" => BuildProbabilityAggregation::TilingOnly,
            "spin" => BuildProbabilityAggregation::spin_search(spin_profile),
            other => {
                return Err(DesktopTauriCommandError::invalid_request(format!(
                    "invalid desktop build-probability aggregation '{other}'"
                )));
            }
        };
        let preserve_b2b = bool_or_default(value, &["preserve_b2b"], false)?;
        let precompute_build_dependencies =
            bool_or_default(value, &["precompute_build_dependencies"], false)?;
        let solution_probabilities = bool_or_default(value, &["solution_probabilities"], false)?;
        let finesse_text = text_or_default(value, &["finesse"], "off")?;
        let finesse_metric = FinesseMetric::parse(finesse_text).ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(format!(
                "invalid desktop build-probability finesse '{finesse_text}'"
            ))
        })?;
        let pattern_knowledge_text = text_or_default(value, &["pattern_knowledge"], "both")?;
        let pattern_knowledge =
            FinessePatternKnowledge::parse(pattern_knowledge_text).ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(format!(
                    "invalid desktop build-probability pattern_knowledge '{pattern_knowledge_text}'"
                ))
            })?;
        if !finesse_metric.requested() && pattern_knowledge != FinessePatternKnowledge::Both {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop build-probability pattern_knowledge requires finesse inputs",
            ));
        }
        if !aggregation.requests_spin_coverage()
            && !preserve_b2b
            && spin_profile != SpinProfileSelection::TSpins
        {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop build-probability spin_profile requires spin aggregation or B2B preservation",
            ));
        }
        if matches!(aggregation, BuildProbabilityAggregation::TilingOnly)
            && (preserve_b2b
                || precompute_build_dependencies
                || solution_probabilities
                || finesse_metric.requested()
                || spin_profile != SpinProfileSelection::TSpins
                || pattern_knowledge != FinessePatternKnowledge::Both
                || text_or_default(value, &["rule"], "srs-plus")? != "srs-plus")
        {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop tiling-only build probability contains a noncanonical inactive option",
            ));
        }

        let hold_enabled = bool_or_default(value, &["hold_enabled"], true)?;
        let hold_piece = parse_hold_piece_kind(value.get("hold_piece"))?;
        if !hold_enabled && hold_piece.is_some() {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop build probability cannot combine disabled hold with an occupied hold slot",
            ));
        }
        let mut input = WebBuildProbabilityInput::from_words(base_words, target_words, height)
            .with_hold_piece(hold_piece)
            .with_allow_hold(hold_enabled)
            .with_horizontal_mirror_included(
                optional_bool(value, &["include_mirror", "include_horizontal_mirror"])?
                    .unwrap_or(true),
            )
            .with_aggregation(aggregation)
            .with_finesse(finesse_metric, pattern_knowledge);
        if let Some(source_piece_count) =
            optional_usize_any(value, &["source_piece_count", "source_pieces"])?
        {
            if source_piece_count == 0 {
                return Err(DesktopTauriCommandError::invalid_request(
                    "desktop build-probability source_piece_count must be positive",
                ));
            }
            input = input.with_source_piece_count(source_piece_count);
        }

        let mut request = CliCommandRequest::build_probability(input)
            .with_rule(parse_desktop_rule(value)?)
            .with_hold_enabled(hold_enabled)
            .with_precompute_build_dependencies(precompute_build_dependencies)
            .with_solution_probabilities(solution_probabilities)
            .with_cpu_warmup(bool_or_default(value, &["cpu_warmup"], false)?)
            .with_gpu_warmup(bool_or_default(value, &["gpu_warmup"], false)?);
        let backend_text = text_or_default(value, &["backend"], "cpu")?;
        let backend = RequestedSearchBackend::parse(backend_text).ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(format!(
                "invalid desktop build-probability backend '{backend_text}'"
            ))
        })?;
        request = request
            .with_backend(backend)
            .with_allow_backend_fallback(bool_or_default(
                value,
                &["allow_backend_fallback"],
                backend == RequestedSearchBackend::Auto,
            )?);
        if let Some(device_text) = optional_text(value, &["gpu_device"])? {
            let device = GpuDeviceSelection::parse(device_text).ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(format!(
                    "invalid desktop build-probability gpu_device '{device_text}'"
                ))
            })?;
            request = request.with_gpu_device(device);
        }
        request = apply_queue(request, value)?;
        if matches!(aggregation, BuildProbabilityAggregation::TilingOnly) {
            request = request.with_objective(ObjectivePolicy::tiling());
        } else if preserve_b2b {
            request = request.with_objective(
                ObjectivePolicy::unique().with_back_to_back_preservation(spin_profile),
            );
        }
        request = apply_pc_resource_limits(request, value)?;
        typed_cli_request_to_app_request("build-probability", request)
    }

    const DESKTOP_BUILD_V2_FIELDS: &[&str] = &[
        "app_request_model",
        "command",
        "language",
        "capability_id",
        "base_mask",
        "target_mask",
        "visible_height",
        "source_piece_count",
        "target_format",
        "target_document",
        "solution_format",
        "solution_document",
        "queue",
        "patterns",
        "queue_knowledge",
        "hold_enabled",
        "hold_piece",
        "objective",
        "score_profile",
        "initial_b2b",
        "rule",
        "workers",
        "use_all_logical_processors",
        "backend",
        "allow_backend_fallback",
    ];

    fn build_v2_app_request(value: &Value) -> Result<AppRequest, DesktopTauriCommandError> {
        validate_build_v2_fields(value)?;
        require_build_v2_literal(value, "app_request_model", "clearra-app/AppRequest")?;
        require_build_v2_literal(value, "command", "build-v2")?;
        match required_build_v2_string(value, "language")? {
            "en" | "ko" => {}
            language => {
                return Err(DesktopTauriCommandError::invalid_request(format!(
                    "invalid desktop build-v2 language '{language}'"
                )));
            }
        }
        require_build_v2_literal(value, "backend", "cpu")?;
        if required_build_v2_bool(value, "allow_backend_fallback")? {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop build-v2 requires allow_backend_fallback=false",
            ));
        }

        let capability =
            parse_build_v2_capability(required_build_v2_string(value, "capability_id")?)?;
        let objective = parse_build_v2_objective(required_build_v2_string(value, "objective")?)?;
        let queue_knowledge = match required_build_v2_string(value, "queue_knowledge")? {
            "oracle" => BuildQueueKnowledge::Oracle,
            "visible-7" => BuildQueueKnowledge::VisibleSeven,
            policy => {
                return Err(DesktopTauriCommandError::invalid_request(format!(
                    "invalid desktop build-v2 queue_knowledge '{policy}'"
                )));
            }
        };

        let hold_enabled = required_build_v2_bool(value, "hold_enabled")?;
        let hold_text = required_build_v2_string(value, "hold_piece")?;
        if !matches!(hold_text, "empty" | "I" | "O" | "T" | "S" | "Z" | "J" | "L") {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop build-v2 hold_piece must be empty or one of I, O, T, S, Z, J, L",
            ));
        }
        let hold_piece = parse_hold_piece_kind(value.get("hold_piece"))?;
        if !hold_enabled && hold_piece.is_some() {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop build-v2 cannot combine disabled hold with an occupied hold slot",
            ));
        }

        let queue = required_build_v2_string(value, "queue")?;
        let patterns = required_build_v2_string(value, "patterns")?;
        match (queue.is_empty(), patterns.is_empty()) {
            (false, true) | (true, false) => {}
            _ => {
                return Err(DesktopTauriCommandError::invalid_request(
                    "desktop build-v2 requires exactly one nonempty queue or patterns field",
                ));
            }
        }

        let mut input = if capability == WebBuildV2Capability::Cover {
            reject_build_v2_fields(
                value,
                capability,
                &[
                    "target_format",
                    "target_document",
                    "solution_format",
                    "solution_document",
                ],
            )?;
            let height = required_build_v2_u16(value, "visible_height")?;
            if !(1..=24).contains(&height) {
                return Err(DesktopTauriCommandError::invalid_request(
                    "desktop build-v2 visible_height must be between 1 and 24",
                ));
            }
            let input = WebBuildV2Input::cover(
                parse_build_v2_mask(value, "base_mask")?,
                parse_build_v2_mask(value, "target_mask")?,
                height,
                objective,
            )
            .map_err(build_v2_web_error)?;
            match optional_usize_any(value, &["source_piece_count"])? {
                Some(0) => {
                    return Err(DesktopTauriCommandError::invalid_request(
                        "desktop build-v2 source_piece_count must be positive",
                    ));
                }
                Some(count) => input
                    .with_source_piece_count(count)
                    .map_err(build_v2_web_error)?,
                None => input,
            }
        } else if capability.uses_target_document() {
            reject_build_v2_fields(
                value,
                capability,
                &[
                    "base_mask",
                    "target_mask",
                    "visible_height",
                    "source_piece_count",
                    "solution_format",
                    "solution_document",
                ],
            )?;
            WebBuildV2Input::target_document(
                capability,
                parse_build_v2_format(value, "target_format")?,
                required_nonempty_build_v2_string(value, "target_document")?,
                objective,
            )
            .map_err(build_v2_web_error)?
        } else {
            reject_build_v2_fields(
                value,
                capability,
                &[
                    "base_mask",
                    "target_mask",
                    "visible_height",
                    "source_piece_count",
                    "target_format",
                    "target_document",
                ],
            )?;
            WebBuildV2Input::solution_document(
                capability,
                parse_build_v2_format(value, "solution_format")?,
                required_nonempty_build_v2_string(value, "solution_document")?,
                objective,
            )
            .map_err(build_v2_web_error)?
        };

        input = input
            .with_queue_knowledge(queue_knowledge)
            .with_hold_piece(hold_piece)
            .with_allow_hold(hold_enabled);
        if capability.score_capable() {
            let score_profile = match optional_text(value, &["score_profile"])? {
                Some(profile) => parse_build_v2_score_profile(profile)?,
                None => BuildScoreProfile::default(),
            };
            let initial_b2b = optional_u16_any(value, &["initial_b2b"])?.unwrap_or(0);
            input = input
                .with_score_options(score_profile, initial_b2b)
                .map_err(build_v2_web_error)?;
        } else if value.get("score_profile").is_some() || value.get("initial_b2b").is_some() {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop {} does not accept score_profile or initial_b2b",
                capability.capability_id()
            )));
        }

        let rule = parse_rule_profile(required_nonempty_build_v2_string(value, "rule")?)
            .map_err(|error| DesktopTauriCommandError::invalid_request(error.to_string()))?;
        let workers = required_build_v2_usize(value, "workers")?;
        let use_all_logical_processors =
            required_build_v2_bool(value, "use_all_logical_processors")?;
        if workers > 0 && use_all_logical_processors {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop build-v2 workers and use_all_logical_processors are mutually exclusive",
            ));
        }

        let mut request = CliCommandRequest::build_v2(input)
            .with_rule(rule)
            .with_hold_enabled(hold_enabled)
            .with_backend(RequestedSearchBackend::Cpu)
            .with_allow_backend_fallback(false)
            .with_worker_hardware_limit(WorkerPolicy::hardware_worker_limit())
            .with_use_all_logical_processors(use_all_logical_processors);
        request = if queue.is_empty() {
            request.with_patterns(patterns)
        } else {
            request.with_queue(queue)
        };
        if workers > 0 {
            request = request.with_workers(workers);
        }
        typed_cli_request_to_app_request("build-v2", request)
    }

    fn validate_build_v2_fields(value: &Value) -> Result<(), DesktopTauriCommandError> {
        let object = value.as_object().ok_or_else(|| {
            DesktopTauriCommandError::invalid_request("desktop request must be a JSON object")
        })?;
        if let Some(field) = object.keys().find(|field| {
            field.as_str() != "profiles" && !DESKTOP_BUILD_V2_FIELDS.contains(&field.as_str())
        }) {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop build-v2 does not accept field '{field}'"
            )));
        }
        Ok(())
    }

    fn required_build_v2_string<'a>(
        value: &'a Value,
        key: &str,
    ) -> Result<&'a str, DesktopTauriCommandError> {
        value
            .get(key)
            .ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(format!(
                    "desktop build-v2 requires {key}"
                ))
            })?
            .as_str()
            .ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(format!(
                    "desktop build-v2 {key} must be a string"
                ))
            })
    }

    fn required_nonempty_build_v2_string<'a>(
        value: &'a Value,
        key: &str,
    ) -> Result<&'a str, DesktopTauriCommandError> {
        let text = required_build_v2_string(value, key)?;
        if text.is_empty() {
            Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop build-v2 {key} must not be empty"
            )))
        } else {
            Ok(text)
        }
    }

    fn required_build_v2_bool(value: &Value, key: &str) -> Result<bool, DesktopTauriCommandError> {
        optional_bool(value, &[key])?.ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(format!(
                "desktop build-v2 requires boolean {key}"
            ))
        })
    }

    fn required_build_v2_u16(value: &Value, key: &str) -> Result<u16, DesktopTauriCommandError> {
        optional_u16_any(value, &[key])?.ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(format!(
                "desktop build-v2 requires integer {key}"
            ))
        })
    }

    fn required_build_v2_usize(
        value: &Value,
        key: &str,
    ) -> Result<usize, DesktopTauriCommandError> {
        optional_usize_any(value, &[key])?.ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(format!(
                "desktop build-v2 requires integer {key}"
            ))
        })
    }

    fn require_build_v2_literal(
        value: &Value,
        key: &str,
        expected: &str,
    ) -> Result<(), DesktopTauriCommandError> {
        let actual = required_build_v2_string(value, key)?;
        if actual == expected {
            Ok(())
        } else {
            Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop build-v2 {key} must be '{expected}'"
            )))
        }
    }

    fn parse_build_v2_capability(
        capability: &str,
    ) -> Result<WebBuildV2Capability, DesktopTauriCommandError> {
        match capability {
            "build.cover" => Ok(WebBuildV2Capability::Cover),
            "build.setup" => Ok(WebBuildV2Capability::Setup),
            "build.congruent" => Ok(WebBuildV2Capability::Congruent),
            "build.congruent-cover" => Ok(WebBuildV2Capability::CongruentCover),
            "build.setup-cover" => Ok(WebBuildV2Capability::SetupCover),
            "build.setup-cover-percent" => Ok(WebBuildV2Capability::SetupCoverPercent),
            "build.setup-cover-score" => Ok(WebBuildV2Capability::SetupCoverScore),
            "build.evaluate.cover" => Ok(WebBuildV2Capability::EvaluateCover),
            "build.evaluate.minimals" => Ok(WebBuildV2Capability::EvaluateMinimals),
            "build.evaluate.score" => Ok(WebBuildV2Capability::EvaluateScore),
            "build.evaluate.b2b-cover" => Ok(WebBuildV2Capability::EvaluateB2bCover),
            "build.evaluate.cover-percent" => Ok(WebBuildV2Capability::EvaluateCoverPercent),
            _ => Err(DesktopTauriCommandError::invalid_request(format!(
                "unsupported desktop build-v2 capability_id '{capability}'"
            ))),
        }
    }

    fn parse_build_v2_objective(
        objective: &str,
    ) -> Result<BuildObjective, DesktopTauriCommandError> {
        match objective {
            "all" => Ok(BuildObjective::All),
            "unique" => Ok(BuildObjective::Unique),
            "min-cover" => Ok(BuildObjective::MinCover),
            "max-probability-minimum" => Ok(BuildObjective::MaxProbabilityMinimum),
            "max-score-cover" => Ok(BuildObjective::MaxScoreCover),
            _ => Err(DesktopTauriCommandError::invalid_request(format!(
                "invalid desktop build-v2 objective '{objective}'"
            ))),
        }
    }

    fn parse_build_v2_score_profile(
        profile: &str,
    ) -> Result<BuildScoreProfile, DesktopTauriCommandError> {
        if !matches!(profile, "guideline" | "jstris-ultra" | "tetrio") {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "invalid desktop build-v2 score_profile '{profile}'"
            )));
        }
        BuildScoreProfile::parse(profile).ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(format!(
                "invalid desktop build-v2 score_profile '{profile}'"
            ))
        })
    }

    fn parse_build_v2_format(
        value: &Value,
        key: &str,
    ) -> Result<FieldDocumentFormat, DesktopTauriCommandError> {
        match required_build_v2_string(value, key)? {
            "ctk3" => Ok(FieldDocumentFormat::Ctk3),
            "fumen" => Ok(FieldDocumentFormat::Fumen),
            format => Err(DesktopTauriCommandError::invalid_request(format!(
                "invalid desktop build-v2 {key} '{format}'"
            ))),
        }
    }

    fn parse_build_v2_mask(value: &Value, key: &str) -> Result<[u64; 4], DesktopTauriCommandError> {
        let text = required_nonempty_build_v2_string(value, key)?;
        parse_board_words_text(text).ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(format!(
                "desktop build-v2 {key} must be a 256-bit decimal or hexadecimal string"
            ))
        })
    }

    fn reject_build_v2_fields(
        value: &Value,
        capability: WebBuildV2Capability,
        fields: &[&str],
    ) -> Result<(), DesktopTauriCommandError> {
        if let Some(field) = fields.iter().find(|field| value.get(**field).is_some()) {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop {} does not accept field '{field}'",
                capability.capability_id()
            )));
        }
        Ok(())
    }

    fn build_v2_web_error(error: impl core::fmt::Display) -> DesktopTauriCommandError {
        DesktopTauriCommandError::invalid_request(format!(
            "invalid desktop build-v2 request: {error}"
        ))
    }

    fn build_forward_app_request(
        value: &Value,
        command: &str,
    ) -> Result<AppRequest, DesktopTauriCommandError> {
        let height = optional_u8_any(value, &["height", "visible_height"])?.unwrap_or(8);
        if !(1..=24).contains(&height) {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop forward-search height must be between 1 and 24",
            ));
        }
        let board_words = match optional_value(value, &["board_mask", "initial_board_mask"])? {
            Some(board) => parse_board_words(Some(board), "board_mask")?,
            None => [0; 4],
        };
        ensure_words_fit_height(board_words, height, "board_mask")?;

        let piece_source = parse_forward_piece_source(value, command == "spin-finder")?;
        let rule = parse_desktop_rule(value)?;
        let default_spin_profile = if command == "ren" {
            "disabled"
        } else {
            "t-spins"
        };
        let spin_profile_text = text_or_default(value, &["spin_profile"], default_spin_profile)?;
        let spin_profile = SpinProfileId::parse(spin_profile_text).ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(format!(
                "invalid desktop forward-search spin_profile '{spin_profile_text}'"
            ))
        })?;
        let initial_combo = optional_u16_any(value, &["initial_combo"])?
            .and_then(|combo| (combo > 0).then_some(combo));
        let initial_back_to_back =
            optional_u16_any(value, &["initial_b2b"])?.and_then(|b2b| (b2b > 0).then(|| b2b - 1));
        let mode = if command == "damage" {
            match text_or_default(value, &["damage_aggregation", "aggregation"], "maximum")? {
                "maximum" | "max" => {
                    if optional_value(value, &["minimum_damage"])?.is_some() {
                        return Err(DesktopTauriCommandError::invalid_request(
                            "desktop maximum damage search cannot contain minimum_damage",
                        ));
                    }
                    ForwardSearchMode::MaximumDamage
                }
                "at-least" | "minimum" => ForwardSearchMode::DamageAtLeast(
                    optional_u32_any(value, &["minimum_damage"])?.ok_or_else(|| {
                        DesktopTauriCommandError::invalid_request(
                            "desktop at-least damage search requires minimum_damage",
                        )
                    })?,
                ),
                other => {
                    return Err(DesktopTauriCommandError::invalid_request(format!(
                        "invalid desktop damage aggregation '{other}'"
                    )));
                }
            }
        } else if command == "spin-finder" {
            if optional_value(value, &["aggregation"])?.is_some() {
                return Err(DesktopTauriCommandError::invalid_request(
                    "desktop spin-finder request cannot contain damage aggregation options",
                ));
            }
            let spin_category =
                parse_spin_category(text_or_default(value, &["spin_category"], "any")?)?;
            if spin_category == ForwardSpinCategory::Other
                && !spin_profile.recognizes_non_t_immobile_spins()
            {
                return Err(DesktopTauriCommandError::invalid_request(
                    "desktop spin_category other requires a non-T spin profile",
                ));
            }
            ForwardSearchMode::SpinFinder(ForwardSpinTarget::with_line_requirement(
                parse_spin_line_requirement(optional_value(value, &["spin_lines"])?)?,
                spin_category,
            ))
        } else {
            ForwardSearchMode::MaximumRen
        };
        let line_clear_policy = if bool_or_default(value, &["preserve_b2b"], false)? {
            ForwardLineClearPolicy::PreserveBackToBack
        } else {
            ForwardLineClearPolicy::Any
        };
        let query = ForwardSearchQuery::new_with_source(
            Board256Mask::from_words(board_words),
            height,
            piece_source,
            bool_or_default(value, &["hold_enabled"], true)?,
            rule.id(),
            spin_profile,
            initial_combo,
            initial_back_to_back,
            mode,
        )
        .with_line_clear_policy(line_clear_policy);
        let request = apply_worker_policy(CliCommandRequest::forward(command, query), value)?;
        typed_cli_request_to_app_request(command, request)
    }

    fn typed_cli_request_to_app_request(
        command: &str,
        request: CliCommandRequest,
    ) -> Result<AppRequest, DesktopTauriCommandError> {
        request.to_app_request().map_err(|error| {
            DesktopTauriCommandError::invalid_request(format!(
                "invalid desktop {command} request: {error}"
            ))
        })
    }

    fn parse_pieces(
        value: &str,
        field_name: &str,
    ) -> Result<Vec<PieceKind>, DesktopTauriCommandError> {
        let sequence = parse_piece_sequence(value, field_name)
            .map_err(|error| DesktopTauriCommandError::invalid_request(error.to_string()))?;
        if sequence.is_empty() {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop {field_name} must contain at least one piece"
            )));
        }
        Ok(sequence.into_pieces())
    }

    fn parse_desktop_rule(
        value: &Value,
    ) -> Result<clearra_rules::profile::rule_profile::RuleProfile, DesktopTauriCommandError> {
        parse_rule_profile(text_or_default(value, &["rule"], "srs-plus")?)
            .map_err(|error| DesktopTauriCommandError::invalid_request(error.to_string()))
    }

    fn parse_request_structural_profiles(
        value: &Value,
    ) -> Result<RequestStructuralProfiles, DesktopTauriCommandError> {
        let Some(profiles) = value.get("profiles") else {
            return Ok(RequestStructuralProfiles::STANDARD);
        };
        let object = profiles.as_object().ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(
                "desktop profiles must be an object with board, piece, and bag",
            )
        })?;
        if let Some(field) = object
            .keys()
            .find(|key| !matches!(key.as_str(), "board" | "piece" | "bag"))
        {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "desktop profiles does not accept field '{field}'"
            )));
        }
        let profile_text = |key: &'static str| {
            object.get(key).and_then(Value::as_str).ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(format!(
                    "desktop profiles requires canonical string field '{key}'"
                ))
            })
        };
        RequestStructuralProfiles::parse_canonical(
            profile_text("board")?,
            profile_text("piece")?,
            profile_text("bag")?,
        )
        .map_err(|error| {
            DesktopTauriCommandError::invalid_request(format!(
                "invalid desktop request profile selection: {error}"
            ))
        })
    }

    fn apply_queue(
        mut request: CliCommandRequest,
        value: &Value,
    ) -> Result<CliCommandRequest, DesktopTauriCommandError> {
        let queue = optional_nonempty_text(value, &["queue"])?;
        let patterns = optional_nonempty_text(value, &["patterns"])?;
        match (queue, patterns) {
            (Some(_), Some(_)) => Err(DesktopTauriCommandError::invalid_request(
                "desktop queue and patterns are mutually exclusive",
            )),
            (Some(queue), None) => {
                request = request.with_queue(queue);
                Ok(request)
            }
            (None, Some(patterns)) => {
                request = request.with_patterns(patterns);
                Ok(request)
            }
            (None, None) => Ok(request),
        }
    }

    fn apply_worker_policy(
        mut request: CliCommandRequest,
        value: &Value,
    ) -> Result<CliCommandRequest, DesktopTauriCommandError> {
        let native_hardware_limit = WorkerPolicy::hardware_worker_limit();
        let workers = optional_usize_any(value, &["workers"])?
            .and_then(|workers| (workers > 0).then_some(workers));
        let automatic_worker_limit =
            optional_usize_any(value, &["automatic_worker_limit", "auto_workers"])?
                .and_then(|workers| (workers > 0).then_some(workers));
        if workers.is_some() && automatic_worker_limit.is_some() {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop workers and automatic_worker_limit are mutually exclusive",
            ));
        }
        if let Some(hardware_limit) =
            optional_usize_any(value, &["worker_hardware_limit", "hardware_workers"])?
        {
            if hardware_limit == 0 {
                return Err(DesktopTauriCommandError::invalid_request(
                    "desktop worker_hardware_limit must be positive",
                ));
            }
            if hardware_limit > native_hardware_limit {
                return Err(DesktopTauriCommandError::invalid_request(format!(
                    "desktop worker_hardware_limit {hardware_limit} exceeds the native logical processor limit {native_hardware_limit}"
                )));
            }
        }
        // The native host is authoritative. A browser/client hint may validate the
        // request, but it cannot lower or raise the actual desktop hardware count.
        request = request.with_worker_hardware_limit(native_hardware_limit);
        request = request.with_use_all_logical_processors(
            optional_bool(
                value,
                &["use_all_logical_processors", "use_all_cpu_threads"],
            )?
            .unwrap_or(false),
        );
        if let Some(workers) = workers {
            request = request.with_workers(workers);
        } else if let Some(workers) = automatic_worker_limit {
            request = request.with_automatic_worker_limit(workers);
        }
        Ok(request)
    }

    fn apply_pc_resource_limits(
        mut request: CliCommandRequest,
        value: &Value,
    ) -> Result<CliCommandRequest, DesktopTauriCommandError> {
        request = apply_worker_policy(request, value)?;
        if let Some(limit) = optional_usize_any(value, &["max_patterns", "pattern_budget"])? {
            if limit > 0 {
                request = request.with_max_patterns(limit);
            }
        }
        if let Some(limit) = optional_usize_any(value, &["max_nodes"])? {
            if limit > 0 {
                request = request.with_max_nodes(limit);
            }
        }
        if let Some(limit) = optional_usize_any(value, &["max_frontier_states"])? {
            if limit > 0 {
                request = request.with_max_frontier_states(limit);
            }
        }
        if let Some(limit) = optional_usize_any(value, &["max_candidates", "candidate_budget"])? {
            if limit > 0 {
                request = request.with_max_candidates(limit);
            }
        }
        if let Some(limit) = optional_u64_any(value, &["max_memory_mib", "memory_budget_mb"])? {
            if limit > 0 {
                request = request.with_max_memory_mib(limit);
            }
        }
        Ok(request)
    }

    fn parse_forward_piece_source(
        value: &Value,
        allow_patterns: bool,
    ) -> Result<ForwardPieceSource, DesktopTauriCommandError> {
        let queue = optional_nonempty_text(value, &["queue"])?;
        let patterns = optional_nonempty_text(value, &["patterns"])?;
        match (queue, patterns) {
            (Some(_), Some(_)) => Err(DesktopTauriCommandError::invalid_request(
                "desktop forward search queue and patterns are mutually exclusive",
            )),
            (Some(queue), None) => Ok(ForwardPieceSource::fixed_queue(parse_pieces(
                queue,
                "forward queue",
            )?)),
            (None, Some(_)) if !allow_patterns => Err(DesktopTauriCommandError::invalid_request(
                "desktop damage search accepts only an exact queue",
            )),
            (None, Some(patterns)) => {
                parse_queue_pattern(patterns, 5_764_801, "desktop spin-finder pattern")
                    .map(ForwardPieceSource::pattern)
                    .map_err(|error| DesktopTauriCommandError::invalid_request(error.to_string()))
            }
            (None, None) => Err(DesktopTauriCommandError::invalid_request(
                if allow_patterns {
                    "desktop spin-finder requires queue or patterns"
                } else {
                    "desktop damage search requires queue"
                },
            )),
        }
    }

    fn parse_spin_line_requirement(
        value: Option<&Value>,
    ) -> Result<ForwardSpinLineRequirement, DesktopTauriCommandError> {
        let Some(value) = value else {
            return Ok(ForwardSpinLineRequirement::Any);
        };
        if let Some(lines) = value.as_u64() {
            let lines = u8::try_from(lines)
                .ok()
                .filter(|lines| *lines <= 4)
                .ok_or_else(|| {
                    DesktopTauriCommandError::invalid_request(
                        "desktop spin_lines must be any, 0..4, or 0+..4+",
                    )
                })?;
            return Ok(ForwardSpinLineRequirement::Exact(lines));
        }
        let text = value.as_str().ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(
                "desktop spin_lines must be any, 0..4, or 0+..4+",
            )
        })?;
        if text.eq_ignore_ascii_case("any") {
            return Ok(ForwardSpinLineRequirement::Any);
        }
        let (line_text, at_least) = text
            .strip_suffix('+')
            .map_or((text, false), |minimum| (minimum, true));
        let lines = line_text
            .parse::<u8>()
            .ok()
            .filter(|lines| *lines <= 4)
            .ok_or_else(|| {
                DesktopTauriCommandError::invalid_request(
                    "desktop spin_lines must be any, 0..4, or 0+..4+",
                )
            })?;
        Ok(if at_least {
            ForwardSpinLineRequirement::AtLeast(lines)
        } else {
            ForwardSpinLineRequirement::Exact(lines)
        })
    }

    fn parse_spin_category(value: &str) -> Result<ForwardSpinCategory, DesktopTauriCommandError> {
        match value.to_ascii_lowercase().as_str() {
            "any" => Ok(ForwardSpinCategory::Any),
            "t" | "t-piece" => Ok(ForwardSpinCategory::T),
            "other" | "non-t" => Ok(ForwardSpinCategory::Other),
            _ => Err(DesktopTauriCommandError::invalid_request(format!(
                "invalid desktop spin_category '{value}'"
            ))),
        }
    }

    fn parse_hold_piece_kind(
        value: Option<&Value>,
    ) -> Result<Option<PieceKind>, DesktopTauriCommandError> {
        let Some(value) = value else {
            return Ok(None);
        };
        let value = value.as_str().ok_or_else(|| {
            DesktopTauriCommandError::invalid_request("desktop hold_piece must be a string")
        })?;
        if value.is_empty() || matches!(value.to_ascii_lowercase().as_str(), "empty" | "none") {
            return Ok(None);
        }
        let mut characters = value.chars();
        let piece = characters.next().ok_or_else(|| {
            DesktopTauriCommandError::invalid_request("desktop hold_piece must not be empty")
        })?;
        if characters.next().is_some() {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop hold_piece must be empty or one tetromino letter",
            ));
        }
        PieceKind::from_ascii(piece).map(Some).map_err(|_| {
            DesktopTauriCommandError::invalid_request(
                "desktop hold_piece must be empty or one of I, O, T, S, Z, J, L",
            )
        })
    }

    fn parse_board_words(
        value: Option<&Value>,
        field_name: &str,
    ) -> Result<[u64; 4], DesktopTauriCommandError> {
        let value = value.ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(format!(
                "desktop request requires {field_name}"
            ))
        })?;
        if let Some(number) = value.as_u64() {
            return Ok([number, 0, 0, 0]);
        }
        if let Some(words) = value.as_array() {
            if words.len() > 4 {
                return Err(invalid_board_words(field_name));
            }
            let mut result = [0_u64; 4];
            for (index, word) in words.iter().enumerate() {
                result[index] =
                    parse_board_word(word).ok_or_else(|| invalid_board_words(field_name))?;
            }
            return Ok(result);
        }
        let text = value
            .as_str()
            .ok_or_else(|| invalid_board_words(field_name))?;
        parse_board_words_text(text).ok_or_else(|| invalid_board_words(field_name))
    }

    fn parse_board_word(value: &Value) -> Option<u64> {
        if let Some(number) = value.as_u64() {
            return Some(number);
        }
        let words = parse_board_words_text(value.as_str()?)?;
        (words[1..] == [0, 0, 0]).then_some(words[0])
    }

    fn parse_board_words_text(value: &str) -> Option<[u64; 4]> {
        if let Some(hex) = value
            .strip_prefix("0x")
            .or_else(|| value.strip_prefix("0X"))
        {
            if hex.is_empty() || hex.len() > 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return None;
            }
            let mut words = [0_u64; 4];
            for (index, chunk_end) in (0..hex.len()).rev().step_by(16).enumerate() {
                let begin = chunk_end.saturating_sub(15);
                words[index] = u64::from_str_radix(&hex[begin..=chunk_end], 16).ok()?;
            }
            return Some(words);
        }
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let mut words = [0_u64; 4];
        for digit in value.bytes().map(|byte| u64::from(byte - b'0')) {
            let mut carry = u128::from(digit);
            for word in &mut words {
                let next = u128::from(*word) * 10 + carry;
                *word = next as u64;
                carry = next >> 64;
            }
            if carry != 0 {
                return None;
            }
        }
        Some(words)
    }

    fn invalid_board_words(field_name: &str) -> DesktopTauriCommandError {
        DesktopTauriCommandError::invalid_request(format!(
            "desktop {field_name} must be a 256-bit number, hex string, or four-word array"
        ))
    }

    fn ensure_words_fit_height(
        words: [u64; 4],
        height: u8,
        field_name: &str,
    ) -> Result<(), DesktopTauriCommandError> {
        let cells = usize::from(height) * 10;
        let full_words = cells / 64;
        let remaining_bits = cells % 64;
        for (index, word) in words.into_iter().enumerate() {
            let allowed = if index < full_words {
                u64::MAX
            } else if index == full_words && remaining_bits > 0 {
                (1_u64 << remaining_bits) - 1
            } else {
                0
            };
            if word & !allowed != 0 {
                return Err(DesktopTauriCommandError::invalid_request(format!(
                    "desktop {field_name} contains cells above visible_height"
                )));
            }
        }
        Ok(())
    }

    fn optional_value<'a>(
        value: &'a Value,
        keys: &[&str],
    ) -> Result<Option<&'a Value>, DesktopTauriCommandError> {
        let mut found = None;
        for key in keys {
            let Some(entry) = value.get(*key) else {
                continue;
            };
            if found.is_some() {
                return Err(DesktopTauriCommandError::invalid_request(format!(
                    "desktop {} aliases are mutually exclusive",
                    keys[0]
                )));
            }
            found = Some(entry);
        }
        Ok(found)
    }

    fn required_text<'a>(
        value: &'a Value,
        keys: &[&str],
        field_name: &str,
    ) -> Result<&'a str, DesktopTauriCommandError> {
        optional_nonempty_text(value, keys)?.ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(format!(
                "desktop request requires {field_name}"
            ))
        })
    }

    fn text_or_default<'a>(
        value: &'a Value,
        keys: &[&str],
        default: &'a str,
    ) -> Result<&'a str, DesktopTauriCommandError> {
        Ok(optional_text(value, keys)?.unwrap_or(default))
    }

    fn optional_text<'a>(
        value: &'a Value,
        keys: &[&str],
    ) -> Result<Option<&'a str>, DesktopTauriCommandError> {
        optional_typed(value, keys, "a string", Value::as_str)
    }

    fn optional_nonempty_text<'a>(
        value: &'a Value,
        keys: &[&str],
    ) -> Result<Option<&'a str>, DesktopTauriCommandError> {
        Ok(optional_text(value, keys)?
            .map(str::trim)
            .filter(|value| !value.is_empty()))
    }

    fn bool_or_default(
        value: &Value,
        keys: &[&str],
        default: bool,
    ) -> Result<bool, DesktopTauriCommandError> {
        Ok(optional_bool(value, keys)?.unwrap_or(default))
    }

    fn optional_bool(
        value: &Value,
        keys: &[&str],
    ) -> Result<Option<bool>, DesktopTauriCommandError> {
        optional_typed(value, keys, "a boolean", Value::as_bool)
    }

    fn optional_typed<'a, T>(
        value: &'a Value,
        keys: &[&str],
        expected: &str,
        parse: impl FnOnce(&'a Value) -> Option<T>,
    ) -> Result<Option<T>, DesktopTauriCommandError> {
        let Some(entry) = optional_value(value, keys)? else {
            return Ok(None);
        };
        parse(entry).map(Some).ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(format!(
                "desktop {} must be {expected}",
                keys[0]
            ))
        })
    }

    fn optional_u64_any(
        value: &Value,
        keys: &[&str],
    ) -> Result<Option<u64>, DesktopTauriCommandError> {
        optional_typed(value, keys, "a nonnegative integer", Value::as_u64)
    }

    fn optional_u8_any(
        value: &Value,
        keys: &[&str],
    ) -> Result<Option<u8>, DesktopTauriCommandError> {
        optional_u64_any(value, keys)?.map_or(Ok(None), |number| {
            u8::try_from(number).map(Some).map_err(|_| {
                DesktopTauriCommandError::invalid_request(format!(
                    "desktop {} must fit in u8",
                    keys[0]
                ))
            })
        })
    }

    fn optional_u16_any(
        value: &Value,
        keys: &[&str],
    ) -> Result<Option<u16>, DesktopTauriCommandError> {
        optional_u64_any(value, keys)?.map_or(Ok(None), |number| {
            u16::try_from(number).map(Some).map_err(|_| {
                DesktopTauriCommandError::invalid_request(format!(
                    "desktop {} must fit in u16",
                    keys[0]
                ))
            })
        })
    }

    fn optional_u32_any(
        value: &Value,
        keys: &[&str],
    ) -> Result<Option<u32>, DesktopTauriCommandError> {
        optional_u64_any(value, keys)?.map_or(Ok(None), |number| {
            u32::try_from(number).map(Some).map_err(|_| {
                DesktopTauriCommandError::invalid_request(format!(
                    "desktop {} must fit in u32",
                    keys[0]
                ))
            })
        })
    }

    fn optional_usize_any(
        value: &Value,
        keys: &[&str],
    ) -> Result<Option<usize>, DesktopTauriCommandError> {
        optional_u64_any(value, keys)?.map_or(Ok(None), |number| {
            usize::try_from(number).map(Some).map_err(|_| {
                DesktopTauriCommandError::invalid_request(format!(
                    "desktop {} must fit in usize",
                    keys[0]
                ))
            })
        })
    }

    fn required_u16(
        value: &Value,
        keys: &[&str],
        field_name: &str,
    ) -> Result<u16, DesktopTauriCommandError> {
        optional_u16_any(value, keys)?.ok_or_else(|| {
            DesktopTauriCommandError::invalid_request(format!(
                "desktop request requires {field_name}"
            ))
        })
    }

    fn parse_board_mask(value: Option<&Value>) -> Result<u64, DesktopTauriCommandError> {
        let value = value.ok_or_else(|| {
            DesktopTauriCommandError::invalid_request("desktop scenario PC requires board_mask")
        })?;
        if let Some(number) = value.as_u64() {
            return Ok(number);
        }
        let text = value.as_str().ok_or_else(|| {
            DesktopTauriCommandError::invalid_request("board_mask must be a number or hex string")
        })?;
        let parsed = text
            .strip_prefix("0x")
            .or_else(|| text.strip_prefix("0X"))
            .map(|hex| u64::from_str_radix(hex, 16))
            .unwrap_or_else(|| text.parse::<u64>());
        parsed.map_err(|error| {
            DesktopTauriCommandError::invalid_request(format!("invalid board_mask: {error}"))
        })
    }

    fn parse_hold_piece(value: Option<&Value>) -> Result<Option<char>, DesktopTauriCommandError> {
        let Some(value) = value else {
            return Ok(None);
        };
        let value = value.as_str().ok_or_else(|| {
            DesktopTauriCommandError::invalid_request("hold_piece must be a string")
        })?;
        if matches!(value, "empty" | "none") {
            return Ok(None);
        }
        let mut characters = value.chars();
        let piece = characters.next().ok_or_else(|| {
            DesktopTauriCommandError::invalid_request("hold_piece must not be empty")
        })?;
        if characters.next().is_some() || !"IOTSZJL".contains(piece) {
            return Err(DesktopTauriCommandError::invalid_request(
                "hold_piece must be empty or one of I, O, T, S, Z, J, L",
            ));
        }
        Ok(Some(piece))
    }

    fn optional_u16(value: &Value, key: &str) -> Result<Option<u16>, DesktopTauriCommandError> {
        optional_u16_any(value, &[key])
    }

    fn optional_u32(value: &Value, key: &str) -> Result<Option<u32>, DesktopTauriCommandError> {
        optional_u32_any(value, &[key])
    }

    #[derive(Clone, Copy)]
    enum CanonicalInactiveValue {
        Text(&'static str),
        Bool(bool),
        U64(u64),
        Null,
        ZeroMask,
        Absent,
    }

    impl CanonicalInactiveValue {
        fn matches(self, value: &Value) -> bool {
            match self {
                Self::Text(expected) => value.as_str() == Some(expected),
                Self::Bool(expected) => value.as_bool() == Some(expected),
                Self::U64(expected) => value.as_u64() == Some(expected),
                Self::Null => value.is_null(),
                Self::ZeroMask => is_zero_mask(value),
                Self::Absent => false,
            }
        }
    }

    struct InactiveFieldRule {
        key: &'static str,
        canonical: CanonicalInactiveValue,
        active_commands: &'static [&'static str],
    }

    const PC_COMMANDS: &[&str] = &["pc", "pc-scenario"];
    const PC_SCENARIO_COMMAND: &[&str] = &["pc-scenario"];
    const SETUP_COMMAND: &[&str] = &["setup"];
    const BUILD_COMMAND: &[&str] = &["build-probability"];
    const DAMAGE_COMMAND: &[&str] = &["damage"];
    const SPIN_FINDER_COMMAND: &[&str] = &["spin-finder"];
    const SPIN_PROFILE_COMMANDS: &[&str] = &[
        "pc",
        "pc-scenario",
        "build-probability",
        "damage",
        "spin-finder",
        "ren",
    ];
    const PC_AND_SETUP_COMMANDS: &[&str] = &["pc", "pc-scenario", "setup"];
    const PC_AND_BUILD_COMMANDS: &[&str] = &["pc", "pc-scenario", "build-probability"];
    const PC_SCENARIO_AND_BUILD_COMMANDS: &[&str] = &["pc-scenario", "build-probability"];
    const PC_BUILD_FORWARD_COMMANDS: &[&str] = &[
        "pc",
        "pc-scenario",
        "build-probability",
        "damage",
        "spin-finder",
    ];
    const PC_BUILD_ALL_FORWARD_COMMANDS: &[&str] = &[
        "pc",
        "pc-scenario",
        "build-probability",
        "damage",
        "spin-finder",
        "ren",
    ];
    const PC_SCENARIO_AND_FORWARD_COMMANDS: &[&str] =
        &["pc-scenario", "damage", "spin-finder", "ren"];
    const SCENARIO_BUILD_FORWARD_COMMANDS: &[&str] = &[
        "pc-scenario",
        "build-probability",
        "damage",
        "spin-finder",
        "ren",
    ];
    const PC_AND_DAMAGE_COMMANDS: &[&str] = &["pc", "pc-scenario", "damage"];
    const ALL_NON_SETUP_COMMANDS: &[&str] = &[
        "pc",
        "pc-scenario",
        "build-probability",
        "damage",
        "spin-finder",
        "ren",
    ];

    const INACTIVE_FIELD_RULES: &[InactiveFieldRule] = &[
        InactiveFieldRule {
            key: "lines",
            canonical: CanonicalInactiveValue::U64(2),
            active_commands: PC_COMMANDS,
        },
        InactiveFieldRule {
            key: "queue",
            canonical: CanonicalInactiveValue::Text(""),
            active_commands: ALL_NON_SETUP_COMMANDS,
        },
        InactiveFieldRule {
            key: "patterns",
            canonical: CanonicalInactiveValue::Text(""),
            active_commands: ALL_NON_SETUP_COMMANDS,
        },
        InactiveFieldRule {
            key: "queue_knowledge",
            canonical: CanonicalInactiveValue::Text("oracle"),
            active_commands: PC_AND_SETUP_COMMANDS,
        },
        InactiveFieldRule {
            key: "hold_enabled",
            canonical: CanonicalInactiveValue::Bool(true),
            active_commands: PC_BUILD_ALL_FORWARD_COMMANDS,
        },
        InactiveFieldRule {
            key: "hold_piece",
            canonical: CanonicalInactiveValue::Text("empty"),
            active_commands: PC_SCENARIO_AND_BUILD_COMMANDS,
        },
        InactiveFieldRule {
            key: "score_mode",
            canonical: CanonicalInactiveValue::Text("off"),
            active_commands: PC_COMMANDS,
        },
        InactiveFieldRule {
            key: "score_profile",
            canonical: CanonicalInactiveValue::Text("tetrio"),
            active_commands: PC_COMMANDS,
        },
        InactiveFieldRule {
            key: "spin_profile",
            canonical: CanonicalInactiveValue::Text("t-spins"),
            active_commands: SPIN_PROFILE_COMMANDS,
        },
        InactiveFieldRule {
            key: "preserve_b2b",
            canonical: CanonicalInactiveValue::Bool(false),
            active_commands: PC_BUILD_FORWARD_COMMANDS,
        },
        InactiveFieldRule {
            key: "precompute_build_dependencies",
            canonical: CanonicalInactiveValue::Bool(false),
            active_commands: PC_AND_BUILD_COMMANDS,
        },
        InactiveFieldRule {
            key: "finesse",
            canonical: CanonicalInactiveValue::Text("off"),
            active_commands: PC_AND_BUILD_COMMANDS,
        },
        InactiveFieldRule {
            key: "pattern_knowledge",
            canonical: CanonicalInactiveValue::Text("both"),
            active_commands: PC_AND_BUILD_COMMANDS,
        },
        InactiveFieldRule {
            key: "initial_b2b",
            canonical: CanonicalInactiveValue::U64(0),
            active_commands: PC_AND_DAMAGE_COMMANDS,
        },
        InactiveFieldRule {
            key: "board_mask",
            canonical: CanonicalInactiveValue::ZeroMask,
            active_commands: PC_SCENARIO_AND_FORWARD_COMMANDS,
        },
        InactiveFieldRule {
            key: "visible_height",
            canonical: CanonicalInactiveValue::U64(2),
            active_commands: SCENARIO_BUILD_FORWARD_COMMANDS,
        },
        InactiveFieldRule {
            key: "piece_window",
            canonical: CanonicalInactiveValue::Null,
            active_commands: PC_SCENARIO_COMMAND,
        },
        InactiveFieldRule {
            key: "count_policy",
            canonical: CanonicalInactiveValue::Text("unique"),
            active_commands: PC_COMMANDS,
        },
        InactiveFieldRule {
            key: "solution_probabilities",
            canonical: CanonicalInactiveValue::Bool(false),
            active_commands: PC_AND_BUILD_COMMANDS,
        },
        InactiveFieldRule {
            key: "gpu_device",
            canonical: CanonicalInactiveValue::Text("auto"),
            active_commands: PC_AND_BUILD_COMMANDS,
        },
        InactiveFieldRule {
            key: "memory_budget_mb",
            canonical: CanonicalInactiveValue::U64(0),
            active_commands: PC_AND_BUILD_COMMANDS,
        },
        InactiveFieldRule {
            key: "candidate_budget",
            canonical: CanonicalInactiveValue::U64(10_000_000),
            active_commands: PC_AND_BUILD_COMMANDS,
        },
        InactiveFieldRule {
            key: "pattern_budget",
            canonical: CanonicalInactiveValue::U64(5040),
            active_commands: PC_AND_BUILD_COMMANDS,
        },
        InactiveFieldRule {
            key: "tablebase_requested",
            canonical: CanonicalInactiveValue::Bool(false),
            active_commands: PC_AND_SETUP_COMMANDS,
        },
        InactiveFieldRule {
            key: "cpu_warmup",
            canonical: CanonicalInactiveValue::Bool(false),
            active_commands: BUILD_COMMAND,
        },
        InactiveFieldRule {
            key: "gpu_warmup",
            canonical: CanonicalInactiveValue::Bool(false),
            active_commands: BUILD_COMMAND,
        },
        InactiveFieldRule {
            key: "setup_mode",
            canonical: CanonicalInactiveValue::Text("oracle"),
            active_commands: SETUP_COMMAND,
        },
        InactiveFieldRule {
            key: "setup_remaining",
            canonical: CanonicalInactiveValue::Text("IOTSZJL"),
            active_commands: SETUP_COMMAND,
        },
        InactiveFieldRule {
            key: "setup_qb",
            canonical: CanonicalInactiveValue::Text(""),
            active_commands: SETUP_COMMAND,
        },
        InactiveFieldRule {
            key: "setup_next_cycle_remaining",
            canonical: CanonicalInactiveValue::Text(""),
            active_commands: SETUP_COMMAND,
        },
        InactiveFieldRule {
            key: "setup_allow_post_cycle_borrow",
            canonical: CanonicalInactiveValue::Bool(false),
            active_commands: SETUP_COMMAND,
        },
        InactiveFieldRule {
            key: "setup_priority",
            canonical: CanonicalInactiveValue::Text("all"),
            active_commands: SETUP_COMMAND,
        },
        InactiveFieldRule {
            key: "setup_length",
            canonical: CanonicalInactiveValue::Text("auto"),
            active_commands: SETUP_COMMAND,
        },
        InactiveFieldRule {
            key: "setup_max_pieces",
            canonical: CanonicalInactiveValue::U64(9),
            active_commands: SETUP_COMMAND,
        },
        InactiveFieldRule {
            key: "setup_path_setup_id",
            canonical: CanonicalInactiveValue::Absent,
            active_commands: SETUP_COMMAND,
        },
        InactiveFieldRule {
            key: "setup_path_condition_id",
            canonical: CanonicalInactiveValue::Absent,
            active_commands: SETUP_COMMAND,
        },
        InactiveFieldRule {
            key: "base_mask",
            canonical: CanonicalInactiveValue::ZeroMask,
            active_commands: BUILD_COMMAND,
        },
        InactiveFieldRule {
            key: "target_mask",
            canonical: CanonicalInactiveValue::ZeroMask,
            active_commands: BUILD_COMMAND,
        },
        InactiveFieldRule {
            key: "build_aggregation",
            canonical: CanonicalInactiveValue::Text("buildability"),
            active_commands: BUILD_COMMAND,
        },
        InactiveFieldRule {
            key: "include_horizontal_mirror",
            canonical: CanonicalInactiveValue::Bool(true),
            active_commands: BUILD_COMMAND,
        },
        InactiveFieldRule {
            key: "initial_combo",
            canonical: CanonicalInactiveValue::U64(0),
            active_commands: DAMAGE_COMMAND,
        },
        InactiveFieldRule {
            key: "damage_aggregation",
            canonical: CanonicalInactiveValue::Text("maximum"),
            active_commands: DAMAGE_COMMAND,
        },
        InactiveFieldRule {
            key: "minimum_damage",
            canonical: CanonicalInactiveValue::U64(0),
            active_commands: DAMAGE_COMMAND,
        },
        InactiveFieldRule {
            key: "spin_lines",
            canonical: CanonicalInactiveValue::Text("any"),
            active_commands: SPIN_FINDER_COMMAND,
        },
        InactiveFieldRule {
            key: "spin_category",
            canonical: CanonicalInactiveValue::Text("any"),
            active_commands: SPIN_FINDER_COMMAND,
        },
    ];

    fn validate_command_inactive_fields(
        value: &Value,
        command: &str,
    ) -> Result<(), DesktopTauriCommandError> {
        for rule in INACTIVE_FIELD_RULES {
            if rule.active_commands.contains(&command) {
                continue;
            }
            let Some(actual) = value.get(rule.key) else {
                continue;
            };
            if !rule.canonical.matches(actual) {
                return Err(inactive_field_error(command, rule.key));
            }
        }
        if !PC_AND_BUILD_COMMANDS.contains(&command) {
            validate_canonical_inactive_backend(value, command)?;
        }
        Ok(())
    }

    fn validate_canonical_inactive_backend(
        value: &Value,
        command: &str,
    ) -> Result<(), DesktopTauriCommandError> {
        let backend = value
            .get("backend")
            .map(Value::as_str)
            .unwrap_or(Some("auto"));
        let default_fallback = backend == Some("auto");
        let fallback = value
            .get("allow_backend_fallback")
            .map(Value::as_bool)
            .unwrap_or(Some(default_fallback));
        if !matches!(
            (backend, fallback),
            (Some("auto"), Some(true)) | (Some("cpu"), Some(false))
        ) {
            let field = if !matches!(backend, Some("auto" | "cpu")) {
                "backend"
            } else {
                "allow_backend_fallback"
            };
            return Err(inactive_field_error(command, field));
        }
        Ok(())
    }

    fn inactive_field_error(command: &str, key: &str) -> DesktopTauriCommandError {
        DesktopTauriCommandError::invalid_request(format!(
            "desktop {command} inactive field '{key}' must keep its canonical default"
        ))
    }

    fn is_zero_mask(value: &Value) -> bool {
        if value.as_u64() == Some(0) {
            return true;
        }
        let Some(text) = value.as_str() else {
            return false;
        };
        let digits = text
            .strip_prefix("0x")
            .or_else(|| text.strip_prefix("0X"))
            .unwrap_or(text);
        !digits.is_empty() && digits.bytes().all(|digit| digit == b'0')
    }

    fn validate_app_request_envelope(value: &Value) -> Result<(), DesktopTauriCommandError> {
        let model = text_or_default(value, &["app_request_model"], "clearra-app/AppRequest")?;
        if model != "clearra-app/AppRequest" {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop host accepts only clearra-app/AppRequest JSON",
            ));
        }
        if value.get("cli_text").is_some() || value.get("command_text").is_some() {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop host does not parse CLI text",
            ));
        }
        let command = text_or_default(value, &["command"], "pc")?;
        if !matches!(
            command,
            "pc" | "pc-scenario"
                | "setup"
                | "setup-score"
                | "build-probability"
                | "build-v2"
                | "spin-structure"
                | "damage"
                | "spin-finder"
                | "ren"
                | "utility-sequence"
                | "utility-sequence-dependencies"
                | "utility-parity"
                | "utility-fumen"
                | "utility-render"
                | "utility-to-gray"
                | "utility-mirror"
        ) {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop request bridge supports pc, pc-scenario, setup, setup-score, build-probability, build-v2, spin-structure, damage, spin-finder, ren, utility-sequence, utility-sequence-dependencies, utility-parity, utility-fumen, utility-render, utility-to-gray, and utility-mirror commands",
            ));
        }
        if matches!(
            command,
            "build-v2"
                | "spin-structure"
                | "setup-score"
                | "utility-sequence"
                | "utility-sequence-dependencies"
                | "utility-parity"
                | "utility-fumen"
                | "utility-render"
                | "utility-to-gray"
                | "utility-mirror"
        ) {
            // The operation-document command has its own closed allowlist;
            // generic search defaults are not members of this JSON contract.
            Ok(())
        } else {
            validate_command_inactive_fields(value, command)
        }
    }
}
mod get_job_events {
    use crate::{GuiJobEvent, GuiJobId};
    use clearra_app::ProductPageStore;

    use super::{
        bridge::DesktopTauriCommandBridge, error::DesktopTauriCommandError,
        job_event_json::job_events_to_json,
    };

    impl DesktopTauriCommandBridge {
        pub fn drain_job_events(
            &mut self,
            job_id: u64,
        ) -> Result<Vec<GuiJobEvent>, DesktopTauriCommandError> {
            if self.active_job_id.map(GuiJobId::get) != Some(job_id) {
                return Err(DesktopTauriCommandError::job("desktop active job mismatch"));
            }
            let handle = self
                .active_job
                .as_ref()
                .ok_or_else(|| DesktopTauriCommandError::job("desktop job handle missing"))?;
            let mut events = handle.drain_events();
            let worker_finished = handle.is_finished();
            let terminal_received = events.iter().any(GuiJobEvent::is_terminal);

            if worker_finished || terminal_received {
                let handle = self.active_job.take().ok_or_else(|| {
                    DesktopTauriCommandError::job("desktop job handle missing during completion")
                })?;
                match handle.join_with_events() {
                    Ok((_result, trailing_events)) => events.extend(trailing_events),
                    Err(_) => events.push(GuiJobEvent::Failed {
                        job_id: GuiJobId::new(job_id),
                        code: "desktop-worker-panicked".to_owned(),
                    }),
                }
                if !events.iter().any(GuiJobEvent::is_terminal) {
                    events.push(GuiJobEvent::Failed {
                        job_id: GuiJobId::new(job_id),
                        code: "desktop-worker-ended-without-terminal-event".to_owned(),
                    });
                }
                self.queue
                    .finish(GuiJobId::new(job_id))
                    .map_err(|error| DesktopTauriCommandError::job(error.to_string()))?;
                self.active_job_id = None;
            }

            for event in &mut events {
                if let GuiJobEvent::Completed {
                    product_page_source_owner,
                    ..
                } = event
                {
                    if let Some(source) = product_page_source_owner.take() {
                        self.product_page_store =
                            Some(ProductPageStore::from_source(source).map_err(|error| {
                                DesktopTauriCommandError::job(format!(
                                    "open desktop product page store: {}",
                                    error.as_str()
                                ))
                            })?);
                    }
                }
            }

            Ok(events)
        }

        pub fn get_job_events(&mut self, job_id: u64) -> Result<String, DesktopTauriCommandError> {
            let events = self.drain_job_events(job_id)?;
            job_events_to_json(&events).map_err(|error| {
                DesktopTauriCommandError::job(format!("serialize desktop job events: {error}"))
            })
        }
    }
}
mod job_event_json {
    use serde::ser::{Serialize, SerializeMap, Serializer};
    use serde_json::value::RawValue;

    use crate::GuiJobEvent;

    pub(super) fn job_events_to_json(events: &[GuiJobEvent]) -> Result<String, serde_json::Error> {
        let serializable = events.iter().map(JobEventJson).collect::<Vec<_>>();
        serde_json::to_string(&serializable)
    }

    struct JobEventJson<'a>(&'a GuiJobEvent);

    impl Serialize for JobEventJson<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut map = serializer.serialize_map(None)?;
            map.serialize_entry("schema_version", &1_u8)?;
            match self.0 {
                GuiJobEvent::Started { job_id } => {
                    map.serialize_entry("event", "started")?;
                    map.serialize_entry("job_id", &job_id.get())?;
                }
                GuiJobEvent::Progress { job_id, progress } => {
                    map.serialize_entry("event", "progress")?;
                    map.serialize_entry("job_id", &job_id.get())?;
                    map.serialize_entry("done", &progress.done())?;
                    map.serialize_entry("total", &progress.total())?;
                    map.serialize_entry("label", progress.label())?;
                    map.serialize_entry("budget_status", progress.budget_status().state())?;
                    map.serialize_entry(
                        "resource_status",
                        &serde_json::json!({
                            "budget_status": progress.budget_status().state(),
                            "done": progress.done(),
                            "total": progress.total()
                        }),
                    )?;
                    map.serialize_entry(
                        "backend_status",
                        &serde_json::json!({
                            "backend_requested": progress.backend_status().backend_requested(),
                            "backend_selected": progress.backend_status().backend_selected(),
                            "fallback_used": progress.backend_status().fallback_used()
                        }),
                    )?;
                    map.serialize_entry(
                        "memory_status",
                        &serde_json::json!({
                            "state": progress.memory_status().state(),
                            "leak_report_clean": progress.memory_status().leak_report_clean(),
                            "raw_pointer_exposed": progress.memory_status().raw_pointer_exposed()
                        }),
                    )?;
                }
                GuiJobEvent::Diagnostic {
                    job_id,
                    code,
                    severity,
                } => {
                    map.serialize_entry("event", "diagnostic")?;
                    map.serialize_entry("job_id", &job_id.get())?;
                    map.serialize_entry("code", code)?;
                    map.serialize_entry("severity", severity)?;
                }
                GuiJobEvent::Completed {
                    job_id,
                    response,
                    search_report_json,
                    product_page_source_owner: _,
                } => {
                    map.serialize_entry("event", "completed")?;
                    map.serialize_entry("job_id", &job_id.get())?;
                    map.serialize_entry("response", response)?;
                    let search_report = search_report_json
                        .as_deref()
                        .and_then(|value| serde_json::from_str::<&RawValue>(value).ok());
                    map.serialize_entry("search_report", &search_report)?;
                }
                GuiJobEvent::Failed { job_id, code } => {
                    map.serialize_entry("event", "failed")?;
                    map.serialize_entry("job_id", &job_id.get())?;
                    map.serialize_entry("code", code)?;
                }
                GuiJobEvent::Cancelled { job_id } => {
                    map.serialize_entry("event", "cancelled")?;
                    map.serialize_entry("job_id", &job_id.get())?;
                    map.serialize_entry("scope_released", &true)?;
                }
            }
            map.end()
        }
    }
}
mod product_pages {
    use clearra_app::{
        CoveragePortfolioPageStore, PcReplayPageAdvance, PcReplayPageStore,
        PortfolioAlternativeAdvance, PortfolioPageLoadAdvance, PortfolioPageLoadState,
        ProductPageStore,
    };
    use clearra_host_contract::ParityReportPagePayload;

    use super::{bridge::DesktopTauriCommandBridge, error::DesktopTauriCommandError};

    // Small pending/error JSON, cancellation control and scalar carriers.
    // App independently reserves its actual public page with the existing 16x
    // projection policy; this reserve never replaces that page admission.
    const PC_REPLAY_HOST_ENVELOPE_RESERVE: u128 = 4096;

    impl DesktopTauriCommandBridge {
        pub fn product_page_next(
            &mut self,
            maximum_work_steps: u64,
        ) -> Result<String, DesktopTauriCommandError> {
            self.product_page_next_with_cancel(maximum_work_steps, &mut || false)
        }

        pub fn product_page_next_with_cancel(
            &mut self,
            maximum_work_steps: u64,
            cancelled: &mut impl FnMut() -> bool,
        ) -> Result<String, DesktopTauriCommandError> {
            if let Some(store) = self
                .product_page_store
                .as_mut()
                .and_then(ProductPageStore::parity_report_mut)
            {
                let next = store.next_page().map_err(|error| {
                    DesktopTauriCommandError::job(format!(
                        "advance desktop parity page: {}",
                        error.as_str()
                    ))
                })?;
                return next.map_or_else(parity_exhausted_json, parity_page_json);
            }
            let (advance, retained_slot) = {
                let store = coverage_store_mut(self)?;
                let advance = store
                    .next_page(maximum_work_steps.max(1), cancelled)
                    .map_err(|error| {
                        DesktopTauriCommandError::job(format!(
                            "advance desktop product page: {}",
                            error.as_str()
                        ))
                    })?;
                let retained_slot = advance
                    .page()
                    .and_then(|page| store.retained_page_slot(page.alternative_index_decimal()));
                (advance, retained_slot)
            };
            if let Some(retained_slot) = retained_slot {
                return page_json(coverage_store(self)?, retained_slot, 1);
            }
            advance_json(&advance)
        }

        pub fn product_page_get(
            &mut self,
            alternative_index_decimal: &str,
            member_page_number_decimal: &str,
        ) -> Result<String, DesktopTauriCommandError> {
            self.product_page_get_with_cancel(
                alternative_index_decimal,
                member_page_number_decimal,
                &mut || false,
            )
        }

        pub fn product_page_get_with_cancel(
            &mut self,
            alternative_index_decimal: &str,
            member_page_number_decimal: &str,
            cancelled: &mut impl FnMut() -> bool,
        ) -> Result<String, DesktopTauriCommandError> {
            if self
                .product_page_store
                .as_ref()
                .and_then(ProductPageStore::pc_replay)
                .is_some()
            {
                // Compatibility callers receive one bounded pending/page reply,
                // not a synchronous loop hidden behind the legacy getter.
                return self.product_page_get_slice_with_cancel(
                    alternative_index_decimal,
                    member_page_number_decimal,
                    64,
                    cancelled,
                );
            }
            let member_page_number = parse_canonical_positive_usize(
                member_page_number_decimal,
                "desktop member page number",
            )?;
            if let Some(store) = self
                .product_page_store
                .as_ref()
                .and_then(ProductPageStore::parity_report)
            {
                if member_page_number != 1 {
                    return Err(DesktopTauriCommandError::job(
                        "desktop parity pages have exactly one member page",
                    ));
                }
                let outer_page_number = parse_canonical_positive_usize(
                    alternative_index_decimal,
                    "desktop parity alternative index",
                )?;
                let page = store.page(outer_page_number).map_err(|error| {
                    DesktopTauriCommandError::job(format!(
                        "load desktop parity page: {}",
                        error.as_str()
                    ))
                })?;
                return parity_page_json(page);
            }
            let retained_slot = coverage_store_mut(self)?
                .load_page_by_alternative_index(alternative_index_decimal, cancelled)
                .map_err(|error| {
                    DesktopTauriCommandError::job(format!(
                        "load desktop product page: {}",
                        error.as_str()
                    ))
                })?;
            page_json(coverage_store(self)?, retained_slot, member_page_number)
        }

        pub fn product_page_get_slice_with_cancel(
            &mut self,
            alternative_index_decimal: &str,
            member_page_number_decimal: &str,
            maximum_work_steps: u64,
            cancelled: &mut impl FnMut() -> bool,
        ) -> Result<String, DesktopTauriCommandError> {
            let member_page_number = parse_canonical_positive_usize(
                member_page_number_decimal,
                "desktop member page number",
            )?;
            if let Some(store) = self
                .product_page_store
                .as_mut()
                .and_then(ProductPageStore::pc_replay_mut)
            {
                let geometry_page_number = parse_canonical_positive_usize(
                    alternative_index_decimal,
                    "desktop replay geometry page number",
                )?;
                let work = maximum_work_steps.clamp(1, 64) as usize;
                let maximum = store.source().maximum_memory_bytes();
                let transport_bytes = PC_REPLAY_HOST_ENVELOPE_RESERVE
                    .checked_add(alternative_index_decimal.len() as u128)
                    .and_then(|n| n.checked_add(member_page_number_decimal.len() as u128))
                    .ok_or_else(|| {
                        DesktopTauriCommandError::job("desktop replay transport size overflow")
                    })?;
                let entry = store
                    .checked_host_entry_bytes()
                    .and_then(|n| n.checked_add(transport_bytes))
                    .ok_or_else(|| {
                        DesktopTauriCommandError::job("desktop replay entry size overflow")
                    })?;
                if entry > maximum {
                    return Err(DesktopTauriCommandError::job(format!(
                        "complete_replay_host_memory_limit_exceeded: required_memory_bytes={entry}, max_memory_bytes={maximum}",
                    )));
                }
                let control =
                    clearra_core_domain::execution_cancellation::ExecutionControl::default();
                if cancelled() {
                    control.cancellation.handle().cancel();
                }
                let mut rejected_host_peak = None;
                let mut advance = store
                    .advance_page_with_memory_guard(
                        geometry_page_number,
                        member_page_number,
                        work,
                        &control,
                        &mut |app_whole_live| {
                            let required = app_whole_live.checked_add(transport_bytes);
                            let admitted = required.is_some_and(|n| n <= maximum);
                            if !admitted {
                                rejected_host_peak = required;
                            }
                            admitted
                        },
                    )
                    .map_err(|error| match rejected_host_peak {
                        Some(required) => DesktopTauriCommandError::job(format!(
                            "{}: required_memory_bytes={required}, max_memory_bytes={maximum}",
                            error.code(),
                        )),
                        None => DesktopTauriCommandError::job(format!(
                            "load desktop replay page slice: {error}"
                        )),
                    })?;
                // The operation token can be cancelled from another thread
                // while this bounded slice runs. Never publish its late page.
                if cancelled() {
                    store.cancel_page();
                    advance = PcReplayPageAdvance::Cancelled { work_steps: 0 };
                }
                return pc_replay_advance_json(
                    store,
                    &advance,
                    geometry_page_number,
                    member_page_number,
                );
            }
            if self
                .product_page_store
                .as_ref()
                .and_then(ProductPageStore::parity_report)
                .is_some()
            {
                return self.product_page_get_with_cancel(
                    alternative_index_decimal,
                    member_page_number_decimal,
                    cancelled,
                );
            }
            let advance = coverage_store_mut(self)?
                .load_page_by_alternative_index_slice(
                    alternative_index_decimal,
                    maximum_work_steps.max(1),
                    cancelled,
                )
                .map_err(|error| {
                    DesktopTauriCommandError::job(format!(
                        "load desktop product page slice: {}",
                        error.as_str()
                    ))
                })?;
            match advance.state() {
                PortfolioPageLoadState::Page => {
                    let retained_slot = advance.retained_slot().ok_or_else(|| {
                        DesktopTauriCommandError::job(
                            "desktop product page slice omitted its retained page",
                        )
                    })?;
                    page_json(coverage_store(self)?, retained_slot, member_page_number)
                }
                PortfolioPageLoadState::WorkBudgetExhausted | PortfolioPageLoadState::Cancelled => {
                    replay_advance_json(coverage_store(self)?, advance)
                }
            }
        }

        pub fn product_page_release(&mut self) {
            self.product_page_store = None;
        }
    }

    fn coverage_store(
        bridge: &DesktopTauriCommandBridge,
    ) -> Result<&CoveragePortfolioPageStore, DesktopTauriCommandError> {
        bridge
            .product_page_store
            .as_ref()
            .and_then(ProductPageStore::coverage_portfolio)
            .ok_or_else(|| DesktopTauriCommandError::job("desktop product page store unavailable"))
    }

    fn pc_replay_advance_json(
        store: &PcReplayPageStore,
        advance: &PcReplayPageAdvance,
        geometry_page_number: usize,
        member_page_number: usize,
    ) -> Result<String, DesktopTauriCommandError> {
        let source_identity = store.source().identity_sha256();
        let value = match advance {
            PcReplayPageAdvance::Completed(page) => {
                let metadata = &page.metadata;
                if metadata.page_source_identity_sha256 != source_identity
                    || metadata.page_contract != clearra_app::PC_REPLAY_MEMBER_PAGE_CONTRACT
                    || metadata.geometry_page_number != geometry_page_number.to_string()
                    || metadata.member_page_number != member_page_number.to_string()
                {
                    return Err(DesktopTauriCommandError::job(
                        "desktop replay page differs from its pending request",
                    ));
                }
                serde_json::json!({
                    "schema_version": 1,
                    "runtime": "clearra-desktop",
                    "product_page_kind": "pc-replay",
                    "state": "page",
                    "page": page,
                })
            }
            PcReplayPageAdvance::Pending { work_steps }
            | PcReplayPageAdvance::Cancelled { work_steps } => serde_json::json!({
                "schema_version": 1,
                "runtime": "clearra-desktop",
                "product_page_kind": "pc-replay",
                "state": if matches!(advance, PcReplayPageAdvance::Pending { .. }) { "pending" } else { "cancelled" },
                "page_contract": clearra_app::PC_REPLAY_MEMBER_PAGE_CONTRACT,
                "page_source_identity_sha256": source_identity,
                "geometry_page_number": geometry_page_number.to_string(),
                "member_page_number": member_page_number.to_string(),
                "work_steps": work_steps.to_string(),
            }),
        };
        serde_json::to_string(&value).map_err(|error| {
            DesktopTauriCommandError::job(
                format!("serialize desktop replay page advance: {error}",),
            )
        })
    }

    fn coverage_store_mut(
        bridge: &mut DesktopTauriCommandBridge,
    ) -> Result<&mut CoveragePortfolioPageStore, DesktopTauriCommandError> {
        bridge
            .product_page_store
            .as_mut()
            .and_then(ProductPageStore::coverage_portfolio_mut)
            .ok_or_else(|| DesktopTauriCommandError::job("desktop product page store unavailable"))
    }

    fn page_json(
        store: &CoveragePortfolioPageStore,
        retained_slot: usize,
        member_page_number: usize,
    ) -> Result<String, DesktopTauriCommandError> {
        let page = store
            .retained_page(retained_slot)
            .ok_or_else(|| DesktopTauriCommandError::job("desktop product page not loaded"))?;
        let members = store
            .source()
            .member_page(page, member_page_number)
            .map_err(|error| {
                DesktopTauriCommandError::job(format!(
                    "load desktop product member page: {}",
                    error.as_str()
                ))
            })?;
        serde_json::to_string(&serde_json::json!({
            "schema_version": 1,
            "runtime": "clearra-desktop",
            "product_page_kind": "coverage-portfolio",
            "state": "page",
            "page": {
                "page_contract": page.contract_id(),
                "member_page_contract": members.contract_id(),
                "set_identity_sha256": page.set_identity_sha256(),
                "candidate_map_sha256": page.candidate_map_sha256(),
                "alternative_index": page.alternative_index_decimal(),
                "optimal_cardinality": page.optimal_cardinality().to_string(),
                "known_alternative_count": page.known_alternative_count_decimal(),
                "total_alternative_count": page.total_alternative_count_decimal(),
                "enumeration_complete": page.enumeration_complete(),
                "member_page_number": members.member_page_number().to_string(),
                "total_member_pages": members.total_member_pages().to_string(),
                "members": members.members().iter().map(|member| serde_json::json!({
                    "candidate_id": member.candidate_id().to_string(),
                    "normalized_solution_key": member.normalized_key(),
                })).collect::<Vec<_>>()
            }
        }))
        .map_err(|error| {
            DesktopTauriCommandError::job(format!("serialize desktop product page: {error}"))
        })
    }

    fn parse_canonical_positive_usize(
        value: &str,
        coordinate: &str,
    ) -> Result<usize, DesktopTauriCommandError> {
        if value.is_empty()
            || value.starts_with('0')
            || !value.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(DesktopTauriCommandError::invalid_request(format!(
                "{coordinate} must be a canonical positive decimal string"
            )));
        }
        value.parse::<usize>().map_err(|_| {
            DesktopTauriCommandError::invalid_request(format!(
                "{coordinate} exceeds the supported in-memory page range"
            ))
        })
    }

    fn advance_json(
        advance: &PortfolioAlternativeAdvance,
    ) -> Result<String, DesktopTauriCommandError> {
        serde_json::to_string(&serde_json::json!({
            "schema_version": 1,
            "runtime": "clearra-desktop",
            "product_page_kind": "coverage-portfolio",
            "state": advance.stop().as_str(),
            "known_alternative_count": advance.checkpoint().known_alternative_count_decimal(),
            "enumeration_complete": advance.checkpoint().enumeration_complete(),
            "work_steps": advance.work_steps(),
        }))
        .map_err(|error| {
            DesktopTauriCommandError::job(format!(
                "serialize desktop product page advance: {error}"
            ))
        })
    }

    fn replay_advance_json(
        store: &CoveragePortfolioPageStore,
        advance: PortfolioPageLoadAdvance,
    ) -> Result<String, DesktopTauriCommandError> {
        if advance.state() == PortfolioPageLoadState::Page || advance.retained_slot().is_some() {
            return Err(DesktopTauriCommandError::job(
                "desktop incomplete replay attempted to expose a page",
            ));
        }
        serde_json::to_string(&serde_json::json!({
            "schema_version": 1,
            "runtime": "clearra-desktop",
            "product_page_kind": "coverage-portfolio",
            "state": advance.state().as_str(),
            "known_alternative_count": store.known_alternative_count_decimal(),
            "enumeration_complete": store.enumeration_complete(),
            "work_steps": advance.work_steps(),
            "replay_cursor_alternative_index": store.replay_cursor_alternative_index_decimal(),
        }))
        .map_err(|error| {
            DesktopTauriCommandError::job(format!(
                "serialize desktop product page replay advance: {error}"
            ))
        })
    }

    fn parity_page_json(page: ParityReportPagePayload) -> Result<String, DesktopTauriCommandError> {
        if page.feasibility_claim() || page.pruning_authority() != "none" {
            return Err(DesktopTauriCommandError::job(
                "desktop parity page attempted to claim feasibility or pruning authority",
            ));
        }
        serde_json::to_string(&serde_json::json!({
            "schema_version": 1,
            "runtime": "clearra-desktop",
            "product_page_kind": "parity-report",
            "state": "page",
            "page": page,
        }))
        .map_err(|error| {
            DesktopTauriCommandError::job(format!("serialize desktop parity page: {error}"))
        })
    }

    fn parity_exhausted_json() -> Result<String, DesktopTauriCommandError> {
        serde_json::to_string(&serde_json::json!({
            "schema_version": 1,
            "runtime": "clearra-desktop",
            "product_page_kind": "parity-report",
            "state": "exhausted",
        }))
        .map_err(|error| {
            DesktopTauriCommandError::job(format!("serialize desktop parity page advance: {error}"))
        })
    }

    #[cfg(all(test, feature = "wasm-cpu-runtime"))]
    mod pc_replay_tests {
        use super::*;
        use clearra_app::{AppContext, AppCoreExecutorService, AppServices, CooperativeAppAdvance};
        use clearra_core_domain::execution_cancellation::ExecutionControl;

        fn bridge_with_replay_source() -> DesktopTauriCommandBridge {
            let context = AppContext::new(
                AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
            );
            let request = clearra_cli_command::CliCommandParser::parse(
                "clearra pc path --board-mask 0x0 --height 2 --lines 2 --pieces 5 --queue IIOOO --no-hold --backend cpu --workers 1",
            ).unwrap().to_app_request().unwrap();
            let mut execution = context.start_cooperative_execution(request);
            let control = ExecutionControl::default();
            for _ in 0..10_000 {
                match execution.advance(64, &control) {
                    CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
                    CooperativeAppAdvance::Completed(response) => {
                        let source = response.public_page_source_owner().expect("replay source");
                        let mut bridge = DesktopTauriCommandBridge::default();
                        bridge.product_page_store =
                            Some(ProductPageStore::from_source(source).unwrap());
                        assert!(
                            bridge
                                .product_page_store
                                .as_ref()
                                .unwrap()
                                .pc_replay()
                                .unwrap()
                                .source()
                                .geometry_count()
                                >= 2
                        );
                        return bridge;
                    }
                    other => panic!("unexpected replay source state: {other:?}"),
                }
            }
            panic!("tiny Desktop replay source exceeded its test-only work bound");
        }

        #[test]
        fn desktop_pc_replay_pending_slices_cancel_without_publishing_partial_or_late_pages() {
            let mut bridge = bridge_with_replay_source();
            let first: serde_json::Value = serde_json::from_str(
                &bridge
                    .product_page_get_slice_with_cancel("2", "1", 1, &mut || false)
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(first["state"], "pending");
            assert_eq!(first["page_contract"], "pc-replay-member-page.v2");
            assert_eq!(first["geometry_page_number"], "2");
            assert_eq!(first["member_page_number"], "1");
            assert!(first.get("page").is_none());
            assert!(first.get("witness_count").is_none());
            let mut checks = 0;
            let cancelled: serde_json::Value = serde_json::from_str(
                &bridge
                    .product_page_get_slice_with_cancel("2", "1", 1, &mut || {
                        checks += 1;
                        checks == 2
                    })
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(cancelled["state"], "cancelled");
            assert_eq!(
                cancelled["page_source_identity_sha256"],
                first["page_source_identity_sha256"]
            );
            assert!(cancelled.get("page").is_none());
            let mut completed = false;
            for _ in 0..10_000 {
                let next: serde_json::Value = serde_json::from_str(
                    &bridge
                        .product_page_get_slice_with_cancel("2", "1", 64, &mut || false)
                        .unwrap(),
                )
                .unwrap();
                if next["state"] == "page" {
                    assert_eq!(
                        next["page"]["page_source_identity_sha256"],
                        first["page_source_identity_sha256"]
                    );
                    assert!(!next["page"]["witnesses"].as_array().unwrap().is_empty());
                    completed = true;
                    break;
                }
                assert_eq!(next["state"], "pending");
            }
            assert!(
                completed,
                "cancel drops only the pending cursor; the immutable source remains usable"
            );
            bridge.product_page_release();
            assert!(bridge
                .product_page_get_slice_with_cancel("2", "1", 64, &mut || false)
                .is_err());
        }
    }
}
mod run_request {
    use clearra_app::ProductPageStore;

    use super::{
        active_request_parser::desktop_request_builds_app_request,
        bridge::DesktopTauriCommandBridge, error::DesktopTauriCommandError,
    };

    impl DesktopTauriCommandBridge {
        pub fn run_request(
            &mut self,
            request_json: &str,
        ) -> Result<String, DesktopTauriCommandError> {
            self.product_page_store = None;
            let request = desktop_request_builds_app_request(request_json)?;
            let response = self.app_context.run(request);
            let product_page_source_owner = response.public_page_source_owner();
            // This legacy synchronous GUI boundary must not turn every solution into a
            // CTK3/Fumen document. Explicit CLI/export routes retain that responsibility.
            let serialized =
                serde_json::to_string(&response.to_host_response()).map_err(|error| {
                    DesktopTauriCommandError::job(format!("serialize AppResponse: {error}"))
                })?;
            self.product_page_store = product_page_source_owner
                .map(ProductPageStore::from_source)
                .transpose()
                .map_err(|error| {
                    DesktopTauriCommandError::job(format!(
                        "open synchronous desktop product page store: {}",
                        error.as_str()
                    ))
                })?;
            Ok(serialized)
        }
    }
}
mod start_job {
    use crate::GuiJobRunner;

    use super::{
        active_request_parser::desktop_request_builds_app_request,
        bridge::DesktopTauriCommandBridge, error::DesktopTauriCommandError,
    };

    impl DesktopTauriCommandBridge {
        pub fn start_job(&mut self, request_json: &str) -> Result<u64, DesktopTauriCommandError> {
            self.reap_finished_job_before_start()?;
            self.product_page_store = None;
            let request = desktop_request_builds_app_request(request_json)?;
            let queued = self
                .queue
                .enqueue(request)
                .map_err(|error| DesktopTauriCommandError::job(error.to_string()))?;
            let job = self
                .queue
                .take_next()
                .map_err(|error| DesktopTauriCommandError::job(error.to_string()))?;
            let job_id = queued.job_id();
            self.active_job_id = Some(job_id);
            self.active_job = Some(GuiJobRunner::spawn(job, self.app_context.clone()));
            Ok(job_id.get())
        }

        fn reap_finished_job_before_start(&mut self) -> Result<(), DesktopTauriCommandError> {
            let Some(handle) = self.active_job.as_ref() else {
                return Ok(());
            };
            if !handle.is_finished() {
                return Err(DesktopTauriCommandError::job("desktop job already active"));
            }

            let job_id = self
                .active_job_id
                .ok_or_else(|| DesktopTauriCommandError::job("finished desktop job id missing"))?;
            if self.queue.active_job_id() != Some(job_id) {
                return Err(DesktopTauriCommandError::job(
                    "finished desktop job does not match the active queue job",
                ));
            }

            let handle = self.active_job.take().ok_or_else(|| {
                DesktopTauriCommandError::job("finished desktop job handle missing")
            })?;
            let join_result = handle.join();
            self.queue
                .finish(job_id)
                .map_err(|error| DesktopTauriCommandError::job(error.to_string()))?;
            self.active_job_id = None;
            join_result.map_err(|_| {
                DesktopTauriCommandError::job("finished desktop worker panicked while being reaped")
            })?;
            Ok(())
        }
    }
}
mod validate_request {
    use clearra_app::AppRequest;
    use serde_json::json;

    use super::{
        active_request_parser::desktop_request_builds_app_request,
        bridge::DesktopTauriCommandBridge, error::DesktopTauriCommandError,
    };

    impl DesktopTauriCommandBridge {
        pub fn validate_request(
            &self,
            request_json: &str,
        ) -> Result<String, DesktopTauriCommandError> {
            let request = desktop_request_builds_app_request(request_json)?;
            Ok(self.validate_built_request(&request))
        }

        fn validate_built_request(&self, request: &AppRequest) -> String {
            let report = self.app_context.validate_request(request);
            let diagnostics = report
                .validation()
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().as_str().to_owned())
                .collect::<Vec<_>>();
            let valid = !report.has_errors();

            json!({
                "schema_version": 1,
                "command": "validate_request",
                "app_request_model": "clearra-app/AppRequest",
                "valid": valid,
                "diagnostics": diagnostics
            })
            .to_string()
        }
    }

    #[cfg(test)]
    mod tests {
        use clearra_app::AppRequest;
        use clearra_cli_command::CliCommandParser;
        use serde_json::Value;

        use super::DesktopTauriCommandBridge;

        #[test]
        fn desktop_validate_endpoint_rejects_missing_product_capability_contract() {
            let attached = CliCommandParser::parse(
                "clearra pc allspin-pres-chance --lines 2 --patterns [TI]! --spin-profile all-mini-plus",
            )
            .expect("typed PC product request")
            .to_app_request()
            .expect("typed PC AppRequest");
            let missing = AppRequest::new(attached.into_command());

            let response: Value = serde_json::from_str(
                &DesktopTauriCommandBridge::default().validate_built_request(&missing),
            )
            .expect("desktop validation response JSON");

            assert_eq!(response["valid"], false);
            let diagnostics = response["diagnostics"]
                .as_array()
                .expect("desktop diagnostic codes");
            assert_eq!(
                diagnostics.last().and_then(Value::as_str),
                Some("E_FRONTEND_TYPED_REQUEST_REQUIRED")
            );
        }
    }
}

pub use bridge::DesktopTauriCommandBridge;
pub use error::DesktopTauriCommandError;

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use clearra_app::{
        encode_ctk3_compact, AppCommand, BuildColoredTargetDocument, BuildV2AppRequest, Ctk3Color,
        Ctk3Document, Ctk3Operation, Ctk3Page, Ctk3PageFlags, Ctk3Piece, Ctk3Rotation,
        FieldDocumentFormat, PcMinimalsIngressOrigin, PcPathIngressOrigin, PcResultProjection,
        PcSaveIngressOrigin, PcScoreIngressOrigin, PcScoreMinimalsIngressOrigin,
        PcTilingIngressOrigin, ProductCapabilityContract, QueryEnvelope, SpinStructureProductMode,
        PC_SCORE_MAX_PATTERNS,
    };
    use clearra_cli_command::CliCommandParser;
    use clearra_core_domain::{
        objective::objective_kind::ObjectiveKind, piece::piece_kind::PieceKind,
    };
    use clearra_forward_search::{
        ForwardLineClearPolicy, ForwardSearchMode, ForwardSpinCategory, ForwardSpinLineRequirement,
    };
    use clearra_i18n::LanguageId;
    use clearra_pc_graph::request::{
        GpuDeviceSelection, PcCountPolicy, RequestedSearchBackend, SupplyWindowSize, WorkerPolicy,
    };
    use clearra_problem::{
        BuildProbabilityAggregation, BuildSolutionProbabilityPolicy, SetupCandidatePriority,
        SetupLengthPreference, SetupSearchMode,
    };
    use clearra_supply::queue::queue_observation_policy::QueueObservationPolicy;
    use serde_json::{json, Value};

    use crate::GuiProblemForm;

    use super::{
        legacy_form_parser::{desktop_form_builds_app_request, desktop_request_builds_app_request},
        DesktopTauriCommandBridge,
    };

    #[test]
    fn desktop_profiles_object_binds_the_verified_structural_bundle() {
        let request = desktop_request_builds_app_request(
            &json!({
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 2,
                "profiles": {
                    "board": "standard-10",
                    "piece": "standard-tetrominoes",
                    "bag": "standard-7-bag"
                }
            })
            .to_string(),
        )
        .expect("canonical desktop profiles");
        let profiles = request.request_profiles();
        assert_eq!(profiles.board().as_str(), "standard-10");
        assert_eq!(profiles.piece_set().as_str(), "standard-tetrominoes");
        assert_eq!(profiles.bag().as_str(), "standard-7-bag");
    }

    #[test]
    fn desktop_profiles_and_unverified_rules_fail_closed_without_fallback() {
        for request in [
            json!({
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 2,
                "profiles": {
                    "board": "wide-10",
                    "piece": "standard-tetrominoes",
                    "bag": "standard-7-bag"
                }
            }),
            json!({
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 2,
                "rule": "custom"
            }),
            json!({
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 2,
                "score_mode": "summary",
                "spin_profile": "unverified-spin"
            }),
            json!({
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 2,
                "score_mode": "summary",
                "score_profile": "classic-score"
            }),
        ] {
            let error = desktop_request_builds_app_request(&request.to_string())
                .expect_err("unsupported profile must be rejected");
            assert!(error.message().contains("profile"), "{error}");
        }
    }

    fn one_operation_document() -> String {
        let mut page = Ctk3Page::new(0, Vec::new());
        page.flags = Ctk3PageFlags::default();
        page.operation = Some(Ctk3Operation {
            piece: Ctk3Piece::O,
            rotation: Ctk3Rotation::Spawn,
            x: 0,
            y: 0,
        });
        encode_ctk3_compact(&Ctk3Document::new(10, vec![page])).expect("one-operation CTK3")
    }

    fn two_page_field_document() -> String {
        encode_ctk3_compact(&Ctk3Document::new(
            2,
            vec![
                Ctk3Page::new(1, vec![Ctk3Color::Gray, Ctk3Color::Empty]),
                Ctk3Page::new(1, vec![Ctk3Color::Empty, Ctk3Color::Gray]),
            ],
        ))
        .expect("two-page CTK3")
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ExpectedBuildV2Request {
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

    fn build_v2_colored_document() -> String {
        let mut cells = vec![Ctk3Color::Empty; 40];
        cells[30..34].fill(Ctk3Color::Piece(Ctk3Piece::I));
        encode_ctk3_compact(&Ctk3Document::new(10, vec![Ctk3Page::new(4, cells)]))
            .expect("one-piece Build v2 CTK3 document")
    }

    fn desktop_build_v2_json(capability_id: &str, document: &str) -> Value {
        let objective = match capability_id {
            "build.cover" | "build.congruent-cover" | "build.setup-cover" => "min-cover",
            "build.setup" | "build.congruent" | "build.setup-cover-percent" => "unique",
            "build.setup-cover-score" | "build.evaluate.score" => "max-score-cover",
            "build.evaluate.cover" | "build.evaluate.b2b-cover" => "all",
            "build.evaluate.minimals" => "min-cover",
            "build.evaluate.cover-percent" => "unique",
            _ => panic!("unknown Build v2 test capability {capability_id}"),
        };
        let mut request = json!({
            "app_request_model": "clearra-app/AppRequest",
            "command": "build-v2",
            "language": "ko",
            "capability_id": capability_id,
            "queue": "I",
            "patterns": "",
            "queue_knowledge": "oracle",
            "hold_enabled": false,
            "hold_piece": "empty",
            "objective": objective,
            "rule": "srs-plus",
            "workers": 1,
            "use_all_logical_processors": false,
            "backend": "cpu",
            "allow_backend_fallback": false,
        });
        let object = request.as_object_mut().expect("Build v2 test object");
        if capability_id == "build.cover" {
            object.insert("base_mask".to_owned(), json!("0"));
            object.insert("target_mask".to_owned(), json!("15"));
            object.insert("visible_height".to_owned(), json!(4));
            object.insert("source_piece_count".to_owned(), json!(1));
        } else if capability_id.starts_with("build.evaluate.") {
            object.insert("solution_format".to_owned(), json!("ctk3"));
            object.insert("solution_document".to_owned(), json!(document));
        } else {
            object.insert("target_format".to_owned(), json!("ctk3"));
            object.insert("target_document".to_owned(), json!(document));
        }
        if matches!(
            capability_id,
            "build.setup-cover-score" | "build.evaluate.score"
        ) {
            object.insert("score_profile".to_owned(), json!("guideline"));
            object.insert("initial_b2b".to_owned(), json!(7));
        }
        request
    }

    fn build_v2_request_kind(request: &BuildV2AppRequest) -> ExpectedBuildV2Request {
        match request {
            BuildV2AppRequest::BuildCover(_) => ExpectedBuildV2Request::Cover,
            BuildV2AppRequest::BuildSetup(_) => ExpectedBuildV2Request::Setup,
            BuildV2AppRequest::BuildCongruent(_) => ExpectedBuildV2Request::Congruent,
            BuildV2AppRequest::BuildCongruentCover(_) => ExpectedBuildV2Request::CongruentCover,
            BuildV2AppRequest::BuildSetupCover(_) => ExpectedBuildV2Request::SetupCover,
            BuildV2AppRequest::BuildSetupCoverPercent(_) => {
                ExpectedBuildV2Request::SetupCoverPercent
            }
            BuildV2AppRequest::BuildSetupCoverScore(_) => ExpectedBuildV2Request::SetupCoverScore,
            BuildV2AppRequest::BuildEvaluateCover(_) => ExpectedBuildV2Request::EvaluateCover,
            BuildV2AppRequest::BuildEvaluateMinimals(_) => ExpectedBuildV2Request::EvaluateMinimals,
            BuildV2AppRequest::BuildEvaluateScore(_) => ExpectedBuildV2Request::EvaluateScore,
            BuildV2AppRequest::BuildEvaluateB2bCover(_) => ExpectedBuildV2Request::EvaluateB2bCover,
            BuildV2AppRequest::BuildEvaluateCoverPercent(_) => {
                ExpectedBuildV2Request::EvaluateCoverPercent
            }
        }
    }

    #[test]
    fn desktop_build_v2_json_exhaustively_lowers_all_twelve_nominal_capabilities() {
        let document = build_v2_colored_document();
        let cases = [
            ("build.cover", ExpectedBuildV2Request::Cover),
            ("build.setup", ExpectedBuildV2Request::Setup),
            ("build.congruent", ExpectedBuildV2Request::Congruent),
            (
                "build.congruent-cover",
                ExpectedBuildV2Request::CongruentCover,
            ),
            ("build.setup-cover", ExpectedBuildV2Request::SetupCover),
            (
                "build.setup-cover-percent",
                ExpectedBuildV2Request::SetupCoverPercent,
            ),
            (
                "build.setup-cover-score",
                ExpectedBuildV2Request::SetupCoverScore,
            ),
            (
                "build.evaluate.cover",
                ExpectedBuildV2Request::EvaluateCover,
            ),
            (
                "build.evaluate.minimals",
                ExpectedBuildV2Request::EvaluateMinimals,
            ),
            (
                "build.evaluate.score",
                ExpectedBuildV2Request::EvaluateScore,
            ),
            (
                "build.evaluate.b2b-cover",
                ExpectedBuildV2Request::EvaluateB2bCover,
            ),
            (
                "build.evaluate.cover-percent",
                ExpectedBuildV2Request::EvaluateCoverPercent,
            ),
        ];

        for (capability_id, expected) in cases {
            let request_json = desktop_build_v2_json(capability_id, &document).to_string();
            let request = desktop_request_builds_app_request(&request_json)
                .unwrap_or_else(|error| panic!("lower {capability_id}: {error}"));
            assert_eq!(
                request.query(),
                &QueryEnvelope::BuildCoverage,
                "{capability_id}"
            );
            assert_eq!(
                request.backend_policy().backend_requested(),
                "cpu",
                "{capability_id}"
            );
            assert!(
                !request.backend_policy().allow_backend_fallback(),
                "{capability_id}"
            );
            assert_eq!(
                request.resource_budget().memory_mib(),
                None,
                "{capability_id}"
            );
            let AppCommand::BuildV2(command) = request.command() else {
                panic!("{capability_id} did not lower to AppCommand::BuildV2");
            };
            assert_eq!(build_v2_request_kind(command.request()), expected);
        }
    }

    fn desktop_spin_structure_json(capability_id: &str) -> Value {
        let mut request = json!({
            "app_request_model": "clearra-app/AppRequest",
            "command": "spin-structure",
            "language": "ko",
            "capability_id": capability_id,
            "board_mask_v1": format!("{:060x}", 0x14000043ff_u64),
            "visible_height": 4,
            "inventory": "T",
            "spin_profile": "t-spins",
            "lines": "any",
            "fill_bottom": 0,
            "fill_top": 4,
            "rule": "srs-plus",
            "max_placements": 1,
            "minimality": "subset-minimal",
            "workers": 1,
            "use_all_logical_processors": false,
            "backend": "cpu",
            "allow_backend_fallback": false
        });
        let object = request.as_object_mut().expect("spin request object");
        match capability_id {
            "spin-structure.search" => {}
            "spin-structure.cover" => {
                object.insert("objective".to_owned(), json!("min-cover"));
                object.insert("max_patterns".to_owned(), json!(8));
            }
            "spin-structure.guaranteed" => {
                object.insert("max_patterns".to_owned(), json!(8));
                object.insert("final_piece".to_owned(), json!("T"));
                object.insert("dependency_report".to_owned(), json!(true));
            }
            _ => panic!("unknown spin-structure test capability"),
        }
        request
    }

    #[test]
    fn desktop_spin_structure_json_lowers_all_three_nominal_capabilities() {
        let cases = [
            ("spin-structure.search", SpinStructureProductMode::Search),
            (
                "spin-structure.cover",
                SpinStructureProductMode::Cover { max_patterns: 8 },
            ),
            (
                "spin-structure.guaranteed",
                SpinStructureProductMode::Guaranteed {
                    final_piece: PieceKind::T,
                    max_patterns: 8,
                    dependency_report: true,
                },
            ),
        ];
        for (capability_id, expected) in cases {
            let request = desktop_request_builds_app_request(
                &desktop_spin_structure_json(capability_id).to_string(),
            )
            .unwrap_or_else(|error| panic!("lower {capability_id}: {error}"));
            assert_eq!(request.backend_policy().backend_requested(), "cpu");
            assert!(!request.backend_policy().allow_backend_fallback());
            assert_eq!(request.resource_budget().memory_mib(), None);
            let AppCommand::SpinStructure(command) = request.command() else {
                panic!("{capability_id} did not lower to SpinStructure");
            };
            assert_eq!(command.product_mode(), expected, "{capability_id}");
            assert_eq!(command.query().inventory.total(), 1, "{capability_id}");
        }
    }

    #[test]
    fn desktop_spin_structure_strictly_rejects_unknown_cross_route_and_resource_fields() {
        let mut invalid = Vec::new();
        for field in [
            "gpu_device",
            "max_memory_mib",
            "memory_budget_mb",
            "hold_enabled",
            "queue",
        ] {
            let mut request = desktop_spin_structure_json("spin-structure.search");
            request
                .as_object_mut()
                .expect("request")
                .insert(field.to_owned(), json!(field == "hold_enabled"));
            invalid.push(request);
        }
        let mut fallback = desktop_spin_structure_json("spin-structure.search");
        fallback["allow_backend_fallback"] = json!(true);
        invalid.push(fallback);
        let mut gpu = desktop_spin_structure_json("spin-structure.search");
        gpu["backend"] = json!("gpu");
        invalid.push(gpu);
        let mut wrong_type = desktop_spin_structure_json("spin-structure.search");
        wrong_type["workers"] = json!("1");
        invalid.push(wrong_type);
        let mut cover_final = desktop_spin_structure_json("spin-structure.cover");
        cover_final["final_piece"] = json!("T");
        invalid.push(cover_final);
        let mut guaranteed_objective = desktop_spin_structure_json("spin-structure.guaranteed");
        guaranteed_objective["objective"] = json!("min-cover");
        invalid.push(guaranteed_objective);
        let mut search_patterns = desktop_spin_structure_json("spin-structure.search");
        search_patterns["max_patterns"] = json!(8);
        invalid.push(search_patterns);
        for request in invalid {
            let error = desktop_request_builds_app_request(&request.to_string())
                .expect_err("invalid desktop spin-structure request must fail closed");
            assert_eq!(error.code(), "desktop-invalid-request", "{request}");
        }
    }

    #[cfg(feature = "wasm-cpu-runtime")]
    #[test]
    fn desktop_actual_build_solution_results_defer_documents_to_page_or_export_routes() {
        use crate::GuiJobEvent;

        let executable_document = encode_ctk3_compact(&Ctk3Document::new(
            10,
            vec![Ctk3Page::new(
                1,
                [
                    vec![Ctk3Color::Piece(Ctk3Piece::I); 4],
                    vec![Ctk3Color::Empty; 6],
                ]
                .concat(),
            )],
        ))
        .expect("executable desktop Build v2 target");
        for capability_id in ["build.cover", "build.setup"] {
            let mut bridge = DesktopTauriCommandBridge::default();
            let request = desktop_build_v2_json(capability_id, &executable_document).to_string();
            let job_id = bridge
                .start_job(&request)
                .expect("start desktop Build v2 job");
            let deadline = Instant::now() + Duration::from_secs(5);
            let response = 'wait: loop {
                for event in bridge
                    .drain_job_events(job_id)
                    .expect("drain desktop Build v2 events")
                {
                    match event {
                        GuiJobEvent::Completed { response, .. } => break 'wait response,
                        GuiJobEvent::Failed { code, .. } => {
                            panic!("desktop Build v2 failed: {code}")
                        }
                        GuiJobEvent::Cancelled { .. } => {
                            panic!("desktop Build v2 was cancelled")
                        }
                        GuiJobEvent::Started { .. }
                        | GuiJobEvent::Progress { .. }
                        | GuiJobEvent::Diagnostic { .. } => {}
                    }
                }
                assert!(
                    Instant::now() < deadline,
                    "desktop Build v2 did not complete"
                );
                std::thread::yield_now();
            };
            assert_eq!(
                response.status(),
                clearra_host_contract::AppStatus::Success,
                "{capability_id} diagnostics: {:?}",
                response.diagnostics()
            );
            assert!(
                response.product_result_payload().is_some(),
                "{capability_id} product payload"
            );
            assert!(
                response.solution_set_artifact().is_none(),
                "GUI completion must not eagerly encode a solution document"
            );

            let synchronous: Value = serde_json::from_str(
                &bridge
                    .run_request(&request)
                    .expect("run synchronous desktop Build v2 request"),
            )
            .expect("desktop Build v2 response JSON");
            assert!(
                synchronous
                    .get("solution_set_artifact")
                    .is_none_or(serde_json::Value::is_null),
                "synchronous GUI completion must not eagerly encode a solution document"
            );
            if capability_id == "build.cover" {
                assert_eq!(
                    synchronous["product_result_payload"]["content"]["payload"]
                        ["page_source_available"],
                    true
                );
                let page: Value = serde_json::from_str(
                    &bridge
                        .product_page_get("1", "1")
                        .expect("load the synchronous desktop canonical page"),
                )
                .expect("synchronous desktop product page JSON");
                assert_eq!(page["state"], "page");
                assert_eq!(page["page"]["alternative_index"], "1");
                assert_eq!(page["page"]["member_page_number"], "1");
            }
        }
    }

    #[test]
    fn desktop_build_v2_preserves_the_colored_supplied_document_identity_and_height() {
        let document = build_v2_colored_document();
        let decoded = BuildColoredTargetDocument::decode(FieldDocumentFormat::Ctk3, &document)
            .expect("Build v2 colored fixture");
        let request = desktop_request_builds_app_request(
            &desktop_build_v2_json("build.evaluate.minimals", &document).to_string(),
        )
        .expect("desktop supplied Build v2 request");
        let AppCommand::BuildV2(command) = request.command() else {
            panic!("expected Build v2 command");
        };
        let BuildV2AppRequest::BuildEvaluateMinimals(request) = command.request() else {
            panic!("expected supplied minimals request");
        };
        assert_eq!(
            request.supplied().document_hash(),
            decoded.target().document_hash()
        );
        assert_eq!(
            request.supplied().identities(),
            decoded.target().identities()
        );
        assert_eq!(request.supplied().visible_height(), 4);
        assert_eq!(request.query().field().height(), 4);
    }

    #[test]
    fn desktop_build_v2_strictly_rejects_unknown_fields_and_wrong_json_types() {
        let document = build_v2_colored_document();
        let base = desktop_build_v2_json("build.cover", &document);
        let mutations = [
            ("unknown_option", json!(true)),
            ("capability_id", json!(12)),
            ("queue", json!(false)),
            ("queue_knowledge", json!(7)),
            ("hold_enabled", json!("false")),
            ("hold_piece", json!(false)),
            ("objective", json!(false)),
            ("rule", json!(false)),
            ("workers", json!("1")),
            ("use_all_logical_processors", json!(0)),
            ("allow_backend_fallback", json!("false")),
            ("visible_height", json!(4.5)),
            ("base_mask", json!(0)),
        ];
        for (field, replacement) in mutations {
            let mut request = base.clone();
            request
                .as_object_mut()
                .expect("Build v2 object")
                .insert(field.to_owned(), replacement);
            let error = desktop_request_builds_app_request(&request.to_string())
                .expect_err("strict Build v2 JSON type or unknown field");
            assert_eq!(error.code(), "desktop-invalid-request", "{field}");
        }

        let mut missing = base;
        missing
            .as_object_mut()
            .expect("Build v2 object")
            .remove("workers");
        desktop_request_builds_app_request(&missing.to_string())
            .expect_err("strict Build v2 JSON requires workers");
    }

    #[test]
    fn desktop_build_v2_rejects_cross_source_gpu_fallback_memory_and_illegal_options() {
        let document = build_v2_colored_document();
        let mut requests = Vec::new();

        let mut cross_target = desktop_build_v2_json("build.cover", &document);
        cross_target["target_format"] = json!("ctk3");
        cross_target["target_document"] = json!(document);
        requests.push(cross_target);

        let mut cross_solution = desktop_build_v2_json("build.setup", &document);
        cross_solution["solution_format"] = json!("ctk3");
        cross_solution["solution_document"] = json!(document);
        requests.push(cross_solution);

        let mut cross_mask = desktop_build_v2_json("build.evaluate.cover", &document);
        cross_mask["base_mask"] = json!("0");
        requests.push(cross_mask);

        let mut gpu = desktop_build_v2_json("build.cover", &document);
        gpu["backend"] = json!("gpu");
        requests.push(gpu);

        let mut fallback = desktop_build_v2_json("build.cover", &document);
        fallback["allow_backend_fallback"] = json!(true);
        requests.push(fallback);

        for memory_field in ["max_memory_mib", "memory_budget_mb"] {
            let mut memory = desktop_build_v2_json("build.cover", &document);
            memory[memory_field] = json!(64);
            requests.push(memory);
        }

        let mut score = desktop_build_v2_json("build.setup", &document);
        score["score_profile"] = json!("tetrio");
        requests.push(score);

        let mut source_count = desktop_build_v2_json("build.setup", &document);
        source_count["source_piece_count"] = json!(1);
        requests.push(source_count);

        let mut both_sources = desktop_build_v2_json("build.cover", &document);
        both_sources["patterns"] = json!("I");
        requests.push(both_sources);

        let mut no_source = desktop_build_v2_json("build.cover", &document);
        no_source["queue"] = json!("");
        requests.push(no_source);

        let mut objective = desktop_build_v2_json("build.setup", &document);
        objective["objective"] = json!("min-cover");
        requests.push(objective);

        for request in requests {
            let error = desktop_request_builds_app_request(&request.to_string())
                .expect_err("illegal Desktop Build v2 state must fail closed");
            assert_eq!(error.code(), "desktop-invalid-request", "{request}");
        }
    }

    #[test]
    fn desktop_typed_document_utilities_use_closed_explicit_json_contracts() {
        let parity = desktop_request_builds_app_request(
            &json!({
                "app_request_model": "clearra-app/AppRequest",
                "command": "utility-parity",
                "format": "ctk3",
                "document": two_page_field_document(),
            })
            .to_string(),
        )
        .expect("typed desktop parity request");
        assert!(matches!(parity.command(), AppCommand::UtilityParity(_)));

        let fumen = desktop_request_builds_app_request(
            &json!({
                "app_request_model": "clearra-app/AppRequest",
                "command": "utility-fumen",
                "format": "fumen",
                "transform": "text-to-fumen",
                "documents": [],
                "comments": ["first", "second"],
            })
            .to_string(),
        )
        .expect("typed desktop Fumen request");
        assert!(matches!(fumen.command(), AppCommand::UtilityFumen(_)));

        let render = desktop_request_builds_app_request(
            &json!({
                "app_request_model": "clearra-app/AppRequest",
                "command": "utility-render",
                "format": "ctk3",
                "document": two_page_field_document(),
                "artifact_format": "png",
                "page_number": 1,
            })
            .to_string(),
        )
        .expect("typed desktop render request");
        assert!(matches!(render.command(), AppCommand::UtilityRender(_)));

        let to_gray = desktop_request_builds_app_request(
            &json!({
                "app_request_model": "clearra-app/AppRequest",
                "command": "utility-to-gray",
                "format": "ctk3",
                "document": two_page_field_document(),
            })
            .to_string(),
        )
        .expect("typed desktop to-gray request");
        assert!(matches!(to_gray.command(), AppCommand::UtilityToGray(_)));

        let mirror = desktop_request_builds_app_request(
            &json!({
                "app_request_model": "clearra-app/AppRequest",
                "command": "utility-mirror",
                "format": "ctk3",
                "document": two_page_field_document(),
            })
            .to_string(),
        )
        .expect("typed desktop mirror request");
        assert!(matches!(mirror.command(), AppCommand::UtilityMirror(_)));

        let rejected = desktop_request_builds_app_request(
            &json!({
                "app_request_model": "clearra-app/AppRequest",
                "command": "utility-fumen",
                "format": "fumen",
                "transform": "roundtrip",
                "documents": ["v115@vhA"],
                "queue": "IOTSZJL",
            })
            .to_string(),
        )
        .expect_err("utility JSON must reject queue inference");
        assert!(rejected.message().contains("does not accept field 'queue'"));

        let rejected = desktop_request_builds_app_request(
            &json!({
                "app_request_model": "clearra-app/AppRequest",
                "command": "utility-mirror",
                "format": "ctk3",
                "document": two_page_field_document(),
                "hold": true,
            })
            .to_string(),
        )
        .expect_err("transform JSON must reject hold inference");
        assert!(rejected.message().contains("does not accept field 'hold'"));
    }

    #[test]
    fn desktop_parity_job_transfers_browsable_owner_until_explicit_release() {
        use clearra_host_contract::ProductResultPayloadContent;

        use crate::GuiJobEvent;

        let request = json!({
            "app_request_model": "clearra-app/AppRequest",
            "command": "utility-parity",
            "format": "ctk3",
            "document": two_page_field_document(),
        })
        .to_string();
        let mut bridge = DesktopTauriCommandBridge::default();
        let job_id = bridge.start_job(&request).expect("start parity job");
        let deadline = Instant::now() + Duration::from_secs(2);
        let response = loop {
            let events = bridge
                .drain_job_events(job_id)
                .expect("drain desktop parity events");
            if let Some(response) = events.into_iter().find_map(|event| match event {
                GuiJobEvent::Completed { response, .. } => Some(response),
                GuiJobEvent::Failed { code, .. } => panic!("desktop parity failed: {code}"),
                GuiJobEvent::Cancelled { .. } => panic!("desktop parity was cancelled"),
                GuiJobEvent::Started { .. }
                | GuiJobEvent::Progress { .. }
                | GuiJobEvent::Diagnostic { .. } => None,
            }) {
                break response;
            }
            assert!(Instant::now() < deadline, "desktop parity did not complete");
            std::thread::yield_now();
        };
        let ProductResultPayloadContent::ParityReportPage(first) = response
            .product_result_payload()
            .expect("parity payload")
            .content()
        else {
            panic!("expected parity page payload")
        };
        assert_eq!(first.page_number(), 1);
        assert_eq!(first.total_pages(), 2);
        assert!(!first.feasibility_claim());
        assert_eq!(first.pruning_authority(), "none");

        let first_page: Value = serde_json::from_str(
            &bridge
                .product_page_get("1", "1")
                .expect("load first desktop parity page"),
        )
        .expect("first parity JSON");
        assert_eq!(first_page["product_page_kind"], "parity-report");
        assert_eq!(first_page["page"]["page_number"], 1);
        assert_eq!(first_page["page"]["feasibility_claim"], false);
        assert_eq!(first_page["page"]["pruning_authority"], "none");

        let second_page: Value = serde_json::from_str(
            &bridge
                .product_page_next(1)
                .expect("advance desktop parity page"),
        )
        .expect("second parity JSON");
        assert_eq!(second_page["page"]["page_number"], 2);
        let exhausted: Value = serde_json::from_str(
            &bridge
                .product_page_next(1)
                .expect("exhaust desktop parity pages"),
        )
        .expect("parity exhausted JSON");
        assert_eq!(exhausted["state"], "exhausted");

        bridge.product_page_release();
        assert!(bridge.product_page_get("1", "1").is_err());
    }

    #[test]
    fn desktop_sequence_dependencies_uses_exact_json_contract_and_forbids_queue_hold() {
        let request = desktop_request_builds_app_request(
            &json!({
                "app_request_model": "clearra-app/AppRequest",
                "command": "utility-sequence-dependencies",
                "document": one_operation_document(),
                "rule_profile": "srs-plus",
                "kick_profile": "srs-plus",
                "timeout_seconds": 900,
            })
            .to_string(),
        )
        .expect("typed desktop operation document");
        let AppCommand::UtilitySequenceDependencies(command) = request.command() else {
            panic!("expected sequence-dependencies command");
        };
        assert_eq!(command.problem().timeout_seconds, 900);

        let error = desktop_request_builds_app_request(
            &json!({
                "app_request_model": "clearra-app/AppRequest",
                "command": "utility-sequence-dependencies",
                "document": one_operation_document(),
                "queue": "O",
            })
            .to_string(),
        )
        .expect_err("queue must never enter the operation-document contract");
        assert!(error.message().contains("does not accept field 'queue'"));
    }

    #[test]
    fn desktop_sequence_uses_replay_json_contract_and_forbids_queue_hold() {
        let request = desktop_request_builds_app_request(
            &json!({
                "app_request_model": "clearra-app/AppRequest",
                "command": "utility-sequence",
                "document": one_operation_document(),
                "rule_profile": "srs-plus",
                "kick_profile": "srs-plus",
                "timeout_seconds": 900,
            })
            .to_string(),
        )
        .expect("typed desktop operation sequence");
        let AppCommand::UtilitySequence(command) = request.command() else {
            panic!("expected operation sequence command");
        };
        assert_eq!(command.problem().timeout_seconds, 900);

        let error = desktop_request_builds_app_request(
            &json!({
                "app_request_model": "clearra-app/AppRequest",
                "command": "utility-sequence",
                "document": one_operation_document(),
                "hold_enabled": true,
            })
            .to_string(),
        )
        .expect_err("hold must never enter the operation-sequence contract");
        assert!(error
            .message()
            .contains("does not accept field 'hold_enabled'"));
    }

    fn canonical_desktop_search_request(command: &str) -> Value {
        let mut request: Value = serde_json::from_str(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "language": "en",
                "lines": 2,
                "queue": "",
                "patterns": "",
                "queue_knowledge": "oracle",
                "hold_enabled": true,
                "hold_piece": "empty",
                "backend": "auto",
                "rule": "srs-plus",
                "score_mode": "off",
                "score_profile": "tetrio",
                "spin_profile": "t-spins",
                "preserve_b2b": false,
                "precompute_build_dependencies": false,
                "finesse": "off",
                "pattern_knowledge": "both",
                "board_mask": "0x0000000000000000",
                "visible_height": 2,
                "piece_window": null,
                "count_policy": "unique",
                "solution_probabilities": false,
                "workers": 0,
                "use_all_logical_processors": false,
                "gpu_device": "auto",
                "allow_backend_fallback": true,
                "memory_budget_mb": 0,
                "candidate_budget": 10000000,
                "pattern_budget": 5040,
                "tablebase_requested": false,
                "setup_mode": "oracle",
                "setup_remaining": "IOTSZJL",
                "setup_qb": "",
                "setup_next_cycle_remaining": "",
                "setup_allow_post_cycle_borrow": false,
                "setup_priority": "all",
                "setup_length": "auto",
                "setup_max_pieces": 9,
                "base_mask": "0x0",
                "target_mask": "0x0",
                "build_aggregation": "buildability",
                "include_horizontal_mirror": true
            }"#,
        )
        .expect("canonical desktop request JSON");
        let object = request
            .as_object_mut()
            .expect("canonical desktop request is an object");
        object.insert("command".into(), json!(command));
        match command {
            "pc" => {
                object.insert("initial_b2b".into(), json!(0));
            }
            "pc-scenario" => {
                object.insert("piece_window".into(), json!(1));
                object.insert("initial_b2b".into(), json!(0));
            }
            "setup" => {
                object.insert("backend".into(), json!("cpu"));
                object.insert("allow_backend_fallback".into(), json!(false));
                object.insert("workers".into(), json!(1));
            }
            "build-probability" => {
                object.insert("backend".into(), json!("cpu"));
                object.insert("allow_backend_fallback".into(), json!(false));
                object.insert("queue".into(), json!("I"));
                object.insert("target_mask".into(), json!("0xf"));
                object.insert("candidate_budget".into(), json!(0));
                object.insert("pattern_budget".into(), json!(0));
                object.insert("workers".into(), json!(1));
            }
            "damage" => {
                object.insert("backend".into(), json!("cpu"));
                object.insert("allow_backend_fallback".into(), json!(false));
                object.insert("queue".into(), json!("T"));
                object.insert("initial_combo".into(), json!(0));
                object.insert("initial_b2b".into(), json!(0));
                object.insert("damage_aggregation".into(), json!("maximum"));
                object.insert("workers".into(), json!(1));
            }
            "spin-finder" => {
                object.insert("backend".into(), json!("cpu"));
                object.insert("allow_backend_fallback".into(), json!(false));
                object.insert("queue".into(), json!("T"));
                object.insert("spin_lines".into(), json!("any"));
                object.insert("spin_category".into(), json!("any"));
                object.insert("workers".into(), json!(1));
            }
            _ => panic!("unsupported canonical desktop command: {command}"),
        }
        request
    }

    #[test]
    fn desktop_ui_base_canonical_defaults_are_accepted_for_every_search_surface() {
        for command in [
            "pc",
            "pc-scenario",
            "setup",
            "build-probability",
            "damage",
            "spin-finder",
        ] {
            let request = canonical_desktop_search_request(command);
            desktop_request_builds_app_request(&request.to_string()).unwrap_or_else(|error| {
                panic!("canonical UI base DTO must build for {command}: {error}")
            });
        }

        let mut legacy_damage = canonical_desktop_search_request("damage");
        legacy_damage.as_object_mut().expect("damage DTO").extend([
            ("spin_lines".into(), json!("any")),
            ("spin_category".into(), json!("any")),
        ]);
        desktop_request_builds_app_request(&legacy_damage.to_string())
            .expect("legacy canonical spin defaults on damage remain compatible");

        let mut legacy_spin = canonical_desktop_search_request("spin-finder");
        legacy_spin.as_object_mut().expect("spin DTO").extend([
            ("initial_combo".into(), json!(0)),
            ("initial_b2b".into(), json!(0)),
            ("damage_aggregation".into(), json!("maximum")),
            ("minimum_damage".into(), json!(0)),
        ]);
        desktop_request_builds_app_request(&legacy_spin.to_string())
            .expect("legacy canonical damage defaults on spin-finder remain compatible");
    }

    #[test]
    fn desktop_search_surfaces_reject_noncanonical_or_malformed_inactive_fields() {
        let cases = vec![
            ("pc", "initial_combo", json!("bad")),
            ("pc", "initial_combo", json!(1)),
            ("pc", "setup_mode", json!("qb")),
            ("pc", "build_aggregation", json!("spin")),
            ("pc", "cpu_warmup", json!(true)),
            ("pc-scenario", "setup_max_pieces", json!(10)),
            ("pc-scenario", "target_mask", json!(15)),
            ("pc-scenario", "damage_aggregation", json!("at-least")),
            ("pc-scenario", "spin_category", json!("t")),
            ("setup", "backend", json!(7)),
            ("setup", "backend", json!("gpu")),
            ("setup", "allow_backend_fallback", json!(true)),
            ("setup", "score_mode", json!("summary")),
            ("setup", "initial_b2b", json!(1)),
            ("setup", "target_mask", json!(15)),
            ("setup", "spin_lines", json!("2+")),
            ("build-probability", "initial_b2b", json!("bad")),
            ("build-probability", "initial_b2b", json!(1)),
            ("build-probability", "score_mode", json!("summary")),
            ("build-probability", "setup_mode", json!("qb")),
            ("build-probability", "initial_combo", json!(1)),
            ("build-probability", "spin_category", json!("t")),
            ("damage", "backend", json!(7)),
            ("damage", "backend", json!("gpu")),
            ("damage", "allow_backend_fallback", json!(true)),
            ("damage", "score_mode", json!("summary")),
            ("damage", "setup_mode", json!("qb")),
            ("damage", "target_mask", json!(15)),
            ("damage", "spin_lines", json!("2+")),
            ("spin-finder", "backend", json!(7)),
            ("spin-finder", "backend", json!("gpu")),
            ("spin-finder", "allow_backend_fallback", json!(true)),
            ("spin-finder", "score_mode", json!("summary")),
            ("spin-finder", "initial_combo", json!("bad")),
            ("spin-finder", "initial_combo", json!(1)),
            ("spin-finder", "damage_aggregation", json!("at-least")),
        ];

        for (command, field, invalid) in cases {
            let mut request = canonical_desktop_search_request(command);
            request
                .as_object_mut()
                .expect("desktop request object")
                .insert(field.into(), invalid);
            let error = desktop_request_builds_app_request(&request.to_string())
                .expect_err("inactive field must fail closed");
            assert_eq!(error.code(), "desktop-invalid-request", "{command}.{field}");
            assert_eq!(
                error.message(),
                format!(
                    "desktop {command} inactive field '{field}' must keep its canonical default"
                ),
                "{command}.{field}"
            );
        }
    }

    #[test]
    fn desktop_request_defaults_hold_on_and_preserves_explicit_off() {
        let default_state = desktop_form_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 2,
                "backend": "cpu"
            }"#,
        )
        .expect("desktop default request");
        let GuiProblemForm::OpeningPc(default_form) = default_state.problem_form() else {
            panic!("expected opening PC form");
        };
        assert!(default_form.hold_enabled());

        let disabled_state = desktop_form_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 2,
                "hold_enabled": false,
                "backend": "cpu"
            }"#,
        )
        .expect("desktop no-hold request");
        let GuiProblemForm::OpeningPc(disabled_form) = disabled_state.problem_form() else {
            panic!("expected opening PC form");
        };
        assert!(!disabled_form.hold_enabled());
    }

    #[test]
    fn desktop_empty_pc_sources_build_the_shared_standard_seven_bag_projection() {
        let opening = desktop_request_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 2,
                "queue": "",
                "patterns": ""
            }"#,
        )
        .expect("desktop opening standard-bag request");
        let AppCommand::Pc(opening) = opening.command() else {
            panic!("expected opening PC command");
        };
        assert_eq!(opening.query().queue().mode(), "standard-7-bag");

        let scenario = desktop_request_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc-scenario",
                "lines": 4,
                "visible_height": 4,
                "board_mask": 0,
                "piece_window": 10,
                "queue": "",
                "patterns": ""
            }"#,
        )
        .expect("desktop scenario standard-bag request");
        let AppCommand::Scenario(scenario) = scenario.command() else {
            panic!("expected scenario PC command");
        };
        assert_eq!(scenario.query().remaining_queue().mode(), "standard-7-bag");

        let pattern = desktop_request_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc-scenario",
                "lines": 2,
                "visible_height": 2,
                "board_mask": 0,
                "piece_window": 5,
                "queue": "",
                "patterns": "[IOTSZ]!"
            }"#,
        )
        .expect("desktop scenario pattern request");
        let AppCommand::Scenario(pattern) = pattern.command() else {
            panic!("expected scenario PC command");
        };
        assert_eq!(
            pattern.query().remaining_queue().mode(),
            "materialized-pattern-expression"
        );
    }

    #[test]
    fn desktop_backend_omission_defaults_match_cli_authority_for_all_four_backends() {
        for (backend, expected_fallback) in [
            ("auto", true),
            ("cpu", false),
            ("gpu", false),
            ("hybrid", false),
        ] {
            let desktop_json = format!(
                r#"{{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "pc",
                    "lines": 2,
                    "backend": "{backend}"
                }}"#
            );
            let desktop = desktop_request_builds_app_request(&desktop_json)
                .expect("desktop backend-default request");
            let cli = CliCommandParser::parse(&format!("clearra pc --lines 2 --backend {backend}"))
                .expect("CLI backend-default request")
                .to_app_request()
                .expect("CLI backend-default AppRequest");

            assert_eq!(
                desktop.backend_policy().allow_backend_fallback(),
                expected_fallback,
                "desktop {backend} fallback default"
            );
            assert_eq!(
                desktop.backend_policy(),
                cli.backend_policy(),
                "Desktop/CLI backend projection for {backend}"
            );
        }
    }

    #[test]
    fn desktop_backend_explicit_fallback_boole_override_surface_defaults() {
        for (backend, explicit) in [
            ("auto", false),
            ("cpu", true),
            ("gpu", true),
            ("hybrid", true),
        ] {
            let state = desktop_form_builds_app_request(&format!(
                r#"{{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "pc",
                    "lines": 2,
                    "backend": "{backend}",
                    "allow_backend_fallback": {explicit}
                }}"#
            ))
            .expect("desktop explicit backend fallback");
            assert_eq!(state.backend_form().allow_fallback(), explicit, "{backend}");
        }
    }

    #[test]
    fn desktop_typed_search_families_reject_present_malformed_fields() {
        let cases = [
            (
                "pc lines string",
                r#"{"app_request_model":"clearra-app/AppRequest","command":"pc","lines":"2"}"#,
                "desktop lines must be a nonnegative integer",
            ),
            (
                "pc backend number",
                r#"{"app_request_model":"clearra-app/AppRequest","command":"pc","backend":7}"#,
                "desktop backend must be a string",
            ),
            (
                "pc fallback string",
                r#"{"app_request_model":"clearra-app/AppRequest","command":"pc","backend":"cpu","allow_backend_fallback":"false"}"#,
                "desktop allow_backend_fallback must be a boolean",
            ),
            (
                "pc initial B2B overflow",
                r#"{"app_request_model":"clearra-app/AppRequest","command":"pc","initial_b2b":65536}"#,
                "desktop initial_b2b must fit in u16",
            ),
            (
                "pc-scenario boolean string",
                r#"{"app_request_model":"clearra-app/AppRequest","command":"pc-scenario","lines":2,"board_mask":0,"piece_window":1,"hold_enabled":"true"}"#,
                "desktop hold_enabled must be a boolean",
            ),
            (
                "pc-scenario zero piece window",
                r#"{"app_request_model":"clearra-app/AppRequest","command":"pc-scenario","lines":2,"board_mask":0,"piece_window":0}"#,
                "desktop scenario PC requires a positive piece_window",
            ),
            (
                "setup null boolean",
                r#"{"app_request_model":"clearra-app/AppRequest","command":"setup","setup_remaining":"IOTS","setup_allow_post_cycle_borrow":null}"#,
                "desktop setup_allow_post_cycle_borrow must be a boolean",
            ),
            (
                "build-probability boolean string",
                r#"{"app_request_model":"clearra-app/AppRequest","command":"build-probability","height":1,"base_mask":0,"target_mask":15,"queue":"I","preserve_b2b":"false"}"#,
                "desktop preserve_b2b must be a boolean",
            ),
            (
                "build-probability backend number",
                r#"{"app_request_model":"clearra-app/AppRequest","command":"build-probability","height":1,"base_mask":0,"target_mask":15,"queue":"I","backend":7}"#,
                "desktop backend must be a string",
            ),
            (
                "damage integer string",
                r#"{"app_request_model":"clearra-app/AppRequest","command":"damage","queue":"I","initial_combo":"2"}"#,
                "desktop initial_combo must be a nonnegative integer",
            ),
            (
                "spin-finder category boolean",
                r#"{"app_request_model":"clearra-app/AppRequest","command":"spin-finder","patterns":"[T]!","spin_category":false}"#,
                "desktop spin_category must be a string",
            ),
        ];

        for (name, request, expected_message) in cases {
            let error = desktop_request_builds_app_request(request)
                .expect_err("present malformed typed field must fail closed");
            assert_eq!(error.code(), "desktop-invalid-request", "{name}");
            assert_eq!(error.message(), expected_message, "{name}");
        }
    }

    #[test]
    fn desktop_typed_alias_conflicts_fail_closed_before_projection() {
        let cases = [
            (
                r#"{"app_request_model":"clearra-app/AppRequest","command":"pc","lines":2,"use_all_logical_processors":true,"use_all_cpu_threads":false}"#,
                "desktop use_all_logical_processors aliases are mutually exclusive",
            ),
            (
                r#"{"app_request_model":"clearra-app/AppRequest","command":"setup","setup_remaining":"IOTS","setup_mode":"oracle","mode":"qb"}"#,
                "desktop setup_mode aliases are mutually exclusive",
            ),
            (
                r#"{"app_request_model":"clearra-app/AppRequest","command":"build-probability","height":1,"visible_height":null,"base_mask":0,"target_mask":15,"queue":"I"}"#,
                "desktop height aliases are mutually exclusive",
            ),
            (
                r#"{"app_request_model":"clearra-app/AppRequest","command":"damage","queue":"I","board_mask":0,"initial_board_mask":"malformed"}"#,
                "desktop board_mask aliases are mutually exclusive",
            ),
        ];

        for (request, expected_message) in cases {
            let error = desktop_request_builds_app_request(request)
                .expect_err("typed aliases must be mutually exclusive");
            assert_eq!(error.code(), "desktop-invalid-request");
            assert_eq!(error.message(), expected_message);
        }
    }

    #[test]
    fn desktop_request_preserves_visible_seven_queue_knowledge() {
        let state = desktop_form_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 4,
                "queue_knowledge": "visible-7",
                "backend": "cpu"
            }"#,
        )
        .expect("desktop visible-seven request");
        let GuiProblemForm::OpeningPc(form) = state.problem_form() else {
            panic!("expected opening PC form");
        };

        assert_eq!(
            form.queue_observation_policy(),
            QueueObservationPolicy::VisibleSeven
        );
    }

    #[test]
    fn desktop_request_rejects_visible_seven_minimum_cover_aliases_consistently() {
        for command in ["pc", "pc-scenario"] {
            for score_mode in ["minimum-cover", "minimum"] {
                let scenario_fields = if command == "pc-scenario" {
                    r#", "piece_window": 10, "board_mask": 0"#
                } else {
                    ""
                };
                let request = format!(
                    r#"{{
                        "app_request_model": "clearra-app/AppRequest",
                        "command": "{command}",
                        "lines": 4,
                        "score_mode": "{score_mode}",
                        "queue_knowledge": "visible-7",
                        "backend": "cpu"
                        {scenario_fields}
                    }}"#
                );
                let error = desktop_request_builds_app_request(&request).expect_err(
                    "visible-7 minimum-cover aliases must fail before AppRequest construction",
                );

                assert_eq!(
                    error.code(),
                    "desktop-invalid-request",
                    "{command} {score_mode}"
                );
                assert!(
                    error
                        .message()
                        .contains("visible-seven-minimum-cover-unsupported"),
                    "{command} {score_mode}: {}",
                    error.message()
                );
            }
        }
    }

    #[test]
    fn desktop_request_preserves_dependency_dag_opt_in_and_default_off() {
        let default_state = desktop_form_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 4,
                "backend": "cpu"
            }"#,
        )
        .expect("desktop default request");
        assert!(!default_state.backend_form().precompute_build_dependencies());

        let enabled_state = desktop_form_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 4,
                "backend": "cpu",
                "precompute_build_dependencies": true
            }"#,
        )
        .expect("desktop dependency DAG request");
        assert!(enabled_state.backend_form().precompute_build_dependencies());
    }

    #[test]
    fn desktop_pc_preserves_explicit_tablebase_policy() {
        for requested in [false, true] {
            let request = desktop_request_builds_app_request(&format!(
                r#"{{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "pc-scenario",
                    "lines": 2,
                    "visible_height": 2,
                    "board_mask": 0,
                    "piece_window": 5,
                    "tablebase_requested": {requested}
                }}"#
            ))
            .expect("desktop PC tablebase request");
            let AppCommand::Scenario(command) = request.command() else {
                panic!("expected scenario PC command");
            };
            assert_eq!(
                command.query().execution_policy().tablebase_requested(),
                requested
            );
        }
    }

    #[test]
    fn desktop_tiling_requests_reject_noncanonical_inactive_options() {
        let canonical = desktop_request_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc-scenario",
                "lines": 2,
                "visible_height": 2,
                "board_mask": 0,
                "piece_window": 5,
                "score_mode": "tiling"
            }"#,
        )
        .expect("canonical desktop tiling PC request");
        assert_eq!(
            canonical.product_capability_contract(),
            Some(ProductCapabilityContract::PcTiling)
        );
        let AppCommand::Scenario(command) = canonical.command() else {
            panic!("expected scenario PC command");
        };
        assert_eq!(
            command.result_projection(),
            PcResultProjection::TilingFamilyV1(PcTilingIngressOrigin::CanonicalPcTiling)
        );
        assert_eq!(
            command.query().supply_window_size(),
            Some(SupplyWindowSize::new(6))
        );

        let generic = desktop_request_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 2,
                "score_mode": "tiling-only"
            }"#,
        )
        .expect("generic desktop tiling objective");
        assert_eq!(generic.product_capability_contract(), None);
        let AppCommand::Pc(command) = generic.command() else {
            panic!("expected opening PC command");
        };
        assert_eq!(command.result_projection(), PcResultProjection::Standard);

        for option in [
            r#""rule": "srs""#,
            r#""count_policy": "all""#,
            r#""score_profile": "guideline""#,
            r#""spin_profile": "all-spin""#,
            r#""initial_b2b": 1"#,
            r#""preserve_b2b": true"#,
            r#""solution_probabilities": true"#,
            r#""precompute_build_dependencies": true"#,
            r#""tablebase_requested": true"#,
            r#""queue_knowledge": "visible-7""#,
            r#""finesse": "inputs""#,
            r#""pattern_knowledge": "oracle""#,
        ] {
            let error = desktop_request_builds_app_request(&format!(
                r#"{{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "pc-scenario",
                    "lines": 2,
                    "visible_height": 2,
                    "board_mask": 0,
                    "piece_window": 5,
                    "score_mode": "tiling",
                    {option}
                }}"#
            ))
            .expect_err(option);
            assert_eq!(error.code(), "desktop-invalid-request");
        }

        for option in [
            r#""rule": "srs""#,
            r#""spin_profile": "all-spin""#,
            r#""preserve_b2b": true"#,
            r#""precompute_build_dependencies": true"#,
            r#""finesse": "inputs""#,
            r#""pattern_knowledge": "oracle""#,
        ] {
            let error = desktop_request_builds_app_request(&format!(
                r#"{{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "build-probability",
                    "height": 1,
                    "base_mask": 0,
                    "target_mask": 15,
                    "queue": "I",
                    "build_aggregation": "tiling",
                    {option}
                }}"#
            ))
            .expect_err(option);
            assert_eq!(error.code(), "desktop-invalid-request", "{option}");
        }

        for tablebase in ["true", "\"yes\""] {
            let error = desktop_request_builds_app_request(&format!(
                r#"{{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "build-probability",
                    "height": 1,
                    "base_mask": 0,
                    "target_mask": 15,
                    "queue": "I",
                    "tablebase_requested": {tablebase}
                }}"#
            ))
            .expect_err(tablebase);
            assert!(error.to_string().contains("tablebase"));
        }
    }

    #[test]
    fn desktop_portfolio_modes_attach_their_typed_product_contracts() {
        let minimals = desktop_request_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc-scenario",
                "lines": 1,
                "visible_height": 1,
                "board_mask": "0x3f",
                "piece_window": 1,
                "queue": "I",
                "hold_enabled": true,
                "hold_piece": "empty",
                "count_policy": "all",
                "score_mode": "minimum-cover"
            }"#,
        )
        .expect("canonical desktop pc minimals request");
        assert_eq!(
            minimals.product_capability_contract(),
            Some(ProductCapabilityContract::PcMinimals)
        );
        let AppCommand::Scenario(command) = minimals.command() else {
            panic!("expected scenario PC command");
        };
        assert_eq!(
            command.result_projection(),
            PcResultProjection::MinimumCoverV2(PcMinimalsIngressOrigin::CanonicalPcMinimals)
        );
        assert_eq!(command.query().count_policy(), PcCountPolicy::CountUnique);
        assert_eq!(command.query().exact_pieces(), Some(1));

        let score = desktop_request_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc-scenario",
                "lines": 1,
                "visible_height": 1,
                "board_mask": "0x3f",
                "piece_window": 1,
                "queue": "I",
                "hold_enabled": true,
                "hold_piece": "empty",
                "count_policy": "all",
                "score_mode": "summary",
                "score_profile": "tetrio",
                "spin_profile": "t-spins",
                "backend": "cpu",
                "workers": 1,
                "allow_backend_fallback": false
            }"#,
        )
        .expect("canonical desktop pc score request");
        assert_eq!(
            score.product_capability_contract(),
            Some(ProductCapabilityContract::PcScore)
        );
        let AppCommand::Scenario(command) = score.command() else {
            panic!("expected scenario PC command");
        };
        assert_eq!(
            command.result_projection(),
            PcResultProjection::ScoreSummaryV2(PcScoreIngressOrigin::CanonicalPcScore)
        );
        assert_eq!(command.query().exact_pieces(), Some(1));
        assert_eq!(command.query().retained_trace_limit(), 1);
        let policy = command.query().execution_policy();
        assert_eq!(policy.requested_backend(), RequestedSearchBackend::Cpu);
        assert_eq!(policy.worker_policy(), WorkerPolicy::Fixed(1));
        assert!(!policy.allow_backend_fallback());
        assert_eq!(policy.max_patterns(), PC_SCORE_MAX_PATTERNS);

        let score_minimals = desktop_request_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc-scenario",
                "lines": 1,
                "visible_height": 1,
                "board_mask": "0x3f",
                "piece_window": 1,
                "queue": "I",
                "hold_enabled": true,
                "hold_piece": "empty",
                "count_policy": "all",
                "score_mode": "score-minimals",
                "score_profile": "tetrio",
                "spin_profile": "t-spins",
                "backend": "cpu",
                "workers": 1,
                "allow_backend_fallback": false
            }"#,
        )
        .expect("canonical desktop pc score-minimals request");
        assert_eq!(
            score_minimals.product_capability_contract(),
            Some(ProductCapabilityContract::PcScoreMinimals)
        );
        let AppCommand::Scenario(command) = score_minimals.command() else {
            panic!("expected scenario PC command");
        };
        assert_eq!(
            command.result_projection(),
            PcResultProjection::ScorePortfolioV2(
                PcScoreMinimalsIngressOrigin::CanonicalPcScoreMinimals
            )
        );
        assert_eq!(
            command.query().objective().kind(),
            ObjectiveKind::MinimumCover
        );
        assert!(command.query().objective().score().requested());
        assert_eq!(command.query().count_policy(), PcCountPolicy::CountAll);
        assert_eq!(command.query().retained_trace_limit(), 1);
        let policy = command.query().execution_policy();
        assert_eq!(policy.requested_backend(), RequestedSearchBackend::Cpu);
        assert_eq!(policy.worker_policy(), WorkerPolicy::Fixed(1));
        assert!(!policy.allow_backend_fallback());
        assert_eq!(policy.max_patterns(), PC_SCORE_MAX_PATTERNS);
    }

    #[test]
    fn legacy_desktop_pc_score_dto_normalizes_to_the_cli_owned_opening_and_scenario_contract() {
        let cases = [
            (
                r#"{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "pc",
                    "lines": 2,
                    "count_policy": "unique",
                    "score_mode": "summary",
                    "score_profile": "tetrio",
                    "spin_profile": "t-spins",
                    "backend": "cpu",
                    "workers": 1,
                    "allow_backend_fallback": false
                }"#,
                "clearra pc score --lines 2 --rule srs-plus --score-profile tetrio --spin-profile t-spins --initial-b2b 0 --workers 1",
            ),
            (
                r#"{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "pc-scenario",
                    "lines": 1,
                    "visible_height": 1,
                    "board_mask": "0x3f",
                    "piece_window": 1,
                    "queue": "I",
                    "hold_enabled": true,
                    "hold_piece": "empty",
                    "count_policy": "unique",
                    "score_mode": "summary",
                    "score_profile": "tetrio",
                    "spin_profile": "t-spins",
                    "backend": "cpu",
                    "workers": 1,
                    "allow_backend_fallback": false
                }"#,
                "clearra pc score --lines 1 --board-mask 0x3f --height 1 --pieces 1 --queue I --hold empty --rule srs-plus --score-profile tetrio --spin-profile t-spins --initial-b2b 0 --workers 1",
            ),
        ];

        for (legacy_json, cli_text) in cases {
            let legacy = desktop_request_builds_app_request(legacy_json)
                .expect("legacy desktop named pc score DTO");
            let cli = CliCommandParser::parse(cli_text)
                .expect("canonical pc score CLI")
                .to_app_request()
                .expect("canonical pc score AppRequest");

            assert_eq!(
                legacy.product_capability_contract(),
                Some(ProductCapabilityContract::PcScore)
            );
            assert_eq!(
                legacy.product_capability_contract(),
                cli.product_capability_contract()
            );
            match (legacy.command(), cli.command()) {
                (AppCommand::Pc(legacy), AppCommand::Pc(cli)) => {
                    assert_eq!(legacy.result_projection(), cli.result_projection());
                    assert_eq!(legacy.query().objective(), cli.query().objective());
                    assert_eq!(legacy.query().count_policy(), PcCountPolicy::CountAll);
                    assert_eq!(legacy.query().count_policy(), cli.query().count_policy());
                    assert_eq!(legacy.query().queue(), cli.query().queue());
                    assert_eq!(legacy.query().hold_policy(), cli.query().hold_policy());
                    assert_eq!(legacy.query().execution_policy(), cli.query().execution_policy());
                }
                (AppCommand::Scenario(legacy), AppCommand::Scenario(cli)) => {
                    assert_eq!(legacy.result_projection(), cli.result_projection());
                    assert_eq!(legacy.query(), cli.query());
                    assert_eq!(legacy.query().objective().kind(), ObjectiveKind::All);
                    assert_eq!(legacy.query().count_policy(), PcCountPolicy::CountAll);
                }
                (legacy, cli) => panic!(
                    "legacy and CLI pc score requests chose different command families: {legacy:?} != {cli:?}"
                ),
            }
        }
    }

    #[test]
    fn desktop_pc_path_attaches_the_complete_ordinary_replay_family_contract() {
        let request = desktop_request_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc-scenario",
                "lines": 1,
                "visible_height": 1,
                "board_mask": "0x3f0",
                "piece_window": 1,
                "queue": "I",
                "hold_enabled": true,
                "hold_piece": "empty",
                "count_policy": "all",
                "score_mode": "path"
            }"#,
        )
        .expect("canonical desktop pc.path request");
        assert_eq!(
            request.product_capability_contract(),
            Some(ProductCapabilityContract::PcPath)
        );
        let AppCommand::Scenario(command) = request.command() else {
            panic!("expected scenario PC command");
        };
        assert_eq!(
            command.result_projection(),
            PcResultProjection::PathFamilyV2(PcPathIngressOrigin::CanonicalPcPath)
        );
        assert_eq!(command.query().count_policy(), PcCountPolicy::CountAll);
        assert_eq!(command.query().exact_pieces(), Some(1));
    }

    #[cfg(feature = "wasm-cpu-runtime")]
    #[test]
    fn desktop_actual_pc_path_returns_the_complete_ordinary_replay_family() {
        let response = DesktopTauriCommandBridge::default()
            .run_request(
                r#"{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "pc-scenario",
                    "lines": 1,
                    "visible_height": 1,
                    "board_mask": "0x3f0",
                    "piece_window": 1,
                    "queue": "I",
                    "hold_enabled": true,
                    "hold_piece": "empty",
                    "count_policy": "all",
                    "score_mode": "path"
                }"#,
            )
            .expect("desktop actual pc.path request");
        let response: Value = serde_json::from_str(&response).expect("desktop response JSON");
        assert_eq!(response["status"], "success");
        assert_eq!(response["result"]["kind"], "pc-path-family.v2");
        assert_eq!(response["product_result_payload"]["contract"], "pc.path");
        assert_eq!(
            response["product_result_payload"]["result_kind"],
            "pc-path-family.v2"
        );
        let content = &response["product_result_payload"]["content"];
        assert_eq!(content["payload_kind"], "pc-path-family");
        let payload = &content["payload"];
        assert_eq!(payload["complete"], true);
        let witness_count = payload["witnesses"].as_array().unwrap().len().to_string();
        assert_eq!(
            payload["witness_count"].as_str(),
            Some(witness_count.as_str())
        );
        assert!(payload.get("tie_metadata").is_none());
        assert!(payload.get("tie_cursor").is_none());
    }

    #[test]
    fn desktop_pc_score_finder_attaches_its_fixed_witness_contract_and_rejects_drift() {
        const CANONICAL: &str = r#"{
            "app_request_model": "clearra-app/AppRequest",
            "command": "pc-scenario",
            "lines": 1,
            "visible_height": 1,
            "board_mask": "0x3f",
            "piece_window": 1,
            "queue": "I",
            "hold_enabled": true,
            "hold_piece": "empty",
            "count_policy": "all",
            "score_mode": "score-finder",
            "score_profile": "jstris-ultra",
            "spin_profile": "t-spins",
            "initial_b2b": 1,
            "backend": "cpu",
            "workers": 1,
            "allow_backend_fallback": false
        }"#;

        let request = desktop_request_builds_app_request(CANONICAL)
            .expect("canonical desktop pc score-finder request");
        assert_eq!(
            request.product_capability_contract(),
            Some(ProductCapabilityContract::PcScoreFinder)
        );
        let AppCommand::Scenario(command) = request.command() else {
            panic!("expected scenario PC command");
        };
        assert_eq!(
            command.result_projection(),
            PcResultProjection::ScoreSummaryV2(PcScoreIngressOrigin::CanonicalPcScoreFinder)
        );
        assert_eq!(command.query().remaining_queue().mode(), "fixed");
        assert_eq!(command.query().count_policy(), PcCountPolicy::CountAll);
        assert_eq!(command.query().retained_trace_limit(), 1);
        assert_eq!(
            command.query().objective().score().profile().as_str(),
            "jstris-ultra"
        );
        assert_eq!(
            command.query().objective().score().spin_profile().as_str(),
            "t-spins"
        );

        for request in [
            CANONICAL.replace("\"command\": \"pc-scenario\"", "\"command\": \"pc\""),
            CANONICAL.replace("\"queue\": \"I\"", "\"queue\": \"\""),
            CANONICAL.replace("\"queue\": \"I\"", "\"patterns\": \"P7\""),
            CANONICAL.replace(
                "\"score_profile\": \"jstris-ultra\"",
                "\"score_profile\": \"tetrio\"",
            ),
            CANONICAL.replace("\"initial_b2b\": 1", "\"initial_b2b\": 2"),
            CANONICAL.replace("\"count_policy\": \"all\"", "\"count_policy\": \"unique\""),
        ] {
            let error = desktop_request_builds_app_request(&request)
                .expect_err("desktop pc score-finder drift must fail closed");
            assert_eq!(error.code(), "desktop-invalid-request");
        }
    }

    #[test]
    fn desktop_pc_save_modes_attach_distinct_typed_product_contracts() {
        for (mode, contract, projection) in [
            (
                "saves",
                ProductCapabilityContract::PcSaves,
                PcResultProjection::SaveGroupsV2(PcSaveIngressOrigin::CanonicalPcSaves),
            ),
            (
                "best-save",
                ProductCapabilityContract::PcBestSave,
                PcResultProjection::BestSaveV2(PcSaveIngressOrigin::CanonicalPcBestSave),
            ),
        ] {
            let request = desktop_request_builds_app_request(&format!(
                r#"{{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "pc-scenario",
                    "lines": 2,
                    "visible_height": 2,
                    "board_mask": "0xf3fcf",
                    "piece_window": 1,
                    "patterns": "P7",
                    "hold_enabled": false,
                    "hold_piece": "empty",
                    "count_policy": "all",
                    "score_mode": "{mode}",
                    "backend": "cpu",
                    "workers": 1,
                    "allow_backend_fallback": false
                }}"#,
            ))
            .expect("canonical desktop pc save request");
            assert_eq!(request.product_capability_contract(), Some(contract));
            let AppCommand::Scenario(command) = request.command() else {
                panic!("expected scenario PC command");
            };
            assert_eq!(command.result_projection(), projection);
            assert_eq!(command.query().count_policy(), PcCountPolicy::CountAll);
            assert_eq!(command.query().retained_trace_limit(), 1);
        }

        for option in [
            r#""queue": "I""#,
            r#""count_policy": "unique""#,
            r#""queue_knowledge": "visible-7""#,
            r#""solution_probabilities": true"#,
            r#""memory_budget_mb": 64"#,
        ] {
            let error = desktop_request_builds_app_request(&format!(
                r#"{{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "pc-scenario",
                    "lines": 2,
                    "visible_height": 2,
                    "board_mask": "0xf3fcf",
                    "piece_window": 1,
                    "hold_enabled": false,
                    "hold_piece": "empty",
                    "count_policy": "all",
                    "score_mode": "saves",
                    "backend": "cpu",
                    {option}
                }}"#,
            ))
            .expect_err("desktop pc save override must fail closed");
            assert_eq!(
                error.code(),
                if option.contains("memory_budget_mb") {
                    "desktop-validation-failed"
                } else {
                    "desktop-invalid-request"
                },
                "{option}"
            );
        }
    }

    #[cfg(feature = "wasm-cpu-runtime")]
    #[test]
    fn desktop_actual_pc_score_finder_returns_a_normal_score_only_witness_family() {
        let response = DesktopTauriCommandBridge::default()
            .run_request(
                r#"{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "pc-scenario",
                    "lines": 1,
                    "visible_height": 1,
                    "board_mask": "0x3f",
                    "piece_window": 1,
                    "queue": "I",
                    "hold_enabled": true,
                    "hold_piece": "empty",
                    "count_policy": "all",
                    "score_mode": "score-finder",
                    "score_profile": "jstris-ultra",
                    "spin_profile": "t-spins",
                    "initial_b2b": 1,
                    "backend": "cpu",
                    "workers": 1,
                    "allow_backend_fallback": false
                }"#,
            )
            .expect("desktop actual pc score-finder request");
        let response: Value = serde_json::from_str(&response).expect("desktop response JSON");
        assert_eq!(response["status"], "success");
        assert_eq!(response["result"]["kind"], "pc-fixed-score-witness.v2");
        assert_eq!(
            response["product_result_payload"]["contract"],
            "pc.score-finder"
        );
        assert_eq!(
            response["product_result_payload"]["result_kind"],
            "pc-fixed-score-witness.v2"
        );
        assert_eq!(
            response["product_result_payload"]["content"]["payload_kind"],
            "score-pattern-winner-family"
        );
        let payload = &response["product_result_payload"]["content"]["payload"];
        assert_eq!(payload["equality"], "score-only-attack-informational");
        assert_eq!(
            payload["informational_attack_basis"],
            "canonical-equal-score-trace"
        );
        assert!(payload.get("tie_metadata").is_none());
        assert!(payload.get("tie_cursor").is_none());
    }

    #[cfg(feature = "wasm-cpu-runtime")]
    #[test]
    fn desktop_actual_pc_save_products_keep_full_distinct_typed_families() {
        use clearra_host_contract::ProductResultPayloadContent;

        use crate::GuiJobEvent;

        fn wait_for_completion(
            bridge: &mut DesktopTauriCommandBridge,
            job_id: u64,
        ) -> clearra_host_contract::AppResponse {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                for event in bridge
                    .drain_job_events(job_id)
                    .expect("drain desktop pc save events")
                {
                    match event {
                        GuiJobEvent::Completed { response, .. } => return response,
                        GuiJobEvent::Failed { code, .. } => {
                            panic!("desktop pc save failed: {code}")
                        }
                        GuiJobEvent::Cancelled { .. } => panic!("desktop pc save was cancelled"),
                        GuiJobEvent::Started { .. }
                        | GuiJobEvent::Progress { .. }
                        | GuiJobEvent::Diagnostic { .. } => {}
                    }
                }
                assert!(
                    Instant::now() < deadline,
                    "desktop pc save did not complete"
                );
                std::thread::yield_now();
            }
        }

        let mut bridge = DesktopTauriCommandBridge::default();
        for (mode, contract, result_kind) in [
            ("saves", "pc.saves", "pc-save-groups.v2"),
            ("best-save", "pc.best-save", "pc-best-save.v2"),
        ] {
            let request = format!(
                r#"{{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "pc-scenario",
                    "lines": 2,
                    "visible_height": 2,
                    "board_mask": "0xf3fcf",
                    "piece_window": 1,
                    "patterns": "P7",
                    "hold_enabled": false,
                    "hold_piece": "empty",
                    "count_policy": "all",
                    "score_mode": "{mode}",
                    "backend": "cpu",
                    "workers": 1,
                    "allow_backend_fallback": false
                }}"#,
            );
            let job_id = bridge.start_job(&request).expect("start desktop pc save");
            let response = wait_for_completion(&mut bridge, job_id);
            let payload = response
                .product_result_payload()
                .expect("desktop pc save retains product payload");
            assert_eq!(payload.contract(), contract);
            assert_eq!(payload.result_kind(), result_kind);
            match (mode, payload.content()) {
                ("saves", ProductResultPayloadContent::PcSaveGroups(groups)) => {
                    assert_eq!(groups.group_count(), groups.groups().len().to_string());
                    assert!(!groups.groups().is_empty());
                    assert!(groups.metadata().completeness().complete());
                }
                ("best-save", ProductResultPayloadContent::PcBestSave(best)) => {
                    assert_eq!(best.winner_count(), best.winners().len().to_string());
                    assert!(!best.winners().is_empty());
                    assert!(best.metadata().completeness().complete());
                }
                _ => panic!("desktop pc save product family mismatch"),
            }
        }
    }

    #[cfg(feature = "wasm-cpu-runtime")]
    #[test]
    fn desktop_actual_pc_minimals_transfers_pages_and_releases_on_navigation_cancel_and_release() {
        use clearra_host_contract::ProductResultPayloadContent;

        use crate::GuiJobEvent;

        const REQUEST: &str = r#"{
            "app_request_model": "clearra-app/AppRequest",
            "command": "pc-scenario",
            "lines": 1,
            "visible_height": 1,
            "board_mask": "0x3f",
            "piece_window": 1,
            "queue": "I",
            "hold_enabled": true,
            "hold_piece": "empty",
            "count_policy": "unique",
            "score_mode": "minimum-cover",
            "backend": "cpu",
            "workers": 1,
            "allow_backend_fallback": false
        }"#;

        fn wait_for_completion(
            bridge: &mut DesktopTauriCommandBridge,
            job_id: u64,
        ) -> clearra_host_contract::AppResponse {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                for event in bridge
                    .drain_job_events(job_id)
                    .expect("drain desktop pc minimals events")
                {
                    match event {
                        GuiJobEvent::Completed { response, .. } => return response,
                        GuiJobEvent::Failed { code, .. } => {
                            panic!("desktop pc minimals failed: {code}")
                        }
                        GuiJobEvent::Cancelled { .. } => {
                            panic!("desktop pc minimals was cancelled")
                        }
                        GuiJobEvent::Started { .. }
                        | GuiJobEvent::Progress { .. }
                        | GuiJobEvent::Diagnostic { .. } => {}
                    }
                }
                assert!(
                    Instant::now() < deadline,
                    "desktop pc minimals did not complete"
                );
                std::thread::yield_now();
            }
        }

        let mut bridge = DesktopTauriCommandBridge::default();
        let first_job = bridge
            .start_job(REQUEST)
            .expect("start desktop pc minimals");
        let response = wait_for_completion(&mut bridge, first_job);
        let payload = response
            .product_result_payload()
            .expect("desktop completed response retains product payload");
        assert_eq!(payload.contract(), "pc.minimals");
        assert_eq!(payload.result_kind(), "pc-minimum-cover.v2");
        let ProductResultPayloadContent::CoveragePortfolio(canonical) = payload.content() else {
            panic!("expected desktop coverage portfolio payload")
        };
        assert!(canonical.page_handle_available());
        assert_eq!(canonical.member_page_number(), "1");
        assert_eq!(canonical.members()[0].candidate_id(), "1");

        let page: Value = serde_json::from_str(
            &bridge
                .product_page_get("1", "1")
                .expect("load canonical desktop product page"),
        )
        .expect("desktop product page JSON");
        assert_eq!(page["state"], "page");
        assert_eq!(page["page"]["alternative_index"], "1");
        assert_eq!(page["page"]["member_page_number"], "1");
        assert_eq!(page["page"]["members"][0]["candidate_id"], "1");
        let next: Value = serde_json::from_str(
            &bridge
                .product_page_next(10_000)
                .expect("advance desktop product pages"),
        )
        .expect("desktop product page advance JSON");
        assert!(matches!(
            next["state"].as_str(),
            Some("page" | "sealed" | "work-budget-exhausted")
        ));

        let replacement_job = bridge
            .start_job(REQUEST)
            .expect("new desktop search replaces the page owner");
        assert!(bridge.product_page_get("1", "1").is_err());
        bridge
            .cancel_job(replacement_job)
            .expect("cancel replacement desktop search");
        assert!(bridge.product_page_get("1", "1").is_err());
        let cancel_deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let terminal = bridge
                .drain_job_events(replacement_job)
                .expect("drain cancelled desktop pc minimals job")
                .iter()
                .any(GuiJobEvent::is_terminal);
            if terminal {
                break;
            }
            assert!(
                Instant::now() < cancel_deadline,
                "cancelled desktop pc minimals job did not terminate"
            );
            std::thread::yield_now();
        }

        let release_job = bridge
            .start_job(REQUEST)
            .expect("start desktop pc minimals for explicit release");
        let _ = wait_for_completion(&mut bridge, release_job);
        assert!(bridge.product_page_get("1", "1").is_ok());
        bridge.product_page_release();
        assert!(bridge.product_page_get("1", "1").is_err());
    }

    #[cfg(feature = "wasm-cpu-runtime")]
    #[test]
    fn desktop_exact_page_replay_observes_the_host_cancellation_token() {
        use crate::GuiJobEvent;

        const REQUEST: &str = r#"{
            "app_request_model": "clearra-app/AppRequest",
            "command": "pc",
            "lines": 2,
            "queue": "IIOOO",
            "hold_enabled": false,
            "count_policy": "unique",
            "score_mode": "minimum-cover",
            "backend": "cpu",
            "workers": 1,
            "allow_backend_fallback": false
        }"#;

        let mut bridge = DesktopTauriCommandBridge::default();
        let job_id = bridge
            .start_job(REQUEST)
            .expect("start tied desktop minimals");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let terminal = bridge
                .drain_job_events(job_id)
                .expect("drain tied desktop minimals events")
                .into_iter()
                .find(GuiJobEvent::is_terminal);
            match terminal {
                Some(GuiJobEvent::Completed { .. }) => break,
                Some(GuiJobEvent::Failed { code, .. }) => {
                    panic!("tied desktop minimals failed: {code}")
                }
                Some(GuiJobEvent::Cancelled { .. }) => {
                    panic!("tied desktop minimals was cancelled")
                }
                Some(GuiJobEvent::Started { .. })
                | Some(GuiJobEvent::Progress { .. })
                | Some(GuiJobEvent::Diagnostic { .. })
                | None => {}
            }
            assert!(Instant::now() < deadline, "tied desktop minimals timed out");
            std::thread::yield_now();
        }

        let second_page = loop {
            let response: Value = serde_json::from_str(
                &bridge
                    .product_page_next(u64::MAX)
                    .expect("advance tied desktop portfolio"),
            )
            .expect("tied desktop portfolio JSON");
            match response["state"].as_str() {
                Some("page") => break response,
                Some("work-budget-exhausted") => continue,
                state => panic!("expected a second tied portfolio page, received {state:?}"),
            }
        };
        assert_eq!(second_page["page"]["alternative_index"], "2");

        let mut cancellation_checks = 0;
        let cancelled: Value = serde_json::from_str(
            &bridge
                .product_page_get_slice_with_cancel("2", "1", 1, &mut || {
                    cancellation_checks += 1;
                    true
                })
                .expect("cancelled exact replay must remain an incomplete advance"),
        )
        .expect("cancelled exact replay JSON");
        assert!(cancellation_checks > 0);
        assert_eq!(cancelled["state"], "cancelled");
        assert!(cancelled.get("page").is_none());

        let replayed: Value = serde_json::from_str(
            &bridge
                .product_page_get_slice_with_cancel("2", "1", 1, &mut || false)
                .expect("retry exact desktop replay"),
        )
        .expect("retried desktop replay JSON");
        assert_eq!(replayed["state"], "page");
        assert_eq!(replayed["page"]["alternative_index"], "2");
        bridge.product_page_release();
    }

    #[test]
    fn desktop_requests_reject_inactive_score_spin_and_knowledge_options() {
        for option in [
            r#""initial_b2b": 1"#,
            r#""score_profile": "guideline""#,
            r#""spin_profile": "all-spin""#,
            r#""score_mode": "failed-queue", "solution_probabilities": true"#,
        ] {
            desktop_request_builds_app_request(&format!(
                r#"{{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "pc-scenario",
                    "lines": 2,
                    "visible_height": 2,
                    "board_mask": 0,
                    "piece_window": 5,
                    {option}
                }}"#
            ))
            .expect_err(option);
        }

        for option in [
            r#""spin_profile": "all-spin""#,
            r#""pattern_knowledge": "oracle""#,
        ] {
            desktop_request_builds_app_request(&format!(
                r#"{{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "build-probability",
                    "height": 1,
                    "base_mask": 0,
                    "target_mask": 15,
                    "queue": "I",
                    {option}
                }}"#
            ))
            .expect_err(option);
        }
    }

    #[test]
    fn desktop_build_probability_preserves_backend_fallback_and_device_projection() {
        let request = desktop_request_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "build-probability",
                "height": 1,
                "base_mask": 0,
                "target_mask": 15,
                "queue": "I",
                "backend": "gpu",
                "gpu_device": "2",
                "allow_backend_fallback": true,
                "gpu_warmup": true
            }"#,
        )
        .expect("desktop build backend request");
        let AppCommand::BuildProbability(command) = request.command() else {
            panic!("expected build-probability command");
        };
        let policy = command.query().core_query().execution_policy();
        assert_eq!(policy.requested_backend(), RequestedSearchBackend::Gpu);
        assert!(policy.allow_backend_fallback());
        assert_eq!(policy.gpu_device(), &GpuDeviceSelection::Index(2));
        assert!(policy.gpu_warmup());
    }

    #[test]
    fn desktop_build_probability_includes_requested_solution_probabilities() {
        let request = desktop_request_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "build-probability",
                "height": 1,
                "base_mask": 0,
                "target_mask": 15,
                "queue": "I",
                "solution_probabilities": true
            }"#,
        )
        .expect("desktop build solution-probability request");
        let AppCommand::BuildProbability(command) = request.command() else {
            panic!("expected build-probability command");
        };
        assert_eq!(
            command.query().solution_probability_policy(),
            BuildSolutionProbabilityPolicy::Include
        );
    }

    #[test]
    fn desktop_build_probability_omits_solution_probabilities_by_default() {
        let request = desktop_request_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "build-probability",
                "height": 1,
                "base_mask": 0,
                "target_mask": 15,
                "queue": "I"
            }"#,
        )
        .expect("desktop default build request");
        let AppCommand::BuildProbability(command) = request.command() else {
            panic!("expected build-probability command");
        };
        assert_eq!(
            command.query().solution_probability_policy(),
            BuildSolutionProbabilityPolicy::Omit
        );
    }

    #[test]
    fn desktop_tiling_build_rejects_solution_probabilities() {
        let error = desktop_request_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "build-probability",
                "height": 1,
                "base_mask": 0,
                "target_mask": 15,
                "queue": "I",
                "build_aggregation": "tiling",
                "solution_probabilities": true
            }"#,
        )
        .expect_err("tiling-only build must reject solution probabilities");

        assert_eq!(error.code(), "desktop-invalid-request");
        assert!(error.message().contains("noncanonical inactive option"));
    }

    #[cfg(feature = "wasm-cpu-runtime")]
    #[test]
    fn desktop_build_source_pieces_changes_executed_universe_and_coverage() {
        use clearra_app::{AppContext, AppCoreExecutorService, AppServices, AppStatus};

        let context = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        let execute = |aggregation: &str, source_piece_count: usize| {
            let request = desktop_request_builds_app_request(&format!(
                r#"{{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "build-probability",
                    "height": 1,
                    "base_mask": 0,
                    "target_mask": 15,
                    "queue": "",
                    "patterns": "",
                    "hold_enabled": true,
                    "hold_piece": "empty",
                    "backend": "cpu",
                    "workers": 1,
                    "include_horizontal_mirror": false,
                    "build_aggregation": "{aggregation}",
                    "source_piece_count": {source_piece_count}
                }}"#
            ))
            .expect("typed desktop build source request");
            let response = context.run(request);
            assert_eq!(
                response.status(),
                AppStatus::Success,
                "{aggregation}/{source_piece_count}: {response:?}"
            );
            let result = response
                .render_model()
                .and_then(|model| model.core_result())
                .expect("executed build-probability result");
            (
                result
                    .usize_field("source_sequence_length")
                    .expect("source sequence length"),
                result
                    .field("total_possible_pattern_count")
                    .expect("pattern universe")
                    .to_owned(),
                result
                    .usize_field("covered_pattern_count")
                    .expect("covered pattern count"),
                result
                    .field("coverage_probability")
                    .expect("coverage probability")
                    .to_owned(),
            )
        };

        let one = execute("buildability", 1);
        let two = execute("buildability", 2);
        assert_eq!(
            one,
            (1, "7".to_owned(), 1, "0.14285714285714285".to_owned())
        );
        assert_eq!(
            two,
            (2, "42".to_owned(), 12, "0.2857142857142857".to_owned())
        );
        let tiling_one = execute("tiling", 1);
        let tiling_two = execute("tiling", 2);
        assert_eq!(
            tiling_one,
            (1, "7".to_owned(), 0, "not-calculated".to_owned())
        );
        assert_eq!(
            tiling_two,
            (2, "42".to_owned(), 0, "not-calculated".to_owned())
        );
        assert_ne!(tiling_one, tiling_two);
    }

    #[test]
    fn desktop_request_rejects_unknown_queue_knowledge() {
        let error = desktop_form_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 4,
                "queue_knowledge": "clairvoyant",
                "backend": "cpu"
            }"#,
        )
        .expect_err("unknown queue knowledge must fail closed");

        assert!(error.to_string().contains("queue_knowledge"));
    }

    #[test]
    fn desktop_pc_worker_aliases_preserve_native_auto_and_full_cpu_modes() {
        let hardware = clearra_pc_graph::request::WorkerPolicy::hardware_worker_limit();
        let default_limit = clearra_pc_graph::request::WorkerPolicy::default_worker_limit();

        let default_request = desktop_request_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 2,
                "backend": "cpu"
            }"#,
        )
        .expect("desktop default PC request");
        let AppCommand::Pc(default_command) = default_request.command() else {
            panic!("expected PC request");
        };
        assert_eq!(
            default_command.query().execution_policy().workers(),
            default_limit
        );

        for alias in ["use_all_logical_processors", "use_all_cpu_threads"] {
            let request_json = format!(
                r#"{{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "pc",
                    "lines": 2,
                    "backend": "cpu",
                    "workers": 0,
                    "{alias}": true
                }}"#
            );
            let request = desktop_request_builds_app_request(&request_json)
                .expect("desktop all-CPU PC request");
            let AppCommand::Pc(command) = request.command() else {
                panic!("expected PC request");
            };
            let policy = command.query().execution_policy();
            assert!(policy.use_all_logical_processors());
            assert_eq!(policy.workers_requested(), None);
            assert_eq!(policy.workers(), hardware);
        }
    }

    #[test]
    fn desktop_typed_worker_policy_uses_native_hardware_as_authority() {
        let hardware = clearra_pc_graph::request::WorkerPolicy::hardware_worker_limit();
        let lower_client_hint = hardware.saturating_sub(1).max(1);
        let request_json = format!(
            r#"{{
                "app_request_model": "clearra-app/AppRequest",
                "command": "setup",
                "setup_remaining": "IOTS",
                "workers": 0,
                "use_all_logical_processors": true,
                "worker_hardware_limit": {lower_client_hint}
            }}"#
        );
        let request = desktop_request_builds_app_request(&request_json)
            .expect("native-authoritative desktop setup request");
        assert_eq!(
            usize::from(request.resource_budget().workers()),
            hardware.min(usize::from(u16::MAX))
        );

        if let Some(overstated) = hardware.checked_add(1) {
            let request_json = format!(
                r#"{{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "setup",
                    "setup_remaining": "IOTS",
                    "use_all_cpu_threads": true,
                    "worker_hardware_limit": {overstated}
                }}"#
            );
            let error = desktop_request_builds_app_request(&request_json)
                .expect_err("overstated desktop hardware limit must fail closed");
            assert!(error
                .to_string()
                .contains("exceeds the native logical processor limit"));
        }
    }

    #[test]
    fn desktop_typed_setup_request_accepts_public_setup_keys_and_path_detail() {
        let request = desktop_request_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "setup",
                "language": "ko",
                "rule": "srs-plus",
                "setup_mode": "qb",
                "setup_remaining": "IOTS",
                "setup_qb": "ZJL",
                "setup_next_cycle_remaining": "IO",
                "setup_allow_post_cycle_borrow": true,
                "setup_priority": "build",
                "setup_length": "longer",
                "setup_max_pieces": 10,
                "setup_path_setup_id": "setup-00080719e6-0012-000000000000000000004210032007",
                "setup_path_condition_id": "hold-empty",
                "workers": 3
            }"#,
        )
        .expect("typed setup request");

        let AppCommand::Setup(command) = request.command() else {
            panic!("expected setup AppCommand");
        };
        let query = command.query();
        assert_eq!(query.search_mode(), SetupSearchMode::QueueBased);
        assert_eq!(
            query.candidate_priority(),
            SetupCandidatePriority::BuildProbabilityFirst
        );
        assert_eq!(query.length_preference(), SetupLengthPreference::Longer);
        assert_eq!(query.max_setup_pieces(), 10);
        let detail = query.path_detail().expect("setup path detail");
        assert_eq!(
            detail.setup_id(),
            "setup-00080719e6-0012-000000000000000000004210032007"
        );
        assert_eq!(detail.condition_id(), "hold-empty");
        assert_eq!(request.resource_budget().workers(), 3);
    }

    #[test]
    fn desktop_typed_build_probability_uses_build_aggregation_not_cli_text() {
        let request = desktop_request_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "build-probability",
                "visible_height": 1,
                "base_mask": "0x0",
                "target_mask": "0xf",
                "queue": "I",
                "patterns": "",
                "hold_enabled": true,
                "hold_piece": "empty",
                "build_aggregation": "tiling",
                "spin_profile": "t-spins",
                "preserve_b2b": false,
                "precompute_build_dependencies": false,
                "include_horizontal_mirror": true,
                "workers": 2
            }"#,
        )
        .expect("typed build-probability request");

        let AppCommand::BuildProbability(command) = request.command() else {
            panic!("expected build-probability AppCommand");
        };
        assert_eq!(
            command.query().aggregation(),
            BuildProbabilityAggregation::TilingOnly
        );
        assert_eq!(command.query().field().height(), 1);
    }

    #[test]
    fn desktop_typed_damage_request_preserves_threshold_and_chain_state() {
        let request = desktop_request_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "damage",
                "visible_height": 4,
                "board_mask": "0x0",
                "queue": "TIO",
                "patterns": "",
                "hold_enabled": true,
                "rule": "srs-plus",
                "spin_profile": "all-mini-plus",
                "preserve_b2b": true,
                "initial_combo": 2,
                "initial_b2b": 3,
                "damage_aggregation": "at-least",
                "minimum_damage": 6,
                "workers": 2
            }"#,
        )
        .expect("typed damage request");

        let AppCommand::Damage(command) = request.command() else {
            panic!("expected damage AppCommand");
        };
        let query = command.query();
        assert_eq!(query.mode(), ForwardSearchMode::DamageAtLeast(6));
        assert_eq!(query.initial_combo(), Some(2));
        assert_eq!(query.initial_back_to_back(), Some(2));
        assert_eq!(
            query.line_clear_policy(),
            ForwardLineClearPolicy::PreserveBackToBack
        );
    }

    #[test]
    fn desktop_forward_commands_reject_cross_mode_and_inactive_options() {
        for request_json in [
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "damage",
                "queue": "T",
                "minimum_damage": 1
            }"#,
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "damage",
                "queue": "T",
                "spin_lines": "2+"
            }"#,
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "spin-finder",
                "queue": "T",
                "damage_aggregation": "at-least"
            }"#,
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "spin-finder",
                "queue": "T",
                "minimum_damage": 1
            }"#,
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "spin-finder",
                "queue": "T",
                "spin_profile": "t-spins",
                "spin_category": "other"
            }"#,
        ] {
            desktop_request_builds_app_request(request_json)
                .expect_err("inactive forward-search option must fail closed");
        }
    }

    #[test]
    fn desktop_typed_spin_finder_request_accepts_pattern_and_target() {
        let request = desktop_request_builds_app_request(
            r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "spin-finder",
                "visible_height": 6,
                "board_mask": "0x0",
                "queue": "",
                "patterns": "[TIO]!",
                "hold_enabled": false,
                "rule": "srs-plus",
                "spin_profile": "t-spins-plus",
                "spin_lines": "2+",
                "spin_category": "t",
                "workers": 1
            }"#,
        )
        .expect("typed spin-finder request");

        let AppCommand::SpinFinder(command) = request.command() else {
            panic!("expected spin-finder AppCommand");
        };
        let ForwardSearchMode::SpinFinder(target) = command.query().mode() else {
            panic!("expected spin-finder mode");
        };
        assert_eq!(
            target.line_requirement(),
            ForwardSpinLineRequirement::AtLeast(2)
        );
        assert_eq!(target.category(), ForwardSpinCategory::T);
        assert!(command.query().piece_source().is_pattern());
    }

    #[test]
    fn desktop_typed_dispatch_rejects_cli_text_fields() {
        for field in ["cli_text", "command_text"] {
            let request = format!(
                r#"{{
                    "app_request_model": "clearra-app/AppRequest",
                    "command": "setup",
                    "setup_remaining": "IOTSZJL",
                    "{field}": "clearra setup --remaining IOTSZJL"
                }}"#
            );
            let error = desktop_request_builds_app_request(&request)
                .expect_err("desktop typed bridge must reject CLI text");
            assert!(error.to_string().contains("does not parse CLI text"));
        }
    }

    #[test]
    fn desktop_cli_argv_envelope_is_fieldwise_identical_to_the_shared_compiler() {
        let arguments = vec![
            "clearra".to_owned(),
            "pc".to_owned(),
            "path".to_owned(),
            "--lines".to_owned(),
            "2".to_owned(),
            "--patterns".to_owned(),
            "P7".to_owned(),
            "--no-hold".to_owned(),
        ];
        let desktop = desktop_request_builds_app_request(
            &json!({
                "app_request_model": "clearra-cli/CommandRequest",
                "command": "cli",
                "language": "ko",
                "arguments": arguments,
            })
            .to_string(),
        )
        .expect("closed canonical CLI argv envelope");
        let direct = CliCommandParser::parse_tokens(&arguments)
            .expect("shared CLI parser")
            .to_app_request()
            .expect("shared typed lowering")
            .with_language(LanguageId::Ko);

        assert_eq!(desktop, direct);
        assert!(matches!(
            desktop.command(),
            AppCommand::Pc(command)
                if matches!(command.result_projection(), PcResultProjection::PathFamilyV2(_))
        ));
    }

    #[test]
    fn desktop_cli_argv_envelope_admits_gui_document_utilities_without_reparsing_text() {
        let arguments = vec![
            "clearra".to_owned(),
            "utility".to_owned(),
            "fumen".to_owned(),
            "text-to-fumen".to_owned(),
            "--format".to_owned(),
            "fumen".to_owned(),
            "--comment".to_owned(),
            "comment with spaces".to_owned(),
        ];
        let desktop = desktop_request_builds_app_request(
            &json!({
                "app_request_model": "clearra-cli/CommandRequest",
                "command": "cli",
                "language": "en",
                "arguments": arguments,
            })
            .to_string(),
        )
        .expect("desktop utility argv envelope");
        let direct = CliCommandParser::parse_tokens(&arguments)
            .expect("shared utility parser")
            .to_app_request()
            .expect("shared utility lowering")
            .with_language(LanguageId::En);

        assert_eq!(desktop, direct);
    }

    #[test]
    fn desktop_cli_argv_envelope_fails_closed_before_semantic_lowering() {
        let base = json!({
            "app_request_model": "clearra-cli/CommandRequest",
            "command": "cli",
            "language": "en",
            "arguments": ["clearra", "pc", "--lines", "2"],
        });
        let cases = [
            ("extra transport field", {
                let mut value = base.clone();
                value["score_mode"] = json!("off");
                value
            }),
            ("non-string token", {
                let mut value = base.clone();
                value["arguments"] = json!(["clearra", "pc", "--lines", 2]);
                value
            }),
            ("non-GUI command root", {
                let mut value = base.clone();
                value["arguments"] = json!(["clearra", "verify"]);
                value
            }),
            ("removed save groups surface", {
                let mut value = base.clone();
                value["arguments"] =
                    json!(["clearra", "pc", "saves", "--lines", "2", "--patterns", "P7"]);
                value
            }),
            ("removed best-save surface", {
                let mut value = base.clone();
                value["arguments"] = json!([
                    "clearra",
                    "pc",
                    "best-save",
                    "--lines",
                    "2",
                    "--patterns",
                    "P7"
                ]);
                value
            }),
            ("malformed canonical option", {
                let mut value = base.clone();
                value["arguments"] = json!(["clearra", "pc", "--lines", "not-a-number"]);
                value
            }),
        ];

        for (label, value) in cases {
            desktop_request_builds_app_request(&value.to_string()).expect_err(label);
        }
    }

    #[test]
    fn desktop_typed_setup_reaches_validate_run_and_start_endpoints() {
        let request = r#"{
            "app_request_model": "clearra-app/AppRequest",
            "command": "setup",
            "setup_mode": "oracle",
            "setup_remaining": "IOTS",
            "setup_max_pieces": 0,
            "workers": 1
        }"#;
        let mut bridge = DesktopTauriCommandBridge::default();

        let validation: serde_json::Value = serde_json::from_str(
            &bridge
                .validate_request(request)
                .expect("validate typed setup request"),
        )
        .expect("validation JSON");
        assert_eq!(validation["valid"], false);

        let run_response: serde_json::Value = serde_json::from_str(
            &bridge
                .run_request(request)
                .expect("run typed setup request"),
        )
        .expect("run response JSON");
        assert!(run_response.is_object());

        let job_id = bridge.start_job(request).expect("start typed setup job");
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let events: serde_json::Value =
                serde_json::from_str(&bridge.get_job_events(job_id).expect("poll typed setup job"))
                    .expect("job event JSON");
            let terminal = events.as_array().is_some_and(|events| {
                events.iter().any(|event| {
                    matches!(
                        event.get("event").and_then(serde_json::Value::as_str),
                        Some("completed" | "failed" | "cancelled")
                    )
                })
            });
            if terminal {
                break;
            }
            assert!(Instant::now() < deadline, "typed setup job did not finish");
            std::thread::yield_now();
        }
    }
}
// SRP rationale: this module has one behavior-level change reason: translating the complete
// typed desktop request and event contract across the Tauri host boundary.
