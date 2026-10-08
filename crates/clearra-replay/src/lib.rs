//! Replay and trace contracts produced after core-c BuildUp.

pub mod board;
pub mod event;
mod full_height_execution;
mod full_height_replay;
pub mod ownership;
pub mod replay;
mod scoring_execution;
mod spin_coverage_execution;
pub mod trace;

pub use event::{
    KickEvidenceEvent, MovementEvidenceEvent, PlacementEvent, RotationRequest, TraceCompleteness,
    TraceCompletenessEvent,
};
pub use full_height_execution::{FullHeightExecutionBatch, FullHeightExecutionBatchError};
pub use full_height_replay::{
    FullHeightReplayBuildError, FullHeightReplayError, FullHeightReplayProjector,
    FullHeightReplayStep, FullHeightReplayTrace, FullHeightTransition,
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
pub use spin_coverage_execution::{SpinCoverageExecutionBatch, SpinCoverageExecutionGraph};
pub use trace::{
    BoardAfterStep, HoldDecision, LineClearEvent, PieceDecision, PlacementStep, SolutionTrace,
    SolutionTraceBuilder, SolutionTraceBuilderError, TraceCanonicalKey,
};
