use serde_json::Value;

use super::{
    build_template::BuildTemplate, template_json_reader::TemplateJsonReader,
    template_json_writer::TemplateJsonWriter,
};

pub use super::template_json_error::{TemplateExportError, TemplateJsonError};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TemplateImportFormat {
    #[default]
    Native,
    Json,
    Adapter,
}

impl TemplateImportFormat {
    pub const fn accepts_raw_text(self) -> bool {
        false
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateImport {
    source_name: String,
    format: TemplateImportFormat,
    template: BuildTemplate,
}

impl TemplateImport {
    pub fn new(
        source_name: impl Into<String>,
        format: TemplateImportFormat,
        template: BuildTemplate,
    ) -> Self {
        Self {
            source_name: source_name.into(),
            format,
            template,
        }
    }
}
impl TemplateImport {
    pub fn source_name(&self) -> &str {
        &self.source_name
    }
}
impl TemplateImport {
    pub fn format(&self) -> TemplateImportFormat {
        self.format
    }
}
impl TemplateImport {
    pub fn template(&self) -> &BuildTemplate {
        &self.template
    }
}
impl TemplateImport {
    pub fn into_template(self) -> BuildTemplate {
        self.template
    }
}
impl TemplateImport {
    pub fn from_json(
        source_name: impl Into<String>,
        input: &str,
    ) -> Result<Self, TemplateJsonError> {
        let value: Value =
            serde_json::from_str(input).map_err(|_| TemplateJsonError::InvalidJson)?;
        let template = TemplateJsonReader::from_value(&value)?;

        Ok(Self::new(source_name, TemplateImportFormat::Json, template))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TemplateExportFormat {
    #[default]
    Native,
    Json,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateExport {
    target_name: String,
    format: TemplateExportFormat,
    template: BuildTemplate,
}

impl TemplateExport {
    pub fn new(
        target_name: impl Into<String>,
        format: TemplateExportFormat,
        template: BuildTemplate,
    ) -> Self {
        Self {
            target_name: target_name.into(),
            format,
            template,
        }
    }
}
impl TemplateExport {
    pub fn target_name(&self) -> &str {
        &self.target_name
    }
}
impl TemplateExport {
    pub fn format(&self) -> TemplateExportFormat {
        self.format
    }
}
impl TemplateExport {
    pub fn template(&self) -> &BuildTemplate {
        &self.template
    }
}
impl TemplateExport {
    pub fn to_json(&self) -> Result<String, TemplateExportError> {
        match self.format {
            TemplateExportFormat::Json => {
                serde_json::to_string_pretty(&TemplateJsonWriter::to_value(&self.template))
                    .map_err(|_| TemplateExportError::JsonSerializationFailed)
            }
            TemplateExportFormat::Native => Err(TemplateExportError::UnsupportedFormat {
                format: self.format,
            }),
        }
    }
}

#[cfg(test)]
#[path = "template_import_tests.rs"]
mod tests;
