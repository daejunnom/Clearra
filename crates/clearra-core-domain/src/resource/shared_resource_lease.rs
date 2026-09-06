use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use super::{ExecutionAvailability, ExecutionAvailabilityReason};

static NEXT_AUTHORITY_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceLeaseOwnerId(u64);

impl ResourceLeaseOwnerId {
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ResourceLeaseCapacity {
    pub compute_units: u32,
    pub memory_bytes: u64,
}

impl ResourceLeaseCapacity {
    pub const fn new(compute_units: u32, memory_bytes: u64) -> Option<Self> {
        if compute_units == 0 {
            None
        } else {
            Some(Self {
                compute_units,
                memory_bytes,
            })
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ResourceLeaseRequest {
    pub compute_units: u32,
    pub memory_bytes: u64,
}

impl ResourceLeaseRequest {
    pub const fn new(compute_units: u32, memory_bytes: u64) -> Option<Self> {
        if compute_units == 0 {
            None
        } else {
            Some(Self {
                compute_units,
                memory_bytes,
            })
        }
    }

    const fn fits(self, capacity: ResourceLeaseCapacity) -> bool {
        self.compute_units <= capacity.compute_units && self.memory_bytes <= capacity.memory_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ResourceLeaseToken {
    authority_id: u64,
    epoch: u64,
    owner: ResourceLeaseOwnerId,
    parent_epoch: Option<u64>,
    grant: ResourceLeaseRequest,
}

impl ResourceLeaseToken {
    pub const fn authority_id(self) -> u64 {
        self.authority_id
    }

    pub const fn epoch(self) -> u64 {
        self.epoch
    }

    pub const fn owner(self) -> ResourceLeaseOwnerId {
        self.owner
    }

    pub const fn parent_epoch(self) -> Option<u64> {
        self.parent_epoch
    }

    pub const fn grant(self) -> ResourceLeaseRequest {
        self.grant
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceLeaseAcquireError {
    availability: ExecutionAvailability,
    requested: ResourceLeaseRequest,
    available: ResourceLeaseCapacity,
}

impl ResourceLeaseAcquireError {
    pub const fn availability(self) -> ExecutionAvailability {
        self.availability
    }

    pub const fn requested(self) -> ResourceLeaseRequest {
        self.requested
    }

    pub const fn available(self) -> ResourceLeaseCapacity {
        self.available
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceLeaseReleaseError {
    AuthorityMismatch,
    OwnerMismatch,
    StaleEpoch,
    AlreadyReleased,
    ChildrenActive,
    AccountingInvariantViolated,
}

#[derive(Clone, Debug)]
pub struct SharedResourceLeaseAuthority {
    inner: Arc<Mutex<AuthorityState>>,
}

#[derive(Debug)]
struct AuthorityState {
    authority_id: u64,
    capacity: ResourceLeaseCapacity,
    used: ResourceLeaseRequest,
    next_epoch: u64,
    allocations: BTreeMap<u64, Allocation>,
}

#[derive(Clone, Copy, Debug)]
struct Allocation {
    token: ResourceLeaseToken,
    remaining: ResourceLeaseRequest,
    child_count: u32,
    release_pending: bool,
}

impl SharedResourceLeaseAuthority {
    pub fn new(capacity: ResourceLeaseCapacity) -> Self {
        let authority_id = NEXT_AUTHORITY_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .expect("shared resource authority identity space exhausted");
        Self::with_identity_and_epoch(capacity, authority_id, 0)
    }

    fn with_identity_and_epoch(
        capacity: ResourceLeaseCapacity,
        authority_id: u64,
        next_epoch: u64,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(AuthorityState {
                authority_id,
                capacity,
                used: ResourceLeaseRequest {
                    compute_units: 0,
                    memory_bytes: 0,
                },
                next_epoch,
                allocations: BTreeMap::new(),
            })),
        }
    }

    pub fn capacity(&self) -> ResourceLeaseCapacity {
        self.lock().capacity
    }

    pub fn available(&self) -> ResourceLeaseCapacity {
        let state = self.lock();
        ResourceLeaseCapacity {
            compute_units: state.capacity.compute_units - state.used.compute_units,
            memory_bytes: state.capacity.memory_bytes - state.used.memory_bytes,
        }
    }

    // Keep the copyable, typed failure evidence inline at this public authority boundary.
    #[allow(clippy::result_large_err)]
    pub fn try_acquire(
        &self,
        owner: ResourceLeaseOwnerId,
        request: ResourceLeaseRequest,
    ) -> Result<ResourceLease, ResourceLeaseAcquireError> {
        let token = {
            let mut state = self.lock();
            let available = ResourceLeaseCapacity {
                compute_units: state.capacity.compute_units - state.used.compute_units,
                memory_bytes: state.capacity.memory_bytes - state.used.memory_bytes,
            };
            if !request.fits(state.capacity) {
                return Err(acquire_error(
                    request,
                    available,
                    ExecutionAvailability::exhausted(exceeded_reason(request, state.capacity)),
                ));
            }
            if !request.fits(available) {
                return Err(acquire_error(
                    request,
                    available,
                    ExecutionAvailability::deferred(
                        ExecutionAvailabilityReason::SharedResourceContention,
                    ),
                ));
            }
            let epoch = next_epoch(&mut state)
                .map_err(|availability| acquire_error(request, available, availability))?;
            state.used.compute_units = state
                .used
                .compute_units
                .checked_add(request.compute_units)
                .expect("capacity check prevents compute overflow");
            state.used.memory_bytes = state
                .used
                .memory_bytes
                .checked_add(request.memory_bytes)
                .expect("capacity check prevents memory overflow");
            let token = ResourceLeaseToken {
                authority_id: state.authority_id,
                epoch,
                owner,
                parent_epoch: None,
                grant: request,
            };
            state.allocations.insert(
                epoch,
                Allocation {
                    token,
                    remaining: request,
                    child_count: 0,
                    release_pending: false,
                },
            );
            token
        };
        Ok(ResourceLease {
            authority: self.clone(),
            token,
            released: false,
        })
    }

    // Keep the copyable, typed failure evidence inline through the private child path.
    #[allow(clippy::result_large_err)]
    fn try_acquire_child(
        &self,
        parent: ResourceLeaseToken,
        owner: ResourceLeaseOwnerId,
        request: ResourceLeaseRequest,
    ) -> Result<ResourceLease, ResourceLeaseAcquireError> {
        let token = {
            let mut state = self.lock();
            if parent.authority_id != state.authority_id {
                return Err(acquire_error(
                    request,
                    zero_capacity(),
                    ExecutionAvailability::unavailable(
                        ExecutionAvailabilityReason::CapabilityUnavailable,
                    ),
                ));
            }
            let Some(parent_allocation) = state.allocations.get(&parent.epoch).copied() else {
                return Err(acquire_error(
                    request,
                    zero_capacity(),
                    ExecutionAvailability::unavailable(
                        ExecutionAvailabilityReason::CapabilityUnavailable,
                    ),
                ));
            };
            if parent_allocation.token != parent {
                return Err(acquire_error(
                    request,
                    zero_capacity(),
                    ExecutionAvailability::unavailable(
                        ExecutionAvailabilityReason::CapabilityUnavailable,
                    ),
                ));
            }
            let parent_capacity = ResourceLeaseCapacity {
                compute_units: parent.grant.compute_units,
                memory_bytes: parent.grant.memory_bytes,
            };
            let remaining_capacity = ResourceLeaseCapacity {
                compute_units: parent_allocation.remaining.compute_units,
                memory_bytes: parent_allocation.remaining.memory_bytes,
            };
            if !request.fits(parent_capacity) {
                return Err(acquire_error(
                    request,
                    remaining_capacity,
                    ExecutionAvailability::exhausted(exceeded_reason(request, parent_capacity)),
                ));
            }
            if !request.fits(remaining_capacity) {
                return Err(acquire_error(
                    request,
                    remaining_capacity,
                    ExecutionAvailability::deferred(
                        ExecutionAvailabilityReason::SharedResourceContention,
                    ),
                ));
            }
            let epoch = next_epoch(&mut state)
                .map_err(|availability| acquire_error(request, remaining_capacity, availability))?;
            let parent_allocation = state
                .allocations
                .get_mut(&parent.epoch)
                .expect("parent checked above");
            parent_allocation.remaining.compute_units -= request.compute_units;
            parent_allocation.remaining.memory_bytes -= request.memory_bytes;
            parent_allocation.child_count = parent_allocation
                .child_count
                .checked_add(1)
                .expect("child count cannot exceed epoch space");
            let token = ResourceLeaseToken {
                authority_id: state.authority_id,
                epoch,
                owner,
                parent_epoch: Some(parent.epoch),
                grant: request,
            };
            state.allocations.insert(
                epoch,
                Allocation {
                    token,
                    remaining: request,
                    child_count: 0,
                    release_pending: false,
                },
            );
            token
        };
        Ok(ResourceLease {
            authority: self.clone(),
            token,
            released: false,
        })
    }

    fn release_token(
        &self,
        token: ResourceLeaseToken,
        releasing_owner: ResourceLeaseOwnerId,
    ) -> Result<(), ResourceLeaseReleaseError> {
        let mut state = self.lock();
        let allocation = validate_release(&state, token, releasing_owner)?;
        if allocation.child_count != 0 || allocation.remaining != allocation.token.grant {
            return Err(ResourceLeaseReleaseError::ChildrenActive);
        }
        release_ready_allocation(&mut state, token.epoch)
    }

    /// Relinquishes a lease whose handle is being dropped. A parent with live
    /// children remains as a tombstone until its final child returns the
    /// delegated grant, at which point release cascades to the global pool.
    fn abandon_token(
        &self,
        token: ResourceLeaseToken,
        releasing_owner: ResourceLeaseOwnerId,
    ) -> Result<(), ResourceLeaseReleaseError> {
        let mut state = self.lock();
        let allocation = validate_release(&state, token, releasing_owner)?;
        if allocation.child_count == 0 && allocation.remaining == allocation.token.grant {
            return release_ready_allocation(&mut state, token.epoch);
        }
        state
            .allocations
            .get_mut(&token.epoch)
            .expect("allocation was validated before mutation")
            .release_pending = true;
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, AuthorityState> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[derive(Debug)]
pub struct ResourceLease {
    authority: SharedResourceLeaseAuthority,
    token: ResourceLeaseToken,
    released: bool,
}

impl ResourceLease {
    pub const fn token(&self) -> ResourceLeaseToken {
        self.token
    }

    // Keep the copyable, typed failure evidence inline at this public authority boundary.
    #[allow(clippy::result_large_err)]
    pub fn try_child(
        &self,
        owner: ResourceLeaseOwnerId,
        request: ResourceLeaseRequest,
    ) -> Result<Self, ResourceLeaseAcquireError> {
        if self.released {
            return Err(acquire_error(
                request,
                zero_capacity(),
                ExecutionAvailability::unavailable(
                    ExecutionAvailabilityReason::CapabilityUnavailable,
                ),
            ));
        }
        self.authority.try_acquire_child(self.token, owner, request)
    }

    pub fn release_as(
        &mut self,
        owner: ResourceLeaseOwnerId,
    ) -> Result<(), ResourceLeaseReleaseError> {
        if self.released {
            return Err(ResourceLeaseReleaseError::AlreadyReleased);
        }
        self.authority.release_token(self.token, owner)?;
        self.released = true;
        Ok(())
    }

    pub fn release(&mut self) -> Result<(), ResourceLeaseReleaseError> {
        self.release_as(self.token.owner)
    }

    pub const fn released(&self) -> bool {
        self.released
    }
}

impl Drop for ResourceLease {
    fn drop(&mut self) {
        if !self.released
            && self
                .authority
                .abandon_token(self.token, self.token.owner)
                .is_ok()
        {
            self.released = true;
        }
    }
}

fn validate_release(
    state: &AuthorityState,
    token: ResourceLeaseToken,
    releasing_owner: ResourceLeaseOwnerId,
) -> Result<Allocation, ResourceLeaseReleaseError> {
    if token.authority_id != state.authority_id {
        return Err(ResourceLeaseReleaseError::AuthorityMismatch);
    }
    if token.owner != releasing_owner {
        return Err(ResourceLeaseReleaseError::OwnerMismatch);
    }
    let Some(allocation) = state.allocations.get(&token.epoch).copied() else {
        return Err(ResourceLeaseReleaseError::StaleEpoch);
    };
    if allocation.token != token {
        return Err(ResourceLeaseReleaseError::StaleEpoch);
    }
    Ok(allocation)
}

fn release_ready_allocation(
    state: &mut AuthorityState,
    mut epoch: u64,
) -> Result<(), ResourceLeaseReleaseError> {
    loop {
        let allocation = state
            .allocations
            .get(&epoch)
            .copied()
            .ok_or(ResourceLeaseReleaseError::StaleEpoch)?;
        if allocation.child_count != 0 || allocation.remaining != allocation.token.grant {
            return Err(ResourceLeaseReleaseError::ChildrenActive);
        }
        let token = allocation.token;
        let Some(parent_epoch) = token.parent_epoch else {
            let compute_units = state
                .used
                .compute_units
                .checked_sub(token.grant.compute_units)
                .ok_or(ResourceLeaseReleaseError::AccountingInvariantViolated)?;
            let memory_bytes = state
                .used
                .memory_bytes
                .checked_sub(token.grant.memory_bytes)
                .ok_or(ResourceLeaseReleaseError::AccountingInvariantViolated)?;
            state.allocations.remove(&epoch);
            state.used.compute_units = compute_units;
            state.used.memory_bytes = memory_bytes;
            return Ok(());
        };

        let parent = state
            .allocations
            .get(&parent_epoch)
            .copied()
            .ok_or(ResourceLeaseReleaseError::AccountingInvariantViolated)?;
        let compute_units = parent
            .remaining
            .compute_units
            .checked_add(token.grant.compute_units)
            .ok_or(ResourceLeaseReleaseError::AccountingInvariantViolated)?;
        let memory_bytes = parent
            .remaining
            .memory_bytes
            .checked_add(token.grant.memory_bytes)
            .ok_or(ResourceLeaseReleaseError::AccountingInvariantViolated)?;
        let child_count = parent
            .child_count
            .checked_sub(1)
            .ok_or(ResourceLeaseReleaseError::AccountingInvariantViolated)?;
        if compute_units > parent.token.grant.compute_units
            || memory_bytes > parent.token.grant.memory_bytes
        {
            return Err(ResourceLeaseReleaseError::AccountingInvariantViolated);
        }
        state.allocations.remove(&epoch);
        let parent = state
            .allocations
            .get_mut(&parent_epoch)
            .expect("parent was validated before mutation");
        parent.remaining.compute_units = compute_units;
        parent.remaining.memory_bytes = memory_bytes;
        parent.child_count = child_count;
        if parent.release_pending
            && parent.child_count == 0
            && parent.remaining == parent.token.grant
        {
            epoch = parent_epoch;
            continue;
        }
        return Ok(());
    }
}

fn acquire_error(
    requested: ResourceLeaseRequest,
    available: ResourceLeaseCapacity,
    availability: ExecutionAvailability,
) -> ResourceLeaseAcquireError {
    ResourceLeaseAcquireError {
        availability,
        requested,
        available,
    }
}

const fn exceeded_reason(
    request: ResourceLeaseRequest,
    capacity: ResourceLeaseCapacity,
) -> ExecutionAvailabilityReason {
    if request.compute_units > capacity.compute_units {
        ExecutionAvailabilityReason::ComputeBudgetExceeded
    } else {
        ExecutionAvailabilityReason::MemoryBudgetExceeded
    }
}

const fn zero_capacity() -> ResourceLeaseCapacity {
    ResourceLeaseCapacity {
        compute_units: 0,
        memory_bytes: 0,
    }
}

// Epoch exhaustion must retain the complete copyable availability evidence.
#[allow(clippy::result_large_err)]
fn next_epoch(state: &mut AuthorityState) -> Result<u64, ExecutionAvailability> {
    let epoch = state.next_epoch.checked_add(1).ok_or_else(|| {
        ExecutionAvailability::unavailable(ExecutionAvailabilityReason::CapabilityUnavailable)
    })?;
    state.next_epoch = epoch;
    Ok(epoch)
}

#[cfg(test)]
mod tests {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    use super::*;
    use crate::resource::ExecutionAvailabilityState;

    fn owner(value: u64) -> ResourceLeaseOwnerId {
        ResourceLeaseOwnerId::new(value).expect("nonzero owner")
    }

    fn capacity(compute_units: u32, memory_bytes: u64) -> ResourceLeaseCapacity {
        ResourceLeaseCapacity {
            compute_units,
            memory_bytes,
        }
    }

    fn request(compute_units: u32, memory_bytes: u64) -> ResourceLeaseRequest {
        ResourceLeaseRequest::new(compute_units, memory_bytes).expect("request")
    }

    #[test]
    fn acquire_distinguishes_exhaustion_from_contention() {
        let authority = SharedResourceLeaseAuthority::new(capacity(4, 1_000));
        let _first = authority
            .try_acquire(owner(1), request(3, 700))
            .expect("first lease");

        let contention = authority
            .try_acquire(owner(2), request(2, 400))
            .expect_err("fits capacity but not current remainder");
        assert_eq!(
            contention.availability().state(),
            ExecutionAvailabilityState::Deferred
        );
        let exhausted = authority
            .try_acquire(owner(3), request(5, 10))
            .expect_err("request exceeds configured capacity");
        assert_eq!(
            exhausted.availability().state(),
            ExecutionAvailabilityState::Exhausted
        );
    }

    #[test]
    fn owner_mismatch_and_double_release_are_inert() {
        let authority = SharedResourceLeaseAuthority::new(capacity(2, 200));
        let mut lease = authority
            .try_acquire(owner(1), request(2, 200))
            .expect("lease");
        assert_eq!(
            lease.release_as(owner(2)),
            Err(ResourceLeaseReleaseError::OwnerMismatch)
        );
        assert_eq!(authority.available(), capacity(0, 0));
        lease.release().expect("owner release");
        assert_eq!(
            lease.release(),
            Err(ResourceLeaseReleaseError::AlreadyReleased)
        );
        assert_eq!(authority.available(), capacity(2, 200));
    }

    #[test]
    fn authority_and_epoch_tokens_do_not_collide() {
        let left = SharedResourceLeaseAuthority::new(capacity(1, 10));
        let right = SharedResourceLeaseAuthority::new(capacity(1, 10));
        let mut left_lease = left
            .try_acquire(owner(1), request(1, 10))
            .expect("left lease");
        let mut right_lease = right
            .try_acquire(owner(1), request(1, 10))
            .expect("right lease");
        assert_ne!(left_lease.token(), right_lease.token());
        assert_eq!(
            right.release_token(left_lease.token(), owner(1)),
            Err(ResourceLeaseReleaseError::AuthorityMismatch)
        );
        left_lease.release().expect("left release");
        assert_eq!(
            left.release_token(left_lease.token(), owner(1)),
            Err(ResourceLeaseReleaseError::StaleEpoch)
        );
        right_lease.release().expect("right release");
    }

    #[test]
    fn child_budget_is_conserved_and_parent_cannot_release_early() {
        let authority = SharedResourceLeaseAuthority::new(capacity(8, 800));
        let mut parent = authority
            .try_acquire(owner(1), request(6, 600))
            .expect("parent");
        let mut first = parent
            .try_child(owner(2), request(2, 200))
            .expect("first child");
        let mut second = parent
            .try_child(owner(3), request(4, 400))
            .expect("second child");
        let blocked = parent
            .try_child(owner(4), request(1, 1))
            .expect_err("parent remainder is fully delegated");
        assert_eq!(
            blocked.availability().state(),
            ExecutionAvailabilityState::Deferred
        );
        assert_eq!(
            parent.release(),
            Err(ResourceLeaseReleaseError::ChildrenActive)
        );
        first.release().expect("first child release");
        second.release().expect("second child release");
        parent.release().expect("parent release");
        assert_eq!(authority.available(), capacity(8, 800));
    }

    #[test]
    fn drop_and_unwind_release_exactly_once() {
        let authority = SharedResourceLeaseAuthority::new(capacity(2, 200));
        let result = catch_unwind(AssertUnwindSafe({
            let authority = authority.clone();
            move || {
                let _lease = authority
                    .try_acquire(owner(1), request(2, 200))
                    .expect("lease");
                panic!("synthetic unwind");
            }
        }));
        assert!(result.is_err());
        assert_eq!(authority.available(), capacity(2, 200));
    }

    #[test]
    fn dropping_parent_before_child_recovers_the_global_grant() {
        let authority = SharedResourceLeaseAuthority::new(capacity(4, 400));
        let parent = authority
            .try_acquire(owner(1), request(4, 400))
            .expect("parent");
        let child = parent.try_child(owner(2), request(3, 300)).expect("child");

        drop(parent);
        assert_eq!(authority.available(), capacity(0, 0));
        drop(child);

        assert_eq!(authority.available(), capacity(4, 400));
        let _recovered = authority
            .try_acquire(owner(3), request(4, 400))
            .expect("capacity is reusable after deferred parent release");
    }

    #[test]
    fn nested_pending_drops_cascade_after_the_last_descendant() {
        let authority = SharedResourceLeaseAuthority::new(capacity(8, 800));
        let parent = authority
            .try_acquire(owner(1), request(8, 800))
            .expect("parent");
        let child = parent.try_child(owner(2), request(6, 600)).expect("child");
        let grandchild = child
            .try_child(owner(3), request(5, 500))
            .expect("grandchild");

        drop(parent);
        drop(child);
        assert_eq!(authority.available(), capacity(0, 0));
        drop(grandchild);

        assert_eq!(authority.available(), capacity(8, 800));
    }

    #[test]
    fn unwind_with_live_child_recovers_after_child_drop() {
        let authority = SharedResourceLeaseAuthority::new(capacity(2, 200));
        let child = std::cell::RefCell::new(None);
        let result = catch_unwind(AssertUnwindSafe(|| {
            let parent = authority
                .try_acquire(owner(1), request(2, 200))
                .expect("parent");
            child.replace(Some(
                parent.try_child(owner(2), request(1, 100)).expect("child"),
            ));
            panic!("synthetic parent unwind");
        }));
        assert!(result.is_err());
        assert_eq!(authority.available(), capacity(0, 0));
        drop(child.into_inner());
        assert_eq!(authority.available(), capacity(2, 200));
    }

    #[test]
    fn epoch_overflow_fails_before_accounting_changes() {
        let authority =
            SharedResourceLeaseAuthority::with_identity_and_epoch(capacity(1, 10), 77, u64::MAX);
        let error = authority
            .try_acquire(owner(1), request(1, 10))
            .expect_err("epoch overflow");
        assert_eq!(
            error.availability().state(),
            ExecutionAvailabilityState::Unavailable
        );
        assert_eq!(authority.available(), capacity(1, 10));
    }
}
