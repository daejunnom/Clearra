//! SRP: retained frontier payload admission. Queue/table element capacities
//! and nested vector payloads share one budget; allocator metadata, graph cache,
//! I/O, source owners and product projection retain their separate authorities.
use core::{mem::size_of, num::NonZeroUsize};

#[derive(Clone, Copy, Debug)]
pub(crate) enum FrontierRetentionError {
    Overflow,
    Underflow,
    Limit { maximum: usize, required: usize },
}

impl FrontierRetentionError {
    pub(super) const fn reason(self) -> &'static str {
        match self {
            Self::Overflow | Self::Underflow => "pc4_compact_union_frontier_accounting_failed",
            Self::Limit { .. } => "pc4_compact_union_frontier_byte_limit",
        }
    }
}

/// Logical payload-capacity model, like FiniteSupplyAllocationLedger. This is
/// not RSS or permission to accept a finite whole-search memory request.
pub(super) struct FrontierRetention {
    maximum: usize,
    nested: usize,
    peak: usize,
}

impl FrontierRetention {
    pub(super) fn new(maximum: NonZeroUsize) -> Self {
        Self {
            maximum: maximum.get(),
            nested: 0,
            peak: 0,
        }
    }

    pub(super) fn authorize(
        &self,
        outer: usize,
        additional: usize,
    ) -> Result<usize, FrontierRetentionError> {
        let required = outer
            .checked_add(self.nested)
            .and_then(|n| n.checked_add(additional))
            .ok_or(FrontierRetentionError::Overflow)?;
        if required > self.maximum {
            return Err(FrontierRetentionError::Limit {
                maximum: self.maximum,
                required,
            });
        }
        Ok(required)
    }

    pub(super) fn observe(&mut self, outer: usize) -> Result<(), FrontierRetentionError> {
        self.peak = self.peak.max(self.authorize(outer, 0)?);
        Ok(())
    }

    /// Called while the newly allocated payload and replaced old payload (if
    /// any) are both live. Release old capacity only after its owner is gone.
    pub(super) fn retain(
        &mut self,
        outer: usize,
        bytes: usize,
    ) -> Result<(), FrontierRetentionError> {
        let required = self.authorize(outer, bytes)?;
        self.nested += bytes; // checked by authorize
        self.peak = self.peak.max(required);
        Ok(())
    }

    pub(super) fn release(&mut self, bytes: usize) -> Result<(), FrontierRetentionError> {
        self.nested = self
            .nested
            .checked_sub(bytes)
            .ok_or(FrontierRetentionError::Underflow)?;
        Ok(())
    }

    pub(super) const fn nested(&self) -> usize {
        self.nested
    }
    pub(super) const fn peak(&self) -> usize {
        self.peak
    }
}

pub(super) fn capacity_bytes<T>(capacity: usize) -> Result<usize, FrontierRetentionError> {
    capacity
        .checked_mul(size_of::<T>())
        .ok_or(FrontierRetentionError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pc4_compact_graph_union_retention_preserves_old_and_new_live_payloads() {
        let mut owner = FrontierRetention::new(NonZeroUsize::new(100).unwrap());
        owner.retain(20, 40).unwrap();
        assert!(matches!(
            owner.authorize(20, 41),
            Err(FrontierRetentionError::Limit {
                maximum: 100,
                required: 101
            })
        ));
        owner.retain(20, 40).unwrap();
        assert_eq!(owner.peak(), 100);
        owner.release(40).unwrap();
        owner.observe(40).unwrap();
        owner.release(40).unwrap();
        assert_eq!(owner.nested(), 0);
        assert!(matches!(
            owner.release(1),
            Err(FrontierRetentionError::Underflow)
        ));
        assert!(capacity_bytes::<u64>(usize::MAX).is_err());
    }
}
