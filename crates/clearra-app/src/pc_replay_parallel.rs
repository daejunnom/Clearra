//! Portable work over the pager's existing immutable replay languages. Only
//! the originating pager can reduce receipts into a complete product source.
use super::*;
use std::collections::BTreeMap;

#[path = "pc_replay_parallel_wire.rs"]
mod wire;

const COUNT: u64 = 1;
const PAGE: u64 = 2;
const GENERATION: u64 = 1;

#[derive(Clone)]
pub struct PcReplayParallelQuery {
    identity: [u8; 32],
    owner_identity: [u8; 32],
    generation: u64,
    batches: ReplayBatches,
    groups: Arc<[(u64, Vec<ExactReplayGraphLocation>)]>,
    projection: PcPathProjectionContext,
    patterns: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcReplayParallelTask {
    identity: [u8; 32],
    generation: u64,
    kind: u64,
    ordinal: usize,
    first: usize,
    length: usize,
}

pub struct PcReplayParallelReceipt {
    task: PcReplayParallelTask,
    count: Option<usize>,
    members: Vec<PcPathWitnessV2>,
}

pub(super) struct ReplayParallelCoordinator {
    query: PcReplayParallelQuery,
    window: usize,
    issue_geometry: usize,
    issue_pattern: usize,
    issued: VecDeque<(PcReplayParallelTask, Option<usize>)>,
    counts_complete: bool,
    page_tasks: Vec<PcReplayParallelTask>,
    page_issued: usize,
    page_received: BTreeMap<usize, Vec<PcPathWitnessV2>>,
}

pub struct PcReplayParallelWorker {
    query: PcReplayParallelQuery,
    task: Option<PcReplayParallelTask>,
    language: Option<ReplayLanguage>,
    members: Vec<PcPathWitnessV2>,
    maximum: u128,
    control: ExecutionControl,
}

fn mismatch() -> PcReplayPageError {
    "complete_replay_parallel_source_mismatch".into()
}

impl PcReplayParallelTask {
    pub fn routing_key(&self) -> [u8; 56] {
        let mut key = [0; 56];
        key[..32].copy_from_slice(&self.identity);
        key[32..40].copy_from_slice(&self.generation.to_le_bytes());
        key[40..48].copy_from_slice(&self.kind.to_le_bytes());
        key[48..].copy_from_slice(&(self.ordinal as u64).to_le_bytes());
        key
    }
}

impl PcReplayParallelQuery {
    pub fn identity(&self) -> ([u8; 32], u64) {
        (self.identity, self.generation)
    }
    pub fn checked_retained_bytes(&self) -> Option<u128> {
        (core::mem::size_of::<Self>() as u128)
            .checked_add(self.checked_group_bytes()?)?
            .checked_add(match &self.batches {
                ReplayBatches::Compact(batches) => batches.iter().try_fold(
                    bytes::<ExactScoringExecutionBatch>(batches.len()).ok()?,
                    |n, b| n.checked_add(b.checked_nested_retained_bytes()?),
                )?,
                ReplayBatches::FullHeight(batches) => batches.iter().try_fold(
                    bytes::<clearra_replay::FullHeightExecutionBatch>(batches.len()).ok()?,
                    |n, b| n.checked_add(b.checked_nested_retained_bytes()?),
                )?,
            })?
            .checked_add((4 * core::mem::size_of::<usize>()) as u128)
    }
    fn checked_group_bytes(&self) -> Option<u128> {
        self.groups.iter().try_fold(
            bytes::<(u64, Vec<ExactReplayGraphLocation>)>(self.groups.len()).ok()?,
            |n, (_, locations)| {
                n.checked_add(bytes::<ExactReplayGraphLocation>(locations.capacity()).ok()?)
            },
        )
    }
    fn validate_task(&self, task: PcReplayParallelTask) -> ReplayResult<(usize, usize)> {
        let geometry = task.ordinal / self.patterns;
        let pattern = task.ordinal % self.patterns;
        if task.identity != self.identity
            || task.generation != self.generation
            || geometry >= self.groups.len()
            || !matches!(task.kind, COUNT | PAGE)
            || (task.kind == COUNT && (task.first != 0 || task.length != 0))
            || (task.kind == PAGE
                && (task.length == 0
                    || task.length > PC_REPLAY_MEMBER_PAGE_SIZE
                    || task.first.checked_add(task.length).is_none()))
        {
            return Err(mismatch());
        }
        Ok((geometry, pattern))
    }
    pub fn checked_wire_bytes(&self) -> ReplayResult<usize> {
        wire::query_size(self)
    }
    /// Physical replay has at most 60 locks (24×10 / 4). Each projected lock
    /// carries bounded integer/board fields and two canonical trace strings;
    /// 1024 bytes per lock plus a 1024-byte witness header dominates their
    /// closed encodings. This reserves transport, not a product count cap.
    pub fn checked_task_receipt_wire_bound(&self) -> ReplayResult<usize> {
        let steps = match &self.batches {
            ReplayBatches::Compact(b) => b
                .iter()
                .flat_map(|b| b.graphs())
                .map(|g| g.identity().placement_count())
                .max()
                .unwrap_or(0),
            ReplayBatches::FullHeight(b) => b
                .iter()
                .flat_map(|b| b.execution().graphs())
                .try_fold(0usize, |largest, graph| {
                    let key =
                        clearra_core_domain::solution::ExtendedTilingSolutionKey::parse_canonical(
                            graph.candidate_key(),
                        )
                        .map_err(|_| mismatch())?;
                    Ok::<_, PcReplayPageError>(largest.max(key.placement_count()))
                })?,
        };
        steps
            .checked_add(1)
            .and_then(|n| n.checked_mul(1024))
            .and_then(|n| n.checked_mul(PC_REPLAY_MEMBER_PAGE_SIZE))
            .and_then(|n| n.checked_add(512))
            .ok_or_else(|| OVERFLOW.into())
    }
    pub fn encode(&self, guard: &mut impl FnMut(u128) -> bool) -> ReplayResult<Vec<u8>> {
        wire::encode_query(self, guard)
    }
    /// Decoding supplies execution input only. It cannot create a pager or a
    /// complete-path proof; promotion still requires the original Core owner.
    pub fn decode(bytes: &[u8], guard: &mut impl FnMut(u128) -> bool) -> ReplayResult<Self> {
        wire::decode_query(bytes, guard)
    }
}

impl PcReplayParallelTask {
    pub fn encode(&self) -> Vec<u8> {
        wire::encode_task(self)
    }
    pub fn decode(bytes: &[u8]) -> ReplayResult<Self> {
        wire::decode_task(bytes)
    }
}
impl PcReplayParallelReceipt {
    pub fn checked_retained_bytes(&self) -> Option<u128> {
        self.members.iter().try_fold(
            (core::mem::size_of::<Self>() as u128)
                .checked_add(bytes::<PcPathWitnessV2>(self.members.capacity()).ok()?)?,
            |n, member| n.checked_add(member.checked_retained_capacity_bytes()?),
        )
    }
    pub fn encode(&self, guard: &mut impl FnMut(u128) -> bool) -> ReplayResult<Vec<u8>> {
        wire::encode_receipt(self, guard)
    }
    pub fn decode(bytes: &[u8], guard: &mut impl FnMut(u128) -> bool) -> ReplayResult<Self> {
        wire::decode_receipt(bytes, guard)
    }
}

impl ReplayParallelCoordinator {
    pub(super) fn checked_additional_retained_bytes(&self) -> Option<u128> {
        let mut n = (core::mem::size_of::<Self>() as u128)
            .checked_add(self.query.checked_group_bytes()?)?
            .checked_add((2 * core::mem::size_of::<usize>()) as u128)?
            .checked_add(
                bytes::<(PcReplayParallelTask, Option<usize>)>(self.issued.capacity()).ok()?,
            )?
            .checked_add(bytes::<PcReplayParallelTask>(self.page_tasks.capacity()).ok()?)?;
        for members in self.page_received.values() {
            // BTreeMap node/link overhead is conservatively bounded per entry.
            n = n
                .checked_add(256)?
                .checked_add(bytes::<PcPathWitnessV2>(members.capacity()).ok()?)?;
            for member in members {
                n = n.checked_add(member.checked_retained_capacity_bytes()?)?;
            }
        }
        Some(n)
    }
}

impl PcReplaySourceBuildSession {
    pub fn checked_parallel_preparation_bytes(&self, workers: usize) -> Option<u128> {
        if self.parallel.is_some() || self.complete || workers == 0 {
            return Some(0);
        }
        let window = workers.checked_mul(32)?;
        bytes::<(u64, Vec<ExactReplayGraphLocation>)>(self.pending.len())
            .ok()?
            .checked_mul(2)?
            .checked_add(self.pending_location_bytes)?
            .checked_add(bytes::<(PcReplayParallelTask, Option<usize>)>(window).ok()?)?
            .checked_add(core::mem::size_of::<ReplayParallelCoordinator>() as u128)?
            .checked_add((2 * core::mem::size_of::<usize>()) as u128)
    }
    pub fn enable_parallel(&mut self, workers: usize) -> ReplayResult<()> {
        if self.parallel.is_some() || self.complete || workers == 0 {
            return Ok(());
        }
        if self.language.is_some()
            || self.next_geometry != 0
            || self.next_pattern != 0
            || !self.ready_counts.is_empty()
        {
            return Err("complete_replay_parallel_already_started".into());
        }
        let window = workers.checked_mul(32).ok_or(OVERFLOW)?;
        let group_bytes = self
            .checked_parallel_preparation_bytes(workers)
            .ok_or(OVERFLOW)?;
        self.guard_additional(group_bytes)?;
        let mut groups = Vec::new();
        groups
            .try_reserve_exact(self.pending.len())
            .map_err(|_| "complete_replay_allocation_failed")?;
        for (candidate, locations) in &self.pending {
            let mut copied = Vec::new();
            copied
                .try_reserve_exact(locations.len())
                .map_err(|_| "complete_replay_allocation_failed")?;
            copied.extend_from_slice(locations);
            groups.push((*candidate, copied));
        }
        let mut issued = VecDeque::new();
        issued
            .try_reserve_exact(window)
            .map_err(|_| "complete_replay_allocation_failed")?;
        let owner_identity: [u8; 32] = self.hasher.clone().finalize().into();
        let mut query = PcReplayParallelQuery {
            identity: [0; 32],
            owner_identity,
            generation: GENERATION,
            batches: self.source.batches.clone(),
            groups: groups.into(),
            projection: self.source.projection,
            patterns: self.source.materialized_pattern_count,
        };
        query.identity = wire::source_identity(&query)?;
        self.parallel = Some(ReplayParallelCoordinator {
            query,
            window,
            issue_geometry: 0,
            issue_pattern: 0,
            issued,
            counts_complete: false,
            page_tasks: Vec::new(),
            page_issued: 0,
            page_received: BTreeMap::new(),
        });
        self.guard_additional(0)
    }
    pub fn parallel_query(&self) -> Option<&PcReplayParallelQuery> {
        self.parallel.as_ref().map(|p| &p.query)
    }
    pub fn parallel_memory_envelope(&self) -> Option<(u128, u128)> {
        Some((
            self.source.maximum_bytes,
            self.checked_retained_capacity_bytes()?
                .checked_add(self.source.retained_external_bytes()?)?,
        ))
    }
    pub fn take_parallel_task(
        &mut self,
        control: &ExecutionControl,
    ) -> ReplayResult<Option<PcReplayParallelTask>> {
        if self.parallel.is_some() {
            self.advance_parallel_reduction(control)?;
        }
        let Some(parallel) = self.parallel.as_mut() else {
            return Ok(None);
        };
        if parallel.counts_complete {
            let task = parallel.page_tasks.get(parallel.page_issued).copied();
            parallel.page_issued += usize::from(task.is_some());
            return Ok(task);
        }
        if parallel.issued.len() == parallel.window {
            return Ok(None);
        }
        while parallel.issue_geometry < self.pending.len() {
            let geometry = parallel.issue_geometry;
            match self
                .source
                .next_possible_pattern(&parallel.query.groups[geometry].1, parallel.issue_pattern)?
            {
                Some(pattern) => {
                    parallel.issue_pattern = pattern + 1;
                    let task = PcReplayParallelTask {
                        identity: parallel.query.identity,
                        generation: parallel.query.generation,
                        kind: COUNT,
                        ordinal: geometry
                            .checked_mul(parallel.query.patterns)
                            .and_then(|n| n.checked_add(pattern))
                            .ok_or(OVERFLOW)?,
                        first: 0,
                        length: 0,
                    };
                    parallel.issued.push_back((task, None));
                    return Ok(Some(task));
                }
                None => {
                    parallel.issue_geometry += 1;
                    parallel.issue_pattern = 0;
                }
            }
        }
        Ok(None)
    }
    pub fn accept_parallel_receipt(
        &mut self,
        receipt: PcReplayParallelReceipt,
        control: &ExecutionControl,
    ) -> ReplayResult<()> {
        if self.complete {
            return Err(mismatch());
        }
        self.guard_additional(
            receipt
                .checked_retained_bytes()
                .and_then(|bytes| bytes.checked_add(256))
                .ok_or(OVERFLOW)?,
        )?;
        if control.is_cancelled() {
            return Err("complete_replay_cancelled".into());
        }
        let parallel = self.parallel.as_mut().ok_or_else(mismatch)?;
        let (geometry, pattern) = parallel.query.validate_task(receipt.task)?;
        let count = receipt.count.ok_or("complete_replay_cancelled")?;
        match receipt.task.kind {
            COUNT => {
                if parallel.counts_complete || !receipt.members.is_empty() {
                    return Err(mismatch());
                }
                let slot = parallel
                    .issued
                    .iter_mut()
                    .find(|slot| slot.0 == receipt.task)
                    .ok_or_else(mismatch)?;
                if slot.1.is_some() {
                    return Err(mismatch());
                }
                slot.1 = Some(count);
            }
            PAGE => {
                if !parallel.counts_complete
                    || !parallel.page_tasks[..parallel.page_issued].contains(&receipt.task)
                    || parallel.page_received.contains_key(&receipt.task.ordinal)
                    || receipt.members.len() != receipt.task.length
                    || self.source.geometries.first().is_none_or(|g| {
                        g.producer_candidate_id != parallel.query.groups[geometry].0
                            || g.patterns
                                .iter()
                                .find(|p| p.pattern_id == pattern)
                                .is_none_or(|p| p.witness_count != count)
                    })
                    || receipt.members.iter().any(|w| {
                        w.candidate_id() != 1
                            || w.producer_candidate_id() != parallel.query.groups[geometry].0
                            || w.pattern_id() != pattern
                            || w.steps().first().is_some_and(|step| {
                                step.board_before_mask() != parallel.query.projection.initial_board
                                    || step.input_cursor()
                                        != parallel.query.projection.initial_cursor
                                    || step.input_hold_piece()
                                        != parallel.query.projection.initial_hold
                            })
                    })
                {
                    return Err(mismatch());
                }
                parallel
                    .page_received
                    .insert(receipt.task.ordinal, receipt.members);
            }
            _ => return Err(mismatch()),
        }
        self.advance_parallel_reduction(control)?;
        self.guard_additional(0)
    }
    pub(super) fn advance_parallel_reduction(
        &mut self,
        control: &ExecutionControl,
    ) -> ReplayResult<bool> {
        if self.complete {
            return Ok(true);
        }
        if control.is_cancelled() {
            return Err("complete_replay_cancelled".into());
        }
        loop {
            let next = self.parallel.as_mut().and_then(|p| {
                p.issued
                    .front()
                    .and_then(|(task, count)| count.map(|count| (*task, count)))
            });
            let Some((task, count)) = next else {
                break;
            };
            self.parallel
                .as_mut()
                .expect("parallel owner")
                .issued
                .pop_front();
            let geometry = task.ordinal / self.source.materialized_pattern_count;
            let pattern = task.ordinal % self.source.materialized_pattern_count;
            while self.next_geometry < geometry {
                self.finish_geometry()?;
            }
            self.next_pattern = pattern;
            self.accept_count(count)?;
        }
        let counts_drained = self.parallel.as_ref().is_some_and(|p| {
            !p.counts_complete && p.issue_geometry == p.query.groups.len() && p.issued.is_empty()
        });
        if counts_drained {
            while self.next_geometry < self.pending.len() {
                self.finish_geometry()?;
            }
            let mut tasks = Vec::new();
            self.guard_additional(bytes::<PcReplayParallelTask>(PC_REPLAY_MEMBER_PAGE_SIZE)?)?;
            tasks
                .try_reserve_exact(PC_REPLAY_MEMBER_PAGE_SIZE)
                .map_err(|_| "complete_replay_allocation_failed")?;
            let parallel = self.parallel.as_ref().expect("parallel owner");
            if let Some(first) = self.source.geometries.first() {
                let geometry = parallel
                    .query
                    .groups
                    .iter()
                    .position(|g| g.0 == first.producer_candidate_id)
                    .ok_or_else(mismatch)?;
                let mut remaining = PC_REPLAY_MEMBER_PAGE_SIZE;
                for pattern in &first.patterns {
                    let length = remaining.min(pattern.witness_count);
                    tasks.push(PcReplayParallelTask {
                        identity: parallel.query.identity,
                        generation: parallel.query.generation,
                        kind: PAGE,
                        ordinal: geometry
                            .checked_mul(parallel.query.patterns)
                            .and_then(|n| n.checked_add(pattern.pattern_id))
                            .ok_or(OVERFLOW)?,
                        first: 0,
                        length,
                    });
                    remaining -= length;
                    if remaining == 0 {
                        break;
                    }
                }
            }
            let parallel = self.parallel.as_mut().expect("parallel owner");
            parallel.counts_complete = true;
            parallel.page_tasks = tasks;
        }
        let pages_drained = self
            .parallel
            .as_ref()
            .is_some_and(|p| p.counts_complete && p.page_received.len() == p.page_tasks.len());
        if pages_drained {
            let total = self
                .parallel
                .as_ref()
                .expect("parallel owner")
                .page_received
                .values()
                .map(Vec::len)
                .sum::<usize>();
            self.guard_additional(bytes::<PcPathWitnessV2>(total)?)?;
            let mut pages = core::mem::take(
                &mut self
                    .parallel
                    .as_mut()
                    .expect("parallel owner")
                    .page_received,
            );
            self.source
                .first_members
                .try_reserve_exact(total)
                .map_err(|_| "complete_replay_allocation_failed")?;
            while let Some((_, members)) = pages.pop_first() {
                for member in members {
                    self.source.retained_first_member_nested_bytes = self
                        .source
                        .retained_first_member_nested_bytes
                        .checked_add(member.checked_retained_capacity_bytes().ok_or(OVERFLOW)?)
                        .ok_or(OVERFLOW)?;
                    self.source.first_members.push(member);
                }
            }
            self.hasher
                .update((self.source.geometries.len() as u128).to_le_bytes());
            self.hasher.update(self.source.witness_count.to_le_bytes());
            self.hasher
                .update((self.source.materialized_pattern_count as u128).to_le_bytes());
            self.guard_additional(64)?;
            self.source.identity_sha256 = format!("{:x}", self.hasher.clone().finalize());
            self.complete = true;
        }
        self.report_progress(control);
        self.guard_additional(0)?;
        Ok(self.complete)
    }
}

impl PcReplayParallelWorker {
    pub fn new(query: PcReplayParallelQuery, maximum: u128) -> ReplayResult<Self> {
        let required = query
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(core::mem::size_of::<Self>() as u128))
            .and_then(|bytes| bytes.checked_add(Self::control_owner_bytes()))
            .ok_or(OVERFLOW)?;
        ensure_peak(required, maximum, &mut |_| true)?;
        let worker = Self {
            query,
            task: None,
            language: None,
            members: Vec::new(),
            maximum,
            control: ExecutionControl::default(),
        };
        worker.guard(0, &mut |_| true)?;
        Ok(worker)
    }
    fn control_owner_bytes() -> u128 {
        clearra_core_domain::execution_cancellation::ComputeBudget::SHARED_ALLOCATION_RETAINED_BYTES
            + (core::mem::size_of::<core::sync::atomic::AtomicU32>()
                + 2 * core::mem::size_of::<usize>()) as u128
    }
    pub fn checked_retained_bytes(&self) -> Option<u128> {
        let mut n = (core::mem::size_of::<Self>() as u128)
            .checked_add(self.query.checked_retained_bytes()?)?
            .checked_add(Self::control_owner_bytes())?
            .checked_add(bytes::<PcPathWitnessV2>(self.members.capacity()).ok()?)?;
        for m in &self.members {
            n = n.checked_add(m.checked_retained_capacity_bytes()?)?;
        }
        if let Some(language) = &self.language {
            n = n.checked_add(language.checked_retained_bytes()?)?;
        }
        Some(n)
    }
    fn guard(&self, extra: u128, guard: &mut impl FnMut(u128) -> bool) -> ReplayResult<()> {
        ensure_peak(
            self.checked_retained_bytes()
                .and_then(|n| n.checked_add(extra))
                .ok_or(OVERFLOW)?,
            self.maximum,
            guard,
        )
    }
    pub fn has_active_task(&self) -> bool {
        self.task.is_some()
    }
    pub fn start(
        &mut self,
        task: PcReplayParallelTask,
        guard: &mut impl FnMut(u128) -> bool,
    ) -> ReplayResult<()> {
        if self.task.is_some() {
            return Err("complete_replay_parallel_worker_busy".into());
        }
        let (geometry, pattern) = self.query.validate_task(task)?;
        let locations = &self.query.groups[geometry].1;
        self.guard(bytes::<ExactReplayGraphLocation>(locations.len())?, guard)?;
        let mut copied = Vec::new();
        copied
            .try_reserve_exact(locations.len())
            .map_err(|_| "complete_replay_allocation_failed")?;
        copied.extend_from_slice(locations);
        let external = self.checked_retained_bytes().ok_or(OVERFLOW)?;
        let language = self
            .query
            .batches
            .new_language(
                copied,
                pattern,
                ExactReplayMaterializationLimits::new(
                    MAX_GEOMETRY_EXECUTIONS,
                    256,
                    self.maximum.saturating_sub(external),
                ),
                &mut |peak| engine_guard(external, peak, self.maximum, guard),
            )
            .map_err(|e| replay_engine_error(e, external))?;
        self.language = Some(language);
        self.task = Some(task);
        self.guard(0, guard)
    }
    pub fn advance(
        &mut self,
        work: usize,
        cancelled: bool,
        guard: &mut impl FnMut(u128) -> bool,
    ) -> ReplayResult<Option<PcReplayParallelReceipt>> {
        let task = self.task.ok_or("complete_replay_parallel_worker_idle")?;
        if cancelled {
            self.language = None;
            self.members.clear();
            self.task = None;
            return Ok(Some(PcReplayParallelReceipt {
                task,
                count: None,
                members: Vec::new(),
            }));
        }
        if work == 0 {
            return Ok(None);
        }
        let mut language = self.language.take().ok_or_else(mismatch)?;
        let external = self.checked_retained_bytes().ok_or(OVERFLOW)?;
        let ready = language
            .advance(work.min(MAX_ADVANCE_WORK), &self.control, &mut |peak| {
                engine_guard(external, peak, self.maximum, guard)
            })
            .map_err(|e| replay_engine_error(e, external))?;
        if !ready {
            self.language = Some(language);
            return Ok(None);
        }
        let count = language.count().ok_or_else(mismatch)?;
        if task.kind == PAGE {
            if task
                .first
                .checked_add(task.length)
                .is_none_or(|last| last > count)
            {
                return Err(mismatch());
            }
            // One projection per slice keeps cancellation and host timers live.
            if self.members.len() < task.length {
                let candidate = self.query.groups[task.ordinal / self.query.patterns].0;
                let member = language.select(
                    candidate,
                    task.first + self.members.len(),
                    &self.control,
                    external,
                    &mut |peak| engine_guard(external, peak, self.maximum, guard),
                )?;
                let projected = member.checked_projection_bytes().ok_or(OVERFLOW)?;
                let peak = language
                    .checked_retained_bytes()
                    .and_then(|n| n.checked_add(member.checked_owned_bytes()?))
                    .and_then(|n| n.checked_add(projected.checked_mul(REPLAY_PUBLIC_PAGE_RESERVE)?))
                    .and_then(|n| {
                        n.checked_add(bytes::<PcPathWitnessV2>(self.members.len() + 1).ok()?)
                    })
                    .ok_or(OVERFLOW)?;
                self.guard(peak, guard)?;
                self.members
                    .try_reserve_exact(1)
                    .map_err(|_| "complete_replay_allocation_failed")?;
                self.members
                    .push(member.project(self.query.projection, self.query.patterns, 1)?);
                self.language = Some(language);
                return Ok(None);
            }
        }
        self.task = None;
        let receipt = PcReplayParallelReceipt {
            task,
            count: Some(count),
            members: core::mem::take(&mut self.members),
        };
        drop(language);
        self.guard(receipt.checked_retained_bytes().ok_or(OVERFLOW)?, guard)?;
        Ok(Some(receipt))
    }
}
