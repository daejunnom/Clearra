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

    /// Heap storage retained by this execution, excluding its inline value.
    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        checked_execution_nested_bytes(
            self.trace_identity.capacity(),
            self.replay_trace.checked_nested_retained_bytes()?,
        )
    }

    /// Heap storage requested by cloning this execution's nested owners.
    pub fn checked_clone_nested_bytes(&self) -> Option<u128> {
        checked_execution_nested_bytes(
            self.trace_identity.len(),
            self.replay_trace.checked_clone_nested_bytes()?,
        )
    }

    pub fn checked_clone_peak_bytes(&self) -> Option<u128> {
        (core::mem::size_of::<Self>() as u128)
            .checked_add(self.checked_nested_retained_bytes()?)?
            .checked_add(core::mem::size_of::<Self>() as u128)?
            .checked_add(self.checked_clone_nested_bytes()?)
    }
}

fn checked_execution_nested_bytes(trace_identity_bytes: usize, trace_bytes: u128) -> Option<u128> {
    (trace_identity_bytes as u128).checked_add(trace_bytes)
}

#[cfg(test)]
mod retained_memory_projection_tests {
    use super::*;

    #[test]
    fn execution_projection_adds_identity_and_trace_without_saturation() {
        assert_eq!(checked_execution_nested_bytes(17, 23), Some(40));
        assert_eq!(checked_execution_nested_bytes(1, u128::MAX), None);
    }
}
