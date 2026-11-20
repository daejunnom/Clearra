use std::collections::BTreeMap;

use clearra_core_executor::CoreExecutionResult;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{PcContinuationTokenCodec, PcCountPolicy};
use clearra_problem::SearchProblem;
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
    compact_input_replay_token: Option<String>,
}

// Compact scenario compatibility need not retain an unbounded suffix. This
// covers the legacy compact queue-view width without depending on its C ABI.
const MAX_COMPACT_SCENARIO_REPLAY_QUEUE: usize = 64;

impl ScenarioAppRenderContract {
    pub fn new(verify_expected: bool, input_fields: Vec<(String, String)>) -> Self {
        Self {
            verify_expected,
            fixture_path: None,
            input_fields,
            expected: None,
            compact_input_replay_token: None,
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

    /// Carries only an immutable input replay for the legacy scenario product;
    /// unlike pc2 it does not assert that an output game state is reachable.
    pub(crate) fn prepare_compact_input_replay(mut self, problem: &SearchProblem) -> Self {
        let query = problem.core_query();
        let board = query.initial_board();
        let policy = query.execution_policy();
        let objective_from_count = match query.count_policy() {
            PcCountPolicy::CountAll => ObjectivePolicy::all(),
            PcCountPolicy::CountUnique => ObjectivePolicy::unique(),
            PcCountPolicy::FirstSolution => return self,
        };
        if board.width() != 10
            || !(2..=6).contains(&board.visible_height())
            || board.has_extended_occupancy()
            || problem.compact_pc_has_unqualified_capped_family()
            || (query.exact_pieces().is_none()
                && problem.compact_pc_geometry_height() >= board.visible_height()
                && problem.compact_pc_proven_empty_family_depth().is_none())
            || query.objective() != objective_from_count
            || query.supply_window_size().is_some()
            || query.allowed_colored_solution_identities().is_some()
            || query
                .queue_observation_policy()
                .requires_observation_policy()
            || query.verified_kick_profile().is_some()
            || policy.tablebase_requested()
            || policy.precompute_build_dependencies()
        {
            return self;
        }
        let Some(fixed) = query.remaining_queue().as_fixed_sequence() else {
            return self;
        };
        if fixed.is_empty() || fixed.len() > MAX_COMPACT_SCENARIO_REPLAY_QUEUE {
            return self;
        }
        self.compact_input_replay_token =
            PcContinuationTokenCodec::encode_scenario_replay(query).ok();
        if self.compact_input_replay_token.is_some() {
            // Format the original public extent and normalized field, not
            // the possibly smaller catalog used to search the legacy input.
            for (key, value) in [
                ("board_width", board.width().to_string()),
                ("visible_height", board.visible_height().to_string()),
                (
                    "initial_board_mask",
                    format!("0x{:016x}", board.occupied_mask()),
                ),
            ] {
                if !self
                    .input_fields
                    .iter()
                    .any(|(existing, _)| existing == key)
                {
                    self.input_fields.push((key.to_owned(), value));
                }
            }
        }
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
        let replay_fields = self
            .compact_input_replay_token
            .filter(|_| {
                result.field("scenario_replay_token").is_none()
                    && result.bool_field("count_complete") == Some(true)
                    && result.bool_field("resource_truncated") == Some(false)
            })
            .map(|token| {
                [
                    ("scenario_replay_token_version".to_owned(), "sr2".to_owned()),
                    (
                        "replay_hint".to_owned(),
                        format!("clearra continue {token}"),
                    ),
                    ("scenario_replay_token".to_owned(), token),
                ]
            });
        let contract_fields = match canonical_contract_fields(
            self.input_fields
                .into_iter()
                .chain(expected_fields)
                .chain(replay_fields.into_iter().flatten()),
        ) {
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
