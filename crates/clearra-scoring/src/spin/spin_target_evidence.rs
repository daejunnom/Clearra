use super::{
    spin_accuracy::{SpinAccuracy, TraceCompleteness},
    spin_classification_input::KickEvidence,
    spin_result::SpinResult,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinTargetEvidence {
    spin_result: SpinResult,
    score_profile_id: Option<String>,
    kick_evidence: Option<KickEvidence>,
    trace_completeness: TraceCompleteness,
    accuracy: SpinAccuracy,
}

impl SpinTargetEvidence {
    pub fn new(spin_result: SpinResult) -> Self {
        Self {
            accuracy: spin_result.accuracy(),
            spin_result,
            score_profile_id: None,
            kick_evidence: None,
            trace_completeness: TraceCompleteness::Full,
        }
    }
}
impl SpinTargetEvidence {
    pub fn with_score_profile_id(mut self, profile_id: impl Into<String>) -> Self {
        self.score_profile_id = Some(profile_id.into());
        self
    }
}
impl SpinTargetEvidence {
    pub fn with_kick_evidence(mut self, kick_evidence: KickEvidence) -> Self {
        self.kick_evidence = Some(kick_evidence);
        self
    }
}
impl SpinTargetEvidence {
    pub fn with_trace_completeness(mut self, trace_completeness: TraceCompleteness) -> Self {
        self.trace_completeness = trace_completeness;
        self
    }
}
impl SpinTargetEvidence {
    pub fn spin_result(&self) -> SpinResult {
        self.spin_result
    }
}
impl SpinTargetEvidence {
    pub fn score_profile_id(&self) -> Option<&str> {
        self.score_profile_id.as_deref()
    }
}
impl SpinTargetEvidence {
    pub fn kick_evidence(&self) -> Option<&KickEvidence> {
        self.kick_evidence.as_ref()
    }
}
impl SpinTargetEvidence {
    pub fn trace_completeness(&self) -> TraceCompleteness {
        self.trace_completeness
    }
}
impl SpinTargetEvidence {
    pub fn accuracy(&self) -> SpinAccuracy {
        self.accuracy
    }
}
