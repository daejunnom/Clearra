use clearra_app::{AppContext, AppStatus};
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
