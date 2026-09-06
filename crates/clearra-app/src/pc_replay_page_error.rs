//! Allocation-free replay failure evidence shared by manifest and page owners.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcReplayPageError {
    Invalid(&'static str),
    MemoryLimit { required: u128, maximum: u128 },
    HostMemoryLimit { required: u128 },
}

impl PcReplayPageError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Invalid(code) => code,
            Self::MemoryLimit { .. } => "complete_replay_whole_live_limit_exceeded",
            Self::HostMemoryLimit { .. } => "complete_replay_host_memory_limit_exceeded",
        }
    }

    pub const fn required_memory_bytes(self) -> Option<u128> {
        match self {
            Self::MemoryLimit { required, .. } | Self::HostMemoryLimit { required } => {
                Some(required)
            }
            Self::Invalid(_) => None,
        }
    }

    pub const fn max_memory_bytes(self) -> Option<u128> {
        match self {
            Self::MemoryLimit { maximum, .. } => Some(maximum),
            _ => None,
        }
    }
}

impl From<&'static str> for PcReplayPageError {
    fn from(code: &'static str) -> Self {
        Self::Invalid(code)
    }
}

impl core::fmt::Display for PcReplayPageError {
    fn fmt(&self, output: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        output.write_str(self.code())?;
        if let Some(required) = self.required_memory_bytes() {
            write!(output, ": required_memory_bytes={required}")?;
        }
        if let Some(maximum) = self.max_memory_bytes() {
            write!(output, ", max_memory_bytes={maximum}")?;
        }
        Ok(())
    }
}

pub(crate) fn replay_engine_error(
    error: clearra_postprocess::ExactReplayMaterializationError,
    external: u128,
) -> PcReplayPageError {
    use clearra_postprocess::ExactReplayMaterializationError as E;
    match error {
        E::MemoryLimitExceeded {
            required_memory_bytes,
            max_memory_bytes,
        } => {
            match (
                external.checked_add(required_memory_bytes),
                external.checked_add(max_memory_bytes),
            ) {
                (Some(required), Some(_)) if required_memory_bytes <= max_memory_bytes => {
                    PcReplayPageError::HostMemoryLimit { required }
                }
                (Some(required), Some(maximum)) => {
                    PcReplayPageError::MemoryLimit { required, maximum }
                }
                _ => "complete_replay_memory_projection_overflow".into(),
            }
        }
        E::Cancelled => "complete_replay_cancelled".into(),
        E::ExecutionLimitExceeded { .. } => "complete_replay_execution_limit_exceeded".into(),
        E::PathStepLimitExceeded { .. } => "complete_replay_path_step_limit_exceeded".into(),
        E::AllocationFailed => "complete_replay_allocation_failed".into(),
        E::ProjectionOverflow => "complete_replay_memory_projection_overflow".into(),
        _ => "complete_replay_evidence_invalid".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_memory_numbers_survive_cell_to_whole_live_translation() {
        let error = replay_engine_error(
            clearra_postprocess::ExactReplayMaterializationError::MemoryLimitExceeded {
                required_memory_bytes: 101,
                max_memory_bytes: 100,
            },
            1000,
        );
        assert_eq!(error.required_memory_bytes(), Some(1101));
        assert_eq!(error.max_memory_bytes(), Some(1100));
        assert!(error
            .to_string()
            .contains("required_memory_bytes=1101, max_memory_bytes=1100"));
        let host = replay_engine_error(
            clearra_postprocess::ExactReplayMaterializationError::MemoryLimitExceeded {
                required_memory_bytes: 99,
                max_memory_bytes: 100,
            },
            1000,
        );
        assert_eq!(host, PcReplayPageError::HostMemoryLimit { required: 1099 });
        assert_eq!(
            host.max_memory_bytes(),
            None,
            "a host refusal does not invent its private admission limit"
        );
        assert_eq!(
            replay_engine_error(
                clearra_postprocess::ExactReplayMaterializationError::MemoryLimitExceeded {
                    required_memory_bytes: 2,
                    max_memory_bytes: 1
                },
                u128::MAX
            )
            .code(),
            "complete_replay_memory_projection_overflow"
        );
    }
}
