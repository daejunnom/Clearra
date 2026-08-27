use core::fmt;

use clearra_core_domain::field::static_parity_observation::{
    StaticParityObservation, StaticParityObservationError, STATIC_PARITY_REPORT_CONTRACT,
};
use clearra_fumen::{ActualFumenParityDocument, ActualFumenParityDocumentError};
use clearra_output::{decode_ctk3_exact, Ctk3CodecError, Ctk3Color};

pub use crate::typed_document_utility::FieldDocumentFormat;

const FIELD_DOCUMENT_MAX_INPUT_BYTES: usize = 16 << 20;
const FIELD_DOCUMENT_MAX_PAGES: usize = 4_096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldDocumentParityPage {
    page_index: usize,
    field: StaticParityObservation,
    pending_garbage_occupied_cell_count: u16,
}

impl FieldDocumentParityPage {
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

/// Typed `parity-report.v1` projection over every page in one bounded field
/// document. It is representation-only: each page observation permanently
/// reports `feasibility_claim=false` and `pruning_authority=none`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldDocumentParityReport {
    document_format: FieldDocumentFormat,
    pages: Vec<FieldDocumentParityPage>,
}

impl FieldDocumentParityReport {
    pub fn observe(source: &str) -> Result<Self, FieldDocumentParityError> {
        if source.len() > FIELD_DOCUMENT_MAX_INPUT_BYTES {
            return Err(FieldDocumentParityError::InputTooLarge {
                length: source.len(),
                maximum: FIELD_DOCUMENT_MAX_INPUT_BYTES,
            });
        }
        let trimmed = source.trim_start();
        if trimmed.is_empty() {
            return Err(FieldDocumentParityError::EmptyDocument);
        }
        if trimmed.starts_with("ctk3_")
            || trimmed.starts_with("ctk3@")
            || trimmed.starts_with("ctk3b_")
        {
            Self::observe_ctk3(source)
        } else {
            Self::observe_fumen(source)
        }
    }

    pub fn observe_typed(
        format: FieldDocumentFormat,
        source: &str,
    ) -> Result<Self, FieldDocumentParityError> {
        if source.len() > FIELD_DOCUMENT_MAX_INPUT_BYTES {
            return Err(FieldDocumentParityError::InputTooLarge {
                length: source.len(),
                maximum: FIELD_DOCUMENT_MAX_INPUT_BYTES,
            });
        }
        match format {
            FieldDocumentFormat::Ctk3 => Self::observe_ctk3(source),
            FieldDocumentFormat::Fumen => Self::observe_fumen(source),
        }
    }

    fn observe_ctk3(source: &str) -> Result<Self, FieldDocumentParityError> {
        let document = decode_ctk3_exact(source).map_err(FieldDocumentParityError::Ctk3)?;
        if document.pages.is_empty() {
            return Err(FieldDocumentParityError::EmptyDocument);
        }
        if document.pages.len() > FIELD_DOCUMENT_MAX_PAGES {
            return Err(FieldDocumentParityError::TooManyPages {
                length: document.pages.len(),
                maximum: FIELD_DOCUMENT_MAX_PAGES,
            });
        }
        let width = u16::try_from(document.width)
            .map_err(|_| FieldDocumentParityError::DimensionTooLarge { page_index: 0 })?;
        let mut pages = Vec::new();
        pages
            .try_reserve(document.pages.len())
            .map_err(|_| FieldDocumentParityError::CapacityExceeded)?;
        for (page_index, page) in document.pages.into_iter().enumerate() {
            let height = u16::try_from(page.height)
                .map_err(|_| FieldDocumentParityError::DimensionTooLarge { page_index })?;
            let mut occupancy = Vec::new();
            occupancy
                .try_reserve_exact(page.cells.len())
                .map_err(|_| FieldDocumentParityError::CapacityExceeded)?;
            occupancy.extend(page.cells.iter().map(|cell| *cell != Ctk3Color::Empty));
            let field =
                StaticParityObservation::from_row_major_occupancy(width, height, &occupancy)
                    .map_err(|source| FieldDocumentParityError::Parity { page_index, source })?;
            let pending_garbage_occupied_cell_count =
                u16::try_from(page.garbage.as_ref().map_or(0, |row| {
                    row.iter().filter(|cell| **cell != Ctk3Color::Empty).count()
                }))
                .map_err(|_| FieldDocumentParityError::DimensionTooLarge { page_index })?;
            pages.push(FieldDocumentParityPage {
                page_index,
                field,
                pending_garbage_occupied_cell_count,
            });
        }
        Ok(Self {
            document_format: FieldDocumentFormat::Ctk3,
            pages,
        })
    }

    fn observe_fumen(source: &str) -> Result<Self, FieldDocumentParityError> {
        let document =
            ActualFumenParityDocument::decode(source).map_err(FieldDocumentParityError::Fumen)?;
        let mut pages = Vec::new();
        pages
            .try_reserve(document.pages().len())
            .map_err(|_| FieldDocumentParityError::CapacityExceeded)?;
        pages.extend(document.pages().iter().map(|page| FieldDocumentParityPage {
            page_index: page.page_index(),
            field: page.field().clone(),
            pending_garbage_occupied_cell_count: page.pending_garbage_occupied_cell_count(),
        }));
        Ok(Self {
            document_format: FieldDocumentFormat::Fumen,
            pages,
        })
    }

    pub const fn contract_id(&self) -> &'static str {
        STATIC_PARITY_REPORT_CONTRACT
    }

    pub const fn document_format(&self) -> FieldDocumentFormat {
        self.document_format
    }

    pub fn pages(&self) -> &[FieldDocumentParityPage] {
        &self.pages
    }

    pub fn into_parts(self) -> (FieldDocumentFormat, Vec<FieldDocumentParityPage>) {
        (self.document_format, self.pages)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FieldDocumentParityError {
    InputTooLarge {
        length: usize,
        maximum: usize,
    },
    EmptyDocument,
    TooManyPages {
        length: usize,
        maximum: usize,
    },
    DimensionTooLarge {
        page_index: usize,
    },
    CapacityExceeded,
    Ctk3(Ctk3CodecError),
    Fumen(ActualFumenParityDocumentError),
    Parity {
        page_index: usize,
        source: StaticParityObservationError,
    },
}

impl fmt::Display for FieldDocumentParityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for FieldDocumentParityError {}

#[cfg(test)]
mod tests {
    use clearra_output::{encode_ctk3_compact, Ctk3Document, Ctk3Page};
    use fumen::{CellColor, Fumen, Page};

    use super::*;

    #[test]
    fn ctk3_and_fumen_share_one_non_authoritative_page_report_contract() {
        let mut ctk3_page = Ctk3Page::new(
            2,
            vec![
                Ctk3Color::Piece(clearra_output::Ctk3Piece::T),
                Ctk3Color::Empty,
                Ctk3Color::Empty,
                Ctk3Color::Piece(clearra_output::Ctk3Piece::I),
            ],
        );
        ctk3_page.garbage = Some(vec![Ctk3Color::Gray, Ctk3Color::Empty]);
        let ctk3 = encode_ctk3_compact(&Ctk3Document::new(2, vec![ctk3_page])).unwrap();
        let ctk3_report = FieldDocumentParityReport::observe(&ctk3).unwrap();

        let mut fumen_page = Page::default();
        fumen_page.field[0][0] = CellColor::T;
        fumen_page.field[1][1] = CellColor::I;
        fumen_page.garbage_row[0] = CellColor::Grey;
        let fumen = Fumen {
            pages: vec![fumen_page],
            guideline: true,
        }
        .encode();
        let fumen_report = FieldDocumentParityReport::observe(&fumen).unwrap();

        assert_eq!(ctk3_report.contract_id(), "parity-report.v1");
        assert_eq!(ctk3_report.document_format().as_str(), "ctk3");
        assert_eq!(fumen_report.document_format().as_str(), "fumen");
        for report in [&ctk3_report, &fumen_report] {
            assert_eq!(report.pages().len(), 1);
            assert_eq!(report.pages()[0].field().occupied_cell_count(), 2);
            assert!(!report.pages()[0].field().feasibility_claim());
            assert_eq!(report.pages()[0].field().pruning_authority(), "none");
            assert_eq!(report.pages()[0].pending_garbage_occupied_cell_count(), 1);
        }
    }

    #[test]
    fn zero_height_page_fails_closed_instead_of_inventing_coordinates() {
        let source =
            encode_ctk3_compact(&Ctk3Document::new(10, vec![Ctk3Page::new(0, vec![])])).unwrap();
        assert!(matches!(
            FieldDocumentParityReport::observe(&source),
            Err(FieldDocumentParityError::Parity {
                source: StaticParityObservationError::EmptyDimensions,
                ..
            })
        ));
    }
}
