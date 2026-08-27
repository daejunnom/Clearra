use clearra_app::{GuiAppRequestPreview, GuiBridgeError, RequestStructuralProfiles};

use crate::{
    host_language_resolver::GuiHostLanguageResolver,
    model::{
        GuiBackendForm, GuiExecutionState, GuiOutputForm, GuiProblemForm, GuiRenderForm, GuiScreen,
        GuiUserPreferences,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiAppState {
    current_language: String,
    current_screen: GuiScreen,
    problem_form: GuiProblemForm,
    backend_form: GuiBackendForm,
    output_form: GuiOutputForm,
    render_form: GuiRenderForm,
    recent_result: Option<String>,
    diagnostics: Vec<String>,
    execution_state: GuiExecutionState,
    user_preferences: GuiUserPreferences,
    request_structural_profiles: RequestStructuralProfiles,
}

impl GuiAppState {
    pub fn new(
        current_language: impl Into<String>,
        current_screen: GuiScreen,
        problem_form: GuiProblemForm,
        backend_form: GuiBackendForm,
        output_form: GuiOutputForm,
        render_form: GuiRenderForm,
        user_preferences: GuiUserPreferences,
    ) -> Self {
        Self {
            current_language: current_language.into(),
            current_screen,
            problem_form,
            backend_form,
            output_form,
            render_form,
            recent_result: None,
            diagnostics: Vec::new(),
            execution_state: GuiExecutionState::idle(),
            user_preferences,
            request_structural_profiles: RequestStructuralProfiles::STANDARD,
        }
    }
}
impl GuiAppState {
    pub fn app_request_preview(&self) -> Result<GuiAppRequestPreview, GuiBridgeError> {
        let form = self
            .problem_form
            .to_app_bridge_form(&self.current_language, &self.backend_form);
        GuiAppRequestPreview::from_form_state(&form)
    }
}
impl GuiAppState {
    pub fn with_current_language(mut self, language: impl Into<String>) -> Self {
        self.current_language = language.into();
        self
    }
}
impl GuiAppState {
    pub fn with_problem_form(mut self, problem_form: GuiProblemForm) -> Self {
        self.problem_form = problem_form;
        self
    }
}
impl GuiAppState {
    pub fn with_backend_form(mut self, backend_form: GuiBackendForm) -> Self {
        self.backend_form = backend_form;
        self
    }
}
impl GuiAppState {
    pub fn with_output_form(mut self, output_form: GuiOutputForm) -> Self {
        self.output_form = output_form;
        self
    }
}
impl GuiAppState {
    pub fn with_render_form(mut self, render_form: GuiRenderForm) -> Self {
        self.render_form = render_form;
        self
    }
}
impl GuiAppState {
    pub fn with_execution_state(mut self, execution_state: GuiExecutionState) -> Self {
        self.execution_state = execution_state;
        self
    }
}
impl GuiAppState {
    pub fn with_recent_result(mut self, recent_result: impl Into<String>) -> Self {
        self.recent_result = Some(recent_result.into());
        self
    }
}
impl GuiAppState {
    pub fn with_diagnostic(mut self, diagnostic: impl Into<String>) -> Self {
        self.diagnostics.push(diagnostic.into());
        self
    }

    pub fn with_request_structural_profiles(mut self, profiles: RequestStructuralProfiles) -> Self {
        self.request_structural_profiles = profiles;
        self
    }
}
impl GuiAppState {
    pub fn current_language(&self) -> &str {
        &self.current_language
    }
}
impl GuiAppState {
    pub const fn current_screen(&self) -> GuiScreen {
        self.current_screen
    }
}
impl GuiAppState {
    pub fn problem_form(&self) -> &GuiProblemForm {
        &self.problem_form
    }
}
impl GuiAppState {
    pub fn backend_form(&self) -> &GuiBackendForm {
        &self.backend_form
    }
}
impl GuiAppState {
    pub fn output_form(&self) -> &GuiOutputForm {
        &self.output_form
    }
}
impl GuiAppState {
    pub fn render_form(&self) -> &GuiRenderForm {
        &self.render_form
    }
}
impl GuiAppState {
    pub fn recent_result(&self) -> Option<&str> {
        self.recent_result.as_deref()
    }
}
impl GuiAppState {
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }
}
impl GuiAppState {
    pub fn execution_state(&self) -> &GuiExecutionState {
        &self.execution_state
    }
}
impl GuiAppState {
    pub fn user_preferences(&self) -> &GuiUserPreferences {
        &self.user_preferences
    }

    pub const fn request_structural_profiles(&self) -> RequestStructuralProfiles {
        self.request_structural_profiles
    }
}

impl Default for GuiAppState {
    fn default() -> Self {
        let default_language = GuiHostLanguageResolver::default_language();
        Self::new(
            default_language.as_str(),
            GuiScreen::PcSearch,
            GuiProblemForm::default(),
            GuiBackendForm::default(),
            GuiOutputForm::default(),
            GuiRenderForm::default(),
            GuiUserPreferences::default(),
        )
    }
}
