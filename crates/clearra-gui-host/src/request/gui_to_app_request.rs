use clearra_app::{AppCommand, AppContext, AppRequest};
use clearra_i18n::LanguageId;
use clearra_output::RenderFormat;

use crate::{
    model::{GuiAppState, GuiProblemForm},
    request::{
        CoverRequestBuilder, OutputRequestBuilder, PcRequestBuilder, RequestBuildError,
        RequestBuildErrorCode, ScenarioRequestBuilder, SetupRequestBuilder,
    },
    validation::GuiFormValidator,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GuiToAppRequest;

impl GuiToAppRequest {
    pub fn build(state: &GuiAppState) -> Result<GuiAppRequestBuild, RequestBuildError> {
        let validation = GuiFormValidator::validate_state(state);
        if validation.has_errors() {
            let codes = validation
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::ValidationFailed,
                format!("GUI validation failed before clearra-app execution: {codes}"),
            ));
        }

        let command = Self::build_command(state)?;
        let output = OutputRequestBuilder::build(state.output_form());
        let render_format = output.render_format();
        let language = LanguageId::parse(state.current_language()).unwrap_or(LanguageId::En);
        let app_request = AppRequest::new(command)
            .with_language(language)
            .with_output_policy(output.into_output_policy());
        let app_validation = AppContext::default().validate_request(&app_request);
        if app_validation.has_errors() {
            let codes = app_validation
                .validation()
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::ValidationFailed,
                format!("clearra-app AppRequest validation failed before GUI execution: {codes}"),
            ));
        }

        Ok(GuiAppRequestBuild {
            app_request,
            render_format,
        })
    }
}
impl GuiToAppRequest {
    fn build_command(state: &GuiAppState) -> Result<AppCommand, RequestBuildError> {
        match state.problem_form() {
            GuiProblemForm::OpeningPc(form) => {
                PcRequestBuilder::build_command(form, state.backend_form())
            }
            GuiProblemForm::ScenarioPc(form) => {
                ScenarioRequestBuilder::build_command(form, state.backend_form())
            }
            GuiProblemForm::SetupSearch(form) => {
                SetupRequestBuilder::build_command(form, state.backend_form())
            }
            GuiProblemForm::BuildCoverage(form) => {
                CoverRequestBuilder::build_command(form, state.backend_form())
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GuiAppRequestBuild {
    app_request: AppRequest,
    render_format: RenderFormat,
}

impl GuiAppRequestBuild {
    pub fn app_request(&self) -> &AppRequest {
        &self.app_request
    }
}
impl GuiAppRequestBuild {
    pub const fn render_format(&self) -> RenderFormat {
        self.render_format
    }
}
impl GuiAppRequestBuild {
    pub fn into_app_request(self) -> AppRequest {
        self.app_request
    }
}
