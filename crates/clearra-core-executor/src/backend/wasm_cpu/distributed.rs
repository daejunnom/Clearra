use std::sync::Arc;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_problem::SearchProblem;
use sha2::{Digest, Sha256};

use crate::{
    CoreExecutionResult, CorePostProcessScoreCell, WasmCpuSearchError,
    WasmCpuTerminalResourceAuthority,
};

use super::{
    mix_digest,
    result::{DistributedGeometryAdvance, ExactSearchAdvance, WasmExactSearchSession},
    WasmExactSearchError,
};

pub const CANONICAL_WASM_CANDIDATE_PACKET_MAGIC: u32 = 0x4342_4131;
pub const CANONICAL_WASM_CANDIDATE_PACKET_WIRE_VERSION: u32 = 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmCandidatePacket {
    ordinal: u64,
    pass_index: u8,
    target_index: u32,
    row_ids: Vec<u32>,
}

impl WasmCandidatePacket {
    const EXTENDED_TARGET_INDEX: u32 = u32::MAX;

    pub fn new(ordinal: u64, target_index: u32, row_ids: Vec<u32>) -> Self {
        Self::for_pass(ordinal, 0, target_index, row_ids)
    }

    pub fn for_pass(ordinal: u64, pass_index: u8, target_index: u32, row_ids: Vec<u32>) -> Self {
        Self {
            ordinal,
            pass_index,
            target_index,
            row_ids,
        }
    }

    pub fn for_extended_pass(ordinal: u64, pass_index: u8, row_ids: Vec<u32>) -> Self {
        Self::for_pass(ordinal, pass_index, Self::EXTENDED_TARGET_INDEX, row_ids)
    }

    pub const fn ordinal(&self) -> u64 {
        self.ordinal
    }

    pub const fn pass_index(&self) -> u8 {
        self.pass_index
    }

    pub const fn target_index(&self) -> u32 {
        self.target_index
    }

    pub const fn is_extended(&self) -> bool {
        self.target_index == Self::EXTENDED_TARGET_INDEX
    }

    pub fn row_ids(&self) -> &[u32] {
        &self.row_ids
    }

    /// Returns the heap payload retained by the private row-id buffer using
    /// its actual allocation capacity. The inline candidate packet and inline
    /// `Vec` owner are excluded.
    pub fn checked_nested_retained_bytes(&self) -> Option<u128> {
        (self.row_ids.capacity() as u128).checked_mul(core::mem::size_of::<u32>() as u128)
    }
}

/// Encodes the sole canonical candidate packet stream shared by native and
/// browser verifier transports. Callers must pass packets emitted by the
/// actual `WasmBuildProbabilityCandidateProducer`; this codec does not mint
/// candidate authority on its own.
pub fn encode_canonical_wasm_candidate_packet_batch(candidates: &[WasmCandidatePacket]) -> Vec<u8> {
    let row_count = candidates
        .iter()
        .map(|candidate| candidate.row_ids().len())
        .sum::<usize>();
    let mut output = Vec::with_capacity(12 + candidates.len() * 20 + row_count * 4);
    output.extend_from_slice(&CANONICAL_WASM_CANDIDATE_PACKET_MAGIC.to_le_bytes());
    output.extend_from_slice(&CANONICAL_WASM_CANDIDATE_PACKET_WIRE_VERSION.to_le_bytes());
    output.extend_from_slice(&(candidates.len() as u32).to_le_bytes());
    for candidate in candidates {
        output.extend_from_slice(&candidate.ordinal().to_le_bytes());
        output.extend_from_slice(&u32::from(candidate.pass_index()).to_le_bytes());
        output.extend_from_slice(&candidate.target_index().to_le_bytes());
        output.extend_from_slice(&(candidate.row_ids().len() as u32).to_le_bytes());
        for row_id in candidate.row_ids() {
            output.extend_from_slice(&row_id.to_le_bytes());
        }
    }
    output
}

pub fn canonical_wasm_candidate_packet_batch_sha256(candidates: &[WasmCandidatePacket]) -> String {
    format!(
        "{:x}",
        Sha256::digest(encode_canonical_wasm_candidate_packet_batch(candidates))
    )
}

#[cfg(test)]
mod candidate_packet_retained_bytes_tests {
    use super::WasmCandidatePacket;

    #[test]
    fn nested_retained_bytes_counts_private_row_id_capacity_not_length() {
        let mut row_ids = Vec::with_capacity(64);
        row_ids.extend([3_u32, 7]);
        assert!(row_ids.capacity() > row_ids.len());
        let expected =
            (row_ids.capacity() as u128).checked_mul(core::mem::size_of::<u32>() as u128);
        let candidate = WasmCandidatePacket::for_pass(11, 2, 5, row_ids);

        assert_eq!(candidate.checked_nested_retained_bytes(), expected);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WasmDistributedGeometrySummary {
    pub candidate_count: usize,
    pub candidate_digest: u64,
    pub candidate_family_count: Option<u128>,
    pub expanded_nodes: usize,
    pub peak_frontier: usize,
    pub domain_pruned_states: usize,
    pub hall_pruned_states: usize,
    pub column_pruned_states: usize,
    pub component_compositions: usize,
    pub truncated_reason: Option<&'static str>,
    pub backend_execution: WasmDistributedBackendExecution,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WasmDistributedProgress {
    pub geometry_nodes: usize,
    pub candidates: usize,
    pub candidate_family_count: Option<u128>,
    pub build_nodes: usize,
    pub coverage_checks: usize,
    pub pass_index: usize,
    pub pass_count: usize,
    pub layer_index: usize,
    pub layer_count: usize,
    pub layer_done: usize,
    pub layer_total: usize,
}

impl WasmDistributedProgress {
    pub fn merge(&mut self, other: Self) {
        self.geometry_nodes = self.geometry_nodes.saturating_add(other.geometry_nodes);
        self.candidates = self.candidates.saturating_add(other.candidates);
        self.build_nodes = self.build_nodes.saturating_add(other.build_nodes);
        self.coverage_checks = self.coverage_checks.saturating_add(other.coverage_checks);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WasmDistributedBackendExecution {
    Cpu,
    WebGpu {
        adapter_index: u8,
        adapter_name: String,
        adapter_type: &'static str,
        adapter_backend: String,
        peak_gpu_bytes: u64,
        shader_hash: String,
        shader_version: &'static str,
        warmup_performed: bool,
        session_reused: bool,
    },
    CpuFallback {
        reason: &'static str,
        failure_class: &'static str,
        failure_stage: &'static str,
        discarded_partial_gpu_result: bool,
        original_gpu_result_incomplete: bool,
    },
}

pub enum WasmCandidateProducerAdvance {
    Pending,
    Candidate(WasmCandidatePacket),
    Completed(WasmDistributedGeometrySummary),
    Cancelled,
}

pub struct WasmCpuCandidateProducer {
    session: WasmExactSearchSession,
    candidate_count: usize,
    candidate_digest: u64,
    verification_required: bool,
    finished: bool,
}

impl WasmCpuCandidateProducer {
    pub fn new(problem: &SearchProblem) -> Result<Self, &'static str> {
        Self::new_typed(problem).map_err(WasmCpuSearchError::reason)
    }

    pub fn new_typed(problem: &SearchProblem) -> Result<Self, WasmCpuSearchError> {
        let verification_required = problem.objective().kind()
            != clearra_core_domain::objective::objective_kind::ObjectiveKind::Tiling;
        Ok(Self {
            session: WasmExactSearchSession::new(problem).map_err(map_typed_error)?,
            candidate_count: 0,
            candidate_digest: 0,
            verification_required,
            finished: false,
        })
    }

    pub fn new_shared_under_terminal_authority(
        problem: Arc<SearchProblem>,
        checked_external_retained_upper_bound_bytes: u128,
        authority: &WasmCpuTerminalResourceAuthority,
    ) -> Result<Self, WasmCpuSearchError> {
        if !problem.objective().score().requested() {
            return Err(WasmCpuSearchError::InvalidProblem {
                reason: "wasm_terminal_authority_requires_typed_score_producer",
            });
        }
        Ok(Self {
            session: WasmExactSearchSession::new_shared_under_authority(
                problem,
                checked_external_retained_upper_bound_bytes,
                authority,
            )
            .map_err(map_typed_error)?,
            candidate_count: 0,
            candidate_digest: 0,
            verification_required: true,
            finished: false,
        })
    }

    pub fn advance(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<WasmCandidateProducerAdvance, &'static str> {
        if self.finished {
            return Err("wasm_distributed_geometry_already_finished");
        }
        if control.is_cancelled() {
            return Ok(WasmCandidateProducerAdvance::Cancelled);
        }
        match self
            .session
            .advance_distributed_geometry(self.candidate_count)
            .map_err(map_error)?
        {
            DistributedGeometryAdvance::Pending => Ok(WasmCandidateProducerAdvance::Pending),
            DistributedGeometryAdvance::Candidate {
                target_index,
                row_ids,
                identity_hash,
            } => {
                let ordinal = self.candidate_count as u64;
                self.candidate_count = self.candidate_count.saturating_add(1);
                self.candidate_digest = mix_digest(self.candidate_digest, identity_hash);
                let candidate = WasmCandidatePacket::new(ordinal, target_index, row_ids);
                if !self.verification_required {
                    match self
                        .session
                        .process_external_candidate_with_ordinal(
                            candidate.target_index(),
                            candidate.row_ids(),
                            candidate.ordinal(),
                            control,
                        )
                        .map_err(map_error)?
                    {
                        Some(ExactSearchAdvance::Cancelled) => {
                            return Ok(WasmCandidateProducerAdvance::Cancelled);
                        }
                        Some(ExactSearchAdvance::Completed(_)) => {
                            return Err("wasm_tiling_producer_completed_early");
                        }
                        Some(ExactSearchAdvance::Pending) | None => {}
                    }
                    Ok(WasmCandidateProducerAdvance::Pending)
                } else {
                    Ok(WasmCandidateProducerAdvance::Candidate(candidate))
                }
            }
            DistributedGeometryAdvance::ResourceIncomplete(reason) => {
                self.finished = true;
                Ok(WasmCandidateProducerAdvance::Completed(
                    self.session.distributed_geometry_summary(
                        self.candidate_count,
                        self.candidate_digest,
                        Some(reason),
                    ),
                ))
            }
            DistributedGeometryAdvance::Complete => {
                self.finished = true;
                Ok(WasmCandidateProducerAdvance::Completed(
                    self.session.distributed_geometry_summary(
                        self.candidate_count,
                        self.candidate_digest,
                        None,
                    ),
                ))
            }
        }
    }

    pub fn into_merger(self) -> Result<WasmDistributedResultMerger, &'static str> {
        if !self.finished {
            return Err("wasm_distributed_geometry_not_finished");
        }
        Ok(WasmDistributedResultMerger::from_session(
            self.session
                .into_distributed_finalizer()
                .map_err(map_error)?,
        ))
    }

    pub fn progress(&self) -> WasmDistributedProgress {
        let mut progress = self.session.distributed_progress();
        progress.candidates = self.candidate_count;
        progress
    }

    pub const fn verification_required(&self) -> bool {
        self.verification_required
    }
}

pub struct WasmDistributedVerifier {
    session: WasmExactSearchSession,
    finished: bool,
}

impl WasmDistributedVerifier {
    pub fn new(problem: &SearchProblem) -> Result<Self, &'static str> {
        Ok(Self {
            session: WasmExactSearchSession::new_external_geometry(problem).map_err(map_error)?,
            finished: false,
        })
    }

    pub fn new_shared_under_terminal_authority(
        problem: Arc<SearchProblem>,
        checked_external_retained_upper_bound_bytes: u128,
        authority: &WasmCpuTerminalResourceAuthority,
    ) -> Result<Self, &'static str> {
        Ok(Self {
            session: WasmExactSearchSession::new_shared_external_verifier_under_authority(
                problem,
                checked_external_retained_upper_bound_bytes,
                authority,
            )
            .map_err(map_error)?,
            finished: false,
        })
    }

    pub fn consume(
        &mut self,
        candidate: &WasmCandidatePacket,
        control: &ExecutionControl,
    ) -> Result<(), &'static str> {
        if self.finished {
            return Err("wasm_distributed_verifier_already_finished");
        }
        match self
            .session
            .process_external_candidate_with_ordinal(
                candidate.target_index,
                candidate.row_ids(),
                candidate.ordinal,
                control,
            )
            .map_err(map_error)?
        {
            Some(ExactSearchAdvance::Cancelled) => Err("wasm_cpu_search_cancelled"),
            Some(ExactSearchAdvance::Completed(_)) => {
                Err("wasm_distributed_verifier_completed_early")
            }
            Some(ExactSearchAdvance::Pending) | None => Ok(()),
        }
    }

    pub fn finish(&mut self) -> Result<CoreExecutionResult, &'static str> {
        if self.finished {
            return Err("wasm_distributed_verifier_already_finished");
        }
        self.finished = true;
        match self
            .session
            .complete_distributed_worker()
            .map_err(map_error)?
        {
            ExactSearchAdvance::Completed(result) => Ok(result),
            ExactSearchAdvance::Cancelled => Err("wasm_cpu_search_cancelled"),
            ExactSearchAdvance::Pending => Err("wasm_distributed_verifier_finish_pending"),
        }
    }

    pub fn progress(&self) -> WasmDistributedProgress {
        self.session.distributed_progress()
    }
}

pub struct WasmDistributedResultMerger {
    session: WasmExactSearchSession,
    score_cells: Vec<CorePostProcessScoreCell>,
    score_cells_complete: bool,
    score_profile_id: Option<String>,
    tiling_candidate_count: usize,
    tiling_candidate_family_count: Option<u128>,
    tiling_expanded_nodes: usize,
    tiling_peak_frontier: usize,
    tiling_domain_pruned_states: usize,
    tiling_hall_pruned_states: usize,
    tiling_column_pruned_states: usize,
    tiling_component_compositions: usize,
}

impl WasmDistributedResultMerger {
    pub(super) fn from_session(session: WasmExactSearchSession) -> Self {
        Self {
            session,
            score_cells: Vec::new(),
            score_cells_complete: true,
            score_profile_id: None,
            tiling_candidate_count: 0,
            tiling_candidate_family_count: Some(0),
            tiling_expanded_nodes: 0,
            tiling_peak_frontier: 0,
            tiling_domain_pruned_states: 0,
            tiling_hall_pruned_states: 0,
            tiling_column_pruned_states: 0,
            tiling_component_compositions: 0,
        }
    }

    pub fn absorb_tiling_chunk(
        &mut self,
        chunk: &super::tiling_parallel::WasmTilingRootChunk,
    ) -> Result<(), &'static str> {
        let applied = self
            .session
            .absorb_packed_tiling_chunk(chunk)
            .map_err(map_error)?;
        if !applied {
            return Ok(());
        }
        self.tiling_candidate_count = self
            .tiling_candidate_count
            .saturating_add(chunk.identities().len());
        self.tiling_candidate_family_count = match (
            self.tiling_candidate_family_count,
            chunk.candidate_family_count(),
        ) {
            (Some(total), Some(value)) => total.checked_add(value),
            _ => None,
        };
        self.tiling_expanded_nodes = self
            .tiling_expanded_nodes
            .saturating_add(chunk.expanded_nodes());
        self.tiling_peak_frontier = self.tiling_peak_frontier.max(chunk.peak_frontier());
        self.tiling_domain_pruned_states = self
            .tiling_domain_pruned_states
            .saturating_add(chunk.domain_pruned_states());
        self.tiling_hall_pruned_states = self
            .tiling_hall_pruned_states
            .saturating_add(chunk.hall_pruned_states());
        self.tiling_column_pruned_states = self
            .tiling_column_pruned_states
            .saturating_add(chunk.column_pruned_states());
        self.tiling_component_compositions = self
            .tiling_component_compositions
            .saturating_add(chunk.component_compositions());
        Ok(())
    }

    pub(super) const fn tiling_candidate_count(&self) -> usize {
        self.tiling_candidate_count
    }

    pub fn tiling_progress(&self) -> Option<WasmDistributedProgress> {
        (self.tiling_candidate_count != 0 || self.tiling_expanded_nodes != 0).then_some(
            WasmDistributedProgress {
                geometry_nodes: self.tiling_expanded_nodes,
                candidates: self.tiling_candidate_count,
                candidate_family_count: self.tiling_candidate_family_count,
                pass_count: 1,
                ..WasmDistributedProgress::default()
            },
        )
    }

    pub fn absorb(&mut self, result: &CoreExecutionResult) -> Result<(), &'static str> {
        if let Some(profile_id) = result.postprocess_score_profile_id() {
            if self
                .score_profile_id
                .as_deref()
                .is_some_and(|current| current != profile_id)
            {
                return Err("wasm_distributed_score_profile_mismatch");
            }
            self.score_profile_id = Some(profile_id.to_owned());
            self.score_cells
                .extend(result.postprocess_score_cells().iter().cloned());
            self.score_cells_complete &= result.postprocess_score_cells_complete();
        }
        self.session
            .absorb_distributed_result(result)
            .map_err(map_error)
    }

    /// Validates borrowed wire/result owners against the merger's live search
    /// session. The caller supplies every external owner that coexists with
    /// decode or absorb plus any not-yet-allocated checked future bytes.
    pub fn validate_external_result_memory(
        &self,
        external_retained_bytes: u128,
        checked_future_bytes: u128,
    ) -> Result<(), &'static str> {
        self.session
            .validate_external_result_memory_with_future(
                external_retained_bytes,
                checked_future_bytes,
            )
            .map_err(map_error)
    }

    /// Validates a terminal public result while this merger still owns the
    /// request-scoped child execution lease. Typed App post-processing uses
    /// this guard for every intermediate/future allocation, then destroys the
    /// merger before constructing the rich host response.
    pub fn validate_public_result_memory_with_future(
        &self,
        result: &CoreExecutionResult,
        checked_future_bytes: u128,
    ) -> Result<(), &'static str> {
        self.session
            .validate_public_result_memory_with_future(result, checked_future_bytes)
            .map_err(map_error)
    }

    pub fn finish(
        &mut self,
        summary: &WasmDistributedGeometrySummary,
        workers_used: usize,
    ) -> Result<CoreExecutionResult, &'static str> {
        let mut summary = summary.clone();
        if self.tiling_candidate_count != 0 || self.tiling_expanded_nodes != 0 {
            summary.candidate_count = self.tiling_candidate_count;
            summary.candidate_family_count = self.tiling_candidate_family_count;
            summary.expanded_nodes = self.tiling_expanded_nodes;
            summary.peak_frontier = self.tiling_peak_frontier;
            summary.domain_pruned_states = self.tiling_domain_pruned_states;
            summary.hall_pruned_states = self.tiling_hall_pruned_states;
            summary.column_pruned_states = self.tiling_column_pruned_states;
            summary.component_compositions = self.tiling_component_compositions;
        }
        let result = match self
            .session
            .complete_distributed_geometry(&summary, workers_used)
            .map_err(map_error)?
        {
            ExactSearchAdvance::Completed(result) => result,
            ExactSearchAdvance::Cancelled => return Err("wasm_cpu_search_cancelled"),
            ExactSearchAdvance::Pending => return Err("wasm_distributed_merge_finish_pending"),
        };
        self.score_cells.sort_unstable();
        self.score_cells.dedup();
        Ok(match self.score_profile_id.take() {
            Some(profile_id) => result.with_verified_distributed_postprocess_score_cells(
                core::mem::take(&mut self.score_cells),
                self.score_cells_complete,
                profile_id,
            ),
            None => result,
        })
    }
}

pub(super) fn map_error(error: WasmExactSearchError) -> &'static str {
    error.reason()
}

fn map_typed_error(error: WasmExactSearchError) -> WasmCpuSearchError {
    match error {
        WasmExactSearchError::InvalidProblem(reason) => {
            WasmCpuSearchError::InvalidProblem { reason }
        }
        WasmExactSearchError::ResourceAdmission(resource_report) => {
            WasmCpuSearchError::ResourceAdmission { resource_report }
        }
        WasmExactSearchError::Cancelled => WasmCpuSearchError::Cancelled,
    }
}
