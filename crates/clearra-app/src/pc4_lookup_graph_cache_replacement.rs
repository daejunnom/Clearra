//! SRP: atomic bounded CLOCK replacement for the compact graph cache. The
//! caller declares live dependency pins; this module owns no traversal or I/O.
use super::{Pc4LookupGraphCache, Pc4LookupGraphCacheError, Pc4LookupGraphCacheUsage};
use core::mem::size_of;

impl Pc4LookupGraphCache {
    pub(super) fn plan_replacement(
        &mut self,
        mut usage: Pc4LookupGraphCacheUsage,
        protected: &dyn Fn(u32) -> bool,
    ) -> Result<(Pc4LookupGraphCacheUsage, Vec<u32>), Pc4LookupGraphCacheError> {
        let mut needed = self.excess(usage);
        if needed == [0; 3] {
            return Ok((usage, Vec::new()));
        }
        let plan = self
            .replacement_plan
            .checked_add(1)
            .ok_or(Pc4LookupGraphCacheError::AccountingOverflow)?;
        self.replacement_plan = plan;
        // Each chosen victim reduces at least one positive integer deficit.
        // Thus planning storage is bounded by the incoming record/targets,
        // not the number of cached records. Zero-contribution entries skip.
        let capacity = needed
            .into_iter()
            .try_fold(0usize, |n, part| n.checked_add(part))
            .ok_or(Pc4LookupGraphCacheError::AccountingOverflow)?;
        let mut victims = Vec::new();
        victims
            .try_reserve_exact(capacity.min(self.entries.len()))
            .map_err(|_| Pc4LookupGraphCacheError::AllocationFailed)?;
        let visits = self
            .entries
            .len()
            .checked_mul(2)
            .ok_or(Pc4LookupGraphCacheError::AccountingOverflow)?;
        for _ in 0..visits {
            self.replacement_clock %= self.entries.len();
            let entry = &self.entries[self.replacement_clock];
            self.replacement_clock += 1;
            if protected(entry.field_id) || entry.replacement_plan.get() == plan {
                continue;
            }
            if entry.referenced.replace(false) {
                continue;
            }
            let targets = entry.decoded_record.total_target_count();
            if needed[0] == 0
                && (needed[1] == 0 || entry.encoded_record.is_empty())
                && (needed[2] == 0 || targets == 0)
            {
                continue;
            }
            entry.replacement_plan.set(plan);
            victims.push(entry.field_id);
            usage.record_count -= 1;
            usage.encoded_graph_bytes -= entry.encoded_record.len();
            usage.decoded_target_count -= targets;
            usage.decoded_target_bytes -= targets * size_of::<u32>();
            needed = self.excess(usage);
            if needed == [0; 3] {
                return Ok((usage, victims));
            }
        }
        // Preserve the original budget error and all old records. Only CLOCK
        // hint bits changed; no semantic cache state or admission revision did.
        Ok((usage, victims))
    }

    fn excess(&self, usage: Pc4LookupGraphCacheUsage) -> [usize; 3] {
        [
            usage.record_count.saturating_sub(self.limits.max_records()),
            usage
                .encoded_graph_bytes
                .saturating_sub(self.limits.max_encoded_graph_bytes()),
            usage
                .decoded_target_count
                .saturating_sub(self.limits.max_decoded_target_bytes() / size_of::<u32>()),
        ]
    }
}
