use core::fmt;

use clearra_fumen::ActualFumenRenderDocument;
use clearra_output::decode_ctk3_exact;

pub const FIELD_DOCUMENT_MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
pub const FIELD_DOCUMENT_MAX_PAGES: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FieldDocumentFormat {
    Ctk3,
    Fumen,
}

impl FieldDocumentFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ctk3 => "ctk3",
            Self::Fumen => "fumen",
        }
    }

    pub fn parse(value: &str) -> Result<Self, TypedFieldDocumentError> {
        match value {
            "ctk3" => Ok(Self::Ctk3),
            "fumen" => Ok(Self::Fumen),
            _ => Err(TypedFieldDocumentError::UnknownFormat(value.to_owned())),
        }
    }

    /// Native CLI-only inference. Only unambiguous canonical encoded prefixes
    /// are recognized; URLs, grids, and embedded payload links fail closed.
    pub fn infer_canonical(document: &str) -> Result<Self, TypedFieldDocumentError> {
        let ctk3 = has_ctk3_prefix(document);
        let fumen = document.starts_with("v115@");
        match (ctk3, fumen) {
            (true, false) => Ok(Self::Ctk3),
            (false, true) => Ok(Self::Fumen),
            _ => Err(TypedFieldDocumentError::FormatInferenceFailed),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedFieldDocument {
    format: FieldDocumentFormat,
    document: String,
    page_count: usize,
}

impl TypedFieldDocument {
    pub fn new(
        format: FieldDocumentFormat,
        document: impl Into<String>,
    ) -> Result<Self, TypedFieldDocumentError> {
        let document = document.into();
        if document.is_empty() {
            return Err(TypedFieldDocumentError::EmptyDocument);
        }
        if document.len() > FIELD_DOCUMENT_MAX_INPUT_BYTES {
            return Err(TypedFieldDocumentError::InputTooLarge {
                length: document.len(),
                maximum: FIELD_DOCUMENT_MAX_INPUT_BYTES,
            });
        }
        let prefix_matches = match format {
            FieldDocumentFormat::Ctk3 => has_ctk3_prefix(&document),
            FieldDocumentFormat::Fumen => document.starts_with("v115@"),
        };
        if !prefix_matches {
            return Err(TypedFieldDocumentError::FormatPrefixMismatch { format });
        }
        let page_count = match format {
            FieldDocumentFormat::Ctk3 => decode_ctk3_exact(&document)
                .map_err(|error| TypedFieldDocumentError::Decode(error.to_string()))?
                .pages
                .len(),
            FieldDocumentFormat::Fumen => ActualFumenRenderDocument::decode(&document)
                .map_err(|error| TypedFieldDocumentError::Decode(error.to_string()))?
                .pages()
                .len(),
        };
        if page_count == 0 {
            return Err(TypedFieldDocumentError::EmptyDocument);
        }
        if page_count > FIELD_DOCUMENT_MAX_PAGES {
            return Err(TypedFieldDocumentError::TooManyPages {
                length: page_count,
                maximum: FIELD_DOCUMENT_MAX_PAGES,
            });
        }
        Ok(Self {
            format,
            document,
            page_count,
        })
    }

    pub const fn format(&self) -> FieldDocumentFormat {
        self.format
    }

    pub fn document(&self) -> &str {
        &self.document
    }

    pub const fn page_count(&self) -> usize {
        self.page_count
    }

    pub fn into_document(self) -> String {
        self.document
    }
}

fn has_ctk3_prefix(document: &str) -> bool {
    document.starts_with("ctk3_") || document.starts_with("ctk3@") || document.starts_with("ctk3b_")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypedFieldDocumentError {
    UnknownFormat(String),
    FormatInferenceFailed,
    FormatPrefixMismatch { format: FieldDocumentFormat },
    EmptyDocument,
    InputTooLarge { length: usize, maximum: usize },
    TooManyPages { length: usize, maximum: usize },
    Decode(String),
}

impl fmt::Display for TypedFieldDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for TypedFieldDocumentError {}

#[cfg(test)]
mod tests {
    use clearra_output::{encode_ctk3_compact, Ctk3Document, Ctk3Page};

    use super::*;

    #[test]
    fn explicit_format_and_native_prefix_inference_are_fail_closed() {
        let ctk3 = encode_ctk3_compact(&Ctk3Document::new(
            10,
            vec![Ctk3Page::new(1, vec![clearra_output::Ctk3Color::Empty; 10])],
        ))
        .unwrap();
        assert_eq!(
            FieldDocumentFormat::infer_canonical(&ctk3),
            Ok(FieldDocumentFormat::Ctk3)
        );
        assert!(TypedFieldDocument::new(FieldDocumentFormat::Ctk3, ctk3).is_ok());
        assert!(matches!(
            TypedFieldDocument::new(FieldDocumentFormat::Fumen, "https://fumen.zui.jp/?v115@vhA"),
            Err(TypedFieldDocumentError::FormatPrefixMismatch { .. })
        ));
        assert_eq!(
            FieldDocumentFormat::infer_canonical("....XXXX"),
            Err(TypedFieldDocumentError::FormatInferenceFailed)
        );
    }
}
