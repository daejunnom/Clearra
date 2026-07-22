use super::{
    drop_score_policy::DropScorePolicy,
    score_accuracy::ScoreAccuracy,
    score_profile::{ScoreProfile, ScoringAccuracyLevel},
    spin_award_policy::SpinAwardPolicy,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreProfileObjectValidatorBasis {
    evaluator_accuracy: ScoreAccuracy,
    spin_award_policy: SpinAwardPolicy,
    drop_score_policy: DropScorePolicy,
    trace_completeness_required: bool,
}

impl ScoreProfileObjectValidatorBasis {
    pub fn new(evaluator_accuracy: ScoreAccuracy) -> Self {
        Self {
            evaluator_accuracy,
            spin_award_policy: SpinAwardPolicy::TSpinsOnly,
            drop_score_policy: DropScorePolicy::Disabled,
            trace_completeness_required: false,
        }
    }
}
impl ScoreProfileObjectValidatorBasis {
    pub fn with_spin_award_policy(mut self, spin_award_policy: SpinAwardPolicy) -> Self {
        self.spin_award_policy = spin_award_policy;
        self
    }
}
impl ScoreProfileObjectValidatorBasis {
    pub fn with_drop_score_policy(mut self, drop_score_policy: DropScorePolicy) -> Self {
        self.drop_score_policy = drop_score_policy;
        self
    }
}
impl ScoreProfileObjectValidatorBasis {
    pub fn requiring_trace_completeness(mut self, required: bool) -> Self {
        self.trace_completeness_required = required;
        self
    }
}
impl ScoreProfileObjectValidatorBasis {
    pub fn validate(&self, profile: &ScoreProfile) -> Result<(), ScoreProfileObjectValidatorError> {
        if profile.accuracy_level() == ScoringAccuracyLevel::ProfileSpecificExact
            && !self.evaluator_accuracy.is_exact()
        {
            return Err(ScoreProfileObjectValidatorError::ExactProfileWithBasicEvaluator);
        }

        if self.drop_score_policy.requires_drop_events() && !self.trace_completeness_required {
            return Err(ScoreProfileObjectValidatorError::DropScoreRequiresTraceCompleteness);
        }

        if self.spin_award_policy.allows_all_spins()
            && matches!(
                self.evaluator_accuracy,
                ScoreAccuracy::PlacementOnlyEstimate
                    | ScoreAccuracy::KickSensitiveUnavailable
                    | ScoreAccuracy::Incomplete
            )
        {
            return Err(ScoreProfileObjectValidatorError::AllSpinPolicyWithoutClassifier);
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScoreProfileObjectValidatorError {
    ExactProfileWithBasicEvaluator,
    DropScoreRequiresTraceCompleteness,
    AllSpinPolicyWithoutClassifier,
}
