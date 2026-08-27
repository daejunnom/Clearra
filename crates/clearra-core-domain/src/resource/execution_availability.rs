/// Whether execution can start or how a non-complete execution ended.
///
/// Availability deliberately does not encode result completeness. An available
/// execution can still become incomplete, exhausted, or cancelled later.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExecutionAvailabilityState {
    Available,
    Unavailable,
    Deferred,
    Exhausted,
    Cancelled,
    Incomplete,
}

impl ExecutionAvailabilityState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Unavailable => "unavailable",
            Self::Deferred => "deferred",
            Self::Exhausted => "exhausted",
            Self::Cancelled => "cancelled",
            Self::Incomplete => "incomplete",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExecutionAvailabilityReason {
    NotExecuted,
    CapabilityUnavailable,
    PatternCountAddressSpaceExceeded,
    DensePatternRepresentationUnavailable,
    ComputeBudgetExceeded,
    MemoryBudgetExceeded,
    SharedResourceContention,
    CancelledByCaller,
    PartialExecution,
}

impl ExecutionAvailabilityReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotExecuted => "not-executed",
            Self::CapabilityUnavailable => "capability-unavailable",
            Self::PatternCountAddressSpaceExceeded => "pattern-count-address-space-exceeded",
            Self::DensePatternRepresentationUnavailable => {
                "dense-pattern-representation-unavailable"
            }
            Self::ComputeBudgetExceeded => "compute-budget-exceeded",
            Self::MemoryBudgetExceeded => "memory-budget-exceeded",
            Self::SharedResourceContention => "shared-resource-contention",
            Self::CancelledByCaller => "cancelled-by-caller",
            Self::PartialExecution => "partial-execution",
        }
    }
}

/// Typed availability evidence carried independently from result completeness.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExecutionAvailability {
    state: ExecutionAvailabilityState,
    reason: Option<ExecutionAvailabilityReason>,
    descriptor_pattern_count: Option<u128>,
    dense_pattern_count: Option<u128>,
    required_dense_bytes: Option<u128>,
    required_memory_bytes: Option<u128>,
}

impl ExecutionAvailability {
    pub const fn available() -> Self {
        Self {
            state: ExecutionAvailabilityState::Available,
            reason: None,
            descriptor_pattern_count: None,
            dense_pattern_count: None,
            required_dense_bytes: None,
            required_memory_bytes: None,
        }
    }

    pub const fn unavailable(reason: ExecutionAvailabilityReason) -> Self {
        Self::non_available(ExecutionAvailabilityState::Unavailable, reason)
    }

    pub const fn deferred(reason: ExecutionAvailabilityReason) -> Self {
        Self::non_available(ExecutionAvailabilityState::Deferred, reason)
    }

    pub const fn exhausted(reason: ExecutionAvailabilityReason) -> Self {
        Self::non_available(ExecutionAvailabilityState::Exhausted, reason)
    }

    pub const fn cancelled() -> Self {
        Self::non_available(
            ExecutionAvailabilityState::Cancelled,
            ExecutionAvailabilityReason::CancelledByCaller,
        )
    }

    pub const fn incomplete(reason: ExecutionAvailabilityReason) -> Self {
        Self::non_available(ExecutionAvailabilityState::Incomplete, reason)
    }

    pub const fn not_executed() -> Self {
        Self::unavailable(ExecutionAvailabilityReason::NotExecuted)
    }

    const fn non_available(
        state: ExecutionAvailabilityState,
        reason: ExecutionAvailabilityReason,
    ) -> Self {
        Self {
            state,
            reason: Some(reason),
            descriptor_pattern_count: None,
            dense_pattern_count: None,
            required_dense_bytes: None,
            required_memory_bytes: None,
        }
    }

    pub const fn with_pattern_evidence(
        mut self,
        descriptor_pattern_count: u128,
        dense_pattern_count: u128,
        required_dense_bytes: u128,
    ) -> Self {
        self.descriptor_pattern_count = Some(descriptor_pattern_count);
        self.dense_pattern_count = Some(dense_pattern_count);
        self.required_dense_bytes = Some(required_dense_bytes);
        self
    }

    /// Records the checked upper bound used for admission and lease sizing.
    /// This can exceed `required_dense_bytes` because it includes catalogs,
    /// scratch space, retained candidates, and other count-proportional data.
    pub const fn with_required_memory_bytes(mut self, required_memory_bytes: u128) -> Self {
        self.required_memory_bytes = Some(required_memory_bytes);
        self
    }

    pub const fn state(self) -> ExecutionAvailabilityState {
        self.state
    }

    pub const fn reason(self) -> Option<ExecutionAvailabilityReason> {
        self.reason
    }

    pub const fn descriptor_pattern_count(self) -> Option<u128> {
        self.descriptor_pattern_count
    }

    pub const fn dense_pattern_count(self) -> Option<u128> {
        self.dense_pattern_count
    }

    pub const fn required_dense_bytes(self) -> Option<u128> {
        self.required_dense_bytes
    }

    pub const fn required_memory_bytes(self) -> Option<u128> {
        self.required_memory_bytes
    }
}

impl Default for ExecutionAvailability {
    fn default() -> Self {
        Self::not_executed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_are_distinct_and_default_is_not_available() {
        let values = [
            ExecutionAvailability::available(),
            ExecutionAvailability::unavailable(ExecutionAvailabilityReason::CapabilityUnavailable),
            ExecutionAvailability::deferred(ExecutionAvailabilityReason::SharedResourceContention),
            ExecutionAvailability::exhausted(ExecutionAvailabilityReason::MemoryBudgetExceeded),
            ExecutionAvailability::cancelled(),
            ExecutionAvailability::incomplete(ExecutionAvailabilityReason::PartialExecution),
        ];
        for (index, left) in values.iter().enumerate() {
            for (other_index, right) in values.iter().enumerate() {
                assert_eq!(left == right, index == other_index);
            }
        }
        assert_eq!(
            ExecutionAvailability::default().state(),
            ExecutionAvailabilityState::Unavailable
        );
    }

    #[test]
    fn descriptor_evidence_does_not_change_state_or_claim_completeness() {
        let availability = ExecutionAvailability::unavailable(
            ExecutionAvailabilityReason::DensePatternRepresentationUnavailable,
        )
        .with_pattern_evidence(35_384_428_800, 35_384_428_800, 4_423_053_600);

        assert_eq!(
            availability.state(),
            ExecutionAvailabilityState::Unavailable
        );
        assert_eq!(
            availability.descriptor_pattern_count(),
            Some(35_384_428_800)
        );
        assert_eq!(availability.dense_pattern_count(), Some(35_384_428_800));
        assert_eq!(availability.required_dense_bytes(), Some(4_423_053_600));
        assert_eq!(availability.required_memory_bytes(), None);
    }

    #[test]
    fn total_admission_projection_is_distinct_from_one_dense_bitset() {
        let availability =
            ExecutionAvailability::exhausted(ExecutionAvailabilityReason::MemoryBudgetExceeded)
                .with_pattern_evidence(1_058_400, 1_058_400, 132_304)
                .with_required_memory_bytes(17_066_704);

        assert_eq!(availability.required_dense_bytes(), Some(132_304));
        assert_eq!(availability.required_memory_bytes(), Some(17_066_704));
    }
}
