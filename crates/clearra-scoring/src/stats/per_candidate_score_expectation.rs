#[derive(Clone, Debug, PartialEq)]
pub struct PerCandidateConditionalAverage {
    candidate_id: usize,
    covered_pattern_count: usize,
    average_score_over_covered_patterns: f64,
    average_attack_over_covered_patterns: f64,
    sample_vs_full_evaluation_distinguished: bool,
}

impl PerCandidateConditionalAverage {
    pub fn new(
        candidate_id: usize,
        covered_pattern_count: usize,
        average_score_over_covered_patterns: f64,
        average_attack_over_covered_patterns: f64,
        sample_vs_full_evaluation_distinguished: bool,
    ) -> Self {
        Self {
            candidate_id,
            covered_pattern_count,
            average_score_over_covered_patterns,
            average_attack_over_covered_patterns,
            sample_vs_full_evaluation_distinguished,
        }
    }
}
impl PerCandidateConditionalAverage {
    pub fn candidate_id(&self) -> usize {
        self.candidate_id
    }
}
impl PerCandidateConditionalAverage {
    pub fn covered_pattern_count(&self) -> usize {
        self.covered_pattern_count
    }
}
impl PerCandidateConditionalAverage {
    pub fn average_score_over_covered_patterns(&self) -> f64 {
        self.average_score_over_covered_patterns
    }
}
impl PerCandidateConditionalAverage {
    pub fn average_attack_over_covered_patterns(&self) -> f64 {
        self.average_attack_over_covered_patterns
    }
}
impl PerCandidateConditionalAverage {
    pub fn sample_vs_full_evaluation_distinguished(&self) -> bool {
        self.sample_vs_full_evaluation_distinguished
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PerCandidateUnconditionalExpectation {
    candidate_id: usize,
    expected_score_over_full_universe: f64,
    expected_attack_over_full_universe: f64,
    probability_mass: f64,
}

impl PerCandidateUnconditionalExpectation {
    pub fn new(
        candidate_id: usize,
        expected_score_over_full_universe: f64,
        expected_attack_over_full_universe: f64,
        probability_mass: f64,
    ) -> Self {
        Self {
            candidate_id,
            expected_score_over_full_universe,
            expected_attack_over_full_universe,
            probability_mass,
        }
    }
}
impl PerCandidateUnconditionalExpectation {
    pub fn candidate_id(&self) -> usize {
        self.candidate_id
    }
}
impl PerCandidateUnconditionalExpectation {
    pub fn expected_score_over_full_universe(&self) -> f64 {
        self.expected_score_over_full_universe
    }
}
impl PerCandidateUnconditionalExpectation {
    pub fn expected_attack_over_full_universe(&self) -> f64 {
        self.expected_attack_over_full_universe
    }
}
impl PerCandidateUnconditionalExpectation {
    pub fn probability_mass(&self) -> f64 {
        self.probability_mass
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_vs_full_evaluation_distinguished() {
        let conditional = PerCandidateConditionalAverage::new(2, 3, 100.0, 4.0, true);
        let unconditional = PerCandidateUnconditionalExpectation::new(2, 25.0, 1.0, 0.25);

        assert!(conditional.sample_vs_full_evaluation_distinguished());
        assert_eq!(conditional.average_score_over_covered_patterns(), 100.0);
        assert_eq!(unconditional.expected_score_over_full_universe(), 25.0);
        assert_eq!(unconditional.probability_mass(), 0.25);
    }
}
