use core::fmt;

use clearra_fumen::{ActualFumenDocumentTransform, ActualFumenTransformError};
use clearra_host_contract::{
    FieldDocumentPayload, FieldDocumentSetPayload, ProductResultPayload,
    ProductResultPayloadContent,
};

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::{number_field, string_field},
    document_utility_encoding::sha256_hex,
    render::{AppMessage, AppRenderModel, AppResultKind},
    typed_document_utility::{FieldDocumentFormat, TypedFieldDocument, TypedFieldDocumentError},
};

pub const FIELD_DOCUMENT_RESULT_CONTRACT: &str = "field-document.v1";
pub const FIELD_DOCUMENT_SET_RESULT_CONTRACT: &str = "field-document-set.v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FumenTransformKind {
    Roundtrip,
    Combine,
    Split,
    GetPage,
    PageShift,
    CleanComments,
    PreserveComments,
    ToGray,
    Mirror,
    TextToFumen,
}

impl FumenTransformKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Roundtrip => "roundtrip",
            Self::Combine => "combine",
            Self::Split => "split",
            Self::GetPage => "get-page",
            Self::PageShift => "page-shift",
            Self::CleanComments => "clean-comments",
            Self::PreserveComments => "preserve-comments",
            Self::ToGray => "to-gray",
            Self::Mirror => "mirror",
            Self::TextToFumen => "text-to-fumen",
        }
    }

    pub fn parse(value: &str) -> Result<Self, FumenAppCommandError> {
        match value {
            "roundtrip" => Ok(Self::Roundtrip),
            "combine" => Ok(Self::Combine),
            "split" => Ok(Self::Split),
            "get-page" => Ok(Self::GetPage),
            "page-shift" => Ok(Self::PageShift),
            "clean-comments" => Ok(Self::CleanComments),
            "preserve-comments" => Ok(Self::PreserveComments),
            "to-gray" => Ok(Self::ToGray),
            "mirror" => Ok(Self::Mirror),
            "text-to-fumen" => Ok(Self::TextToFumen),
            _ => Err(FumenAppCommandError::UnknownTransform(value.to_owned())),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FumenAppCommand {
    transform: FumenTransformKind,
    documents: Vec<TypedFieldDocument>,
    page_number: Option<usize>,
    page_shift: Option<isize>,
    comments: Vec<String>,
}

impl FumenAppCommand {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        format: FieldDocumentFormat,
        transform: FumenTransformKind,
        documents: Vec<String>,
        page_number: Option<usize>,
        page_shift: Option<isize>,
        comments: Vec<String>,
    ) -> Result<Self, FumenAppCommandError> {
        if format != FieldDocumentFormat::Fumen {
            return Err(FumenAppCommandError::FumenFormatRequired);
        }
        validate_shape(transform, &documents, page_number, page_shift, &comments)?;
        let documents = documents
            .into_iter()
            .map(|document| TypedFieldDocument::new(format, document))
            .collect::<Result<Vec<_>, _>>()
            .map_err(FumenAppCommandError::Document)?;
        Ok(Self {
            transform,
            documents,
            page_number,
            page_shift,
            comments,
        })
    }

    pub const fn transform(&self) -> FumenTransformKind {
        self.transform
    }
    pub fn documents(&self) -> &[TypedFieldDocument] {
        &self.documents
    }
    pub const fn page_number(&self) -> Option<usize> {
        self.page_number
    }
    pub const fn page_shift(&self) -> Option<isize> {
        self.page_shift
    }
    pub fn comments(&self) -> &[String] {
        &self.comments
    }
}

fn validate_shape(
    transform: FumenTransformKind,
    documents: &[String],
    page_number: Option<usize>,
    page_shift: Option<isize>,
    comments: &[String],
) -> Result<(), FumenAppCommandError> {
    let expected_documents = match transform {
        FumenTransformKind::Combine => {
            if documents.is_empty() {
                return Err(FumenAppCommandError::DocumentCountInvalid);
            }
            None
        }
        FumenTransformKind::TextToFumen => Some(0),
        _ => Some(1),
    };
    if expected_documents.is_some_and(|expected| documents.len() != expected) {
        return Err(FumenAppCommandError::DocumentCountInvalid);
    }
    match transform {
        FumenTransformKind::GetPage if page_number.is_some_and(|page| page > 0) => {}
        FumenTransformKind::GetPage => return Err(FumenAppCommandError::PageNumberRequired),
        _ if page_number.is_some() => return Err(FumenAppCommandError::UnexpectedPageNumber),
        _ => {}
    }
    match transform {
        FumenTransformKind::PageShift if page_shift.is_some() => {}
        FumenTransformKind::PageShift => return Err(FumenAppCommandError::PageShiftRequired),
        _ if page_shift.is_some() => return Err(FumenAppCommandError::UnexpectedPageShift),
        _ => {}
    }
    match transform {
        FumenTransformKind::TextToFumen if comments.is_empty() => {
            Err(FumenAppCommandError::CommentsRequired)
        }
        FumenTransformKind::TextToFumen => Ok(()),
        _ if !comments.is_empty() => Err(FumenAppCommandError::UnexpectedComments),
        _ => Ok(()),
    }
}

impl RunnableAppCommand for FumenAppCommand {
    fn run(self, _context: &AppExecutionContext<'_>) -> AppResponse {
        let transform = self.transform;
        let result = match execute_transform(&self) {
            Ok(result) => result,
            Err(error) => {
                return AppResponse::failed(
                    AppStatus::ValidationFailed,
                    AppError::new(AppErrorCode::UtilityFumenInvalid, error.to_string()),
                )
            }
        };
        let (contract, content, document_count) = match result {
            FumenTransformResult::Document(document) => {
                let payload = match field_document_payload(document, None) {
                    Ok(payload) => payload,
                    Err(error) => return fumen_error_response(error),
                };
                (
                    FIELD_DOCUMENT_RESULT_CONTRACT,
                    ProductResultPayloadContent::FieldDocument(payload),
                    1,
                )
            }
            FumenTransformResult::DocumentSet(documents) => {
                let mut payloads = Vec::new();
                if payloads.try_reserve(documents.len()).is_err() {
                    return fumen_error_response(FumenAppCommandError::CapacityExceeded);
                }
                for (index, document) in documents.into_iter().enumerate() {
                    let page_number = match index.checked_add(1) {
                        Some(number) => number,
                        None => {
                            return fumen_error_response(FumenAppCommandError::CapacityExceeded)
                        }
                    };
                    match field_document_payload(document, Some(page_number)) {
                        Ok(payload) => payloads.push(payload),
                        Err(error) => return fumen_error_response(error),
                    }
                }
                let count = payloads.len();
                (
                    FIELD_DOCUMENT_SET_RESULT_CONTRACT,
                    ProductResultPayloadContent::FieldDocumentSet(FieldDocumentSetPayload::new(
                        FIELD_DOCUMENT_RESULT_CONTRACT,
                        payloads,
                    )),
                    count,
                )
            }
        };
        let fields = vec![
            string_field("contract_id", contract),
            string_field("format", "fumen"),
            string_field("transform", transform.as_str()),
            number_field("document_count", document_count),
        ];
        AppResponse::success(AppRenderModel::Verify(AppMessage::new(
            AppResultKind::Fumen,
            fields,
        )))
        .with_public_product_result(
            ProductResultPayload::new(contract, AppResultKind::Fumen.as_str(), content),
            None,
        )
    }
}

enum FumenTransformResult {
    Document(String),
    DocumentSet(Vec<String>),
}

fn execute_transform(
    command: &FumenAppCommand,
) -> Result<FumenTransformResult, ActualFumenTransformError> {
    let source = || command.documents[0].document();
    match command.transform {
        FumenTransformKind::Roundtrip => {
            ActualFumenDocumentTransform::roundtrip(source()).map(FumenTransformResult::Document)
        }
        FumenTransformKind::Combine => {
            let sources = command
                .documents
                .iter()
                .map(|document| document.document().to_owned())
                .collect::<Vec<_>>();
            ActualFumenDocumentTransform::combine(&sources).map(FumenTransformResult::Document)
        }
        FumenTransformKind::Split => {
            ActualFumenDocumentTransform::split(source()).map(FumenTransformResult::DocumentSet)
        }
        FumenTransformKind::GetPage => ActualFumenDocumentTransform::get_page(
            source(),
            command
                .page_number
                .expect("constructor requires page number")
                - 1,
        )
        .map(FumenTransformResult::Document),
        FumenTransformKind::PageShift => ActualFumenDocumentTransform::page_shift(
            source(),
            command.page_shift.expect("constructor requires page shift"),
        )
        .map(FumenTransformResult::Document),
        FumenTransformKind::CleanComments => ActualFumenDocumentTransform::clean_comments(source())
            .map(FumenTransformResult::Document),
        FumenTransformKind::PreserveComments => {
            ActualFumenDocumentTransform::preserve_comments(source())
                .map(FumenTransformResult::Document)
        }
        FumenTransformKind::ToGray => {
            ActualFumenDocumentTransform::to_gray(source()).map(FumenTransformResult::Document)
        }
        FumenTransformKind::Mirror => {
            ActualFumenDocumentTransform::mirror(source()).map(FumenTransformResult::Document)
        }
        FumenTransformKind::TextToFumen => {
            ActualFumenDocumentTransform::text_to_fumen(&command.comments)
                .map(FumenTransformResult::Document)
        }
    }
}

fn field_document_payload(
    document: String,
    split_page_number: Option<usize>,
) -> Result<FieldDocumentPayload, FumenAppCommandError> {
    let typed = TypedFieldDocument::new(FieldDocumentFormat::Fumen, document)
        .map_err(FumenAppCommandError::Document)?;
    let page_count =
        u32::try_from(typed.page_count()).map_err(|_| FumenAppCommandError::CapacityExceeded)?;
    let filename = split_page_number.map_or_else(
        || "clearra-fumen-v115.txt".to_owned(),
        |page_number| format!("clearra-fumen-page-{page_number:04}.txt"),
    );
    let document = typed.into_document();
    let digest = sha256_hex(document.as_bytes());
    Ok(FieldDocumentPayload::new(
        "fumen", document, page_count, digest, filename,
    ))
}

fn fumen_error_response(error: FumenAppCommandError) -> AppResponse {
    AppResponse::failed(
        AppStatus::ExecutionFailed,
        AppError::new(AppErrorCode::UtilityFumenInvalid, error.to_string()),
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FumenAppCommandError {
    UnknownTransform(String),
    FumenFormatRequired,
    DocumentCountInvalid,
    PageNumberRequired,
    UnexpectedPageNumber,
    PageShiftRequired,
    UnexpectedPageShift,
    CommentsRequired,
    UnexpectedComments,
    CapacityExceeded,
    Document(TypedFieldDocumentError),
}

impl fmt::Display for FumenAppCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for FumenAppCommandError {}

#[cfg(test)]
mod tests {
    use clearra_fumen::ActualFumenDocumentTransform;

    use super::*;

    #[test]
    fn split_returns_typed_document_set_and_get_page_is_publicly_one_based() {
        let source =
            ActualFumenDocumentTransform::text_to_fumen(&["first".to_owned(), "second".to_owned()])
                .unwrap();
        let split = FumenAppCommand::new(
            FieldDocumentFormat::Fumen,
            FumenTransformKind::Split,
            vec![source.clone()],
            None,
            None,
            vec![],
        )
        .unwrap();
        let response = crate::AppContext::default().run(crate::AppRequest::new(
            crate::AppCommand::UtilityFumen(split),
        ));
        let host = response.to_host_response();
        let ProductResultPayloadContent::FieldDocumentSet(set) =
            host.product_result_payload().unwrap().content()
        else {
            panic!("expected document set")
        };
        assert_eq!(set.documents().len(), 2);
        assert_eq!(set.documents()[0].filename(), "clearra-fumen-page-0001.txt");

        assert!(FumenAppCommand::new(
            FieldDocumentFormat::Fumen,
            FumenTransformKind::GetPage,
            vec![source],
            Some(0),
            None,
            vec![],
        )
        .is_err());
    }
}
