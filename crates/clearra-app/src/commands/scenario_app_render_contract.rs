use std::collections::BTreeMap;

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
        let contract_fields =
            match canonical_contract_fields(self.input_fields.into_iter().chain(expected_fields)) {
                Ok(fields) => fields,
                Err(message) => {
                    return AppResponse::failed(
                        AppStatus::ExecutionFailed,
                        AppError::new(AppErrorCode::ExecutionFailed, message),
                    )
                }
            };

        // Keep successful scenario results typed until the host renderer. This preserves
        // solution-set authority and lets every host apply the shared public field contract.
        // Contract-owned input/expected fields replace same-named engine fields atomically.
        AppResponse::success(AppRenderModel::Scenario(
            result.with_replaced_fields(contract_fields),
        ))
    }
}

fn canonical_contract_fields(
    fields: impl IntoIterator<Item = (String, String)>,
) -> Result<Vec<(String, String)>, String> {
    let mut values = BTreeMap::<String, String>::new();
    let mut canonical = Vec::new();
    for (key, value) in fields {
        if let Some(previous) = values.get(&key) {
            if previous != &value {
                return Err(format!(
                    "scenario render contract contains conflicting values for '{key}'"
                ));
            }
            continue;
        }
        values.insert(key.clone(), value.clone());
        canonical.push((key, value));
    }
    Ok(canonical)
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

#[cfg(test)]
mod tests {
    use clearra_core_executor::CoreExecutionResult;

    use super::*;

    #[test]
    fn successful_contract_preserves_typed_result_authority_and_replaces_duplicate_keys_once() {
        let result = CoreExecutionResult::new(
            vec![
                ("status".to_owned(), "scenario-searched".to_owned()),
                ("input_mode".to_owned(), "engine-default".to_owned()),
            ],
            Vec::new(),
        )
        .with_normalized_solution_keys(vec!["ctk1:test-authority".to_owned()]);
        let response = ScenarioAppRenderContract::new(
            false,
            vec![("input_mode".to_owned(), "inline".to_owned())],
        )
        .success_response(result);

        let Some(AppRenderModel::Scenario(result)) = response.render_model() else {
            panic!("successful scenario contract must preserve the typed result");
        };
        assert_eq!(result.field("input_mode"), Some("inline"));
        assert_eq!(result.field("expected_checked"), Some("false"));
        assert_eq!(
            result.normalized_solution_keys(),
            &["ctk1:test-authority".to_owned()]
        );
        assert_eq!(
            result
                .summary_fields()
                .iter()
                .filter(|(key, _)| key == "input_mode")
                .count(),
            1
        );
    }

    #[test]
    fn conflicting_contract_fields_fail_closed_before_rendering() {
        let response = ScenarioAppRenderContract::new(
            false,
            vec![
                ("input_mode".to_owned(), "inline".to_owned()),
                ("input_mode".to_owned(), "fixture".to_owned()),
            ],
        )
        .success_response(CoreExecutionResult::new(Vec::new(), Vec::new()));

        assert_eq!(response.status(), AppStatus::ExecutionFailed);
        assert_eq!(
            response.error().map(AppError::code),
            Some(AppErrorCode::ExecutionFailed)
        );
        assert!(response.render_model().is_none());
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
