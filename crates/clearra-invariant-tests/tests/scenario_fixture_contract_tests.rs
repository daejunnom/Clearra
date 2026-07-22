#![cfg(feature = "native-c-core")]

use std::{fs, path::PathBuf};

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_executor::CoreExecutor;
use clearra_pc_graph::request::{
    PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};
use clearra_problem::ProblemCompiler;
use clearra_rules::profile::builtin_rules::{no_kick, srs, srs_plus};
use clearra_supply::queue::fixed_sequence::FixedSequence;
use clearra_validation::{
    diagnostic::diagnostic_code::DiagnosticCode,
    validators::pc_query_validator::validate_pc_scenario_query,
};
use serde::Deserialize;
use serde_json::Value;

const REQUIRES_180_UNSUPPORTED_REASON: &str = "scenario_requires_180_unsupported";

fn scenario_fixtures() -> Vec<ScenarioFixture> {
    let fixture_dir = workspace_root().join("tests").join("fixtures").join("pc");
    let mut fixtures = Vec::new();
    for entry in fs::read_dir(&fixture_dir).expect("scenario fixture directory") {
        let path = entry.expect("scenario fixture entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let contents = fs::read_to_string(&path).expect("scenario fixture json");
        let raw_fixture: Value = serde_json::from_str(&contents)
            .unwrap_or_else(|error| panic!("invalid fixture json {path:?}: {error}"));
        if raw_fixture.get("scenario").is_none() {
            continue;
        }
        fixtures.push(
            serde_json::from_str::<ScenarioFixture>(&contents)
                .unwrap_or_else(|error| panic!("invalid scenario fixture {path:?}: {error}")),
        );
    }
    fixtures.sort_by(|a, b| a.name.cmp(&b.name));
    assert!(
        fixtures
            .iter()
            .any(|fixture| !fixture.scenario.requires_180),
        "tests/fixtures/pc must include a supported scenario search fixture"
    );
    assert!(
        fixtures.iter().any(|fixture| fixture.scenario.requires_180
            && fixture.expected.unsupported.unwrap_or(false)),
        "tests/fixtures/pc must include a requires_180 unsupported fixture"
    );
    assert!(fixtures
        .iter()
        .any(|fixture| fixture.expected.unsupported_reason.as_deref()
            == Some(REQUIRES_180_UNSUPPORTED_REASON)));
    fixtures
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn assert_field(fields: &[(String, String)], key: &str, expected: bool) {
    assert_eq!(
        fields
            .iter()
            .find_map(|(field_key, value)| (field_key == key).then_some(value.as_str())),
        Some(if expected { "true" } else { "false" }),
        "field {key} mismatch"
    );
}

fn field_value<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find_map(|(field_key, value)| (field_key == key).then_some(value.as_str()))
}

fn numeric_field(fields: &[(String, String)], key: &str) -> usize {
    fields
        .iter()
        .find_map(|(field_key, value)| {
            (field_key == key).then(|| value.parse::<usize>().expect("numeric field"))
        })
        .unwrap_or_else(|| panic!("missing numeric field {key}"))
}

fn parse_trace_key_list(value: &str) -> Vec<String> {
    if value.is_empty() || value == "none" {
        return Vec::new();
    }
    value.split(',').map(ToOwned::to_owned).collect()
}

#[derive(Debug, Deserialize)]
struct ScenarioFixture {
    name: String,
    source: ScenarioFixtureSource,
    scenario: ScenarioFixtureInput,
    expected: ScenarioFixtureExpected,
}

impl ScenarioFixture {
    fn to_query(&self) -> PcScenarioQuery {
        assert_eq!(self.scenario.board_width, 10, "MVP1 scenario width");
        PcScenarioQuery::new(
            PcScenarioBoard::new(
                self.scenario.board_width,
                self.scenario.visible_height,
                parse_hex_mask(&self.scenario.initial_board_mask),
            ),
            PcQueueInput::fixed_sequence(FixedSequence::new(parse_queue(
                &self.scenario.remaining_queue,
            ))),
            PieceWindow::new(self.scenario.max_pieces),
        )
        .with_hold_piece(self.scenario.hold.map(parse_piece))
        .with_rule(rule_profile(&self.scenario.rule))
        .with_requires_180(self.scenario.requires_180)
        .with_exact_pieces(self.scenario.exact_pieces)
        .with_min_remaining_queue(self.scenario.min_remaining_queue)
        .with_allow_hold(self.scenario.allow_hold)
        .with_count_policy(count_policy(&self.scenario.count_policy))
        .with_retained_trace_limit(self.scenario.retained_trace_limit)
    }
}
impl ScenarioFixture {
    fn unsupported_reason(&self) -> &str {
        self.expected
            .unsupported_reason
            .as_deref()
            .expect("unsupported fixture reason")
    }
}

#[derive(Debug, Deserialize)]
struct ScenarioFixtureSource {
    site: String,
    page: String,
    section: String,
    human_verified: bool,
}

#[derive(Debug, Deserialize)]
struct ScenarioFixtureInput {
    board_width: u16,
    visible_height: u16,
    initial_board_mask: String,
    remaining_queue: String,
    hold: Option<char>,
    rule: String,
    requires_180: bool,
    goal: String,
    max_pieces: usize,
    exact_pieces: Option<usize>,
    min_remaining_queue: usize,
    allow_hold: bool,
    count_policy: String,
    retained_trace_limit: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioFixtureExpected {
    solution_exists: bool,
    expected_total_solution_count: Option<usize>,
    #[serde(default)]
    count_complete: Option<bool>,
    #[serde(default)]
    unsupported: Option<bool>,
    #[serde(default)]
    unsupported_reason: Option<String>,
    #[serde(default)]
    accepted_retained_trace_keys: Vec<String>,
}

fn parse_hex_mask(mask: &str) -> u64 {
    let digits = mask
        .strip_prefix("0x")
        .expect("hex mask must start with 0x");
    u64::from_str_radix(digits, 16).expect("hex mask")
}

fn parse_queue(queue: &str) -> Vec<PieceKind> {
    queue.chars().map(parse_piece).collect()
}

fn parse_piece(piece: char) -> PieceKind {
    PieceKind::from_ascii(piece).expect("standard piece")
}

fn rule_profile(rule: &str) -> clearra_rules::profile::rule_profile::RuleProfile {
    match rule {
        "srs-90" | "srs" => srs(),
        "srs-plus" => srs_plus(),
        "no-kick" => no_kick(),
        other => panic!("unsupported scenario fixture rule {other}"),
    }
}

fn count_policy(policy: &str) -> PcCountPolicy {
    match policy {
        "first-solution" => PcCountPolicy::FirstSolution,
        "count-all" => PcCountPolicy::CountAll,
        "count-unique" => PcCountPolicy::CountUnique,
        other => panic!("unsupported scenario fixture count_policy {other}"),
    }
}

mod case_scenario_fixtures_drive_clear_to_empty_search_and_count_contracts {
    use super::*;

    #[test]
    fn scenario_fixtures_drive_clear_to_empty_search_and_count_contracts() {
        for fixture in scenario_fixtures() {
            assert!(
                fixture.source.human_verified,
                "{} must keep human_verified source metadata",
                fixture.name
            );
            assert!(
                !fixture.source.site.is_empty()
                    && !fixture.source.page.is_empty()
                    && !fixture.source.section.is_empty(),
                "{} must keep non-empty source site/page/section metadata",
                fixture.name
            );
            assert_eq!(
                fixture.scenario.goal, "clear-to-empty",
                "{} must use the clear-to-empty scenario goal",
                fixture.name
            );

            let query = fixture.to_query();
            let report = validate_pc_scenario_query(&query);

            if fixture.expected.unsupported.unwrap_or(false) {
                assert!(
                    report.has_errors(),
                    "{} must be rejected before search",
                    fixture.name
                );
                assert!(report.contains_code(DiagnosticCode::EPcQueryInvalid));
                assert!(
                    report.diagnostics().iter().any(|diagnostic| {
                        diagnostic
                            .evidence()
                            .iter()
                            .any(|evidence| evidence.value() == fixture.unsupported_reason())
                    }),
                    "{} must disclose unsupported reason {}",
                    fixture.name,
                    fixture.unsupported_reason()
                );
                continue;
            }

            assert!(
                !report.has_errors(),
                "{} must validate before scenario search: {:?}",
                fixture.name,
                report.diagnostics()
            );

            let problem = ProblemCompiler::compile_scenario_pc(&query).expect("scenario problem");
            let result = CoreExecutor::execute(&problem).expect("scenario fixture execution");
            let fields = result.summary_fields();
            assert_field(&fields, "solution_found", fixture.expected.solution_exists);
            if let Some(count_complete) = fixture.expected.count_complete {
                assert_field(&fields, "count_complete", count_complete);
            }
            assert_field(&fields, "allow_hold", fixture.scenario.allow_hold);
            assert_eq!(
                field_value(&fields, "count_policy"),
                Some(fixture.scenario.count_policy.as_str()),
                "{} must pass fixture count_policy into PcScenarioQuery",
                fixture.name
            );
            assert_eq!(
                numeric_field(&fields, "min_remaining_queue"),
                fixture.scenario.min_remaining_queue,
                "{} must pass fixture min_remaining_queue into PcScenarioQuery",
                fixture.name
            );
            assert_eq!(
                numeric_field(&fields, "retained_trace_limit"),
                fixture.scenario.retained_trace_limit,
                "{} must pass fixture retained_trace_limit into PcScenarioQuery",
                fixture.name
            );
            if let Some(exact_pieces) = fixture.scenario.exact_pieces {
                let expected_exact_pieces = exact_pieces.to_string();
                assert_eq!(
                    field_value(&fields, "exact_pieces"),
                    Some(expected_exact_pieces.as_str()),
                    "{} must pass fixture exact_pieces into PcScenarioQuery",
                    fixture.name
                );
            }

            let total_solution_count = numeric_field(&fields, "total_solution_count");
            if let Some(expected_total_solution_count) =
                fixture.expected.expected_total_solution_count
            {
                assert_eq!(
                    total_solution_count, expected_total_solution_count,
                    "{} must compare expected_total_solution_count, not retained traces",
                    fixture.name
                );
            }
            assert_eq!(
                numeric_field(&fields, "solution_trace_count"),
                numeric_field(&fields, "retained_trace_count"),
                "{} must keep solution_trace_count as retained trace count",
                fixture.name
            );
            assert!(
                total_solution_count >= numeric_field(&fields, "retained_trace_count"),
                "{} must not infer total count from retained traces",
                fixture.name
            );
            if !fixture.expected.accepted_retained_trace_keys.is_empty() {
                assert!(
                    numeric_field(&fields, "retained_trace_count") > 0,
                    "{} must retain traces when accepted retained keys are listed",
                    fixture.name
                );
                let retained_keys = parse_trace_key_list(
                    field_value(&fields, "retained_trace_keys").unwrap_or_else(|| {
                        panic!("{} must export retained_trace_keys", fixture.name)
                    }),
                );
                assert!(
                !retained_keys.is_empty(),
                "{} must export retained trace canonical keys when accepted retained keys are listed",
                fixture.name
            );
                for retained_key in retained_keys {
                    assert!(
                        fixture
                            .expected
                            .accepted_retained_trace_keys
                            .contains(&retained_key),
                        "{} retained trace key {retained_key} must be accepted by fixture",
                        fixture.name
                    );
                }
            }
        }
    }
}
