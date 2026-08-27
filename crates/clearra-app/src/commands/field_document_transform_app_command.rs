use core::fmt;

use clearra_ctk3::{TypedCtk3DocumentTransform, TypedCtk3TransformError};
use clearra_fumen::{ActualFumenDocumentTransform, ActualFumenTransformError};
use clearra_host_contract::{
    FieldDocumentPayload, ProductResultPayload, ProductResultPayloadContent,
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

pub const FIELD_DOCUMENT_TRANSFORM_RESULT_CONTRACT: &str = "field-document.v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldDocumentTransformKind {
    ToGray,
    Mirror,
}

impl FieldDocumentTransformKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ToGray => "to-gray",
            Self::Mirror => "mirror",
        }
    }

    pub fn parse(value: &str) -> Result<Self, FieldDocumentTransformAppCommandError> {
        match value {
            "to-gray" => Ok(Self::ToGray),
            "mirror" => Ok(Self::Mirror),
            _ => Err(FieldDocumentTransformAppCommandError::UnknownTransform(
                value.to_owned(),
            )),
        }
    }

    const fn result_kind(self) -> AppResultKind {
        match self {
            Self::ToGray => AppResultKind::ToGray,
            Self::Mirror => AppResultKind::Mirror,
        }
    }

    const fn error_code(self) -> AppErrorCode {
        match self {
            Self::ToGray => AppErrorCode::UtilityToGrayInvalid,
            Self::Mirror => AppErrorCode::UtilityMirrorInvalid,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldDocumentTransformAppCommand {
    transform: FieldDocumentTransformKind,
    document: TypedFieldDocument,
}

impl FieldDocumentTransformAppCommand {
    pub fn new(
        transform: FieldDocumentTransformKind,
        format: FieldDocumentFormat,
        document: impl Into<String>,
    ) -> Result<Self, FieldDocumentTransformAppCommandError> {
        Ok(Self {
            transform,
            document: TypedFieldDocument::new(format, document)
                .map_err(FieldDocumentTransformAppCommandError::Document)?,
        })
    }

    pub const fn transform(&self) -> FieldDocumentTransformKind {
        self.transform
    }

    pub const fn format(&self) -> FieldDocumentFormat {
        self.document.format()
    }

    pub fn document(&self) -> &TypedFieldDocument {
        &self.document
    }
}

impl RunnableAppCommand for FieldDocumentTransformAppCommand {
    fn run(self, _context: &AppExecutionContext<'_>) -> AppResponse {
        let transform = self.transform;
        let format = self.document.format();
        let result_kind = transform.result_kind();
        let transformed = match execute_transform(transform, &self.document) {
            Ok(document) => document,
            Err(error) => {
                return AppResponse::failed(
                    AppStatus::ValidationFailed,
                    AppError::new(transform.error_code(), error.to_string()),
                )
            }
        };
        let typed = match TypedFieldDocument::new(format, transformed) {
            Ok(document) => document,
            Err(error) => {
                return AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(transform.error_code(), error.to_string()),
                )
            }
        };
        let page_count = match u32::try_from(typed.page_count()) {
            Ok(page_count) => page_count,
            Err(_) => {
                return AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(
                        transform.error_code(),
                        FieldDocumentTransformAppCommandError::CapacityExceeded.to_string(),
                    ),
                )
            }
        };
        let document = typed.into_document();
        let payload = FieldDocumentPayload::new(
            format.as_str(),
            document.as_str(),
            page_count,
            sha256_hex(document.as_bytes()),
            output_filename(transform, format),
        );
        let fields = vec![
            string_field("contract_id", FIELD_DOCUMENT_TRANSFORM_RESULT_CONTRACT),
            string_field("format", format.as_str()),
            string_field("transform", transform.as_str()),
            number_field("page_count", page_count),
        ];
        AppResponse::success(AppRenderModel::Verify(AppMessage::new(result_kind, fields)))
            .with_public_product_result(
                ProductResultPayload::new(
                    FIELD_DOCUMENT_TRANSFORM_RESULT_CONTRACT,
                    result_kind.as_str(),
                    ProductResultPayloadContent::FieldDocument(payload),
                ),
                None,
            )
    }
}

fn execute_transform(
    transform: FieldDocumentTransformKind,
    source: &TypedFieldDocument,
) -> Result<String, FieldDocumentTransformAppCommandError> {
    match (source.format(), transform) {
        (FieldDocumentFormat::Ctk3, FieldDocumentTransformKind::ToGray) => {
            TypedCtk3DocumentTransform::to_gray(source.document())
                .map_err(FieldDocumentTransformAppCommandError::Ctk3)
        }
        (FieldDocumentFormat::Ctk3, FieldDocumentTransformKind::Mirror) => {
            TypedCtk3DocumentTransform::mirror(source.document())
                .map_err(FieldDocumentTransformAppCommandError::Ctk3)
        }
        (FieldDocumentFormat::Fumen, FieldDocumentTransformKind::ToGray) => {
            ActualFumenDocumentTransform::to_gray(source.document())
                .map_err(FieldDocumentTransformAppCommandError::Fumen)
        }
        (FieldDocumentFormat::Fumen, FieldDocumentTransformKind::Mirror) => {
            ActualFumenDocumentTransform::mirror(source.document())
                .map_err(FieldDocumentTransformAppCommandError::Fumen)
        }
    }
}

fn output_filename(transform: FieldDocumentTransformKind, format: FieldDocumentFormat) -> String {
    match format {
        FieldDocumentFormat::Ctk3 => format!("clearra-{}.ctk3", transform.as_str()),
        FieldDocumentFormat::Fumen => format!("clearra-{}-v115.txt", transform.as_str()),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FieldDocumentTransformAppCommandError {
    UnknownTransform(String),
    CapacityExceeded,
    Document(TypedFieldDocumentError),
    Ctk3(TypedCtk3TransformError),
    Fumen(ActualFumenTransformError),
}

impl fmt::Display for FieldDocumentTransformAppCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for FieldDocumentTransformAppCommandError {}

#[cfg(test)]
mod tests {
    use clearra_ctk3::{
        decode_ctk3_exact, encode_ctk3, Ctk3Color, Ctk3Document, Ctk3Operation, Ctk3Page,
        Ctk3Piece, Ctk3Rotation,
    };
    use clearra_host_contract::ProductResultPayloadContent;

    use super::*;

    fn run_document(
        transform: FieldDocumentTransformKind,
        format: FieldDocumentFormat,
        source: String,
    ) -> FieldDocumentPayload {
        let command = FieldDocumentTransformAppCommand::new(transform, format, source).unwrap();
        let variant = match transform {
            FieldDocumentTransformKind::ToGray => crate::AppCommand::UtilityToGray(command),
            FieldDocumentTransformKind::Mirror => crate::AppCommand::UtilityMirror(command),
        };
        let response = crate::AppContext::default().run(crate::AppRequest::new(variant));
        let host = response.to_host_response();
        let ProductResultPayloadContent::FieldDocument(payload) = host
            .product_result_payload()
            .expect("typed transform payload")
            .content()
        else {
            panic!("expected field-document payload")
        };
        payload.clone()
    }

    fn ctk3_source() -> String {
        let mut page = Ctk3Page::new(
            1,
            vec![
                Ctk3Color::Piece(Ctk3Piece::J),
                Ctk3Color::Empty,
                Ctk3Color::Piece(Ctk3Piece::S),
                Ctk3Color::Gray,
            ],
        );
        page.comment = "identity".to_owned();
        page.garbage = Some(vec![
            Ctk3Color::Piece(Ctk3Piece::L),
            Ctk3Color::Empty,
            Ctk3Color::Piece(Ctk3Piece::Z),
            Ctk3Color::Gray,
        ]);
        page.operation = Some(Ctk3Operation {
            piece: Ctk3Piece::T,
            rotation: Ctk3Rotation::Right,
            x: 1,
            y: 0,
        });
        encode_ctk3(&Ctk3Document::new(4, vec![page])).unwrap()
    }

    #[test]
    fn to_gray_preserves_ctk3_non_color_identity_and_returns_typed_payload() {
        let source = ctk3_source();
        let before = decode_ctk3_exact(&source).unwrap();
        let payload = run_document(
            FieldDocumentTransformKind::ToGray,
            FieldDocumentFormat::Ctk3,
            source,
        );
        let after = decode_ctk3_exact(payload.document()).unwrap();
        assert_eq!(payload.filename(), "clearra-to-gray.ctk3");
        assert_eq!(after.width, before.width);
        assert_eq!(after.pages[0].operation, before.pages[0].operation);
        assert_eq!(after.pages[0].comment, before.pages[0].comment);
        assert_eq!(after.pages[0].flags, before.pages[0].flags);
        assert_eq!(after.pages[0].height, before.pages[0].height);
        assert!(after.pages[0]
            .cells
            .iter()
            .chain(after.pages[0].garbage.as_ref().unwrap())
            .all(|cell| matches!(cell, Ctk3Color::Empty | Ctk3Color::Gray)));
    }

    #[test]
    fn mirror_is_a_typed_ctk3_and_fumen_involution() {
        let source = ctk3_source();
        let once = run_document(
            FieldDocumentTransformKind::Mirror,
            FieldDocumentFormat::Ctk3,
            source.clone(),
        );
        let twice = run_document(
            FieldDocumentTransformKind::Mirror,
            FieldDocumentFormat::Ctk3,
            once.document().to_owned(),
        );
        assert_eq!(
            decode_ctk3_exact(twice.document()),
            decode_ctk3_exact(&source)
        );

        let fumen = ActualFumenDocumentTransform::text_to_fumen(&["mirror".to_owned()]).unwrap();
        let once = run_document(
            FieldDocumentTransformKind::Mirror,
            FieldDocumentFormat::Fumen,
            fumen.clone(),
        );
        let twice = run_document(
            FieldDocumentTransformKind::Mirror,
            FieldDocumentFormat::Fumen,
            once.document().to_owned(),
        );
        assert_eq!(
            ActualFumenDocumentTransform::roundtrip(twice.document()).unwrap(),
            ActualFumenDocumentTransform::roundtrip(&fumen).unwrap()
        );
        assert_eq!(twice.filename(), "clearra-mirror-v115.txt");
    }
}
