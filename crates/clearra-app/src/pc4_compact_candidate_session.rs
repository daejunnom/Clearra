//! SRP: compose compact candidate work with a bounded set of independent,
//! qualified lookup sessions. Hosts own I/O; this owner never waits on HTTP,
//! starts fallback, or labels a missing/failed record as an empty adjacency.
use crate::{
    pc4_lookup_graph_runtime_adapter::{Pc4LookupGraphCache, Pc4LookupGraphCacheLimits},
    pc_candidate_page_boundary::{
        compact_graph_union::{
            CompactGraphUnionLimits, CompactGraphUnionStep, Pc4CompactGraphUnion,
        },
        graph_candidate_adapter::Pc4GraphCandidateGuard,
    },
    AppOnlinePc4LookupSession, AppOnlinePc4LookupStep, AppOnlinePc4RangeDisposition,
    AppQualifiedPc4LookupHit, Pc4OfflineFallbackAuthorization, Pc4OnlineLookupRequest,
    Pc4PreparedOnlineInput, PcCandidateReducerInput, PcCandidateSourceBinding,
};
use clearra_pc4_tablebase::{
    LookupSessionId, PinnedPc4Generation, RangeAdmissionAttempt, RangeAdmissionGuard,
    RangeAdmissionInput, RangeAdmissionLimits, RangeRequest,
};
use core::num::{NonZeroU16, NonZeroU32, NonZeroUsize};

pub(crate) struct CompactSessionLimits {
    pub union: CompactGraphUnionLimits,
    pub cache: Pc4LookupGraphCacheLimits,
    pub ranges: RangeAdmissionLimits,
    pub concurrent_lookups: NonZeroUsize,
}

struct ActiveLookup {
    field: u32,
    owner: AppOnlinePc4LookupSession,
    pending: Option<RangeRequest>,
    ordinal: u32,
}

pub(crate) struct Pc4CompactCandidateSession {
    generation: PinnedPc4Generation,
    limits: CompactSessionLimits,
    cache: Pc4LookupGraphCache,
    union: Option<Pc4CompactGraphUnion>,
    lookups: Vec<ActiveLookup>,
    next_lookup: u64,
    started_lookups: usize,
    complete: bool,
    failed: bool,
}

impl Pc4CompactCandidateSession {
    pub(crate) fn start<G: Pc4GraphCandidateGuard>(
        generation: PinnedPc4Generation,
        source: &PcCandidateSourceBinding,
        prepared: &Pc4PreparedOnlineInput,
        initial: AppQualifiedPc4LookupHit,
        limits: CompactSessionLimits,
        guard: &G,
    ) -> Result<Option<Self>, &'static str> {
        // This is a host-queue bound, not a claim about available CPU cores.
        if limits.concurrent_lookups.get() > 16 {
            return Err("pc4_compact_session_concurrency_invalid");
        }
        // At most two field records are pinned by each resident work item.
        // The record bound must admit that set; encoded/decoded budgets are
        // independently checked by every actual replacement plan.
        if limits
            .union
            .resident_work
            .get()
            .checked_mul(2)
            .is_none_or(|pins| pins > limits.cache.max_records())
        {
            return Err("pc4_compact_session_dependency_capacity_invalid");
        }
        let mut cache = Pc4LookupGraphCache::new(
            generation.activated_snapshot(),
            prepared.target().clone(),
            limits.cache,
        )
        .map_err(|e| e.reason())?;
        let (target, hit) = initial.into_parts();
        if &target != prepared.target() {
            return Err("pc4_compact_session_initial_target_mismatch");
        }
        let field = hit.field_id;
        let next_lookup = hit
            .lookup_session
            .get()
            .checked_add(1)
            .ok_or("pc4_compact_session_id_overflow")?;
        cache.admit(&target, hit).map_err(|e| e.reason())?;
        let Some(union) =
            Pc4CompactGraphUnion::prepare(source, prepared, &cache, field, limits.union, guard)
                .map_err(|e| e.reason())?
        else {
            return Ok(None);
        };
        let mut lookups = Vec::new();
        lookups
            .try_reserve_exact(limits.concurrent_lookups.get())
            .map_err(|_| "pc4_compact_session_allocation_failed")?;
        Ok(Some(Self {
            generation,
            limits,
            cache,
            union: Some(union),
            lookups,
            next_lookup,
            started_lookups: 0,
            complete: false,
            failed: false,
        }))
    }

    pub(crate) fn pending_ranges(&self) -> impl Iterator<Item = &RangeRequest> {
        self.lookups
            .iter()
            .filter_map(|lookup| lookup.pending.as_ref())
    }

    pub(crate) fn has_ready_work(&self) -> bool {
        !self.failed
            && !self.complete
            && (self.lookups.iter().any(|lookup| lookup.pending.is_none())
                || self
                    .union
                    .as_ref()
                    .is_some_and(|union| union.has_ready_work()))
    }

    pub(crate) fn advance<G: Pc4GraphCandidateGuard>(
        &mut self,
        work: NonZeroUsize,
        guard: &G,
    ) -> Result<bool, &'static str> {
        let result = self.advance_inner(work, guard);
        if result.is_err() {
            self.fail();
        }
        result
    }

    fn advance_inner<G: Pc4GraphCandidateGuard>(
        &mut self,
        work: NonZeroUsize,
        guard: &G,
    ) -> Result<bool, &'static str> {
        if self.failed {
            return Err("pc4_compact_session_terminated");
        }
        let union = self
            .union
            .as_mut()
            .ok_or("pc4_compact_session_terminated")?;
        union.check_current(guard).map_err(|e| e.reason())?;
        if self.complete {
            return Ok(true);
        }
        let mut index = 0;
        while index < self.lookups.len() {
            // A pending response never blocks the other sessions or CPU work.
            if self.lookups[index].pending.is_some() {
                index += 1;
                continue;
            }
            match self.lookups[index].owner.step() {
                AppOnlinePc4LookupStep::NeedRange(range) => {
                    self.lookups[index].pending = Some(range);
                    index += 1;
                }
                AppOnlinePc4LookupStep::Hit(hit) => {
                    let (target, hit) = hit.into_parts();
                    if hit.field_id != self.lookups[index].field {
                        return Err("pc4_compact_session_field_mismatch");
                    }
                    self.cache
                        .admit_replacing_unprotected(&target, hit, &|field| {
                            union.protects_field(field)
                        })
                        .map_err(|e| e.reason())?;
                    self.lookups.swap_remove(index);
                }
                AppOnlinePc4LookupStep::Miss => return Err("pc4_online_field_miss"),
                AppOnlinePc4LookupStep::Failed(error) => return Err(error.reason()),
                AppOnlinePc4LookupStep::Cancelled => return Err("pc4_compact_session_cancelled"),
            }
        }
        // Only new resident admission is throttled. Ready dependency owners
        // must keep progressing even when the I/O window is full; otherwise
        // their pins/credits could never be released.
        let step = union
            .advance(
                &self.cache,
                NonZeroUsize::new(work.get().min(64)).unwrap(),
                guard,
            )
            .map_err(|e| e.reason())?;
        if step == CompactGraphUnionStep::Complete {
            if !self.lookups.is_empty() {
                return Err("pc4_compact_session_incomplete_io");
            }
            self.complete = true;
            return Ok(true);
        }
        // At most one live lookup per demanded field. Include occupied slots
        // in the read window so old IDs cannot starve new independent demands.
        let demands = union
            .pending_fields(self.limits.concurrent_lookups.get() + self.lookups.len())
            .map_err(|e| e.reason())?;
        for field in demands {
            if self.lookups.len() == self.limits.concurrent_lookups.get() {
                break;
            }
            if self.cache.contains_field_id(field)
                || self.lookups.iter().any(|entry| entry.field == field)
            {
                continue;
            }
            // A demand must originate in a charged graph work unit. Existing
            // finite graph work and host HTTP byte/request budgets remain;
            // a cumulative cache-size count is not an I/O residency budget.
            if self.started_lookups >= union.usage().work {
                return Err("pc4_compact_session_lookup_without_work");
            }
            let id =
                LookupSessionId::new(self.next_lookup).ok_or("pc4_compact_session_id_overflow")?;
            self.next_lookup = self
                .next_lookup
                .checked_add(1)
                .ok_or("pc4_compact_session_id_overflow")?;
            let owner = AppOnlinePc4LookupSession::start(
                self.generation.clone(),
                Pc4OnlineLookupRequest::from_field_id(
                    id,
                    self.cache.target().clone(),
                    field,
                    self.limits.ranges,
                    Pc4OfflineFallbackAuthorization::NotAuthorized,
                ),
            )
            .map_err(|e| e.reason())?;
            let AppOnlinePc4LookupStep::NeedRange(range) = owner.step() else {
                return Err("pc4_compact_session_lookup_start_invalid");
            };
            self.lookups.push(ActiveLookup {
                field,
                owner,
                pending: Some(range),
                ordinal: 0,
            });
            self.started_lookups += 1;
        }
        union.check_current(guard).map_err(|e| e.reason())?;
        Ok(false)
    }

    pub(crate) fn admit<G: Pc4GraphCandidateGuard + RangeAdmissionGuard>(
        &mut self,
        lookup_id: u64,
        request_id: u64,
        input: RangeAdmissionInput,
        guard: &G,
    ) -> Result<(), &'static str> {
        if self.failed || self.complete {
            return Err("pc4_compact_session_terminated");
        }
        self.union
            .as_ref()
            .ok_or("pc4_compact_session_terminated")?
            .check_current(guard)
            .map_err(|e| e.reason())?;
        let entry = self
            .lookups
            .iter_mut()
            .find(|entry| {
                entry.pending.as_ref().is_some_and(|r| {
                    r.lookup_session().get() == lookup_id && r.request_id() == request_id
                })
            })
            .ok_or("pc4_online_response_id_mismatch")?;
        let ordinal = entry
            .ordinal
            .checked_add(1)
            .and_then(NonZeroU32::new)
            .ok_or("pc4_online_request_count_overflow")?;
        let result = entry.owner.admit_range(
            RangeAdmissionAttempt::new(ordinal, NonZeroU16::new(1).unwrap()),
            input,
            guard,
        );
        match result {
            Ok(AppOnlinePc4RangeDisposition::PartialContentSupplied) => {
                entry.ordinal = ordinal.get();
                entry.pending = None;
                Ok(())
            }
            Ok(_) => {
                self.fail();
                Err("pc4_compact_session_range_failed")
            }
            Err(error) => {
                let reason = error.reason();
                self.fail();
                Err(reason)
            }
        }
    }

    pub(crate) fn into_reducer_input<G: Pc4GraphCandidateGuard>(
        mut self,
        guard: &G,
    ) -> Result<PcCandidateReducerInput, &'static str> {
        if self.failed || !self.complete || !self.lookups.is_empty() {
            return Err("pc4_compact_session_incomplete");
        }
        // Consume/drop the lookup cache before downstream product ownership.
        let union = self.union.take().ok_or("pc4_compact_session_terminated")?;
        drop(self);
        union.into_reducer_input(guard).map_err(|e| e.reason())
    }

    fn fail(&mut self) {
        self.failed = true;
        self.complete = false;
        self.lookups.clear();
        self.union = None;
    }
}
