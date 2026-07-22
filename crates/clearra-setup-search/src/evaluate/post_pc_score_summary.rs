use clearra_scoring::model::score_evaluation::ScoreEvaluationSummary;

pub use clearra_scoring::model::ScoreEvaluationBasis;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PostPcScoreSummary {
    summary: ScoreEvaluationSummary,
}

impl PostPcScoreSummary {
    pub fn none() -> Self {
        Self::default()
    }
}
impl PostPcScoreSummary {
    pub fn new(
        profile_id: impl Into<String>,
        best_score: u64,
        best_attack: u32,
        score_evaluation_trace_count: usize,
        score_evaluation_complete: bool,
        score_evaluation_basis: ScoreEvaluationBasis,
    ) -> Self {
        Self {
            summary: ScoreEvaluationSummary::new(
                profile_id,
                best_score,
                best_attack,
                score_evaluation_trace_count,
                score_evaluation_complete,
                score_evaluation_basis,
            ),
        }
    }
}
impl PostPcScoreSummary {
    pub fn from_summary(summary: ScoreEvaluationSummary) -> Self {
        Self { summary }
    }
}
impl PostPcScoreSummary {
    pub fn evaluation_summary(&self) -> &ScoreEvaluationSummary {
        &self.summary
    }
}
impl PostPcScoreSummary {
    pub fn profile_id(&self) -> &str {
        self.summary.profile_id()
    }
}
impl PostPcScoreSummary {
    pub fn best_score(&self) -> u64 {
        self.summary.best_score()
    }
}
impl PostPcScoreSummary {
    pub fn best_attack(&self) -> u32 {
        self.summary.best_attack()
    }
}
impl PostPcScoreSummary {
    pub fn score_evaluation_trace_count(&self) -> usize {
        self.summary.evaluated_trace_count()
    }
}
impl PostPcScoreSummary {
    pub fn score_evaluation_complete(&self) -> bool {
        self.summary.evaluation_complete()
    }
}
impl PostPcScoreSummary {
    pub fn score_evaluation_basis(&self) -> ScoreEvaluationBasis {
        self.summary.evaluation_basis()
    }
}
impl PostPcScoreSummary {
    pub fn evaluated_trace_count(&self) -> usize {
        self.summary.evaluated_trace_count()
    }
}
