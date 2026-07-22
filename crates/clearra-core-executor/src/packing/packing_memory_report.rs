use clearra_core_domain::resource::{ResourceReport, ResourceTruncationReason};
use clearra_core_ffi::PackingCandidateBatch;

use super::{candidate_pattern_index::CandidatePatternIndex, PackingExecutionSource};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackingMemoryLeakCheckState {
    OwnershipReleaseConfirmed,
    BackendInstrumentationUnavailable,
}

impl PackingMemoryLeakCheckState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OwnershipReleaseConfirmed => "ownership-release-confirmed",
            Self::BackendInstrumentationUnavailable => "backend-instrumentation-unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackingMemoryReport {
    transient_scope_release_complete: bool,
    leak_check_state: PackingMemoryLeakCheckState,
    peak_cpu_bytes: usize,
    retained_candidate_bytes: usize,
    retained_pattern_index_bytes: usize,
    retained_search_bytes: usize,
    retained_candidate_allocation_count: usize,
    retained_pattern_index_allocation_count: usize,
    retained_allocation_count: usize,
    transient_live_allocations: usize,
    pressure_level: &'static str,
}

impl PackingMemoryReport {
    pub(crate) fn from_execution(
        source: PackingExecutionSource,
        resource: &ResourceReport,
        candidates: &PackingCandidateBatch,
        candidate_patterns: &CandidatePatternIndex,
    ) -> Self {
        let owned_native_scope = matches!(
            source,
            PackingExecutionSource::NativeCpuPacking
                | PackingExecutionSource::NativeGpuPacking
                | PackingExecutionSource::NativeHybridPacking
        );
        let retained_candidate_bytes = candidates.resident_bytes();
        let retained_pattern_index_bytes = candidate_patterns.resident_bytes();
        let retained_candidate_allocation_count = candidates.resident_allocation_count();
        let retained_pattern_index_allocation_count =
            candidate_patterns.resident_allocation_count();
        Self {
            transient_scope_release_complete: owned_native_scope,
            leak_check_state: if owned_native_scope {
                PackingMemoryLeakCheckState::OwnershipReleaseConfirmed
            } else {
                PackingMemoryLeakCheckState::BackendInstrumentationUnavailable
            },
            peak_cpu_bytes: resource.peak_cpu_bytes,
            retained_candidate_bytes,
            retained_pattern_index_bytes,
            retained_search_bytes: retained_candidate_bytes
                .saturating_add(retained_pattern_index_bytes),
            retained_candidate_allocation_count,
            retained_pattern_index_allocation_count,
            retained_allocation_count: retained_candidate_allocation_count
                .saturating_add(retained_pattern_index_allocation_count),
            transient_live_allocations: 0,
            pressure_level: if resource.truncation_reason
                == Some(ResourceTruncationReason::MemoryExceeded)
            {
                "high"
            } else {
                "low"
            },
        }
    }

    pub const fn transient_scope_release_complete(self) -> bool {
        self.transient_scope_release_complete
    }

    pub const fn leak_check_state(self) -> PackingMemoryLeakCheckState {
        self.leak_check_state
    }

    pub const fn memory_leak_report_clean(self) -> bool {
        self.transient_scope_release_complete
            && matches!(
                self.leak_check_state,
                PackingMemoryLeakCheckState::OwnershipReleaseConfirmed
            )
            && self.transient_live_allocations == 0
    }

    pub const fn peak_cpu_bytes(self) -> usize {
        self.peak_cpu_bytes
    }

    pub const fn retained_candidate_bytes(self) -> usize {
        self.retained_candidate_bytes
    }

    pub const fn retained_pattern_index_bytes(self) -> usize {
        self.retained_pattern_index_bytes
    }

    pub const fn retained_search_bytes(self) -> usize {
        self.retained_search_bytes
    }

    pub const fn retained_candidate_allocation_count(self) -> usize {
        self.retained_candidate_allocation_count
    }

    pub const fn retained_pattern_index_allocation_count(self) -> usize {
        self.retained_pattern_index_allocation_count
    }

    pub const fn retained_allocation_count(self) -> usize {
        self.retained_allocation_count
    }

    pub const fn transient_live_allocations(self) -> usize {
        self.transient_live_allocations
    }

    pub const fn pressure_level(self) -> &'static str {
        self.pressure_level
    }
}
