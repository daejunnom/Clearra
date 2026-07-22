use clearra_i18n::TranslationKey;

use crate::i18n::LocalizedLabelSchema;

use super::setup_result_column_schema::SetupResultColumnType;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinProbabilityColumnSchema {
    id: &'static str,
    label: &'static str,
    localized_label: LocalizedLabelSchema,
    column_type: SetupResultColumnType,
    json_contract_key: &'static str,
}

impl SpinProbabilityColumnSchema {
    pub fn new(
        id: &'static str,
        label: &'static str,
        column_type: SetupResultColumnType,
        json_contract_key: &'static str,
    ) -> Self {
        Self {
            id,
            label,
            localized_label: LocalizedLabelSchema::new(
                TranslationKey::new(format!("ui.spin_probability.column.{id}.label")),
                label,
            ),
            column_type,
            json_contract_key,
        }
    }
}
impl SpinProbabilityColumnSchema {
    pub fn id(&self) -> &'static str {
        self.id
    }
}
impl SpinProbabilityColumnSchema {
    pub fn label(&self) -> &'static str {
        self.label
    }
}
impl SpinProbabilityColumnSchema {
    pub fn localized_label(&self) -> &LocalizedLabelSchema {
        &self.localized_label
    }
}
impl SpinProbabilityColumnSchema {
    pub fn column_type(&self) -> SetupResultColumnType {
        self.column_type
    }
}
impl SpinProbabilityColumnSchema {
    pub fn json_contract_key(&self) -> &'static str {
        self.json_contract_key
    }
}

pub(crate) fn spin_probability_columns() -> Vec<SpinProbabilityColumnSchema> {
    use SetupResultColumnType::{Boolean, Float, Integer, Probability, Text};

    vec![
        SpinProbabilityColumnSchema::new(
            "spin_target_id",
            "Spin target id",
            Text,
            "spin_target_id",
        ),
        SpinProbabilityColumnSchema::new(
            "spin_target_name",
            "Spin target",
            Text,
            "spin_target_name",
        ),
        SpinProbabilityColumnSchema::new(
            "spin_probability",
            "Spin probability",
            Probability,
            "probability",
        ),
        SpinProbabilityColumnSchema::new(
            "covered_pattern_count",
            "Covered patterns",
            Integer,
            "covered_pattern_count",
        ),
        SpinProbabilityColumnSchema::new("pattern_count", "Patterns", Integer, "pattern_count"),
        SpinProbabilityColumnSchema::new(
            "pattern_universe_id",
            "Pattern universe",
            Text,
            "pattern_universe_id",
        ),
        SpinProbabilityColumnSchema::new(
            "pattern_weight_model_id",
            "Weight model",
            Text,
            "pattern_weight_model_id",
        ),
        SpinProbabilityColumnSchema::new(
            "probability_complete",
            "Probability complete",
            Boolean,
            "probability_complete",
        ),
        SpinProbabilityColumnSchema::new(
            "materialized_probability_mass",
            "Materialized mass",
            Float,
            "materialized_probability_mass",
        ),
        SpinProbabilityColumnSchema::new("renormalized", "Renormalized", Boolean, "renormalized"),
        SpinProbabilityColumnSchema::new(
            "truncation_reason",
            "Truncation reason",
            Text,
            "truncation_reason",
        ),
        SpinProbabilityColumnSchema::new("spin_accuracy", "Spin accuracy", Text, "spin_accuracy"),
        SpinProbabilityColumnSchema::new(
            "trace_completeness",
            "Trace completeness",
            Text,
            "trace_completeness",
        ),
        SpinProbabilityColumnSchema::new(
            "score_profile_id",
            "Score profile",
            Text,
            "score_profile_id",
        ),
        SpinProbabilityColumnSchema::new(
            "special_spin_disabled_reason",
            "Special spin reason",
            Text,
            "special_spin_disabled_reason",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_schema_does_not_localize_json_contract_keys() {
        let columns = spin_probability_columns();
        let probability = columns
            .iter()
            .find(|column| column.id() == "spin_probability")
            .expect("spin probability column");

        assert_eq!(probability.json_contract_key(), "probability");
        assert_eq!(
            probability.localized_label().key().as_str(),
            "ui.spin_probability.column.spin_probability.label"
        );
        assert_ne!(
            probability.json_contract_key(),
            probability.localized_label().key().as_str()
        );
    }
}
