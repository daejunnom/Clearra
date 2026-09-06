use clearra_replay::ReplayTrace;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateExecution {
    pattern_id: usize,
    trace_identity: String,
    replay_trace: ReplayTrace,
}

impl CandidateExecution {
    pub fn new(
        pattern_id: usize,
        trace_identity: impl Into<String>,
        replay_trace: ReplayTrace,
    ) -> Self {
        Self {
            pattern_id,
            trace_identity: trace_identity.into(),
            replay_trace,
        }
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

    pub fn into_parts(self) -> (usize, String, ReplayTrace) {
        (self.pattern_id, self.trace_identity, self.replay_trace)
    }

    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        (self.trace_identity.capacity() as u128)
            .checked_add(self.replay_trace.checked_nested_retained_bytes()?)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateExecutionAggregate {
    candidate_id: u64,
    executions: Vec<CandidateExecution>,
}

impl CandidateExecutionAggregate {
    pub fn new(candidate_id: u64, executions: Vec<CandidateExecution>) -> Self {
        Self {
            candidate_id,
            executions,
        }
    }

    pub fn candidate_id(&self) -> u64 {
        self.candidate_id
    }

    pub fn executions(&self) -> &[CandidateExecution] {
        &self.executions
    }

    pub fn into_parts(self) -> (u64, Vec<CandidateExecution>) {
        (self.candidate_id, self.executions)
    }

    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        let inline_bytes = (self.executions.capacity() as u128)
            .checked_mul(core::mem::size_of::<CandidateExecution>() as u128)?;
        self.executions
            .iter()
            .try_fold(inline_bytes, |bytes, execution| {
                bytes.checked_add(execution.checked_nested_retained_bytes()?)
            })
    }
}
