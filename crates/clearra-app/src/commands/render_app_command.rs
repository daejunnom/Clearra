use core::fmt;

use clearra_host_contract::{
    ProductResultPayload, ProductResultPayloadContent, RenderArtifactPayload,
};
use clearra_output::{
    ExactBitmapOutputFormat, ExactFieldDocumentFormat, FieldDocumentRenderError,
    RenderExactOutputGate, PUBLIC_BITMAP_ARTIFACT_MAX_BYTES,
};

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::{bool_field, number_field, string_field},
    document_utility_encoding::{base64_standard, sha256_hex},
    render::{AppMessage, AppRenderModel, AppResultKind},
    typed_document_utility::{FieldDocumentFormat, TypedFieldDocument, TypedFieldDocumentError},
};

pub const RENDER_ARTIFACT_RESULT_CONTRACT: &str = "render-artifact.v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderArtifactFormat {
    Png,
    Gif,
}

impl RenderArtifactFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Gif => "gif",
        }
    }

    pub fn parse(value: &str) -> Result<Self, RenderAppCommandError> {
        match value {
            "png" => Ok(Self::Png),
            "gif" => Ok(Self::Gif),
            _ => Err(RenderAppCommandError::UnknownArtifactFormat(
                value.to_owned(),
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderAppCommand {
    document: TypedFieldDocument,
    artifact_format: RenderArtifactFormat,
    page_number: Option<usize>,
}

impl RenderAppCommand {
    pub fn new(
        format: FieldDocumentFormat,
        document: impl Into<String>,
        artifact_format: RenderArtifactFormat,
        page_number: Option<usize>,
    ) -> Result<Self, RenderAppCommandError> {
        match (artifact_format, page_number) {
            (RenderArtifactFormat::Png, Some(0)) => {
                return Err(RenderAppCommandError::PageNumberInvalid)
            }
            (RenderArtifactFormat::Gif, Some(_)) => {
                return Err(RenderAppCommandError::PageNumberNotAllowedForGif)
            }
            _ => {}
        }
        Ok(Self {
            document: TypedFieldDocument::new(format, document)
                .map_err(RenderAppCommandError::Document)?,
            artifact_format,
            page_number,
        })
    }

    pub const fn format(&self) -> FieldDocumentFormat {
        self.document.format()
    }
    pub fn document(&self) -> &str {
        self.document.document()
    }
    pub const fn artifact_format(&self) -> RenderArtifactFormat {
        self.artifact_format
    }
    pub const fn page_number(&self) -> Option<usize> {
        self.page_number
    }
}

impl RunnableAppCommand for RenderAppCommand {
    fn run(self, _context: &AppExecutionContext<'_>) -> AppResponse {
        let page_number = match self.artifact_format {
            RenderArtifactFormat::Png => Some(self.page_number.unwrap_or(1)),
            RenderArtifactFormat::Gif => None,
        };
        let output = match RenderExactOutputGate::render_field_document(
            self.document.document(),
            exact_document_format(self.document.format()),
            exact_artifact_format(self.artifact_format),
            page_number,
        ) {
            Ok(output) => output,
            Err(error) => return render_error_response(error),
        };
        let byte_length = match u64::try_from(output.bytes().len()) {
            Ok(length) => length,
            Err(_) => {
                return AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(
                        AppErrorCode::UtilityRenderLimitExceeded,
                        "render artifact length cannot be represented",
                    ),
                )
            }
        };
        let document_page_count = match u32::try_from(self.document.page_count()) {
            Ok(count) => count,
            Err(_) => {
                return AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(
                        AppErrorCode::UtilityRenderLimitExceeded,
                        "render document page count cannot be represented",
                    ),
                )
            }
        };
        let bytes_base64 = match base64_standard(output.bytes()) {
            Ok(encoded) => encoded,
            Err(error) => {
                return AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(AppErrorCode::ExecutionFailed, format!("{error:?}")),
                )
            }
        };
        let sha256 = sha256_hex(output.bytes());
        let media_type = match self.artifact_format {
            RenderArtifactFormat::Png => "image/png",
            RenderArtifactFormat::Gif => "image/gif",
        };
        let filename = match self.artifact_format {
            RenderArtifactFormat::Png => {
                format!("clearra-render-page-{:04}.png", page_number.unwrap_or(1))
            }
            RenderArtifactFormat::Gif => "clearra-render-timeline.gif".to_owned(),
        };
        let public_max = PUBLIC_BITMAP_ARTIFACT_MAX_BYTES as u64;
        let payload = RenderArtifactPayload::new(
            self.document.format().as_str(),
            self.artifact_format.as_str(),
            page_number.and_then(|number| u32::try_from(number).ok()),
            document_page_count,
            media_type,
            filename,
            byte_length,
            sha256,
            bytes_base64,
            output.render_exact(),
            output.skin_id(),
            public_max,
            public_max,
        );
        let fields = vec![
            string_field("contract_id", RENDER_ARTIFACT_RESULT_CONTRACT),
            string_field("document_format", self.document.format().as_str()),
            string_field("artifact_format", self.artifact_format.as_str()),
            number_field("document_page_count", document_page_count),
            number_field("byte_length", byte_length),
            bool_field("render_exact", true),
        ];
        AppResponse::success(AppRenderModel::Verify(AppMessage::new(
            AppResultKind::Render,
            fields,
        )))
        .with_public_product_result(
            ProductResultPayload::new(
                RENDER_ARTIFACT_RESULT_CONTRACT,
                AppResultKind::Render.as_str(),
                ProductResultPayloadContent::RenderArtifact(payload),
            ),
            None,
        )
    }
}

const fn exact_document_format(format: FieldDocumentFormat) -> ExactFieldDocumentFormat {
    match format {
        FieldDocumentFormat::Ctk3 => ExactFieldDocumentFormat::Ctk3,
        FieldDocumentFormat::Fumen => ExactFieldDocumentFormat::Fumen,
    }
}

const fn exact_artifact_format(format: RenderArtifactFormat) -> ExactBitmapOutputFormat {
    match format {
        RenderArtifactFormat::Png => ExactBitmapOutputFormat::Png,
        RenderArtifactFormat::Gif => ExactBitmapOutputFormat::Gif,
    }
}

fn render_error_response(error: FieldDocumentRenderError) -> AppResponse {
    let limit = error.is_limit_exceeded();
    AppResponse::failed(
        if limit {
            AppStatus::ExecutionFailed
        } else {
            AppStatus::ValidationFailed
        },
        AppError::new(
            if limit {
                AppErrorCode::UtilityRenderLimitExceeded
            } else {
                AppErrorCode::UtilityRenderInvalid
            },
            error.to_string(),
        ),
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RenderAppCommandError {
    UnknownArtifactFormat(String),
    PageNumberInvalid,
    PageNumberNotAllowedForGif,
    Document(TypedFieldDocumentError),
}

impl fmt::Display for RenderAppCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for RenderAppCommandError {}

#[cfg(test)]
mod tests {
    use clearra_output::{encode_ctk3_compact, Ctk3Color, Ctk3Document, Ctk3Page};

    use super::*;

    #[test]
    fn app_exposes_exact_png_as_bounded_typed_artifact() {
        let document = encode_ctk3_compact(&Ctk3Document::new(
            2,
            vec![Ctk3Page::new(1, vec![Ctk3Color::Gray, Ctk3Color::Empty])],
        ))
        .unwrap();
        let command = RenderAppCommand::new(
            FieldDocumentFormat::Ctk3,
            document,
            RenderArtifactFormat::Png,
            Some(1),
        )
        .unwrap();
        let response = crate::AppContext::default().run(crate::AppRequest::new(
            crate::AppCommand::UtilityRender(command),
        ));
        assert_eq!(response.status(), AppStatus::Success);
        let host = response.to_host_response();
        let ProductResultPayloadContent::RenderArtifact(artifact) =
            host.product_result_payload().unwrap().content()
        else {
            panic!("expected render artifact")
        };
        assert_eq!(artifact.media_type(), "image/png");
        assert!(artifact.bytes_base64().starts_with("iVBOR"));
        assert!(artifact.render_exact());
        assert!(artifact.byte_length() <= artifact.transport_max_bytes());
    }
}
