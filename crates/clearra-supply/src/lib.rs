//! Queue, bag, hold, and supply validation primitives.

pub mod bag;
pub mod custom_bag;
pub mod diagnostics;
pub mod execution_automaton;
pub mod finite_allocation;
pub mod frontier;
pub mod hold;
pub mod hold_automaton;
pub mod mixed;
pub mod normalize;
pub mod pattern_universe;
pub mod piece_source;
pub mod queue;

pub use execution_automaton::{
    SupplyBranchKind, SupplyExecutionAutomaton, SupplyExecutionError, SupplyExecutionMemoKey,
    SupplyExecutionState, SupplyExecutionStep, SupplyHoldState, SupplyObservationIdentity,
    SupplyTransitionEvidence,
};
pub use finite_allocation::{
    FiniteSupplyAllocationError, FiniteSupplyAllocationLedger, FiniteSupplyAllocationTransaction,
};
pub use pattern_universe::{
    reachable_bag_multisets, BagHoldBranchKind, BagMultisetProjectionError, BagPlacementAutomaton,
    BagPlacementState, BagSupplyBranch, MaterializedPatternUniverse, PackingHoldProjection,
    PackingMultisetFamily, PackingMultisetGroup, PackingPatternMembershipKind,
    PatternPiecePositionIndex, PatternPiecePositionIndexError, PatternSequenceReader,
    PatternUniverseMaterializationError, PatternUniverseMaterializer, PieceMultisetKey,
    ProbabilityWeight,
};
pub use piece_source::{
    finite_build_piece_source_returned_carrier_delta_bytes,
    FiniteBuildPieceSourceAllocationProjection, FiniteBuildPieceSourceMaterialization,
    FiniteBuildPieceSourceRequest, FiniteBuildQueueRef, FiniteBuildSupplyQueue,
    FinitePieceSourceMaterializationError, FiniteSupplyProvenanceRef,
};
pub use queue::queue_observation_policy::QueueObservationPolicy;
