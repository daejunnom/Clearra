use crate::profile::{ScoreAccuracy, ScoreEvaluationScope};

#[derive(Clone, Debug, PartialEq)]
pub struct AverageScoreReport {
    candidate_count: usize,
    evaluated_build_variant_count: usize,
    average_score: Option<f64>,
    average_attack: Option<f64>,
    evaluation_scope: ScoreEvaluationScope,
    score_accuracy: ScoreAccuracy,
    sample_vs_full_evaluation_distinguished: bool,
}

impl AverageScoreReport {
    pub fn retained_sample(
        candidate_count: usize,
        evaluated_build_variant_count: usize,
        average_score: f64,
    ) -> Self {
        Self {
            candidate_count,
            evaluated_build_variant_count,
            average_score: Some(average_score),
            average_attack: None,
            evaluation_scope: ScoreEvaluationScope::RetainedTraceSample,
            score_accuracy: ScoreAccuracy::TraceSampleOnly,
            sample_vs_full_evaluation_distinguished: true,
        }
    }
}
impl AverageScoreReport {
    pub fn full_universe(
        candidate_count: usize,
        evaluated_build_variant_count: usize,
        average_score: f64,
        average_attack: f64,
    ) -> Self {
        Self {
            candidate_count,
            evaluated_build_variant_count,
            average_score: Some(average_score),
            average_attack: Some(average_attack),
            evaluation_scope: ScoreEvaluationScope::FullPatternUniverseExpected,
            score_accuracy: ScoreAccuracy::PatternComplete,
            sample_vs_full_evaluation_distinguished: true,
        }
    }
}
impl AverageScoreReport {
    pub fn candidate_count(&self) -> usize {
        self.candidate_count
    }
}
impl AverageScoreReport {
    pub fn evaluated_build_variant_count(&self) -> usize {
        self.evaluated_build_variant_count
    }
}
impl AverageScoreReport {
    pub fn average_score(&self) -> Option<f64> {
        self.average_score
    }
}
impl AverageScoreReport {
    pub fn average_attack(&self) -> Option<f64> {
        self.average_attack
    }
}
impl AverageScoreReport {
    pub fn evaluation_scope(&self) -> ScoreEvaluationScope {
        self.evaluation_scope
    }
}
impl AverageScoreReport {
    pub fn score_accuracy(&self) -> ScoreAccuracy {
        self.score_accuracy
    }
}
impl AverageScoreReport {
    pub fn sample_vs_full_evaluation_distinguished(&self) -> bool {
        self.sample_vs_full_evaluation_distinguished
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_trace_sample_is_not_reported_as_full_expected_score() {
        let report = AverageScoreReport::retained_sample(4, 2, 1_000.0);

        assert_eq!(
            report.evaluation_scope(),
            ScoreEvaluationScope::RetainedTraceSample
        );
        assert_eq!(report.score_accuracy(), ScoreAccuracy::TraceSampleOnly);
        assert!(report.sample_vs_full_evaluation_distinguished());
    }
}
