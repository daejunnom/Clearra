use clearra_i18n::TranslationKey;
use clearra_scoring::profile::ScoreEvaluationScope;

use crate::{dropdown::DropdownOption, i18n::LocalizedLabelSchema};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreExpectationSchema {
    aggregation_scope_options: Vec<ScoreEvaluationScopeOptionSchema>,
    result_contract_keys: Vec<&'static str>,
}

impl ScoreExpectationSchema {
    pub fn mvp2() -> Self {
        Self {
            aggregation_scope_options: score_aggregation_scope_options(),
            result_contract_keys: vec![
                "score_accuracy",
                "trace_completeness",
                "evaluation_scope",
                "retained_trace_average_score",
                "covered_pattern_conditional_average_score",
                "unconditional_expected_score",
                "score_does_not_change_probability_union",
            ],
        }
    }
}
impl ScoreExpectationSchema {
    pub fn aggregation_scope_options(&self) -> &[ScoreEvaluationScopeOptionSchema] {
        &self.aggregation_scope_options
    }
}
impl ScoreExpectationSchema {
    pub fn result_contract_keys(&self) -> &[&'static str] {
        &self.result_contract_keys
    }
}
impl ScoreExpectationSchema {
    pub fn dropdown_options(&self) -> Vec<DropdownOption> {
        self.aggregation_scope_options
            .iter()
            .map(|option| {
                DropdownOption::new(option.id(), option.label())
                    .with_localized_label(option.localized_label().clone())
            })
            .collect()
    }
}

impl Default for ScoreExpectationSchema {
    fn default() -> Self {
        Self::mvp2()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreEvaluationScopeOptionSchema {
    id: &'static str,
    label: &'static str,
    localized_label: LocalizedLabelSchema,
    scope: ScoreEvaluationScope,
    probability_scope: &'static str,
}

impl ScoreEvaluationScopeOptionSchema {
    pub fn new(
        id: &'static str,
        label: &'static str,
        scope: ScoreEvaluationScope,
        probability_scope: &'static str,
    ) -> Self {
        Self {
            id,
            label,
            localized_label: LocalizedLabelSchema::new(
                TranslationKey::new(format!("ui.score.expectation.{id}.label")),
                label,
            ),
            scope,
            probability_scope,
        }
    }
}
impl ScoreEvaluationScopeOptionSchema {
    pub fn id(&self) -> &'static str {
        self.id
    }
}
impl ScoreEvaluationScopeOptionSchema {
    pub fn label(&self) -> &'static str {
        self.label
    }
}
impl ScoreEvaluationScopeOptionSchema {
    pub fn localized_label(&self) -> &LocalizedLabelSchema {
        &self.localized_label
    }
}
impl ScoreEvaluationScopeOptionSchema {
    pub fn scope(&self) -> ScoreEvaluationScope {
        self.scope
    }
}
impl ScoreEvaluationScopeOptionSchema {
    pub fn probability_scope(&self) -> &'static str {
        self.probability_scope
    }
}

fn score_aggregation_scope_options() -> Vec<ScoreEvaluationScopeOptionSchema> {
    vec![
        ScoreEvaluationScopeOptionSchema::new(
            "retained-trace-sample",
            "Retained trace sample",
            ScoreEvaluationScope::RetainedTraceSample,
            "sample-only",
        ),
        ScoreEvaluationScopeOptionSchema::new(
            "covered-patterns-conditional",
            "Covered patterns conditional",
            ScoreEvaluationScope::CoveredPatternsConditional,
            "covered-patterns",
        ),
        ScoreEvaluationScopeOptionSchema::new(
            "full-universe-expected",
            "Full universe expected",
            ScoreEvaluationScope::FullPatternUniverseExpected,
            "full-pattern-universe",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_schema_distinguishes_score_evaluation_scope() {
        let schema = ScoreExpectationSchema::mvp2();
        let scopes = schema
            .aggregation_scope_options()
            .iter()
            .map(ScoreEvaluationScopeOptionSchema::scope)
            .collect::<Vec<_>>();
        let probability_scopes = schema
            .aggregation_scope_options()
            .iter()
            .map(ScoreEvaluationScopeOptionSchema::probability_scope)
            .collect::<Vec<_>>();

        assert_eq!(
            scopes,
            [
                ScoreEvaluationScope::RetainedTraceSample,
                ScoreEvaluationScope::CoveredPatternsConditional,
                ScoreEvaluationScope::FullPatternUniverseExpected
            ]
        );
        assert_eq!(
            probability_scopes,
            ["sample-only", "covered-patterns", "full-pattern-universe"]
        );
        assert!(schema
            .result_contract_keys()
            .contains(&"unconditional_expected_score"));
        assert!(schema
            .result_contract_keys()
            .contains(&"retained_trace_average_score"));
    }
}
