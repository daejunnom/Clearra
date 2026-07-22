use super::*;
use crate::json::json_contract::{JsonContract, JsonField};
use crate::model::{RenderField, RenderFieldValue};
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_replay::{BuildVariantOperation, BuildVariantReplayInput, ReplayEngine};

#[test]
fn escapes_control_characters() {
    let contract = JsonContract::new(vec![JsonField::new("page", "a\nb\rc\td")]);

    assert_eq!(JsonWriter::write(&contract), "{\"page\":\"a\\nb\\rc\\td\"}");
}

#[test]
fn writes_typed_nested_json_values() {
    let contract = JsonContract::from_render_message(
        "pc",
        &[
            RenderField::new("solution_found", RenderFieldValue::bool(true)),
            RenderField::new("total_solution_count", RenderFieldValue::number("3")),
            RenderField::new("count_complete", RenderFieldValue::bool(false)),
        ],
    );

    let rendered = JsonWriter::write(&contract);

    assert!(rendered.contains("\"schema_version\":2"));
    assert!(rendered.contains("\"solution_found\":true"));
    assert!(rendered.contains("\"total_solution_count\":3"));
    assert!(rendered.contains("\"count_complete\":false"));
    assert!(rendered.contains("\"contract\":{\"command\""));
}

#[test]
fn writes_replay_trace_json_contract() {
    let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
    let input = BuildVariantReplayInput::new(
        "variant-1",
        layout,
        0,
        vec![BuildVariantOperation::new(
            PieceKind::O,
            RotationState::Zero,
            0,
            0,
        )],
    );
    let trace = ReplayEngine::build_variant_to_trace(&input).expect("trace");

    let rendered = JsonWriter::write_replay_trace(&trace);

    assert!(rendered.contains("\"kind\":\"replay-trace\""));
    assert!(rendered.contains("\"representative\":true"));
    assert!(rendered.contains("\"sample\":true"));
    assert!(rendered.contains("\"colored_cell_ownership\""));
}
