// SRP rationale: this integration test has one behavior-level change reason:
// preserving the finite terminal-hold solution set in the public CLI JSON contract.

use clearra_app::ProductBuildIdentity;
use clearra_cli::{exit::ExitCode, run_with_args};
use clearra_core_domain::solution::normalized_tiling_solution::{
    NormalizedTilingSolutionKey, NormalizedTilingSolutionSet,
};
use serde_json::Value;

const TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT: usize = 18;
const TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH: &str = "cts1:8a7fc484d9b49994";

fn assert_terminal_supply_solution_set(keys: &[String]) {
    assert_eq!(keys.len(), TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT);
    assert!(keys.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(keys
        .iter()
        .all(|key| { key.starts_with("ctk1|initial=00000001c0701c07|placements=") }));

    let normalized = NormalizedTilingSolutionSet::new(keys.iter().map(|key| {
        NormalizedTilingSolutionKey::parse_canonical(key)
            .unwrap_or_else(|error| panic!("non-canonical CLI solution key {key:?}: {error:?}"))
    }));
    assert_eq!(normalized.len(), TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT);
    assert_eq!(
        normalized.hash(),
        TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH
    );
}

#[test]
fn terminal_supply_p0_cli_json_preserves_the_exact_solution_set() {
    let output = run_with_args([
        "clearra",
        "--format",
        "json",
        "--include-solution-data",
        "pc-scenario",
        "--field",
        "0x1c0701c07",
        "--visible-height",
        "4",
        "--queue",
        "STOILJZ",
        "--max-pieces",
        "7",
        "--exact-pieces",
        "7",
        "--count-policy",
        "count-unique",
        "--backend",
        "cpu",
        "--workers",
        "1",
    ]);
    assert_eq!(output.exit_code(), ExitCode::Success, "{}", output.stderr());

    let value: Value = serde_json::from_str(output.stdout()).expect("public CLI JSON");
    let identity = value["runtime_identity"]
        .as_object()
        .expect("CLI runtime_identity object");
    let expected_identity = ProductBuildIdentity::current();
    assert_eq!(identity.len(), 5);
    assert_eq!(
        identity["engine_build_id"],
        expected_identity.engine_build_id()
    );
    assert_eq!(identity["source_commit"], expected_identity.source_commit());
    assert_eq!(
        identity["contract_schema_version"],
        expected_identity.contract_schema_version()
    );
    assert_eq!(
        identity["supply_semantics_id"],
        expected_identity.supply_semantics_id()
    );
    assert_eq!(
        identity["artifact_schema_version"],
        expected_identity.artifact_schema_version()
    );
    if let Some(expected) = option_env!("CLEARRA_SOURCE_COMMIT") {
        assert_eq!(identity["source_commit"], expected);
    }
    if let Some(expected) = option_env!("CLEARRA_ENGINE_BUILD_ID") {
        assert_eq!(identity["engine_build_id"], expected);
    }
    assert_eq!(value["kind"], "pc-scenario");
    assert_eq!(
        value["summary"]["unique_solution_count"],
        TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT
    );
    assert_eq!(
        value["summary"]["normalized_unique_solution_count"],
        TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT
    );
    assert_eq!(value["summary"]["solution_count_calculated"], true);
    assert_eq!(value["summary"]["solution_set_materialized"], true);
    assert_eq!(
        value["summary"]["solution_keys_materialized_count"],
        TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT
    );
    assert_eq!(value["summary"]["solution_keys_complete"], true);
    assert_eq!(
        value["summary"]["supply_window_resolution"],
        "projected-terminal-lookahead"
    );
    assert_eq!(value["summary"]["projects_unplaced_lookahead"], true);
    assert_eq!(value["summary"]["projects_standard_bag_lookahead"], false);
    assert_eq!(value["summary"]["source_sequence_length"], 7);
    assert_eq!(value["summary"]["total_possible_pattern_count"], "1");
    assert_eq!(
        value["summary"]["normalized_solution_set_hash"],
        TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH
    );
    assert_eq!(
        value["summary"]["actual_normalized_solution_set_hash"],
        TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH
    );
    assert_eq!(value["contract"]["solution_data"]["requested"], true);
    assert_eq!(value["contract"]["solution_data"]["status"], "complete");
    assert_eq!(value["contract"]["solution_data"]["reason"], Value::Null);

    let keys = value["contract"]["artifacts"]["solution_keys"]
        .as_array()
        .expect("complete CLI solution-key artifact")
        .iter()
        .map(|key| key.as_str().expect("CLI solution key string").to_owned())
        .collect::<Vec<_>>();
    assert_terminal_supply_solution_set(&keys);
}
