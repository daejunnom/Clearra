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
    use clearra_supply::queue::queue_observation_policy::QueueObservationPolicy;
    use serde_json::Value;

    use crate::{GuiAppState, GuiBackendForm, GuiOpeningPcForm, GuiProblemForm};

    use super::error::DesktopTauriCommandError;

    pub(super) fn desktop_form_builds_app_request(
        request_json: &str,
    ) -> Result<GuiAppState, DesktopTauriCommandError> {
        let value: Value = serde_json::from_str(request_json)
            .map_err(|error| DesktopTauriCommandError::invalid_request(error.to_string()))?;
        validate_app_request_envelope(&value)?;

        let command = value.get("command").and_then(Value::as_str).unwrap_or("pc");
        let language = value
            .get("language")
            .and_then(Value::as_str)
            .unwrap_or("en");
        let lines = value
            .get("lines")
            .and_then(Value::as_u64)
            .and_then(|lines| u8::try_from(lines).ok())
            .unwrap_or(2);
        let rule = value
            .get("rule")
            .and_then(Value::as_str)
            .unwrap_or("srs-plus");
        let backend = value
            .get("backend")
            .and_then(Value::as_str)
            .unwrap_or("auto");
        let queue = value.get("queue").and_then(Value::as_str).unwrap_or("");
        let patterns = value.get("patterns").and_then(Value::as_str).unwrap_or("");
        if !queue.trim().is_empty() && !patterns.trim().is_empty() {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop queue and patterns are mutually exclusive",
            ));
        }
        let hold_enabled = value
            .get("hold_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let queue_observation_policy = value
            .get("queue_knowledge")
            .and_then(Value::as_str)
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
                let visible_height = value
                    .get("visible_height")
                    .and_then(Value::as_u64)
                    .and_then(|height| u8::try_from(height).ok())
                    .unwrap_or(lines);
                let board_mask = parse_board_mask(value.get("board_mask"))?;
                let piece_window = value
                    .get("piece_window")
                    .and_then(Value::as_u64)
                    .and_then(|count| usize::try_from(count).ok())
                    .ok_or_else(|| {
                        DesktopTauriCommandError::invalid_request(
                            "desktop scenario PC requires a positive piece_window",
                        )
                    })?;
                let hold_piece = parse_hold_piece(value.get("hold_piece"))?;
                let count_policy = value
                    .get("count_policy")
                    .and_then(Value::as_str)
                    .unwrap_or("unique");
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
        let score_mode = value
            .get("score_mode")
            .and_then(Value::as_str)
            .unwrap_or("off");
        let initial_b2b = optional_u32(&value, "initial_b2b")?.unwrap_or(0);
        let score_profile = value
            .get("score_profile")
            .and_then(Value::as_str)
            .unwrap_or("tetrio");
        let spin_profile = value
            .get("spin_profile")
            .and_then(Value::as_str)
            .unwrap_or("t-spins");
        let solution_probabilities = value
            .get("solution_probabilities")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let preserve_b2b = value
            .get("preserve_b2b")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let problem_form = problem_form
            .with_queue_observation_policy(queue_observation_policy)
            .with_score_input(score_mode, initial_b2b)
            .with_score_profiles(score_profile, spin_profile)
            .with_back_to_back_preservation(preserve_b2b)
            .with_solution_probabilities(solution_probabilities);

        let mut backend_form = GuiBackendForm::from_backend_id(backend).with_allow_fallback(
            value
                .get("allow_backend_fallback")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        );
        if let Some(workers) = optional_u16(&value, "workers")? {
            if workers > 0 {
                backend_form = backend_form.with_workers(workers);
            }
        }
        if let Some(device) = value.get("gpu_device").and_then(Value::as_str) {
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
        let Some(value) = value.and_then(Value::as_str) else {
            return Ok(None);
        };
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
        value
            .get(key)
            .map(|entry| {
                entry
                    .as_u64()
                    .and_then(|number| u16::try_from(number).ok())
                    .ok_or_else(|| {
                        DesktopTauriCommandError::invalid_request(format!("{key} must fit in u16"))
                    })
            })
            .transpose()
    }

    fn optional_u32(value: &Value, key: &str) -> Result<Option<u32>, DesktopTauriCommandError> {
        value
            .get(key)
            .map(|entry| {
                entry
                    .as_u64()
                    .and_then(|number| u32::try_from(number).ok())
                    .ok_or_else(|| {
                        DesktopTauriCommandError::invalid_request(format!("{key} must fit in u32"))
                    })
            })
            .transpose()
    }

    fn validate_app_request_envelope(value: &Value) -> Result<(), DesktopTauriCommandError> {
        let model = value
            .get("app_request_model")
            .and_then(Value::as_str)
            .unwrap_or("clearra-app/AppRequest");
        if model != "clearra-app/AppRequest" {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop host accepts only clearra-app/AppRequest JSON",
            ));
        }
        if value.get("cli_text").is_some() {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop host does not parse CLI text",
            ));
        }
        if !matches!(
            value.get("command").and_then(Value::as_str).unwrap_or("pc"),
            "pc" | "pc-scenario"
        ) {
            return Err(DesktopTauriCommandError::invalid_request(
                "desktop request bridge supports pc and pc-scenario commands",
            ));
        }
        Ok(())
    }
}
mod get_job_events {
    use serde_json::Value;

    use crate::{GuiJobEvent, GuiJobId};

    use super::{
        bridge::DesktopTauriCommandBridge, error::DesktopTauriCommandError,
        job_event_json::job_event_to_json,
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
            let events = self
                .drain_job_events(job_id)?
                .iter()
                .map(job_event_to_json)
                .collect::<Vec<Value>>();
            serde_json::to_string(&events).map_err(|error| {
                DesktopTauriCommandError::job(format!("serialize desktop job events: {error}"))
            })
        }
    }
}
mod job_event_json {
    use serde_json::{json, Value};

    use crate::GuiJobEvent;

    pub(super) fn job_event_to_json(event: &GuiJobEvent) -> Value {
        match event {
            GuiJobEvent::Started { job_id } => json!({
                "schema_version": 1,
                "event": "started",
                "job_id": job_id.get()
            }),
            GuiJobEvent::Progress { job_id, progress } => json!({
                "schema_version": 1,
                "event": "progress",
                "job_id": job_id.get(),
                "done": progress.done(),
                "total": progress.total(),
                "label": progress.label(),
                "budget_status": progress.budget_status().state(),
                "resource_status": {
                    "budget_status": progress.budget_status().state(),
                    "done": progress.done(),
                    "total": progress.total()
                },
                "backend_status": {
                    "backend_requested": progress.backend_status().backend_requested(),
                    "backend_selected": progress.backend_status().backend_selected(),
                    "fallback_used": progress.backend_status().fallback_used()
                },
                "memory_status": {
                    "state": progress.memory_status().state(),
                    "leak_report_clean": progress.memory_status().leak_report_clean(),
                    "raw_pointer_exposed": progress.memory_status().raw_pointer_exposed()
                }
            }),
            GuiJobEvent::Diagnostic {
                job_id,
                code,
                severity,
            } => json!({
                "schema_version": 1,
                "event": "diagnostic",
                "job_id": job_id.get(),
                "code": code,
                "severity": severity
            }),
            GuiJobEvent::Completed { job_id, response } => json!({
                "schema_version": 1,
                "event": "completed",
                "job_id": job_id.get(),
                "response": response
            }),
            GuiJobEvent::Failed { job_id, code } => json!({
                "schema_version": 1,
                "event": "failed",
                "job_id": job_id.get(),
                "code": code
            }),
            GuiJobEvent::Cancelled { job_id } => json!({
                "schema_version": 1,
                "event": "cancelled",
                "job_id": job_id.get(),
                "scope_released": true
            }),
        }
    }
}
mod run_request {
    use crate::GuiToAppRequest;

    use super::{
        bridge::DesktopTauriCommandBridge, error::DesktopTauriCommandError,
        form_parser::desktop_form_builds_app_request,
    };

    impl DesktopTauriCommandBridge {
        pub fn run_request(&self, request_json: &str) -> Result<String, DesktopTauriCommandError> {
            let state = desktop_form_builds_app_request(request_json)?;
            let request = GuiToAppRequest::build(&state)
                .map_err(|error| DesktopTauriCommandError::validation(error.to_string()))?
                .into_app_request();
            let response = self.app_context.run(request);
            serde_json::to_string(&response.to_host_response()).map_err(|error| {
                DesktopTauriCommandError::job(format!("serialize AppResponse: {error}"))
            })
        }
    }
}
mod start_job {
    use crate::{GuiJobRunner, GuiToAppRequest};

    use super::{
        bridge::DesktopTauriCommandBridge, error::DesktopTauriCommandError,
        form_parser::desktop_form_builds_app_request,
    };

    impl DesktopTauriCommandBridge {
        pub fn start_job(&mut self, request_json: &str) -> Result<u64, DesktopTauriCommandError> {
            if self.active_job.is_some() {
                return Err(DesktopTauriCommandError::job("desktop job already active"));
            }
            let state = desktop_form_builds_app_request(request_json)?;
            let request = GuiToAppRequest::build(&state)
                .map_err(|error| DesktopTauriCommandError::validation(error.to_string()))?
                .into_app_request();
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
    }
}
mod validate_request {
    use serde_json::json;

    use crate::GuiToAppRequest;

    use super::{
        bridge::DesktopTauriCommandBridge, error::DesktopTauriCommandError,
        form_parser::desktop_form_builds_app_request,
    };

    impl DesktopTauriCommandBridge {
        pub fn validate_request(
            &self,
            request_json: &str,
        ) -> Result<String, DesktopTauriCommandError> {
            let state = desktop_form_builds_app_request(request_json)?;
            let (valid, diagnostics) = match GuiToAppRequest::build(&state) {
                Ok(build) => {
                    let report = self.app_context.validate_request(build.app_request());
                    let diagnostics = report
                        .validation()
                        .diagnostics()
                        .iter()
                        .map(|diagnostic| diagnostic.code().as_str().to_owned())
                        .collect::<Vec<_>>();
                    (!report.has_errors(), diagnostics)
                }
                Err(error) => (false, vec![format!("{:?}", error.code())]),
            };

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
    use clearra_supply::queue::queue_observation_policy::QueueObservationPolicy;

    use crate::GuiProblemForm;

    use super::form_parser::desktop_form_builds_app_request;

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
}
