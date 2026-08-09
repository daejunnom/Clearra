use clearra_app::{
    AppContext, AppCoreExecutorService, AppServices, AppStatus, FinesseReport, FinesseReportInput,
    FinesseReportPlacement, FinesseRepresentativeWitness,
};
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_web_command::WebCommandParser;

use super::*;

const DAMAGE_TWO_WORKERS: &str = concat!(
    "clearra damage --board-mask 0xffbfe --height 4 --queue IOTJ --no-hold ",
    "--spin-profile all-mini-plus --minimum-damage 1 --workers 2"
);
const SPIN_TWO_WORKERS: &str = concat!(
    "clearra spin-finder --board-mask 0xffbfe --height 4 --queue IOTJ --no-hold ",
    "--spin-profile t-spins-plus --lines any --workers 2"
);
const STRUCTURE_TWO_WORKERS: &str = concat!(
    "clearra spin-structure --board-mask 0x5000010 --height 4 --pieces T ",
    "--spin-profile t-spins --lines any --fill-top 4 --max-placements 1 --workers 2"
);
const STRUCTURE_WITH_COMPLETED_INPUT_ROW: &str = concat!(
    "clearra spin-structure --board-mask 0x14000043ff --height 4 --pieces T ",
    "--spin-profile t-spins --lines any --fill-top 4 --max-placements 1 --workers 2"
);

fn render_forward(command: &str, format: RenderFormat) -> String {
    let request = WebCommandParser::parse_with_worker_limit(command, 8)
        .expect("forward CLI command")
        .to_app_request()
        .expect("typed app request");
    assert_eq!(request.resource_budget().workers(), 2);
    let response = AppContext::default().run(request);
    assert_eq!(response.status(), AppStatus::Success);

    AppResponseRenderer::render(response, format, CliErrorCode::ProductRuntimeUnsupported)
        .stdout()
        .to_owned()
}

#[test]
fn finesse_renderer_preserves_the_typed_representative_witness() {
    let report = FinesseReport::new("search", "oracle", true, Some("3".to_owned()), vec![])
        .with_representative_witness(FinesseRepresentativeWitness::new(
            "oracle",
            Some("solution-a".to_owned()),
            vec![4],
            vec![PieceKind::T],
            3,
            vec![
                FinesseReportInput::TapLeft,
                FinesseReportInput::RotateClockwise,
                FinesseReportInput::HardDrop,
            ],
            vec![FinesseReportPlacement::new(
                PieceKind::T,
                RotationState::Right,
                2,
                0,
            )],
        ));
    let RenderFieldValue::Object(fields) = finesse_report_value(&report) else {
        panic!("finesse report object");
    };
    assert_eq!(
        fields
            .iter()
            .find(|field| field.key() == "exact_total_inputs")
            .map(|field| field.value()),
        Some(&RenderFieldValue::string("3"))
    );
    let witness = fields
        .iter()
        .find(|field| field.key() == "representative_witness")
        .expect("representative witness field");
    assert_eq!(
        witness.value(),
        &RenderFieldValue::object([
            ("policy", RenderFieldValue::string("oracle")),
            ("solution_key", RenderFieldValue::string("solution-a")),
            (
                "pattern_ids",
                RenderFieldValue::array([RenderFieldValue::from(4_usize)]),
            ),
            (
                "queue",
                RenderFieldValue::array([RenderFieldValue::string("T")]),
            ),
            ("total_inputs", RenderFieldValue::from(3_u32)),
            (
                "input_sequence",
                RenderFieldValue::array([
                    RenderFieldValue::string("tap-left"),
                    RenderFieldValue::string("rotate-clockwise"),
                    RenderFieldValue::string("hard-drop"),
                ]),
            ),
            (
                "placements",
                RenderFieldValue::array([RenderFieldValue::object([
                    ("piece", RenderFieldValue::string("T")),
                    ("rotation", RenderFieldValue::from(1_u8)),
                    ("x", RenderFieldValue::from(2_i16)),
                    ("y", RenderFieldValue::from(0_i16)),
                ])]),
            ),
        ])
    );
}

#[test]
fn fixed_queue_finesse_score_cli_json_preserves_the_typed_public_contract() {
    let request = WebCommandParser::parse_with_worker_limit(
        "clearra finesse score --initial-mask 0 --height 4 \
         --placements O:spawn:4:0 --queue O --no-hold --pattern-knowledge both \
         --rule srs-plus --workers 2",
        4,
    )
    .expect("finesse score CLI command")
    .to_app_request()
    .expect("typed score request");
    let response = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    )
    .run(request);
    assert_eq!(response.status(), AppStatus::Success);

    let rendered = AppResponseRenderer::render_with_solution_data(
        response,
        RenderFormat::Json,
        CliErrorCode::ProductRuntimeUnsupported,
        true,
    )
    .stdout()
    .to_owned();
    let value: serde_json::Value = serde_json::from_str(&rendered).expect("score CLI JSON");

    assert_eq!(value["finesse_report"]["mode"], "score");
    assert_eq!(value["finesse_report"]["exact_total_inputs"], "1");
    assert_eq!(
        value["contract"]["artifacts"]["finesse_report"],
        value["finesse_report"]
    );
    assert_eq!(
        value["contract"]["artifacts"]["finesse_score"]["representative_path"][0]["piece"],
        "O"
    );
    assert!(
        !rendered.contains("wasm-cpu-finesse-score"),
        "the browser adapter fallback must not leak into CLI output"
    );
}

#[test]
fn damage_and_spin_json_report_the_two_requested_workers() {
    for command in [DAMAGE_TWO_WORKERS, SPIN_TWO_WORKERS] {
        let rendered = render_forward(command, RenderFormat::Json);

        assert!(rendered.contains("\"workers_used\":2"), "{rendered}");
    }
}

#[test]
fn damage_and_spin_text_profiles_report_the_two_requested_workers() {
    for command in [DAMAGE_TWO_WORKERS, SPIN_TWO_WORKERS] {
        for format in [RenderFormat::Text, RenderFormat::TextVerbose] {
            let rendered = render_forward(command, format);

            assert!(rendered.contains("workers_used: 2"), "{rendered}");
        }
    }
}

#[test]
fn spin_structure_json_exposes_logical_ctk3_artifacts_without_large_result_arrays() {
    let request = WebCommandParser::parse_with_worker_limit(STRUCTURE_TWO_WORKERS, 8)
        .expect("structure CLI command")
        .to_app_request()
        .expect("typed app request");
    let response = AppContext::default().run(request);
    assert_eq!(response.status(), AppStatus::Success);

    let rendered = AppResponseRenderer::render_with_solution_data(
        response,
        RenderFormat::Json,
        CliErrorCode::ProductRuntimeUnsupported,
        true,
    )
    .stdout()
    .to_owned();
    let value: serde_json::Value = serde_json::from_str(&rendered).expect("structure JSON");

    assert_eq!(value["kind"], "spin-structure");
    assert_eq!(value["summary"]["workers_used"], 2);
    assert!(value["summary"]["result_count"].as_u64().unwrap_or(0) > 0);
    assert!(value["summary"].get("regular").is_none());
    assert!(value["summary"].get("mini").is_none());
    let keys = value["contract"]["artifacts"]["solution_keys"]
        .as_array()
        .expect("solution keys");
    assert!(!keys.is_empty());
    assert!(keys.iter().all(|key| key
        .as_str()
        .is_some_and(|key| key.starts_with("ctk2|height=4|initial="))));
    let classes = value["contract"]["artifacts"]["solution_classes"]
        .as_array()
        .expect("solution classes");
    assert_eq!(classes.len(), keys.len());
    assert_eq!(
        classes
            .iter()
            .filter(|class| class.as_str() == Some("regular"))
            .count(),
        value["summary"]["regular_count"].as_u64().unwrap_or(0) as usize
    );
    assert_eq!(
        classes
            .iter()
            .filter(|class| class.as_str() == Some("mini"))
            .count(),
        value["summary"]["mini_count"].as_u64().unwrap_or(0) as usize
    );
}

#[test]
fn spin_structure_ctk3_keys_start_from_the_line_cleared_input_board() {
    let request = WebCommandParser::parse_with_worker_limit(STRUCTURE_WITH_COMPLETED_INPUT_ROW, 8)
        .expect("structure CLI command")
        .to_app_request()
        .expect("typed app request");
    let response = AppContext::default().run(request);
    assert_eq!(response.status(), AppStatus::Success);

    let rendered = AppResponseRenderer::render_with_solution_data(
        response,
        RenderFormat::Json,
        CliErrorCode::ProductRuntimeUnsupported,
        true,
    )
    .stdout()
    .to_owned();
    let value: serde_json::Value = serde_json::from_str(&rendered).expect("structure JSON");
    let keys = value["contract"]["artifacts"]["solution_keys"]
        .as_array()
        .expect("solution keys");
    assert!(!keys.is_empty());
    for key in keys {
        let key = key.as_str().expect("solution key string");
        let initial = key
            .split("|initial=")
            .nth(1)
            .and_then(|value| value.split('|').next())
            .expect("initial board segment");
        assert_eq!(initial.trim_start_matches('0'), "5000010");
    }
}
