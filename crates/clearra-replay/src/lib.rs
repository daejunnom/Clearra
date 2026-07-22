//! Replay and trace contracts produced after core-c BuildUp.

pub mod board;
pub mod event;
pub mod ownership;
pub mod replay;
mod scoring_execution;
pub mod trace;

pub use event::{
    KickEvidenceEvent, MovementEvidenceEvent, PlacementEvent, RotationRequest, TraceCompleteness,
    TraceCompletenessEvent,
};
pub use ownership::{ColoredCellOwner, ColoredCellOwnership, ColoredCellOwnershipError};
pub use replay::CellOwner;
pub use replay::{
    BuildVariantOperation, BuildVariantReplayInput, ReplayBoardSnapshotEvent,
    ReplayBoardSnapshotPhase, ReplayEngine, ReplayEngineError, ReplayEvent, ReplayEventId,
    ReplayHoldReleaseEvent, ReplayHoldStoreEvent, ReplayHoldSwapEvent, ReplayLockEvent,
    ReplayScoreBasisEvent, ReplayTrace, ReplayTraceBufferBudget, RowMask,
};
pub use scoring_execution::{
    ExactScoringExecutionBatch, ExactScoringExecutionGraph, ScoringExecutionEdge,
    ScoringExecutionNode, ScoringLockEvidence,
};
pub use trace::{
    BoardAfterStep, HoldDecision, LineClearEvent, PieceDecision, PlacementStep, SolutionTrace,
    SolutionTraceBuilder, SolutionTraceBuilderError, TraceCanonicalKey,
};
