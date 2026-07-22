use std::fmt;

use super::template_import::TemplateExportFormat;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TemplateJsonError {
    InvalidJson,
    ExpectedObject {
        context: &'static str,
    },
    MissingField {
        context: &'static str,
        field: &'static str,
    },
    UnknownField {
        context: &'static str,
        field: String,
    },
    InvalidField {
        context: &'static str,
        field: &'static str,
        reason: String,
    },
    UnsupportedSchemaVersion {
        version: u64,
    },
}

impl fmt::Display for TemplateJsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson => write!(formatter, "template json is not valid JSON"),
            Self::ExpectedObject { context } => {
                write!(
                    formatter,
                    "template json field '{context}' must be an object"
                )
            }
            Self::MissingField { context, field } => {
                write!(
                    formatter,
                    "template json field '{context}.{field}' is required"
                )
            }
            Self::UnknownField { context, field } => {
                write!(
                    formatter,
                    "template json field '{context}.{field}' is not supported"
                )
            }
            Self::InvalidField {
                context,
                field,
                reason,
            } => write!(
                formatter,
                "template json field '{context}.{field}' is invalid: {reason}"
            ),
            Self::UnsupportedSchemaVersion { version } => write!(
                formatter,
                "template json schema_version {version} is not supported"
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TemplateExportError {
    UnsupportedFormat { format: TemplateExportFormat },
    JsonSerializationFailed,
}

impl fmt::Display for TemplateExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormat { format } => {
                write!(formatter, "template export format '{format:?}' is not JSON")
            }
            Self::JsonSerializationFailed => write!(formatter, "template JSON export failed"),
        }
    }
}
