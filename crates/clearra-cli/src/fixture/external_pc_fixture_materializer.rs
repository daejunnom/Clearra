use std::path::Path;

use clearra_fumen::{SourceFumenDiagramSet, SourceFumenSetup};
use serde_json::{json, Map, Value};

use super::{
    external_pc_fixture_materializer_fields::{
        format_mask, required_object, required_string, scalar_bool, scalar_string, scalar_usize,
    },
    external_pc_fixture_materializer_fumen::{
        read_initial_setup, read_source_fumen_diagrams_if_requested,
    },
};

const MATERIALIZED_SCENARIO_MISMATCH: &str = "E_EXTERNAL_PC_MATERIALIZED_SCENARIO_MISMATCH";

pub(super) struct ExternalPcFixtureMaterializer;

impl ExternalPcFixtureMaterializer {
    pub(super) fn materialize(fixture_path: &Path, value: Value) -> Result<Value, String> {
        let fixture_id = required_string(&value, "/fixture_id")?;
        let source_id = required_string(&value, "/source/source_id")?;
        let human_verified = value
            .pointer("/source/human_verified")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let input = required_object(&value, "/input")?;
        let mut expected = value
            .pointer("/input/materialized_expected")
            .cloned()
            .ok_or_else(|| "external PC fixture missing input.materialized_expected".to_owned())?;
        let source_diagrams =
            read_source_fumen_diagrams_if_requested(fixture_path, source_id, input)?;
        if let Some(diagrams) = source_diagrams.as_ref() {
            add_source_diagram_expected(&mut expected, diagrams)?;
        }
        let setup = read_initial_setup(fixture_path, input)?;
        let scenario = derive_scenario_from_external_pc_input(input, setup)?;
        assert_materialized_scenario_cache_matches(input, &scenario)?;

        Ok(json!({
            "name": fixture_id,
            "source": {
                "site": source_id,
                "page": "external-pc-worker-fixture",
                "section": fixture_id,
                "human_verified": human_verified
            },
            "scenario": scenario,
            "expected": expected
        }))
    }
}

fn add_source_diagram_expected(
    expected: &mut Value,
    diagrams: &SourceFumenDiagramSet,
) -> Result<(), String> {
    let expected = expected
        .as_object_mut()
        .ok_or_else(|| "input.materialized_expected must be an object".to_owned())?;
    let keys = diagrams
        .solution_set()
        .keys()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    expected.insert(
        "normalized_solution_oracle".to_owned(),
        Value::String("source-fumen-count-and-tiling-set".to_owned()),
    );
    expected.insert(
        "expected_normalized_solution_set_hash".to_owned(),
        Value::String(diagrams.solution_set().hash().to_owned()),
    );
    expected.insert(
        "expected_normalized_solution_keys".to_owned(),
        serde_json::to_value(keys).map_err(|error| error.to_string())?,
    );
    expected.insert(
        "operation_replay_available".to_owned(),
        Value::Bool(diagrams.operation_replay_available()),
    );
    Ok(())
}

fn derive_scenario_from_external_pc_input(
    input: &Map<String, Value>,
    setup: SourceFumenSetup,
) -> Result<Value, String> {
    let board_width = scalar_usize(input, "board_width")?.unwrap_or(10);
    if board_width != 10 {
        return Err("external PC v115 setup must use the standard 10-wide field".to_owned());
    }
    let visible_height =
        scalar_usize(input, "visible_height")?.unwrap_or(usize::from(setup.visible_height()));
    if visible_height != usize::from(setup.visible_height()) {
        return Err(format!(
            "external PC fixture visible_height {visible_height} does not match decoded setup height {}",
            setup.visible_height()
        ));
    }
    let remaining_queue = scalar_string(input, "remaining_queue").unwrap_or_default();
    let queue_mode = scalar_string(input, "queue_mode").unwrap_or_else(|| "fixed".to_owned());
    let max_pieces =
        scalar_usize(input, "piece_window")?.unwrap_or_else(|| remaining_queue.chars().count());
    let exact_pieces = scalar_usize(input, "exact_pieces")?.unwrap_or(max_pieces);
    let rule = scalar_string(input, "rule").unwrap_or_else(|| "srs-plus".to_owned());
    let goal = scalar_string(input, "goal").unwrap_or_else(|| "clear-to-empty".to_owned());
    let hold = derive_hold(input)?;
    let budget = input.get("materialized_budget").and_then(Value::as_object);

    Ok(json!({
        "board_width": board_width,
        "visible_height": visible_height,
        "initial_board_mask": format_mask(setup.initial_board_mask()),
        "remaining_queue": remaining_queue,
        "queue_mode": queue_mode,
        "hold": hold,
        "rule": rule,
        "requires_180": scalar_bool(input, "requires_180")?.unwrap_or(false),
        "goal": goal,
        "max_pieces": max_pieces,
        "exact_pieces": exact_pieces,
        "min_remaining_queue": scalar_usize(input, "min_remaining_queue")?.unwrap_or(0),
        "allow_hold": scalar_bool(input, "allow_hold")?.unwrap_or(true),
        "count_policy": scalar_string(input, "count_policy").unwrap_or_else(|| "count-all".to_owned()),
        "retained_trace_limit": scalar_usize(input, "retained_trace_limit")?.unwrap_or(1),
        "max_candidates": budget.and_then(|value| value.get("max_candidates")).and_then(Value::as_u64).unwrap_or(250_000),
        "max_patterns": budget.and_then(|value| value.get("max_patterns")).and_then(Value::as_u64).unwrap_or(5_040),
        "max_frontier_states": budget.and_then(|value| value.get("max_frontier_states")).and_then(Value::as_u64).unwrap_or(1_000_000)
    }))
}

fn derive_hold(input: &Map<String, Value>) -> Result<Value, String> {
    if scalar_bool(input, "hold_empty")?.unwrap_or(false) {
        return Ok(Value::Null);
    }
    Ok(scalar_string(input, "hold_piece")
        .map(Value::String)
        .unwrap_or(Value::Null))
}

fn assert_materialized_scenario_cache_matches(
    input: &Map<String, Value>,
    derived_scenario: &Value,
) -> Result<(), String> {
    let Some(cache) = input.get("materialized_scenario") else {
        return Ok(());
    };
    if cache == derived_scenario {
        return Ok(());
    }

    Err(format!(
        "{MATERIALIZED_SCENARIO_MISMATCH} external PC fixture input.materialized_scenario is an optional cache and does not match the Fumen-derived scenario; first mismatch: {}",
        first_mismatch(cache, derived_scenario)
    ))
}

fn first_mismatch(left: &Value, right: &Value) -> String {
    let (Some(left_object), Some(right_object)) = (left.as_object(), right.as_object()) else {
        return format!("cache={left} derived={right}");
    };
    let mut keys = left_object
        .keys()
        .chain(right_object.keys())
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();
    for key in keys {
        if left_object.get(key) != right_object.get(key) {
            let left_value = left_object.get(key).unwrap_or(&Value::Null);
            let right_value = right_object.get(key).unwrap_or(&Value::Null);
            return format!("{key}: cache={left_value} derived={right_value}");
        }
    }
    "unknown".to_owned()
}
