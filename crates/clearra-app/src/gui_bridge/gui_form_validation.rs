use clearra_i18n::LanguageId;
use clearra_pc_graph::request::RequestedSearchBackend;

use crate::gui_bridge::{
    gui_bridge_error::{GuiBridgeError, GuiBridgeErrorCode},
    gui_form_state::GuiFormState,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiValidatedForm {
    language: LanguageId,
    backend: RequestedSearchBackend,
    selected_problem_preset: String,
    selected_lines: u8,
    selected_rule: String,
}

impl GuiValidatedForm {
    pub fn language(&self) -> LanguageId {
        self.language
    }
}
impl GuiValidatedForm {
    pub fn backend(&self) -> RequestedSearchBackend {
        self.backend
    }
}
impl GuiValidatedForm {
    pub fn selected_problem_preset(&self) -> &str {
        &self.selected_problem_preset
    }
}
impl GuiValidatedForm {
    pub fn selected_lines(&self) -> u8 {
        self.selected_lines
    }
}
impl GuiValidatedForm {
    pub fn selected_rule(&self) -> &str {
        &self.selected_rule
    }
}

pub struct GuiFormValidation;

impl GuiFormValidation {
    pub fn validate(form: &GuiFormState) -> Result<GuiValidatedForm, GuiBridgeError> {
        if form.selected_lines() == 0 {
            return Err(GuiBridgeError::new(
                GuiBridgeErrorCode::InvalidLineCount,
                "GUI bridge preview requires a positive line count",
            ));
        }
        if form.selected_problem_preset() != "opening-pc" {
            return Err(GuiBridgeError::new(
                GuiBridgeErrorCode::UnsupportedProblemPreset,
                format!(
                    "GUI bridge preview only supports opening-pc, got {}",
                    form.selected_problem_preset()
                ),
            ));
        }
        if form.selected_rule() != "srs-plus" {
            return Err(GuiBridgeError::new(
                GuiBridgeErrorCode::UnsupportedRule,
                format!(
                    "GUI bridge preview only supports srs-plus, got {}",
                    form.selected_rule()
                ),
            ));
        }
        if form.selected_lines() != 2 {
            return Err(GuiBridgeError::new(
                GuiBridgeErrorCode::UnsupportedLineTarget,
                format!(
                    "GUI bridge preview only supports 2 lines, got {}",
                    form.selected_lines()
                ),
            ));
        }
        let backend = parse_gui_backend_option(form.selected_backend())?;
        let language = LanguageId::parse(form.selected_language()).unwrap_or(LanguageId::En);

        Ok(GuiValidatedForm {
            language,
            backend,
            selected_problem_preset: form.selected_problem_preset().to_owned(),
            selected_lines: form.selected_lines(),
            selected_rule: form.selected_rule().to_owned(),
        })
    }
}

fn parse_gui_backend_option(value: &str) -> Result<RequestedSearchBackend, GuiBridgeError> {
    let Some(backend) = RequestedSearchBackend::parse(value) else {
        return Err(unknown_backend_option(value));
    };
    if matches!(
        backend,
        RequestedSearchBackend::Auto
            | RequestedSearchBackend::Cpu
            | RequestedSearchBackend::Gpu
            | RequestedSearchBackend::Hybrid
    ) {
        Ok(backend)
    } else {
        Err(unknown_backend_option(value))
    }
}

fn unknown_backend_option(value: &str) -> GuiBridgeError {
    GuiBridgeError::new(
        GuiBridgeErrorCode::UnknownBackendOption,
        format!(
            "unknown GUI backend option '{value}'; expected one of auto, cpu, gpu, hybrid from clearra-ui-schema/setup_explorer/BackendOptionsSchema"
        ),
    )
}
