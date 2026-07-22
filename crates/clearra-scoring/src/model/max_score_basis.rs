use super::pattern_score_contribution::PatternScoreContribution;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaxScoreBasis {
    best_score_by_pattern: Vec<PatternScoreContribution>,
}

impl MaxScoreBasis {
    pub fn new(best_score_by_pattern: Vec<PatternScoreContribution>) -> Self {
        Self {
            best_score_by_pattern,
        }
    }
}
impl MaxScoreBasis {
    pub fn best_score_by_pattern(&self) -> &[PatternScoreContribution] {
        &self.best_score_by_pattern
    }
}
impl MaxScoreBasis {
    pub fn best_score_by_pattern_count(&self) -> usize {
        self.best_score_by_pattern.len()
    }
}
