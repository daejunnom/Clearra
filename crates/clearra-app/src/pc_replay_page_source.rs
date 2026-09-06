//! Query-bound replay geometry paging. The complete graph remains immutable;
//! only one geometry-pattern cell's replay expansion is live at a time.

use std::{collections::BTreeMap, sync::Arc};

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_executor::{CoreExecutionResult, CorePostProcessExecution};
use clearra_host_contract::{PcReplayPageMetadata, PcReplayPagePayload};
use clearra_postprocess::{ExactReplayMaterializationLimits, ExactScoringExecutionMaterializer};
use clearra_problem::SearchProblem;
use clearra_replay::ExactScoringExecutionBatch;
use sha2::{Digest, Sha256};

use crate::pc_path_result::{
    checked_execution_projection_peak_bytes, pc_path_witness_payload,
    project_canonical_execution_with_context as project_execution_with_context,
    validate_execution_with_context, PcPathProjectionContext, PcPathWitnessV2,
};

pub const PC_REPLAY_MEMBER_PAGE_CONTRACT: &str = "pc-replay-member-page.v1";
pub const PC_REPLAY_MEMBER_PAGE_SIZE: usize = 100;
// Only an actual <=100-member public page crosses the App/Host/JSON carrier
// projections. Raw geometry-pattern cells never retain those whole-cell views.
const REPLAY_PUBLIC_PAGE_RESERVE: u128 = 16;
const MAX_GEOMETRY_EXECUTIONS: usize = 1_000_000;
// Named stack/move carriers are separate from the nested allocations reported
// by the materializer. The two IntoIter buffers themselves remain included in
// the report's retained capacities until their members have been moved.
const REPLAY_CELL_CARRIER_BYTES: u128 = (core::mem::size_of::<
    clearra_postprocess::ExactScoringExecutionMaterialization,
>() + core::mem::size_of::<
    clearra_postprocess::ExactReplayMaterializationReport,
>() + core::mem::size_of::<
    clearra_postprocess::CandidateExecutionAggregate,
>() + core::mem::size_of::<
    clearra_postprocess::CandidateExecution,
>() + core::mem::size_of::<CorePostProcessExecution>()
    + core::mem::size_of::<Vec<CorePostProcessExecution>>()
    + core::mem::size_of::<std::vec::IntoIter<clearra_postprocess::CandidateExecutionAggregate>>()
    + core::mem::size_of::<std::vec::IntoIter<clearra_postprocess::CandidateExecution>>())
    as u128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GraphLocation {
    batch: usize,
    graph: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GeometryManifest {
    producer_candidate_id: u64,
    locations: Vec<GraphLocation>,
    witness_count: usize,
    pattern_count: usize,
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
    batches: Vec<ExactScoringExecutionBatch>,
    // These private DAG owners never mutate. Snapshot their actual allocated
    // capacities once; recounting every graph at each pattern is quadratic.
    retained_graph_bytes: u128,
    retained_manifest_nested_bytes: u128,
    retained_first_member_nested_bytes: u128,
    geometries: Vec<GeometryManifest>,
    materialized_pattern_count: usize,
    witness_count: u128,
    identity_sha256: String,
    first_members: Vec<PcPathWitnessV2>,
    maximum_bytes: u128,
    // The original Core result remains live while the initial manifest is
    // constructed. Retaining this reserve later is conservative, not a claim
    // that a wire cap owns result-construction memory.
    original_source_bytes: u128,
}

pub struct PcReplaySourceBuildSession {
    source: PcReplayPageSource,
    pending: Vec<(u64, Vec<GraphLocation>)>,
    pending_location_bytes: u128,
    next_geometry: usize,
    next_pattern: usize,
    current_patterns: Vec<PatternManifest>,
    current_witness_count: usize,
    hasher: Sha256,
    complete: bool,
}

impl PcReplaySourceBuildSession {
    pub fn new(
        problem: &SearchProblem,
        result: &CoreExecutionResult,
        maximum_bytes: u128,
    ) -> Result<Self, &'static str> {
        let original_source_bytes = result
            .checked_resource_retained_bytes()
            .ok_or("complete_replay_memory_projection_overflow")?;
        #[cfg(test)]
        eprintln!("pc-replay source entry max={maximum_bytes} original={original_source_bytes} batches={} graphs={}",
            result.exact_scoring_execution_batches().len(),
            result.exact_scoring_execution_batches().iter().map(|batch| batch.graphs().len()).sum::<usize>());
        if original_source_bytes
            .checked_mul(2)
            .ok_or("complete_replay_memory_projection_overflow")?
            > maximum_bytes
        {
            return Err("complete_replay_whole_live_limit_exceeded");
        }
        let materialized_pattern_count = result
            .usize_field("materialized_pattern_count")
            .ok_or("pc replay materialized pattern count unavailable")?;
        if materialized_pattern_count == 0 {
            return Err("pc replay empty pattern universe");
        }
        let projection = PcPathProjectionContext::from_problem(problem);
        let batches = result.exact_scoring_execution_batches();
        let mut by_candidate = BTreeMap::<u64, Vec<GraphLocation>>::new();
        let mut hasher = Sha256::new();
        hasher.update(b"clearra.pc-replay-source.v1\0");
        hasher.update(problem.problem_id().as_str().as_bytes());
        for (batch_index, batch) in batches.iter().enumerate() {
            if !batch.complete()
                || batch.initial_occupied() != projection.initial_board
                || usize::from(batch.initial_cursor()) != projection.initial_cursor
                || batch.initial_hold() != projection.initial_hold
                || batch.patterns().len() != materialized_pattern_count
            {
                return Err("pc replay graph source does not match the query");
            }
            hasher.update(batch.kick_table_id().to_le_bytes());
            hasher.update(batch.rule_profile_id().to_le_bytes());
            for pattern in batch.patterns() {
                hasher.update((pattern.len() as u64).to_le_bytes());
                for piece in pattern {
                    hasher.update([piece.as_ascii() as u8]);
                }
            }
            for (graph_index, graph) in batch.graphs().iter().enumerate() {
                by_candidate
                    .entry(graph.candidate_id())
                    .or_default()
                    .push(GraphLocation {
                        batch: batch_index,
                        graph: graph_index,
                    });
            }
        }
        if batches.is_empty()
            && !(result.bool_field("count_complete") == Some(true)
                && result.usize_field("solution_count") == Some(0)
                && result.bool_field("solution_found") == Some(false))
        {
            return Err("pc replay execution graph is missing or its empty result is unproven");
        }
        let batches = batches.to_vec();
        let retained_graph_bytes = batches
            .iter()
            .try_fold(
                (batches.capacity() as u128)
                    .checked_mul(core::mem::size_of::<ExactScoringExecutionBatch>() as u128)
                    .ok_or("complete_replay_memory_projection_overflow")?,
                |bytes, batch| bytes.checked_add(batch.checked_nested_retained_bytes()?),
            )
            .ok_or("complete_replay_memory_projection_overflow")?;
        let source = PcReplayPageSource {
            problem_id: problem.problem_id().as_str().to_owned(),
            projection,
            batches,
            retained_graph_bytes,
            retained_manifest_nested_bytes: 0,
            retained_first_member_nested_bytes: 0,
            geometries: Vec::new(),
            materialized_pattern_count,
            witness_count: 0,
            identity_sha256: String::new(),
            first_members: Vec::new(),
            maximum_bytes,
            original_source_bytes,
        };
        let pending: Vec<_> = by_candidate.into_iter().collect();
        let pending_location_bytes = pending
            .iter()
            .try_fold(0_u128, |bytes, (_, locations)| {
                bytes.checked_add(
                    (locations.capacity() as u128)
                        .checked_mul(core::mem::size_of::<GraphLocation>() as u128)?,
                )
            })
            .ok_or("complete_replay_memory_projection_overflow")?;
        let session = Self {
            source,
            pending,
            pending_location_bytes,
            next_geometry: 0,
            next_pattern: 0,
            current_patterns: Vec::new(),
            current_witness_count: 0,
            hasher,
            complete: false,
        };
        session.guard_live()?;
        Ok(session)
    }

    /// Each work unit visits at most 64 geometry-pattern cells. A geometry with
    /// thousands of patterns therefore yields before its exact manifest is
    /// complete, and only one cell's exhaustive replay expansion is retained.
    pub fn advance(
        &mut self,
        work_units: usize,
        control: &ExecutionControl,
    ) -> Result<bool, &'static str> {
        if self.complete {
            return Ok(true);
        }
        if control.is_cancelled() {
            return Err("complete_replay_cancelled");
        }
        self.guard_live()?;
        let mut remaining_cells = work_units.max(1).saturating_mul(64);
        #[cfg(test)]
        if self.next_geometry == 0 && self.next_pattern == 0 {
            eprintln!(
                "pc-replay first advance source={} session={} patterns={} geometries={}",
                self.source
                    .checked_retained_capacity_bytes()
                    .unwrap_or(u128::MAX),
                self.checked_retained_capacity_bytes().unwrap_or(u128::MAX),
                self.source.materialized_pattern_count,
                self.pending.len()
            );
        }
        while self.next_geometry < self.pending.len() && remaining_cells != 0 {
            if control.is_cancelled() {
                return Err("complete_replay_cancelled");
            }
            let producer_id = self.pending[self.next_geometry].0;
            let locations = &self.pending[self.next_geometry].1;
            let additional_live = self
                .checked_retained_capacity_bytes()
                .and_then(|bytes| bytes.checked_sub(self.source.checked_retained_capacity_bytes()?))
                .ok_or("complete_replay_memory_projection_overflow")?;
            let executions = self.source.materialize_geometry_pattern(
                producer_id,
                locations,
                self.next_pattern,
                additional_live,
                control,
            )?;
            if !executions.is_empty() {
                let cell_bytes = checked_execution_bytes(&executions)
                    .ok_or("complete_replay_memory_projection_overflow")?;
                let candidate_id = u64::try_from(self.source.geometries.len())
                    .ok()
                    .and_then(|n| n.checked_add(1))
                    .ok_or("pc replay candidate identity overflow")?;
                for execution in &executions {
                    if execution.pattern_id() != self.next_pattern {
                        return Err("complete_replay_pattern_identity_mismatch");
                    }
                    validate_execution_with_context(
                        self.source.projection,
                        execution,
                        self.source.materialized_pattern_count,
                    )?;
                    self.hasher.update(candidate_id.to_le_bytes());
                    self.hasher
                        .update((execution.pattern_id() as u64).to_le_bytes());
                    self.hasher
                        .update((execution.trace_identity().len() as u64).to_le_bytes());
                    self.hasher.update(execution.trace_identity().as_bytes());
                    if self.source.geometries.is_empty()
                        && self.source.first_members.len() < PC_REPLAY_MEMBER_PAGE_SIZE
                    {
                        let projection_peak = checked_execution_projection_peak_bytes(execution)
                            .ok_or("complete_replay_memory_projection_overflow")?;
                        let growth_peak = if self.source.first_members.len()
                            == self.source.first_members.capacity()
                        {
                            (self.source.first_members.len() as u128).checked_add(1)
                                .and_then(|count| count.checked_mul(core::mem::size_of::<PcPathWitnessV2>() as u128))
                                .ok_or("complete_replay_memory_projection_overflow")?
                        } else {
                            0
                        };
                        self.guard_additional(
                            cell_bytes
                                .checked_add(projection_peak)
                                .and_then(|n| n.checked_add(growth_peak))
                                .ok_or("complete_replay_memory_projection_overflow")?,
                        )?;
                        self.source
                            .first_members
                            .try_reserve_exact(1)
                            .map_err(|_| "complete_replay_allocation_failed")?;
                        self.guard_additional(
                            cell_bytes
                                .checked_add(projection_peak)
                                .ok_or("complete_replay_memory_projection_overflow")?,
                        )?;
                        let witness = project_execution_with_context(
                            self.source.projection,
                            execution,
                            self.source.materialized_pattern_count,
                            candidate_id,
                        )?;
                        self.guard_additional(
                            cell_bytes
                                .checked_add(core::mem::size_of::<PcPathWitnessV2>() as u128)
                                .and_then(|n| {
                                    n.checked_add(witness.checked_retained_capacity_bytes()?)
                                })
                                .ok_or("complete_replay_memory_projection_overflow")?,
                        )?;
                        self.source.retained_first_member_nested_bytes = self
                            .source
                            .retained_first_member_nested_bytes
                            .checked_add(
                                witness
                                    .checked_retained_capacity_bytes()
                                    .ok_or("complete_replay_memory_projection_overflow")?,
                            )
                            .ok_or("complete_replay_memory_projection_overflow")?;
                        self.source.first_members.push(witness);
                    }
                }
                self.source.witness_count = self
                    .source
                    .witness_count
                    .checked_add(executions.len() as u128)
                    .ok_or("pc replay witness count overflow")?;
                self.current_witness_count = self
                    .current_witness_count
                    .checked_add(executions.len())
                    .ok_or("pc replay geometry witness count overflow")?;
                if self.current_patterns.len() == self.current_patterns.capacity() {
                    let additional = 64
                        .min(self.source.materialized_pattern_count - self.current_patterns.len());
                    let new_inline = (self.current_patterns.len() as u128)
                        .checked_add(additional as u128)
                        .and_then(|n| {
                            n.checked_mul(core::mem::size_of::<PatternManifest>() as u128)
                        })
                        .ok_or("complete_replay_memory_projection_overflow")?;
                    self.guard_additional(
                        cell_bytes
                            .checked_add(new_inline)
                            .ok_or("complete_replay_memory_projection_overflow")?,
                    )?;
                    self.current_patterns
                        .try_reserve_exact(additional)
                        .map_err(|_| "complete_replay_allocation_failed")?;
                    self.guard_additional(cell_bytes)?;
                }
                self.current_patterns.push(PatternManifest {
                    pattern_id: self.next_pattern,
                    witness_count: executions.len(),
                    end_offset: self.current_witness_count,
                });
            }
            self.next_pattern += 1;
            remaining_cells -= 1;
            if self.next_pattern == self.source.materialized_pattern_count {
                if self.current_witness_count != 0 {
                    if self.source.geometries.len() == self.source.geometries.capacity() {
                        let additional = 64.min(self.pending.len() - self.source.geometries.len());
                        let new_inline = (self.source.geometries.len() as u128)
                            .checked_add(additional as u128)
                            .and_then(|n| {
                                n.checked_mul(core::mem::size_of::<GeometryManifest>() as u128)
                            })
                            .ok_or("complete_replay_memory_projection_overflow")?;
                        let cell_bytes = checked_execution_bytes(&executions)
                            .ok_or("complete_replay_memory_projection_overflow")?;
                        self.guard_additional(
                            cell_bytes
                                .checked_add(new_inline)
                                .ok_or("complete_replay_memory_projection_overflow")?,
                        )?;
                        self.source
                            .geometries
                            .try_reserve_exact(additional)
                            .map_err(|_| "complete_replay_allocation_failed")?;
                        self.guard_additional(cell_bytes)?;
                    }
                    let locations = core::mem::take(&mut self.pending[self.next_geometry].1);
                    let location_bytes = (locations.capacity() as u128)
                        .checked_mul(core::mem::size_of::<GraphLocation>() as u128)
                        .ok_or("complete_replay_memory_projection_overflow")?;
                    self.pending_location_bytes = self
                        .pending_location_bytes
                        .checked_sub(location_bytes)
                        .ok_or("complete_replay_memory_projection_overflow")?;
                    self.source.retained_manifest_nested_bytes = self
                        .source
                        .retained_manifest_nested_bytes
                        .checked_add(location_bytes)
                        .and_then(|n| {
                            n.checked_add(
                                (self.current_patterns.capacity() as u128)
                                    .checked_mul(core::mem::size_of::<PatternManifest>() as u128)?,
                            )
                        })
                        .ok_or("complete_replay_memory_projection_overflow")?;
                    self.source.geometries.push(GeometryManifest {
                        producer_candidate_id: producer_id,
                        locations,
                        witness_count: self.current_witness_count,
                        pattern_count: self.current_patterns.len(),
                        patterns: core::mem::take(&mut self.current_patterns),
                    });
                }
                self.current_witness_count = 0;
                self.next_pattern = 0;
                self.next_geometry += 1;
            }
            self.guard_live()?;
        }
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
        if self.next_geometry == self.pending.len() {
            self.guard_additional(64)?;
            self.source.identity_sha256 = format!("{:x}", self.hasher.clone().finalize());
            self.guard_live()?;
            self.complete = true;
        }
        Ok(self.complete)
    }

    pub fn complete(self) -> Result<Arc<PcReplayPageSource>, &'static str> {
        if !self.complete {
            return Err("pc replay manifest is incomplete");
        }
        let first_page_bytes = (self.source.first_members.capacity() as u128)
            .checked_mul(core::mem::size_of::<PcPathWitnessV2>() as u128)
            .and_then(|n| n.checked_add(self.source.retained_first_member_nested_bytes))
            .ok_or("complete_replay_memory_projection_overflow")?;
        // The source already includes one first-page owner. Retain the same
        // public App/Host/JSON projection reserve for the remaining carriers,
        // without imposing that multiplier on any unprojected raw cell.
        self.guard_additional(
            first_page_bytes
                .checked_mul(REPLAY_PUBLIC_PAGE_RESERVE - 1)
                .and_then(|n| n.checked_add((2 * core::mem::size_of::<usize>()) as u128))
                .ok_or("complete_replay_memory_projection_overflow")?,
        )?;
        Ok(Arc::new(self.source))
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let bytes = (core::mem::size_of::<Self>() as u128)
            .checked_add(self.source.checked_retained_capacity_bytes()?)?
            .checked_add(
                (self.current_patterns.capacity() as u128)
                    .checked_mul(core::mem::size_of::<PatternManifest>() as u128)?,
            )?
            .checked_add(
                (self.pending.capacity() as u128)
                    .checked_mul(core::mem::size_of::<(u64, Vec<GraphLocation>)>() as u128)?,
            )?
            .checked_add(self.pending_location_bytes)?;
        Some(bytes)
    }

    fn guard_live(&self) -> Result<(), &'static str> {
        self.guard_additional(0)
    }

    fn guard_additional(&self, additional: u128) -> Result<(), &'static str> {
        let bytes = self
            .checked_retained_capacity_bytes()
            .and_then(|n| n.checked_add(self.source.original_source_bytes))
            .and_then(|n| n.checked_add(additional))
            .ok_or("complete_replay_memory_projection_overflow")?;
        if bytes > self.source.maximum_bytes {
            return Err("complete_replay_whole_live_limit_exceeded");
        }
        Ok(())
    }
}

impl PcReplayPageSource {
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
    pub(crate) fn first_members(&self) -> &[PcPathWitnessV2] {
        &self.first_members
    }
    pub(crate) fn matches_result(&self, result: &CoreExecutionResult) -> bool {
        self.batches.as_slice() == result.exact_scoring_execution_batches()
    }

    pub fn page_metadata(
        &self,
        geometry_page_number: usize,
        member_page_number: usize,
    ) -> Result<PcReplayPageMetadata, &'static str> {
        let index = geometry_page_number
            .checked_sub(1)
            .ok_or("pc replay invalid geometry page")?;
        let geometry = self
            .geometries
            .get(index)
            .ok_or("pc replay invalid geometry page")?;
        let member_page_count = geometry.witness_count.div_ceil(PC_REPLAY_MEMBER_PAGE_SIZE);
        if member_page_number == 0 || member_page_number > member_page_count {
            return Err("pc replay invalid member page");
        }
        Ok(PcReplayPageMetadata {
            page_contract: PC_REPLAY_MEMBER_PAGE_CONTRACT.to_owned(),
            page_source_available: true,
            page_source_identity_sha256: self.identity_sha256.clone(),
            geometry_count: self.geometry_count().to_string(),
            geometry_page_number: geometry_page_number.to_string(),
            candidate_id: geometry_page_number.to_string(),
            geometry_witness_count: geometry.witness_count.to_string(),
            geometry_pattern_count: geometry.pattern_count.to_string(),
            member_page_number: member_page_number.to_string(),
            member_page_count: member_page_count.to_string(),
        })
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let bytes = (core::mem::size_of::<Self>() as u128)
            .checked_add(self.problem_id.capacity() as u128)?
            .checked_add(self.identity_sha256.capacity() as u128)?
            .checked_add(self.retained_graph_bytes)?
            .checked_add(
                (self.geometries.capacity() as u128)
                    .checked_mul(core::mem::size_of::<GeometryManifest>() as u128)?,
            )?
            .checked_add(
                (self.first_members.capacity() as u128)
                    .checked_mul(core::mem::size_of::<PcPathWitnessV2>() as u128)?,
            )?
            .checked_add(self.retained_manifest_nested_bytes)?
            .checked_add(self.retained_first_member_nested_bytes)?;
        Some(bytes)
    }

    #[cfg(test)]
    pub(crate) fn checked_full_capacity_recount(&self) -> Option<u128> {
        let graphs = self.batches.iter().try_fold(
            (self.batches.capacity() as u128)
                .checked_mul(core::mem::size_of::<ExactScoringExecutionBatch>() as u128)?,
            |bytes, batch| bytes.checked_add(batch.checked_nested_retained_bytes()?),
        )?;
        let manifests = self.geometries.iter().try_fold(0_u128, |bytes, geometry| {
            bytes
                .checked_add(
                    (geometry.locations.capacity() as u128)
                        .checked_mul(core::mem::size_of::<GraphLocation>() as u128)?,
                )?
                .checked_add(
                    (geometry.patterns.capacity() as u128)
                        .checked_mul(core::mem::size_of::<PatternManifest>() as u128)?,
                )
        })?;
        let members = self
            .first_members
            .iter()
            .try_fold(0_u128, |bytes, witness| {
                bytes.checked_add(witness.checked_retained_capacity_bytes()?)
            })?;
        self.checked_retained_capacity_bytes()?
            .checked_sub(self.retained_graph_bytes)?
            .checked_add(graphs)?
            .checked_sub(self.retained_manifest_nested_bytes)?
            .checked_add(manifests)?
            .checked_sub(self.retained_first_member_nested_bytes)?
            .checked_add(members)
    }

    fn output_allowance(&self, additional_live: u128) -> Result<u128, &'static str> {
        self.maximum_bytes
            .checked_sub(self.original_source_bytes)
            .and_then(|bytes| bytes.checked_sub(self.checked_retained_capacity_bytes()?))
            .and_then(|bytes| bytes.checked_sub(additional_live))
            .ok_or("complete_replay_whole_live_limit_exceeded")
    }

    fn materialize_geometry_pattern(
        &self,
        producer_id: u64,
        locations: &[GraphLocation],
        pattern_id: usize,
        additional_live: u128,
        control: &ExecutionControl,
    ) -> Result<Vec<CorePostProcessExecution>, &'static str> {
        let mut executions = Vec::new();
        let allowance = self
            .output_allowance(additional_live)?
            .checked_sub(REPLAY_CELL_CARRIER_BYTES)
            .ok_or("complete_replay_whole_live_limit_exceeded")?;
        for location in locations {
            if control.is_cancelled() {
                return Err("complete_replay_cancelled");
            }
            let retained = checked_execution_bytes(&executions)
                .ok_or("complete_replay_memory_projection_overflow")?;
            let limits = ExactReplayMaterializationLimits::new(
                MAX_GEOMETRY_EXECUTIONS
                    .checked_sub(executions.len())
                    .ok_or("complete_replay_execution_limit_exceeded")?,
                256,
                allowance
                    .checked_sub(retained)
                    .ok_or("complete_replay_whole_live_limit_exceeded")?,
            );
            let (materialized, report) = ExactScoringExecutionMaterializer::materialize_complete_replay_cell_with_limits(
                &self.batches[location.batch], location.graph, pattern_id, control, limits,
            ).map_err(|error| match error {
                clearra_postprocess::ExactReplayMaterializationError::Cancelled => "complete_replay_cancelled",
                clearra_postprocess::ExactReplayMaterializationError::MemoryLimitExceeded { required_memory_bytes, max_memory_bytes } => {
                    #[cfg(test)]
                    eprintln!("pc-replay cell memory producer={producer_id} pattern={pattern_id} batch={} graph={} required={required_memory_bytes} allowed={max_memory_bytes} retained={retained} source={} extra={additional_live} host={}",
                        location.batch, location.graph, self.checked_retained_capacity_bytes().unwrap_or(u128::MAX), self.maximum_bytes);
                    #[cfg(not(test))]
                    let _ = (required_memory_bytes, max_memory_bytes);
                    "complete_replay_memory_limit_exceeded"
                },
                clearra_postprocess::ExactReplayMaterializationError::ExecutionLimitExceeded { .. } => "complete_replay_execution_limit_exceeded",
                clearra_postprocess::ExactReplayMaterializationError::PathStepLimitExceeded { .. } => "complete_replay_path_step_limit_exceeded",
                clearra_postprocess::ExactReplayMaterializationError::AllocationFailed => "complete_replay_allocation_failed",
                _ => "complete_replay_evidence_invalid",
            })?;
            if !materialized.complete() {
                return Err("pc replay geometry execution is incomplete");
            }
            #[cfg(test)]
            if producer_id == 1 && (pattern_id == 64 || pattern_id == 0) {
                eprintln!(
                    "pc-replay cell producer={producer_id} pattern={pattern_id} raw={} dedup={} retained={} admitted_peak={} allowance={allowance}",
                    report.raw_execution_count(), report.execution_count(), report.retained_bytes(), report.admitted_peak_bytes(),
                );
            }
            let materialized_bytes = report.retained_bytes();
            for aggregate in materialized.into_aggregates() {
                let (candidate_id, members) = aggregate.into_parts();
                if candidate_id != producer_id {
                    return Err("pc replay geometry identity mismatch");
                }
                // Member trace/String allocations move into Core owners; only
                // both inline vectors overlap during reserve/transfer. Guard
                // that real peak before allocating and after actual capacity
                // is known, rather than charging an eager whole-family factor.
                let new_inline = (executions.len() as u128)
                    .checked_add(members.len() as u128)
                    .and_then(|n| {
                        n.checked_mul(core::mem::size_of::<CorePostProcessExecution>() as u128)
                    })
                    .ok_or("complete_replay_memory_projection_overflow")?;
                let transfer_peak = checked_execution_bytes(&executions)
                    .and_then(|n| n.checked_add(materialized_bytes))
                    .and_then(|n| n.checked_add(new_inline))
                    .ok_or("complete_replay_memory_projection_overflow")?;
                if transfer_peak > allowance {
                    return Err("complete_replay_transfer_peak_exceeded");
                }
                executions
                    .try_reserve_exact(members.len())
                    .map_err(|_| "complete_replay_allocation_failed")?;
                if checked_execution_bytes(&executions)
                    .and_then(|n| n.checked_add(materialized_bytes))
                    .ok_or("complete_replay_memory_projection_overflow")?
                    > allowance
                {
                    return Err("complete_replay_transfer_peak_exceeded");
                }
                for member in members {
                    let (pattern_id, trace_identity, trace) = member.into_parts();
                    // The public canonical ordering is (candidate, pattern,
                    // normalized_trace_key, trace_identity). The materializer
                    // emits canonical trace identity; verify that invariant
                    // before sorting or slicing pages so a future producer
                    // change cannot silently reorder a complete family.
                    if !trace.canonical_key_matches(&trace_identity) {
                        return Err("complete_replay_trace_identity_mismatch");
                    }
                    executions.push(CorePostProcessExecution::new(
                        candidate_id,
                        pattern_id,
                        trace_identity,
                        trace,
                    ));
                }
            }
            if checked_execution_bytes(&executions)
                .ok_or("complete_replay_memory_projection_overflow")?
                > allowance
            {
                return Err("complete_replay_whole_live_limit_exceeded");
            }
        }
        executions.sort_unstable_by(|a, b| {
            a.pattern_id()
                .cmp(&b.pattern_id())
                .then_with(|| a.trace_identity().cmp(b.trace_identity()))
        });
        executions.dedup_by(|a, b| {
            a.pattern_id() == b.pattern_id() && a.trace_identity() == b.trace_identity()
        });
        Ok(executions)
    }
}

#[derive(Debug)]
pub struct PcReplayPageStore {
    source: Arc<PcReplayPageSource>,
    current_geometry: Option<usize>,
    current_pattern: Option<usize>,
    current_executions: Vec<CorePostProcessExecution>,
}

impl PcReplayPageStore {
    pub fn new(source: Arc<PcReplayPageSource>) -> Self {
        Self {
            source,
            current_geometry: None,
            current_pattern: None,
            current_executions: Vec::new(),
        }
    }

    pub fn source(&self) -> &Arc<PcReplayPageSource> {
        &self.source
    }

    pub fn page(
        &mut self,
        geometry_page_number: usize,
        member_page_number: usize,
        control: &ExecutionControl,
    ) -> Result<PcReplayPagePayload, &'static str> {
        let metadata = self
            .source
            .page_metadata(geometry_page_number, member_page_number)?;
        let index = geometry_page_number - 1;
        let start = (member_page_number - 1)
            .checked_mul(PC_REPLAY_MEMBER_PAGE_SIZE)
            .ok_or("pc replay invalid member page")?;
        let end = start
            .saturating_add(PC_REPLAY_MEMBER_PAGE_SIZE)
            .min(self.source.geometries[index].witness_count);
        let mut witnesses: Vec<clearra_host_contract::PcPathWitnessPayload> = Vec::new();
        let requested_page_slots = ((end - start) as u128)
            .checked_mul(
                core::mem::size_of::<clearra_host_contract::PcPathWitnessPayload>() as u128,
            )
            .ok_or("complete_replay_memory_projection_overflow")?;
        self.guard_page_peak(
            metadata
                .checked_retained_capacity_bytes()
                .and_then(|n| n.checked_add(requested_page_slots))
                .ok_or("complete_replay_memory_projection_overflow")?,
            0,
        )?;
        witnesses
            .try_reserve_exact(end - start)
            .map_err(|_| "complete_replay_allocation_failed")?;
        let geometry = &self.source.geometries[index];
        for pattern in &geometry.patterns {
            let pattern_start = pattern.end_offset - pattern.witness_count;
            if pattern.end_offset <= start {
                continue;
            }
            if pattern_start >= end {
                break;
            }
            if control.is_cancelled() {
                return Err("complete_replay_cancelled");
            }
            if self.current_geometry != Some(index)
                || self.current_pattern != Some(pattern.pattern_id)
            {
                // Drop the old cell before loading a new one. A selected
                // geometry may contain millions of witnesses, but no retained
                // owner expands that entire Cartesian family.
                self.current_executions = Vec::new();
                self.current_geometry = None;
                self.current_pattern = None;
                let mut page_bytes = (witnesses.capacity() as u128)
                    .checked_mul(
                        core::mem::size_of::<clearra_host_contract::PcPathWitnessPayload>() as u128,
                    )
                    .and_then(|bytes| {
                        bytes.checked_add(metadata.checked_retained_capacity_bytes()?)
                    })
                    .ok_or("complete_replay_memory_projection_overflow")?;
                for witness in &witnesses {
                    page_bytes = page_bytes
                        .checked_add(
                            witness
                                .checked_retained_capacity_bytes()
                                .ok_or("complete_replay_memory_projection_overflow")?,
                        )
                        .ok_or("complete_replay_memory_projection_overflow")?;
                }
                let additional_live = (core::mem::size_of::<Self>() as u128)
                    .checked_add(
                        page_bytes
                            .checked_mul(REPLAY_PUBLIC_PAGE_RESERVE)
                            .ok_or("complete_replay_memory_projection_overflow")?,
                    )
                    .ok_or("complete_replay_memory_projection_overflow")?;
                let executions = self.source.materialize_geometry_pattern(
                    geometry.producer_candidate_id,
                    &geometry.locations,
                    pattern.pattern_id,
                    additional_live,
                    control,
                )?;
                if executions.len() != pattern.witness_count {
                    return Err("pc replay manifest count mismatch");
                }
                self.current_executions = executions;
                self.current_geometry = Some(index);
                self.current_pattern = Some(pattern.pattern_id);
            }
            let local_start = start.saturating_sub(pattern_start);
            let local_end = (end - pattern_start).min(pattern.witness_count);
            for execution in &self.current_executions[local_start..local_end] {
                if control.is_cancelled() {
                    return Err("complete_replay_cancelled");
                }
                let page_bytes = checked_partial_page_bytes(&metadata, &witnesses)
                    .ok_or("complete_replay_memory_projection_overflow")?;
                let requested_witness = checked_execution_projection_peak_bytes(execution)
                    .ok_or("complete_replay_memory_projection_overflow")?;
                self.guard_page_peak(page_bytes, requested_witness)?;
                let witness = project_execution_with_context(
                    self.source.projection,
                    execution,
                    self.source.materialized_pattern_count,
                    geometry_page_number as u64,
                )?;
                let actual_witness = (core::mem::size_of::<PcPathWitnessV2>() as u128)
                    .checked_add(
                        witness
                            .checked_retained_capacity_bytes()
                            .ok_or("complete_replay_memory_projection_overflow")?,
                    )
                    .ok_or("complete_replay_memory_projection_overflow")?;
                self.guard_page_peak(page_bytes, actual_witness)?;
                let payload = pc_path_witness_payload(&witness);
                self.guard_page_peak(
                    page_bytes
                        .checked_add(
                            payload
                                .checked_retained_capacity_bytes()
                                .ok_or("complete_replay_memory_projection_overflow")?,
                        )
                        .ok_or("complete_replay_memory_projection_overflow")?,
                    actual_witness,
                )?;
                witnesses.push(payload);
            }
        }
        if witnesses.len() != end - start {
            return Err("pc replay member page count mismatch");
        }
        let page = PcReplayPagePayload {
            metadata,
            witness_count: self.source.witness_count.to_string(),
            materialized_pattern_count: self.source.materialized_pattern_count.to_string(),
            witnesses,
        };
        let peak = self
            .checked_retained_capacity_bytes()
            .and_then(|bytes| bytes.checked_add(self.source.original_source_bytes))
            .and_then(|bytes| {
                bytes.checked_add(
                    page.checked_retained_capacity_bytes()?
                        .checked_mul(REPLAY_PUBLIC_PAGE_RESERVE)?,
                )
            })
            .ok_or("complete_replay_memory_projection_overflow")?;
        if peak > self.source.maximum_bytes {
            return Err("complete_replay_whole_live_limit_exceeded");
        }
        Ok(page)
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (core::mem::size_of::<Self>() as u128)
            .checked_add(self.source.checked_retained_capacity_bytes()?)?
            .checked_add(checked_execution_bytes(&self.current_executions)?)
    }

    fn guard_page_peak(
        &self,
        public_page_bytes: u128,
        one_witness_bytes: u128,
    ) -> Result<(), &'static str> {
        let peak = self
            .checked_retained_capacity_bytes()
            .and_then(|n| n.checked_add(self.source.original_source_bytes))
            .and_then(|n| {
                n.checked_add(
                    public_page_bytes
                        .checked_add(one_witness_bytes)?
                        .checked_mul(REPLAY_PUBLIC_PAGE_RESERVE)?,
                )
            })
            .ok_or("complete_replay_memory_projection_overflow")?;
        if peak > self.source.maximum_bytes {
            return Err("complete_replay_whole_live_limit_exceeded");
        }
        Ok(())
    }
}

fn checked_partial_page_bytes(
    metadata: &PcReplayPageMetadata,
    witnesses: &Vec<clearra_host_contract::PcPathWitnessPayload>,
) -> Option<u128> {
    witnesses.iter().try_fold(
        metadata.checked_retained_capacity_bytes()?.checked_add(
            (witnesses.capacity() as u128).checked_mul(core::mem::size_of::<
                clearra_host_contract::PcPathWitnessPayload,
            >() as u128)?,
        )?,
        |bytes, witness| bytes.checked_add(witness.checked_retained_capacity_bytes()?),
    )
}

fn checked_execution_bytes(executions: &Vec<CorePostProcessExecution>) -> Option<u128> {
    let bytes = (executions.capacity() as u128)
        .checked_mul(core::mem::size_of::<CorePostProcessExecution>() as u128)?;
    executions.iter().try_fold(bytes, |bytes, execution| {
        bytes.checked_add(execution.checked_nested_retained_bytes()?)
    })
}
