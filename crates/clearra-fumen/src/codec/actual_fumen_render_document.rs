use core::fmt;

use fumen::CellColor;

use super::{
    source_fumen_diagram::decode_document, SourceFumenDiagramError, FUMEN_MAX_INPUT_BYTES,
    FUMEN_MAX_PAGES,
};

/// Codec-neutral cell colors exposed to the product renderer.
///
/// The Fumen dependency and its concrete page representation remain private to
/// `clearra-fumen`; rendering consumers receive a bounded, lossless color
/// projection in the decoder's bottom-up row-major coordinate system.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActualFumenRenderColor {
    Empty,
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
    Garbage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActualFumenRenderPage {
    page_index: usize,
    width: usize,
    height: usize,
    cells_bottom_up: Vec<ActualFumenRenderColor>,
    pending_garbage: Vec<ActualFumenRenderColor>,
    comment: String,
}

impl ActualFumenRenderPage {
    pub const fn page_index(&self) -> usize {
        self.page_index
    }

    pub const fn width(&self) -> usize {
        self.width
    }

    pub const fn height(&self) -> usize {
        self.height
    }

    pub fn cells_bottom_up(&self) -> &[ActualFumenRenderColor] {
        &self.cells_bottom_up
    }

    /// Pending garbage remains a distinct row. It is never silently applied
    /// to the decoded field before a Fumen `rise` transition does so.
    pub fn pending_garbage(&self) -> &[ActualFumenRenderColor] {
        &self.pending_garbage
    }

    pub fn comment(&self) -> &str {
        &self.comment
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActualFumenRenderDocument {
    pages: Vec<ActualFumenRenderPage>,
}

impl ActualFumenRenderDocument {
    pub fn decode(source: &str) -> Result<Self, ActualFumenRenderDocumentError> {
        if source.len() > FUMEN_MAX_INPUT_BYTES {
            return Err(ActualFumenRenderDocumentError::InputTooLarge {
                length: source.len(),
                maximum: FUMEN_MAX_INPUT_BYTES,
            });
        }
        let document = decode_document(source).map_err(ActualFumenRenderDocumentError::Decode)?;
        if document.pages.is_empty() {
            return Err(ActualFumenRenderDocumentError::EmptyDocument);
        }
        if document.pages.len() > FUMEN_MAX_PAGES {
            return Err(ActualFumenRenderDocumentError::TooManyPages {
                length: document.pages.len(),
                maximum: FUMEN_MAX_PAGES,
            });
        }

        let mut pages = Vec::new();
        pages
            .try_reserve(document.pages.len())
            .map_err(|_| ActualFumenRenderDocumentError::CapacityExceeded)?;
        for (page_index, page) in document.pages.into_iter().enumerate() {
            let height = page.field.len();
            let width = page.field.first().map_or(0, |row| row.len());
            if width == 0 || height == 0 || page.field.iter().any(|row| row.len() != width) {
                return Err(ActualFumenRenderDocumentError::InvalidFieldShape { page_index });
            }
            if page.garbage_row.len() != width {
                return Err(ActualFumenRenderDocumentError::InvalidGarbageShape { page_index });
            }
            let cell_count = width
                .checked_mul(height)
                .ok_or(ActualFumenRenderDocumentError::CapacityExceeded)?;
            let mut cells_bottom_up = Vec::new();
            cells_bottom_up
                .try_reserve_exact(cell_count)
                .map_err(|_| ActualFumenRenderDocumentError::CapacityExceeded)?;
            cells_bottom_up.extend(
                page.field
                    .iter()
                    .flat_map(|row| row.iter().copied())
                    .map(render_color),
            );
            let mut pending_garbage = Vec::new();
            pending_garbage
                .try_reserve_exact(width)
                .map_err(|_| ActualFumenRenderDocumentError::CapacityExceeded)?;
            pending_garbage.extend(page.garbage_row.into_iter().map(render_color));
            pages.push(ActualFumenRenderPage {
                page_index,
                width,
                height,
                cells_bottom_up,
                pending_garbage,
                comment: page.comment.unwrap_or_default(),
            });
        }
        Ok(Self { pages })
    }

    pub fn pages(&self) -> &[ActualFumenRenderPage] {
        &self.pages
    }
}

const fn render_color(color: CellColor) -> ActualFumenRenderColor {
    match color {
        CellColor::Empty => ActualFumenRenderColor::Empty,
        CellColor::I => ActualFumenRenderColor::I,
        CellColor::O => ActualFumenRenderColor::O,
        CellColor::T => ActualFumenRenderColor::T,
        CellColor::S => ActualFumenRenderColor::S,
        CellColor::Z => ActualFumenRenderColor::Z,
        CellColor::J => ActualFumenRenderColor::J,
        CellColor::L => ActualFumenRenderColor::L,
        CellColor::Grey => ActualFumenRenderColor::Garbage,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActualFumenRenderDocumentError {
    InputTooLarge { length: usize, maximum: usize },
    Decode(SourceFumenDiagramError),
    EmptyDocument,
    TooManyPages { length: usize, maximum: usize },
    InvalidFieldShape { page_index: usize },
    InvalidGarbageShape { page_index: usize },
    CapacityExceeded,
}

impl fmt::Display for ActualFumenRenderDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ActualFumenRenderDocumentError {}

#[cfg(test)]
mod tests {
    use fumen::{CellColor, Fumen, Page};

    use super::{ActualFumenRenderColor, ActualFumenRenderDocument};

    #[test]
    fn retains_bottom_up_colors_and_pending_garbage_as_separate_rows() {
        let mut page = Page::default();
        page.field[0][0] = CellColor::T;
        page.field[22][9] = CellColor::I;
        page.garbage_row[1] = CellColor::Grey;
        page.comment = Some("주석 😀".to_owned());
        let source = Fumen {
            pages: vec![page],
            guideline: true,
        }
        .encode();

        let document = ActualFumenRenderDocument::decode(&source).expect("render document");
        let page = &document.pages()[0];
        assert_eq!((page.width(), page.height()), (10, 23));
        assert_eq!(page.cells_bottom_up()[0], ActualFumenRenderColor::T);
        assert_eq!(
            page.cells_bottom_up()[22 * 10 + 9],
            ActualFumenRenderColor::I
        );
        assert_eq!(page.pending_garbage()[1], ActualFumenRenderColor::Garbage);
        assert_eq!(page.comment(), "주석 😀");
    }
}
