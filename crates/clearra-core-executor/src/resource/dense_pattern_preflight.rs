use clearra_core_domain::resource::{ExecutionAvailability, ExecutionAvailabilityReason};

/// Product safety ceiling for one native dense pattern representation.
///
/// This is a representation capability boundary, not a request budget. A
/// caller-provided memory budget is checked separately and reports Exhausted.
pub(crate) const NATIVE_DENSE_PATTERN_BYTE_LIMIT: u128 = 1024 * 1024 * 1024;
pub(crate) const WASM32_LINEAR_ADDRESS_SPACE_BYTES: u128 = (u32::MAX as u128) + 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DensePatternExecutionSurface {
    Native,
    Wasm32,
}

impl DensePatternExecutionSurface {
    pub(crate) const fn current() -> Self {
        if cfg!(target_pointer_width = "32") {
            Self::Wasm32
        } else {
            Self::Native
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DensePatternPreflight {
    pub descriptor_pattern_count: u128,
    pub dense_pattern_count: u128,
    pub dense_word_count: u128,
    pub required_dense_bytes: u128,
    pub availability: ExecutionAvailability,
}

pub(crate) fn preflight_dense_pattern_execution(
    descriptor_pattern_count: u128,
    dense_pattern_count: u128,
    surface: DensePatternExecutionSurface,
    request_memory_budget_bytes: Option<u128>,
) -> DensePatternPreflight {
    let computed = dense_pattern_count
        .checked_add(u128::from(u64::BITS - 1))
        .map(|rounded| rounded / u128::from(u64::BITS))
        .and_then(|words| {
            words
                .checked_mul(core::mem::size_of::<u64>() as u128)
                .map(|bytes| (words, bytes))
        });
    let Some((dense_word_count, required_dense_bytes)) = computed else {
        return preflight_failure(
            descriptor_pattern_count,
            dense_pattern_count,
            u128::MAX,
            u128::MAX,
            ExecutionAvailability::unavailable(
                ExecutionAvailabilityReason::PatternCountAddressSpaceExceeded,
            ),
        );
    };

    let addressable_pattern_count = match surface {
        DensePatternExecutionSurface::Native => usize::MAX as u128,
        DensePatternExecutionSurface::Wasm32 => u32::MAX as u128,
    };
    if dense_pattern_count > addressable_pattern_count {
        return preflight_failure(
            descriptor_pattern_count,
            dense_pattern_count,
            dense_word_count,
            required_dense_bytes,
            ExecutionAvailability::unavailable(
                ExecutionAvailabilityReason::PatternCountAddressSpaceExceeded,
            ),
        );
    }

    let representation_limit = match surface {
        DensePatternExecutionSurface::Native => NATIVE_DENSE_PATTERN_BYTE_LIMIT,
        DensePatternExecutionSurface::Wasm32 => WASM32_LINEAR_ADDRESS_SPACE_BYTES,
    };
    if required_dense_bytes > representation_limit {
        return preflight_failure(
            descriptor_pattern_count,
            dense_pattern_count,
            dense_word_count,
            required_dense_bytes,
            ExecutionAvailability::unavailable(
                ExecutionAvailabilityReason::DensePatternRepresentationUnavailable,
            ),
        );
    }

    if request_memory_budget_bytes.is_some_and(|budget| required_dense_bytes > budget) {
        return preflight_failure(
            descriptor_pattern_count,
            dense_pattern_count,
            dense_word_count,
            required_dense_bytes,
            ExecutionAvailability::exhausted(ExecutionAvailabilityReason::MemoryBudgetExceeded),
        );
    }

    DensePatternPreflight {
        descriptor_pattern_count,
        dense_pattern_count,
        dense_word_count,
        required_dense_bytes,
        availability: ExecutionAvailability::available().with_pattern_evidence(
            descriptor_pattern_count,
            dense_pattern_count,
            required_dense_bytes,
        ),
    }
}

fn preflight_failure(
    descriptor_pattern_count: u128,
    dense_pattern_count: u128,
    dense_word_count: u128,
    required_dense_bytes: u128,
    availability: ExecutionAvailability,
) -> DensePatternPreflight {
    DensePatternPreflight {
        descriptor_pattern_count,
        dense_pattern_count,
        dense_word_count,
        required_dense_bytes,
        availability: availability.with_pattern_evidence(
            descriptor_pattern_count,
            dense_pattern_count,
            required_dense_bytes,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::resource::ExecutionAvailabilityState;

    const SIX_LINE_PATTERN_COUNT: u128 = 35_384_428_800;
    const SIX_LINE_DENSE_BYTES: u128 = 4_423_053_600;

    #[test]
    fn native_six_line_dense_preflight_fails_before_allocation() {
        let report = preflight_dense_pattern_execution(
            SIX_LINE_PATTERN_COUNT,
            SIX_LINE_PATTERN_COUNT,
            DensePatternExecutionSurface::Native,
            None,
        );

        assert_eq!(report.required_dense_bytes, SIX_LINE_DENSE_BYTES);
        assert_eq!(
            report.availability.state(),
            ExecutionAvailabilityState::Unavailable
        );
        assert_eq!(
            report.availability.reason(),
            Some(ExecutionAvailabilityReason::DensePatternRepresentationUnavailable)
        );
    }

    #[test]
    fn wasm32_six_line_count_is_typed_address_space_unavailability() {
        let report = preflight_dense_pattern_execution(
            SIX_LINE_PATTERN_COUNT,
            SIX_LINE_PATTERN_COUNT,
            DensePatternExecutionSurface::Wasm32,
            None,
        );

        assert_eq!(
            report.availability.state(),
            ExecutionAvailabilityState::Unavailable
        );
        assert_eq!(
            report.availability.reason(),
            Some(ExecutionAvailabilityReason::PatternCountAddressSpaceExceeded)
        );
    }

    #[test]
    fn configured_memory_budget_is_exhaustion_not_capability_unavailability() {
        let report = preflight_dense_pattern_execution(
            8_000_000,
            8_000_000,
            DensePatternExecutionSurface::Native,
            Some(999_999),
        );

        assert_eq!(report.required_dense_bytes, 1_000_000);
        assert_eq!(
            report.availability.state(),
            ExecutionAvailabilityState::Exhausted
        );
        assert_eq!(
            report.availability.reason(),
            Some(ExecutionAvailabilityReason::MemoryBudgetExceeded)
        );
    }

    #[test]
    fn checked_byte_math_fails_closed() {
        let report = preflight_dense_pattern_execution(
            u128::MAX,
            u128::MAX,
            DensePatternExecutionSurface::Native,
            None,
        );

        assert_eq!(
            report.availability.state(),
            ExecutionAvailabilityState::Unavailable
        );
        assert_eq!(report.required_dense_bytes, u128::MAX);
    }

    #[test]
    fn available_is_not_a_completeness_claim() {
        let report =
            preflight_dense_pattern_execution(64, 64, DensePatternExecutionSurface::Native, None);
        assert_eq!(report.required_dense_bytes, 8);
        assert_eq!(
            report.availability.state(),
            ExecutionAvailabilityState::Available
        );
    }

    #[test]
    fn large_descriptor_with_bounded_materialization_remains_executable_but_not_complete() {
        let report = preflight_dense_pattern_execution(
            SIX_LINE_PATTERN_COUNT,
            4_096,
            DensePatternExecutionSurface::Wasm32,
            None,
        );

        assert_eq!(report.descriptor_pattern_count, SIX_LINE_PATTERN_COUNT);
        assert_eq!(report.dense_pattern_count, 4_096);
        assert_eq!(report.required_dense_bytes, 512);
        assert_eq!(
            report.availability.state(),
            ExecutionAvailabilityState::Available
        );
    }
}
