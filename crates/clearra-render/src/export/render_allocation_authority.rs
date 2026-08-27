use core::mem::size_of;

use crate::RenderError;

/// Per-materialization authority for renderer-owned vector payloads.
///
/// The caller must prevalidate the complete requested shape through
/// `RenderExportLimits`. This authority repeats the byte authorization before
/// each allocator call and immediately accounts for the allocator's actual
/// retained capacity.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct RenderAllocationAuthority {
    max_materialization_bytes: u64,
    live_materialization_bytes: u64,
}

impl RenderAllocationAuthority {
    pub(crate) const fn new(max_materialization_bytes: u64) -> Self {
        Self {
            max_materialization_bytes,
            live_materialization_bytes: 0,
        }
    }

    pub(crate) fn try_vec_with_capacity<T>(
        &mut self,
        capacity: usize,
        allocation: &'static str,
    ) -> Result<Vec<T>, RenderError> {
        let requested_bytes = capacity_bytes::<T>(capacity)?;
        self.authorize_additional_bytes(requested_bytes)?;

        record_allocation_attempt();
        let mut values = Vec::new();
        values
            .try_reserve_exact(capacity)
            .map_err(|_| RenderError::AllocationFailed {
                allocation,
                requested_bytes,
            })?;

        let actual_bytes = capacity_bytes::<T>(values.capacity())?;
        let actual_live_bytes = self.authorize_additional_bytes(actual_bytes)?;
        self.live_materialization_bytes = actual_live_bytes;
        Ok(values)
    }

    fn authorize_additional_bytes(&self, additional_bytes: u64) -> Result<u64, RenderError> {
        let required_bytes = self
            .live_materialization_bytes
            .checked_add(additional_bytes)
            .ok_or_else(|| materialization_limit_error(u64::MAX, self.max_materialization_bytes))?;
        if required_bytes > self.max_materialization_bytes {
            return Err(materialization_limit_error(
                required_bytes,
                self.max_materialization_bytes,
            ));
        }
        Ok(required_bytes)
    }
}

fn capacity_bytes<T>(capacity: usize) -> Result<u64, RenderError> {
    let bytes = (capacity as u128)
        .checked_mul(size_of::<T>() as u128)
        .ok_or_else(|| materialization_limit_error(u64::MAX, u64::MAX))?;
    u64::try_from(bytes).map_err(|_| materialization_limit_error(u64::MAX, u64::MAX))
}

fn materialization_limit_error(actual: u64, max: u64) -> RenderError {
    RenderError::ExportLimitExceeded {
        limit: "max_materialization_bytes",
        actual,
        max,
    }
}

#[cfg(test)]
thread_local! {
    static ALLOCATION_ATTEMPTS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

#[cfg(test)]
fn record_allocation_attempt() {
    ALLOCATION_ATTEMPTS.with(|attempts| attempts.set(attempts.get() + 1));
}

#[cfg(not(test))]
fn record_allocation_attempt() {}

#[cfg(test)]
pub(crate) fn reset_allocation_attempts() {
    ALLOCATION_ATTEMPTS.with(|attempts| attempts.set(0));
}

#[cfg(test)]
pub(crate) fn allocation_attempts() -> usize {
    ALLOCATION_ATTEMPTS.with(core::cell::Cell::get)
}
