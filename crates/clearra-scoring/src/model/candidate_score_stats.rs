use crate::{
    profile::{ScoreAccuracy, ScoreEvaluationScope},
    spin::TraceCompleteness,
};

#[derive(Clone, Debug, PartialEq)]
pub struct CandidateScoreStats {
    candidate_id: usize,
    covered_pattern_count: usize,
    conditional_average_score: Option<u64>,
    unconditional_expected_score: Option<u64>,
    conditional_average_attack: Option<u32>,
    unconditional_expected_attack: Option<u32>,
    min_score: Option<u64>,
    max_score: Option<u64>,
    best_score: Option<u64>,
    best_attack: Option<u32>,
    score_accuracy: ScoreAccuracy,
    trace_completeness: TraceCompleteness,
    evaluation_scope: ScoreEvaluationScope,
}

impl CandidateScoreStats {
    pub fn retained_sample(candidate_id: usize, average_score: u64) -> Self {
        Self {
            candidate_id,
            covered_pattern_count: 0,
            conditional_average_score: Some(average_score),
            unconditional_expected_score: None,
            conditional_average_attack: None,
            unconditional_expected_attack: None,
            min_score: Some(average_score),
            max_score: Some(average_score),
            best_score: Some(average_score),
            best_attack: None,
            score_accuracy: ScoreAccuracy::TraceSampleOnly,
            trace_completeness: TraceCompleteness::RetainedSample,
            evaluation_scope: ScoreEvaluationScope::RetainedTraceSample,
        }
    }
}
impl CandidateScoreStats {
    pub fn universe_expected(
        candidate_id: usize,
        covered_pattern_count: usize,
        score: u64,
    ) -> Self {
        Self {
            candidate_id,
            covered_pattern_count,
            conditional_average_score: Some(score),
            unconditional_expected_score: Some(score),
            conditional_average_attack: None,
            unconditional_expected_attack: None,
            min_score: Some(score),
            max_score: Some(score),
            best_score: Some(score),
            best_attack: None,
            score_accuracy: ScoreAccuracy::PatternComplete,
            trace_completeness: TraceCompleteness::Full,
            evaluation_scope: ScoreEvaluationScope::FullPatternUniverseExpected,
        }
    }
}
impl CandidateScoreStats {
    pub fn candidate_id(&self) -> usize {
        self.candidate_id
    }
}
impl CandidateScoreStats {
    pub fn covered_pattern_count(&self) -> usize {
        self.covered_pattern_count
    }
}
impl CandidateScoreStats {
    pub fn conditional_average_score(&self) -> Option<u64> {
        self.conditional_average_score
    }
}
impl CandidateScoreStats {
    pub fn unconditional_expected_score(&self) -> Option<u64> {
        self.unconditional_expected_score
    }
}
impl CandidateScoreStats {
    pub fn evaluation_scope(&self) -> ScoreEvaluationScope {
        self.evaluation_scope
    }
}
impl CandidateScoreStats {
    pub fn score_accuracy(&self) -> ScoreAccuracy {
        self.score_accuracy
    }
}
impl CandidateScoreStats {
    pub fn trace_completeness(&self) -> TraceCompleteness {
        self.trace_completeness
    }
}
impl CandidateScoreStats {
    pub fn conditional_average_attack(&self) -> Option<u32> {
        self.conditional_average_attack
    }
}
impl CandidateScoreStats {
    pub fn unconditional_expected_attack(&self) -> Option<u32> {
        self.unconditional_expected_attack
    }
}
impl CandidateScoreStats {
    pub fn min_score(&self) -> Option<u64> {
        self.min_score
    }
}
impl CandidateScoreStats {
    pub fn max_score(&self) -> Option<u64> {
        self.max_score
    }
}
impl CandidateScoreStats {
    pub fn best_score(&self) -> Option<u64> {
        self.best_score
    }
}
impl CandidateScoreStats {
    pub fn best_attack(&self) -> Option<u32> {
        self.best_attack
    }
}
