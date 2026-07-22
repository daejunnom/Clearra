use clearra_coverage::universe::{
    pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
};

use super::JsonValue;

#[derive(Clone, Debug, PartialEq)]
pub struct ProbabilityResultContract {
    pattern_universe_id: PatternUniverseId,
    pattern_weight_model_id: PatternWeightModelId,
    pattern_count: usize,
    covered_pattern_count: usize,
    coverage_probability: f64,
    probability_complete: bool,
    materialized_pattern_count: usize,
    total_possible_pattern_count_or_unknown: Option<usize>,
    materialized_probability_mass: f64,
    renormalized: bool,
    truncation_reason: Option<String>,
}

impl ProbabilityResultContract {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        pattern_count: usize,
        covered_pattern_count: usize,
        coverage_probability: f64,
        probability_complete: bool,
        materialized_pattern_count: usize,
        total_possible_pattern_count_or_unknown: Option<usize>,
        materialized_probability_mass: f64,
        renormalized: bool,
        truncation_reason: Option<String>,
    ) -> Self {
        Self {
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count,
            covered_pattern_count,
            coverage_probability,
            probability_complete,
            materialized_pattern_count,
            total_possible_pattern_count_or_unknown,
            materialized_probability_mass,
            renormalized,
            truncation_reason,
        }
    }
}
impl ProbabilityResultContract {
    pub fn to_json(&self) -> JsonValue {
        JsonValue::object([
            ("kind", JsonValue::string("ProbabilityResult")),
            (
                "probability_basis",
                JsonValue::string("PatternBitSet union"),
            ),
            (
                "pattern_universe_id",
                JsonValue::number(self.pattern_universe_id.get().to_string()),
            ),
            (
                "pattern_weight_model_id",
                JsonValue::number(self.pattern_weight_model_id.get().to_string()),
            ),
            (
                "pattern_count",
                JsonValue::number(self.pattern_count.to_string()),
            ),
            (
                "covered_pattern_count",
                JsonValue::number(self.covered_pattern_count.to_string()),
            ),
            (
                "coverage_probability",
                JsonValue::number(format_float(self.coverage_probability)),
            ),
            (
                "probability_complete",
                JsonValue::Bool(self.probability_complete),
            ),
            (
                "materialized_pattern_count",
                JsonValue::number(self.materialized_pattern_count.to_string()),
            ),
            (
                "total_possible_pattern_count_or_unknown",
                self.total_possible_pattern_count_or_unknown
                    .map(|count| JsonValue::number(count.to_string()))
                    .unwrap_or(JsonValue::Null),
            ),
            (
                "materialized_probability_mass",
                JsonValue::number(format_float(self.materialized_probability_mass)),
            ),
            ("renormalized", JsonValue::Bool(self.renormalized)),
            (
                "truncation_reason",
                self.truncation_reason
                    .as_ref()
                    .map(|reason| JsonValue::string(reason.clone()))
                    .unwrap_or(JsonValue::Null),
            ),
            ("score_does_not_modify_probability", JsonValue::Bool(true)),
        ])
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoverageContractJson;

impl CoverageContractJson {
    pub fn probability_result(result: &ProbabilityResultContract) -> JsonValue {
        result.to_json()
    }
}

fn format_float(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        value.to_string()
    }
}

#[cfg(test)]
#[path = "coverage_contract_tests.rs"]
mod tests;
