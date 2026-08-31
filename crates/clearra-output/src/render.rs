use core::fmt;

use clearra_ctk3::operation_cells;
use clearra_fumen::codec::{FumenLikeTrace, FumenLikeWriter};
use clearra_fumen::{
    ActualFumenRenderColor, ActualFumenRenderDocument, ActualFumenRenderDocumentError,
};
use clearra_render::{
    ExactBitmapRenderer, RenderBoard, RenderCapabilityReport, RenderCell, RenderError,
    RenderExportLimits,
};

use crate::{
    json::json_writer::JsonWriter,
    model::render_message::RenderMessage,
    text::{text_output_profile::TextOutputProfile, text_writer::TextWriter},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RenderFormat {
    #[default]
    Text,
    TextVerbose,
    TextDiagnostics,
    Json,
    FumenLike,
}

impl RenderFormat {
    pub fn with_text_profile(self, profile: TextOutputProfile) -> Self {
        match self {
            Self::Text | Self::TextVerbose | Self::TextDiagnostics => match profile {
                TextOutputProfile::HumanSummary => Self::Text,
                TextOutputProfile::Verbose => Self::TextVerbose,
                TextOutputProfile::Diagnostics => Self::TextDiagnostics,
            },
            Self::Json | Self::FumenLike => self,
        }
    }
}
impl RenderFormat {
    fn text_profile(self) -> Option<TextOutputProfile> {
        match self {
            Self::Text => Some(TextOutputProfile::HumanSummary),
            Self::TextVerbose => Some(TextOutputProfile::Verbose),
            Self::TextDiagnostics => Some(TextOutputProfile::Diagnostics),
            Self::Json | Self::FumenLike => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactBitmapOutputFormat {
    Png,
    Gif,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactFieldDocumentFormat {
    Ctk3,
    Fumen,
}

impl ExactFieldDocumentFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ctk3 => "ctk3",
            Self::Fumen => "fumen",
        }
    }
}

/// The Discord transport limit is a second mandatory product gate. A render
/// is public only when it satisfies both the renderer limits and this bound.
pub const PUBLIC_BITMAP_ARTIFACT_MAX_BYTES: usize = 8 * 1024 * 1024;
// The local CTK/Fumen result page follows the canonical Discord viewer pace.
const FIELD_DOCUMENT_GIF_FRAME_DELAY_MS: u16 = 500;
const FIELD_DOCUMENT_MIN_VIEW_ROWS: usize = 4;
const FIELD_DOCUMENT_MAX_VIEW_ROWS: usize = 31;

impl ExactBitmapOutputFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Gif => "gif",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactBitmapOutput {
    format: ExactBitmapOutputFormat,
    bytes: Vec<u8>,
    render_exact: bool,
    skin_id: &'static str,
}

impl ExactBitmapOutput {
    pub const fn format(&self) -> ExactBitmapOutputFormat {
        self.format
    }
}
impl ExactBitmapOutput {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
impl ExactBitmapOutput {
    pub const fn render_exact(&self) -> bool {
        self.render_exact
    }
}
impl ExactBitmapOutput {
    pub const fn skin_id(&self) -> &'static str {
        self.skin_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BitmapExportLimitReport {
    max_frame_width: u32,
    max_frame_height: u32,
    max_gif_frames: u32,
    max_frame_delay_ms: u16,
    renderer: &'static str,
}

impl BitmapExportLimitReport {
    pub const fn product_default() -> Self {
        Self {
            max_frame_width: 1920,
            max_frame_height: 1080,
            max_gif_frames: 240,
            max_frame_delay_ms: 5000,
            renderer: "clearra-render-exact-bitmap",
        }
    }
}
impl BitmapExportLimitReport {
    pub const fn max_frame_width(self) -> u32 {
        self.max_frame_width
    }
}
impl BitmapExportLimitReport {
    pub const fn max_frame_height(self) -> u32 {
        self.max_frame_height
    }
}
impl BitmapExportLimitReport {
    pub const fn max_gif_frames(self) -> u32 {
        self.max_gif_frames
    }
}
impl BitmapExportLimitReport {
    pub const fn max_frame_delay_ms(self) -> u16 {
        self.max_frame_delay_ms
    }
}
impl BitmapExportLimitReport {
    pub const fn renderer(self) -> &'static str {
        self.renderer
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RenderExactOutputGate;

impl RenderExactOutputGate {
    pub fn render_replay_trace(
        trace: &clearra_replay::ReplayTrace,
        format: ExactBitmapOutputFormat,
    ) -> Result<ExactBitmapOutput, clearra_render::RenderError> {
        let limits = RenderExportLimits::product_default();
        let bytes = match format {
            ExactBitmapOutputFormat::Png => {
                ExactBitmapRenderer::render_replay_png(trace, 16, limits)?
            }
            ExactBitmapOutputFormat::Gif => {
                ExactBitmapRenderer::render_replay_timeline_gif(trace, 16, 160, limits)?
            }
        };
        Ok(ExactBitmapOutput {
            format,
            bytes,
            render_exact: true,
            skin_id: "default",
        })
    }

    /// Renders the selected 1-based page as PNG or the document page order as
    /// a GIF. Document pages are observations; no operation replay frames are
    /// synthesized. Occupied pending garbage is retained as a distinct bottom
    /// row; an all-empty pending row cannot create a blank rendered line.
    pub fn render_field_document(
        source: &str,
        document_format: ExactFieldDocumentFormat,
        format: ExactBitmapOutputFormat,
        page_number: Option<usize>,
    ) -> Result<ExactBitmapOutput, FieldDocumentRenderError> {
        let pages = decode_render_pages(source, document_format)?;
        let limits = RenderExportLimits::product_default();
        let bytes = match format {
            ExactBitmapOutputFormat::Png => {
                let page_number = page_number.unwrap_or(1);
                let page_index = page_number.checked_sub(1).ok_or(
                    FieldDocumentRenderError::PageNumberOutOfRange {
                        page_number,
                        page_count: pages.len(),
                    },
                )?;
                let page = pages.get(page_index).ok_or(
                    FieldDocumentRenderError::PageNumberOutOfRange {
                        page_number,
                        page_count: pages.len(),
                    },
                )?;
                let board = render_board(page, page.height, has_occupied_pending_garbage(page))?;
                ExactBitmapRenderer::render_connected_board_with_comment_png(
                    &board,
                    &page.comment,
                    16,
                    limits,
                )
                .map_err(FieldDocumentRenderError::Render)?
            }
            ExactBitmapOutputFormat::Gif => {
                if page_number.is_some() {
                    return Err(FieldDocumentRenderError::PageNumberNotAllowedForGif);
                }
                if pages.len() > limits.max_gif_frames() {
                    return Err(FieldDocumentRenderError::Render(
                        RenderError::ExportLimitExceeded {
                            limit: "max_gif_frames",
                            actual: u64::try_from(pages.len()).unwrap_or(u64::MAX),
                            max: u64::try_from(limits.max_gif_frames()).unwrap_or(u64::MAX),
                        },
                    ));
                }
                let max_height = pages
                    .iter()
                    .map(|page| page.height)
                    .max()
                    .ok_or(FieldDocumentRenderError::EmptyDocument)?;
                let include_pending_garbage = pages.iter().any(has_occupied_pending_garbage);
                let mut frames = Vec::new();
                frames
                    .try_reserve(pages.len())
                    .map_err(|_| FieldDocumentRenderError::CapacityExceeded)?;
                for page in &pages {
                    frames.push(render_board(page, max_height, include_pending_garbage)?);
                }
                let comments = pages
                    .iter()
                    .map(|page| page.comment.clone())
                    .collect::<Vec<_>>();
                ExactBitmapRenderer::render_connected_timeline_with_comments_gif(
                    &frames,
                    &comments,
                    16,
                    FIELD_DOCUMENT_GIF_FRAME_DELAY_MS,
                    limits,
                )
                .map_err(FieldDocumentRenderError::Render)?
            }
        };
        if bytes.len() > PUBLIC_BITMAP_ARTIFACT_MAX_BYTES {
            return Err(FieldDocumentRenderError::ArtifactTooLarge {
                length: bytes.len(),
                maximum: PUBLIC_BITMAP_ARTIFACT_MAX_BYTES,
            });
        }
        Ok(ExactBitmapOutput {
            format,
            bytes,
            render_exact: true,
            skin_id: "default",
        })
    }

    pub fn capability_report() -> RenderCapabilityReport {
        RenderCapabilityReport::current()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TypedRenderPage {
    width: usize,
    height: usize,
    cells_bottom_up: Vec<RenderCell>,
    connection_groups_bottom_up: Vec<u32>,
    pending_garbage: Vec<RenderCell>,
    comment: String,
}

fn decode_render_pages(
    source: &str,
    format: ExactFieldDocumentFormat,
) -> Result<Vec<TypedRenderPage>, FieldDocumentRenderError> {
    match format {
        ExactFieldDocumentFormat::Ctk3 => {
            let document =
                crate::decode_ctk3_exact(source).map_err(FieldDocumentRenderError::Ctk3)?;
            if document.pages.is_empty() {
                return Err(FieldDocumentRenderError::EmptyDocument);
            }
            let mut pages = Vec::new();
            pages
                .try_reserve(document.pages.len())
                .map_err(|_| FieldDocumentRenderError::CapacityExceeded)?;
            for (page_index, page) in document.pages.into_iter().enumerate() {
                if page.cells.len()
                    != document
                        .width
                        .checked_mul(page.height)
                        .ok_or(FieldDocumentRenderError::CapacityExceeded)?
                {
                    return Err(FieldDocumentRenderError::InvalidPageShape { page_index });
                }
                let operation = page.operation;
                let occupied = operation.map(operation_cells);
                let mut render_height = page.height.max(FIELD_DOCUMENT_MIN_VIEW_ROWS);
                if let Some(cells) = occupied {
                    for (_, y) in cells {
                        let Ok(y) = usize::try_from(y) else {
                            continue;
                        };
                        render_height = render_height.max(y.saturating_add(1));
                    }
                }
                render_height = render_height.min(FIELD_DOCUMENT_MAX_VIEW_ROWS);
                let render_cell_count = document
                    .width
                    .checked_mul(render_height)
                    .ok_or(FieldDocumentRenderError::CapacityExceeded)?;
                let pending_garbage = match page.garbage {
                    Some(row) if row.len() == document.width => {
                        row.into_iter().map(ctk3_render_cell).collect()
                    }
                    Some(_) => {
                        return Err(FieldDocumentRenderError::InvalidGarbageShape { page_index })
                    }
                    None => vec![RenderCell::Empty; document.width],
                };
                let mut cells_bottom_up = Vec::new();
                cells_bottom_up
                    .try_reserve_exact(render_cell_count)
                    .map_err(|_| FieldDocumentRenderError::CapacityExceeded)?;
                cells_bottom_up.resize(render_cell_count, RenderCell::Empty);
                for (destination, color) in cells_bottom_up.iter_mut().zip(page.cells) {
                    *destination = ctk3_render_cell(color);
                }
                let mut connection_groups_bottom_up = Vec::new();
                connection_groups_bottom_up
                    .try_reserve_exact(render_cell_count)
                    .map_err(|_| FieldDocumentRenderError::CapacityExceeded)?;
                connection_groups_bottom_up.resize(render_cell_count, 0);
                if let (Some(operation), Some(cells)) = (operation, occupied) {
                    for (x, y) in cells {
                        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
                            continue;
                        };
                        if x >= document.width || y >= render_height {
                            continue;
                        }
                        let index = y * document.width + x;
                        cells_bottom_up[index] = ctk3_piece_render_cell(operation.piece);
                        connection_groups_bottom_up[index] = 1;
                    }
                }
                pages.push(TypedRenderPage {
                    width: document.width,
                    height: render_height,
                    cells_bottom_up,
                    connection_groups_bottom_up,
                    pending_garbage,
                    comment: page.comment,
                });
            }
            Ok(pages)
        }
        ExactFieldDocumentFormat::Fumen => {
            let document = ActualFumenRenderDocument::decode(source)
                .map_err(FieldDocumentRenderError::Fumen)?;
            let mut pages = Vec::new();
            pages
                .try_reserve(document.pages().len())
                .map_err(|_| FieldDocumentRenderError::CapacityExceeded)?;
            for page in document.pages() {
                pages.push(TypedRenderPage {
                    width: page.width(),
                    height: page.height(),
                    cells_bottom_up: page
                        .cells_bottom_up()
                        .iter()
                        .copied()
                        .map(fumen_render_cell)
                        .collect(),
                    connection_groups_bottom_up: vec![0; page.width() * page.height()],
                    pending_garbage: page
                        .pending_garbage()
                        .iter()
                        .copied()
                        .map(fumen_render_cell)
                        .collect(),
                    comment: page.comment().to_owned(),
                });
            }
            Ok(pages)
        }
    }
}

fn render_board(
    page: &TypedRenderPage,
    target_field_height: usize,
    include_pending_garbage: bool,
) -> Result<RenderBoard, FieldDocumentRenderError> {
    if target_field_height < page.height || page.pending_garbage.len() != page.width {
        return Err(FieldDocumentRenderError::InvalidPageShape { page_index: 0 });
    }
    let board_height = target_field_height
        .checked_add(usize::from(include_pending_garbage))
        .ok_or(FieldDocumentRenderError::CapacityExceeded)?;
    let cell_count = page
        .width
        .checked_mul(board_height)
        .ok_or(FieldDocumentRenderError::CapacityExceeded)?;
    let mut cells_top_down = Vec::new();
    cells_top_down
        .try_reserve_exact(cell_count)
        .map_err(|_| FieldDocumentRenderError::CapacityExceeded)?;
    cells_top_down.resize(cell_count, RenderCell::Empty);
    let mut connection_groups_top_down = Vec::new();
    connection_groups_top_down
        .try_reserve_exact(cell_count)
        .map_err(|_| FieldDocumentRenderError::CapacityExceeded)?;
    connection_groups_top_down.resize(cell_count, 0);
    for source_y in 0..page.height {
        let destination_y = target_field_height - 1 - source_y;
        let source_offset = source_y * page.width;
        let destination_offset = destination_y * page.width;
        cells_top_down[destination_offset..destination_offset + page.width]
            .copy_from_slice(&page.cells_bottom_up[source_offset..source_offset + page.width]);
        connection_groups_top_down[destination_offset..destination_offset + page.width]
            .copy_from_slice(
                &page.connection_groups_bottom_up[source_offset..source_offset + page.width],
            );
    }
    if include_pending_garbage {
        let garbage_offset = target_field_height * page.width;
        cells_top_down[garbage_offset..garbage_offset + page.width]
            .copy_from_slice(&page.pending_garbage);
    }
    RenderBoard::from_cells_with_connection_groups(
        page.width,
        board_height,
        &cells_top_down,
        &connection_groups_top_down,
    )
    .map_err(FieldDocumentRenderError::Render)
}

fn has_occupied_pending_garbage(page: &TypedRenderPage) -> bool {
    page.pending_garbage
        .iter()
        .any(|cell| *cell != RenderCell::Empty)
}

const fn ctk3_render_cell(color: crate::Ctk3Color) -> RenderCell {
    match color {
        crate::Ctk3Color::Empty => RenderCell::Empty,
        crate::Ctk3Color::Gray => RenderCell::Garbage,
        crate::Ctk3Color::Piece(crate::Ctk3Piece::I) => RenderCell::I,
        crate::Ctk3Color::Piece(crate::Ctk3Piece::O) => RenderCell::O,
        crate::Ctk3Color::Piece(crate::Ctk3Piece::T) => RenderCell::T,
        crate::Ctk3Color::Piece(crate::Ctk3Piece::S) => RenderCell::S,
        crate::Ctk3Color::Piece(crate::Ctk3Piece::Z) => RenderCell::Z,
        crate::Ctk3Color::Piece(crate::Ctk3Piece::J) => RenderCell::J,
        crate::Ctk3Color::Piece(crate::Ctk3Piece::L) => RenderCell::L,
    }
}

const fn ctk3_piece_render_cell(piece: crate::Ctk3Piece) -> RenderCell {
    ctk3_render_cell(crate::Ctk3Color::Piece(piece))
}

const fn fumen_render_cell(color: ActualFumenRenderColor) -> RenderCell {
    match color {
        ActualFumenRenderColor::Empty => RenderCell::Empty,
        ActualFumenRenderColor::I => RenderCell::I,
        ActualFumenRenderColor::O => RenderCell::O,
        ActualFumenRenderColor::T => RenderCell::T,
        ActualFumenRenderColor::S => RenderCell::S,
        ActualFumenRenderColor::Z => RenderCell::Z,
        ActualFumenRenderColor::J => RenderCell::J,
        ActualFumenRenderColor::L => RenderCell::L,
        ActualFumenRenderColor::Garbage => RenderCell::Garbage,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FieldDocumentRenderError {
    EmptyDocument,
    PageNumberOutOfRange {
        page_number: usize,
        page_count: usize,
    },
    PageNumberNotAllowedForGif,
    InvalidPageShape {
        page_index: usize,
    },
    InvalidGarbageShape {
        page_index: usize,
    },
    ArtifactTooLarge {
        length: usize,
        maximum: usize,
    },
    CapacityExceeded,
    Ctk3(crate::Ctk3CodecError),
    Fumen(ActualFumenRenderDocumentError),
    Render(RenderError),
}

impl FieldDocumentRenderError {
    pub const fn is_limit_exceeded(&self) -> bool {
        matches!(
            self,
            Self::ArtifactTooLarge { .. } | Self::Render(RenderError::ExportLimitExceeded { .. })
        )
    }
}

impl fmt::Display for FieldDocumentRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for FieldDocumentRenderError {}
impl RenderExactOutputGate {
    pub const fn bitmap_export_limits() -> BitmapExportLimitReport {
        BitmapExportLimitReport::product_default()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RenderFormatDispatcher;

impl RenderFormatDispatcher {
    pub fn render(
        message: &RenderMessage,
        format: RenderFormat,
    ) -> Result<String, clearra_fumen::codec::FumenLikeWriteError> {
        match format {
            RenderFormat::Text | RenderFormat::TextVerbose | RenderFormat::TextDiagnostics => {
                Ok(TextWriter::lines(&message.text_lines_with_profile(
                    format.text_profile().expect("text profile"),
                )))
            }
            RenderFormat::Json => Ok(JsonWriter::write(&message.json_contract())),
            RenderFormat::FumenLike => {
                FumenLikeWriter::write(&FumenLikeTrace::new(message.fumen_pages()))
            }
        }
    }
}
impl RenderFormatDispatcher {
    pub fn render_replay_trace(
        trace: &clearra_replay::ReplayTrace,
        format: RenderFormat,
    ) -> Result<String, clearra_fumen::codec::FumenLikeWriteError> {
        match format {
            RenderFormat::Text | RenderFormat::TextVerbose | RenderFormat::TextDiagnostics => {
                Ok(TextWriter::replay_trace(trace))
            }
            RenderFormat::Json => Ok(JsonWriter::write(
                &crate::json::JsonContract::from_replay_trace(trace),
            )),
            RenderFormat::FumenLike => FumenLikeWriter::write_replay_trace(trace),
        }
    }
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
