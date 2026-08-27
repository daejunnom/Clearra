use core::mem::size_of;

/// Unique finite-allocation authority shared by the scenario-PC compiler and
/// its supply materializer.
///
/// The ledger follows Clearra's logical memory model: vector and string
/// payload capacities are counted, while allocator metadata, allocation
/// rounding, and shared-owner control blocks are not. The type intentionally
/// has no `Clone` implementation. A caller owns one ledger and lends it through
/// a mutable transaction to each lower constructor.
#[derive(Debug, Eq, PartialEq)]
pub struct FiniteSupplyAllocationLedger {
    max_memory_bytes: u128,
    live_memory_bytes: u128,
    peak_memory_bytes: u128,
}

impl FiniteSupplyAllocationLedger {
    pub fn try_new(
        max_memory_bytes: u128,
        live_memory_bytes: u128,
    ) -> Result<Self, FiniteSupplyAllocationError> {
        ensure_within_limit(live_memory_bytes, max_memory_bytes)?;
        Ok(Self {
            max_memory_bytes,
            live_memory_bytes,
            peak_memory_bytes: live_memory_bytes,
        })
    }

    pub const fn max_memory_bytes(&self) -> u128 {
        self.max_memory_bytes
    }

    pub const fn live_memory_bytes(&self) -> u128 {
        self.live_memory_bytes
    }

    pub const fn peak_memory_bytes(&self) -> u128 {
        self.peak_memory_bytes
    }

    /// Begins a rollback-on-drop allocation transaction.
    ///
    /// Allocations returned by the transaction must remain live until
    /// `commit`. If any later step fails, dropping those values and the
    /// uncommitted transaction restores the caller's ledger exactly.
    pub fn transaction(&mut self) -> FiniteSupplyAllocationTransaction<'_> {
        FiniteSupplyAllocationTransaction {
            live_memory_bytes: self.live_memory_bytes,
            peak_memory_bytes: self.peak_memory_bytes,
            ledger: self,
        }
    }
}

/// A unique, rollback-on-drop view of a finite allocation ledger.
///
/// `try_vec_with_capacity` and `try_string_with_capacity` authorize the exact
/// requested payload before calling the allocator, then immediately remeasure
/// actual capacity. A caller must not grow the returned buffer outside these
/// methods while it remains governed by the ledger.
#[derive(Debug)]
pub struct FiniteSupplyAllocationTransaction<'a> {
    ledger: &'a mut FiniteSupplyAllocationLedger,
    live_memory_bytes: u128,
    peak_memory_bytes: u128,
}

impl FiniteSupplyAllocationTransaction<'_> {
    pub const fn live_memory_bytes(&self) -> u128 {
        self.live_memory_bytes
    }

    pub const fn peak_memory_bytes(&self) -> u128 {
        self.peak_memory_bytes
    }

    pub fn try_vec_with_capacity<T>(
        &mut self,
        capacity: usize,
    ) -> Result<Vec<T>, FiniteSupplyAllocationError> {
        let requested_capacity_bytes = checked_capacity_bytes::<T>(capacity)?;
        self.authorize_additional_bytes(requested_capacity_bytes)?;

        let mut values = Vec::new();
        values.try_reserve_exact(capacity).map_err(|_| {
            FiniteSupplyAllocationError::AllocationFailed {
                requested_capacity_bytes,
            }
        })?;

        let actual_capacity_bytes = checked_capacity_bytes::<T>(values.capacity())?;
        self.commit_actual_allocation(actual_capacity_bytes)?;
        Ok(values)
    }

    pub fn try_string_with_capacity(
        &mut self,
        capacity: usize,
    ) -> Result<String, FiniteSupplyAllocationError> {
        let requested_capacity_bytes = capacity as u128;
        self.authorize_additional_bytes(requested_capacity_bytes)?;

        let mut value = String::new();
        value.try_reserve_exact(capacity).map_err(|_| {
            FiniteSupplyAllocationError::AllocationFailed {
                requested_capacity_bytes,
            }
        })?;

        let actual_capacity_bytes = value.capacity() as u128;
        self.commit_actual_allocation(actual_capacity_bytes)?;
        Ok(value)
    }

    /// Releases payload bytes for an input owner that has been dropped or
    /// moved out of this finite envelope.
    ///
    /// The caller must pass the owner's measured allocation capacity, not its
    /// logical length. Under-reporting would invalidate the finite authority,
    /// so arithmetic underflow fails closed.
    pub fn release_retained_bytes(
        &mut self,
        retained_bytes: u128,
    ) -> Result<(), FiniteSupplyAllocationError> {
        self.live_memory_bytes = self
            .live_memory_bytes
            .checked_sub(retained_bytes)
            .ok_or(FiniteSupplyAllocationError::AccountingUnderflow)?;
        Ok(())
    }

    pub fn commit(self) {
        self.ledger.live_memory_bytes = self.live_memory_bytes;
        self.ledger.peak_memory_bytes = self.peak_memory_bytes;
    }

    fn authorize_additional_bytes(
        &self,
        additional_bytes: u128,
    ) -> Result<(), FiniteSupplyAllocationError> {
        let required_memory_bytes = self
            .live_memory_bytes
            .checked_add(additional_bytes)
            .ok_or(FiniteSupplyAllocationError::ProjectionOverflow)?;
        ensure_within_limit(required_memory_bytes, self.ledger.max_memory_bytes)
    }

    fn commit_actual_allocation(
        &mut self,
        actual_capacity_bytes: u128,
    ) -> Result<(), FiniteSupplyAllocationError> {
        let required_memory_bytes = self
            .live_memory_bytes
            .checked_add(actual_capacity_bytes)
            .ok_or(FiniteSupplyAllocationError::ProjectionOverflow)?;
        ensure_within_limit(required_memory_bytes, self.ledger.max_memory_bytes)?;
        self.live_memory_bytes = required_memory_bytes;
        self.peak_memory_bytes = self.peak_memory_bytes.max(required_memory_bytes);
        Ok(())
    }
}

fn checked_capacity_bytes<T>(capacity: usize) -> Result<u128, FiniteSupplyAllocationError> {
    (capacity as u128)
        .checked_mul(size_of::<T>() as u128)
        .ok_or(FiniteSupplyAllocationError::ProjectionOverflow)
}

fn ensure_within_limit(
    required_memory_bytes: u128,
    max_memory_bytes: u128,
) -> Result<(), FiniteSupplyAllocationError> {
    if required_memory_bytes > max_memory_bytes {
        return Err(FiniteSupplyAllocationError::MemoryCapacityExceeded {
            required_memory_bytes,
            max_memory_bytes,
        });
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FiniteSupplyAllocationError {
    ProjectionOverflow,
    AccountingUnderflow,
    MemoryCapacityExceeded {
        required_memory_bytes: u128,
        max_memory_bytes: u128,
    },
    AllocationFailed {
        requested_capacity_bytes: u128,
    },
}

#[cfg(test)]
mod tests {
    use super::{FiniteSupplyAllocationError, FiniteSupplyAllocationLedger};

    #[test]
    fn uncommitted_transaction_preserves_the_unique_owner() {
        let mut ledger = FiniteSupplyAllocationLedger::try_new(64, 7).expect("ledger");
        {
            let mut transaction = ledger.transaction();
            let _buffer = transaction
                .try_vec_with_capacity::<u32>(4)
                .expect("authorized allocation");
        }

        assert_eq!(ledger.live_memory_bytes(), 7);
        assert_eq!(ledger.peak_memory_bytes(), 7);
    }

    #[test]
    fn actual_capacity_is_committed_and_one_byte_short_is_rejected() {
        let mut discovery = FiniteSupplyAllocationLedger::try_new(u128::MAX, 3).expect("ledger");
        let actual_capacity_bytes = {
            let mut transaction = discovery.transaction();
            let buffer = transaction
                .try_vec_with_capacity::<u32>(5)
                .expect("discover actual capacity");
            let actual = (buffer.capacity() as u128) * (core::mem::size_of::<u32>() as u128);
            transaction.commit();
            actual
        };
        let exact_limit = 3 + actual_capacity_bytes;

        let mut exact =
            FiniteSupplyAllocationLedger::try_new(exact_limit, 3).expect("exact ledger");
        let mut exact_transaction = exact.transaction();
        let _buffer = exact_transaction
            .try_vec_with_capacity::<u32>(5)
            .expect("exact capacity remains admissible");
        exact_transaction.commit();
        assert_eq!(exact.peak_memory_bytes(), exact_limit);

        let mut short =
            FiniteSupplyAllocationLedger::try_new(exact_limit - 1, 3).expect("short ledger");
        let original_live = short.live_memory_bytes();
        let original_peak = short.peak_memory_bytes();
        let error = short
            .transaction()
            .try_vec_with_capacity::<u32>(5)
            .expect_err("one-byte-short capacity must fail");
        assert_eq!(
            error,
            FiniteSupplyAllocationError::MemoryCapacityExceeded {
                required_memory_bytes: exact_limit,
                max_memory_bytes: exact_limit - 1,
            }
        );
        assert_eq!(short.live_memory_bytes(), original_live);
        assert_eq!(short.peak_memory_bytes(), original_peak);
    }
}
