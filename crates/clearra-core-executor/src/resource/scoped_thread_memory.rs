//! Incremental scratch ownership for short native parallel phases. Existing
//! search compute slots are reused, so this lease reserves memory alone.

use clearra_core_domain::resource::ResourceLease;
use clearra_core_domain::resource::ResourceLeaseRequest;

pub(crate) const SCOPED_THREAD_STACK_BYTES: usize = 256 * 1024;
const THREAD_METADATA_BUDGET_BYTES: usize = 64 * 1024;

fn reservation_bytes(background_threads: usize) -> Option<u64> {
    let per_thread = SCOPED_THREAD_STACK_BYTES.checked_add(THREAD_METADATA_BUDGET_BYTES)?;
    u64::try_from(background_threads.checked_mul(per_thread)?).ok()
}

/// The explicit stack size and a metadata allowance share the existing global
/// memory authority. This is an incremental logical budget, not a claim about
/// complete process RSS, allocator arena reservations, or idle pool stacks.
///
/// The caller keeps the lease through every join and falls back to its original
/// serial calculation if this additional reservation is unavailable.
pub(crate) fn try_acquire_scoped_thread_memory(background_threads: usize) -> Option<ResourceLease> {
    let request = ResourceLeaseRequest::memory_only(reservation_bytes(background_threads)?)?;
    super::acquire_shared_execution_resources(super::next_execution_resource_owner(), request).ok()
}

#[cfg(test)]
mod tests {
    use super::reservation_bytes;
    use super::SCOPED_THREAD_STACK_BYTES;
    use super::THREAD_METADATA_BUDGET_BYTES;

    #[test]
    fn scoped_thread_memory_counts_only_background_threads_and_rejects_overflow() {
        assert_eq!(reservation_bytes(0), Some(0));
        assert_eq!(
            reservation_bytes(7),
            Some(7 * (SCOPED_THREAD_STACK_BYTES + THREAD_METADATA_BUDGET_BYTES) as u64),
        );
        assert_eq!(reservation_bytes(usize::MAX), None);
    }
}
