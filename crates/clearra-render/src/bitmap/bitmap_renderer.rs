use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_replay::ReplayTrace;

use crate::{
    bitmap::{
        gif_encoder::GifEncoder, png_encoder::PngEncoder, render_board::RenderBoard, RenderCell,
    },
    export::RenderExportLimits,
    RenderError, RenderScene, RenderSceneFrame, RenderTile, SkinAtlas,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExactBitmapRenderer;

impl ExactBitmapRenderer {
    pub fn render_board_png(
        board: &RenderBoard,
        cell_size: u32,
        limits: RenderExportLimits,
    ) -> Result<Vec<u8>, RenderError> {
        let atlas = SkinAtlas::builtin_default()?;
        let (width, height, rgba) = render_board_pixels(board, &atlas, cell_size)?;
        limits.validate_frame(width, height)?;
        PngEncoder::encode_rgba(width, height, &rgba)
    }

    pub fn render_minos_crop_png(
        board: &RenderBoard,
        cell_size: u32,
        limits: RenderExportLimits,
    ) -> Result<Vec<u8>, RenderError> {
        Self::render_board_png(&crop_board(board), cell_size, limits)
    }

    pub fn render_lock_frame_png(
        board: &RenderBoard,
        cell_size: u32,
        limits: RenderExportLimits,
    ) -> Result<Vec<u8>, RenderError> {
        Self::render_board_png(board, cell_size, limits)
    }

    pub fn render_after_clear_png(
        board: &RenderBoard,
        cell_size: u32,
        limits: RenderExportLimits,
    ) -> Result<Vec<u8>, RenderError> {
        Self::render_board_png(board, cell_size, limits)
    }

    pub fn render_timeline_gif(
        frames: &[RenderBoard],
        cell_size: u32,
        delay_ms: u16,
        limits: RenderExportLimits,
    ) -> Result<Vec<u8>, RenderError> {
        let first = frames.first().ok_or(RenderError::EmptyTimeline)?;
        let width = u32::try_from(first.width()).unwrap_or(u32::MAX) * cell_size;
        let height = u32::try_from(first.height()).unwrap_or(u32::MAX) * cell_size;
        limits.validate_timeline(frames.len(), width, height, delay_ms)?;
        let atlas = SkinAtlas::builtin_default()?;
        let rgba_frames = frames
            .iter()
            .map(|frame| {
                let (frame_width, frame_height, rgba) =
                    render_board_pixels(frame, &atlas, cell_size)?;
                if frame_width != width || frame_height != height {
                    return Err(RenderError::InvalidBoardRows);
                }
                Ok(rgba)
            })
            .collect::<Result<Vec<_>, _>>()?;
        GifEncoder::encode_rgba_frames(
            u16::try_from(width).map_err(|_| frame_dimension_error("gif_width", width))?,
            u16::try_from(height).map_err(|_| frame_dimension_error("gif_height", height))?,
            &rgba_frames,
            delay_ms,
        )
    }

    pub fn render_replay_png(
        trace: &ReplayTrace,
        cell_size: u32,
        limits: RenderExportLimits,
    ) -> Result<Vec<u8>, RenderError> {
        let scene = RenderScene::from_replay_trace(trace)?;
        let atlas = SkinAtlas::builtin_default()?;
        Self::render_scene_frame_png(scene.final_frame(), &atlas, cell_size, limits)
    }

    pub fn render_replay_lock_png(
        trace: &ReplayTrace,
        cell_size: u32,
        limits: RenderExportLimits,
    ) -> Result<Vec<u8>, RenderError> {
        let scene = RenderScene::from_replay_trace(trace)?;
        let frame = scene
            .frames()
            .iter()
            .rev()
            .find(|frame| frame.phase() == crate::RenderFramePhase::Lock)
            .ok_or(RenderError::EmptyTimeline)?;
        let atlas = SkinAtlas::builtin_default()?;
        Self::render_scene_frame_png(frame, &atlas, cell_size, limits)
    }

    pub fn render_replay_timeline_gif(
        trace: &ReplayTrace,
        cell_size: u32,
        delay_ms: u16,
        limits: RenderExportLimits,
    ) -> Result<Vec<u8>, RenderError> {
        let scene = RenderScene::from_replay_trace(trace)?;
        let atlas = SkinAtlas::builtin_default()?;
        let first = scene.frames().first().ok_or(RenderError::EmptyTimeline)?;
        let width = u32::from(first.width()) * cell_size;
        let height = u32::from(first.height()) * cell_size;
        limits.validate_timeline(scene.frames().len(), width, height, delay_ms)?;
        let frames = scene
            .frames()
            .iter()
            .map(|frame| {
                render_scene_frame_pixels(frame, &atlas, cell_size).map(|(_, _, rgba)| rgba)
            })
            .collect::<Result<Vec<_>, _>>()?;
        GifEncoder::encode_rgba_frames(
            u16::try_from(width).map_err(|_| frame_dimension_error("gif_width", width))?,
            u16::try_from(height).map_err(|_| frame_dimension_error("gif_height", height))?,
            &frames,
            delay_ms,
        )
    }

    pub fn render_scene_frame_png(
        frame: &RenderSceneFrame,
        atlas: &SkinAtlas,
        cell_size: u32,
        limits: RenderExportLimits,
    ) -> Result<Vec<u8>, RenderError> {
        let (width, height, rgba) = render_scene_frame_pixels(frame, atlas, cell_size)?;
        limits.validate_frame(width, height)?;
        PngEncoder::encode_rgba(width, height, &rgba)
    }
}

fn render_board_pixels(
    board: &RenderBoard,
    atlas: &SkinAtlas,
    cell_size: u32,
) -> Result<(u32, u32, Vec<u8>), RenderError> {
    if cell_size == 0 {
        return Err(frame_dimension_error("cell_size", 0));
    }
    let width = u32::try_from(board.width()).unwrap_or(u32::MAX) * cell_size;
    let height = u32::try_from(board.height()).unwrap_or(u32::MAX) * cell_size;
    let mut rgba = allocate_rgba(width, height)?;
    for y in 0..board.height() {
        for x in 0..board.width() {
            atlas.paint_tile(
                render_cell_tile(board.cell(x, y)),
                &mut rgba,
                width,
                u32::try_from(x).unwrap_or(u32::MAX) * cell_size,
                u32::try_from(y).unwrap_or(u32::MAX) * cell_size,
                cell_size,
            )?;
        }
    }
    Ok((width, height, rgba))
}

fn render_scene_frame_pixels(
    frame: &RenderSceneFrame,
    atlas: &SkinAtlas,
    cell_size: u32,
) -> Result<(u32, u32, Vec<u8>), RenderError> {
    if cell_size == 0 {
        return Err(frame_dimension_error("cell_size", 0));
    }
    let width = u32::from(frame.width()) * cell_size;
    let height = u32::from(frame.height()) * cell_size;
    let mut rgba = allocate_rgba(width, height)?;
    for y in 0..frame.height() {
        for x in 0..frame.width() {
            let tile = frame
                .tile_at_top_down(x, y)
                .ok_or(RenderError::ReplayLayoutMismatch)?;
            atlas.paint_tile(
                tile,
                &mut rgba,
                width,
                u32::from(x) * cell_size,
                u32::from(y) * cell_size,
                cell_size,
            )?;
        }
    }
    Ok((width, height, rgba))
}

fn allocate_rgba(width: u32, height: u32) -> Result<Vec<u8>, RenderError> {
    let length = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .ok_or_else(|| frame_dimension_error("rgba_bytes", u32::MAX))?;
    Ok(vec![0; length])
}

fn render_cell_tile(cell: RenderCell) -> RenderTile {
    match cell {
        RenderCell::Empty => RenderTile::Empty,
        RenderCell::I => RenderTile::Piece(PieceKind::I),
        RenderCell::O => RenderTile::Piece(PieceKind::O),
        RenderCell::T => RenderTile::Piece(PieceKind::T),
        RenderCell::S => RenderTile::Piece(PieceKind::S),
        RenderCell::Z => RenderTile::Piece(PieceKind::Z),
        RenderCell::J => RenderTile::Piece(PieceKind::J),
        RenderCell::L => RenderTile::Piece(PieceKind::L),
        RenderCell::Garbage => RenderTile::InitialGray,
    }
}

fn crop_board(board: &RenderBoard) -> RenderBoard {
    let Some((min_x, min_y, max_x, max_y)) = board.occupied_bounds() else {
        return board.clone();
    };
    let rows = (min_y..=max_y)
        .map(|y| {
            (min_x..=max_x)
                .map(|x| cell_char(board.cell(x, y)))
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    let borrowed = rows.iter().map(String::as_str).collect::<Vec<_>>();
    RenderBoard::from_rows(&borrowed).expect("cropped board is derived from a valid board")
}

fn cell_char(cell: RenderCell) -> char {
    match cell {
        RenderCell::Empty => '.',
        RenderCell::I => 'I',
        RenderCell::O => 'O',
        RenderCell::T => 'T',
        RenderCell::S => 'S',
        RenderCell::Z => 'Z',
        RenderCell::J => 'J',
        RenderCell::L => 'L',
        RenderCell::Garbage => 'G',
    }
}

fn frame_dimension_error(limit: &'static str, actual: u32) -> RenderError {
    RenderError::ExportLimitExceeded {
        limit,
        actual: u64::from(actual),
        max: u64::from(u16::MAX),
    }
}

#[cfg(test)]
#[path = "bitmap_renderer_tests.rs"]
mod tests;
