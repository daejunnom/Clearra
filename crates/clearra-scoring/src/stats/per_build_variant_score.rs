use crate::{profile::ScoreAccuracy, spin::TraceCompleteness};

#[derive(Clone, Debug, PartialEq)]
pub struct PerBuildVariantScore {
    build_variant_id: String,
    candidate_id: usize,
    pattern_id: usize,
    score: u64,
    attack: u32,
    score_accuracy: ScoreAccuracy,
    trace_completeness: TraceCompleteness,
}

impl PerBuildVariantScore {
    pub fn new(
        build_variant_id: impl Into<String>,
        candidate_id: usize,
        pattern_id: usize,
        score: u64,
        attack: u32,
        score_accuracy: ScoreAccuracy,
        trace_completeness: TraceCompleteness,
    ) -> Self {
        Self {
            build_variant_id: build_variant_id.into(),
            candidate_id,
            pattern_id,
            score,
            attack,
            score_accuracy,
            trace_completeness,
        }
    }
}
impl PerBuildVariantScore {
    pub fn build_variant_id(&self) -> &str {
        &self.build_variant_id
    }
}
impl PerBuildVariantScore {
    pub fn candidate_id(&self) -> usize {
        self.candidate_id
    }
}
impl PerBuildVariantScore {
    pub fn pattern_id(&self) -> usize {
        self.pattern_id
    }
}
impl PerBuildVariantScore {
    pub fn score(&self) -> u64 {
        self.score
    }
}
impl PerBuildVariantScore {
    pub fn attack(&self) -> u32 {
        self.attack
    }
}
impl PerBuildVariantScore {
    pub fn score_accuracy(&self) -> ScoreAccuracy {
        self.score_accuracy
    }
}
impl PerBuildVariantScore {
    pub fn trace_completeness(&self) -> TraceCompleteness {
        self.trace_completeness
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_build_variant_score_keeps_score_on_replay_variant_not_coverage_row() {
        let score = PerBuildVariantScore::new(
            "bvk1:abcd",
            7,
            3,
            4_200,
            12,
            ScoreAccuracy::PatternComplete,
            TraceCompleteness::Full,
        );

        assert_eq!(score.build_variant_id(), "bvk1:abcd");
        assert_eq!(score.candidate_id(), 7);
        assert_eq!(score.pattern_id(), 3);
        assert_eq!(score.score(), 4_200);
        assert_eq!(score.attack(), 12);
    }
}
