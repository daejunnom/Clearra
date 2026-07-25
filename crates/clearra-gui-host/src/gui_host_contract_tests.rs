use crate::{
    DesktopTauriCommandBridge, GuiAppState, GuiBackendChoice, GuiBackendForm, GuiExecutionPhase,
    GuiExecutionState, GuiHostLanguageResolver, GuiJobId, GuiOpeningPcForm, GuiOutputFormat,
    GuiProblemForm, GuiScreen, PcRequestBuilder, SetupRequestBuilder,
};
use serde_json::Value;

mod case_gui_pc_request_preserves_back_to_back_constraint {
    use clearra_app::AppCommand;

    use super::*;

    #[test]
    fn gui_pc_request_preserves_back_to_back_constraint() {
        let form = GuiOpeningPcForm::new(2, "srs-plus")
            .with_score_profiles("tetrio", "all-mini-plus")
            .with_back_to_back_preservation(true);
        let command = PcRequestBuilder::build_command(&form, &GuiBackendForm::default())
            .expect("GUI PC request");
        let AppCommand::Pc(command) = command else {
            panic!("expected PC command");
        };
        let constraint = command.query().objective().execution_constraints();

        assert!(constraint.preserves_back_to_back());
        assert_eq!(constraint.spin_profile().as_str(), "all-mini-plus");
        assert!(!command.query().objective().score().requested());
    }
}

mod case_gui_opening_pc_preserves_hold_selection_without_queue_override {
    use clearra_app::AppCommand;

    use super::*;

    #[test]
    fn gui_opening_pc_preserves_hold_selection_without_queue_override() {
        let form = GuiOpeningPcForm::new(2, "srs-plus").with_hold_enabled(false);
        let command = PcRequestBuilder::build_command(&form, &GuiBackendForm::default())
            .expect("GUI PC request");
        let AppCommand::Pc(command) = command else {
            panic!("expected PC command");
        };

        assert!(!command.query().hold_policy().is_enabled());
    }
}

mod case_gui_setup_request_uses_residue_and_cycle_boundary_policy {
    use clearra_app::AppCommand;
    use clearra_problem::SetupCycleResetBorrowPolicy;

    use super::*;

    #[test]
    fn gui_setup_request_uses_residue_and_cycle_boundary_policy() {
        let form = crate::GuiSetupSearchForm::new("I,T,O", true, "srs-plus");
        let command = SetupRequestBuilder::build_command(&form, &GuiBackendForm::default())
            .expect("GUI setup request");
        let AppCommand::Setup(command) = command else {
            panic!("expected setup command");
        };

        assert_eq!(command.query().residue().remaining_count(), 3);
        assert_eq!(command.query().residue().cycle(), Some(7));
        assert_eq!(
            command.query().cycle_reset_borrow_policy(),
            SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse
        );
    }
}

mod case_gui_app_state_builds_app_request_preview_from_forms {
    use super::*;

    #[test]
    fn gui_app_state_builds_app_request_preview_from_forms() {
        let state = GuiAppState::default();
        let preview = state.app_request_preview().expect("AppRequest preview");

        assert_eq!(state.current_language(), "en");
        assert_eq!(state.current_screen(), GuiScreen::PcSearch);
        assert_eq!(preview.request_model(), "clearra-app/AppRequest");
        assert_eq!(preview.app_request_kind(), "Pc");
        assert_eq!(preview.selected_problem_preset(), "opening-pc");
        assert_eq!(preview.selected_backend(), "auto");
        assert_eq!(preview.selected_lines(), 2);
        assert_eq!(preview.selected_rule(), "srs-plus");
        assert_eq!(
            preview.compiled_command_preview(),
            "clearra pc --lines 2 --backend auto"
        );
        assert_eq!(preview.solver_execution(), "not_started");
    }
}

mod case_gui_domain_model_tracks_screens_backend_and_output_forms {
    use super::*;

    #[test]
    fn gui_domain_model_tracks_screens_backend_and_output_forms() {
        let screens = GuiScreen::ALL
            .iter()
            .map(|screen| screen.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            screens,
            [
                "home",
                "pc-search",
                "scenario-pc",
                "setup-search",
                "build-coverage",
                "rules",
                "scoring",
                "render",
                "settings",
                "diagnostics"
            ]
        );

        let backend_form = GuiBackendForm::default()
            .with_backend(GuiBackendChoice::Hybrid)
            .with_workers(6)
            .with_memory_budget_mb(512)
            .with_candidate_budget(8192)
            .with_pattern_budget(2048);

        assert_eq!(backend_form.backend(), GuiBackendChoice::Hybrid);
        assert!(backend_form.allow_fallback());
        assert_eq!(backend_form.workers(), 6);
        assert!(backend_form.deterministic());
        assert_eq!(backend_form.memory_budget_mb(), 512);
        assert_eq!(backend_form.candidate_budget(), 8192);
        assert_eq!(backend_form.pattern_budget(), 2048);

        let state = GuiAppState::default().with_backend_form(backend_form);
        assert_eq!(state.output_form().format(), GuiOutputFormat::Text);
        assert_eq!(state.render_form().unsupported_reason(), None);
    }
}

mod case_gui_app_state_preserves_execution_job_and_diagnostics {
    use super::*;

    #[test]
    fn gui_app_state_preserves_execution_job_and_diagnostics() {
        let state = GuiAppState::default()
            .with_problem_form(GuiProblemForm::opening_pc(2, "srs-plus"))
            .with_execution_state(GuiExecutionState::running(GuiJobId::new(42)))
            .with_recent_result("unsupported")
            .with_diagnostic("E_NATIVE_CORE_UNAVAILABLE");

        assert_eq!(state.execution_state().phase(), GuiExecutionPhase::Running);
        assert_eq!(
            state.execution_state().active_job_id().map(GuiJobId::get),
            Some(42)
        );
        assert_eq!(state.recent_result(), Some("unsupported"));
        assert_eq!(state.diagnostics(), ["E_NATIVE_CORE_UNAVAILABLE"]);
        assert_eq!(state.user_preferences().language_id().as_str(), "en");
    }
}

mod case_gui_host_default_language_is_english_when_no_locale_signal {
    use super::*;

    #[test]
    fn gui_host_default_language_is_english_when_no_locale_signal() {
        assert_eq!(
            GuiHostLanguageResolver::resolve_with_sources(Some("auto"), None, None, None).as_str(),
            "en"
        );
    }
}

mod case_gui_host_default_language_is_korean_when_os_locale_ko {
    use super::*;

    #[test]
    fn gui_host_default_language_is_korean_when_os_locale_ko() {
        assert_eq!(
            GuiHostLanguageResolver::resolve_with_sources(Some("auto"), None, None, Some("ko-KR"))
                .as_str(),
            "ko"
        );
    }
}

mod case_gui_host_stored_preference_wins_over_os_locale {
    use super::*;

    #[test]
    fn gui_host_stored_preference_wins_over_os_locale() {
        assert_eq!(
            GuiHostLanguageResolver::resolve_with_sources(
                Some("auto"),
                Some("en"),
                None,
                Some("ko-KR")
            )
            .as_str(),
            "en"
        );
    }
}

mod case_gui_host_explicit_language_wins_over_stored_preference {
    use super::*;

    #[test]
    fn gui_host_explicit_language_wins_over_stored_preference() {
        assert_eq!(
            GuiHostLanguageResolver::resolve_with_sources(
                Some("ko"),
                Some("en"),
                None,
                Some("en-US")
            )
            .as_str(),
            "ko"
        );
    }
}

mod case_gui_host_env_language_wins_over_os_locale {
    use super::*;

    #[test]
    fn gui_host_env_language_wins_over_os_locale() {
        assert_eq!(
            GuiHostLanguageResolver::resolve_with_sources(
                Some("auto"),
                None,
                Some("ko"),
                Some("en-US")
            )
            .as_str(),
            "ko"
        );
    }
}

mod case_tauri_command_calls_clearra_gui_host_only {
    use super::*;

    #[test]
    pub(crate) fn tauri_command_calls_clearra_gui_host_only() {
        let bridge = DesktopTauriCommandBridge::default();
        let response = bridge
            .run_request(
                r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 2,
                "queue": "IIOOO",
                "hold_enabled": false,
                "backend": "auto"
            }"#,
            )
            .expect("desktop run request");
        let value: Value = serde_json::from_str(&response).expect("desktop response JSON");
        let render_capability = &value["capability_report"]["render_capability"];
        assert_eq!(render_capability["png_supported"], true);
        assert_eq!(render_capability["gif_supported"], true);
        assert_eq!(render_capability["render_exact"], true);
        assert!(render_capability["unsupported_reason"].is_null());
        #[cfg(not(feature = "native-c-core"))]
        {
            assert_eq!(value["command"], "pc");
            assert_eq!(value["status"], "unsupported");
            assert!(value["result"].is_null());
            assert_eq!(value["backend_report"]["backend_selected"], "none");
            assert_eq!(value["backend_report"]["fallback_used"], false);
            assert_eq!(value["resource_report"]["solver_executed"], false);
            assert_eq!(value["resource_report"]["probability_complete"], false);
            assert!(value["diagnostics"]
                .as_array()
                .expect("diagnostics array")
                .iter()
                .any(|diagnostic| diagnostic["code"] == "E_PRODUCT_RUNTIME_UNSUPPORTED"));
        }
        #[cfg(feature = "native-c-core")]
        {
            assert_eq!(value["command"], "pc");
            assert_eq!(value["resource_report"]["solver_executed"], true);
            assert!(!value["diagnostics"]
                .as_array()
                .expect("diagnostics array")
                .iter()
                .any(|diagnostic| diagnostic["code"] == "E_NATIVE_CORE_UNAVAILABLE"));
        }
    }
}
pub(crate) use case_tauri_command_calls_clearra_gui_host_only::tauri_command_calls_clearra_gui_host_only;

mod case_desktop_form_builds_app_request {
    use super::*;

    #[test]
    fn desktop_form_builds_app_request() {
        let bridge = DesktopTauriCommandBridge::default();
        let report = bridge
            .validate_request(
                r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 2,
                "queue": "IIOOO",
                "hold_enabled": false,
                "backend": "cpu"
            }"#,
            )
            .expect("desktop validation report");
        let value: Value = serde_json::from_str(&report).expect("validation JSON");

        assert_eq!(value["command"], "validate_request");
        assert_eq!(value["app_request_model"], "clearra-app/AppRequest");
        assert_eq!(value["valid"], true);
        assert!(value["diagnostics"].is_array());
    }
}

mod case_desktop_job_queue_reports_progress_cancel_result {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn all_events_delivered_in_order_completed_job_releases_active_slot_and_second_job_can_start() {
        let mut bridge = DesktopTauriCommandBridge::default();
        let first_job = bridge
            .start_job(request_json())
            .expect("start first desktop job");
        let first_events = drain_until_terminal(&mut bridge, first_job);
        assert_event_order(&first_events, first_job);

        let second_job = bridge
            .start_job(request_json())
            .expect("completed job releases the active slot");
        assert_ne!(first_job, second_job);
        let second_events = drain_until_terminal(&mut bridge, second_job);
        assert_event_order(&second_events, second_job);
    }

    #[cfg(feature = "native-c-core")]
    #[test]
    fn cancel_button_stops_real_search() {
        let mut bridge = DesktopTauriCommandBridge::default();
        let job_id = bridge
            .start_job(
                r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 6,
                "queue": "IOTSZJLIOTSZJLI",
                "hold_enabled": false,
                "backend": "cpu"
            }"#,
            )
            .expect("start desktop job");
        let mut events = drain_until_progress(&mut bridge, job_id);
        bridge
            .cancel_job(job_id)
            .expect("request real search cancellation");
        events.extend(drain_until_terminal(&mut bridge, job_id));
        assert_eq!(
            events.last().and_then(|event| event["event"].as_str()),
            Some("cancelled")
        );
        assert_eq!(
            events
                .last()
                .and_then(|event| event["scope_released"].as_bool()),
            Some(true)
        );
    }

    fn request_json() -> &'static str {
        r#"{
            "app_request_model": "clearra-app/AppRequest",
            "command": "pc",
            "lines": 2,
            "queue": "IIOOO",
            "hold_enabled": false,
            "backend": "auto"
        }"#
    }

    fn drain_until_terminal(bridge: &mut DesktopTauriCommandBridge, job_id: u64) -> Vec<Value> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut events = Vec::new();
        loop {
            let batch = bridge
                .get_job_events(job_id)
                .expect("desktop job event batch");
            let mut batch: Vec<Value> =
                serde_json::from_str(&batch).expect("desktop job event JSON array");
            let terminal = batch.iter().any(|event| {
                matches!(
                    event["event"].as_str(),
                    Some("completed" | "failed" | "cancelled")
                )
            });
            events.append(&mut batch);
            if terminal {
                return events;
            }
            assert!(Instant::now() < deadline, "desktop job did not terminate");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[cfg(feature = "native-c-core")]
    fn drain_until_progress(bridge: &mut DesktopTauriCommandBridge, job_id: u64) -> Vec<Value> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut events = Vec::new();
        loop {
            let batch = bridge
                .get_job_events(job_id)
                .expect("desktop pre-cancel event batch");
            let mut batch: Vec<Value> =
                serde_json::from_str(&batch).expect("desktop pre-cancel event JSON array");
            assert!(
                !batch.iter().any(|event| matches!(
                    event["event"].as_str(),
                    Some("completed" | "failed" | "cancelled")
                )),
                "native search ended before the cancellation signal was exercised"
            );
            let progress = batch.iter().any(|event| event["event"] == "progress");
            events.append(&mut batch);
            if progress {
                return events;
            }
            assert!(
                Instant::now() < deadline,
                "desktop job did not begin execution"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn assert_event_order(events: &[Value], job_id: u64) {
        assert_eq!(
            events.first().and_then(|event| event["event"].as_str()),
            Some("started")
        );
        assert!(events.iter().any(|event| event["event"] == "progress"));
        assert!(matches!(
            events.last().and_then(|event| event["event"].as_str()),
            Some("completed" | "failed")
        ));
        assert!(events.iter().all(|event| event["job_id"] == job_id));
    }
}

mod case_desktop_tauri_command_calls_gui_host_only {
    use super::*;

    #[test]
    fn desktop_tauri_command_calls_gui_host_only() {
        tauri_command_calls_clearra_gui_host_only();
    }
}

mod case_desktop_does_not_own_core_c_or_render_atlas {
    use super::*;

    #[test]
    fn desktop_does_not_own_core_c_or_render_atlas() {
        let bridge = DesktopTauriCommandBridge::default();
        let response = bridge
            .run_request(
                r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 2,
                "queue": "IIOOO",
                "hold_enabled": false,
                "backend": "auto"
            }"#,
            )
            .expect("desktop run request");
        let value: Value = serde_json::from_str(&response).expect("desktop response JSON");

        assert_eq!(value["command"], "pc");
        assert!(value.get("raw_pointer").is_none());
        assert!(value.get("render_atlas").is_none());
    }
}
