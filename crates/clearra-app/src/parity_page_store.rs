use std::sync::Arc;

use clearra_host_contract::ParityReportPagePayload;

use crate::{
    field_document_parity::{
        FieldDocumentFormat, FieldDocumentParityPage, FieldDocumentParityReport,
    },
    portfolio_alternative_store::PortfolioAlternativeError,
};

/// Immutable owner for the complete bounded parity report. Public pages are
/// 1-based and the owner survives every page request until the caller drops
/// the handle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParityReportPageSource {
    document_format: FieldDocumentFormat,
    pages: Vec<FieldDocumentParityPage>,
}

impl ParityReportPageSource {
    pub fn from_report(
        report: FieldDocumentParityReport,
    ) -> Result<Self, PortfolioAlternativeError> {
        let (document_format, pages) = report.into_parts();
        if pages.is_empty() {
            return Err(PortfolioAlternativeError::InvalidParityPage);
        }
        u32::try_from(pages.len())
            .map_err(|_| PortfolioAlternativeError::ParityPageCountOverflow)?;
        Ok(Self {
            document_format,
            pages,
        })
    }

    pub const fn document_format(&self) -> FieldDocumentFormat {
        self.document_format
    }

    pub fn pages(&self) -> &[FieldDocumentParityPage] {
        &self.pages
    }

    pub fn page_payload(
        &self,
        page_number: usize,
        page_handle_available: bool,
    ) -> Result<ParityReportPagePayload, PortfolioAlternativeError> {
        let page_index = page_number
            .checked_sub(1)
            .ok_or(PortfolioAlternativeError::InvalidParityPage)?;
        let page = self
            .pages
            .get(page_index)
            .ok_or(PortfolioAlternativeError::InvalidParityPage)?;
        let observation = page.field();
        Ok(ParityReportPagePayload::new(
            self.document_format.as_str(),
            u32::try_from(page_number)
                .map_err(|_| PortfolioAlternativeError::ParityPageCountOverflow)?,
            u32::try_from(self.pages.len())
                .map_err(|_| PortfolioAlternativeError::ParityPageCountOverflow)?,
            observation.coordinate_basis(),
            observation.width(),
            observation.height(),
            observation.occupied_cell_count(),
            observation.checker_black_count(),
            observation.checker_white_count(),
            observation.checker_delta(),
            observation.four_color_counts(),
            observation.even_column_count(),
            observation.odd_column_count(),
            observation.column_parity_delta(),
            observation.occupied_area_mod_four(),
            page.pending_garbage_occupied_cell_count(),
            false,
            "none",
            page_handle_available,
        ))
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.pages.capacity() as u128)
            .checked_mul(core::mem::size_of::<FieldDocumentParityPage>() as u128)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParityReportPageStore {
    source: Arc<ParityReportPageSource>,
    current_page_number: usize,
}

impl ParityReportPageStore {
    pub fn new(source: Arc<ParityReportPageSource>) -> Result<Self, PortfolioAlternativeError> {
        if source.pages().is_empty() {
            return Err(PortfolioAlternativeError::InvalidParityPage);
        }
        Ok(Self {
            source,
            current_page_number: 1,
        })
    }

    pub fn source(&self) -> &Arc<ParityReportPageSource> {
        &self.source
    }

    pub const fn current_page_number(&self) -> usize {
        self.current_page_number
    }

    pub fn page(
        &self,
        page_number: usize,
    ) -> Result<ParityReportPagePayload, PortfolioAlternativeError> {
        self.source.page_payload(page_number, true)
    }

    pub fn next_page(
        &mut self,
    ) -> Result<Option<ParityReportPagePayload>, PortfolioAlternativeError> {
        let next = self
            .current_page_number
            .checked_add(1)
            .ok_or(PortfolioAlternativeError::ParityPageCountOverflow)?;
        if next > self.source.pages().len() {
            return Ok(None);
        }
        self.current_page_number = next;
        self.source.page_payload(next, true).map(Some)
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        self.source.checked_retained_capacity_bytes()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use clearra_output::{encode_ctk3_compact, Ctk3Color, Ctk3Document, Ctk3Page};

    use super::*;

    #[test]
    fn page_store_is_one_based_and_never_promotes_parity_to_feasibility() {
        let source = encode_ctk3_compact(&Ctk3Document::new(
            2,
            vec![
                Ctk3Page::new(1, vec![Ctk3Color::Gray, Ctk3Color::Empty]),
                Ctk3Page::new(1, vec![Ctk3Color::Empty, Ctk3Color::Gray]),
            ],
        ))
        .unwrap();
        let report =
            FieldDocumentParityReport::observe_typed(FieldDocumentFormat::Ctk3, &source).unwrap();
        let source = Arc::new(ParityReportPageSource::from_report(report).unwrap());
        let mut store = ParityReportPageStore::new(source).unwrap();

        let first = store.page(1).unwrap();
        assert_eq!(first.page_number(), 1);
        assert!(!first.feasibility_claim());
        assert_eq!(first.pruning_authority(), "none");
        assert_eq!(store.next_page().unwrap().unwrap().page_number(), 2);
        assert_eq!(store.next_page().unwrap(), None);
    }
}
