use core::fmt;

use clearra_core_domain::field::static_parity_observation::{
    StaticParityObservation, StaticParityObservationError,
};
use fumen::CellColor;

use super::{
    source_fumen_diagram::decode_document, SourceFumenDiagramError, FUMEN_MAX_INPUT_BYTES,
    FUMEN_MAX_PAGES,
};

/// A read-only parity projection of one real v115 Fumen page.
///
/// The page field is observed in the decoder's bottom-up row-major coordinate
/// system. The pending garbage row is retained as separate evidence and is not
/// folded into field parity before a `rise` transition actually applies it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActualFumenPageParityObservation {
    page_index: usize,
    field: StaticParityObservation,
    pending_garbage_occupied_cell_count: u16,
}

impl ActualFumenPageParityObservation {
    pub const fn page_index(&self) -> usize {
        self.page_index
    }

    pub const fn field(&self) -> &StaticParityObservation {
        &self.field
    }

    pub const fn pending_garbage_occupied_cell_count(&self) -> u16 {
        self.pending_garbage_occupied_cell_count
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActualFumenParityDocument {
    pages: Vec<ActualFumenPageParityObservation>,
}

impl ActualFumenParityDocument {
    pub fn decode(source: &str) -> Result<Self, ActualFumenParityDocumentError> {
        if source.len() > FUMEN_MAX_INPUT_BYTES {
            return Err(ActualFumenParityDocumentError::InputTooLarge {
                length: source.len(),
                maximum: FUMEN_MAX_INPUT_BYTES,
            });
        }
        let document = decode_document(source).map_err(ActualFumenParityDocumentError::Decode)?;
        if document.pages.is_empty() {
            return Err(ActualFumenParityDocumentError::EmptyDocument);
        }
        if document.pages.len() > FUMEN_MAX_PAGES {
            return Err(ActualFumenParityDocumentError::TooManyPages {
                length: document.pages.len(),
                maximum: FUMEN_MAX_PAGES,
            });
        }

        let mut pages = Vec::new();
        pages
            .try_reserve(document.pages.len())
            .map_err(|_| ActualFumenParityDocumentError::CapacityExceeded)?;
        for (page_index, page) in document.pages.into_iter().enumerate() {
            let height = page.field.len();
            let width = page.field.first().map_or(0, |row| row.len());
            if width == 0 || height == 0 || page.field.iter().any(|row| row.len() != width) {
                return Err(ActualFumenParityDocumentError::InvalidFieldShape { page_index });
            }
            let width = u16::try_from(width)
                .map_err(|_| ActualFumenParityDocumentError::DimensionTooLarge { page_index })?;
            let height = u16::try_from(height)
                .map_err(|_| ActualFumenParityDocumentError::DimensionTooLarge { page_index })?;
            let cell_count = usize::from(width)
                .checked_mul(usize::from(height))
                .ok_or(ActualFumenParityDocumentError::CapacityExceeded)?;
            let mut occupancy = Vec::new();
            occupancy
                .try_reserve_exact(cell_count)
                .map_err(|_| ActualFumenParityDocumentError::CapacityExceeded)?;
            occupancy.extend(
                page.field
                    .iter()
                    .flat_map(|row| row.iter())
                    .map(|cell| *cell != CellColor::Empty),
            );
            let field =
                StaticParityObservation::from_row_major_occupancy(width, height, &occupancy)
                    .map_err(|source| ActualFumenParityDocumentError::Parity {
                        page_index,
                        source,
                    })?;
            let pending_garbage_occupied_cell_count = u16::try_from(
                page.garbage_row
                    .iter()
                    .filter(|cell| **cell != CellColor::Empty)
                    .count(),
            )
            .map_err(|_| ActualFumenParityDocumentError::DimensionTooLarge { page_index })?;
            pages.push(ActualFumenPageParityObservation {
                page_index,
                field,
                pending_garbage_occupied_cell_count,
            });
        }
        Ok(Self { pages })
    }

    pub fn pages(&self) -> &[ActualFumenPageParityObservation] {
        &self.pages
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActualFumenParityDocumentError {
    InputTooLarge {
        length: usize,
        maximum: usize,
    },
    Decode(SourceFumenDiagramError),
    EmptyDocument,
    TooManyPages {
        length: usize,
        maximum: usize,
    },
    InvalidFieldShape {
        page_index: usize,
    },
    DimensionTooLarge {
        page_index: usize,
    },
    CapacityExceeded,
    Parity {
        page_index: usize,
        source: StaticParityObservationError,
    },
}

impl fmt::Display for ActualFumenParityDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ActualFumenParityDocumentError {}

#[cfg(test)]
mod tests {
    use fumen::{CellColor, Fumen, Page};

    use super::ActualFumenParityDocument;

    #[test]
    fn observes_real_pages_without_promoting_pending_garbage_to_field_parity() {
        let mut first = Page::default();
        first.field[0][0] = CellColor::T;
        first.field[0][1] = CellColor::I;
        first.field[1][0] = CellColor::J;
        first.garbage_row[0] = CellColor::Grey;
        let mut second = Page::default();
        second.field[2][2] = CellColor::O;
        let source = Fumen {
            pages: vec![first, second],
            guideline: true,
        }
        .encode();

        let observed = ActualFumenParityDocument::decode(&source).expect("parity document");

        assert_eq!(observed.pages().len(), 2);
        let first = &observed.pages()[0];
        assert_eq!(first.page_index(), 0);
        assert_eq!(first.field().width(), 10);
        assert_eq!(first.field().occupied_cell_count(), 3);
        assert_eq!(first.field().checker_black_count(), 1);
        assert_eq!(first.field().checker_white_count(), 2);
        assert_eq!(first.pending_garbage_occupied_cell_count(), 1);
        assert!(!first.field().feasibility_claim());
        assert_eq!(first.field().pruning_authority(), "none");
        assert_eq!(observed.pages()[1].field().occupied_cell_count(), 1);
    }
}
