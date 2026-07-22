use super::PcScenarioFixture;

use std::{fs, path::PathBuf};

use serde_json::{json, Map, Value};

fn fixture_json(extra_root_field: &str) -> String {
    format!(
        r#"{{
  {extra_root_field}
  "name": "schema_contract",
  "source": {{
    "site": "clearra",
    "page": "fixture schema",
    "section": "contract",
    "human_verified": true
  }},
  "scenario": {{
    "board_width": 10,
    "visible_height": 2,
    "initial_board_mask": "0x0",
    "remaining_queue": "I",
    "hold": null,
    "rule": "srs-90",
    "requires_180": false,
    "goal": "clear-to-empty",
    "max_pieces": 1
  }},
  "expected": {{
    "solution_exists": true,
    "expected_total_solution_count": 1,
    "count_complete": true,
    "accepted_retained_trace_keys": []
  }}
}}"#
    )
}

fn external_pc_fixture_json(
    include_initial_fumen: bool,
    include_materialized_scenario: bool,
    cache_override: Option<(&str, Value)>,
) -> String {
    let mut input = Map::new();
    if include_initial_fumen {
        input.insert(
            "initial_fumen".to_owned(),
            Value::String(external_setup_fumen_path()),
        );
    }
    input.insert("board_width".to_owned(), json!(10));
    input.insert("visible_height".to_owned(), json!(4));
    input.insert(
        "expected_setup_rows_top_down".to_owned(),
        json!(["OOXXXXXOOO", "OOOXXXXOOO", "OOOOXXXOOO", "OOOXXXXOOO"]),
    );
    input.insert("remaining_queue".to_owned(), json!("OIJT"));
    input.insert("piece_window".to_owned(), json!(4));
    input.insert("exact_pieces".to_owned(), json!(4));
    input.insert("goal".to_owned(), json!("clear-to-empty"));
    input.insert("rule".to_owned(), json!("srs-plus"));
    input.insert("hold_piece".to_owned(), json!("I"));
    input.insert("hold_empty".to_owned(), json!(false));
    input.insert("allow_hold".to_owned(), json!(true));
    input.insert("count_policy".to_owned(), json!("count-all"));
    input.insert("retained_trace_limit".to_owned(), json!(1));
    if include_materialized_scenario {
        let mut materialized_scenario = json!({
            "board_width": 10,
            "visible_height": 4,
            "initial_board_mask": "0x000000e0f87e3f87",
            "remaining_queue": "OIJT",
            "queue_mode": "fixed",
            "hold": "I",
            "rule": "srs-plus",
            "requires_180": false,
            "goal": "clear-to-empty",
            "max_pieces": 4,
            "exact_pieces": 4,
            "min_remaining_queue": 0,
            "allow_hold": true,
            "count_policy": "count-all",
            "retained_trace_limit": 1,
            "max_candidates": 250000,
            "max_patterns": 5040,
            "max_frontier_states": 1000000
        });
        if let Some((key, value)) = cache_override {
            materialized_scenario
                .as_object_mut()
                .expect("scenario object")
                .insert(key.to_owned(), value);
        }
        input.insert("materialized_scenario".to_owned(), materialized_scenario);
    }
    input.insert(
        "materialized_expected".to_owned(),
        json!({
            "solution_exists": true,
            "expected_total_solution_count": 1,
            "count_complete": true,
            "accepted_retained_trace_keys": []
        }),
    );

    json!({
        "schema_version": 1,
        "kind": "external-pc-worker-fixture",
        "fixture_id": "pco_i_hold_6p_second_bag_pc",
        "source": {
            "source_id": "external-source",
            "human_verified": true
        },
        "input": input
    })
    .to_string()
}

fn external_setup_fumen_path() -> String {
    workspace_root()
        .join("tests/fixtures/fumens/external-pc/pco_i_hold_6p_second_bag_pc_setup.fumen")
        .to_string_lossy()
        .into_owned()
}

fn temp_fixture_path(prefix: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{unique}.json"))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn fixture_json_with_source(extra_source_field: &str) -> String {
    fixture_json("").replace(
        r#""human_verified": true"#,
        &format!("{extra_source_field}\n    \"human_verified\": true"),
    )
}

fn fixture_json_with_scenario(extra_scenario_field: &str) -> String {
    fixture_json("").replace(
        r#""max_pieces": 1"#,
        &format!("{extra_scenario_field}\n    \"max_pieces\": 1"),
    )
}

mod case_rejects_unknown_root_source_and_scenario_fields {
    use super::*;

    #[test]
    fn rejects_unknown_root_source_and_scenario_fields() {
        for json in [
            fixture_json(r#""extra_root": true,"#),
            fixture_json_with_source(r#""extra_source": true,"#),
            fixture_json_with_scenario(r#""requires180": false,"#),
        ] {
            let error = serde_json::from_str::<PcScenarioFixture>(&json)
                .expect_err("unknown fields are rejected");

            assert!(
                error.to_string().contains("unknown field"),
                "unexpected error: {error}"
            );
        }
    }
}

mod case_fixture_count_complete_expectation_is_optional {
    use super::*;

    #[test]
    fn fixture_count_complete_expectation_is_optional() {
        let mut value =
            serde_json::from_str::<Value>(&fixture_json("")).expect("fixture JSON value");
        value["expected"]
            .as_object_mut()
            .expect("expected object")
            .remove("count_complete");

        let fixture = serde_json::from_value::<PcScenarioFixture>(value)
            .expect("fixture without count_complete expectation");

        assert_eq!(fixture.expected().count_complete(), None);
    }
}

mod case_read_rejects_sensitive_file_names_before_opening {
    use super::*;

    #[test]
    fn read_rejects_sensitive_file_names_before_opening() {
        let error = PcScenarioFixture::read("credentials.json").expect_err("sensitive path");

        assert!(error.contains("sensitive-looking file path"));
    }
}

mod case_reads_external_pc_worker_fixture_through_external_materializer {
    use super::*;

    #[test]
    fn reads_external_pc_worker_fixture_through_external_materializer() {
        let path = temp_fixture_path("clearra-external-pc-fixture");
        fs::write(&path, external_pc_fixture_json(true, true, None))
            .expect("write external pc fixture");

        let fixture = PcScenarioFixture::read(&path).expect("external pc fixture");

        assert_eq!(fixture.name(), "pco_i_hold_6p_second_bag_pc");
        assert_eq!(fixture.scenario().remaining_queue(), "OIJT");
        assert_eq!(
            fixture.scenario().initial_board_mask(),
            "0x000000e0f87e3f87"
        );
        assert_eq!(fixture.scenario().hold(), Some('I'));
        assert_eq!(fixture.scenario().rule(), "srs-plus");
        assert_eq!(fixture.scenario().max_pieces(), 4);
        assert_eq!(fixture.scenario().exact_pieces(), Some(4));
        assert!(fixture.expected().solution_exists());
        assert_eq!(fixture.expected().expected_total_solution_count(), Some(1));
        assert_eq!(
            fixture.source_fields(),
            vec![
                (
                    "fixture_source_site".to_owned(),
                    "external-source".to_owned()
                ),
                (
                    "fixture_source_page".to_owned(),
                    "external-pc-worker-fixture".to_owned()
                ),
                (
                    "fixture_source_section".to_owned(),
                    "pco_i_hold_6p_second_bag_pc".to_owned()
                ),
                (
                    "fixture_source_human_verified".to_owned(),
                    "true".to_owned()
                ),
            ]
        );

        let _ = fs::remove_file(path);
    }
}

mod case_external_pc_fixture_materializes_from_initial_fumen {
    use super::*;

    #[test]
    fn external_pc_fixture_materializes_from_initial_fumen() {
        let fixture = PcScenarioFixture::read(
            workspace_root().join("tests/fixtures/external-pc/pco_i_hold_6p_second_bag_pc.json"),
        )
        .expect("external PC fixture materialized from initial_fumen");

        assert_eq!(fixture.name(), "pco_i_hold_6p_second_bag_pc");
        assert_eq!(
            fixture.scenario().initial_board_mask(),
            "0x000000e0f87e3f87"
        );
        assert_eq!(fixture.scenario().remaining_queue(), "");
        assert_eq!(fixture.scenario().queue_mode(), "standard-7-bag");
        assert_eq!(fixture.scenario().hold(), Some('I'));
    }
}

mod case_external_pc_worker_fixture_requires_initial_fumen {
    use super::*;

    #[test]
    fn external_pc_worker_fixture_requires_initial_fumen() {
        let path = temp_fixture_path("clearra-external-pc-missing-initial-fumen");
        fs::write(&path, external_pc_fixture_json(false, true, None))
            .expect("write external pc fixture");

        let error = PcScenarioFixture::read(&path).expect_err("initial fumen required");

        assert!(
            error.contains("external PC fixture missing input.initial_fumen"),
            "unexpected error: {error}"
        );

        let _ = fs::remove_file(path);
    }
}

mod case_external_pc_worker_fixture_accepts_missing_materialized_scenario_cache {
    use super::*;

    #[test]
    fn external_pc_worker_fixture_accepts_missing_materialized_scenario_cache() {
        let path = temp_fixture_path("clearra-external-pc-no-cache");
        fs::write(&path, external_pc_fixture_json(true, false, None))
            .expect("write external pc fixture");

        let fixture = PcScenarioFixture::read(&path).expect("external pc fixture without cache");

        assert_eq!(
            fixture.scenario().initial_board_mask(),
            "0x000000e0f87e3f87"
        );
        assert_eq!(fixture.scenario().remaining_queue(), "OIJT");

        let _ = fs::remove_file(path);
    }
}

mod case_external_pc_worker_fixture_rejects_stale_materialized_scenario_cache {
    use super::*;

    #[test]
    pub(crate) fn external_pc_worker_fixture_rejects_stale_materialized_scenario_cache() {
        let path = temp_fixture_path("clearra-external-pc-stale-cache");
        fs::write(
            &path,
            external_pc_fixture_json(
                true,
                true,
                Some(("initial_board_mask", json!("0x00000000000003f0"))),
            ),
        )
        .expect("write external pc fixture");

        let error = PcScenarioFixture::read(&path).expect_err("stale cache rejected");

        assert!(
            error.contains("E_EXTERNAL_PC_MATERIALIZED_SCENARIO_MISMATCH"),
            "unexpected error: {error}"
        );

        let _ = fs::remove_file(path);
    }
}
pub(crate) use case_external_pc_worker_fixture_rejects_stale_materialized_scenario_cache::external_pc_worker_fixture_rejects_stale_materialized_scenario_cache;

mod case_external_pc_materialized_scenario_mismatch_is_error {
    use super::*;

    #[test]
    fn external_pc_materialized_scenario_mismatch_is_error() {
        external_pc_worker_fixture_rejects_stale_materialized_scenario_cache();
    }
}

mod case_pco_runtime_scenario_uses_setup_fumen_mask {
    use super::*;

    #[test]
    fn pco_runtime_scenario_uses_setup_fumen_mask() {
        let fixture = PcScenarioFixture::read(
            workspace_root().join("tests/fixtures/external-pc/pco_i_hold_6p_second_bag_pc.json"),
        )
        .expect("PCO fixture");

        assert_eq!(
            fixture.scenario().initial_board_mask(),
            "0x000000e0f87e3f87"
        );
        assert_ne!(
            fixture.scenario().initial_board_mask(),
            "0x00000000000003f0",
            "external PC runtime scenario must not fall back to the trivial I-piece stub"
        );
    }
}

mod case_tsar_runtime_scenario_uses_setup_fumen_mask {
    use super::*;

    #[test]
    fn tsar_runtime_scenario_uses_setup_fumen_mask() {
        let fixture = PcScenarioFixture::read(
            workspace_root().join("tests/fixtures/external-pc/tsar_cannon_after_2bag_full_42.json"),
        )
        .expect("Tsar fixture");

        assert_eq!(
            fixture.scenario().initial_board_mask(),
            "0x000300c0399e3fdf"
        );
        assert_ne!(
            fixture.scenario().initial_board_mask(),
            "0x00000000000003f0",
            "external PC runtime scenario must not fall back to the trivial I-piece stub"
        );
    }
}

mod case_external_pc_fixture_rejects_trivial_stub_materialization {
    use super::*;

    #[test]
    fn external_pc_fixture_rejects_trivial_stub_materialization() {
        let path = temp_fixture_path("clearra-external-pc-trivial-stub-cache");
        fs::write(
            &path,
            external_pc_fixture_json(
                true,
                true,
                Some(("initial_board_mask", json!("0x00000000000003f0"))),
            ),
        )
        .expect("write external pc fixture");

        let error = PcScenarioFixture::read(&path).expect_err("trivial stub cache rejected");

        assert!(error.contains("E_EXTERNAL_PC_MATERIALIZED_SCENARIO_MISMATCH"));
        assert!(
            error.contains("0x00000000000003f0"),
            "mismatch should point at the stale trivial stub cache: {error}"
        );
        assert!(
            error.contains("0x000000e0f87e3f87"),
            "mismatch should point at the Fumen-derived source-of-truth mask: {error}"
        );

        let _ = fs::remove_file(path);
    }
}
