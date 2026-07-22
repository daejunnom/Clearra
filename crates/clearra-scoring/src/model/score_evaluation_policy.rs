#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SameShapeScorePolicy {
    #[default]
    HighestLegalTrace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoreEvaluationPolicy {
    initial_b2b: u32,
    include_drop_score: bool,
    level_multiplier: u32,
    same_shape: SameShapeScorePolicy,
}

impl ScoreEvaluationPolicy {
    pub const fn profile_defaults() -> Self {
        Self {
            initial_b2b: 0,
            include_drop_score: true,
            level_multiplier: 1,
            same_shape: SameShapeScorePolicy::HighestLegalTrace,
        }
    }

    pub const fn tetrio_pc(initial_b2b: u32) -> Self {
        Self {
            initial_b2b,
            include_drop_score: false,
            level_multiplier: 1,
            same_shape: SameShapeScorePolicy::HighestLegalTrace,
        }
    }

    pub const fn initial_b2b(self) -> u32 {
        self.initial_b2b
    }

    pub const fn include_drop_score(self) -> bool {
        self.include_drop_score
    }

    pub const fn level_multiplier(self) -> u32 {
        self.level_multiplier
    }

    pub const fn same_shape(self) -> SameShapeScorePolicy {
        self.same_shape
    }
}

impl Default for ScoreEvaluationPolicy {
    fn default() -> Self {
        Self::tetrio_pc(0)
    }
}
