use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_problem::SearchProblem;

use crate::{CoreExecutionResult, CorePostProcessScoreCell};

use super::{
    mix_digest,
    result::{DistributedGeometryAdvance, ExactSearchAdvance, WasmExactSearchSession},
    WasmExactSearchError,
};

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
    finished: bool,
}

impl WasmCpuCandidateProducer {
    pub fn new(problem: &SearchProblem) -> Result<Self, &'static str> {
        Ok(Self {
            session: WasmExactSearchSession::new(problem).map_err(map_error)?,
            candidate_count: 0,
            candidate_digest: 0,
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
                Ok(WasmCandidateProducerAdvance::Candidate(
                    WasmCandidatePacket::new(ordinal, target_index, row_ids),
                ))
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
}

impl WasmDistributedResultMerger {
    pub(super) fn from_session(session: WasmExactSearchSession) -> Self {
        Self {
            session,
            score_cells: Vec::new(),
            score_cells_complete: true,
            score_profile_id: None,
        }
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

    pub fn finish(
        &mut self,
        summary: &WasmDistributedGeometrySummary,
        workers_used: usize,
    ) -> Result<CoreExecutionResult, &'static str> {
        let result = match self
            .session
            .complete_distributed_geometry(summary, workers_used)
            .map_err(map_error)?
        {
            ExactSearchAdvance::Completed(result) => result,
            ExactSearchAdvance::Cancelled => return Err("wasm_cpu_search_cancelled"),
            ExactSearchAdvance::Pending => return Err("wasm_distributed_merge_finish_pending"),
        };
        self.score_cells.sort_unstable();
        self.score_cells.dedup();
        Ok(match self.score_profile_id.take() {
            Some(profile_id) => result.with_postprocess_score_cells(
                core::mem::take(&mut self.score_cells),
                self.score_cells_complete,
                profile_id,
            ),
            None => result,
        })
    }
}

fn map_error(error: WasmExactSearchError) -> &'static str {
    match error {
        WasmExactSearchError::InvalidProblem(reason) => reason,
        WasmExactSearchError::Cancelled => "wasm_cpu_search_cancelled",
    }
}
