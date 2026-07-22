use crate::spin::TraceCompleteness;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScoreBasis {
    #[default]
    RetainedTrace,
    PatternComplete,
    Estimated,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PatternScoreContribution {
    pattern_id: usize,
    candidate_id: usize,
    score: u64,
    attack: u32,
    score_basis: ScoreBasis,
    trace_completeness: TraceCompleteness,
}

impl PatternScoreContribution {
    pub fn new(pattern_id: usize, candidate_id: usize, score: u64, attack: u32) -> Self {
        Self {
            pattern_id,
            candidate_id,
            score,
            attack,
            score_basis: ScoreBasis::PatternComplete,
            trace_completeness: TraceCompleteness::Full,
        }
    }
}
impl PatternScoreContribution {
    pub fn with_basis(mut self, score_basis: ScoreBasis) -> Self {
        self.score_basis = score_basis;
        self
    }
}
impl PatternScoreContribution {
    pub fn pattern_id(&self) -> usize {
        self.pattern_id
    }
}
impl PatternScoreContribution {
    pub fn candidate_id(&self) -> usize {
        self.candidate_id
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
    pub fn score_basis(&self) -> ScoreBasis {
        self.score_basis
    }
}
impl PatternScoreContribution {
    pub fn trace_completeness(&self) -> TraceCompleteness {
        self.trace_completeness
    }
}
