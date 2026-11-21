//! Query-bound exact replay manifests. Only one cell's language memo is live.
use crate::pc_path_result::{
    checked_execution_projection_peak_bytes, pc_path_witness_payload,
    project_canonical_execution_with_context as project_execution_with_context,
    PcPathProjectionContext, PcPathWitnessV2,
};
use crate::pc_replay_page_error::{replay_engine_error, PcReplayPageError};
use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_executor::performance::CooperativeWorkQuantum;
use clearra_core_executor::{CoreExecutionResult, CorePostProcessExecution};
use clearra_host_contract::{PcReplayPageMetadata, PcReplayPagePayload};
use clearra_postprocess::{
    ExactReplayGraphLocation, ExactReplayLanguageSession, ExactReplayMaterializationError,
    ExactReplayMaterializationLimits,
};
use clearra_problem::SearchProblem;
use clearra_replay::ExactScoringExecutionBatch;
use sha2::{Digest, Sha256};
use std::sync::Arc;

#[path = "pc_replay_source_batches.rs"]
mod batches;
use batches::{PreparedReplayBatches, ReplayBatches, ReplayLanguage};

#[path = "pc_replay_inventory.rs"]
mod inventory;
use inventory::P7P4InventoryIndex;

#[path = "pc_replay_page_store.rs"]
mod store;
pub use store::{PcReplayPageAdvance, PcReplayPageStore};

pub const PC_REPLAY_MEMBER_PAGE_CONTRACT: &str = "pc-replay-member-page.v2";
pub const PC_REPLAY_MEMBER_PAGE_SIZE: usize = 100;
const REPLAY_PUBLIC_PAGE_RESERVE: u128 = 16;
// Preserve the previous raw cell-path and replay-depth limits.
const MAX_GEOMETRY_EXECUTIONS: usize = 1_000_000;
// A caller may amortize transport over useful work, but never drain the whole
// manifest in one turn. The clock bounds the batch between atomic primitives;
// an allocation/hash probe within a primitive can still exceed that quantum.
const MAX_ADVANCE_WORK: usize = 8192;
const ADVANCE_QUANTUM_MILLIS: u32 = 8;
const QUANTUM_CHECK_INTERVAL: usize = 64;
const OVERFLOW: &str = "complete_replay_memory_projection_overflow";
type ReplayResult<T> = Result<T, PcReplayPageError>;

#[derive(Clone, Debug, Eq, PartialEq)]
struct GeometryManifest {
    producer_candidate_id: u64,
    locations: Vec<ExactReplayGraphLocation>,
    witness_count: usize,
    patterns: Vec<PatternManifest>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PatternManifest {
    pattern_id: usize,
    witness_count: usize,
    end_offset: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcReplayPageSource {
    problem_id: String,
    projection: PcPathProjectionContext,
    batches: ReplayBatches,
    retained_graph_bytes: u128,
    retained_manifest_nested_bytes: u128,
    retained_first_member_nested_bytes: u128,
    geometries: Vec<GeometryManifest>,
    materialized_pattern_count: usize,
    witness_count: u128,
    identity_sha256: String,
    first_members: Vec<PcPathWitnessV2>,
    maximum_bytes: u128,
    // Original Core is live during manifest construction. The same physical
    // compact or full-height graph is charged to batches, not duplicated here.
    // Keeping the remaining reserve for later pages is conservative as before.
    original_source_bytes: u128,
    // Additional App response/context/query owners. Separate from the original
    // Core reservation so that moving Core into the response never counts its
    // heap twice. Retaining this reserve during later page requests is safe.
    external_reserve_bytes: u128,
}

pub struct PcReplaySourceBuildSession {
    source: PcReplayPageSource,
    pending: Vec<(u64, Vec<ExactReplayGraphLocation>)>,
    pending_location_bytes: u128,
    next_geometry: usize,
    next_pattern: usize,
    current_patterns: Vec<PatternManifest>,
    current_witness_count: usize,
    language: Option<ReplayLanguage>,
    inventory: Option<Box<P7P4InventoryIndex>>,
    first_rank: usize,
    hasher: Sha256,
    complete: bool,
}

fn bytes<T>(capacity: usize) -> ReplayResult<u128> {
    (capacity as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or_else(|| OVERFLOW.into())
}
fn ensure_peak(
    required: u128,
    maximum: u128,
    guard: &mut impl FnMut(u128) -> bool,
) -> ReplayResult<()> {
    if required > maximum {
        return Err(PcReplayPageError::MemoryLimit { required, maximum });
    }
    if !guard(required) {
        return Err(PcReplayPageError::HostMemoryLimit { required });
    }
    Ok(())
}
fn engine_guard(
    external: u128,
    engine: u128,
    maximum: u128,
    guard: &mut impl FnMut(u128) -> bool,
) -> Result<(), ExactReplayMaterializationError> {
    let required = external
        .checked_add(engine)
        .ok_or(ExactReplayMaterializationError::AllocationFailed)?;
    if required > maximum || !guard(required) {
        return Err(ExactReplayMaterializationError::MemoryLimitExceeded {
            required_memory_bytes: engine,
            max_memory_bytes: maximum.saturating_sub(external),
        });
    }
    Ok(())
}
fn into_core(
    candidate: u64,
    member: clearra_postprocess::CandidateExecution,
) -> ReplayResult<CorePostProcessExecution> {
    let (pattern, identity, trace) = member.into_parts();
    if !trace.canonical_key_matches(&identity) {
        return Err("complete_replay_trace_identity_mismatch".into());
    }
    Ok(CorePostProcessExecution::new(
        candidate, pattern, identity, trace,
    ))
}

impl PcReplaySourceBuildSession {
    pub fn new(
        problem: &SearchProblem,
        result: &CoreExecutionResult,
        maximum_bytes: u128,
    ) -> ReplayResult<Self> {
        Self::new_with_external_reserve(problem, result, maximum_bytes, 0)
    }

    pub(crate) fn new_with_external_reserve(
        problem: &SearchProblem,
        result: &CoreExecutionResult,
        maximum_bytes: u128,
        external_reserve_bytes: u128,
    ) -> ReplayResult<Self> {
        let _profile = clearra_core_executor::PcReplayProfileSpan::manifest_prepare();
        let original_core = result.checked_resource_retained_bytes().ok_or(OVERFLOW)?;
        let pattern_count = result
            .usize_field("materialized_pattern_count")
            .ok_or("pc replay materialized pattern count unavailable")?;
        if pattern_count == 0 {
            return Err("pc replay empty pattern universe".into());
        }
        let projection = PcPathProjectionContext::from_problem(problem);
        let PreparedReplayBatches {
            batches,
            retained_graph_bytes,
            graph_count,
            shared_input_graph_bytes,
        } = ReplayBatches::prepare_from_result(problem, result, projection, pattern_count)?;
        let original_source_bytes = original_core
            .checked_sub(shared_input_graph_bytes)
            .ok_or(OVERFLOW)?;
        let constructor_live = original_source_bytes
            .checked_add(external_reserve_bytes)
            .and_then(|n| n.checked_add(retained_graph_bytes))
            .and_then(|n| n.checked_add(core::mem::size_of::<Self>() as u128))
            .and_then(|n| n.checked_add(problem.problem_id().as_str().len() as u128))
            .ok_or(OVERFLOW)?;
        ensure_peak(
            constructor_live
                .checked_add(bytes::<(u64, ExactReplayGraphLocation)>(graph_count)?)
                .ok_or(OVERFLOW)?,
            maximum_bytes,
            &mut |_| true,
        )?;
        let mut flat = Vec::new();
        flat.try_reserve_exact(graph_count)
            .map_err(|_| "complete_replay_allocation_failed")?;
        batches.push_flat_index(&mut flat);
        flat.sort_unstable_by_key(|(id, loc)| (*id, loc.batch, loc.graph));
        let flat_live = constructor_live
            .checked_add(bytes::<(u64, ExactReplayGraphLocation)>(flat.capacity())?)
            .ok_or(OVERFLOW)?;
        ensure_peak(
            flat_live
                .checked_add(bytes::<(u64, Vec<ExactReplayGraphLocation>)>(graph_count)?)
                .ok_or(OVERFLOW)?,
            maximum_bytes,
            &mut |_| true,
        )?;
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(graph_count)
            .map_err(|_| "complete_replay_allocation_failed")?;
        let mut index = 0;
        let mut pending_location_bytes = 0u128;
        while index < flat.len() {
            let id = flat[index].0;
            let count = flat[index..]
                .iter()
                .take_while(|(other, _)| *other == id)
                .count();
            ensure_peak(
                flat_live
                    .checked_add(bytes::<(u64, Vec<ExactReplayGraphLocation>)>(
                        pending.capacity(),
                    )?)
                    .and_then(|n| n.checked_add(pending_location_bytes))
                    .and_then(|n| n.checked_add(bytes::<ExactReplayGraphLocation>(count).ok()?))
                    .ok_or(OVERFLOW)?,
                maximum_bytes,
                &mut |_| true,
            )?;
            let mut locations = Vec::new();
            locations
                .try_reserve_exact(count)
                .map_err(|_| "complete_replay_allocation_failed")?;
            locations.extend(
                flat[index..index + count]
                    .iter()
                    .map(|(_, location)| *location),
            );
            pending_location_bytes = pending_location_bytes
                .checked_add(bytes::<ExactReplayGraphLocation>(locations.capacity())?)
                .ok_or(OVERFLOW)?;
            pending.push((id, locations));
            let actual = Some(flat_live)
                .and_then(|n| {
                    n.checked_add(
                        bytes::<(u64, Vec<ExactReplayGraphLocation>)>(pending.capacity()).ok()?,
                    )
                })
                .and_then(|n| n.checked_add(pending_location_bytes))
                .ok_or(OVERFLOW)?;
            ensure_peak(actual, maximum_bytes, &mut |_| true)?;
            index += count;
        }
        drop(flat);
        let hasher = batches.source_hasher(problem)?;
        let inventory_eligible = !pending.is_empty()
            && batches
                .single_compact_batch()
                .is_some_and(|batch| P7P4InventoryIndex::eligible(problem, batch));
        let mut session = Self {
            source: PcReplayPageSource {
                problem_id: problem.problem_id().as_str().to_owned(),
                projection,
                batches,
                retained_graph_bytes,
                retained_manifest_nested_bytes: 0,
                retained_first_member_nested_bytes: 0,
                geometries: Vec::new(),
                materialized_pattern_count: pattern_count,
                witness_count: 0,
                identity_sha256: String::new(),
                first_members: Vec::new(),
                maximum_bytes,
                original_source_bytes,
                external_reserve_bytes,
            },
            pending,
            pending_location_bytes,
            next_geometry: 0,
            next_pattern: 0,
            current_patterns: Vec::new(),
            current_witness_count: 0,
            language: None,
            inventory: None,
            first_rank: 0,
            hasher,
            complete: false,
        };
        session.guard_additional(0)?;
        // Charge the optional table and its transient initialization separately
        // so all other request shapes retain the original allocation envelope.
        if inventory_eligible
            && session
                .guard_additional(2 * core::mem::size_of::<P7P4InventoryIndex>() as u128)
                .is_ok()
        {
            session.inventory = Some(Box::new(P7P4InventoryIndex::new()));
        }
        Ok(session)
    }

    /// Honor the caller's primitive budget, yielding at the host-turn quantum.
    /// No operation drains an entire cell's terminal-path language. Cancellation
    /// is checked per primitive, not only when the clock is sampled.
    pub fn advance(&mut self, work: usize, control: &ExecutionControl) -> ReplayResult<bool> {
        if self.complete {
            return Ok(true);
        }
        let _profile = clearra_core_executor::PcReplayProfileSpan::manifest_count();
        self.guard_additional(0)?;
        // Even a tiny language may finish in one caller slice. Publish its
        // stage before doing work so a progress-triggered cancellation cannot
        // be skipped by the fast completion path.
        self.report_progress(control);
        let quantum = CooperativeWorkQuantum::start(ADVANCE_QUANTUM_MILLIS);
        for step in 0..work.clamp(1, MAX_ADVANCE_WORK) {
            if control.is_cancelled() {
                return Err("complete_replay_cancelled".into());
            }
            if step != 0 && step % QUANTUM_CHECK_INTERVAL == 0 && quantum.is_exhausted() {
                break;
            }
            if self.next_geometry == self.pending.len() {
                self.inventory = None;
                self.hasher
                    .update((self.source.geometries.len() as u128).to_le_bytes());
                self.hasher.update(self.source.witness_count.to_le_bytes());
                self.hasher
                    .update((self.source.materialized_pattern_count as u128).to_le_bytes());
                self.guard_additional(64)?;
                self.source.identity_sha256 = format!("{:x}", self.hasher.clone().finalize());
                self.guard_additional(0)?;
                self.complete = true;
                return Ok(true);
            }
            if self.language.is_none() {
                if let Some(index) = &mut self.inventory {
                    if !index.ready() {
                        let valid = self
                            .source
                            .batches
                            .single_compact_batch()
                            .is_some_and(|batch| index.advance_layout(batch));
                        if !valid {
                            self.inventory = None;
                        }
                        continue;
                    }
                    if !index.has_geometry() {
                        self.prepare_inventory_geometry();
                        if quantum.is_exhausted() {
                            break;
                        }
                        continue;
                    }
                    let next = index.next_pattern(self.next_pattern);
                    if next != self.next_pattern {
                        self.next_pattern = next;
                        if next == self.source.materialized_pattern_count {
                            self.finish_geometry()?;
                            self.guard_additional(0)?;
                        }
                        continue;
                    }
                }
                let additional = self
                    .checked_retained_capacity_bytes()
                    .and_then(|n| n.checked_sub(self.source.checked_retained_capacity_bytes()?))
                    .ok_or(OVERFLOW)?;
                self.language = Some(self.source.new_language(
                    &self.pending[self.next_geometry].1,
                    self.next_pattern,
                    additional,
                    &mut |_| true,
                )?);
            }
            let mut language = self.language.take().expect("installed language");
            let external = self
                .checked_retained_capacity_bytes()
                .and_then(|n| n.checked_add(self.source.retained_external_bytes()?))
                .ok_or(OVERFLOW)?;
            let maximum = self.source.maximum_bytes;
            let mut language_guard = |peak| engine_guard(external, peak, maximum, &mut |_| true);
            let ready = language
                .advance(1, control, &mut language_guard)
                .map_err(|e| replay_engine_error(e, external))?;
            if !ready {
                self.language = Some(language);
                continue;
            }
            let count = language
                .count()
                .ok_or("pc replay incomplete language count")?;
            if self.source.geometries.is_empty()
                && self.source.first_members.len() < PC_REPLAY_MEMBER_PAGE_SIZE
                && self.first_rank < count
            {
                let execution = language.select(
                    self.pending[self.next_geometry].0,
                    self.first_rank,
                    control,
                    external,
                    &mut language_guard,
                )?;
                let engine_bytes = language.checked_retained_bytes().ok_or(OVERFLOW)?;
                let execution_bytes = execution.checked_owned_bytes().ok_or(OVERFLOW)?;
                let projection = execution.checked_projection_bytes().ok_or(OVERFLOW)?;
                let new_slots = bytes::<PcPathWitnessV2>(
                    self.source
                        .first_members
                        .len()
                        .checked_add(1)
                        .ok_or(OVERFLOW)?,
                )?;
                self.guard_additional(
                    engine_bytes
                        .checked_add(execution_bytes)
                        .and_then(|n| {
                            n.checked_add(projection.checked_mul(REPLAY_PUBLIC_PAGE_RESERVE)?)
                        })
                        .and_then(|n| n.checked_add(new_slots))
                        .ok_or(OVERFLOW)?,
                )?;
                self.source
                    .first_members
                    .try_reserve_exact(1)
                    .map_err(|_| "complete_replay_allocation_failed")?;
                self.guard_additional(
                    engine_bytes
                        .checked_add(execution_bytes)
                        .and_then(|n| {
                            n.checked_add(projection.checked_mul(REPLAY_PUBLIC_PAGE_RESERVE)?)
                        })
                        .ok_or(OVERFLOW)?,
                )?;
                let witness = execution.project(
                    self.source.projection,
                    self.source.materialized_pattern_count,
                    1,
                )?;
                self.source.retained_first_member_nested_bytes = self
                    .source
                    .retained_first_member_nested_bytes
                    .checked_add(witness.checked_retained_capacity_bytes().ok_or(OVERFLOW)?)
                    .ok_or(OVERFLOW)?;
                self.source.first_members.push(witness);
                self.first_rank += 1;
                self.guard_additional(engine_bytes.checked_add(execution_bytes).ok_or(OVERFLOW)?)?;
                self.language = Some(language);
                continue;
            }
            drop(language);
            if count != 0 {
                self.current_witness_count = self
                    .current_witness_count
                    .checked_add(count)
                    .ok_or("pc replay geometry witness count overflow")?;
                self.source.witness_count = self
                    .source
                    .witness_count
                    .checked_add(count as u128)
                    .ok_or("pc replay witness count overflow")?;
                self.guard_additional(bytes::<PatternManifest>(
                    self.current_patterns.len().checked_add(1).ok_or(OVERFLOW)?,
                )?)?;
                self.current_patterns
                    .try_reserve_exact(1)
                    .map_err(|_| "complete_replay_allocation_failed")?;
                self.current_patterns.push(PatternManifest {
                    pattern_id: self.next_pattern,
                    witness_count: count,
                    end_offset: self.current_witness_count,
                });
            }
            self.first_rank = 0;
            self.next_pattern += 1;
            if self.next_pattern == self.source.materialized_pattern_count {
                self.finish_geometry()?;
            }
            self.guard_additional(0)?;
        }
        self.report_progress(control);
        Ok(false)
    }

    fn prepare_inventory_geometry(&mut self) {
        let [location] = self.pending[self.next_geometry].1.as_slice() else {
            self.inventory
                .as_mut()
                .expect("inventory")
                .use_all_patterns();
            return;
        };
        let Some((batch, graph)) = self
            .source
            .batches
            .single_compact_batch()
            .filter(|_| location.batch == 0)
            .and_then(|batch| {
                batch
                    .graphs()
                    .get(location.graph)
                    .map(|graph| (batch, graph))
            })
        else {
            self.inventory
                .as_mut()
                .expect("inventory")
                .use_all_patterns();
            return;
        };
        if !P7P4InventoryIndex::scratch_projection(graph)
            .is_some_and(|additional| self.guard_additional(additional).is_ok())
        {
            self.inventory
                .as_mut()
                .expect("inventory")
                .use_all_patterns();
            return;
        }
        self.inventory
            .as_mut()
            .expect("inventory")
            .prepare_geometry(batch, graph);
    }

    fn report_progress(&self, control: &ExecutionControl) {
        control.report_progress(
            "complete-replay-pattern",
            self.next_geometry
                .saturating_mul(self.source.materialized_pattern_count)
                .saturating_add(self.next_pattern) as u64,
            Some(
                self.pending
                    .len()
                    .saturating_mul(self.source.materialized_pattern_count) as u64,
            ),
        );
    }

    fn finish_geometry(&mut self) -> ReplayResult<()> {
        if self.current_witness_count != 0 {
            self.guard_additional(bytes::<GeometryManifest>(
                self.source
                    .geometries
                    .len()
                    .checked_add(1)
                    .ok_or(OVERFLOW)?,
            )?)?;
            self.source
                .geometries
                .try_reserve_exact(1)
                .map_err(|_| "complete_replay_allocation_failed")?;
            let locations = core::mem::take(&mut self.pending[self.next_geometry].1);
            let location_bytes = bytes::<ExactReplayGraphLocation>(locations.capacity())?;
            self.pending_location_bytes = self
                .pending_location_bytes
                .checked_sub(location_bytes)
                .ok_or(OVERFLOW)?;
            self.source.retained_manifest_nested_bytes = self
                .source
                .retained_manifest_nested_bytes
                .checked_add(location_bytes)
                .and_then(|n| {
                    n.checked_add(bytes::<PatternManifest>(self.current_patterns.capacity()).ok()?)
                })
                .ok_or(OVERFLOW)?;
            self.hasher
                .update((self.source.geometries.len() as u128 + 1).to_le_bytes());
            self.hasher
                .update(self.pending[self.next_geometry].0.to_le_bytes());
            self.hasher.update((locations.len() as u128).to_le_bytes());
            for location in &locations {
                self.hasher.update((location.batch as u128).to_le_bytes());
                self.hasher.update((location.graph as u128).to_le_bytes());
            }
            self.hasher
                .update((self.current_patterns.len() as u128).to_le_bytes());
            for pattern in &self.current_patterns {
                self.hasher
                    .update((pattern.pattern_id as u128).to_le_bytes());
                self.hasher
                    .update((pattern.witness_count as u128).to_le_bytes());
            }
            self.source.geometries.push(GeometryManifest {
                producer_candidate_id: self.pending[self.next_geometry].0,
                locations,
                witness_count: self.current_witness_count,
                patterns: core::mem::take(&mut self.current_patterns),
            });
        }
        self.current_witness_count = 0;
        self.next_pattern = 0;
        self.next_geometry += 1;
        if let Some(index) = &mut self.inventory {
            index.clear_geometry();
        }
        Ok(())
    }

    pub fn complete(self) -> ReplayResult<Arc<PcReplayPageSource>> {
        if !self.complete {
            return Err("pc replay manifest is incomplete".into());
        }
        let first = bytes::<PcPathWitnessV2>(self.source.first_members.capacity())?
            .checked_add(self.source.retained_first_member_nested_bytes)
            .ok_or(OVERFLOW)?;
        self.guard_additional(
            first
                .checked_mul(REPLAY_PUBLIC_PAGE_RESERVE - 1)
                .and_then(|n| n.checked_add((2 * core::mem::size_of::<usize>()) as u128))
                .ok_or(OVERFLOW)?,
        )?;
        Ok(Arc::new(self.source))
    }
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (core::mem::size_of::<Self>() as u128)
            .checked_add(self.source.checked_retained_capacity_bytes()?)?
            .checked_add(bytes::<PatternManifest>(self.current_patterns.capacity()).ok()?)?
            .checked_add(
                bytes::<(u64, Vec<ExactReplayGraphLocation>)>(self.pending.capacity()).ok()?,
            )?
            .checked_add(self.pending_location_bytes)?
            .checked_add(if self.inventory.is_some() {
                core::mem::size_of::<P7P4InventoryIndex>() as u128
            } else {
                0
            })?
            .checked_add(match &self.language {
                Some(language) => language.checked_retained_bytes()?,
                None => 0,
            })
    }
    fn guard_additional(&self, additional: u128) -> ReplayResult<()> {
        ensure_peak(
            self.checked_retained_capacity_bytes()
                .and_then(|n| n.checked_add(self.source.retained_external_bytes()?))
                .and_then(|n| n.checked_add(additional))
                .ok_or(OVERFLOW)?,
            self.source.maximum_bytes,
            &mut |_| true,
        )
    }
}

impl PcReplayPageSource {
    fn retained_external_bytes(&self) -> Option<u128> {
        self.original_source_bytes
            .checked_add(self.external_reserve_bytes)
    }
    pub fn problem_id(&self) -> &str {
        &self.problem_id
    }
    pub fn identity_sha256(&self) -> &str {
        &self.identity_sha256
    }
    pub fn geometry_count(&self) -> usize {
        self.geometries.len()
    }
    pub fn witness_count(&self) -> u128 {
        self.witness_count
    }
    pub fn materialized_pattern_count(&self) -> usize {
        self.materialized_pattern_count
    }
    pub fn maximum_memory_bytes(&self) -> u128 {
        self.maximum_bytes
    }
    pub(crate) fn first_members(&self) -> &[PcPathWitnessV2] {
        &self.first_members
    }
    pub(crate) fn matches_result(&self, result: &CoreExecutionResult) -> bool {
        self.batches.matches_result(result)
    }

    pub fn page_metadata(
        &self,
        geometry: usize,
        member: usize,
    ) -> ReplayResult<PcReplayPageMetadata> {
        let entry = self
            .geometries
            .get(
                geometry
                    .checked_sub(1)
                    .ok_or("pc replay invalid geometry page")?,
            )
            .ok_or("pc replay invalid geometry page")?;
        let pages = entry.witness_count.div_ceil(PC_REPLAY_MEMBER_PAGE_SIZE);
        if member == 0 || member > pages {
            return Err("pc replay invalid member page".into());
        }
        Ok(PcReplayPageMetadata {
            page_contract: PC_REPLAY_MEMBER_PAGE_CONTRACT.to_owned(),
            page_source_available: true,
            page_source_identity_sha256: self.identity_sha256.clone(),
            geometry_count: self.geometries.len().to_string(),
            geometry_page_number: geometry.to_string(),
            candidate_id: geometry.to_string(),
            geometry_witness_count: entry.witness_count.to_string(),
            geometry_pattern_count: entry.patterns.len().to_string(),
            member_page_number: member.to_string(),
            member_page_count: pages.to_string(),
        })
    }
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (core::mem::size_of::<Self>() as u128)
            .checked_add(self.problem_id.capacity() as u128)?
            .checked_add(self.identity_sha256.capacity() as u128)?
            .checked_add(self.retained_graph_bytes)?
            .checked_add(bytes::<GeometryManifest>(self.geometries.capacity()).ok()?)?
            .checked_add(bytes::<PcPathWitnessV2>(self.first_members.capacity()).ok()?)?
            .checked_add(self.retained_manifest_nested_bytes)?
            .checked_add(self.retained_first_member_nested_bytes)
    }
    #[cfg(test)]
    pub(crate) fn checked_full_capacity_recount(&self) -> Option<u128> {
        let graph = self.batches.checked_retained_bytes()?;
        let manifest = self.geometries.iter().try_fold(0u128, |n, g| {
            n.checked_add(bytes::<ExactReplayGraphLocation>(g.locations.capacity()).ok()?)?
                .checked_add(bytes::<PatternManifest>(g.patterns.capacity()).ok()?)
        })?;
        let first = self.first_members.iter().try_fold(0u128, |n, w| {
            n.checked_add(w.checked_retained_capacity_bytes()?)
        })?;
        self.checked_retained_capacity_bytes()?
            .checked_sub(self.retained_graph_bytes)?
            .checked_add(graph)?
            .checked_sub(self.retained_manifest_nested_bytes)?
            .checked_add(manifest)?
            .checked_sub(self.retained_first_member_nested_bytes)?
            .checked_add(first)
    }
    fn new_language(
        &self,
        locations: &[ExactReplayGraphLocation],
        pattern: usize,
        additional: u128,
        guard: &mut impl FnMut(u128) -> bool,
    ) -> ReplayResult<ReplayLanguage> {
        let external = self
            .checked_retained_capacity_bytes()
            .and_then(|n| n.checked_add(self.retained_external_bytes()?))
            .and_then(|n| n.checked_add(additional))
            .ok_or(OVERFLOW)?;
        ensure_peak(
            external
                .checked_add(bytes::<ExactReplayGraphLocation>(locations.len())?)
                .ok_or(OVERFLOW)?,
            self.maximum_bytes,
            guard,
        )?;
        let locations = locations.to_vec();
        ensure_peak(
            external
                .checked_add(bytes::<ExactReplayGraphLocation>(locations.capacity())?)
                .ok_or(OVERFLOW)?,
            self.maximum_bytes,
            guard,
        )?;
        let allowance =
            self.maximum_bytes
                .checked_sub(external)
                .ok_or(PcReplayPageError::MemoryLimit {
                    required: external,
                    maximum: self.maximum_bytes,
                })?;
        self.batches
            .new_language(
                locations,
                pattern,
                ExactReplayMaterializationLimits::new(MAX_GEOMETRY_EXECUTIONS, 256, allowance),
                &mut |peak| engine_guard(external, peak, self.maximum_bytes, guard),
            )
            .map_err(|e| replay_engine_error(e, external))
    }
}

#[cfg(test)]
mod memory_tests {
    use super::*;

    fn tiny_replay_source() -> (SearchProblem, CoreExecutionResult) {
        use clearra_core_domain::piece::piece_kind::PieceKind;
        use clearra_objectives::policy::objective_policy::ObjectivePolicy;
        use clearra_pc_graph::request::{
            PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
            PieceWindow,
        };
        use clearra_supply::queue::fixed_sequence::FixedSequence;
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0xfc3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O; 2])),
            PieceWindow::new(2),
        )
        .with_exact_pieces(Some(2))
        .with_allow_hold(false)
        // These tests own replay memory, not a worker pool. The serial-only
        // Core feature must not inherit the host's Auto worker count and then
        // request unavailable parallel execution on larger CI machines.
        .with_execution_policy(PcExecutionPolicy::mvp_default().with_workers(1))
        .with_count_policy(PcCountPolicy::CountAll)
        .with_objective(ObjectivePolicy::all());
        let problem = clearra_problem::ProblemCompiler::compile_scenario_pc(&query).unwrap();
        let execution_problem = clearra_problem::ProblemCompiler::compile_scenario_pc(&query)
            .unwrap()
            .with_pc_path_v2_evidence();
        let result = clearra_core_executor::WasmCpuSearchBackend::execute_with_control(
            &execution_problem,
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!(!result.exact_scoring_execution_batches().is_empty());
        (problem, result)
    }

    #[test]
    fn replay_external_response_projection_counts_core_heap_only_once() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let (_, result) = tiny_replay_source();
        let core_heap = result
            .checked_resource_retained_bytes()
            .unwrap()
            .checked_sub(core::mem::size_of::<CoreExecutionResult>() as u128)
            .unwrap();
        let response = crate::AppResponse::success(crate::render::AppRenderModel::Scenario(result));
        assert_eq!(
            response.checked_pc_replay_external_retained_capacity_bytes(),
            None,
            "an unbound response must not gain a replay owner projection"
        );
        let response = response.with_contract_context(clearra_host_contract::AppCommandKind::Pc);
        let full = response
            .checked_pc_minimals_retained_capacity_bytes()
            .unwrap();
        let extra = response
            .checked_pc_replay_external_retained_capacity_bytes()
            .unwrap();
        assert_eq!(extra.checked_add(core_heap), Some(full));
        assert!(
            extra > 0,
            "response summary/report ownership is not the rendered Core heap"
        );
    }

    #[test]
    fn replay_external_app_reserve_precedes_constructor_allocation() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let (problem, result) = tiny_replay_source();
        let required = |extra| match PcReplaySourceBuildSession::new_with_external_reserve(
            &problem, &result, 0, extra,
        ) {
            Err(PcReplayPageError::MemoryLimit {
                required,
                maximum: 0,
            }) => required,
            _ => panic!("constructor must reject at its first admitted allocation"),
        };
        assert_eq!(required(7919), required(0).checked_add(7919).unwrap());
        assert!(matches!(
            PcReplaySourceBuildSession::new_with_external_reserve(&problem, &result, u128::MAX, u128::MAX),
            Err(error) if error.code() == OVERFLOW
        ));
    }

    #[test]
    fn replay_external_app_reserve_survives_advance_and_page_owner_handoff() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let (problem, result) = tiny_replay_source();
        let extra = 7919;
        let mut session = PcReplaySourceBuildSession::new_with_external_reserve(
            &problem,
            &result,
            u128::MAX,
            extra,
        )
        .unwrap();
        let original = result.checked_resource_retained_bytes().unwrap();
        let graph = result
            .checked_exact_scoring_execution_batches_retained_bytes()
            .unwrap();
        let original_remainder = original.checked_sub(graph).unwrap();
        assert_eq!(session.source.original_source_bytes, original_remainder);
        assert_eq!(session.source.retained_graph_bytes, graph);
        assert_eq!(
            session.source.retained_external_bytes(),
            original_remainder.checked_add(extra)
        );
        let required = session
            .checked_retained_capacity_bytes()
            .unwrap()
            .checked_add(original_remainder)
            .unwrap()
            .checked_add(extra)
            .unwrap();
        session.source.maximum_bytes = required;
        assert_eq!(session.guard_additional(0), Ok(()));
        session.source.maximum_bytes = required - 1;
        assert_eq!(
            session.advance(8192, &ExecutionControl::default()),
            Err(PcReplayPageError::MemoryLimit {
                required,
                maximum: required - 1
            })
        );
        session.source.external_reserve_bytes = u128::MAX;
        assert!(matches!(session.guard_additional(0), Err(error) if error.code() == OVERFLOW));
        session.source.external_reserve_bytes = extra;
        let store = PcReplayPageStore::new(Arc::new(session.source));
        assert_eq!(
            store.checked_host_entry_bytes(),
            store
                .checked_retained_capacity_bytes()
                .unwrap()
                .checked_add(original_remainder)
                .unwrap()
                .checked_add(extra)
        );
    }

    #[test]
    fn compact_replay_reuses_exact_graph_and_queue_owner() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let (problem, result) = tiny_replay_source();
        let original = result.exact_scoring_execution_batch_owner().unwrap();
        let graph_pointer = original[0].graphs().as_ptr();
        let pattern_pointer = original[0].patterns().as_ptr();
        let mut session = PcReplaySourceBuildSession::new(&problem, &result, u128::MAX).unwrap();
        let ReplayBatches::Compact(Some(shared)) = &session.source.batches else {
            panic!("expected the Core-owned compact replay evidence");
        };
        assert!(Arc::ptr_eq(shared, original));
        assert!(session.source.matches_result(&result.clone()));
        let independent = result.clone().with_exact_scoring_execution_batches(
            result.exact_scoring_execution_batches().to_vec(),
        );
        assert_eq!(result, independent);
        assert!(!session.source.matches_result(&independent));
        // The paging cursor owns everything it needs after Core retirement.
        drop(result);
        let control = ExecutionControl::default();
        let done = (0..1024).any(|_| session.advance(128, &control).unwrap());
        assert!(done, "bounded compact replay fixture must complete");
        let source = session.complete().unwrap();
        assert!(source.witness_count() > 0);
        assert!(!source.first_members().is_empty());
        let mut independent_session =
            PcReplaySourceBuildSession::new(&problem, &independent, u128::MAX).unwrap();
        assert!((0..1024).any(|_| independent_session.advance(128, &control).unwrap()));
        let independent_source = independent_session.complete().unwrap();
        assert_eq!(
            source.identity_sha256(),
            independent_source.identity_sha256()
        );
        assert_eq!(source.witness_count(), independent_source.witness_count());
        assert_eq!(source.first_members(), independent_source.first_members());
        assert_eq!(
            source.checked_retained_capacity_bytes(),
            source.checked_full_capacity_recount()
        );
        let ReplayBatches::Compact(Some(shared)) = &source.batches else {
            panic!("completed pager lost the Core allocation");
        };
        assert_eq!(shared[0].graphs().as_ptr(), graph_pointer);
        assert_eq!(shared[0].patterns().as_ptr(), pattern_pointer);
    }

    #[test]
    fn full_height_replay_shares_exact_graph_owner_across_staged_promotion() {
        use clearra_core_domain::{
            board::standard_pc_board::Board256Mask, piece::piece_kind::PieceKind,
        };
        use clearra_core_executor::WasmCpuSearchBackend;
        use clearra_objectives::policy::objective_policy::ObjectivePolicy;
        use clearra_pc_graph::request::{
            PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
            PieceWindow,
        };
        use clearra_supply::queue::fixed_sequence::FixedSequence;

        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let control = ExecutionControl::default();
        let mut initial = Board256Mask::all_cells(70).unwrap();
        for (column, start) in [(0_u16, 0_u16), (1, 3)] {
            for row in start..start + 4 {
                initial = initial.without(Board256Mask::singleton(row * 10 + column).unwrap());
            }
        }
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10_from_words(7, initial.words()).unwrap(),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; 2])),
            PieceWindow::new(2),
        )
        .with_rule(clearra_rules::profile::builtin_rules::srs_plus())
        .with_exact_pieces(Some(2))
        .with_min_remaining_queue(0)
        .with_retained_trace_limit(1)
        .with_allow_hold(false)
        .with_execution_policy(PcExecutionPolicy::mvp_default().with_workers(1))
        .with_count_policy(PcCountPolicy::CountAll)
        .with_objective(ObjectivePolicy::all());
        let problem = clearra_problem::ProblemCompiler::compile_scenario_pc(&query)
            .unwrap()
            .with_pc_path_v2_evidence();
        let raw = WasmCpuSearchBackend::execute_with_control(&problem, &control).unwrap();
        let evidence = raw.pc_full_height_replay_evidence().unwrap();
        let raw_graph_bytes = evidence.checked_shared_batch_retained_bytes().unwrap();
        let raw_retained = raw.checked_resource_retained_bytes().unwrap();
        let graph_pointer = evidence.batch().execution().graphs().as_ptr();
        let pattern_pointer = evidence.batch().execution().patterns().as_ptr();
        let extra = 7919;
        let mut session =
            PcReplaySourceBuildSession::new_with_external_reserve(&problem, &raw, u128::MAX, extra)
                .unwrap();
        let ReplayBatches::FullHeight(shared) = &session.source.batches else {
            panic!("expected full-height replay owner");
        };
        assert!(Arc::ptr_eq(shared, evidence.shared_batch_owner()));
        assert_eq!(shared[0].execution().graphs().as_ptr(), graph_pointer);
        assert_eq!(shared[0].execution().patterns().as_ptr(), pattern_pointer);
        assert_eq!(session.source.retained_graph_bytes, raw_graph_bytes);
        assert_eq!(
            session
                .source
                .original_source_bytes
                .checked_add(raw_graph_bytes),
            Some(raw_retained),
        );
        assert_eq!(
            session.source.retained_external_bytes().unwrap() + raw_graph_bytes,
            raw_retained + extra,
        );
        assert!(session.source.matches_result(&raw.clone()));
        assert!(!session.source.matches_result(
            &raw.clone()
                .with_normalized_solution_keys(vec!["unbound".to_owned()]),
        ));
        let independent = WasmCpuSearchBackend::execute_with_control(&problem, &control).unwrap();
        assert_eq!(
            raw.pc_full_height_replay_evidence(),
            independent.pc_full_height_replay_evidence()
        );
        assert!(
            !session.source.matches_result(&independent),
            "equal content is not shared memory"
        );
        drop(independent);

        let exact = session.checked_retained_capacity_bytes().unwrap()
            + session.source.retained_external_bytes().unwrap();
        session.source.maximum_bytes = exact;
        assert_eq!(session.guard_additional(0), Ok(()));
        session.source.maximum_bytes = exact - 1;
        assert_eq!(
            session.advance(1, &control),
            Err(PcReplayPageError::MemoryLimit {
                required: exact,
                maximum: exact - 1,
            }),
        );
        session.source.maximum_bytes = u128::MAX;
        let mut done = false;
        for _ in 0..1024 {
            if session.advance(128, &control).unwrap() {
                done = true;
                break;
            }
        }
        assert!(done, "small replay fixture must complete");
        let source = session.complete().unwrap();
        assert!(source.matches_result(&raw));
        assert_eq!(
            source.checked_retained_capacity_bytes(),
            source.checked_full_capacity_recount()
        );
        drop(raw);
        let ReplayBatches::FullHeight(shared) = &source.batches else {
            panic!("pager lost its graph owner");
        };
        assert_eq!(Arc::strong_count(shared), 1);
        assert_eq!(shared[0].execution().graphs().as_ptr(), graph_pointer);
        let mut store = PcReplayPageStore::new(source);
        let page = store.page(1, 1, &control).unwrap();
        assert!(!page.witnesses.is_empty());
        assert!(page.witnesses[0]
            .normalized_trace_key()
            .starts_with("trk2:"));
    }

    #[test]
    fn replay_manifest_honors_caller_work_instead_of_one_fixed_primitive_batch() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let (problem, result) = tiny_replay_source();
        let mut single = PcReplaySourceBuildSession::new(&problem, &result, u128::MAX).unwrap();
        let mut batched = PcReplaySourceBuildSession::new(&problem, &result, u128::MAX).unwrap();
        assert!(!single.advance(1, &ExecutionControl::default()).unwrap());
        let narrow_work = single.language.as_ref().unwrap().work_units();
        let done = batched.advance(8192, &ExecutionControl::default()).unwrap();
        assert!(
            done || batched.next_geometry > single.next_geometry
                || batched.next_pattern > single.next_pattern
                || batched
                    .language
                    .as_ref()
                    .is_some_and(|language| language.work_units() > narrow_work),
            "a larger caller budget must execute more than the one-primitive cursor"
        );
        let cancellation =
            clearra_core_domain::execution_cancellation::ExecutionCancellationToken::new();
        cancellation.handle().cancel();
        assert!(
            matches!(single.advance(8192, &ExecutionControl::new(cancellation)),
            Err(error) if error.code() == "complete_replay_cancelled")
        );
    }

    #[test]
    fn replay_whole_live_admission_accepts_exact_cap_and_rejects_one_byte_less() {
        assert_eq!(ensure_peak(1100, 1100, &mut |_| true), Ok(()));
        assert_eq!(
            ensure_peak(1100, 1099, &mut |_| true),
            Err(PcReplayPageError::MemoryLimit {
                required: 1100,
                maximum: 1099
            })
        );
        assert_eq!(
            ensure_peak(1100, 1100, &mut |_| false),
            Err(PcReplayPageError::HostMemoryLimit { required: 1100 })
        );
    }
}
