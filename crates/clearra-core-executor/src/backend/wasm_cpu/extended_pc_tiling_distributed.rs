//! Four-word, geometry-only browser distribution. The coordinator compiles one
//! exact ILC family, then delegates deterministic disjoint Geometry branches.
//! Workers never run BuildUp or attach PC coverage, minimum, or replay evidence.
//! Only the original admitted Tiling owner may publish a complete family.

use std::{mem::size_of, sync::Arc};

use clearra_core_domain::{
    objective::objective_kind::ObjectiveKind, solution::ExtendedTilingSolutionKey,
};
use clearra_problem::{SearchOutputPolicy, SearchProblem, SearchProblemPreset};
use sha2::{Digest, Sha256};

use crate::{
    resource::{
        admit_budget_bound_search_execution,
        admit_budget_bound_search_execution_under_terminal_authority, ExecutionAdmission,
    },
    CoreExecutionResult, WasmCpuSearchError, WasmCpuTerminalResourceAuthority,
};

use super::super::{
    extended_board::ExtendedBoard, extended_geometry::ExtendedParallelGeometryPlan,
    kick_profiles::replay_profile_ids, WasmCandidatePacket, WasmCandidateProducerAdvance,
    WasmDistributedBackendExecution, WasmDistributedGeometrySummary, WasmDistributedProgress,
};
use super::{
    ExactSearchAdvance, ExecutionControl, ExtendedGeometryAdvance, ExtendedGeometrySearch,
    ExtendedInverseCatalog, ExtendedPcTilingSession, ExtendedTilingKey, WasmExactSearchError,
};

const MAGIC: &[u8; 8] = b"CTL24C01";
const CHUNK_KEYS: usize = 512;

#[derive(Clone, Copy)]
struct RootReceipt {
    first: u64,
    count: u64,
    received: u64,
    complete: bool,
}

struct FamilyOwner {
    session: ExtendedPcTilingSession,
    reconstruction: ExtendedGeometrySearch,
    roots: Vec<RootReceipt>,
    expected_count: usize,
    workers: usize,
    original_external: u128,
    dispatched: usize,
    source_complete: bool,
}

pub struct WasmFullHeightTilingProducer {
    owner: FamilyOwner,
    plan: Option<ExtendedParallelGeometryPlan>,
    replica_cap: u128,
}

pub struct WasmFullHeightTilingWorker {
    session: ExtendedPcTilingSession,
    plan: Option<ExtendedParallelGeometryPlan>,
    workers: usize,
    root: Option<usize>,
    produced: u64,
    chunk_start: u64,
    completed_roots: usize,
    total_candidates: usize,
    finished: bool,
}

pub struct WasmFullHeightTilingResultMerger {
    owner: FamilyOwner,
}

impl WasmFullHeightTilingProducer {
    pub fn supports(problem: &SearchProblem) -> bool {
        matches!(
            problem.preset(),
            SearchProblemPreset::ScenarioPc | SearchProblemPreset::OpeningPc
        ) && problem.goal().as_str() == "clear-to-empty"
            && problem.output_policy() == SearchOutputPolicy::TilingOnly
            && problem.objective().kind() == ObjectiveKind::Tiling
            && !problem.objective().execution_constraints().requested()
            && !problem.solution_probability_policy().requested()
            && problem.initial_board().width() == 10
            && (7..=24).contains(&problem.visible_height())
    }

    /// A bounded producer-envelope projection for the App-owned terminal lease.
    /// Count-proportional DAG, key, packet and wire storage is admitted at the
    /// corresponding allocation boundary, not hidden inside this fixed bound.
    pub fn checked_shared_external_retained_upper_bound(problem: &SearchProblem) -> Option<u128> {
        if !Self::supports(problem) {
            return None;
        }
        let roots = problem.backend_policy().workers().max(1).checked_mul(4)?;
        (size_of::<Self>() as u128)
            .checked_add(size_of::<WasmFullHeightTilingResultMerger>() as u128)?
            .checked_add((roots as u128).checked_mul(size_of::<RootReceipt>() as u128)?)
    }

    pub fn new_shared_under_authority(
        problem: Arc<SearchProblem>,
        workers: usize,
        external: u128,
        authority: &WasmCpuTerminalResourceAuthority,
    ) -> Result<Self, WasmCpuSearchError> {
        let admission = admit_budget_bound_search_execution_under_terminal_authority(
            &problem, external, authority, 1,
        )
        .map_err(WasmCpuSearchError::resource_admission)?;
        Self::with_admission(problem, workers, external, admission)
    }

    fn with_admission(
        problem: Arc<SearchProblem>,
        workers: usize,
        external: u128,
        admission: ExecutionAdmission,
    ) -> Result<Self, WasmCpuSearchError> {
        validate_request(&problem, workers)?;
        // N realm-private shares and two coordinator shares fit in the one
        // original logical cap. Separate realms never each acquire that cap.
        let replica_cap = replica_cap(admission.memory_cap_bytes(), external, workers)?;
        let parent_cap = replica_cap
            .checked_mul(2)
            .and_then(|n| n.checked_add(external))
            .ok_or_else(projection_error)?;
        let admission = admission
            .restrict_memory_cap(parent_cap)
            .map_err(WasmCpuSearchError::resource_admission)?;
        let session = ExtendedPcTilingSession::new_with_admission(problem, external, admission)
            .map_err(typed_error)?;
        let future = session
            .geometry
            .checked_external_verifier_future_bytes()
            .and_then(|n| n.checked_add(size_of::<ExtendedGeometrySearch>() as u128))
            .ok_or_else(projection_error)?;
        session.ensure_memory(future).map_err(typed_error)?;
        let reconstruction = session
            .geometry
            .external_verifier_from_owner()
            .ok_or_else(|| invalid("extended_tiling_distributed_index_unavailable"))?;
        let owner = FamilyOwner {
            session,
            reconstruction,
            roots: Vec::new(),
            expected_count: 0,
            workers,
            original_external: external,
            dispatched: 0,
            source_complete: false,
        };
        let producer = Self {
            owner,
            plan: None,
            replica_cap,
        };
        producer.validate_external_result_memory(0)?;
        Ok(producer)
    }

    pub fn new_delegated_worker(&self) -> Result<WasmFullHeightTilingWorker, WasmCpuSearchError> {
        let admission = self
            .owner
            .session
            .execution_admission
            .try_delegate_compute_only_with_memory_cap(self.replica_cap)
            .map_err(WasmCpuSearchError::resource_admission)?;
        // Only immutable input/catalog ownership is shared. The worker owns
        // a new traversal/compiler and must not take the producer's cursor.
        WasmFullHeightTilingWorker::with_shared_catalog(
            &self.owner.session,
            self.owner.workers,
            admission,
        )
    }

    pub fn advance_with_external_retained(
        &mut self,
        control: &ExecutionControl,
        external: u128,
    ) -> Result<WasmCandidateProducerAdvance, WasmCpuSearchError> {
        if self.owner.source_complete {
            return Err(invalid("extended_tiling_distributed_producer_finished"));
        }
        if control.is_cancelled() {
            return Ok(WasmCandidateProducerAdvance::Cancelled);
        }
        self.validate_external_result_memory(external)?;
        if self.owner.session.geometry.is_compiling() {
            let outside = self
                .checked_retained_bytes()
                .and_then(|n| n.checked_add(external))
                .and_then(|n| n.checked_sub(self.owner.session.geometry.retained_bytes() as u128))
                .ok_or_else(projection_error)?;
            match self
                .owner
                .session
                .geometry
                .advance_bounded(
                    &self.owner.session.catalog,
                    None,
                    self.owner.session.execution_admission.memory_bound(),
                    outside,
                )
                .map_err(typed_error)?
            {
                ExtendedGeometryAdvance::Pending => {}
                ExtendedGeometryAdvance::ResourceIncomplete(reason) => return Err(invalid(reason)),
                _ => return Err(invalid("extended_tiling_distributed_prepare_invalid")),
            }
            self.validate_external_result_memory(external)?;
            return Ok(WasmCandidateProducerAdvance::Pending);
        }
        if self.plan.is_none() && self.owner.roots.is_empty() {
            let live = self
                .checked_retained_bytes()
                .ok_or_else(projection_error)?
                .checked_add(external)
                .ok_or_else(projection_error)?;
            let desired = self
                .owner
                .workers
                .checked_mul(4)
                .ok_or_else(projection_error)?;
            let plan = self
                .owner
                .session
                .geometry
                .take_parallel_plan(
                    desired,
                    self.owner.session.execution_admission.memory_bound(),
                    live,
                )
                .map_err(typed_error)?;
            let expected = self
                .owner
                .session
                .geometry
                .candidate_family_count()
                .ok_or_else(|| invalid("extended_tiling_distributed_family_count_unavailable"))?;
            self.owner.expected_count =
                usize::try_from(expected).map_err(|_| projection_error())?;
            let limit = self
                .owner
                .session
                .problem
                .backend_request()
                .max_candidates();
            if limit != 0 && self.owner.expected_count > limit {
                return Err(invalid("candidate_budget_exceeded"));
            }
            if let Some(plan) = plan {
                if plan.candidate_count != expected {
                    return Err(invalid("extended_tiling_distributed_family_count_mismatch"));
                }
                self.owner.ensure_memory(
                    external,
                    (plan.branches.len() as u128) * size_of::<RootReceipt>() as u128
                        + plan.shared_retained_bytes() as u128
                        + plan.branch_retained_bytes() as u128,
                )?;
                self.owner
                    .roots
                    .try_reserve_exact(plan.branches.len())
                    .map_err(|_| storage_error())?;
                for branch in &plan.branches {
                    self.owner.roots.push(RootReceipt {
                        first: u64::try_from(branch.first_ordinal)
                            .map_err(|_| projection_error())?,
                        count: u64::try_from(branch.candidate_count)
                            .map_err(|_| projection_error())?,
                        received: 0,
                        complete: false,
                    });
                }
                self.plan = Some(plan);
            } else if expected != 0 {
                return Err(invalid("extended_tiling_distributed_family_missing"));
            }
            self.validate_external_result_memory(external)?;
        }
        if self.owner.dispatched < self.owner.roots.len() {
            let index = self.owner.dispatched;
            self.owner.dispatched += 1;
            return Ok(WasmCandidateProducerAdvance::Candidate(
                WasmCandidatePacket::for_pass(
                    index as u64,
                    0,
                    u32::try_from(index).map_err(|_| projection_error())?,
                    Vec::new(),
                ),
            ));
        }
        self.owner.source_complete = true;
        Ok(WasmCandidateProducerAdvance::Completed(
            self.owner.summary(),
        ))
    }

    pub fn absorb_raw(&mut self, bytes: &[u8], external: u128) -> Result<(), WasmCpuSearchError> {
        let plan_bytes = self.plan.as_ref().map_or(0, plan_retained_bytes);
        self.owner.absorb_raw(
            bytes,
            external
                .checked_add(plan_bytes)
                .ok_or_else(projection_error)?,
        )
    }

    pub fn validate_external_result_memory(
        &self,
        external: u128,
    ) -> Result<(), WasmCpuSearchError> {
        self.owner
            .session
            .execution_admission
            .ensure_memory_bound(
                self.checked_retained_bytes().ok_or_else(projection_error)?,
                external,
            )
            .map_err(WasmCpuSearchError::resource_admission)
    }

    fn checked_retained_bytes(&self) -> Option<u128> {
        self.owner
            .checked_retained_bytes()?
            .checked_add(size_of::<Self>() as u128)?
            .checked_add(self.plan.as_ref().map_or(0, plan_retained_bytes))
    }

    pub fn progress(&self) -> WasmDistributedProgress {
        self.owner.progress()
    }

    pub fn into_merger(self) -> Result<WasmFullHeightTilingResultMerger, &'static str> {
        self.into_merger_with_external_retained(0)
    }

    pub fn into_merger_with_external_retained(
        self,
        external: u128,
    ) -> Result<WasmFullHeightTilingResultMerger, &'static str> {
        if !self.owner.source_complete {
            return Err("extended_tiling_distributed_source_not_complete");
        }
        self.validate_external_result_memory(external)
            .map_err(|e| e.reason())?;
        // The original compiler metrics and source family cardinality remain
        // on this Tiling session; only the unused enumeration DAG is released.
        drop(self.plan);
        let merger = WasmFullHeightTilingResultMerger { owner: self.owner };
        merger
            .validate_external_result_memory(external, 0)
            .map_err(|e| e.reason())?;
        Ok(merger)
    }
}

impl WasmFullHeightTilingWorker {
    pub fn new(problem: Arc<SearchProblem>, workers: usize) -> Result<Self, WasmCpuSearchError> {
        validate_request(&problem, workers)?;
        let admission = admit_budget_bound_search_execution(&problem, 1)
            .map_err(WasmCpuSearchError::resource_admission)?;
        let cap = replica_cap(admission.memory_cap_bytes(), 0, workers)?;
        let admission = admission
            .restrict_memory_cap(cap)
            .map_err(WasmCpuSearchError::resource_admission)?;
        let session = ExtendedPcTilingSession::new_with_admission(problem, 0, admission)
            .map_err(typed_error)?;
        Self::from_session(session, workers)
    }

    fn with_shared_catalog(
        source: &ExtendedPcTilingSession,
        workers: usize,
        admission: ExecutionAdmission,
    ) -> Result<Self, WasmCpuSearchError> {
        let universe = source
            .problem
            .piece_source()
            .materialized_universe()
            .ok_or_else(|| invalid("wasm_piece_source_not_materialized"))?;
        let count = source.catalog.required_cells().count_ones() as usize / 4;
        let family = universe.packing_multiset_family_for_execution(
            count,
            source.problem.initial_hold(),
            source.problem.supply().hold_enabled(),
            super::super::packing_hold_projection(&source.problem),
        );
        let outside = (size_of::<ExtendedPcTilingSession>() as u128)
            .checked_add(source.catalog.retained_bytes() as u128)
            .and_then(|n| n.checked_add(size_of::<ExtendedInverseCatalog>() as u128 + 32))
            .and_then(|n| n.checked_add(super::super::extended_pc_search::checked_pc_family_problem_nested_retained_bytes(&source.problem)?))
            .and_then(|n| n.checked_add(size_of::<SearchProblem>() as u128 + 32))
            .ok_or_else(projection_error)?;
        let geometry = ExtendedGeometrySearch::new_bounded(
            universe,
            &family,
            &source.catalog,
            admission.memory_bound(),
            outside,
        )
        .map_err(typed_error)?;
        Self::from_session(
            ExtendedPcTilingSession {
                problem: Arc::clone(&source.problem),
                catalog: Arc::clone(&source.catalog),
                geometry,
                keys: Vec::new(),
                execution_admission: admission,
                external_retained_bytes: 0,
                finished: false,
                workers_used: 1,
                parallel_active_workers: 1,
                parallel_peak_bytes: 0,
                request_node_budget: None,
            },
            workers,
        )
    }

    fn from_session(
        session: ExtendedPcTilingSession,
        workers: usize,
    ) -> Result<Self, WasmCpuSearchError> {
        let worker = Self {
            session,
            plan: None,
            workers,
            root: None,
            produced: 0,
            chunk_start: 0,
            completed_roots: 0,
            total_candidates: 0,
            finished: false,
        };
        worker.validate_external_result_memory(0, 0)?;
        Ok(worker)
    }

    /// Returns (raw chunk, root completed, number of concrete candidates).
    /// Each entry advances one exact candidate or one compiler step, yielding
    /// between entries so worker heartbeat/cancellation does not wait for a root.
    /// Returned transfers belong to the caller. If it retains earlier buffers,
    /// their allocator-visible capacity must remain in `external` on later calls.
    pub fn advance_root_with_external_retained(
        &mut self,
        packet: &WasmCandidatePacket,
        control: &ExecutionControl,
        external: u128,
    ) -> Result<(Option<Vec<u8>>, bool, usize), WasmCpuSearchError> {
        if self.finished
            || packet.pass_index() != 0
            || !packet.row_ids().is_empty()
            || packet.ordinal() != u64::from(packet.target_index())
        {
            return Err(invalid("extended_tiling_distributed_root_packet_invalid"));
        }
        if control.is_cancelled() {
            return Err(invalid("wasm_cpu_search_cancelled"));
        }
        self.validate_external_result_memory(external, 0)?;
        if self.session.geometry.is_compiling() {
            let outside = self
                .checked_retained_bytes()
                .and_then(|n| n.checked_add(external))
                .and_then(|n| n.checked_sub(self.session.geometry.retained_bytes() as u128))
                .ok_or_else(projection_error)?;
            match self
                .session
                .geometry
                .advance_bounded(
                    &self.session.catalog,
                    None,
                    self.session.execution_admission.memory_bound(),
                    outside,
                )
                .map_err(typed_error)?
            {
                ExtendedGeometryAdvance::Pending => {}
                ExtendedGeometryAdvance::ResourceIncomplete(reason) => return Err(invalid(reason)),
                _ => return Err(invalid("extended_tiling_distributed_prepare_invalid")),
            }
            self.validate_external_result_memory(external, 0)?;
            return Ok((None, false, 0));
        }
        if self.plan.is_none() {
            let live = self
                .checked_retained_bytes()
                .ok_or_else(projection_error)?
                .checked_add(external)
                .ok_or_else(projection_error)?;
            self.plan = self
                .session
                .geometry
                .take_parallel_plan(
                    self.workers.checked_mul(4).ok_or_else(projection_error)?,
                    self.session.execution_admission.memory_bound(),
                    live,
                )
                .map_err(typed_error)?;
            if self.plan.is_none() {
                return Err(invalid("extended_tiling_distributed_worker_family_empty"));
            }
        }
        let index = packet.target_index() as usize;
        if let Some(current) = self.root {
            if current != index {
                return Err(invalid("extended_tiling_distributed_root_still_pending"));
            }
        } else {
            let plan = self.plan.as_ref().ok_or_else(projection_error)?;
            if plan.branches.get(index).is_none() {
                return Err(invalid("extended_tiling_distributed_root_not_in_family"));
            }
            self.root = Some(index);
            self.produced = 0;
            self.chunk_start = 0;
        }
        let owned = self
            .checked_retained_bytes()
            .ok_or_else(projection_error)?
            .checked_add(external)
            .ok_or_else(projection_error)?;
        let bound = self.session.execution_admission.memory_bound();
        let branch = &mut self.plan.as_mut().ok_or_else(projection_error)?.branches[index];
        let outside = owned
            .checked_sub(branch.private_retained_bytes() as u128)
            .ok_or_else(projection_error)?;
        let candidate = branch
            .next_candidate_bounded(&self.session.catalog, bound, outside)
            .map_err(typed_error)?;
        if let Some(candidate) = candidate {
            let pieces = candidate.row_ids().len() as u128;
            let growth = if self.session.keys.len() == self.session.keys.capacity() {
                self.session
                    .keys
                    .len()
                    .saturating_add(1)
                    .saturating_mul(2)
                    .max(4) as u128
                    * size_of::<String>() as u128
            } else {
                0
            };
            self.validate_external_result_memory(external, growth + 128 + pieces * (64 + 67))?;
            self.session
                .keys
                .try_reserve(1)
                .map_err(|_| storage_error())?;
            let tiling = ExtendedTilingKey::from_candidate(&self.session.catalog, &candidate);
            self.session.keys.push(tiling.canonical_key(
                self.session.catalog.initial_board(),
                self.session.catalog.height(),
            ));
            self.produced = self.produced.checked_add(1).ok_or_else(projection_error)?;
            self.total_candidates = self
                .total_candidates
                .checked_add(1)
                .ok_or_else(projection_error)?;
            let expected =
                self.plan.as_ref().ok_or_else(projection_error)?.branches[index].candidate_count;
            if u128::from(self.produced) > expected {
                return Err(invalid("extended_tiling_distributed_root_count_mismatch"));
            }
            let output = if self.session.keys.len() == CHUNK_KEYS {
                Some(self.flush(false, external)?)
            } else {
                None
            };
            self.validate_external_result_memory(external, 0)?;
            return Ok((output, false, 1));
        }
        let expected =
            self.plan.as_ref().ok_or_else(projection_error)?.branches[index].candidate_count;
        if u128::from(self.produced) != expected {
            return Err(invalid("extended_tiling_distributed_root_count_mismatch"));
        }
        let output = self.flush(true, external)?;
        self.root = None;
        self.completed_roots += 1;
        Ok((Some(output), true, 0))
    }

    fn flush(&mut self, complete: bool, external: u128) -> Result<Vec<u8>, WasmCpuSearchError> {
        let index = self
            .root
            .ok_or_else(|| invalid("extended_tiling_distributed_no_pending_root"))?;
        let branch = &self.plan.as_ref().ok_or_else(projection_error)?.branches[index];
        let chunk = Chunk {
            root: index as u32,
            first: u64::try_from(branch.first_ordinal).map_err(|_| projection_error())?,
            count: u64::try_from(branch.candidate_count).map_err(|_| projection_error())?,
            start: self.chunk_start,
            complete,
            keys: core::mem::take(&mut self.session.keys),
        };
        let size = chunk.encoded_len().ok_or_else(projection_error)?;
        // The key owner moved above still coexists with the encoded transfer.
        self.validate_external_result_memory(
            external
                .checked_add(
                    chunk
                        .checked_retained_bytes()
                        .ok_or_else(projection_error)?,
                )
                .ok_or_else(projection_error)?,
            size as u128,
        )?;
        let output = chunk.encode(&self.session.problem, self.workers)?;
        self.chunk_start = self.produced;
        self.validate_external_result_memory(
            external
                .checked_add(
                    chunk
                        .checked_retained_bytes()
                        .ok_or_else(projection_error)?,
                )
                .and_then(|n| n.checked_add(output.capacity() as u128))
                .ok_or_else(projection_error)?,
            0,
        )?;
        Ok(output)
    }

    fn checked_retained_bytes(&self) -> Option<u128> {
        self.session
            .checked_retained_bytes()?
            .checked_add(size_of::<Self>() as u128)?
            .checked_add(self.plan.as_ref().map_or(0, plan_retained_bytes))
    }

    pub fn validate_external_result_memory(
        &self,
        external: u128,
        future: u128,
    ) -> Result<(), WasmCpuSearchError> {
        let owned = self
            .checked_retained_bytes()
            .ok_or_else(projection_error)?
            .checked_add(external)
            .ok_or_else(projection_error)?;
        self.session
            .execution_admission
            .ensure_memory_bound(owned, future)
            .map_err(WasmCpuSearchError::resource_admission)
    }

    pub fn progress(&self) -> WasmDistributedProgress {
        WasmDistributedProgress {
            geometry_nodes: self.session.geometry.expanded_nodes(),
            candidates: self.completed_roots,
            build_nodes: self.total_candidates,
            candidate_family_count: self.session.geometry.candidate_family_count(),
            pass_count: 1,
            ..WasmDistributedProgress::default()
        }
    }

    pub fn finish_raw(&mut self) -> Result<Vec<u8>, WasmCpuSearchError> {
        if self.finished || self.root.is_some() || !self.session.keys.is_empty() {
            return Err(invalid("extended_tiling_distributed_worker_finish_pending"));
        }
        self.finished = true;
        Ok(Vec::new()) // all finite chunks were transferred at their root boundary
    }
}

impl FamilyOwner {
    fn checked_retained_bytes(&self) -> Option<u128> {
        self.session
            .checked_retained_bytes()?
            .checked_add(size_of::<Self>() as u128)?
            .checked_add(self.reconstruction.retained_bytes() as u128)?
            .checked_add(size_of::<ExtendedGeometrySearch>() as u128)?
            .checked_add(
                (self.roots.capacity() as u128).checked_mul(size_of::<RootReceipt>() as u128)?,
            )
    }

    fn ensure_memory(&self, external: u128, future: u128) -> Result<(), WasmCpuSearchError> {
        self.session
            .execution_admission
            .ensure_memory_bound(
                self.checked_retained_bytes()
                    .ok_or_else(projection_error)?
                    .checked_add(external)
                    .ok_or_else(projection_error)?,
                future,
            )
            .map_err(WasmCpuSearchError::resource_admission)
    }

    fn absorb_raw(&mut self, bytes: &[u8], external: u128) -> Result<(), WasmCpuSearchError> {
        if self.session.finished || external < bytes.len() as u128 {
            return Err(invalid("extended_tiling_distributed_merger_state_invalid"));
        }
        let chunk = Chunk::decode(bytes, &self.session.problem, self.workers, |future| {
            self.ensure_memory(external, future)
        })?;
        let index = chunk.root as usize;
        let receipt = self
            .roots
            .get(index)
            .ok_or_else(|| invalid("extended_tiling_distributed_root_not_in_family"))?;
        let count = u64::try_from(chunk.keys.len()).map_err(|_| projection_error())?;
        let next = receipt
            .received
            .checked_add(count)
            .ok_or_else(projection_error)?;
        if index >= self.dispatched
            || receipt.complete
            || chunk.first != receipt.first
            || chunk.count != receipt.count
            || chunk.start != receipt.received
            || next > receipt.count
            || (chunk.complete && next != receipt.count)
            || (!chunk.complete && chunk.keys.is_empty())
        {
            return Err(invalid("extended_tiling_distributed_root_receipt_mismatch"));
        }
        let owned = external
            .checked_add(
                chunk
                    .checked_retained_bytes()
                    .ok_or_else(projection_error)?,
            )
            .ok_or_else(projection_error)?;
        let growth = (self
            .session
            .keys
            .len()
            .checked_add(chunk.keys.len())
            .ok_or_else(projection_error)? as u128)
            .checked_mul(2 * size_of::<String>() as u128)
            .ok_or_else(projection_error)?;
        // Parsing/reconstruction is bounded to sixty four-word placements.
        self.ensure_memory(owned, growth + 128 + 60 * (4 + 64 + 67))?;
        for key in &chunk.keys {
            let rows = row_ids(key, &self.session.catalog)?;
            let candidate = self
                .reconstruction
                .external_candidate(&self.session.catalog, &rows)
                .ok_or_else(|| invalid("extended_tiling_distributed_key_not_in_source_family"))?;
            if ExtendedTilingKey::from_candidate(&self.session.catalog, &candidate).canonical_key(
                self.session.catalog.initial_board(),
                self.session.catalog.height(),
            ) != *key
            {
                return Err(invalid("extended_tiling_distributed_key_not_canonical"));
            }
        }
        self.session
            .keys
            .try_reserve(chunk.keys.len())
            .map_err(|_| storage_error())?;
        self.session.keys.extend(chunk.keys);
        self.roots[index].received = next;
        self.roots[index].complete = chunk.complete;
        self.ensure_memory(external, 0)
    }

    fn progress(&self) -> WasmDistributedProgress {
        WasmDistributedProgress {
            geometry_nodes: self.session.geometry.expanded_nodes(),
            candidates: self.dispatched,
            candidate_family_count: self.session.geometry.candidate_family_count(),
            build_nodes: self.session.keys.len(),
            coverage_checks: 0,
            pass_count: 1,
            ..WasmDistributedProgress::default()
        }
    }

    fn summary(&self) -> WasmDistributedGeometrySummary {
        WasmDistributedGeometrySummary {
            candidate_count: self.expected_count,
            candidate_digest: 0,
            candidate_family_count: self.session.geometry.candidate_family_count(),
            expanded_nodes: self.session.geometry.expanded_nodes(),
            peak_frontier: self.session.geometry.peak_frontier(),
            domain_pruned_states: self.session.geometry.domain_pruned_states(),
            hall_pruned_states: self.session.geometry.hall_pruned_states(),
            column_pruned_states: self.session.geometry.column_pruned_states(),
            component_compositions: self.session.geometry.component_compositions(),
            truncated_reason: None,
            backend_execution: WasmDistributedBackendExecution::Cpu,
        }
    }
}

impl WasmFullHeightTilingResultMerger {
    pub fn absorb_raw(&mut self, bytes: &[u8], external: u128) -> Result<(), WasmCpuSearchError> {
        self.owner.absorb_raw(bytes, external)
    }
    pub fn progress(&self) -> WasmDistributedProgress {
        self.owner.progress()
    }
    pub fn validate_external_result_memory(
        &self,
        external: u128,
        future: u128,
    ) -> Result<(), WasmCpuSearchError> {
        self.owner.ensure_memory(external, future)
    }
    pub fn validate_public_result_memory_with_future(
        &self,
        result: &CoreExecutionResult,
        future: u128,
    ) -> Result<(), &'static str> {
        self.owner
            .ensure_memory(
                result
                    .checked_resource_retained_bytes()
                    .ok_or("extended_tiling_distributed_memory_projection_unavailable")?,
                future,
            )
            .map_err(|e| e.reason())
    }
    pub fn finish(
        &mut self,
        summary: &WasmDistributedGeometrySummary,
        workers: usize,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, &'static str> {
        if control.is_cancelled() {
            return Err("wasm_cpu_search_cancelled");
        }
        if !self.owner.source_complete
            || self.owner.session.finished
            || summary.truncated_reason.is_some()
            || summary.candidate_count != self.owner.expected_count
            || summary.candidate_family_count != Some(self.owner.expected_count as u128)
            || self
                .owner
                .roots
                .iter()
                .any(|root| !root.complete || root.received != root.count)
            || self.owner.session.keys.len() != self.owner.expected_count
            || workers == 0
            || workers > self.owner.workers
        {
            return Err("extended_tiling_distributed_family_completion_mismatch");
        }
        self.owner.ensure_memory(0, 0).map_err(|e| e.reason())?;
        // Cardinality proof: every received full identity is in the original
        // exact geometric cover domain, every delegated interval is complete,
        // and the distinct-key cardinality equals the original complete DAG.
        // A duplicate cannot be silently deduplicated into a smaller family.
        self.owner.session.keys.sort_unstable();
        if self
            .owner
            .session
            .keys
            .windows(2)
            .any(|pair| pair[0] == pair[1])
        {
            return Err("extended_tiling_distributed_duplicate_solution");
        }
        self.owner
            .session
            .geometry
            .complete_parallel_enumeration(self.owner.expected_count)
            .map_err(|e| e.reason())?;
        self.owner.session.workers_used = workers;
        self.owner.session.parallel_active_workers = 0;
        let retained = self
            .owner
            .checked_retained_bytes()
            .ok_or("extended_tiling_distributed_memory_projection_unavailable")?;
        self.owner.session.parallel_peak_bytes =
            self.owner.session.parallel_peak_bytes.max(retained);
        // Preserve the helper owners during the original typed terminal
        // materialization guard. No worker-owned Core result is accepted.
        let session_bytes = self
            .owner
            .session
            .checked_retained_bytes()
            .ok_or("extended_tiling_distributed_memory_projection_unavailable")?;
        let extra = retained
            .checked_sub(session_bytes)
            .ok_or("extended_tiling_distributed_memory_projection_unavailable")?;
        self.owner.session.external_retained_bytes = self
            .owner
            .original_external
            .checked_add(extra)
            .ok_or("extended_tiling_distributed_memory_projection_unavailable")?;
        let result = match self.owner.session.finish().map_err(|e| e.reason())? {
            ExactSearchAdvance::Completed(result) => result,
            _ => return Err("extended_tiling_distributed_terminal_invalid"),
        };
        self.validate_public_result_memory_with_future(&result, 0)?;
        Ok(result)
    }
}

struct Chunk {
    root: u32,
    first: u64,
    count: u64,
    start: u64,
    complete: bool,
    keys: Vec<String>,
}

impl Chunk {
    fn encoded_len(&self) -> Option<usize> {
        let mut bytes = 8_usize.checked_add(32 + 4 + 3 * 8 + 1 + 4)?;
        for key in &self.keys {
            bytes = bytes.checked_add(4)?.checked_add(key.len())?;
        }
        Some(bytes)
    }
    fn checked_retained_bytes(&self) -> Option<u128> {
        let mut bytes = (size_of::<Self>() as u128).checked_add(
            (self.keys.capacity() as u128).checked_mul(size_of::<String>() as u128)?,
        )?;
        for key in &self.keys {
            bytes = bytes.checked_add(key.capacity() as u128)?;
        }
        Some(bytes)
    }
    fn encode(
        &self,
        problem: &SearchProblem,
        workers: usize,
    ) -> Result<Vec<u8>, WasmCpuSearchError> {
        let mut output = Vec::new();
        output
            .try_reserve_exact(self.encoded_len().ok_or_else(projection_error)?)
            .map_err(|_| storage_error())?;
        output.extend_from_slice(MAGIC);
        output.extend_from_slice(&snapshot(problem, workers)?);
        output.extend_from_slice(&self.root.to_le_bytes());
        for value in [self.first, self.count, self.start] {
            output.extend_from_slice(&value.to_le_bytes());
        }
        output.push(u8::from(self.complete));
        output.extend_from_slice(
            &u32::try_from(self.keys.len())
                .map_err(|_| projection_error())?
                .to_le_bytes(),
        );
        for key in &self.keys {
            output.extend_from_slice(
                &u32::try_from(key.len())
                    .map_err(|_| projection_error())?
                    .to_le_bytes(),
            );
            output.extend_from_slice(key.as_bytes());
        }
        Ok(output)
    }
    fn decode(
        bytes: &[u8],
        problem: &SearchProblem,
        workers: usize,
        mut guard: impl FnMut(u128) -> Result<(), WasmCpuSearchError>,
    ) -> Result<Self, WasmCpuSearchError> {
        let mut r = Reader { bytes, position: 0 };
        if r.take(8)? != MAGIC || r.array::<32>()? != snapshot(problem, workers)? {
            return Err(invalid("extended_tiling_distributed_snapshot_mismatch"));
        }
        let root = r.u32()?;
        let first = r.u64()?;
        let count = r.u64()?;
        let start = r.u64()?;
        let complete = match r.take(1)?[0] {
            0 => false,
            1 => true,
            _ => return Err(invalid("extended_tiling_distributed_flag_invalid")),
        };
        let size = r.u32()? as usize;
        if size > CHUNK_KEYS || size > bytes.len().saturating_sub(r.position) / 4 {
            return Err(invalid("extended_tiling_distributed_wire_count_invalid"));
        }
        let keys_start = r.position;
        let mut future = (size_of::<Self>() as u128) + size as u128 * size_of::<String>() as u128;
        for _ in 0..size {
            let length = r.u32()? as usize;
            let key = core::str::from_utf8(r.take(length)?)
                .map_err(|_| invalid("extended_tiling_distributed_key_encoding_invalid"))?;
            if key.is_empty() || key.len() > 128 + 60 * 67 {
                return Err(invalid("extended_tiling_distributed_key_size_invalid"));
            }
            future = future
                .checked_add(length as u128)
                .ok_or_else(projection_error)?;
        }
        if r.position != bytes.len() {
            return Err(invalid("extended_tiling_distributed_wire_trailing_bytes"));
        }
        guard(future)?; // no allocation occurs before finite borrowed projection
        r.position = keys_start;
        let mut keys = Vec::new();
        keys.try_reserve_exact(size).map_err(|_| storage_error())?;
        for _ in 0..size {
            let length = r.u32()? as usize;
            let value = core::str::from_utf8(r.take(length)?)
                .map_err(|_| invalid("extended_tiling_distributed_key_encoding_invalid"))?;
            let mut key = String::new();
            key.try_reserve_exact(length).map_err(|_| storage_error())?;
            key.push_str(value);
            keys.push(key);
        }
        let chunk = Self {
            root,
            first,
            count,
            start,
            complete,
            keys,
        };
        guard(
            chunk
                .checked_retained_bytes()
                .ok_or_else(projection_error)?,
        )?;
        Ok(chunk)
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl Reader<'_> {
    fn take(&mut self, count: usize) -> Result<&[u8], WasmCpuSearchError> {
        let end = self
            .position
            .checked_add(count)
            .ok_or_else(projection_error)?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| invalid("extended_tiling_distributed_wire_truncated"))?;
        self.position = end;
        Ok(bytes)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], WasmCpuSearchError> {
        self.take(N)?
            .try_into()
            .map_err(|_| invalid("extended_tiling_distributed_wire_truncated"))
    }
    fn u32(&mut self) -> Result<u32, WasmCpuSearchError> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    fn u64(&mut self) -> Result<u64, WasmCpuSearchError> {
        Ok(u64::from_le_bytes(self.array()?))
    }
}

fn snapshot(problem: &SearchProblem, workers: usize) -> Result<[u8; 32], WasmCpuSearchError> {
    let universe = problem
        .piece_source()
        .materialized_universe()
        .ok_or_else(|| invalid("wasm_piece_source_not_materialized"))?;
    let mut hash = Sha256::new();
    hash.update(b"clearra.pc-four-word-tiling-family.v1\0");
    hash.update(problem.problem_id().as_str().as_bytes());
    hash.update(problem.visible_height().to_le_bytes());
    for word in problem.initial_board().occupied_words() {
        hash.update(word.to_le_bytes());
    }
    let (kick, rule) = replay_profile_ids(problem);
    hash.update(kick.to_le_bytes());
    hash.update(rule.to_le_bytes());
    hash.update(universe.pattern_universe_id().get().to_le_bytes());
    hash.update(universe.pattern_weight_model_id().get().to_le_bytes());
    hash.update((workers as u64).to_le_bytes());
    hash.update(
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{}|{}",
            problem.piece_source().id(),
            problem.objective(),
            problem.count_policy(),
            problem.initial_hold(),
            problem.spawn_profile(),
            problem.supply().hold_enabled(),
            problem.supply().projects_unplaced_lookahead()
        )
        .as_bytes(),
    );
    Ok(hash.finalize().into())
}

fn row_ids(key: &str, catalog: &ExtendedInverseCatalog) -> Result<Vec<u32>, WasmCpuSearchError> {
    let identity = ExtendedTilingSolutionKey::parse_canonical(key)
        .map_err(|_| invalid("extended_tiling_distributed_key_encoding_invalid"))?;
    if identity.height() != catalog.height()
        || identity.initial_board().words() != catalog.initial_board().words()
    {
        return Err(invalid("extended_tiling_distributed_key_snapshot_mismatch"));
    }
    let mut rows = Vec::new();
    rows.try_reserve_exact(identity.placement_count())
        .map_err(|_| storage_error())?;
    for placement in identity.placements() {
        let cells = ExtendedBoard::from_words(placement.cells().words());
        let mut found = catalog
            .skeletons()
            .iter()
            .enumerate()
            .filter(|(_, row)| row.piece == placement.piece() && row.cells == cells);
        let (index, _) = found
            .next()
            .ok_or_else(|| invalid("extended_tiling_distributed_key_not_in_catalog"))?;
        if found.next().is_some() {
            return Err(invalid("extended_tiling_distributed_catalog_collision"));
        }
        rows.push(u32::try_from(index).map_err(|_| projection_error())?);
    }
    Ok(rows)
}

fn plan_retained_bytes(plan: &ExtendedParallelGeometryPlan) -> u128 {
    size_of::<ExtendedParallelGeometryPlan>() as u128
        + plan.shared_retained_bytes() as u128
        + plan.branch_retained_bytes() as u128
}
fn validate_request(problem: &SearchProblem, workers: usize) -> Result<(), WasmCpuSearchError> {
    if workers < 2 || !WasmFullHeightTilingProducer::supports(problem) {
        return Err(invalid("extended_tiling_distributed_contract_invalid"));
    }
    if problem.backend_request().max_nodes() != 0 {
        // Finite aggregate node credit cannot be cloned across browser realms.
        return Err(invalid(
            "extended_tiling_distributed_node_credit_not_connected",
        ));
    }
    Ok(())
}
fn replica_cap(cap: u128, external: u128, workers: usize) -> Result<u128, WasmCpuSearchError> {
    cap.checked_sub(external)
        .and_then(|n| n.checked_div(workers.checked_add(2)? as u128))
        .filter(|n| *n != 0)
        .ok_or_else(projection_error)
}
fn invalid(reason: &'static str) -> WasmCpuSearchError {
    WasmCpuSearchError::InvalidProblem { reason }
}
fn projection_error() -> WasmCpuSearchError {
    invalid("extended_tiling_distributed_memory_projection_unavailable")
}
fn storage_error() -> WasmCpuSearchError {
    invalid("extended_tiling_distributed_storage_unavailable")
}
fn typed_error(error: WasmExactSearchError) -> WasmCpuSearchError {
    match error {
        WasmExactSearchError::ResourceAdmission(report) => {
            WasmCpuSearchError::resource_admission(*report)
        }
        WasmExactSearchError::Cancelled => invalid("wasm_cpu_search_cancelled"),
        WasmExactSearchError::InvalidProblem(reason) => invalid(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::{
        board::standard_pc_board::Board256Mask, piece::piece_kind::PieceKind,
    };
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::{
        PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
        PieceWindow, RequestedSearchBackend,
    };
    use clearra_problem::ProblemCompiler;
    use clearra_rules::profile::{
        builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x},
        rule_profile::RuleProfile,
    };
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    fn forced_problem(height: u8, rule: RuleProfile) -> Arc<SearchProblem> {
        let starts = if height == 7 {
            vec![0, 3]
        } else {
            (0..u16::from(height)).step_by(4).collect()
        };
        let count = starts.len();
        let mut required = Board256Mask::EMPTY;
        for (column, start) in starts.into_iter().enumerate() {
            for row in start..start + 4 {
                required =
                    required.union(Board256Mask::singleton(row * 10 + column as u16).unwrap());
            }
        }
        let initial = Board256Mask::all_cells(u16::from(height) * 10)
            .unwrap()
            .without(required);
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10_from_words(u16::from(height), initial.words()).unwrap(),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; count])),
            PieceWindow::new(count),
        )
        .with_rule(rule)
        .with_exact_pieces(Some(count))
        .with_allow_hold(false)
        .with_min_remaining_queue(0)
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_objective(ObjectivePolicy::tiling())
        .with_execution_policy(
            PcExecutionPolicy::default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_workers(1)
                .with_allow_backend_fallback(false),
        );
        Arc::new(
            ProblemCompiler::compile_scenario_pc(&query)
                .unwrap()
                .with_output_policy(SearchOutputPolicy::TilingOnly),
        )
    }

    fn delegated(
        problem: &Arc<SearchProblem>,
        workers: usize,
        authority: &WasmCpuTerminalResourceAuthority,
    ) -> (
        WasmFullHeightTilingResultMerger,
        WasmDistributedGeometrySummary,
        Vec<Vec<u8>>,
    ) {
        let control = ExecutionControl::default();
        let mut producer = WasmFullHeightTilingProducer::new_shared_under_authority(
            Arc::clone(problem),
            workers,
            0,
            authority,
        )
        .unwrap();
        let mut packets = Vec::new();
        let summary = loop {
            match producer
                .advance_with_external_retained(&control, 0)
                .unwrap()
            {
                WasmCandidateProducerAdvance::Pending => {}
                WasmCandidateProducerAdvance::Candidate(packet) => {
                    assert!(packet.row_ids().is_empty());
                    assert_eq!(packet.ordinal(), u64::from(packet.target_index()));
                    packets.push(packet);
                }
                WasmCandidateProducerAdvance::Completed(summary) => break summary,
                WasmCandidateProducerAdvance::Cancelled => panic!("unexpected cancellation"),
            }
        };
        let mut wires = Vec::new();
        // Source-only wire fixtures delegate one admitted physical slot at a
        // time; the real WASM pool smoke owns actual concurrent browser realms.
        for index in 0..workers {
            let mut worker = producer.new_delegated_worker().unwrap();
            for packet in packets
                .iter()
                .filter(|packet| packet.ordinal() as usize % workers == index)
            {
                loop {
                    let retained = wires
                        .iter()
                        .map(|wire: &Vec<u8>| wire.capacity() as u128)
                        .sum::<u128>()
                        + packet.checked_nested_retained_bytes().unwrap();
                    let (chunk, complete, _) = worker
                        .advance_root_with_external_retained(packet, &control, retained)
                        .unwrap();
                    if let Some(chunk) = chunk {
                        wires.push(chunk);
                    }
                    if complete {
                        break;
                    }
                }
            }
            assert!(worker.finish_raw().unwrap().is_empty());
        }
        (producer.into_merger().unwrap(), summary, wires)
    }

    #[test]
    fn delegated_tiling_four_word_family_matches_scalar_in_all_profiles_and_heights() {
        let authority = WasmCpuTerminalResourceAuthority::try_acquire_full_capacity().unwrap();
        for rule in [srs(), srs_plus(), srs_x(), jstris_180(), no_kick()] {
            for height in [7, 8, 12, 24] {
                let problem = forced_problem(height, rule.clone());
                let reference = {
                    let mut scalar = ExtendedPcTilingSession::new_under_authority(
                        Arc::clone(&problem),
                        0,
                        &authority,
                    )
                    .unwrap();
                    loop {
                        match scalar.advance(128, &ExecutionControl::default()).unwrap() {
                            ExactSearchAdvance::Completed(result) => break result,
                            ExactSearchAdvance::Pending => {}
                            ExactSearchAdvance::Cancelled => {
                                panic!("unexpected scalar cancellation")
                            }
                        }
                    }
                };
                let (mut merger, summary, wires) = delegated(&problem, 3, &authority);
                let retained = wires.iter().map(|wire| wire.capacity() as u128).sum();
                for wire in &wires {
                    assert_eq!(&wire[..8], MAGIC);
                    merger.absorb_raw(wire, retained).unwrap();
                }
                let result = merger
                    .finish(&summary, 3, &ExecutionControl::default())
                    .unwrap();
                assert!(result.pc_tiling_family_publication_contract_is_valid());
                assert!(result.pc_chance_coverage_evidence().is_none());
                assert_eq!(
                    result.normalized_solution_keys(),
                    reference.normalized_solution_keys()
                );
                assert_eq!(
                    result.field("normalized_solution_set_hash"),
                    reference.field("normalized_solution_set_hash")
                );
                assert_eq!(
                    result.field("unique_solution_count"),
                    reference.field("unique_solution_count")
                );
                assert_eq!(result.field("workers_used"), Some("3"));
                assert!(result.normalized_solution_keys().iter().all(|key| {
                    let parsed = ExtendedTilingSolutionKey::parse_canonical(key).unwrap();
                    parsed.height() == height
                        && parsed.initial_board().words()
                            == problem.initial_board().occupied_words()
                }));
            }
        }
    }

    #[test]
    fn raw_tiling_rejects_snapshot_duplicate_incomplete_and_cancelled_authority() {
        let authority = WasmCpuTerminalResourceAuthority::try_acquire_full_capacity().unwrap();
        let problem = forced_problem(8, srs_plus());
        let (mut merger, summary, wires) = delegated(&problem, 3, &authority);
        assert_eq!(
            merger
                .finish(&summary, 3, &ExecutionControl::default())
                .unwrap_err(),
            "extended_tiling_distributed_family_completion_mismatch"
        );
        let retained = wires
            .iter()
            .map(|wire| wire.capacity() as u128)
            .sum::<u128>();
        let mut wrong = wires[0].clone();
        wrong[8] ^= 1;
        assert_eq!(
            merger
                .absorb_raw(&wrong, retained + wrong.capacity() as u128)
                .unwrap_err()
                .reason(),
            "extended_tiling_distributed_snapshot_mismatch"
        );
        let mut wrong = Chunk::decode(&wires[0], &problem, 3, |_| Ok(())).unwrap();
        wrong.start += 1;
        let wrong = wrong.encode(&problem, 3).unwrap();
        assert_eq!(
            merger
                .absorb_raw(&wrong, retained + wrong.capacity() as u128)
                .unwrap_err()
                .reason(),
            "extended_tiling_distributed_root_receipt_mismatch"
        );
        for wire in &wires {
            merger.absorb_raw(wire, retained).unwrap();
        }
        assert_eq!(
            merger.absorb_raw(&wires[0], retained).unwrap_err().reason(),
            "extended_tiling_distributed_root_receipt_mismatch"
        );
        let control = ExecutionControl::default();
        control.cancellation.handle().cancel();
        assert_eq!(
            merger.finish(&summary, 3, &control).unwrap_err(),
            "wasm_cpu_search_cancelled"
        );
    }

    #[test]
    fn raw_tiling_borrowed_decode_guards_allocation_and_refuses_malformed_keys() {
        let authority = WasmCpuTerminalResourceAuthority::try_acquire_full_capacity().unwrap();
        let problem = forced_problem(8, srs_plus());
        let (mut merger, _, wires) = delegated(&problem, 2, &authority);
        let raw = &wires[0];
        let mut called = false;
        let error = Chunk::decode(raw, &problem, 2, |_| {
            called = true;
            Err(invalid("test-memory-denied"))
        })
        .err()
        .unwrap();
        assert!(called);
        assert_eq!(error.reason(), "test-memory-denied");
        let mut trailing = raw.clone();
        trailing.push(0);
        assert_eq!(
            Chunk::decode(&trailing, &problem, 2, |_| Ok(()))
                .err()
                .unwrap()
                .reason(),
            "extended_tiling_distributed_wire_trailing_bytes"
        );
        let mut chunk = Chunk::decode(raw, &problem, 2, |_| Ok(())).unwrap();
        assert!(!chunk.keys.is_empty());
        chunk.keys[0] = "ctk2|invalid".to_owned();
        let invalid_key = chunk.encode(&problem, 2).unwrap();
        assert_eq!(
            merger
                .absorb_raw(&invalid_key, invalid_key.capacity() as u128)
                .unwrap_err()
                .reason(),
            "extended_tiling_distributed_key_encoding_invalid"
        );
        assert!(
            merger.absorb_raw(&[], 0).is_err(),
            "empty worker finish is not a proof payload"
        );
    }
}
