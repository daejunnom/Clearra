//! Bounded page request ownership and lexical rank selection, not a GUI solver.
use super::*;

struct PendingPage {
    geometry: usize,
    member: usize,
    start: usize,
    end: usize,
    pattern_index: usize,
    metadata: PcReplayPageMetadata,
    witnesses: Vec<clearra_host_contract::PcPathWitnessPayload>,
}

#[derive(Debug)]
pub enum PcReplayPageAdvance {
    Pending { work_steps: u64 },
    Completed(PcReplayPagePayload),
    Cancelled { work_steps: u64 },
}

pub struct PcReplayPageStore {
    source: Arc<PcReplayPageSource>,
    current_geometry: Option<usize>,
    current_pattern: Option<usize>,
    language: Option<ExactReplayLanguageSession>,
    pending: Option<PendingPage>,
}

impl core::fmt::Debug for PcReplayPageStore {
    fn fmt(&self, output: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        output
            .debug_struct("PcReplayPageStore")
            .field("source", &self.source.identity_sha256)
            .field("pending", &self.pending.is_some())
            .finish()
    }
}

impl PcReplayPageStore {
    pub fn new(source: Arc<PcReplayPageSource>) -> Self {
        Self {
            source,
            current_geometry: None,
            current_pattern: None,
            language: None,
            pending: None,
        }
    }
    pub fn source(&self) -> &Arc<PcReplayPageSource> {
        &self.source
    }
    pub fn checked_host_entry_bytes(&self) -> Option<u128> {
        self.checked_retained_capacity_bytes()?
            .checked_add(self.source.retained_external_bytes()?)
    }
    pub fn cancel_page(&mut self) {
        self.pending = None;
        self.language = None;
        self.current_geometry = None;
        self.current_pattern = None;
    }
    pub fn advance_page(
        &mut self,
        geometry: usize,
        member: usize,
        work: usize,
        control: &ExecutionControl,
    ) -> ReplayResult<PcReplayPageAdvance> {
        self.advance_page_with_memory_guard(geometry, member, work, control, &mut |_| true)
    }

    pub fn advance_page_with_memory_guard(
        &mut self,
        geometry: usize,
        member: usize,
        work: usize,
        control: &ExecutionControl,
        guard: &mut impl FnMut(u128) -> bool,
    ) -> ReplayResult<PcReplayPageAdvance> {
        if control.is_cancelled() {
            self.cancel_page();
            return Ok(PcReplayPageAdvance::Cancelled { work_steps: 0 });
        }
        if let Some(pending) = &self.pending {
            // Do not silently relabel existing in-flight work. The original
            // request can resume after an unrelated/stale ordinal is rejected.
            if pending.geometry != geometry || pending.member != member {
                return Err("pc replay pending request mismatch".into());
            }
        } else {
            self.guard_page(4096, 0, guard)?;
            let metadata = self.source.page_metadata(geometry, member)?;
            let start = member
                .checked_sub(1)
                .and_then(|n| n.checked_mul(PC_REPLAY_MEMBER_PAGE_SIZE))
                .ok_or("pc replay invalid member page")?;
            let end = start
                .checked_add(PC_REPLAY_MEMBER_PAGE_SIZE)
                .ok_or(OVERFLOW)?
                .min(self.source.geometries[geometry - 1].witness_count);
            self.guard_page(
                metadata
                    .checked_retained_capacity_bytes()
                    .and_then(|n| {
                        n.checked_add(
                            bytes::<clearra_host_contract::PcPathWitnessPayload>(end - start)
                                .ok()?,
                        )
                    })
                    .ok_or(OVERFLOW)?,
                0,
                guard,
            )?;
            let mut witnesses = Vec::new();
            witnesses
                .try_reserve_exact(end - start)
                .map_err(|_| "complete_replay_allocation_failed")?;
            let pending = PendingPage {
                geometry,
                member,
                start,
                end,
                pattern_index: 0,
                metadata,
                witnesses,
            };
            self.guard_page(pending_bytes(&pending)?, 0, guard)?;
            self.pending = Some(pending);
        }
        let mut pending = self.pending.take().expect("installed pending page");
        let result = self.advance_pending(
            &mut pending,
            work.max(1).min(MAX_ADVANCE_WORK),
            control,
            guard,
        );
        match result {
            Ok(Some(work_steps)) => {
                self.guard_page(pending_bytes(&pending)?, 0, guard)?;
                self.pending = Some(pending);
                Ok(PcReplayPageAdvance::Pending { work_steps })
            }
            Ok(None) => {
                if pending.witnesses.len() != pending.end - pending.start {
                    return Err("pc replay member page count mismatch".into());
                }
                self.guard_page(
                    pending_bytes(&pending)?.checked_add(128).ok_or(OVERFLOW)?,
                    0,
                    guard,
                )?;
                let page = PcReplayPagePayload {
                    metadata: pending.metadata,
                    witness_count: self.source.witness_count.to_string(),
                    materialized_pattern_count: self.source.materialized_pattern_count.to_string(),
                    witnesses: pending.witnesses,
                };
                self.guard_page(
                    page.checked_retained_capacity_bytes().ok_or(OVERFLOW)?,
                    0,
                    guard,
                )?;
                Ok(PcReplayPageAdvance::Completed(page))
            }
            Err(error) if error.code() == "complete_replay_cancelled" => {
                self.cancel_page();
                Ok(PcReplayPageAdvance::Cancelled { work_steps: 0 })
            }
            Err(error) => {
                self.cancel_page();
                Err(error)
            }
        }
    }

    fn advance_pending(
        &mut self,
        pending: &mut PendingPage,
        work: usize,
        control: &ExecutionControl,
        guard: &mut impl FnMut(u128) -> bool,
    ) -> ReplayResult<Option<u64>> {
        let quantum = CooperativeWorkQuantum::start(ADVANCE_QUANTUM_MILLIS);
        for step in 0..work {
            if control.is_cancelled() {
                return Err("complete_replay_cancelled".into());
            }
            if step != 0 && step % QUANTUM_CHECK_INTERVAL == 0 && quantum.is_exhausted() {
                return Ok(Some(step as u64));
            }
            if pending.witnesses.len() == pending.end - pending.start {
                return Ok(None);
            }
            let geometry_index = pending.geometry - 1;
            let geometry = &self.source.geometries[geometry_index];
            let candidate_id = geometry.producer_candidate_id;
            let pattern = *geometry
                .patterns
                .get(pending.pattern_index)
                .ok_or("pc replay member page count mismatch")?;
            let pattern_start = pattern
                .end_offset
                .checked_sub(pattern.witness_count)
                .ok_or(OVERFLOW)?;
            let absolute = pending
                .start
                .checked_add(pending.witnesses.len())
                .ok_or(OVERFLOW)?;
            if absolute >= pattern.end_offset {
                pending.pattern_index += 1;
                continue;
            }
            if self.current_geometry != Some(geometry_index)
                || self.current_pattern != Some(pattern.pattern_id)
            {
                // Old and replacement memo never overlap. The partial public
                // page remains live and is included in the replacement guard.
                self.language = None;
                self.current_geometry = None;
                self.current_pattern = None;
                let additional = (core::mem::size_of::<Self>() as u128)
                    .checked_add(
                        pending_bytes(pending)?
                            .checked_mul(REPLAY_PUBLIC_PAGE_RESERVE)
                            .ok_or(OVERFLOW)?,
                    )
                    .and_then(|n| n.checked_add((2 * core::mem::size_of::<usize>()) as u128))
                    .ok_or(OVERFLOW)?;
                self.language = Some(self.source.new_language(
                    &geometry.locations,
                    pattern.pattern_id,
                    additional,
                    guard,
                )?);
                self.current_geometry = Some(geometry_index);
                self.current_pattern = Some(pattern.pattern_id);
            }
            let mut language = self.language.take().expect("installed language");
            let public = pending_bytes(pending)?;
            let external = self
                .checked_retained_capacity_bytes()
                .and_then(|n| n.checked_add(self.source.retained_external_bytes()?))
                .and_then(|n| n.checked_add(public.checked_mul(REPLAY_PUBLIC_PAGE_RESERVE)?))
                .ok_or(OVERFLOW)?;
            let maximum = self.source.maximum_bytes;
            let ready = language
                .advance(1, control, &mut |peak| {
                    engine_guard(external, peak, maximum, guard)
                })
                .map_err(|error| replay_engine_error(error, external))?;
            if !ready {
                self.language = Some(language);
                continue;
            }
            if language.count() != Some(pattern.witness_count) {
                return Err("pc replay manifest count mismatch".into());
            }
            let rank = absolute
                .checked_sub(pattern_start)
                .ok_or("pc replay rank outside current pattern")?;
            let member = language
                .select(rank, control, &mut |peak| {
                    engine_guard(external, peak, maximum, guard)
                })
                .map_err(|error| replay_engine_error(error, external))?;
            let execution = into_core(candidate_id, member)?;
            self.language = Some(language);
            let retained_execution = (core::mem::size_of::<CorePostProcessExecution>() as u128)
                .checked_add(execution.checked_nested_retained_bytes().ok_or(OVERFLOW)?)
                .ok_or(OVERFLOW)?;
            let projection_peak =
                checked_execution_projection_peak_bytes(&execution).ok_or(OVERFLOW)?;
            self.guard_page(
                public,
                projection_peak
                    .checked_add(retained_execution)
                    .ok_or(OVERFLOW)?,
                guard,
            )?;
            let witness = project_execution_with_context(
                self.source.projection,
                &execution,
                self.source.materialized_pattern_count,
                u64::try_from(pending.geometry)
                    .map_err(|_| "pc replay candidate identity overflow")?,
            )?;
            let actual = (core::mem::size_of::<PcPathWitnessV2>() as u128)
                .checked_add(witness.checked_retained_capacity_bytes().ok_or(OVERFLOW)?)
                .and_then(|n| n.checked_add(retained_execution))
                .ok_or(OVERFLOW)?;
            self.guard_page(public, actual, guard)?;
            let payload = pc_path_witness_payload(&witness);
            self.guard_page(
                public
                    .checked_add(payload.checked_retained_capacity_bytes().ok_or(OVERFLOW)?)
                    .ok_or(OVERFLOW)?,
                actual,
                guard,
            )?;
            pending.witnesses.push(payload);
        }
        Ok(Some(work as u64))
    }

    /// Synchronous convenience retained only for CLI/native callers. Browser
    /// and Desktop host adapters must use the bounded Pending interface.
    pub fn page(
        &mut self,
        geometry: usize,
        member: usize,
        control: &ExecutionControl,
    ) -> ReplayResult<PcReplayPagePayload> {
        loop {
            match self.advance_page(geometry, member, MAX_ADVANCE_WORK, control)? {
                PcReplayPageAdvance::Pending { .. } => (),
                PcReplayPageAdvance::Completed(page) => return Ok(page),
                PcReplayPageAdvance::Cancelled { .. } => {
                    return Err("complete_replay_cancelled".into());
                }
            }
        }
    }
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (core::mem::size_of::<Self>() as u128)
            .checked_add(self.source.checked_retained_capacity_bytes()?)?
            .checked_add((2 * core::mem::size_of::<usize>()) as u128)?
            .checked_add(match &self.language {
                Some(language) => language.checked_retained_bytes()?,
                None => 0,
            })?
            .checked_add(match &self.pending {
                Some(pending) => pending_bytes(pending)
                    .ok()?
                    .checked_mul(REPLAY_PUBLIC_PAGE_RESERVE)?,
                None => 0,
            })
    }
    fn guard_page(
        &self,
        public: u128,
        temporary: u128,
        guard: &mut impl FnMut(u128) -> bool,
    ) -> ReplayResult<()> {
        ensure_peak(
            self.checked_retained_capacity_bytes()
                .and_then(|n| n.checked_add(self.source.retained_external_bytes()?))
                .and_then(|n| {
                    n.checked_add(
                        public
                            .checked_add(temporary)?
                            .checked_mul(REPLAY_PUBLIC_PAGE_RESERVE)?,
                    )
                })
                .ok_or(OVERFLOW)?,
            self.source.maximum_bytes,
            guard,
        )
    }
}

fn pending_bytes(pending: &PendingPage) -> ReplayResult<u128> {
    let mut retained = (core::mem::size_of::<PendingPage>() as u128)
        .checked_add(
            pending
                .metadata
                .checked_retained_capacity_bytes()
                .ok_or(OVERFLOW)?,
        )
        .and_then(|n| {
            n.checked_add(
                bytes::<clearra_host_contract::PcPathWitnessPayload>(pending.witnesses.capacity())
                    .ok()?,
            )
        })
        .ok_or(OVERFLOW)?;
    for witness in &pending.witnesses {
        retained = retained
            .checked_add(witness.checked_retained_capacity_bytes().ok_or(OVERFLOW)?)
            .ok_or(OVERFLOW)?;
    }
    Ok(retained)
}
