use crate::profile::ScoreEvaluationScope;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScoreExpectationReport {
    evaluation_scope: ScoreEvaluationScope,
    retained_trace_average_score: Option<u64>,
    covered_pattern_conditional_average_score: Option<u64>,
    unconditional_expected_score: Option<u64>,
}

impl ScoreExpectationReport {
    pub fn retained_trace_average(score: u64) -> Self {
        Self {
            evaluation_scope: ScoreEvaluationScope::RetainedTraceSample,
            retained_trace_average_score: Some(score),
            covered_pattern_conditional_average_score: None,
            unconditional_expected_score: None,
        }
    }
}
impl ScoreExpectationReport {
    pub fn full_universe(conditional_average: u64, expected: u64) -> Self {
        Self {
            evaluation_scope: ScoreEvaluationScope::FullPatternUniverseExpected,
            retained_trace_average_score: None,
            covered_pattern_conditional_average_score: Some(conditional_average),
            unconditional_expected_score: Some(expected),
        }
    }
}
impl ScoreExpectationReport {
    pub fn evaluation_scope(&self) -> ScoreEvaluationScope {
        self.evaluation_scope
    }
}
impl ScoreExpectationReport {
    pub fn retained_trace_average_score(&self) -> Option<u64> {
        self.retained_trace_average_score
    }
}
impl ScoreExpectationReport {
    pub fn covered_pattern_conditional_average_score(&self) -> Option<u64> {
        self.covered_pattern_conditional_average_score
    }
}
impl ScoreExpectationReport {
    pub fn unconditional_expected_score(&self) -> Option<u64> {
        self.unconditional_expected_score
    }
}
