use clearra_replay::ReplayTrace;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorePostProcessExecution {
    candidate_id: u64,
    pattern_id: usize,
    trace_identity: String,
    replay_trace: ReplayTrace,
}

impl CorePostProcessExecution {
    pub fn new(
        candidate_id: u64,
        pattern_id: usize,
        trace_identity: impl Into<String>,
        replay_trace: ReplayTrace,
    ) -> Self {
        Self {
            candidate_id,
            pattern_id,
            trace_identity: trace_identity.into(),
            replay_trace,
        }
    }

    pub fn candidate_id(&self) -> u64 {
        self.candidate_id
    }

    pub fn pattern_id(&self) -> usize {
        self.pattern_id
    }

    pub fn trace_identity(&self) -> &str {
        &self.trace_identity
    }

    pub fn replay_trace(&self) -> &ReplayTrace {
        &self.replay_trace
    }
}
