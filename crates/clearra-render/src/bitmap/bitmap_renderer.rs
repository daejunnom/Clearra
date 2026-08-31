use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_replay::ReplayTrace;

use crate::{
    bitmap::{
        gif_encoder::GifEncoder, png_encoder::PngEncoder, render_board::RenderBoard, RenderCell,
    },
    export::{render_allocation_authority::RenderAllocationAuthority, RenderExportLimits},
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
        let plan = board_frame_plan(board, cell_size, limits)?;
        let atlas = SkinAtlas::builtin_default()?;
        let mut authority = RenderAllocationAuthority::new(limits.max_materialization_bytes());
        let rgba = render_board_pixels(board, &atlas, cell_size, plan, &mut authority)?;
        PngEncoder::encode_rgba(plan.width, plan.height, &rgba)
    }

    /// Renders a document-style board with a persistent empty-cell grid and
    /// one outer bevel around each same-color, same-group occupied region.
    pub fn render_connected_board_png(
        board: &RenderBoard,
        cell_size: u32,
        limits: RenderExportLimits,
    ) -> Result<Vec<u8>, RenderError> {
        let plan = board_frame_plan(board, cell_size, limits)?;
        let mut authority = RenderAllocationAuthority::new(limits.max_materialization_bytes());
        let rgba = render_connected_board_pixels(board, cell_size, plan, &mut authority)?;
        PngEncoder::encode_rgba(plan.width, plan.height, &rgba)
    }

    pub fn render_minos_crop_png(
        board: &RenderBoard,
        cell_size: u32,
        limits: RenderExportLimits,
    ) -> Result<Vec<u8>, RenderError> {
        let (crop_width, crop_height) = cropped_board_dimensions(board);
        let _ = limits.validated_scaled_frame(crop_width, crop_height, cell_size)?;
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
        let first_plan = board_frame_plan(first, cell_size, limits)?;
        limits.validate_timeline(frames.len(), first_plan.width, first_plan.height, delay_ms)?;
        validate_board_timeline_shapes(frames, cell_size, limits, first_plan)?;

        let atlas = SkinAtlas::builtin_default()?;
        let mut authority = RenderAllocationAuthority::new(limits.max_materialization_bytes());
        let mut rgba_frames =
            authority.try_vec_with_capacity::<Vec<u8>>(frames.len(), "rgba_frame_carriers")?;
        for frame in frames {
            let plan = board_frame_plan(frame, cell_size, limits)?;
            rgba_frames.push(render_board_pixels(
                frame,
                &atlas,
                cell_size,
                plan,
                &mut authority,
            )?);
        }
        GifEncoder::encode_rgba_frames(
            u16::try_from(first_plan.width)
                .map_err(|_| frame_dimension_error("gif_width", first_plan.width))?,
            u16::try_from(first_plan.height)
                .map_err(|_| frame_dimension_error("gif_height", first_plan.height))?,
            &rgba_frames,
            delay_ms,
        )
    }

    pub fn render_connected_timeline_gif(
        frames: &[RenderBoard],
        cell_size: u32,
        delay_ms: u16,
        limits: RenderExportLimits,
    ) -> Result<Vec<u8>, RenderError> {
        let first = frames.first().ok_or(RenderError::EmptyTimeline)?;
        let first_plan = board_frame_plan(first, cell_size, limits)?;
        limits.validate_timeline(frames.len(), first_plan.width, first_plan.height, delay_ms)?;
        validate_board_timeline_shapes(frames, cell_size, limits, first_plan)?;

        let mut authority = RenderAllocationAuthority::new(limits.max_materialization_bytes());
        let mut rgba_frames =
            authority.try_vec_with_capacity::<Vec<u8>>(frames.len(), "rgba_frame_carriers")?;
        for frame in frames {
            let plan = board_frame_plan(frame, cell_size, limits)?;
            rgba_frames.push(render_connected_board_pixels(
                frame,
                cell_size,
                plan,
                &mut authority,
            )?);
        }
        GifEncoder::encode_rgba_frames(
            u16::try_from(first_plan.width)
                .map_err(|_| frame_dimension_error("gif_width", first_plan.width))?,
            u16::try_from(first_plan.height)
                .map_err(|_| frame_dimension_error("gif_height", first_plan.height))?,
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
        let first = scene.frames().first().ok_or(RenderError::EmptyTimeline)?;
        let first_plan = scene_frame_plan(first, cell_size, limits)?;
        limits.validate_timeline(
            scene.frames().len(),
            first_plan.width,
            first_plan.height,
            delay_ms,
        )?;
        validate_scene_timeline_shapes(scene.frames(), cell_size, limits, first_plan)?;

        let atlas = SkinAtlas::builtin_default()?;
        let mut authority = RenderAllocationAuthority::new(limits.max_materialization_bytes());
        let mut frames = authority
            .try_vec_with_capacity::<Vec<u8>>(scene.frames().len(), "rgba_frame_carriers")?;
        for frame in scene.frames() {
            let plan = scene_frame_plan(frame, cell_size, limits)?;
            frames.push(render_scene_frame_pixels(
                frame,
                &atlas,
                cell_size,
                plan,
                &mut authority,
            )?);
        }
        GifEncoder::encode_rgba_frames(
            u16::try_from(first_plan.width)
                .map_err(|_| frame_dimension_error("gif_width", first_plan.width))?,
            u16::try_from(first_plan.height)
                .map_err(|_| frame_dimension_error("gif_height", first_plan.height))?,
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
        let plan = scene_frame_plan(frame, cell_size, limits)?;
        let mut authority = RenderAllocationAuthority::new(limits.max_materialization_bytes());
        let rgba = render_scene_frame_pixels(frame, atlas, cell_size, plan, &mut authority)?;
        PngEncoder::encode_rgba(plan.width, plan.height, &rgba)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RgbaFramePlan {
    width: u32,
    height: u32,
    capacity: usize,
}

fn board_frame_plan(
    board: &RenderBoard,
    cell_size: u32,
    limits: RenderExportLimits,
) -> Result<RgbaFramePlan, RenderError> {
    frame_plan(board.width(), board.height(), cell_size, limits)
}

fn scene_frame_plan(
    frame: &RenderSceneFrame,
    cell_size: u32,
    limits: RenderExportLimits,
) -> Result<RgbaFramePlan, RenderError> {
    frame_plan(
        usize::from(frame.width()),
        usize::from(frame.height()),
        cell_size,
        limits,
    )
}

fn frame_plan(
    cell_width: usize,
    cell_height: usize,
    cell_size: u32,
    limits: RenderExportLimits,
) -> Result<RgbaFramePlan, RenderError> {
    let (width, height, capacity) =
        limits.validated_scaled_frame(cell_width, cell_height, cell_size)?;
    Ok(RgbaFramePlan {
        width,
        height,
        capacity,
    })
}

fn validate_board_timeline_shapes(
    frames: &[RenderBoard],
    cell_size: u32,
    limits: RenderExportLimits,
    expected: RgbaFramePlan,
) -> Result<(), RenderError> {
    for frame in frames {
        let actual = board_frame_plan(frame, cell_size, limits)?;
        if actual != expected {
            return Err(RenderError::InvalidBoardRows);
        }
    }
    Ok(())
}

fn validate_scene_timeline_shapes(
    frames: &[RenderSceneFrame],
    cell_size: u32,
    limits: RenderExportLimits,
    expected: RgbaFramePlan,
) -> Result<(), RenderError> {
    for frame in frames {
        let actual = scene_frame_plan(frame, cell_size, limits)?;
        if actual != expected {
            return Err(RenderError::ReplayLayoutMismatch);
        }
    }
    Ok(())
}

fn render_board_pixels(
    board: &RenderBoard,
    atlas: &SkinAtlas,
    cell_size: u32,
    plan: RgbaFramePlan,
    authority: &mut RenderAllocationAuthority,
) -> Result<Vec<u8>, RenderError> {
    let mut rgba = allocate_rgba(plan.capacity, authority)?;
    for y in 0..board.height() {
        for x in 0..board.width() {
            atlas.paint_tile(
                render_cell_tile(board.cell(x, y)),
                &mut rgba,
                plan.width,
                cell_origin(x, cell_size, "pixel_x")?,
                cell_origin(y, cell_size, "pixel_y")?,
                cell_size,
            )?;
        }
    }
    Ok(rgba)
}

fn render_connected_board_pixels(
    board: &RenderBoard,
    cell_size: u32,
    plan: RgbaFramePlan,
    authority: &mut RenderAllocationAuthority,
) -> Result<Vec<u8>, RenderError> {
    let mut rgba = allocate_rgba(plan.capacity, authority)?;
    for y in 0..board.height() {
        for x in 0..board.width() {
            paint_connected_cell(
                &mut rgba,
                plan.width,
                cell_origin(x, cell_size, "pixel_x")?,
                cell_origin(y, cell_size, "pixel_y")?,
                cell_size,
                render_cell_tile(board.cell(x, y)),
                connected_cell_edges(board, x, y),
            );
        }
    }
    Ok(rgba)
}

fn paint_connected_cell(
    output: &mut [u8],
    output_width: u32,
    destination_x: u32,
    destination_y: u32,
    cell_size: u32,
    tile: RenderTile,
    edges: [bool; 4],
) {
    // Canonical CTK document colors and bevels are shared with the Discord
    // viewer. Empty edges deliberately remain a subtle, one-pixel grid.
    const EMPTY: [u8; 4] = [30, 41, 39, 255];
    const GRID: [u8; 4] = [63, 74, 72, 255];
    const HIGHLIGHT: [u8; 4] = [103, 116, 111, 255];
    const SHADOW: [u8; 4] = [38, 50, 46, 255];

    let interior = connected_tile_color(tile);
    for local_y in 0..cell_size {
        for local_x in 0..cell_size {
            let color = if tile == RenderTile::Empty {
                if (local_y == 0 && edges[0])
                    || (local_x == 0 && edges[1])
                    || (local_y + 1 == cell_size && edges[2])
                    || (local_x + 1 == cell_size && edges[3])
                {
                    GRID
                } else {
                    EMPTY
                }
            } else if (local_x + 1 == cell_size && edges[3])
                || (local_y + 1 == cell_size && edges[2])
            {
                SHADOW
            } else if (local_x == 0 && edges[1]) || (local_y == 0 && edges[0]) {
                HIGHLIGHT
            } else {
                interior
            };
            let destination = usize::try_from(
                ((destination_y + local_y) * output_width + destination_x + local_x) * 4,
            )
            .expect("validated render dimensions fit usize");
            output[destination..destination + 4].copy_from_slice(&color);
        }
    }
}

const fn connected_tile_color(tile: RenderTile) -> [u8; 4] {
    match tile {
        RenderTile::Empty => [30, 41, 39, 255],
        RenderTile::InitialGray => [123, 133, 129, 255],
        RenderTile::Piece(PieceKind::I) => [85, 203, 211, 255],
        RenderTile::Piece(PieceKind::O) => [243, 207, 77, 255],
        RenderTile::Piece(PieceKind::T) => [182, 106, 208, 255],
        RenderTile::Piece(PieceKind::S) => [101, 199, 120, 255],
        RenderTile::Piece(PieceKind::Z) => [233, 110, 110, 255],
        RenderTile::Piece(PieceKind::J) => [98, 138, 224, 255],
        RenderTile::Piece(PieceKind::L) => [239, 156, 77, 255],
    }
}

fn connected_cell_edges(board: &RenderBoard, x: usize, y: usize) -> [bool; 4] {
    let top = y
        .checked_sub(1)
        .is_none_or(|neighbor_y| !same_connected_cell(board, x, y, x, neighbor_y));
    let left = x
        .checked_sub(1)
        .is_none_or(|neighbor_x| !same_connected_cell(board, x, y, neighbor_x, y));
    let bottom = y + 1 >= board.height() || !same_connected_cell(board, x, y, x, y + 1);
    let right = x + 1 >= board.width() || !same_connected_cell(board, x, y, x + 1, y);
    [top, left, bottom, right]
}

fn same_connected_cell(
    board: &RenderBoard,
    x: usize,
    y: usize,
    neighbor_x: usize,
    neighbor_y: usize,
) -> bool {
    let cell = board.cell(x, y);
    cell != RenderCell::Empty
        && board.cell(neighbor_x, neighbor_y) == cell
        && board.connection_group(neighbor_x, neighbor_y) == board.connection_group(x, y)
}

fn render_scene_frame_pixels(
    frame: &RenderSceneFrame,
    atlas: &SkinAtlas,
    cell_size: u32,
    plan: RgbaFramePlan,
    authority: &mut RenderAllocationAuthority,
) -> Result<Vec<u8>, RenderError> {
    let mut rgba = allocate_rgba(plan.capacity, authority)?;
    for y in 0..frame.height() {
        for x in 0..frame.width() {
            let tile = frame
                .tile_at_top_down(x, y)
                .ok_or(RenderError::ReplayLayoutMismatch)?;
            atlas.paint_tile(
                tile,
                &mut rgba,
                plan.width,
                u32::from(x)
                    .checked_mul(cell_size)
                    .ok_or_else(|| frame_dimension_error("pixel_x", u32::MAX))?,
                u32::from(y)
                    .checked_mul(cell_size)
                    .ok_or_else(|| frame_dimension_error("pixel_y", u32::MAX))?,
                cell_size,
            )?;
        }
    }
    Ok(rgba)
}

fn allocate_rgba(
    capacity: usize,
    authority: &mut RenderAllocationAuthority,
) -> Result<Vec<u8>, RenderError> {
    let mut rgba = authority.try_vec_with_capacity::<u8>(capacity, "rgba_frame")?;
    rgba.resize(capacity, 0);
    Ok(rgba)
}

fn cell_origin(index: usize, cell_size: u32, limit: &'static str) -> Result<u32, RenderError> {
    let actual = (index as u128)
        .checked_mul(u128::from(cell_size))
        .unwrap_or(u128::MAX);
    u32::try_from(actual).map_err(|_| RenderError::ExportLimitExceeded {
        limit,
        actual: u64::try_from(actual).unwrap_or(u64::MAX),
        max: u64::from(u32::MAX),
    })
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

fn cropped_board_dimensions(board: &RenderBoard) -> (usize, usize) {
    board
        .occupied_bounds()
        .map(|(min_x, min_y, max_x, max_y)| (max_x - min_x + 1, max_y - min_y + 1))
        .unwrap_or((board.width(), board.height()))
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
