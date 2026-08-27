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

/// Opaque owner of the complete shared execution-memory surface and one CPU
/// compute slot for a typed WASM score request.
///
/// Acquire this authority before compiling the request into a SearchProblem,
/// then retain it through terminal post-processing. Child search admissions
/// borrow compute from this owner without acquiring a second global memory
/// lease, so request compilation, exact search, and public projection remain
/// serialized under one physical-capacity authority.
#[must_use = "retain this authority through typed score terminal post-processing"]
pub struct WasmCpuTerminalResourceAuthority {
    lease: ResourceLease,
    memory_capacity_bytes: u128,
}

impl WasmCpuTerminalResourceAuthority {
    pub fn try_acquire_full_capacity() -> Result<Self, ResourceReport> {
        let capacity = shared_execution_resource_capacity();
        let request = ResourceLeaseRequest::new(1, capacity.memory_bytes)
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

    pub(crate) fn try_acquire_compute_child(
        &self,
    ) -> Result<ResourceLease, ResourceLeaseAcquireError> {
        let request =
            ResourceLeaseRequest::new(1, 0).expect("one compute unit is a valid child request");
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
    std::thread::available_parallelism()
        .ok()
        .and_then(|value| u32::try_from(value.get()).ok())
        .unwrap_or(1)
        .max(1)
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
}
