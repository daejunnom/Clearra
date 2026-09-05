use crate::{event::score_event::ScoreEvent, state::score_state::ScoreState};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScoreEvaluationBasis {
    AllTraces,
    #[default]
    RetainedTraces,
    Sample,
}

impl ScoreEvaluationBasis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AllTraces => "all-traces",
            Self::RetainedTraces => "retained-traces",
            Self::Sample => "sample",
        }
    }
}
impl ScoreEvaluationBasis {
    pub fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Sample, _) | (_, Self::Sample) => Self::Sample,
            (Self::RetainedTraces, _) | (_, Self::RetainedTraces) => Self::RetainedTraces,
            (Self::AllTraces, Self::AllTraces) => Self::AllTraces,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreEvaluation {
    profile_id: String,
    final_state: ScoreState,
    events: Vec<ScoreEvent>,
}

impl ScoreEvaluation {
    pub fn new(
        profile_id: impl Into<String>,
        final_state: ScoreState,
        events: Vec<ScoreEvent>,
    ) -> Self {
        Self {
            profile_id: profile_id.into(),
            final_state,
            events,
        }
    }
}
impl ScoreEvaluation {
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }
}
impl ScoreEvaluation {
    pub fn final_state(&self) -> ScoreState {
        self.final_state
    }
}
impl ScoreEvaluation {
    pub fn events(&self) -> &[ScoreEvent] {
        &self.events
    }
}
impl ScoreEvaluation {
    pub fn event_count(&self) -> usize {
        self.events.len()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScoreEvaluationSummary {
    profile_id: String,
    best_score: u64,
    best_attack: u32,
    evaluated_trace_count: usize,
    evaluation_complete: bool,
    evaluation_basis: ScoreEvaluationBasis,
}

impl ScoreEvaluationSummary {
    pub fn none() -> Self {
        Self::default()
    }
}
impl ScoreEvaluationSummary {
    pub fn new(
        profile_id: impl Into<String>,
        best_score: u64,
        best_attack: u32,
        evaluated_trace_count: usize,
        evaluation_complete: bool,
        evaluation_basis: ScoreEvaluationBasis,
    ) -> Self {
        Self {
            profile_id: profile_id.into(),
            best_score,
            best_attack,
            evaluated_trace_count,
            evaluation_complete,
            evaluation_basis,
        }
    }
}
impl ScoreEvaluationSummary {
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }
}
impl ScoreEvaluationSummary {
    pub fn best_score(&self) -> u64 {
        self.best_score
    }
}
impl ScoreEvaluationSummary {
    pub fn best_attack(&self) -> u32 {
        self.best_attack
    }
}
impl ScoreEvaluationSummary {
    pub fn evaluated_trace_count(&self) -> usize {
        self.evaluated_trace_count
    }
}
impl ScoreEvaluationSummary {
    pub fn evaluation_complete(&self) -> bool {
        self.evaluation_complete
    }
}
impl ScoreEvaluationSummary {
    pub fn evaluation_basis(&self) -> ScoreEvaluationBasis {
        self.evaluation_basis
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_evaluation_summary_discloses_trace_basis_and_completeness() {
        let summary = ScoreEvaluationSummary::new(
            "profile",
            1200,
            4,
            64,
            false,
            ScoreEvaluationBasis::Sample,
        );

        assert_eq!(summary.profile_id(), "profile");
        assert_eq!(summary.best_score(), 1200);
        assert_eq!(summary.best_attack(), 4);
        assert_eq!(summary.evaluated_trace_count(), 64);
        assert!(!summary.evaluation_complete());
        assert_eq!(summary.evaluation_basis().as_str(), "sample");
    }
}
