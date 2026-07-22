use clearra_i18n::TranslationKey;

use crate::i18n::LocalizedLabelSchema;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupResultColumnSchema {
    id: String,
    label: String,
    localized_label: LocalizedLabelSchema,
    column_type: SetupResultColumnType,
    source: SetupResultColumnSource,
}

impl SetupResultColumnSchema {
    pub fn new(
        id: &'static str,
        label: &'static str,
        column_type: SetupResultColumnType,
        source: SetupResultColumnSource,
    ) -> Self {
        Self {
            id: id.to_owned(),
            label: label.to_owned(),
            localized_label: LocalizedLabelSchema::new(TranslationKey::ui_setup_result(id), label),
            column_type,
            source,
        }
    }
}
impl SetupResultColumnSchema {
    pub fn with_localized_label(mut self, localized_label: LocalizedLabelSchema) -> Self {
        self.localized_label = localized_label;
        self
    }
}
impl SetupResultColumnSchema {
    pub fn id(&self) -> &str {
        &self.id
    }
}
impl SetupResultColumnSchema {
    pub fn label(&self) -> &str {
        &self.label
    }
}
impl SetupResultColumnSchema {
    pub fn localized_label(&self) -> &LocalizedLabelSchema {
        &self.localized_label
    }
}
impl SetupResultColumnSchema {
    pub fn column_type(&self) -> SetupResultColumnType {
        self.column_type
    }
}
impl SetupResultColumnSchema {
    pub fn source(&self) -> SetupResultColumnSource {
        self.source
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupResultColumnType {
    Text,
    Probability,
    Integer,
    Float,
    Boolean,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupResultColumnSource {
    SetupFamily,
    TilingVariant,
    BuildVariant,
    BuildVariantMetrics,
    SetupRawMetrics,
    DiagnosticEvidence,
    PostPcEvaluation,
    ScoreAggregation,
    Continuation,
}

pub(crate) fn column(
    id: &'static str,
    label: &'static str,
    column_type: SetupResultColumnType,
    source: SetupResultColumnSource,
) -> SetupResultColumnSchema {
    SetupResultColumnSchema::new(id, label, column_type, source)
}

#[cfg(test)]
mod tests {
    use clearra_i18n::{LanguageId, TranslationCatalog};

    use super::*;

    #[test]
    fn result_column_keeps_contract_id_and_localized_label_separate() {
        let column = column(
            "total_solution_count",
            "Solutions",
            SetupResultColumnType::Integer,
            SetupResultColumnSource::PostPcEvaluation,
        );

        assert_eq!(column.id(), "total_solution_count");
        assert_eq!(
            column.localized_label().key().as_str(),
            "ui.setup.result.total_solution_count"
        );
        assert_eq!(
            column
                .localized_label()
                .resolve(TranslationCatalog::new(LanguageId::Ko))
                .text(),
            "전체 해법 수"
        );
    }
}
