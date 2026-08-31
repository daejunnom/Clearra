use std::io::Cursor;

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_replay::{BuildVariantOperation, BuildVariantReplayInput, ReplayEngine};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::*;
use crate::export::render_allocation_authority::{allocation_attempts, reset_allocation_attempts};

fn golden() -> Value {
    serde_json::from_str(include_str!(
        "../../../../tests/golden/render/render_exact_output_connected.json"
    ))
    .expect("render golden")
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn png_board_render_golden() {
    let board = RenderBoard::from_rows(&["..I.", ".OIT", "SSZZ"]).expect("board");
    let png =
        ExactBitmapRenderer::render_board_png(&board, 16, RenderExportLimits::product_default())
            .expect("png");

    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(
        sha256(&png),
        golden()["png_board_sha256"].as_str().expect("png hash")
    );
    assert_eq!(
        png,
        ExactBitmapRenderer::render_board_png(&board, 16, RenderExportLimits::product_default())
            .expect("png repeat")
    );
}

#[test]
fn png_lock_frame_render_golden() {
    let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
    let input = BuildVariantReplayInput::new(
        "owner-lock-frame",
        layout,
        0x03fc,
        vec![BuildVariantOperation::new(
            PieceKind::O,
            RotationState::Zero,
            0,
            0,
        )],
    );
    let trace = ReplayEngine::build_variant_to_trace(&input).expect("trace");
    let png = ExactBitmapRenderer::render_replay_lock_png(
        &trace,
        16,
        RenderExportLimits::product_default(),
    )
    .expect("lock png");
    let (width, rgba) = decode_rgba(&png);

    assert_eq!(
        sha256(&png),
        golden()["png_lock_frame_sha256"]
            .as_str()
            .expect("lock hash")
    );
    assert_eq!(pixel(&rgba, width, 8, 8), [243, 211, 71, 255]);
    assert_eq!(
        pixel(&rgba, width, 2 * 16 + 8, 16 + 8),
        [118, 126, 140, 255]
    );
}

#[test]
fn png_after_clear_and_minos_crop_render_golden() {
    let board = RenderBoard::from_rows(&["....", ".GG.", ".TI."]).expect("board");
    let after_clear = ExactBitmapRenderer::render_after_clear_png(
        &board,
        16,
        RenderExportLimits::product_default(),
    )
    .expect("after clear png");
    let cropped = ExactBitmapRenderer::render_minos_crop_png(
        &board,
        16,
        RenderExportLimits::product_default(),
    )
    .expect("crop png");

    assert!(after_clear.starts_with(b"\x89PNG"));
    assert!(cropped.starts_with(b"\x89PNG"));
    assert_ne!(after_clear, cropped);
}

#[test]
fn gif_timeline_render_golden() {
    let frames = vec![
        RenderBoard::from_rows(&["I.", "I."]).expect("frame 1"),
        RenderBoard::from_rows(&[".I", ".I"]).expect("frame 2"),
    ];
    let gif = ExactBitmapRenderer::render_timeline_gif(
        &frames,
        16,
        40,
        RenderExportLimits::product_default(),
    )
    .expect("gif");

    assert!(gif.starts_with(b"GIF89a"));
    assert_eq!(gif.last(), Some(&0x3b));
    assert!(gif.windows(11).any(|chunk| chunk == b"NETSCAPE2.0"));
    assert_eq!(
        sha256(&gif),
        golden()["gif_timeline_sha256"].as_str().expect("gif hash")
    );
}

#[test]
fn connected_board_joins_same_group_and_preserves_the_empty_grid() {
    let cells = [
        RenderCell::Empty,
        RenderCell::T,
        RenderCell::Empty,
        RenderCell::Empty,
        RenderCell::T,
        RenderCell::T,
        RenderCell::T,
        RenderCell::Empty,
    ];
    let board = RenderBoard::from_cells_with_connection_groups(4, 2, &cells, &[0; 8])
        .expect("connected board");
    let png = ExactBitmapRenderer::render_connected_board_png(
        &board,
        8,
        RenderExportLimits::product_default(),
    )
    .expect("connected png");
    let (width, rgba) = decode_rgba(&png);

    let t = [182, 106, 208, 255];
    assert_eq!(pixel(&rgba, width, 7, 12), t);
    assert_eq!(pixel(&rgba, width, 8, 12), t);
    assert_eq!(pixel(&rgba, width, 12, 7), t);
    assert_eq!(pixel(&rgba, width, 12, 8), t);
    assert_eq!(pixel(&rgba, width, 12, 0), [103, 116, 111, 255]);
    assert_eq!(pixel(&rgba, width, 0, 12), [103, 116, 111, 255]);
    assert_eq!(pixel(&rgba, width, 23, 12), [38, 50, 46, 255]);
    assert_eq!(pixel(&rgba, width, 12, 15), [38, 50, 46, 255]);
    assert_eq!(pixel(&rgba, width, 28, 12), [30, 41, 39, 255]);
    assert_eq!(pixel(&rgba, width, 24, 8), [63, 74, 72, 255]);
}

#[test]
fn connected_board_keeps_a_boundary_between_different_owner_groups() {
    let board = RenderBoard::from_cells_with_connection_groups(
        2,
        1,
        &[RenderCell::T, RenderCell::T],
        &[0, 1],
    )
    .expect("owner-separated board");
    let png = ExactBitmapRenderer::render_connected_board_png(
        &board,
        8,
        RenderExportLimits::product_default(),
    )
    .expect("owner-separated png");
    let (width, rgba) = decode_rgba(&png);

    assert_eq!(pixel(&rgba, width, 7, 4), [38, 50, 46, 255]);
    assert_eq!(pixel(&rgba, width, 8, 4), [103, 116, 111, 255]);
}

#[test]
fn renderer_reports_export_limits() {
    let limits = RenderExportLimits::tight_for_tests();
    assert_eq!(limits.max_frame_width(), 64);
    assert_eq!(
        RenderExportLimits::product_default().max_materialization_bytes(),
        256 * 1024 * 1024
    );
    assert_eq!(
        limits.validate_frame(65, 1),
        Err(RenderError::ExportLimitExceeded {
            limit: "max_frame_width",
            actual: 65,
            max: 64,
        })
    );
    assert_eq!(
        limits.validate_timeline(9, 2, 2, 40),
        Err(RenderError::ExportLimitExceeded {
            limit: "max_gif_frames",
            actual: 9,
            max: 8,
        })
    );
}

#[test]
fn oversized_frame_dimension_is_rejected_before_rgba_allocation() {
    let board = RenderBoard::from_rows(&["."]).expect("board");
    reset_allocation_attempts();

    let error =
        ExactBitmapRenderer::render_board_png(&board, 65, RenderExportLimits::tight_for_tests());

    assert_eq!(
        error,
        Err(RenderError::ExportLimitExceeded {
            limit: "max_frame_width",
            actual: 65,
            max: 64,
        })
    );
    assert_eq!(allocation_attempts(), 0);
}

#[test]
fn oversized_frame_height_is_rejected_before_rgba_allocation() {
    let board = RenderBoard::from_rows(&[".", "."]).expect("board");
    reset_allocation_attempts();

    let error =
        ExactBitmapRenderer::render_board_png(&board, 33, RenderExportLimits::tight_for_tests());

    assert_eq!(
        error,
        Err(RenderError::ExportLimitExceeded {
            limit: "max_frame_height",
            actual: 66,
            max: 64,
        })
    );
    assert_eq!(allocation_attempts(), 0);
}

#[test]
fn oversized_frame_pixel_count_is_rejected_before_rgba_allocation() {
    let board = RenderBoard::from_rows(&["..", ".."]).expect("board");
    let limits = RenderExportLimits::tight_for_tests().with_max_frame_pixels_for_test(3);
    reset_allocation_attempts();

    let error = ExactBitmapRenderer::render_board_png(&board, 1, limits);

    assert_eq!(
        error,
        Err(RenderError::ExportLimitExceeded {
            limit: "max_frame_pixels",
            actual: 4,
            max: 3,
        })
    );
    assert_eq!(allocation_attempts(), 0);
}

#[test]
fn oversized_frame_memory_is_rejected_before_rgba_allocation() {
    let board = RenderBoard::from_rows(&["..", ".."]).expect("board");
    let limits = RenderExportLimits::tight_for_tests().with_max_materialization_bytes_for_test(15);
    reset_allocation_attempts();

    let error = ExactBitmapRenderer::render_board_png(&board, 1, limits);

    assert_eq!(
        error,
        Err(RenderError::ExportLimitExceeded {
            limit: "max_materialization_bytes",
            actual: 16,
            max: 15,
        })
    );
    assert_eq!(allocation_attempts(), 0);
}

#[test]
fn oversized_timeline_frame_count_is_rejected_before_rgba_allocation() {
    let board = RenderBoard::from_rows(&["."]).expect("board");
    let frames = vec![board; 9];
    reset_allocation_attempts();

    let error = ExactBitmapRenderer::render_timeline_gif(
        &frames,
        1,
        40,
        RenderExportLimits::tight_for_tests(),
    );

    assert_eq!(
        error,
        Err(RenderError::ExportLimitExceeded {
            limit: "max_gif_frames",
            actual: 9,
            max: 8,
        })
    );
    assert_eq!(allocation_attempts(), 0);
}

#[test]
fn oversized_timeline_memory_is_rejected_before_frame_carrier_or_rgba_allocation() {
    let board = RenderBoard::from_rows(&["."]).expect("board");
    let frames = vec![board; 240];
    let required_bytes = 240 * (529 * 529 * 4 + core::mem::size_of::<Vec<u8>>() as u64);
    let limits = RenderExportLimits::product_default();
    reset_allocation_attempts();

    let error = ExactBitmapRenderer::render_timeline_gif(&frames, 529, 40, limits);

    assert_eq!(
        error,
        Err(RenderError::ExportLimitExceeded {
            limit: "max_materialization_bytes",
            actual: required_bytes,
            max: 256 * 1024 * 1024,
        })
    );
    assert_eq!(allocation_attempts(), 0);
}

fn decode_rgba(png_bytes: &[u8]) -> (u32, Vec<u8>) {
    let mut decoder = png::Decoder::new(Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().expect("png header");
    let mut buffer = vec![0; reader.output_buffer_size().expect("buffer size")];
    let info = reader.next_frame(&mut buffer).expect("png frame");
    assert_eq!(info.color_type, png::ColorType::Rgba);
    (info.width, buffer[..info.buffer_size()].to_vec())
}

fn pixel(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let offset = usize::try_from((y * width + x) * 4).expect("pixel offset");
    rgba[offset..offset + 4].try_into().expect("rgba pixel")
}
