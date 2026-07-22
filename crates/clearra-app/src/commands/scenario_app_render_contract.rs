use clearra_core_executor::CoreExecutionResult;
use clearra_validation::diagnostic::diagnostic_report::DiagnosticReport;

use crate::{
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::scenario_app_expected::ScenarioAppExpected,
    commands::scenario_app_field_policy::scenario_field,
    commands::scenario_app_validation_fields::validation_fields,
    render::{AppMessage, AppRenderModel, AppResultKind},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScenarioAppRenderContract {
    verify_expected: bool,
    fixture_path: Option<String>,
    input_fields: Vec<(String, String)>,
    expected: Option<ScenarioAppExpected>,
}

impl ScenarioAppRenderContract {
    pub fn new(verify_expected: bool, input_fields: Vec<(String, String)>) -> Self {
        Self {
            verify_expected,
            fixture_path: None,
            input_fields,
            expected: None,
        }
    }
}
impl ScenarioAppRenderContract {
    pub fn with_fixture_path(mut self, fixture_path: Option<String>) -> Self {
        self.fixture_path = fixture_path;
        self
    }
}
impl ScenarioAppRenderContract {
    pub fn with_expected(mut self, expected: Option<ScenarioAppExpected>) -> Self {
        self.expected = expected;
        self
    }
}
impl ScenarioAppRenderContract {
    pub fn success_response(self, result: CoreExecutionResult) -> AppResponse {
        let expected_fields = match self.verify_search_expected(&result) {
            Ok(fields) => fields,
            Err(error) => {
                return AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(AppErrorCode::PcScenarioExpectedMismatch, error),
                )
            }
        };
        let result_fields = result.summary_fields();
        let mut fields = self.input_fields;
        fields.extend(expected_fields);
        fields.extend(result_fields);
        AppResponse::success(AppRenderModel::ScenarioMessage(AppMessage::new(
            AppResultKind::Scenario,
            fields
                .into_iter()
                .map(|(key, value)| scenario_field(key, value))
                .collect(),
        )))
    }
}
impl ScenarioAppRenderContract {
    pub fn validation_failed_response(&self, report: DiagnosticReport) -> Option<AppResponse> {
        let expected = self.expected.as_ref()?;
        if !self.verify_expected || !expected.expects_unsupported() {
            return None;
        }
        let Ok(mut expected_fields) = expected.verify_validation(&report) else {
            return None;
        };
        let mut fields = self.input_fields.clone();
        fields.append(&mut expected_fields);
        fields.extend(validation_fields(&report));
        Some(AppResponse::success(AppRenderModel::ScenarioMessage(
            AppMessage::new(
                AppResultKind::Scenario,
                fields
                    .into_iter()
                    .map(|(key, value)| scenario_field(key, value))
                    .collect(),
            ),
        )))
    }
}
impl ScenarioAppRenderContract {
    fn verify_search_expected(
        &self,
        result: &CoreExecutionResult,
    ) -> Result<Vec<(String, String)>, String> {
        if !self.verify_expected {
            return Ok(vec![("expected_checked".to_owned(), "false".to_owned())]);
        }
        let Some(expected) = self.expected.as_ref() else {
            return Ok(vec![
                ("expected_checked".to_owned(), "false".to_owned()),
                (
                    "expected_skip_reason".to_owned(),
                    "inline_scenario_has_no_expected_contract".to_owned(),
                ),
            ]);
        };
        expected.verify_search(result).map_err(|error| {
            format!(
                "scenario fixture '{}' expected result mismatch: {error}",
                self.fixture_path.as_deref().unwrap_or("<inline>")
            )
        })
    }
}
