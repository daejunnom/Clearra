#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TraceCompleteness {
    #[default]
    Complete,
    MissingKickEvidence,
    SampleOnly,
    Incomplete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraceCompletenessEvent {
    completeness: TraceCompleteness,
}

impl TraceCompletenessEvent {
    pub fn new(completeness: TraceCompleteness) -> Self {
        Self { completeness }
    }
}
impl TraceCompletenessEvent {
    pub fn completeness(self) -> TraceCompleteness {
        self.completeness
    }
}
