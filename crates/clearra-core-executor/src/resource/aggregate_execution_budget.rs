//! One exact node debit and one aggregate memory owner for a complete request.
//! Immutable graphs/catalogs are charged once. Private workers borrow from the
//! remaining common surface; unequal workloads never receive artificial equal
//! caps, and a rejected reservation does not change either worker width or cap.
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use clearra_core_domain::resource::{
    ExecutionAvailability, ExecutionAvailabilityReason, ResourceReport,
};

use super::ExecutionMemoryBound;

pub(crate) struct SharedExecutionNodeBudget {
    limit: usize,
    consumed: AtomicUsize,
}

impl SharedExecutionNodeBudget {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            limit,
            consumed: AtomicUsize::new(0),
        }
    }

    /// Debit before visiting a Geometry/BuildUp node, never after the work.
    pub(crate) fn try_consume(&self) -> bool {
        self.limit == 0
            || self
                .consumed
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                    (used < self.limit).then(|| used + 1)
                })
                .is_ok()
    }

    pub(crate) fn remaining(&self) -> usize {
        self.limit
            .saturating_sub(self.consumed.load(Ordering::Relaxed))
    }

    pub(crate) fn consumed(&self) -> usize {
        self.consumed.load(Ordering::Relaxed)
    }

    pub(crate) fn exhaustion_report(&self) -> ResourceReport {
        let mut report = ResourceReport::execution_incomplete(ExecutionAvailability::exhausted(
            ExecutionAvailabilityReason::ComputeBudgetExceeded,
        ));
        report.mark_truncated_unknown();
        report
    }
}

struct MemoryState {
    owners: Vec<u128>,
    private_total: u128,
    admitted_peak: u128,
}

pub(crate) struct SharedExecutionMemoryBudget {
    bound: ExecutionMemoryBound,
    shared_retained_bytes: u128,
    state: Mutex<MemoryState>,
}

#[derive(Clone)]
pub(crate) struct ExecutionMemoryOwner {
    request: Arc<SharedExecutionMemoryBudget>,
    index: usize,
}

impl SharedExecutionMemoryBudget {
    #[cfg(any(test, all(feature = "parallel", not(target_family = "wasm"))))]
    #[allow(clippy::result_large_err)]
    pub(crate) fn new(
        bound: ExecutionMemoryBound,
        shared_retained_bytes: u128,
        owners: usize,
    ) -> Result<Arc<Self>, ResourceReport> {
        let control_bytes = (owners as u128)
            .checked_mul(core::mem::size_of::<u128>() as u128)
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<Self>() as u128 + 32))
            .ok_or_else(|| overflow_report(bound))?;
        bound.ensure(shared_retained_bytes, control_bytes)?;
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(owners)
            .map_err(|_| overflow_report(bound))?;
        entries.resize(owners, 0);
        let shared_retained_bytes = shared_retained_bytes
            .checked_add(control_bytes)
            .ok_or_else(|| overflow_report(bound))?;
        Ok(Arc::new(Self {
            bound,
            shared_retained_bytes,
            state: Mutex::new(MemoryState {
                owners: entries,
                private_total: 0,
                admitted_peak: shared_retained_bytes,
            }),
        }))
    }

    #[cfg(any(test, all(feature = "parallel", not(target_family = "wasm"))))]
    pub(crate) fn owner(self: &Arc<Self>, index: usize) -> ExecutionMemoryOwner {
        ExecutionMemoryOwner {
            request: Arc::clone(self),
            index,
        }
    }

    #[cfg(any(test, all(feature = "parallel", not(target_family = "wasm"))))]
    pub(crate) fn admitted_peak_bytes(&self) -> u128 {
        self.state
            .lock()
            .map(|state| state.admitted_peak)
            .unwrap_or(u128::MAX)
    }
}

impl ExecutionMemoryOwner {
    /// Moves already-admitted live backing between owners atomically. Call
    /// after the queue transfers that backing, before the new owner grows it.
    /// Neither a temporary release nor duplicate reservation is permitted.
    #[cfg(any(test, all(feature = "parallel", not(target_family = "wasm"))))]
    #[allow(clippy::result_large_err)]
    pub(crate) fn transfer_retained_to(
        &self,
        target: &Self,
        bytes: u128,
    ) -> Result<(), ResourceReport> {
        let bound = self.request.bound;
        if !Arc::ptr_eq(&self.request, &target.request) || self.index == target.index {
            return Err(overflow_report(bound));
        }
        let mut state = self
            .request
            .state
            .lock()
            .map_err(|_| overflow_report(bound))?;
        let source = state
            .owners
            .get(self.index)
            .copied()
            .and_then(|value| value.checked_sub(bytes))
            .ok_or_else(|| overflow_report(bound))?;
        let destination = state
            .owners
            .get(target.index)
            .copied()
            .and_then(|value| value.checked_add(bytes))
            .ok_or_else(|| overflow_report(bound))?;
        state.owners[self.index] = source;
        state.owners[target.index] = destination;
        Ok(())
    }

    /// The coordinator calls this only after this owner's scratch/result
    /// backing has dropped or transferred into an independently charged owner.
    /// Dropping an arbitrary clone must not release a still-live workspace.
    #[cfg(any(test, all(feature = "parallel", not(target_family = "wasm"))))]
    #[allow(clippy::result_large_err)]
    pub(crate) fn release_after_drop(&self) -> Result<(), ResourceReport> {
        self.ensure(0)
    }

    /// Replace this owner's previous live/reserved projection atomically. This
    /// reserves future allocations before they happen while peers keep their
    /// own reservations. A smaller checkpoint returns unused common credit.
    #[allow(clippy::result_large_err)]
    pub(crate) fn ensure(&self, private_bytes: u128) -> Result<(), ResourceReport> {
        let bound = self.request.bound;
        let mut state = self
            .request
            .state
            .lock()
            .map_err(|_| overflow_report(bound))?;
        let old = *state
            .owners
            .get(self.index)
            .ok_or_else(|| overflow_report(bound))?;
        let total = state
            .private_total
            .checked_sub(old)
            .and_then(|bytes| bytes.checked_add(private_bytes))
            .ok_or_else(|| overflow_report(bound))?;
        bound.ensure(self.request.shared_retained_bytes, total)?;
        state.owners[self.index] = private_bytes;
        state.private_total = total;
        state.admitted_peak = state
            .admitted_peak
            .max(self.request.shared_retained_bytes + total);
        Ok(())
    }
}

fn overflow_report(bound: ExecutionMemoryBound) -> ResourceReport {
    bound
        .ensure(u128::MAX, 1)
        .expect_err("aggregate accounting overflow is unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_node_debit_is_exact_across_eleven_workers() {
        let budget = Arc::new(SharedExecutionNodeBudget::new(997));
        std::thread::scope(|scope| {
            for _ in 0..11 {
                let budget = Arc::clone(&budget);
                scope.spawn(move || while budget.try_consume() {});
            }
        });
        assert_eq!(budget.consumed(), 997);
        assert_eq!(budget.remaining(), 0);
        assert!(!budget.try_consume());
        let report = budget.exhaustion_report();
        assert!(report.execution_started());
        assert!(!report.result_complete());
        assert!(report.truncated);
        assert!(!report.probability_complete);
    }

    #[test]
    fn aggregate_memory_preserves_unequal_private_owners_and_rejected_state() {
        let bound = ExecutionMemoryBound::for_test_cap(4096);
        let budget = SharedExecutionMemoryBudget::new(bound, 256, 2).unwrap();
        let left = budget.owner(0);
        let right = budget.owner(1);
        left.ensure(2600).unwrap();
        right.ensure(400).unwrap();
        assert!(right.ensure(1400).is_err());
        // A rejected reservation leaves both exact prior owners unchanged.
        left.ensure(2600).unwrap();
        right.ensure(400).unwrap();
        left.ensure(200).unwrap();
        right.ensure(2600).unwrap();
        // An alias is not a separate allocation owner and dropping it cannot
        // erase its workspace's reservation.
        let alias = right.clone();
        drop(alias);
        assert!(left.ensure(1400).is_err());
        right.release_after_drop().unwrap();
        left.ensure(3000).unwrap();
        assert!(budget.admitted_peak_bytes() <= 4096);
    }

    #[test]
    fn aggregate_memory_queue_transfer_never_releases_or_duplicates_live_backing() {
        let budget =
            SharedExecutionMemoryBudget::new(ExecutionMemoryBound::for_test_cap(4096), 256, 3)
                .unwrap();
        let queue = budget.owner(0);
        let worker = budget.owner(1);
        let peer = budget.owner(2);
        queue.ensure(1800).unwrap();
        worker.ensure(900).unwrap();
        peer.ensure(300).unwrap();
        let peak = budget.admitted_peak_bytes();
        queue.transfer_retained_to(&worker, 1200).unwrap();
        assert_eq!(budget.admitted_peak_bytes(), peak);
        assert!(peer.ensure(1300).is_err());
        // No partial accounting mutation on an invalid transfer.
        assert!(queue.transfer_retained_to(&worker, 601).is_err());
        queue.ensure(600).unwrap();
        worker.ensure(2100).unwrap();
        worker.release_after_drop().unwrap();
        peer.ensure(2500).unwrap();
    }
}
