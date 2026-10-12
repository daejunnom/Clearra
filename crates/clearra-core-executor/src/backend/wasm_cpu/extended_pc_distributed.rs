//! Browser distribution of the *PC* four-word family. The existing extended
//! ILC packets and BuildUp evaluator are reused without granting Build authority
//! to PC reducers. Only the complete coordinator can issue product evidence.

#[path = "extended_pc_distributed_wire.rs"]
mod wire;

use std::mem::size_of;
use std::sync::Arc;

use clearra_core_domain::board::standard_pc_board::Board256Mask;
use sha2::{Digest, Sha256};

use crate::{
    pc_failed_queue_evidence::{
        PcFailedQueueEvidenceProducer, PcFailedQueueExecutionAuthority,
        PcFailedQueueProducerAdmission, PcFailedQueueSourceCompleteness,
    },
    resource::{
        admit_budget_bound_search_execution,
        admit_budget_bound_search_execution_under_terminal_authority, ExecutionAdmission,
    },
    PcFailedQueueEvidence, PcFailedQueueEvidenceError, WasmCpuSearchError,
    WasmCpuTerminalResourceAuthority,
};

use super::super::extended_geometry::ExtendedGeometrySearch;
use super::{
    extended_row_ids_from_canonical_key, BuildProbabilityAdvance, BuildProbabilityField,
    CoreExecutionResult, ExecutionControl, ExtendedBuildProbabilitySession, ExtendedFamilyPurpose,
    ExtendedTilingKey, HashSet, PatternBitSet, SearchProblem, WasmCandidatePacket,
    WasmCandidateProducerAdvance, WasmDistributedGeometrySummary, WasmDistributedProgress,
    WasmExactSearchError,
};

/// One ILC producer, not one full search replicated in every browser worker.
pub struct WasmFullHeightPcProducer {
    engine: ExtendedBuildProbabilitySession,
    verification_geometry: ExtendedGeometrySearch,
    admission: ExecutionAdmission,
    candidates: Vec<[u8; 32]>,
    compute_workers: usize,
    replica_cap: u128,
    original_coexisting: u128,
    finished: bool,
}

/// A scalar exact verifier owns private BuildUp/language scratch. Its input
/// remains the original PC problem; worker identity does not redefine a query.
pub struct WasmFullHeightPcVerifier {
    engine: ExtendedBuildProbabilitySession,
    _admission: ExecutionAdmission,
    receipts: Vec<(u64, [u8; 32])>,
    finished: bool,
}

/// Raw partials contain no attachable minimum/score/replay authority. Keeping
/// the producing geometry owner here binds every returned ordinal and candidate
/// to the exact original family before the ordinary PC finalizer runs.
pub struct WasmFullHeightPcResultMerger {
    engine: ExtendedBuildProbabilitySession,
    verification_geometry: ExtendedGeometrySearch,
    _admission: ExecutionAdmission,
    candidates: Vec<[u8; 32]>,
    observed: Vec<u64>,
    received_workers: usize,
    compute_workers: usize,
    original_coexisting: u128,
    failed_queue_evidence: Option<PcFailedQueueEvidence>,
}

impl WasmFullHeightPcProducer {
    pub fn supports(problem: &SearchProblem) -> bool {
        (7..=24).contains(&problem.visible_height())
            && super::super::extended_pc_search::validate_pc_family_problem(problem).is_ok()
    }

    pub fn new(
        problem: &SearchProblem,
        compute_workers: usize,
    ) -> Result<Self, WasmCpuSearchError> {
        let admission = admit_budget_bound_search_execution(problem, 1)
            .map_err(WasmCpuSearchError::resource_admission)?;
        Self::with_admission(problem, compute_workers, admission, 0)
    }

    /// The App's original compiled Arc remains the failed-queue execution
    /// token. A structurally equal reconstructed problem cannot impersonate it.
    pub fn new_shared(
        problem: Arc<SearchProblem>,
        compute_workers: usize,
    ) -> Result<Self, WasmCpuSearchError> {
        let mut producer = Self::new(&problem, compute_workers)?;
        producer.engine.problem = problem;
        Ok(producer)
    }

    pub fn new_under_authority(
        problem: &SearchProblem,
        compute_workers: usize,
        external_retained_bytes: u128,
        authority: &WasmCpuTerminalResourceAuthority,
    ) -> Result<Self, WasmCpuSearchError> {
        let admission = admit_budget_bound_search_execution_under_terminal_authority(
            problem,
            external_retained_bytes,
            authority,
            1,
        )
        .map_err(WasmCpuSearchError::resource_admission)?;
        Self::with_admission(problem, compute_workers, admission, external_retained_bytes)
    }

    fn with_admission(
        problem: &SearchProblem,
        compute_workers: usize,
        admission: ExecutionAdmission,
        original_coexisting: u128,
    ) -> Result<Self, WasmCpuSearchError> {
        if compute_workers < 2 || !Self::supports(problem) {
            return Err(invalid("extended_pc_distributed_contract_invalid"));
        }
        // An independent realm cannot borrow an Arc node counter. Explicit
        // finite-node requests remain fail-closed until host credit delegation
        // is provided; each worker must never receive the full global budget.
        if problem.backend_request().max_nodes() != 0 {
            return Err(invalid("extended_pc_distributed_node_credit_not_connected"));
        }
        let replica_cap = replica_cap(
            admission.memory_cap_bytes(),
            original_coexisting,
            compute_workers,
        )?;
        let bound = admission
            .memory_bound()
            .with_cap(replica_cap)
            .map_err(WasmCpuSearchError::resource_admission)?;
        let engine = ExtendedBuildProbabilitySession::new_pc_family(
            problem,
            pc_field(problem)?,
            bound,
            original_coexisting,
        )
        .map_err(typed_error)?;
        let future = engine
            .geometry
            .checked_external_verifier_future_bytes()
            .and_then(|n| n.checked_add(size_of::<ExtendedGeometrySearch>() as u128))
            .ok_or_else(projection_error)?;
        engine.ensure_memory_bound(future).map_err(typed_error)?;
        // Enumeration releases its final traversal target owner on completion.
        // Keep only the immutable verifier indexes, not its compiler/DAG copy.
        let verification_geometry = engine
            .geometry
            .external_verifier_from_owner()
            .ok_or_else(|| invalid("extended_pc_distributed_verifier_index_unavailable"))?;
        let producer = Self {
            engine,
            verification_geometry,
            admission,
            candidates: Vec::new(),
            compute_workers,
            replica_cap,
            original_coexisting,
            finished: false,
        };
        producer.validate_external_result_memory(0)?;
        Ok(producer)
    }

    pub fn new_delegated_verifier(&self) -> Result<WasmFullHeightPcVerifier, WasmCpuSearchError> {
        let admission = self
            .admission
            .try_delegate_compute_only_with_memory_cap(self.replica_cap)
            .map_err(WasmCpuSearchError::resource_admission)?;
        let bound = admission
            .memory_bound()
            .with_cap(self.replica_cap)
            .map_err(WasmCpuSearchError::resource_admission)?;
        let mut engine = self
            .engine
            .new_shared_pc_verifier_with_bound(bound)
            .map_err(typed_error)?;
        let future = self
            .verification_geometry
            .checked_external_verifier_future_bytes()
            .ok_or_else(|| invalid("extended_pc_distributed_verifier_index_unavailable"))?;
        engine.ensure_memory_bound(future).map_err(typed_error)?;
        engine.geometry = self
            .verification_geometry
            .external_verifier_from_owner()
            .ok_or_else(|| invalid("extended_pc_distributed_verifier_index_unavailable"))?;
        engine.ensure_memory_bound(0).map_err(typed_error)?;
        Ok(WasmFullHeightPcVerifier {
            engine,
            _admission: admission,
            receipts: Vec::new(),
            finished: false,
        })
    }

    pub fn advance_with_external_retained(
        &mut self,
        control: &ExecutionControl,
        external_retained_bytes: u128,
    ) -> Result<WasmCandidateProducerAdvance, WasmCpuSearchError> {
        if self.finished {
            return Err(invalid("extended_pc_distributed_producer_finished"));
        }
        let receipt_bytes = receipt_backing(&self.candidates).ok_or_else(projection_error)?;
        self.engine.coexisting_retained_bytes = self
            .original_coexisting
            .checked_add(self.verification_geometry.retained_bytes() as u128)
            .and_then(|n| n.checked_add(size_of::<ExtendedGeometrySearch>() as u128))
            .and_then(|n| n.checked_add(receipt_bytes))
            .and_then(|n| n.checked_add(external_retained_bytes))
            .ok_or_else(projection_error)?;
        let advance = self
            .engine
            .advance_distributed_geometry(0, control)
            .map_err(typed_error)?;
        match &advance {
            WasmCandidateProducerAdvance::Candidate(packet) => {
                if packet.ordinal() != self.candidates.len() as u64 || !packet.is_extended() {
                    return Err(invalid("extended_pc_distributed_producer_ordinal_mismatch"));
                }
                self.engine
                    .ensure_memory_bound(
                        ((self.candidates.len() + 1) as u128)
                            .checked_mul((2 * size_of::<[u8; 32]>()) as u128)
                            .ok_or_else(projection_error)?,
                    )
                    .map_err(typed_error)?;
                self.candidates
                    .try_reserve(1)
                    .map_err(|_| storage_error())?;
                self.candidates
                    .push(candidate_fingerprint(packet.row_ids())?);
            }
            WasmCandidateProducerAdvance::Completed(summary) => {
                if summary.truncated_reason.is_some()
                    || summary.candidate_count != self.candidates.len()
                {
                    return Err(invalid(
                        summary
                            .truncated_reason
                            .unwrap_or("extended_pc_distributed_family_incomplete"),
                    ));
                }
                self.finished = true;
            }
            _ => {}
        }
        self.validate_external_result_memory(external_retained_bytes)?;
        Ok(advance)
    }

    pub fn progress(&self) -> WasmDistributedProgress {
        self.engine.distributed_progress()
    }

    pub fn validate_external_result_memory(
        &self,
        external: u128,
    ) -> Result<(), WasmCpuSearchError> {
        let owned = self
            .engine
            .checked_retained_bytes()
            .and_then(|n| n.checked_add(self.verification_geometry.retained_bytes() as u128))
            .and_then(|n| n.checked_add(size_of::<ExtendedGeometrySearch>() as u128))
            .and_then(|n| n.checked_add(receipt_backing(&self.candidates)?))
            .and_then(|n| n.checked_add(self.original_coexisting))
            .ok_or_else(projection_error)?;
        self.engine
            .memory_bound
            .ensure(owned, external)
            .map_err(WasmCpuSearchError::resource_admission)
    }

    pub fn into_merger(self) -> Result<WasmFullHeightPcResultMerger, &'static str> {
        self.into_merger_with_external_retained(0)
    }

    pub fn into_merger_with_external_retained(
        mut self,
        external: u128,
    ) -> Result<WasmFullHeightPcResultMerger, &'static str> {
        if !self.finished {
            return Err("extended_pc_distributed_family_not_complete");
        }
        // N verifiers may still retain their final wire until host cleanup. The
        // two reserved coordinator shares cover result decode and key union;
        // no realm independently acquires the request's whole memory cap.
        let merge_cap = self
            .replica_cap
            .checked_mul(2)
            .ok_or("extended_pc_distributed_memory_projection_overflow")?;
        self.engine.memory_bound = self
            .admission
            .memory_bound()
            .with_cap(merge_cap)
            .map_err(|_| "extended_pc_distributed_memory_capacity_unavailable")?;
        self.engine.prepare_distributed_finalizer();
        self.engine.coexisting_retained_bytes = self
            .original_coexisting
            .checked_add(self.verification_geometry.retained_bytes() as u128)
            .and_then(|n| n.checked_add(size_of::<ExtendedGeometrySearch>() as u128))
            .ok_or("extended_pc_distributed_memory_projection_overflow")?;
        let words = self.candidates.len().div_ceil(64);
        self.validate_external_result_memory(
            external
                .checked_add((words as u128) * size_of::<u64>() as u128)
                .ok_or("extended_pc_distributed_memory_projection_overflow")?,
        )
        .map_err(|e| e.reason())?;
        let mut observed = Vec::new();
        observed
            .try_reserve_exact(words)
            .map_err(|_| "extended_pc_distributed_receipt_storage_unavailable")?;
        observed.resize(words, 0);
        Ok(WasmFullHeightPcResultMerger {
            engine: self.engine,
            verification_geometry: self.verification_geometry,
            _admission: self.admission,
            candidates: self.candidates,
            observed,
            received_workers: 0,
            compute_workers: self.compute_workers,
            original_coexisting: self.original_coexisting,
            failed_queue_evidence: None,
        })
    }
}

impl WasmFullHeightPcVerifier {
    pub fn new(
        problem: &SearchProblem,
        compute_workers: usize,
    ) -> Result<Self, WasmCpuSearchError> {
        if !WasmFullHeightPcProducer::supports(problem) || compute_workers < 2 {
            return Err(invalid("extended_pc_distributed_verifier_contract_invalid"));
        }
        if problem.backend_request().max_nodes() != 0 {
            return Err(invalid("extended_pc_distributed_node_credit_not_connected"));
        }
        let admission = admit_budget_bound_search_execution(problem, 1)
            .map_err(WasmCpuSearchError::resource_admission)?;
        let cap = replica_cap(admission.memory_cap_bytes(), 0, compute_workers)?;
        Self::with_admission(problem, admission, cap)
    }

    fn with_admission(
        problem: &SearchProblem,
        admission: ExecutionAdmission,
        cap: u128,
    ) -> Result<Self, WasmCpuSearchError> {
        let bound = admission
            .memory_bound()
            .with_cap(cap)
            .map_err(WasmCpuSearchError::resource_admission)?;
        let mut engine =
            ExtendedBuildProbabilitySession::new_pc_family(problem, pc_field(problem)?, bound, 0)
                .map_err(typed_error)?;
        if !engine.geometry.prepare_external() {
            return Err(invalid(
                "extended_pc_distributed_verifier_index_unavailable",
            ));
        }
        engine.external_geometry = true;
        Ok(Self {
            engine,
            _admission: admission,
            receipts: Vec::new(),
            finished: false,
        })
    }

    pub fn consume_with_external_retained(
        &mut self,
        packet: &WasmCandidatePacket,
        control: &ExecutionControl,
        external: u128,
    ) -> Result<(), WasmCpuSearchError> {
        if self.finished || packet.pass_index() != 0 || !packet.is_extended() {
            return Err(invalid("extended_pc_distributed_verifier_packet_invalid"));
        }
        self.validate_external_result_memory(
            external,
            ((self.receipts.len() + 1) as u128) * (2 * size_of::<(u64, [u8; 32])>()) as u128,
        )?;
        self.receipts.try_reserve(1).map_err(|_| storage_error())?;
        self.engine.coexisting_retained_bytes = external
            .checked_add(receipt_backing(&self.receipts).ok_or_else(projection_error)?)
            .ok_or_else(projection_error)?;
        self.engine
            .process_external_candidate(packet, control)
            .map_err(typed_error)?;
        if let Some(reason) = self.engine.truncated_reason {
            return Err(invalid(reason));
        }
        self.receipts
            .push((packet.ordinal(), candidate_fingerprint(packet.row_ids())?));
        self.validate_external_result_memory(external, 0)
    }

    pub fn progress(&self) -> WasmDistributedProgress {
        self.engine.distributed_progress()
    }

    pub fn validate_external_result_memory(
        &self,
        external: u128,
        future: u128,
    ) -> Result<(), WasmCpuSearchError> {
        let owned = self
            .engine
            .checked_retained_bytes()
            .and_then(|n| n.checked_add(receipt_backing(&self.receipts)?))
            .and_then(|n| n.checked_add(external))
            .ok_or_else(projection_error)?;
        self.engine
            .memory_bound
            .ensure(owned, future)
            .map_err(WasmCpuSearchError::resource_admission)
    }

    pub fn finish_raw(&mut self) -> Result<Vec<u8>, WasmCpuSearchError> {
        if self.finished
            || self.receipts.len() != self.engine.processed_candidate_count
            || self.engine.truncated_reason.is_some()
        {
            return Err(invalid("extended_pc_distributed_verifier_finish_invalid"));
        }
        self.receipts.sort_unstable_by_key(|receipt| receipt.0);
        if self.receipts.windows(2).any(|w| w[0].0 >= w[1].0) {
            return Err(invalid("extended_pc_distributed_duplicate_ordinal"));
        }
        let output = wire::encode(&self.engine, &self.receipts, |future| {
            self.validate_external_result_memory(0, future)
        })?;
        self.finished = true;
        Ok(output)
    }
}

impl WasmFullHeightPcResultMerger {
    pub fn absorb_raw(&mut self, input: &[u8], external: u128) -> Result<(), WasmCpuSearchError> {
        if self.engine.finished
            || external < input.len() as u128
            || self.received_workers >= self.compute_workers
        {
            return Err(invalid("extended_pc_distributed_merger_state_invalid"));
        }
        let partial = wire::decode(input, &self.engine.problem, |future| {
            self.validate_external_result_memory(external, future)
        })?;
        let partial_bytes = partial
            .checked_retained_bytes()
            .ok_or_else(projection_error)?;
        self.validate_external_result_memory(
            external
                .checked_add(partial_bytes)
                .ok_or_else(projection_error)?,
            partial.merge_future_bytes().ok_or_else(projection_error)?,
        )?;
        let mut allowed = HashSet::new();
        allowed
            .try_reserve(partial.receipts.len())
            .map_err(|_| storage_error())?;
        let mut ordinals = HashSet::new();
        ordinals
            .try_reserve(partial.receipts.len())
            .map_err(|_| storage_error())?;
        for (ordinal, fingerprint) in &partial.receipts {
            let index = usize::try_from(*ordinal).map_err(|_| projection_error())?;
            if !ordinals.insert(*ordinal)
                || self.candidates.get(index) != Some(fingerprint)
                || self
                    .observed
                    .get(index / 64)
                    .is_none_or(|word| *word & (1_u64 << (index & 63)) != 0)
            {
                return Err(invalid("extended_pc_distributed_receipt_mismatch"));
            }
            allowed.insert(*fingerprint);
        }
        let mut accepted = Vec::new();
        accepted
            .try_reserve_exact(partial.rows.len())
            .map_err(|_| storage_error())?;
        for row in &partial.rows {
            let ids = extended_row_ids_from_canonical_key(
                &row.key,
                self.engine.field.height(),
                &self.engine.catalog,
            )
            .map_err(typed_error)?;
            if !allowed.contains(&candidate_fingerprint(&ids)?) {
                return Err(invalid("extended_pc_distributed_solution_not_delegated"));
            }
            let candidate = self
                .verification_geometry
                .external_candidate(&self.engine.catalog, &ids)
                .ok_or_else(|| invalid("extended_pc_distributed_solution_not_in_family"))?;
            let tiling = ExtendedTilingKey::from_candidate(&self.engine.catalog, &candidate);
            if tiling.canonical_key(
                self.engine.catalog.initial_board(),
                self.engine.field.height(),
            ) != row.key
            {
                return Err(invalid(
                    "extended_pc_distributed_canonical_solution_mismatch",
                ));
            }
            accepted.push(tiling);
        }
        partial.validate_union(&self.engine)?;
        // All untrusted schema, family, coverage and physical graph checks precede
        // any mutation. The caller invalidates this job on every merge error.
        self.engine
            .covered_patterns
            .union_with(&partial.covered)
            .map_err(|_| invalid("extended_pc_distributed_coverage_mismatch"))?;
        self.engine
            .buildable_tilings
            .try_reserve(accepted.len())
            .map_err(|_| storage_error())?;
        self.engine.buildable_tilings.extend(accepted);
        if self.engine.pc_observation.is_some() {
            // Worker-local observation trees cannot be OR'ed. The original
            // owner re-materializes one global language from this exact family.
            self.engine
                .distributed_solution_keys
                .try_reserve(partial.rows.len())
                .map_err(|_| storage_error())?;
            for row in &partial.rows {
                self.engine
                    .distributed_solution_keys
                    .insert(row.key.clone());
            }
        }
        if let Some(target) = self.engine.solution_coverage.as_mut() {
            target
                .try_reserve(partial.rows.len())
                .map_err(|_| storage_error())?;
            for row in &partial.rows {
                let bits = row
                    .coverage
                    .as_ref()
                    .ok_or_else(|| invalid("extended_pc_distributed_coverage_row_missing"))?;
                if let Some(existing) = target.get_mut(&row.key) {
                    existing
                        .union_with(bits)
                        .map_err(|_| invalid("extended_pc_distributed_coverage_mismatch"))?;
                } else {
                    target.insert(row.key.clone(), bits.clone());
                }
            }
        }
        self.engine
            .spin_execution_graphs
            .try_reserve_exact(partial.graphs.len())
            .map_err(|_| storage_error())?;
        for (ordinal, _) in &partial.receipts {
            let index = *ordinal as usize;
            self.observed[index / 64] |= 1_u64 << (index & 63);
        }
        self.engine.processed_candidate_count = self
            .engine
            .processed_candidate_count
            .checked_add(partial.receipts.len())
            .ok_or_else(projection_error)?;
        let candidates = partial.receipts.len();
        self.engine.parallel_active_workers += usize::from(candidates != 0);
        self.engine.parallel_minimum_worker_candidates = self
            .engine
            .parallel_minimum_worker_candidates
            .min(candidates);
        self.engine.parallel_maximum_worker_candidates = self
            .engine
            .parallel_maximum_worker_candidates
            .max(candidates);
        partial.apply(&mut self.engine)?;
        self.received_workers += 1;
        self.validate_external_result_memory(external, 0)
    }

    pub fn progress(&self) -> WasmDistributedProgress {
        self.engine.distributed_progress()
    }

    pub fn validate_external_result_memory(
        &self,
        external: u128,
        future: u128,
    ) -> Result<(), WasmCpuSearchError> {
        let owned = self
            .engine
            .checked_retained_bytes()
            .and_then(|n| n.checked_add(self.verification_geometry.retained_bytes() as u128))
            .and_then(|n| n.checked_add(size_of::<ExtendedGeometrySearch>() as u128))
            .and_then(|n| n.checked_add(receipt_backing(&self.candidates)?))
            .and_then(|n| n.checked_add(receipt_backing(&self.observed)?))
            .and_then(|n| {
                n.checked_add(
                    self.failed_queue_evidence
                        .as_ref()
                        .map_or(0, |e| e.memory_report().retained_producer_bytes()),
                )
            })
            .and_then(|n| n.checked_add(self.original_coexisting))
            .and_then(|n| n.checked_add(external))
            .ok_or_else(projection_error)?;
        self.engine
            .memory_bound
            .ensure(owned, future)
            .map_err(WasmCpuSearchError::resource_admission)
    }

    pub fn finish(
        &mut self,
        summary: &WasmDistributedGeometrySummary,
        workers_used: usize,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, &'static str> {
        if control.is_cancelled() {
            return Err("wasm_cpu_search_cancelled");
        }
        if self.engine.finished
            || summary.truncated_reason.is_some()
            || summary.candidate_count != self.candidates.len()
            || self
                .observed
                .iter()
                .map(|word| word.count_ones() as usize)
                .sum::<usize>()
                != self.candidates.len()
            || self.engine.processed_candidate_count != self.candidates.len()
            || self.received_workers == 0
            || workers_used == 0
            || workers_used > self.compute_workers
        {
            return Err("extended_pc_distributed_family_completion_mismatch");
        }
        self.engine.workers_used = workers_used;
        self.engine.coexisting_retained_bytes = self
            .original_coexisting
            .checked_add(self.verification_geometry.retained_bytes() as u128)
            .and_then(|n| n.checked_add(size_of::<ExtendedGeometrySearch>() as u128))
            .and_then(|n| n.checked_add(receipt_backing(&self.candidates)?))
            .and_then(|n| n.checked_add(receipt_backing(&self.observed)?))
            .ok_or("extended_pc_distributed_memory_projection_overflow")?;
        self.validate_external_result_memory(0, 0)
            .map_err(|e| e.reason())?;
        self.engine
            .rebuild_distributed_observation_language(&self.verification_geometry, control)
            .map_err(|e| e.reason())?;
        self.engine
            .finalize_pc_observation(control)
            .map_err(|e| e.reason())?;
        match self
            .engine
            .complete_distributed_geometry(summary, workers_used)
            .map_err(|e| e.reason())?
        {
            BuildProbabilityAdvance::Completed(result) => {
                if let Some(limit) = self
                    .engine
                    .problem
                    .pc_chance_evidence_policy()
                    .pc_failed_queue_example_limit()
                {
                    let source = result
                        .pc_chance_coverage_evidence()
                        .ok_or("extended_pc_failed_queue_source_evidence_missing")?;
                    if !source.complete()
                        || !source
                            .problem()
                            .matches_search_problem(&self.engine.problem)
                        || source.coverage_union().words() != result.coverage_pattern_words()
                    {
                        return Err("extended_pc_failed_queue_source_evidence_mismatch");
                    }
                    let authority =
                        PcFailedQueueExecutionAuthority::new(Arc::clone(&self.engine.problem));
                    let observed = self
                        .engine
                        .checked_retained_bytes_with_coexisting_owners()
                        .and_then(|n| n.checked_add(receipt_backing(&self.candidates)?))
                        .and_then(|n| n.checked_add(receipt_backing(&self.observed)?))
                        .and_then(|n| n.checked_add(result.checked_resource_retained_bytes()?))
                        .and_then(|n| n.checked_add((3 * size_of::<usize>()) as u128))
                        .ok_or("extended_pc_failed_queue_memory_projection_unavailable")?;
                    let evidence = PcFailedQueueEvidenceProducer::produce(
                        authority,
                        source.rows(),
                        limit,
                        PcFailedQueueSourceCompleteness::new(true, true, true, true),
                        PcFailedQueueProducerAdmission::new(self.engine.memory_bound, observed),
                    )
                    .map_err(|error| match error {
                        PcFailedQueueEvidenceError::MemoryAdmission(_) => {
                            "extended_pc_failed_queue_memory_capacity_unavailable"
                        }
                        _ => "extended_pc_failed_queue_evidence_invalid",
                    })?;
                    self.failed_queue_evidence = Some(evidence);
                    return Ok(result.without_pc_chance_transient_evidence());
                }
                Ok(result)
            }
            _ => Err("extended_pc_distributed_terminal_incomplete"),
        }
    }

    pub fn take_failed_queue_evidence(&mut self) -> Option<PcFailedQueueEvidence> {
        self.failed_queue_evidence.take()
    }

    pub fn validate_public_result_memory_with_future(
        &self,
        result: &CoreExecutionResult,
        future: u128,
    ) -> Result<(), &'static str> {
        let bytes = result
            .checked_resource_retained_bytes()
            .ok_or("extended_pc_distributed_memory_projection_overflow")?;
        self.validate_external_result_memory(bytes, future)
            .map_err(|e| e.reason())
    }
}

fn pc_field(problem: &SearchProblem) -> Result<BuildProbabilityField, WasmCpuSearchError> {
    let height = u8::try_from(problem.visible_height())
        .map_err(|_| invalid("extended_pc_height_invalid"))?;
    let initial = Board256Mask::from_words(problem.initial_board().occupied_words());
    let full = Board256Mask::all_cells(u16::from(height) * 10)
        .map_err(|_| invalid("extended_pc_height_invalid"))?;
    BuildProbabilityField::from_words_preserving_height(
        height,
        initial.words(),
        full.without(initial).words(),
    )
    .map_err(|_| invalid("extended_pc_field_invalid"))
}

fn replica_cap(cap: u128, external: u128, workers: usize) -> Result<u128, WasmCpuSearchError> {
    cap.checked_sub(external)
        .and_then(|n| n.checked_div(workers.checked_add(2)? as u128))
        .filter(|n| *n != 0)
        .ok_or_else(projection_error)
}

fn candidate_fingerprint(row_ids: &[u32]) -> Result<[u8; 32], WasmCpuSearchError> {
    if row_ids.len() > 60 {
        return Err(invalid("extended_pc_distributed_candidate_too_large"));
    }
    let mut rows = [0_u32; 60];
    rows[..row_ids.len()].copy_from_slice(row_ids);
    rows[..row_ids.len()].sort_unstable();
    if rows[..row_ids.len()].windows(2).any(|w| w[0] == w[1]) {
        return Err(invalid("extended_pc_distributed_candidate_duplicate_row"));
    }
    let mut hash = Sha256::new();
    hash.update(b"clearra.pc-full-height-delegated-rows.v1\0");
    hash.update((row_ids.len() as u32).to_le_bytes());
    for row in &rows[..row_ids.len()] {
        hash.update(row.to_le_bytes());
    }
    Ok(hash.finalize().into())
}

fn receipt_backing<T>(items: &Vec<T>) -> Option<u128> {
    (items.capacity() as u128).checked_mul(size_of::<T>() as u128)
}

fn invalid(reason: &'static str) -> WasmCpuSearchError {
    WasmCpuSearchError::InvalidProblem { reason }
}
fn projection_error() -> WasmCpuSearchError {
    invalid("extended_pc_distributed_memory_projection_overflow")
}
fn storage_error() -> WasmCpuSearchError {
    invalid("extended_pc_distributed_storage_unavailable")
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
    use crate::WasmCpuSearchBackend;
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::{
        PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
        PieceWindow, RequestedSearchBackend,
    };
    use clearra_problem::{ProblemCompiler, SearchOutputPolicy};
    use clearra_rules::profile::{
        builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x},
        rule_profile::RuleProfile,
    };
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    fn forced_problem(height: u8, rule: RuleProfile, mode: &str) -> Arc<SearchProblem> {
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
        let objective = match mode {
            "minimum" => ObjectivePolicy::minimum_cover(),
            "score" => ObjectivePolicy::all().with_score_summary(),
            _ => ObjectivePolicy::all(),
        };
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10_from_words(u16::from(height), initial.words()).unwrap(),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; count])),
            PieceWindow::new(count),
        )
        .with_rule(rule)
        .with_exact_pieces(Some(count))
        .with_allow_hold(false)
        .with_min_remaining_queue(0)
        .with_count_policy(PcCountPolicy::CountAll)
        .with_objective(objective)
        .with_execution_policy(
            PcExecutionPolicy::default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_workers(1)
                .with_allow_backend_fallback(false),
        );
        let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
        Arc::new(match mode {
            "minimum" => problem
                .with_output_policy(SearchOutputPolicy::Trace)
                .with_pc_minimum_cover_v2_evidence(),
            "path" => problem
                .with_output_policy(SearchOutputPolicy::Trace)
                .with_pc_path_v2_evidence(),
            "score" => problem.with_output_policy(SearchOutputPolicy::Trace),
            "failed" => problem
                .with_output_policy(SearchOutputPolicy::CoverageSummary)
                .with_pc_failed_queue_v2_evidence(2),
            _ => problem,
        })
    }

    fn delegated(
        problem: &Arc<SearchProblem>,
        workers: usize,
    ) -> (
        WasmFullHeightPcResultMerger,
        WasmDistributedGeometrySummary,
        Vec<Vec<u8>>,
    ) {
        let control = ExecutionControl::default();
        let mut producer =
            WasmFullHeightPcProducer::new_shared(Arc::clone(problem), workers).unwrap();
        let mut packets = Vec::new();
        let summary = loop {
            match producer
                .advance_with_external_retained(&control, 0)
                .unwrap()
            {
                WasmCandidateProducerAdvance::Pending => {}
                WasmCandidateProducerAdvance::Candidate(packet) => {
                    assert!(packet.is_extended());
                    packets.push(packet);
                }
                WasmCandidateProducerAdvance::Completed(summary) => break summary,
                WasmCandidateProducerAdvance::Cancelled => panic!("unexpected cancellation"),
            }
        };
        // This source-only protocol fixture borrows one physical compute slot
        // at a time. Actual concurrent realms and requested width are covered
        // by realVerifierPool.smoke, not fabricated in one native lease.
        let mut wires = Vec::new();
        for index in 0..workers {
            let mut verifier = producer.new_delegated_verifier().unwrap();
            for packet in packets
                .iter()
                .filter(|packet| packet.ordinal() as usize % workers == index)
            {
                verifier
                    .consume_with_external_retained(
                        packet,
                        &control,
                        packet.checked_nested_retained_bytes().unwrap(),
                    )
                    .unwrap();
            }
            wires.push(verifier.finish_raw().unwrap());
        }
        (producer.into_merger().unwrap(), summary, wires)
    }

    #[test]
    fn delegated_pc_four_word_family_matches_scalar_in_all_profiles_and_heights() {
        for rule in [srs(), srs_plus(), srs_x(), jstris_180(), no_kick()] {
            for height in [7, 8, 12, 24] {
                let problem = forced_problem(height, rule.clone(), "all");
                let reference = WasmCpuSearchBackend::execute_with_control(
                    &problem,
                    &ExecutionControl::default(),
                )
                .unwrap();
                let (mut merger, summary, wires) = delegated(&problem, 3);
                for wire in &wires {
                    merger.absorb_raw(wire, wire.capacity() as u128).unwrap();
                }
                let result = merger
                    .finish(&summary, 3, &ExecutionControl::default())
                    .unwrap();
                assert_eq!(
                    result.normalized_solution_keys(),
                    reference.normalized_solution_keys()
                );
                assert_eq!(
                    result.field("normalized_solution_set_hash"),
                    reference.field("normalized_solution_set_hash")
                );
                assert_eq!(
                    result.coverage_pattern_words(),
                    reference.coverage_pattern_words()
                );
                for field in [
                    "coverage_probability",
                    "count_complete",
                    "unique_solution_count",
                    "build_variant_count",
                ] {
                    assert_eq!(
                        result.field(field),
                        reference.field(field),
                        "{height}/{field}"
                    );
                }
                assert!(result
                    .normalized_solution_keys()
                    .iter()
                    .all(|key| key.starts_with(&format!("ctk2|height={height}|initial="))));
            }
        }
    }

    #[test]
    fn delegated_pc_minimum_path_score_and_failed_queue_keep_distinct_root_authority() {
        for mode in ["minimum", "path", "score", "failed"] {
            let problem = forced_problem(8, srs_plus(), mode);
            let (mut merger, summary, wires) = delegated(&problem, 3);
            for wire in &wires {
                assert_eq!(&wire[..8], b"CPC24P01");
                // These bytes are deliberately not a public Core-result wire.
                let raw = wire::decode(wire, &problem, |_| Ok(())).unwrap();
                assert!(raw
                    .rows
                    .iter()
                    .all(|row| row.key.starts_with("ctk2|height=8|initial=")));
                merger.absorb_raw(wire, wire.capacity() as u128).unwrap();
            }
            let result = merger
                .finish(&summary, 3, &ExecutionControl::default())
                .unwrap();
            assert!(!result.normalized_solution_keys().is_empty());
            match mode {
                "minimum" => assert!(result.pc_chance_coverage_evidence().unwrap().complete()),
                "path" => assert!(result
                    .pc_full_height_replay_evidence()
                    .unwrap()
                    .matches_result_source(&problem, result.normalized_solution_keys())),
                "score" => assert!(result
                    .pc_score_problem_evidence()
                    .unwrap()
                    .matches_search_problem(&problem)),
                "failed" => {
                    assert!(result.pc_chance_coverage_evidence().is_none());
                    let evidence = merger.take_failed_queue_evidence().unwrap();
                    assert!(evidence.matches_problem_owner(&problem));
                    assert_eq!(
                        evidence.success_coverage().words(),
                        result.coverage_pattern_words()
                    );
                    assert!(merger.take_failed_queue_evidence().is_none());
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn raw_pc_rejects_duplicate_ordinal_snapshot_and_unfinished_family() {
        let problem = forced_problem(8, srs_plus(), "all");
        let (mut merger, summary, wires) = delegated(&problem, 3);
        let nonempty = wires
            .iter()
            .find(|wire| u32::from_le_bytes(wire[48..52].try_into().unwrap()) != 0)
            .unwrap();
        merger
            .absorb_raw(nonempty, nonempty.capacity() as u128)
            .unwrap();
        assert_eq!(
            merger
                .absorb_raw(nonempty, nonempty.capacity() as u128)
                .unwrap_err()
                .reason(),
            "extended_pc_distributed_receipt_mismatch"
        );
        drop(merger);
        let (mut fresh, _, _) = delegated(&problem, 3);
        let mut wrong = nonempty.clone();
        wrong[8] ^= 1;
        assert_eq!(
            fresh
                .absorb_raw(&wrong, wrong.capacity() as u128)
                .unwrap_err()
                .reason(),
            "extended_pc_distributed_snapshot_mismatch"
        );
        assert_eq!(
            fresh
                .finish(&summary, 3, &ExecutionControl::default())
                .unwrap_err(),
            "extended_pc_distributed_family_completion_mismatch"
        );
    }

    #[test]
    fn raw_pc_decode_preflight_refuses_allocation_before_guard_and_trailing_bytes() {
        let problem = forced_problem(8, srs_plus(), "all");
        let (_, _, wires) = delegated(&problem, 2);
        let raw = &wires[0];
        let mut called = false;
        let error = match wire::decode(raw, &problem, |_| {
            called = true;
            Err(invalid("test-memory-denied"))
        }) {
            Ok(_) => panic!("a failed memory preflight cannot produce a partial"),
            Err(error) => error,
        };
        assert!(called);
        assert_eq!(error.reason(), "test-memory-denied");
        let mut wrong = raw.clone();
        wrong.push(0);
        assert!(wire::decode(&wrong, &problem, |_| Ok(())).is_err());
        let mut wrong = raw.clone();
        wrong[48..52].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            wire::decode(&wrong, &problem, |_| Ok(()))
                .err()
                .unwrap()
                .reason(),
            "extended_pc_distributed_wire_count_invalid"
        );
    }

    #[test]
    fn raw_pc_requires_coverage_and_complete_valid_physical_family() {
        let problem = forced_problem(8, srs_plus(), "minimum");
        let (merger, _, wires) = delegated(&problem, 2);
        let mut partial = wires
            .iter()
            .map(|bytes| wire::decode(bytes, &problem, |_| Ok(())).unwrap())
            .find(|partial| !partial.rows.is_empty())
            .unwrap();
        partial.covered = PatternBitSet::new(partial.covered.pattern_count());
        assert_eq!(
            partial.validate_union(&merger.engine).unwrap_err().reason(),
            "extended_pc_distributed_coverage_union_mismatch"
        );
        drop(merger);

        let problem = forced_problem(8, srs_plus(), "path");
        let (merger, _, wires) = delegated(&problem, 2);
        let mut partial = wires
            .iter()
            .map(|bytes| wire::decode(bytes, &problem, |_| Ok(())).unwrap())
            .find(|partial| !partial.graphs.is_empty())
            .unwrap();
        partial.graphs.clear();
        assert_eq!(
            partial.validate_union(&merger.engine).unwrap_err().reason(),
            "extended_pc_distributed_physical_family_incomplete"
        );
        let mut partial = wires
            .iter()
            .map(|bytes| wire::decode(bytes, &problem, |_| Ok(())).unwrap())
            .find(|partial| !partial.graphs.is_empty())
            .unwrap();
        let graph = &partial.graphs[0];
        partial.graphs[0] = clearra_replay::SpinCoverageExecutionGraph::new(
            0,
            graph.candidate_key(),
            u32::MAX,
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(
            partial.validate_union(&merger.engine).unwrap_err().reason(),
            "extended_pc_distributed_physical_graph_invalid"
        );
    }
}
