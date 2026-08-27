use clearra_app::{
    AppCommand, AppContext, AppRequest, PcResultProjection, ProductCapabilityContract,
};
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
        let product_capability_contract = match &command {
            AppCommand::Pc(command) => product_contract(command.result_projection()),
            AppCommand::Scenario(command) => product_contract(command.result_projection()),
            _ => None,
        };
        let output = OutputRequestBuilder::build(state.output_form());
        let render_format = output.render_format();
        let language = LanguageId::parse(state.current_language()).unwrap_or(LanguageId::En);
        let app_request = AppRequest::new(command)
            .with_language(language)
            .with_output_policy(output.into_output_policy());
        let app_request = app_request
            .with_request_structural_profiles(state.request_structural_profiles())
            .map_err(|error| {
                RequestBuildError::new(
                    RequestBuildErrorCode::ValidationFailed,
                    format!("clearra-app rejected the GUI request profiles: {error}"),
                )
            })?;
        let app_request = match product_capability_contract {
            Some(contract) => app_request
                .with_product_capability_contract(contract)
                .map_err(|error| {
                    RequestBuildError::new(
                        RequestBuildErrorCode::ValidationFailed,
                        format!(
                            "clearra-app rejected the GUI product capability contract: {error}"
                        ),
                    )
                })?,
            None => app_request,
        };
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

fn product_contract(projection: PcResultProjection) -> Option<ProductCapabilityContract> {
    match projection {
        PcResultProjection::TilingFamilyV1(_) => Some(ProductCapabilityContract::PcTiling),
        PcResultProjection::PathFamilyV2(_) => Some(ProductCapabilityContract::PcPath),
        PcResultProjection::MinimumCoverV2(_) => Some(ProductCapabilityContract::PcMinimals),
        PcResultProjection::ScoreSummaryV2(origin) if origin.is_score_finder() => {
            Some(ProductCapabilityContract::PcScoreFinder)
        }
        PcResultProjection::ScoreSummaryV2(_) => Some(ProductCapabilityContract::PcScore),
        PcResultProjection::ScorePortfolioV2(_) => Some(ProductCapabilityContract::PcScoreMinimals),
        PcResultProjection::SaveGroupsV2(_) => Some(ProductCapabilityContract::PcSaves),
        PcResultProjection::BestSaveV2(_) => Some(ProductCapabilityContract::PcBestSave),
        PcResultProjection::Standard
        | PcResultProjection::ChanceProbabilityV2(_)
        | PcResultProjection::AllSpinSolution(_)
        | PcResultProjection::AllSpinPreservationChance(_) => None,
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
