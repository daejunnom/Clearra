use clearra_core_domain::probability::probability_value::ProbabilityValue;
use clearra_coverage::{
    pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId},
    probability::union_probability::UnionProbabilityError,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaxScoreCoverPolicy {
    score_weight: f64,
    attack_weight: f64,
}

impl MaxScoreCoverPolicy {
    pub fn new(score_weight: f64, attack_weight: f64) -> Result<Self, MaxScoreCoverPolicyError> {
        if !score_weight.is_finite() || !attack_weight.is_finite() {
            return Err(MaxScoreCoverPolicyError::NonFiniteWeight);
        }
        if score_weight < 0.0 || attack_weight < 0.0 {
            return Err(MaxScoreCoverPolicyError::NegativeWeight);
        }
        if score_weight == 0.0 && attack_weight == 0.0 {
            return Err(MaxScoreCoverPolicyError::EmptyObjective);
        }
        Ok(Self {
            score_weight,
            attack_weight,
        })
    }
}
impl MaxScoreCoverPolicy {
    pub fn score_only() -> Self {
        Self {
            score_weight: 1.0,
            attack_weight: 0.0,
        }
    }
}
impl MaxScoreCoverPolicy {
    pub fn score_weight(self) -> f64 {
        self.score_weight
    }
}
impl MaxScoreCoverPolicy {
    pub fn attack_weight(self) -> f64 {
        self.attack_weight
    }
}
impl MaxScoreCoverPolicy {
    pub(crate) fn candidate_value(self, score: u64, attack: u32) -> f64 {
        self.score_weight * score as f64 + self.attack_weight * attack as f64
    }
}

impl Default for MaxScoreCoverPolicy {
    fn default() -> Self {
        Self::score_only()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaxScoreCoverPolicyError {
    NonFiniteWeight,
    NegativeWeight,
    EmptyObjective,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PatternScoreContribution {
    pattern: PatternId,
    candidate_id: usize,
    probability: ProbabilityValue,
    score: u64,
    attack: u32,
    trace_identity: Option<String>,
    accuracy_level: Option<String>,
}

impl PatternScoreContribution {
    pub(crate) fn new(
        pattern: PatternId,
        candidate_id: usize,
        probability: ProbabilityValue,
        score: u64,
        attack: u32,
    ) -> Self {
        Self {
            pattern,
            candidate_id,
            probability,
            score,
            attack,
            trace_identity: None,
            accuracy_level: None,
        }
    }

    pub(crate) fn from_materialized_cell(
        pattern: PatternId,
        candidate_id: usize,
        probability: ProbabilityValue,
        score: u64,
        attack: u32,
        trace_identity: impl Into<String>,
        accuracy_level: impl Into<String>,
    ) -> Self {
        Self {
            pattern,
            candidate_id,
            probability,
            score,
            attack,
            trace_identity: Some(trace_identity.into()),
            accuracy_level: Some(accuracy_level.into()),
        }
    }
}
impl PatternScoreContribution {
    pub fn pattern(&self) -> PatternId {
        self.pattern
    }
}
impl PatternScoreContribution {
    pub fn candidate_id(&self) -> usize {
        self.candidate_id
    }
}
impl PatternScoreContribution {
    pub fn probability(&self) -> ProbabilityValue {
        self.probability
    }
}
impl PatternScoreContribution {
    pub fn score(&self) -> u64 {
        self.score
    }
}
impl PatternScoreContribution {
    pub fn attack(&self) -> u32 {
        self.attack
    }
}
impl PatternScoreContribution {
    pub fn trace_identity(&self) -> Option<&str> {
        self.trace_identity.as_deref()
    }

    pub fn accuracy_level(&self) -> Option<&str> {
        self.accuracy_level.as_deref()
    }
}
impl PatternScoreContribution {
    pub fn expected_score(&self) -> f64 {
        self.probability.get() * self.score as f64
    }
}
impl PatternScoreContribution {
    pub fn expected_attack(&self) -> f64 {
        self.probability.get() * self.attack as f64
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaxScoreCoverResult {
    selected_candidate_ids: Vec<usize>,
    covered_patterns: PatternBitSet,
    covered_probability: ProbabilityValue,
    expected_score: f64,
    expected_attack: f64,
    complete: bool,
    pattern_contributions: Vec<PatternScoreContribution>,
}

impl MaxScoreCoverResult {
    pub(crate) fn new(
        selected_candidate_ids: Vec<usize>,
        covered_patterns: PatternBitSet,
        covered_probability: ProbabilityValue,
        complete: bool,
        pattern_contributions: Vec<PatternScoreContribution>,
    ) -> Self {
        let expected_score = pattern_contributions
            .iter()
            .map(PatternScoreContribution::expected_score)
            .sum();
        let expected_attack = pattern_contributions
            .iter()
            .map(PatternScoreContribution::expected_attack)
            .sum();

        Self {
            selected_candidate_ids,
            covered_patterns,
            covered_probability,
            expected_score,
            expected_attack,
            complete,
            pattern_contributions,
        }
    }
}
impl MaxScoreCoverResult {
    pub fn selected_candidate_ids(&self) -> &[usize] {
        &self.selected_candidate_ids
    }
}
impl MaxScoreCoverResult {
    pub fn covered_patterns(&self) -> &PatternBitSet {
        &self.covered_patterns
    }
}
impl MaxScoreCoverResult {
    pub fn covered_probability(&self) -> ProbabilityValue {
        self.covered_probability
    }
}
impl MaxScoreCoverResult {
    pub fn expected_score(&self) -> f64 {
        self.expected_score
    }
}
impl MaxScoreCoverResult {
    pub fn expected_attack(&self) -> f64 {
        self.expected_attack
    }
}
impl MaxScoreCoverResult {
    pub fn complete(&self) -> bool {
        self.complete
    }
}
impl MaxScoreCoverResult {
    pub fn pattern_contributions(&self) -> &[PatternScoreContribution] {
        &self.pattern_contributions
    }
}
impl MaxScoreCoverResult {
    pub fn best_score_by_pattern(&self) -> &[PatternScoreContribution] {
        &self.pattern_contributions
    }
}
impl MaxScoreCoverResult {
    pub fn best_score_by_pattern_count(&self) -> usize {
        self.pattern_contributions.len()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MaxScoreCoverError {
    ScoreMatrixIncomplete,
    ScoreMatrixPatternUniverseMismatch {
        expected: usize,
        actual: usize,
    },
    ScoreCellPatternOutOfRange {
        pattern_index: usize,
        pattern_count: usize,
    },
    CandidatePatternUniverseMismatch {
        expected: usize,
        actual: usize,
    },
    RequiredPatternUniverseMismatch {
        expected: usize,
        actual: usize,
    },
    PatternBitSetWordCapacityExceeded {
        word_count: usize,
        word_limit: usize,
    },
    PatternBitSetWordCountMismatch {
        expected: usize,
        actual: usize,
    },
    Probability(UnionProbabilityError),
}
