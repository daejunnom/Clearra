use clearra_core_domain::pc::pc_target::PcTarget;
use clearra_pc_graph::request::{OpeningPcSearchQuery, PcExecutionPolicy};
use clearra_rules::profile::{builtin_rules::srs_plus, rule_profile::RuleProfileId};

use crate::{
    app_command::AppCommand,
    app_request::AppRequest,
    commands::PcAppCommand,
    gui_bridge::{
        gui_bridge_error::GuiBridgeError, gui_command_preview::GuiCommandPreview,
        gui_form_state::GuiFormState, gui_form_validation::GuiFormValidation,
    },
};

#[derive(Clone, Debug, PartialEq)]
pub struct GuiAppRequestPreview {
    request_model: &'static str,
    app_request_kind: &'static str,
    selected_language: String,
    selected_backend: String,
    selected_problem_preset: String,
    selected_lines: u8,
    selected_rule: String,
    compiled_command_preview: String,
    solver_execution: &'static str,
    app_request: AppRequest,
}

impl GuiAppRequestPreview {
    pub fn from_form_state(form: &GuiFormState) -> Result<Self, GuiBridgeError> {
        let validated = GuiFormValidation::validate(form)?;
        let command_preview = GuiCommandPreview::pc_opening(&validated);

        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_rule(srs_plus())
            .with_execution_policy(
                PcExecutionPolicy::mvp_default().with_backend(validated.backend()),
            );
        let app_request = AppRequest::new(AppCommand::Pc(PcAppCommand::new(query)))
            .with_language(validated.language());

        Ok(Self {
            request_model: "clearra-app/AppRequest",
            app_request_kind: "Pc",
            selected_language: validated.language().as_str().to_owned(),
            selected_backend: validated.backend().as_str().to_owned(),
            selected_problem_preset: validated.selected_problem_preset().to_owned(),
            selected_lines: validated.selected_lines(),
            selected_rule: RuleProfileId::SrsPlus.as_str().to_owned(),
            compiled_command_preview: command_preview.command().to_owned(),
            solver_execution: "not_started",
            app_request,
        })
    }
}
impl GuiAppRequestPreview {
    pub fn request_model(&self) -> &str {
        self.request_model
    }
}
impl GuiAppRequestPreview {
    pub fn app_request_kind(&self) -> &str {
        self.app_request_kind
    }
}
impl GuiAppRequestPreview {
    pub fn selected_language(&self) -> &str {
        &self.selected_language
    }
}
impl GuiAppRequestPreview {
    pub fn selected_backend(&self) -> &str {
        &self.selected_backend
    }
}
impl GuiAppRequestPreview {
    pub fn selected_problem_preset(&self) -> &str {
        &self.selected_problem_preset
    }
}
impl GuiAppRequestPreview {
    pub fn selected_lines(&self) -> u8 {
        self.selected_lines
    }
}
impl GuiAppRequestPreview {
    pub fn selected_rule(&self) -> &str {
        &self.selected_rule
    }
}
impl GuiAppRequestPreview {
    pub fn compiled_command_preview(&self) -> &str {
        &self.compiled_command_preview
    }
}
impl GuiAppRequestPreview {
    pub fn solver_execution(&self) -> &str {
        self.solver_execution
    }
}
impl GuiAppRequestPreview {
    pub fn app_request(&self) -> &AppRequest {
        &self.app_request
    }
}
