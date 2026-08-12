use super::*;
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_replay::{BuildVariantOperation, BuildVariantReplayInput, ReplayEngine};

use crate::model::RenderFieldValue;

#[test]
fn dispatches_message_to_text_writer() {
    let message = RenderMessage::new("pc").with_field("lines", "2");

    assert_eq!(
        RenderFormatDispatcher::render(&message, RenderFormat::Text),
        "kind: pc\nlines: 2"
    );
}

#[test]
fn pc_text_default_is_human_sized() {
    let message = RenderMessage::new("pc")
        .with_value("status", "searched")
        .with_value("lines", 2usize)
        .with_value("queue_len", 7usize)
        .with_value("coverage_probability", "1.0")
        .with_value("executor_flow", "rust-shell-to-core")
        .with_value("compact_problem_descriptor", "compact")
        .with_value("gpu_backend_scope", "disabled")
        .with_value("hybrid_scheduler", "not-supported")
        .with_value("score_event_basis", "sample")
        .with_value("coverage_row_view", "raw");

    let rendered = RenderFormatDispatcher::render(&message, RenderFormat::Text);

    assert!(rendered.contains("kind: pc"));
    assert!(rendered.contains("status: searched"));
    assert!(rendered.contains("lines: 2"));
    assert!(rendered.contains("queue_len: 7"));
    assert!(rendered.contains("coverage_probability: 100%"));
    assert!(!rendered.contains("executor_flow"));
    assert!(!rendered.contains("compact_problem_descriptor"));
    assert!(!rendered.contains("gpu_backend_scope"));
    assert!(!rendered.contains("hybrid_scheduler"));
    assert!(!rendered.contains("score_event_basis"));
    assert!(!rendered.contains("coverage_row_view"));
}

#[test]
fn probability_is_percent_only_at_the_human_text_boundary() {
    let message = RenderMessage::new("percent")
        .with_value("coverage_probability", RenderFieldValue::number("0.625"))
        .with_value("probability_complete", true);

    let text = RenderFormatDispatcher::render(&message, RenderFormat::Text);
    let json = RenderFormatDispatcher::render(&message, RenderFormat::Json);

    assert!(text.contains("coverage_probability: 62.5%"));
    assert!(text.contains("probability_complete: true"));
    assert!(json.contains("\"coverage_probability\":0.625"));
    assert!(message.fumen_pages()[0].contains("coverage_probability=0.625"));
}

#[test]
fn pc_text_verbose_contains_executor_flow() {
    let message = RenderMessage::new("pc")
        .with_value("lines", 2usize)
        .with_value("executor_flow", "rust-shell-to-core");

    let rendered = RenderFormatDispatcher::render(&message, RenderFormat::TextVerbose);

    assert!(rendered.contains("lines: 2"));
    assert!(rendered.contains("executor_flow: rust-shell-to-core"));
}

#[test]
fn pc_json_still_contains_full_contract() {
    let message = RenderMessage::new("pc")
        .with_value("lines", 2usize)
        .with_value("executor_flow", "rust-shell-to-core");

    let rendered = RenderFormatDispatcher::render(&message, RenderFormat::Json);

    assert!(rendered.contains("\"lines\":2"));
    assert!(rendered.contains("\"executor_flow\":\"rust-shell-to-core\""));
}

#[test]
fn dispatches_message_to_json_writer() {
    let message = RenderMessage::new("pc").with_value("lines", 2usize);

    let rendered = RenderFormatDispatcher::render(&message, RenderFormat::Json);
    assert!(rendered.contains("\"schema_version\":2"));
    assert!(rendered.contains("\"kind\":\"pc\""));
    assert!(rendered.contains("\"summary\":{\"lines\":2}"));
    assert!(rendered.contains("\"contract\":{\"command\""));
}

#[test]
fn json_exposes_solution_artifacts_only_when_the_host_requests_them() {
    let exposed = RenderMessage::new("pc")
        .with_value("solution_data_requested", true)
        .with_value(
            "regular",
            RenderFieldValue::array([RenderFieldValue::string("large-regular-payload")]),
        )
        .with_value(
            "mini",
            RenderFieldValue::array([RenderFieldValue::string("large-mini-payload")]),
        )
        .with_value(
            "solution_keys",
            RenderFieldValue::array([RenderFieldValue::string("ctk1|example")]),
        )
        .with_value(
            "solution_classes",
            RenderFieldValue::array([RenderFieldValue::string("regular")]),
        );
    let exposed = RenderFormatDispatcher::render(&exposed, RenderFormat::Json);
    assert!(exposed.contains("\"summary\":{}"));
    assert!(exposed.contains("\"artifacts\""));
    assert!(exposed.contains("\"schema_version\":\"clearra.solution-data.v1\""));
    assert!(exposed.contains("ctk1|example"));
    assert!(exposed.contains("\"solution_classes\":[\"regular\"]"));
    assert!(!exposed.contains("large-regular-payload"));
    assert!(!exposed.contains("large-mini-payload"));
}

#[test]
fn json_solution_data_contract_distinguishes_all_availability_states() {
    for (status, reason, artifacts_expected) in [
        ("not-requested", RenderFieldValue::Null, false),
        (
            "unavailable",
            RenderFieldValue::string("solution-set-not-materialized"),
            false,
        ),
        (
            "partial",
            RenderFieldValue::string("solution-set-incomplete"),
            true,
        ),
        ("complete", RenderFieldValue::Null, true),
    ] {
        let message = RenderMessage::new("pc")
            .with_value("solution_data_requested", status != "not-requested")
            .with_value("solution_data_status", status)
            .with_value("solution_data_reason", reason)
            .with_value(
                "solution_keys",
                RenderFieldValue::array([RenderFieldValue::string("ctk1|example")]),
            );

        let rendered = RenderFormatDispatcher::render(&message, RenderFormat::Json);

        assert!(
            rendered.contains(&format!("\"status\":\"{status}\"")),
            "{rendered}"
        );
        assert_eq!(rendered.contains("\"artifacts\""), artifacts_expected);
        assert!(!rendered.contains("solution_data_status"));
        assert!(!rendered.contains("solution_data_requested"));
        assert!(!rendered.contains("solution_data_reason"));
    }
}

#[test]
fn dispatches_message_to_fumen_like_writer() {
    let message = RenderMessage::new("pc").with_field("lines", "2");

    assert_eq!(
        RenderFormatDispatcher::render(&message, RenderFormat::FumenLike),
        "v115@vhAAgWVArORoDlcRPEjI8vBsOprDz4K6BSAAAA"
    );
}

#[test]
fn dispatches_replay_trace_to_all_output_formats() {
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

    assert!(
        RenderFormatDispatcher::render_replay_trace(&trace, RenderFormat::Text)
            .contains("kind: replay-trace")
    );
    assert!(
        RenderFormatDispatcher::render_replay_trace(&trace, RenderFormat::Json)
            .contains("\"kind\":\"replay-trace\"")
    );
    assert!(
        RenderFormatDispatcher::render_replay_trace(&trace, RenderFormat::FumenLike)
            .starts_with("v115@")
    );
}

#[test]
fn png_gif_exact_output_renders_replay_trace() {
    let golden = include_str!("../../../tests/golden/render/render_exact_output_connected.json");
    let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
    let input = BuildVariantReplayInput::new(
        "bitmap-output",
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

    for format in [ExactBitmapOutputFormat::Png, ExactBitmapOutputFormat::Gif] {
        let rendered = RenderExactOutputGate::render_replay_trace(&trace, format)
            .expect("exact bitmap output");

        assert_eq!(rendered.format(), format);
        assert!(rendered.render_exact());
        assert_eq!(rendered.skin_id(), "default");
        match format {
            ExactBitmapOutputFormat::Png => assert!(rendered.bytes().starts_with(b"\x89PNG")),
            ExactBitmapOutputFormat::Gif => assert!(rendered.bytes().starts_with(b"GIF89a")),
        }
        assert!(golden.contains(&format!("\"format\": \"{}\"", format.as_str())));
    }
    assert!(golden.contains("\"render_exact\": true"));
    assert!(golden.contains("\"supported\": true"));
}

#[test]
fn renderer_reports_export_limits() {
    let limits = RenderExactOutputGate::bitmap_export_limits();

    assert_eq!(limits.max_frame_width(), 1920);
    assert_eq!(limits.max_frame_height(), 1080);
    assert_eq!(limits.max_gif_frames(), 240);
    assert_eq!(limits.max_frame_delay_ms(), 5000);
    assert_eq!(limits.renderer(), "clearra-render-exact-bitmap");
}
