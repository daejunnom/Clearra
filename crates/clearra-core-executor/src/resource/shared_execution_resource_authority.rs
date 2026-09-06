use std::sync::{
    atomic::{AtomicU64, Ordering},
    OnceLock,
};

use clearra_core_domain::resource::{
    ResourceLease, ResourceLeaseAcquireError, ResourceLeaseCapacity, ResourceLeaseOwnerId,
    ResourceLeaseRequest, ResourceReport, SharedResourceLeaseAuthority,
};

use super::dense_pattern_preflight::{
    NATIVE_DENSE_PATTERN_BYTE_LIMIT, WASM32_LINEAR_ADDRESS_SPACE_BYTES,
};

static SHARED_EXECUTION_AUTHORITY: OnceLock<SharedResourceLeaseAuthority> = OnceLock::new();
static NEXT_EXECUTION_OWNER: AtomicU64 = AtomicU64::new(1);

/// Opaque owner of the complete shared execution-memory surface and the CPU
/// compute slots reserved for one typed score request.
///
/// Acquire this authority before compiling the request into a SearchProblem,
/// then retain it through terminal post-processing. Child search admissions
/// borrow compute from this owner without acquiring a second global memory
/// lease, so request compilation, exact search, and public projection remain
/// serialized under one physical-capacity authority. Browser WASM reserves one
/// local compute slot; native callers may reserve their complete effective
/// worker count before the exact-search session starts.
#[must_use = "retain this authority through typed score terminal post-processing"]
pub struct WasmCpuTerminalResourceAuthority {
    lease: ResourceLease,
    memory_capacity_bytes: u128,
}

impl WasmCpuTerminalResourceAuthority {
    // The resource layer returns typed admission evidence; product errors box it later.
    #[allow(clippy::result_large_err)]
    pub fn try_acquire_full_capacity() -> Result<Self, ResourceReport> {
        Self::try_acquire_full_capacity_with_compute_units(1)
    }

    /// Atomically reserves the complete shared memory surface and the exact
    /// native compute width that the child search session may consume.
    ///
    /// Keeping compute and memory in one parent lease prevents a typed score
    /// request from compiling successfully and then discovering that its
    /// requested worker width cannot be admitted. The effective worker policy
    /// is already capped to host capacity before it reaches this boundary.
    #[allow(clippy::result_large_err)]
    pub fn try_acquire_full_capacity_with_compute_units(
        requested_compute_units: usize,
    ) -> Result<Self, ResourceReport> {
        let capacity = shared_execution_resource_capacity();
        let requested_compute_units =
            u32::try_from(requested_compute_units.max(1)).unwrap_or(u32::MAX);
        let request = ResourceLeaseRequest::new(requested_compute_units, capacity.memory_bytes)
            .expect("the shared execution surface has nonzero capacity");
        let lease = acquire_shared_execution_resources(next_execution_resource_owner(), request)
            .map_err(|error| ResourceReport::admission_failure(error.availability()))?;
        Ok(Self {
            lease,
            memory_capacity_bytes: u128::from(capacity.memory_bytes),
        })
    }

    /// The physical memory surface held by this authority. This is read-only
    /// evidence for checked request-level accounting, not allocation credit.
    pub const fn memory_capacity_bytes(&self) -> u128 {
        self.memory_capacity_bytes
    }

    /// Compute width owned by this request-level authority. Child sessions may
    /// borrow at most this many slots and return them before terminal projection.
    pub fn compute_capacity_units(&self) -> u32 {
        self.lease.token().grant().compute_units
    }

    #[allow(clippy::result_large_err)]
    pub(crate) fn try_acquire_compute_child(
        &self,
        requested_compute_units: usize,
    ) -> Result<ResourceLease, ResourceLeaseAcquireError> {
        let requested_compute_units =
            u32::try_from(requested_compute_units.max(1)).unwrap_or(u32::MAX);
        let request = ResourceLeaseRequest::new(requested_compute_units, 0)
            .expect("a nonzero compute child request is valid");
        self.lease
            .try_child(next_execution_resource_owner(), request)
    }
}

pub(crate) fn next_execution_resource_owner() -> ResourceLeaseOwnerId {
    let value = NEXT_EXECUTION_OWNER
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .expect("shared execution resource owner identity space exhausted");
    ResourceLeaseOwnerId::new(value).expect("owner identity starts at one")
}

#[allow(clippy::result_large_err)]
pub(crate) fn acquire_shared_execution_resources(
    owner: ResourceLeaseOwnerId,
    request: ResourceLeaseRequest,
) -> Result<ResourceLease, ResourceLeaseAcquireError> {
    shared_authority().try_acquire(owner, request)
}

pub(crate) fn shared_execution_resource_capacity() -> ResourceLeaseCapacity {
    shared_authority().capacity()
}

fn shared_authority() -> &'static SharedResourceLeaseAuthority {
    SHARED_EXECUTION_AUTHORITY.get_or_init(|| {
        SharedResourceLeaseAuthority::new(
            ResourceLeaseCapacity::new(host_compute_capacity(), host_memory_capacity())
                .expect("host compute capacity is nonzero"),
        )
    })
}

fn host_compute_capacity() -> u32 {
    if cfg!(target_family = "wasm") {
        return 1;
    }
    // Worker selection may raise Rust's quota-aware recommendation to an
    // explicitly configured vCPU ceiling after validating Linux affinity.
    // Admission must use that same authority or an accepted worker policy can
    // immediately reject itself with `compute-budget-exceeded`.
    host_compute_capacity_from_hard_limit(
        clearra_core_domain::runtime_cpu_capacity::CpuCapacity::current().hard_limit(),
    )
}

fn host_compute_capacity_from_hard_limit(hard_limit: usize) -> u32 {
    u32::try_from(hard_limit).unwrap_or(u32::MAX).max(1)
}

const fn host_memory_capacity() -> u64 {
    let capacity = if cfg!(target_pointer_width = "32") {
        WASM32_LINEAR_ADDRESS_SPACE_BYTES
    } else {
        NATIVE_DENSE_PATTERN_BYTE_LIMIT
    };
    capacity as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_owners_are_nonzero_and_distinct() {
        let left = next_execution_resource_owner();
        let right = next_execution_resource_owner();
        assert_ne!(left, right);
        assert_ne!(left.get(), 0);
        assert_ne!(right.get(), 0);
    }

    #[test]
    fn native_compute_capacity_preserves_the_authoritative_worker_ceiling() {
        assert_eq!(host_compute_capacity_from_hard_limit(0), 1);
        assert_eq!(host_compute_capacity_from_hard_limit(8), 8);
        assert_eq!(host_compute_capacity_from_hard_limit(usize::MAX), u32::MAX);
    }
}
