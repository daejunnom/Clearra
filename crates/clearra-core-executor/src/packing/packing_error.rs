use clearra_core_ffi::{
    FfiProblemError, NativeCoreError, NativePackingCandidateSinkError, PackingCandidateBatchError,
    PackingCandidateIdentityError,
};
use clearra_supply::BagMultisetProjectionError;

use crate::backend::{
    BackendSelectionError, BackendTrustState, GpuExecutionFailure, GpuExecutionFailureResolution,
    SelectedSearchBackend,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackingRunnerError {
    Ffi(FfiProblemError),
    Native(NativeCoreError),
    Backend(BackendSelectionError),
    BackendExecutorUnavailable {
        backend: SelectedSearchBackend,
        reason: &'static str,
    },
    BackendExecutionMismatch {
        selected: SelectedSearchBackend,
        actual: SelectedSearchBackend,
    },
    BackendTrustMismatch {
        backend: SelectedSearchBackend,
        trust_state: BackendTrustState,
    },
    GpuExecution(GpuExecutionFailure),
    GpuExecutionRejected(GpuExecutionFailureResolution),
    CandidateIdentityExhausted,
    PatternGroupCapacityExceeded,
    CandidateBatch(PackingCandidateBatchError),
    CandidateReducer(NativePackingCandidateSinkError),
    CandidateIdentity(PackingCandidateIdentityError),
    CandidateMultisetOutsideFamily,
    GeometryCatalogMismatch,
    BagMultisetProjection(BagMultisetProjectionError),
    NoReachablePieceMultiset,
    ExecutionCancelled,
    ParallelWorkerPanicked,
}
