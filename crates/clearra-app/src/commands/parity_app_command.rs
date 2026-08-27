use std::sync::Arc;

use clearra_host_contract::{ProductResultPayload, ProductResultPayloadContent};

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::{bool_field, number_field, string_field},
    field_document_parity::FieldDocumentParityReport,
    parity_page_store::ParityReportPageSource,
    portfolio_alternative_store::ProductPageSourceOwner,
    render::{AppMessage, AppRenderModel, AppResultKind},
    typed_document_utility::{FieldDocumentFormat, TypedFieldDocument, TypedFieldDocumentError},
};

pub const PARITY_RESULT_CONTRACT: &str = "parity-report.v1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParityAppCommand {
    document: TypedFieldDocument,
}

impl ParityAppCommand {
    pub fn new(
        format: FieldDocumentFormat,
        document: impl Into<String>,
    ) -> Result<Self, TypedFieldDocumentError> {
        Ok(Self {
            document: TypedFieldDocument::new(format, document)?,
        })
    }

    pub const fn format(&self) -> FieldDocumentFormat {
        self.document.format()
    }

    pub fn document(&self) -> &str {
        self.document.document()
    }
}

impl RunnableAppCommand for ParityAppCommand {
    fn run(self, _context: &AppExecutionContext<'_>) -> AppResponse {
        let report = match FieldDocumentParityReport::observe_typed(
            self.document.format(),
            self.document.document(),
        ) {
            Ok(report) => report,
            Err(error) => {
                return AppResponse::failed(
                    AppStatus::ValidationFailed,
                    AppError::new(AppErrorCode::UtilityParityInvalid, error.to_string()),
                )
            }
        };
        let source = match ParityReportPageSource::from_report(report) {
            Ok(source) => Arc::new(source),
            Err(error) => {
                return AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(AppErrorCode::ExecutionFailed, error.as_str()),
                )
            }
        };
        let first_page = match source.page_payload(1, true) {
            Ok(page) => page,
            Err(error) => {
                return AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(AppErrorCode::ExecutionFailed, error.as_str()),
                )
            }
        };
        let fields = vec![
            string_field("contract_id", PARITY_RESULT_CONTRACT),
            string_field("document_format", first_page.document_format()),
            number_field("page_number", first_page.page_number()),
            number_field("total_pages", first_page.total_pages()),
            string_field("coordinate_basis", first_page.coordinate_basis()),
            number_field("occupied_cell_count", first_page.occupied_cell_count()),
            number_field(
                "pending_garbage_occupied_cell_count",
                first_page.pending_garbage_occupied_cell_count(),
            ),
            bool_field("feasibility_claim", false),
            string_field("pruning_authority", "none"),
        ];
        AppResponse::success(AppRenderModel::Verify(AppMessage::new(
            AppResultKind::Parity,
            fields,
        )))
        .with_public_product_result(
            ProductResultPayload::new(
                PARITY_RESULT_CONTRACT,
                AppResultKind::Parity.as_str(),
                ProductResultPayloadContent::ParityReportPage(first_page),
            ),
            Some(ProductPageSourceOwner::ParityReport(source)),
        )
    }
}

#[cfg(test)]
mod tests {
    use clearra_output::{encode_ctk3_compact, Ctk3Color, Ctk3Document, Ctk3Page};

    use super::*;

    #[test]
    fn app_result_carries_first_page_and_transferable_owner_without_authority_claims() {
        let document = encode_ctk3_compact(&Ctk3Document::new(
            2,
            vec![
                Ctk3Page::new(1, vec![Ctk3Color::Gray, Ctk3Color::Empty]),
                Ctk3Page::new(1, vec![Ctk3Color::Empty, Ctk3Color::Gray]),
            ],
        ))
        .unwrap();
        let response = crate::AppContext::default().run(crate::AppRequest::new(
            crate::AppCommand::UtilityParity(
                ParityAppCommand::new(FieldDocumentFormat::Ctk3, document).unwrap(),
            ),
        ));
        assert_eq!(response.status(), AppStatus::Success);
        let host = response.to_host_response();
        let ProductResultPayloadContent::ParityReportPage(page) = host
            .product_result_payload()
            .expect("typed payload")
            .content()
        else {
            panic!("expected parity page")
        };
        assert_eq!(page.page_number(), 1);
        assert_eq!(page.total_pages(), 2);
        assert!(!page.feasibility_claim());
        assert_eq!(page.pruning_authority(), "none");
        assert!(matches!(
            response.public_page_source_owner(),
            Some(ProductPageSourceOwner::ParityReport(_))
        ));
    }
}
