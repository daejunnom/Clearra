use super::*;
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_fumen::ActualFumenDocumentTransform;
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_replay::{BuildVariantOperation, BuildVariantReplayInput, ReplayEngine};

use crate::{
    encode_ctk3_compact, model::RenderFieldValue, Ctk3Color, Ctk3Document, Ctk3Operation, Ctk3Page,
    Ctk3Piece, Ctk3Rotation,
};

#[test]
fn dispatches_message_to_text_writer() {
    let message = RenderMessage::new("pc").with_field("lines", "2");

    assert_eq!(
        RenderFormatDispatcher::render(&message, RenderFormat::Text),
        Ok("kind: pc\nlines: 2".to_owned())
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

    let rendered = RenderFormatDispatcher::render(&message, RenderFormat::Text).expect("text");

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

    let text = RenderFormatDispatcher::render(&message, RenderFormat::Text).expect("text");
    let json = RenderFormatDispatcher::render(&message, RenderFormat::Json).expect("json");

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

    let rendered =
        RenderFormatDispatcher::render(&message, RenderFormat::TextVerbose).expect("text");

    assert!(rendered.contains("lines: 2"));
    assert!(rendered.contains("executor_flow: rust-shell-to-core"));
}

#[test]
fn pc_json_still_contains_full_contract() {
    let message = RenderMessage::new("pc")
        .with_value("lines", 2usize)
        .with_value("executor_flow", "rust-shell-to-core");

    let rendered = RenderFormatDispatcher::render(&message, RenderFormat::Json).expect("json");

    assert!(rendered.contains("\"lines\":2"));
    assert!(rendered.contains("\"executor_flow\":\"rust-shell-to-core\""));
}

#[test]
fn dispatches_message_to_json_writer() {
    let message = RenderMessage::new("pc").with_value("lines", 2usize);

    let rendered = RenderFormatDispatcher::render(&message, RenderFormat::Json).expect("json");
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
    let exposed = RenderFormatDispatcher::render(&exposed, RenderFormat::Json).expect("json");
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

        let rendered = RenderFormatDispatcher::render(&message, RenderFormat::Json).expect("json");

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
        Ok("v115@vhAAgWVArORoDlcRPEjI8vBsOprDz4K6BSAAAA".to_owned())
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
            .expect("text replay")
            .contains("kind: replay-trace")
    );
    assert!(
        RenderFormatDispatcher::render_replay_trace(&trace, RenderFormat::Json)
            .expect("json replay")
            .contains("\"kind\":\"replay-trace\"")
    );
    assert!(
        RenderFormatDispatcher::render_replay_trace(&trace, RenderFormat::FumenLike)
            .expect("fumen replay")
            .starts_with("v115@")
    );
}

#[test]
fn oversized_fumen_output_fails_closed_without_panicking() {
    let message = RenderMessage::new("x".repeat(4096));

    assert!(matches!(
        RenderFormatDispatcher::render(&message, RenderFormat::FumenLike),
        Err(clearra_fumen::codec::FumenLikeWriteError::CommentTooLong { .. })
    ));
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

#[test]
fn typed_ctk3_png_uses_one_based_pages_and_only_draws_occupied_garbage_rows() {
    let mut first = Ctk3Page::new(1, vec![Ctk3Color::Piece(Ctk3Piece::T), Ctk3Color::Empty]);
    first.garbage = Some(vec![Ctk3Color::Gray, Ctk3Color::Empty]);
    let second = Ctk3Page::new(1, vec![Ctk3Color::Empty, Ctk3Color::Piece(Ctk3Piece::I)]);
    let source =
        encode_ctk3_compact(&Ctk3Document::new(2, vec![first, second])).expect("ctk3 document");

    let rendered = RenderExactOutputGate::render_field_document(
        &source,
        ExactFieldDocumentFormat::Ctk3,
        ExactBitmapOutputFormat::Png,
        Some(2),
    )
    .expect("selected page PNG");

    assert!(rendered.bytes().starts_with(b"\x89PNG"));
    assert_eq!(
        u32::from_be_bytes(rendered.bytes()[16..20].try_into().unwrap()),
        32
    );
    assert_eq!(
        u32::from_be_bytes(rendered.bytes()[20..24].try_into().unwrap()),
        64,
        "an empty pending-garbage row must not create a blank rendered line"
    );
    let first_page = RenderExactOutputGate::render_field_document(
        &source,
        ExactFieldDocumentFormat::Ctk3,
        ExactBitmapOutputFormat::Png,
        Some(1),
    )
    .expect("first page PNG");
    assert_eq!(
        u32::from_be_bytes(first_page.bytes()[20..24].try_into().unwrap()),
        80,
        "occupied pending garbage remains a distinct semantic row"
    );
    assert!(matches!(
        RenderExactOutputGate::render_field_document(
            &source,
            ExactFieldDocumentFormat::Ctk3,
            ExactBitmapOutputFormat::Png,
            Some(0),
        ),
        Err(FieldDocumentRenderError::PageNumberOutOfRange { .. })
    ));
}

#[test]
fn typed_ctk3_gif_uses_one_discord_speed_frame_per_page() {
    let first = Ctk3Page::new(1, vec![Ctk3Color::Piece(Ctk3Piece::T), Ctk3Color::Empty]);
    let second = Ctk3Page::new(1, vec![Ctk3Color::Empty, Ctk3Color::Piece(Ctk3Piece::I)]);
    let source =
        encode_ctk3_compact(&Ctk3Document::new(2, vec![first, second])).expect("ctk3 document");

    let rendered = RenderExactOutputGate::render_field_document(
        &source,
        ExactFieldDocumentFormat::Ctk3,
        ExactBitmapOutputFormat::Gif,
        None,
    )
    .expect("document GIF");

    assert_eq!(FIELD_DOCUMENT_GIF_FRAME_DELAY_MS, 500);
    assert_eq!(gif_graphic_control_delays(rendered.bytes()), vec![50, 50]);
}

#[test]
fn typed_ctk3_operation_uses_a_distinct_connected_region() {
    let mut page = Ctk3Page::new(
        2,
        vec![
            Ctk3Color::Empty,
            Ctk3Color::Empty,
            Ctk3Color::Empty,
            Ctk3Color::Empty,
            Ctk3Color::Empty,
            Ctk3Color::Empty,
            Ctk3Color::Piece(Ctk3Piece::T),
            Ctk3Color::Empty,
        ],
    );
    page.operation = Some(Ctk3Operation {
        piece: Ctk3Piece::T,
        rotation: Ctk3Rotation::Spawn,
        x: 1,
        y: 0,
    });
    let source = encode_ctk3_compact(&Ctk3Document::new(4, vec![page])).expect("ctk3");
    let pages = decode_render_pages(&source, ExactFieldDocumentFormat::Ctk3).expect("pages");
    let board = render_board(&pages[0], pages[0].height, false).expect("render board");

    assert_eq!(board.cell(1, 2), RenderCell::T);
    assert_eq!(board.connection_group(1, 2), 1);
    assert_eq!(board.cell(2, 2), RenderCell::T);
    assert_eq!(board.connection_group(2, 2), 0);
    assert_eq!(board.connection_group(1, 3), 1);
}

#[test]
fn typed_ctk3_operation_above_a_trimmed_field_expands_the_render_height() {
    let mut page = Ctk3Page::new(6, vec![Ctk3Color::Empty; 4 * 6]);
    page.operation = Some(Ctk3Operation {
        piece: Ctk3Piece::T,
        rotation: Ctk3Rotation::Spawn,
        x: 1,
        y: 4,
    });
    let source = encode_ctk3_compact(&Ctk3Document::new(4, vec![page])).expect("ctk3");
    let decoded = crate::decode_ctk3_exact(&source).expect("decoded ctk3");
    assert_eq!(
        decoded.pages[0].height, 0,
        "the codec trims the empty field"
    );
    let decoded_operation = decoded.pages[0].operation.expect("retained operation");
    assert_eq!(
        operation_cells(decoded_operation)
            .into_iter()
            .map(|(_, y)| y)
            .max(),
        Some(5),
        "the typed operation remains above the trimmed field"
    );

    let pages = decode_render_pages(&source, ExactFieldDocumentFormat::Ctk3).expect("pages");
    assert_eq!(pages[0].height, 6);
    assert_eq!(pages[0].cells_bottom_up.len(), 4 * 6);
    assert_eq!(pages[0].connection_groups_bottom_up[5 * 4 + 1], 1);
    let rendered = RenderExactOutputGate::render_field_document(
        &source,
        ExactFieldDocumentFormat::Ctk3,
        ExactBitmapOutputFormat::Png,
        Some(1),
    )
    .expect("operation PNG");
    assert_eq!(
        u32::from_be_bytes(rendered.bytes()[20..24].try_into().unwrap()),
        96,
        "an empty pending-garbage row must not extend the operation view"
    );
}

#[test]
fn typed_ctk3_operation_extent_is_capped_at_the_discord_row_limit() {
    let mut page = Ctk3Page::new(1, vec![Ctk3Color::Empty; 4]);
    page.operation = Some(Ctk3Operation {
        piece: Ctk3Piece::O,
        rotation: Ctk3Rotation::Spawn,
        x: 1,
        y: 30,
    });
    let source = encode_ctk3_compact(&Ctk3Document::new(4, vec![page])).expect("ctk3");
    let pages = decode_render_pages(&source, ExactFieldDocumentFormat::Ctk3).expect("pages");

    assert_eq!(pages[0].height, FIELD_DOCUMENT_MAX_VIEW_ROWS);
    assert_eq!(pages[0].cells_bottom_up.len(), 4 * 31);
    assert_eq!(pages[0].connection_groups_bottom_up[30 * 4 + 1], 1);
    assert_eq!(pages[0].connection_groups_bottom_up[30 * 4 + 2], 1);
}

#[test]
fn real_fumen_gif_renders_document_pages_without_accepting_a_page_selector() {
    let source =
        ActualFumenDocumentTransform::text_to_fumen(&["first".to_owned(), "second".to_owned()])
            .expect("real fumen");
    let rendered = RenderExactOutputGate::render_field_document(
        &source,
        ExactFieldDocumentFormat::Fumen,
        ExactBitmapOutputFormat::Gif,
        None,
    )
    .expect("document GIF");

    assert!(rendered.bytes().starts_with(b"GIF89a"));
    assert_eq!(
        u16::from_le_bytes([rendered.bytes()[6], rendered.bytes()[7]]),
        160
    );
    assert_eq!(
        u16::from_le_bytes([rendered.bytes()[8], rendered.bytes()[9]]),
        391,
        "23 field rows plus the Discord-compatible one-line comment panel"
    );
    assert!(rendered.bytes().len() <= PUBLIC_BITMAP_ARTIFACT_MAX_BYTES);
    assert_eq!(
        RenderExactOutputGate::render_field_document(
            &source,
            ExactFieldDocumentFormat::Fumen,
            ExactBitmapOutputFormat::Gif,
            Some(1),
        ),
        Err(FieldDocumentRenderError::PageNumberNotAllowedForGif)
    );
}

#[test]
fn typed_ctk3_gif_uses_one_fixed_comment_panel_for_mixed_comment_pages() {
    let commented =
        Ctk3Page::new(1, vec![Ctk3Color::Piece(Ctk3Piece::T), Ctk3Color::Empty]).with_comment("A");
    let empty = Ctk3Page::new(1, vec![Ctk3Color::Empty, Ctk3Color::Piece(Ctk3Piece::I)]);
    let mixed_source = encode_ctk3_compact(&Ctk3Document::new(
        2,
        vec![commented.clone(), empty.clone()],
    ))
    .expect("mixed comments");
    let mixed = RenderExactOutputGate::render_field_document(
        &mixed_source,
        ExactFieldDocumentFormat::Ctk3,
        ExactBitmapOutputFormat::Gif,
        None,
    )
    .expect("mixed comment GIF");
    assert_eq!(
        u16::from_le_bytes([mixed.bytes()[6], mixed.bytes()[7]]),
        80,
        "Discord comment panels keep a minimum 80px width"
    );
    assert_eq!(
        u16::from_le_bytes([mixed.bytes()[8], mixed.bytes()[9]]),
        87,
        "both frames share four board rows plus one fixed comment panel"
    );

    let empty_source = encode_ctk3_compact(&Ctk3Document::new(2, vec![empty.clone(), empty]))
        .expect("empty comments");
    let all_empty = RenderExactOutputGate::render_field_document(
        &empty_source,
        ExactFieldDocumentFormat::Ctk3,
        ExactBitmapOutputFormat::Gif,
        None,
    )
    .expect("board-only GIF");
    assert_eq!(
        u16::from_le_bytes([all_empty.bytes()[6], all_empty.bytes()[7]]),
        32
    );
    assert_eq!(
        u16::from_le_bytes([all_empty.bytes()[8], all_empty.bytes()[9]]),
        64,
        "all-empty comments add neither a panel nor a blank garbage line"
    );
}

#[test]
fn typed_ctk3_gif_uses_one_fixed_garbage_row_for_mixed_garbage_pages() {
    let mut occupied_garbage =
        Ctk3Page::new(1, vec![Ctk3Color::Piece(Ctk3Piece::T), Ctk3Color::Empty]);
    occupied_garbage.garbage = Some(vec![Ctk3Color::Gray, Ctk3Color::Empty]);
    let empty_garbage = Ctk3Page::new(1, vec![Ctk3Color::Empty, Ctk3Color::Piece(Ctk3Piece::I)]);
    let source = encode_ctk3_compact(&Ctk3Document::new(2, vec![occupied_garbage, empty_garbage]))
        .expect("mixed garbage");
    let rendered = RenderExactOutputGate::render_field_document(
        &source,
        ExactFieldDocumentFormat::Ctk3,
        ExactBitmapOutputFormat::Gif,
        None,
    )
    .expect("mixed garbage GIF");

    assert_eq!(
        u16::from_le_bytes([rendered.bytes()[8], rendered.bytes()[9]]),
        80,
        "all frames share four field rows and the one document-level garbage row"
    );
    assert_eq!(gif_graphic_control_delays(rendered.bytes()), vec![50, 50]);
}

#[test]
fn typed_ctk3_png_rasterizes_hangul_comment_below_board_without_a_blank_garbage_row() {
    let page = Ctk3Page::new(1, vec![Ctk3Color::Piece(Ctk3Piece::T), Ctk3Color::Empty])
        .with_comment("<b>@everyone</b> 한글 주석");
    let source = encode_ctk3_compact(&Ctk3Document::new(2, vec![page])).expect("ctk3");
    let rendered = RenderExactOutputGate::render_field_document(
        &source,
        ExactFieldDocumentFormat::Ctk3,
        ExactBitmapOutputFormat::Png,
        Some(1),
    )
    .expect("comment PNG");

    assert_eq!(
        u32::from_be_bytes(rendered.bytes()[20..24].try_into().unwrap()),
        115,
        "four 16px board rows plus the bounded three-line comment panel"
    );
    assert!(rendered.bytes().len() <= PUBLIC_BITMAP_ARTIFACT_MAX_BYTES);
}

fn gif_graphic_control_delays(bytes: &[u8]) -> Vec<u16> {
    bytes
        .windows(8)
        .filter(|window| window[..3] == [0x21, 0xf9, 0x04])
        .map(|window| u16::from_le_bytes([window[4], window[5]]))
        .collect()
}
