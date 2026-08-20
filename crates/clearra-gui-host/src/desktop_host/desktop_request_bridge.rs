mod bridge {
    use clearra_app::AppContext;
    #[cfg(feature = "wasm-cpu-runtime")]
    use clearra_app::{AppCoreExecutorService, AppServices};

    use crate::{GuiJobHandle, GuiJobId, GuiJobQueue};

    #[derive(Debug)]
    pub struct DesktopTauriCommandBridge {
        pub(super) app_context: AppContext,
        pub(super) queue: GuiJobQueue,
        pub(super) active_job: Option<GuiJobHandle>,
        pub(super) active_job_id: Option<GuiJobId>,
    }

    impl DesktopTauriCommandBridge {
        pub fn new(app_context: AppContext) -> Self {
            Self {
                app_context,
                queue: GuiJobQueue::new(),
                active_job: None,
                active_job_id: None,
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
            return AppContext::new(
                AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
            );
        }

        #[cfg(not(feature = "wasm-cpu-runtime"))]
        AppContext::default()
    }
}
mod cancel_job {
    use crate::GuiJobId;

    use super::{bridge::DesktopTauriCommandBridge, error::DesktopTauriCommandError};

    impl DesktopTauriCommandBridge {
        pub fn cancel_job(&self, job_id: u64) -> Result<(), DesktopTauriCommandError> {
            if self.active_job_id.map(GuiJobId::get) != Some(job_id) {
                return Err(DesktopTauriCommandError::job("desktop active job mismatch"));
            }
            self.active_job
                .as_ref()
                .ok_or_else(|| DesktopTauriCommandError::job("desktop job handle missing"))?
                .cancel();
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
mod form_parser {
    use clearra_app::AppRequest;
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
        objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
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
    use clearra_web_command::{WebBuildProbabilityInput, WebCommandRequest};
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
        validate_app_request_envelope(&value)?;
        let command = text_or_default(&value, &["command"], "pc")?;
        let request = match command {
            "pc" | "pc-scenario" => {
                let state = desktop_form_builds_app_request(request_json)?;
                GuiToAppRequest::build(&state)
                    .map_err(|error| DesktopTauriCommandError::validation(error.to_string()))?
                    .into_app_request()
            }
            "setup" => build_setup_app_request(&value)?,
            "build-probability" => build_probability_app_request(&value)?,
            "damage" | "spin-finder" => build_forward_app_request(&value, command)?,
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
        Ok(request.with_language(language))
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
        let base_objective_kind = match command {
            "pc" if score_mode == "failed-queue" => ObjectiveKind::All,
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
        let score_active = score_mode == "summary";
        if (!score_active && initial_b2b != 0)
            || (!score_active && score_profile != "tetrio")
            || (!score_active && !preserve_b2b && spin_profile != "t-spins")
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
        let mut request = WebCommandRequest::setup(remaining, allow_post_cycle_borrow)
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
        typed_web_request_to_app_request("setup", request)
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

        let mut request = WebCommandRequest::build_probability(input)
            .with_rule(parse_desktop_rule(value)?)
            .with_hold_enabled(hold_enabled)
            .with_precompute_build_dependencies(precompute_build_dependencies)
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
        typed_web_request_to_app_request("build-probability", request)
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
        let spin_profile_text = text_or_default(value, &["spin_profile"], "t-spins")?;
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
        } else {
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
        let request = apply_worker_policy(WebCommandRequest::forward(command, query), value)?;
        typed_web_request_to_app_request(command, request)
    }

    fn typed_web_request_to_app_request(
        command: &str,
        request: WebCommandRequest,
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

    fn apply_queue(
        mut request: WebCommandRequest,
        value: &Value,
    ) -> Result<WebCommandRequest, DesktopTauriCommandError> {
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
        mut request: WebCommandRequest,
        value: &Value,
    ) -> Result<WebCommandRequest, DesktopTauriCommandError> {
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
        mut request: WebCommandRequest,
        value: &Value,
    ) -> Result<WebCommandRequest, DesktopTauriCommandError> {
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
    const PC_SCENARIO_AND_FORWARD_COMMANDS: &[&str] = &["pc-scenario", "damage", "spin-finder"];
    const SCENARIO_BUILD_FORWARD_COMMANDS: &[&str] =
        &["pc-scenario", "build-probability", "damage", "spin-finder"];
    const PC_AND_DAMAGE_COMMANDS: &[&str] = &["pc", "pc-scenario", "damage"];
    const ALL_NON_SETUP_COMMANDS: &[&str] = &[
        "pc",
        "pc-scenario",
        "build-probability",
        "damage",
        "spin-finder",
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
            active_commands: PC_BUILD_FORWARD_COMMANDS,
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
            active_commands: PC_BUILD_FORWARD_COMMANDS,
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
            active_commands: PC_COMMANDS,
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
            "pc" | "pc-scenario" | "setup" | "build-probability" | "damage" | "spin-finder"
        ) {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop request bridge supports pc, pc-scenario, setup, build-probability, damage, and spin-finder commands",
            ));
        }
        validate_command_inactive_fields(value, command)
    }
}
mod get_job_events {
    use crate::{GuiJobEvent, GuiJobId};

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
mod run_request {
    use super::{
        bridge::DesktopTauriCommandBridge, error::DesktopTauriCommandError,
        form_parser::desktop_request_builds_app_request,
    };

    impl DesktopTauriCommandBridge {
        pub fn run_request(&self, request_json: &str) -> Result<String, DesktopTauriCommandError> {
            let request = desktop_request_builds_app_request(request_json)?;
            let response = self.app_context.run(request);
            serde_json::to_string(&response.to_host_response()).map_err(|error| {
                DesktopTauriCommandError::job(format!("serialize AppResponse: {error}"))
            })
        }
    }
}
mod start_job {
    use crate::GuiJobRunner;

    use super::{
        bridge::DesktopTauriCommandBridge, error::DesktopTauriCommandError,
        form_parser::desktop_request_builds_app_request,
    };

    impl DesktopTauriCommandBridge {
        pub fn start_job(&mut self, request_json: &str) -> Result<u64, DesktopTauriCommandError> {
            self.reap_finished_job_before_start()?;
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
    use serde_json::json;

    use super::{
        bridge::DesktopTauriCommandBridge, error::DesktopTauriCommandError,
        form_parser::desktop_request_builds_app_request,
    };

    impl DesktopTauriCommandBridge {
        pub fn validate_request(
            &self,
            request_json: &str,
        ) -> Result<String, DesktopTauriCommandError> {
            let request = desktop_request_builds_app_request(request_json)?;
            let report = self.app_context.validate_request(&request);
            let diagnostics = report
                .validation()
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().as_str().to_owned())
                .collect::<Vec<_>>();
            let valid = !report.has_errors();

            Ok(json!({
                "schema_version": 1,
                "command": "validate_request",
                "app_request_model": "clearra-app/AppRequest",
                "valid": valid,
                "diagnostics": diagnostics
            })
            .to_string())
        }
    }
}

pub use bridge::DesktopTauriCommandBridge;
pub use error::DesktopTauriCommandError;

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use clearra_app::AppCommand;
    use clearra_forward_search::{
        ForwardLineClearPolicy, ForwardSearchMode, ForwardSpinCategory, ForwardSpinLineRequirement,
    };
    use clearra_pc_graph::request::{GpuDeviceSelection, RequestedSearchBackend};
    use clearra_problem::{
        BuildProbabilityAggregation, SetupCandidatePriority, SetupLengthPreference, SetupSearchMode,
    };
    use clearra_supply::queue::queue_observation_policy::QueueObservationPolicy;
    use clearra_web_command::WebCommandParser;
    use serde_json::{json, Value};

    use crate::GuiProblemForm;

    use super::{
        form_parser::{desktop_form_builds_app_request, desktop_request_builds_app_request},
        DesktopTauriCommandBridge,
    };

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
    fn desktop_backend_omission_defaults_match_web_for_all_four_backends() {
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
            let web = WebCommandParser::parse(&format!("clearra pc --lines 2 --backend {backend}"))
                .expect("web backend-default request")
                .to_app_request()
                .expect("web backend-default AppRequest");

            assert_eq!(
                desktop.backend_policy().allow_backend_fallback(),
                expected_fallback,
                "desktop {backend} fallback default"
            );
            assert_eq!(
                desktop.backend_policy(),
                web.backend_policy(),
                "Desktop/Web backend projection for {backend}"
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
        desktop_request_builds_app_request(
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
