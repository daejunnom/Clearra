use crate::{
    app_command::AppCommand,
    gui_bridge::{
        GuiAppRequestPreview, GuiBackendCapabilityView, GuiCommandPreview, GuiDisabledReason,
        GuiFormState, GuiFormValidation, GuiStatePersistenceContract,
    },
};

#[test]
fn gui_form_state_builds_pc_app_request_preview() {
    let form = GuiFormState::default();
    let preview = GuiAppRequestPreview::from_form_state(&form).expect("preview");

    assert_eq!(preview.request_model(), "clearra-app/AppRequest");
    assert_eq!(preview.app_request_kind(), "Pc");
    assert_eq!(preview.selected_language(), "en");
    assert_eq!(preview.selected_backend(), "auto");
    assert_eq!(preview.selected_problem_preset(), "opening-pc");
    assert_eq!(preview.selected_lines(), 2);
    assert_eq!(preview.selected_rule(), "srs-plus");
    assert_eq!(
        preview.compiled_command_preview(),
        "clearra pc --lines 2 --backend auto"
    );
    assert_eq!(preview.solver_execution(), "not_started");

    match preview.app_request().command() {
        AppCommand::Pc(command) => {
            assert_eq!(command.query().target().lines(), 2);
            assert_eq!(command.query().rule().id().as_str(), "srs-plus");
            assert_eq!(
                command
                    .query()
                    .execution_policy()
                    .requested_backend()
                    .as_str(),
                "auto"
            );
        }
        other => panic!("expected Pc AppCommand, got {other:?}"),
    }
}

#[test]
fn gui_backend_options_are_runtime_selected_instead_of_statically_disabled() {
    let reasons = GuiDisabledReason::backend_reasons();
    assert!(reasons.is_empty());
}

#[test]
fn gui_form_state_rejects_invalid_line_count() {
    let form = GuiFormState::new("ko", "auto", "opening-pc", 0, "srs-plus");
    let error = GuiFormValidation::validate(&form).expect_err("invalid line count");

    assert_eq!(
        error.code(),
        crate::gui_bridge::GuiBridgeErrorCode::InvalidLineCount
    );
    assert!(error.message().contains("positive line count"));
}

#[test]
fn gui_command_preview_is_stable_and_does_not_execute() {
    let validated = GuiFormValidation::validate(&GuiFormState::default()).expect("validated");
    let preview = GuiCommandPreview::pc_opening(&validated);

    assert_eq!(preview.command(), "clearra pc --lines 2 --backend auto");
    assert_eq!(preview.execution_policy(), "display_only_no_subprocess");
}

#[test]
fn gui_language_preference_contract_uses_stable_keys() {
    assert_eq!(
        GuiStatePersistenceContract::preference_path(),
        "app-config/clearra-gui/preferences.json"
    );
    assert_eq!(
        GuiStatePersistenceContract::stable_keys(),
        &[
            "schema_version",
            "language",
            "backend",
            "recent_problem_preset",
            "workers",
            "allow_backend_fallback",
            "deterministic",
            "default_output_format",
            "last_opened_fixture_dir",
            "theme"
        ]
    );
}

#[test]
fn gui_backend_capability_view_exposes_runtime_selected_gpu() {
    let options = GuiBackendCapabilityView::backend_options();
    let gpu = options
        .iter()
        .find(|option| option.backend_id() == "gpu")
        .expect("gpu backend option");

    assert!(gpu.enabled());
    assert_eq!(gpu.label_key(), "ui.backend.gpu.label");
    assert_eq!(gpu.disabled_reason_code(), None);
    assert_eq!(gpu.diagnostic_localization_key(), None);
    assert_eq!(
        gpu.schema_source(),
        "clearra-ui-schema/setup_explorer/BackendOptionsSchema"
    );
}

#[test]
fn gui_backend_capability_view_exposes_gpu_ready_or_cpu_hybrid_policy() {
    let options = GuiBackendCapabilityView::backend_options();
    let hybrid = options
        .iter()
        .find(|option| option.backend_id() == "hybrid")
        .expect("hybrid backend option");

    assert!(hybrid.enabled());
    assert_eq!(hybrid.disabled_reason_code(), None);
    assert_eq!(hybrid.diagnostic_localization_key(), None);
}
