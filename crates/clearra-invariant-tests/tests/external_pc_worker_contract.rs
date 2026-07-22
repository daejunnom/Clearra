use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use serde_json::Value;

const PCO_FIXTURE_PATH: &str = "tests/fixtures/external-pc/pco_i_hold_6p_second_bag_pc.json";
const PCO_SOURCE_SET_PATH: &str =
    "tests/fixtures/external-pc/pco_opener_full_63.source_solutions.json";
const TSAR_FIXTURE_PATH: &str = "tests/fixtures/external-pc/tsar_cannon_after_2bag_full_42.json";
const TSAR_SOURCE_SET_PATH: &str =
    "tests/fixtures/external-pc/tsar_cannon_after_2bag_full_42.source_solutions.json";

fn pco_fixture() -> ExternalPcFixture {
    read_json(PCO_FIXTURE_PATH)
}

fn tsar_fixture() -> ExternalPcFixture {
    read_json(TSAR_FIXTURE_PATH)
}

fn read_json<T>(path: &str) -> T
where
    T: for<'de> Deserialize<'de>,
{
    let contents = fs::read_to_string(workspace_root().join(path))
        .unwrap_or_else(|error| panic!("failed to read {path}: {error}"));
    serde_json::from_str(&contents).unwrap_or_else(|error| panic!("invalid json {path}: {error}"))
}

fn read_json_value(path: &str) -> Value {
    read_json(path)
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn assert_no_forbidden_image_golden_keys(path: &str, value: &Value) {
    let mut queue = VecDeque::from([value]);
    while let Some(node) = queue.pop_front() {
        match node {
            Value::Object(object) => {
                for (key, value) in object {
                    assert!(
                        !matches!(
                            key.as_str(),
                            "image_path"
                                | "image_golden"
                                | "raw_image_golden"
                                | "pixel_golden"
                                | "expected_image"
                        ),
                        "{path} must not use image or pixel golden key {key}"
                    );
                    queue.push_back(value);
                }
            }
            Value::Array(items) => {
                queue.extend(items);
            }
            _ => {}
        }
    }
}

fn assert_fumen_fixture_path(path: &str) {
    assert!(
        path.starts_with("tests/fixtures/fumens/external-pc/"),
        "external PC fumen must be stored under external-pc fumen fixtures: {path}"
    );
    let contents = fs::read_to_string(workspace_root().join(path))
        .unwrap_or_else(|error| panic!("failed to read fumen fixture {path}: {error}"));
    assert!(
        contents.trim_start().starts_with("v115@"),
        "external PC fumen fixture must use normalized fumen-like v115 payload: {path}"
    );
}

fn unique_solution_labels(source_set: &SourceSolutionSet) -> BTreeSet<&str> {
    source_set
        .solutions
        .iter()
        .map(|solution| solution.label.as_str())
        .collect()
}

fn assert_solution_label(
    source_set: &SourceSolutionSet,
    page: u32,
    expected_label: &str,
    expected_percent: &str,
) {
    let solution = source_set
        .solutions
        .iter()
        .find(|solution| solution.page == page)
        .unwrap_or_else(|| panic!("missing source solution page {page}"));
    assert_eq!(solution.label, expected_label);
    assert_eq!(solution.source_percent, expected_percent);
}

#[derive(Debug, Deserialize)]
struct ExternalPcFixture {
    kind: String,
    fixture_id: String,
    source: ExternalPcSource,
    input: ExternalPcInput,
    #[serde(default)]
    source_solution_labels: Vec<SourceSolutionLabel>,
    #[serde(default)]
    source_counts: Option<TsarSourceCounts>,
    #[serde(default)]
    clearra_count_policy: Option<TsarCountPolicy>,
    expected: ExternalPcExpected,
}

#[derive(Debug, Deserialize)]
struct ExternalPcSource {
    source_id: String,
    human_verified: bool,
}

#[derive(Debug, Deserialize)]
struct ExternalPcInput {
    initial_fumen: String,
    #[serde(default)]
    expected_solution_fumen: Option<String>,
    #[serde(default)]
    setup_contract: Option<String>,
    #[serde(default)]
    hold_piece: Option<String>,
    #[serde(default)]
    placed_piece_count: Option<u32>,
    second_bag_pc_entry: Option<bool>,
    goal: String,
}

#[derive(Debug, Deserialize)]
struct SourceSolutionLabel {
    label: String,
    #[serde(default)]
    may_require_180: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct TsarSourceCounts {
    minimal_solve_set: u32,
    minimal_plus_tspin_extra: u32,
    unique_solve_set: u32,
}

#[derive(Debug, Deserialize)]
struct TsarCountPolicy {
    worker_correctness_basis: Value,
    expected_unique_solution_count: u32,
    minimal_solve_set_is_metadata_only: bool,
}

#[derive(Debug, Deserialize)]
struct NormalizeReport {
    kind: String,
    fixture_id: String,
    source_unique_solution_count: u32,
    normalized_unique_solution_count: u32,
    solution_set_contract: String,
    comment_ignored: bool,
    mirror_policy: String,
    page_count: u32,
    decoded_page_count: u32,
    solution_set_hash_algorithm: String,
    solution_set_hash: String,
}

#[derive(Debug, Deserialize)]
struct SourceSolutionSet {
    kind: String,
    source_solution_set_id: String,
    source_id: String,
    solution_set_contract: String,
    source_page_count: u32,
    source_unique_label_count: u32,
    operation_page_count: u32,
    operation_replay_available: bool,
    worker_correctness_basis: Value,
    raw_fumen_string_exact_equality: bool,
    solutions: Vec<SourceSolution>,
}

#[derive(Debug, Deserialize)]
struct SourceSolution {
    page: u32,
    source_percent: String,
    label: String,
}

#[derive(Debug, Deserialize)]
struct ExternalPcExpected {
    #[serde(default)]
    worker_correctness_gate_enabled: Option<bool>,
    #[serde(default)]
    worker_correctness_blocked_reason: Option<String>,
    #[serde(default)]
    expected_unique_solution_count: Option<u32>,
    #[serde(default)]
    unique_solution_count_basis: Option<String>,
    #[serde(default)]
    oracle_kind: Option<String>,
    #[serde(default)]
    expected_normalized_unique_solution_count: Option<u32>,
    #[serde(default)]
    documented_source_page_count: Option<u32>,
    #[serde(default)]
    worker_correctness_basis: Option<String>,
    #[serde(default)]
    packing_candidate_is_solution: Option<bool>,
    #[serde(default)]
    coverage_row_created_after_buildup: Option<bool>,
}

mod case_pco_i_hold_fixture_preserves_source_metadata {
    use super::*;

    #[test]
    fn pco_i_hold_fixture_preserves_source_metadata() {
        let fixture = pco_fixture();

        assert_eq!(fixture.kind, "external-pc-worker-fixture");
        assert_eq!(fixture.fixture_id, "pco_i_hold_6p_second_bag_pc");
        assert_eq!(fixture.source.source_id, "pcinfo-korea-pco-6p-i-hold");
        assert!(fixture.source.human_verified);
        assert_eq!(fixture.input.goal, "clear-to-empty");
        assert_eq!(
            fixture.input.setup_contract.as_deref(),
            Some("user-confirmed-v115-occupancy-field")
        );
        assert_eq!(fixture.input.hold_piece.as_deref(), Some("I"));
        assert_eq!(fixture.input.placed_piece_count, Some(6));
        assert_eq!(fixture.input.second_bag_pc_entry, Some(true));
        assert_eq!(
            fixture.expected.oracle_kind.as_deref(),
            Some("source-fumen-colored-tiling-set")
        );
        assert_eq!(fixture.expected.documented_source_page_count, Some(63));
        assert_eq!(
            fixture.expected.expected_normalized_unique_solution_count,
            Some(63)
        );
        assert_eq!(
            fixture.expected.worker_correctness_basis.as_deref(),
            Some("source-fumen-colored-tiling-set")
        );
        assert_eq!(fixture.expected.packing_candidate_is_solution, None);
        assert_eq!(fixture.expected.coverage_row_created_after_buildup, None);
    }
}

mod case_pco_i_hold_fixture_has_required_solution_labels {
    use super::*;

    #[test]
    fn pco_i_hold_fixture_has_required_solution_labels() {
        let fixture = pco_fixture();
        let labels = fixture
            .source_solution_labels
            .iter()
            .map(|label| label.label.as_str())
            .collect::<BTreeSet<_>>();

        for required_label in [
            "I-OIJ", "I-OLZ", "I-OJZ", "I-IJS", "I-LSZ", "I-JSZ", "I-TOZ", "I-TIZ", "I-TOL",
            "I-TOJ", "I-TIJ", "I-TLS", "I-TLZ", "I-TJS", "I-TJZ",
        ] {
            assert!(
                labels.contains(required_label),
                "PCO fixture must preserve source solution label {required_label}"
            );
        }
        assert!(
            fixture
                .source_solution_labels
                .iter()
                .any(|label| label.may_require_180.unwrap_or(false)),
            "PCO source labels must preserve may_require_180 metadata"
        );
    }
}

mod case_tsar_cannon_fixture_uses_unique_solve_set_not_minimal_set {
    use super::*;

    #[test]
    fn tsar_cannon_fixture_uses_unique_solve_set_not_minimal_set() {
        let fixture = tsar_fixture();
        let counts = fixture.source_counts.expect("Tsar source counts");
        let policy = fixture.clearra_count_policy.expect("Tsar count policy");

        assert_eq!(counts.minimal_solve_set, 18);
        assert_eq!(counts.minimal_plus_tspin_extra, 25);
        assert_eq!(counts.unique_solve_set, 42);
        assert_eq!(policy.worker_correctness_basis, "unique_solve_set");
        assert_eq!(
            policy.expected_unique_solution_count,
            counts.unique_solve_set
        );
        assert!(policy.minimal_solve_set_is_metadata_only);
    }
}

mod case_tsar_cannon_expected_solution_count_is_42 {
    use super::*;

    #[test]
    fn tsar_cannon_expected_solution_count_is_42() {
        let fixture = tsar_fixture();

        assert_eq!(fixture.source.source_id, "hse30-tsar-cannon-full-42");
        assert!(fixture.source.human_verified);
        assert_eq!(fixture.input.goal, "clear-to-empty");
        assert_eq!(fixture.expected.expected_unique_solution_count, Some(42));
        assert_eq!(fixture.expected.worker_correctness_gate_enabled, None);
        assert_eq!(fixture.expected.worker_correctness_blocked_reason, None);
        assert_eq!(
            fixture.expected.unique_solution_count_basis.as_deref(),
            Some("source-fumen-colored-tiling-set")
        );
        assert_eq!(fixture.expected.packing_candidate_is_solution, Some(false));
        assert_eq!(
            fixture.expected.coverage_row_created_after_buildup,
            Some(true)
        );
    }
}

mod case_tsar_cannon_normalize_report_pins_full_42_solution_set {
    use super::*;

    #[test]
    fn tsar_cannon_normalize_report_pins_full_42_solution_set() {
        let report: NormalizeReport =
            read_json("tests/fixtures/external-pc/tsar_cannon_after_2bag_full_42.normalize.json");

        assert_eq!(report.kind, "external-pc-normalize-report");
        assert_eq!(report.fixture_id, "tsar_cannon_after_2bag_full_42");
        assert_eq!(report.source_unique_solution_count, 42);
        assert_eq!(report.normalized_unique_solution_count, 42);
        assert_eq!(
            report.solution_set_contract,
            "fumen-normalized-solution-set"
        );
        assert!(report.comment_ignored);
        assert_eq!(report.mirror_policy, "none");
        assert_eq!(report.page_count, 42);
        assert_eq!(report.decoded_page_count, 44);
        assert_eq!(
            report.solution_set_hash_algorithm,
            "worker-e2e-normalized-solution-key-fnv64-v1"
        );
        assert_eq!(report.solution_set_hash, "wes1:548277ae9ac32701");
    }
}

mod case_external_pc_source_solution_sets_pin_user_confirmed_counts {
    use super::*;

    #[test]
    fn external_pc_source_solution_sets_pin_user_confirmed_counts() {
        let pco: SourceSolutionSet = read_json(PCO_SOURCE_SET_PATH);
        let tsar: SourceSolutionSet = read_json(TSAR_SOURCE_SET_PATH);

        assert_eq!(pco.kind, "external-pc-source-solution-set");
        assert_eq!(pco.source_solution_set_id, "four-pco-opener-full-63");
        assert_eq!(pco.source_id, "four-pco-opener-full-63");
        assert_eq!(pco.solution_set_contract, "fumen-colored-tiling-set");
        assert_eq!(pco.source_page_count, 63);
        assert_eq!(pco.source_unique_label_count, 58);
        assert_eq!(pco.operation_page_count, 0);
        assert!(!pco.operation_replay_available);
        assert_eq!(
            pco.worker_correctness_basis,
            Value::String("source-fumen-colored-tiling-set".to_owned())
        );
        assert!(!pco.raw_fumen_string_exact_equality);
        assert_eq!(pco.solutions.len(), 63);
        assert_eq!(unique_solution_labels(&pco).len(), 58);
        assert_solution_label(&pco, 1, "LITI", "11.4");
        assert_solution_label(&pco, 63, "JTSZ", "0.1");

        assert_eq!(tsar.kind, "external-pc-source-solution-set");
        assert_eq!(
            tsar.source_solution_set_id,
            "hse30-tsar-cannon-full-42-v115"
        );
        assert_eq!(tsar.source_id, "hse30-tsar-cannon-full-42");
        assert_eq!(
            tsar.solution_set_contract,
            "fumen-source-label-solution-set"
        );
        assert_eq!(tsar.source_page_count, 42);
        assert_eq!(tsar.source_unique_label_count, 39);
        assert_eq!(tsar.operation_page_count, 0);
        assert!(!tsar.operation_replay_available);
        assert_eq!(tsar.worker_correctness_basis, Value::Bool(true));
        assert!(!tsar.raw_fumen_string_exact_equality);
        assert_eq!(tsar.solutions.len(), 42);
        assert_eq!(unique_solution_labels(&tsar).len(), 39);
        assert_solution_label(&tsar, 1, "TSOJIL", "48.0");
        assert_solution_label(&tsar, 42, "SZTOJL", "4.4");
    }
}

mod case_external_pc_fixture_does_not_use_raw_image_golden {
    use super::*;

    #[test]
    fn external_pc_fixture_does_not_use_raw_image_golden() {
        for path in [
            PCO_FIXTURE_PATH,
            PCO_SOURCE_SET_PATH,
            TSAR_FIXTURE_PATH,
            TSAR_SOURCE_SET_PATH,
            "tests/golden/external-pc/pco_i_hold_6p_second_bag_pc.json",
            "tests/golden/external-pc/tsar_cannon_after_2bag_full_42.json",
        ] {
            let value = read_json_value(path);
            assert_no_forbidden_image_golden_keys(path, &value);
        }
    }
}

mod case_external_pc_fixture_requires_human_verified_fumen {
    use super::*;

    #[test]
    fn external_pc_fixture_requires_human_verified_fumen() {
        for fixture in [pco_fixture(), tsar_fixture()] {
            assert!(
                fixture.source.human_verified,
                "{} must require human-verified external PC materialization",
                fixture.fixture_id
            );
            assert_fumen_fixture_path(&fixture.input.initial_fumen);
            if let Some(expected_solution_fumen) = fixture.input.expected_solution_fumen.as_deref()
            {
                assert_fumen_fixture_path(expected_solution_fumen);
            }
        }
    }
}
