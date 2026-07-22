use crate::backend::{
    gpu_worker::{
        validate_gpu_worker_result, GpuBackendCapability, GpuBackendError, GpuBackendKind,
        GpuCpuConfirmBridge, GpuCpuConfirmBridgeError, GpuDeviceSelector, GpuFenceEpoch,
        GpuMemoryTicket, GpuWorkerAutotune, GpuWorkerBackpressure, GpuWorkerBudget,
        GpuWorkerExactnessGate, GpuWorkerMemoryPressureLevel, GpuWorkerMetrics, GpuWorkerReduction,
        GpuWorkerRequest, GpuWorkerResult, GpuWorkerResultReducer, GpuWorkerSubmission,
        PackingBatchDescriptorBuilder, PackingBatchId, PackingBatchValidationError,
    },
    GpuExecutionFailure, GpuExecutionFailureClass, GpuExecutionFailureStage, GpuFallbackBackend,
    GpuPartialResultDisposition, GpuTrustState, GpuWorkerError, HybridThrottleReason,
    SearchBackendFallbackReason,
};
use clearra_core_ffi::{
    problem::{
        CBackendRequest, CBoardDescriptor, CPieceMultisetWindow, CProblemBudget,
        CRuleProfileDescriptor, C_GPU_PIECE_SOURCE_FIXED_SEQUENCE, C_PIECE_I, C_PIECE_O, C_PIECE_S,
        C_PIECE_T, C_PIECE_Z,
    },
    supply::{CPieceSourceDescriptor, C_PIECE_SOURCE_FIXED_QUEUE},
    CPackingProblem,
};
use clearra_pc_graph::request::BackendFallbackPolicy;

fn ticket(id: u64) -> GpuMemoryTicket {
    GpuMemoryTicket::new(id, GpuFenceEpoch::new(3), 4096)
}

fn compact_piece_multiset_window() -> CPieceMultisetWindow {
    let mut window = CPieceMultisetWindow {
        total_count: 5,
        exact_count: 5,
        ..Default::default()
    };
    for piece in [C_PIECE_I, C_PIECE_O, C_PIECE_T, C_PIECE_S, C_PIECE_Z] {
        window.counts[usize::from(piece)] += 1;
    }
    window
}

fn compact_piece_source() -> CPieceSourceDescriptor {
    CPieceSourceDescriptor {
        piece_source_id: 1,
        source_kind: C_PIECE_SOURCE_FIXED_QUEUE,
        provenance_id: 1,
        fixed_sequence_len: 5,
        piece_set_profile_id: 1,
        complete: 1,
        ..Default::default()
    }
}

fn compact_problem() -> CPackingProblem {
    CPackingProblem {
        problem_kind: CPackingProblem::OPENING_PC,
        board: CBoardDescriptor {
            width: 10,
            visible_height: 2,
            search_height: 2,
            initial_mask: 0,
            cell_count: 20,
            ..Default::default()
        },
        piece_window: clearra_core_ffi::problem::CPieceWindowDescriptor {
            max_pieces: 5,
            exact_pieces: 5,
            has_exact_pieces: 1,
            ..Default::default()
        },
        piece_multiset_window: compact_piece_multiset_window(),
        piece_source: compact_piece_source(),
        rule: CRuleProfileDescriptor {
            rule_profile_id: 1,
            kick_profile_id: 3,
            ..Default::default()
        },
        budget: CProblemBudget {
            max_results: 64,
            ..Default::default()
        },
        backend: CBackendRequest {
            requested_backend: 6,
            reserved_flags: 0,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[path = "gpu_worker_contract_behavior/descriptor.rs"]
mod descriptor;
#[path = "gpu_worker_contract_behavior/memory_lifetime.rs"]
mod memory_lifetime;
#[path = "gpu_worker_contract_behavior/scheduling.rs"]
mod scheduling;
#[path = "gpu_worker_contract_behavior/trust_fallback.rs"]
mod trust_fallback;
