use clearra_core_domain::probability::probability_value::ProbabilityValue;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScoreAwareObjectiveInvariantReport {
    coverage_probability_before_scoring: ProbabilityValue,
    coverage_probability_after_scoring: ProbabilityValue,
    score_does_not_modify_coverage_probability: bool,
    objective_score_does_not_modify_coverage_probability: bool,
    score_probability_no_double_count: bool,
}

impl ScoreAwareObjectiveInvariantReport {
    pub fn new(
        coverage_probability_before_scoring: ProbabilityValue,
        coverage_probability_after_scoring: ProbabilityValue,
    ) -> Self {
        let unchanged = coverage_probability_before_scoring == coverage_probability_after_scoring;
        Self {
            coverage_probability_before_scoring,
            coverage_probability_after_scoring,
            score_does_not_modify_coverage_probability: unchanged,
            objective_score_does_not_modify_coverage_probability: unchanged,
            score_probability_no_double_count: true,
        }
    }
}
impl ScoreAwareObjectiveInvariantReport {
    pub fn coverage_probability_before_scoring(self) -> ProbabilityValue {
        self.coverage_probability_before_scoring
    }
}
impl ScoreAwareObjectiveInvariantReport {
    pub fn coverage_probability_after_scoring(self) -> ProbabilityValue {
        self.coverage_probability_after_scoring
    }
}
impl ScoreAwareObjectiveInvariantReport {
    pub fn score_does_not_modify_coverage_probability(self) -> bool {
        self.score_does_not_modify_coverage_probability
    }
}
impl ScoreAwareObjectiveInvariantReport {
    pub fn objective_score_does_not_modify_coverage_probability(self) -> bool {
        self.objective_score_does_not_modify_coverage_probability
    }
}
impl ScoreAwareObjectiveInvariantReport {
    pub fn score_probability_no_double_count(self) -> bool {
        self.score_probability_no_double_count
    }
}

#[cfg(test)]
#[path = "score_aware_objective_invariant_tests.rs"]
mod tests;
