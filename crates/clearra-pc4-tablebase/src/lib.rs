//! Pure protocol and reader core for the online PC4 solution tablebase.
//!
//! The crate intentionally has no I/O dependencies. Hosts resolve a dynamic
//! upstream snapshot, qualify it, and satisfy the emitted bounded Range
//! requests. No dataset revision or digest is compiled into the product.

mod bag_draw;
mod bag_reveal;
mod fixed_queue_hold;
mod fixed_queue_traversal;
mod generation_registry;
mod graph;
mod lazy_fixed_queue_traversal;
mod lazy_materialized_path;
mod lookup;
mod manifest;
mod materializer;
mod observation_frontier;
mod protocol;
mod range_admission;
mod range_fragment_cache;

pub use bag_draw::{
    draw_pc4_bag, Pc4BagDrawBatch, Pc4BagDrawError, Pc4BagDrawTransition, Pc4BagDrawWeight,
    Pc4BagProfile, Pc4BagProfileError, Pc4BagState, Pc4BagStateError, PC4_BAG_PIECES,
};
pub use bag_reveal::{
    prepare_pc4_bag_reveal_family, Pc4BagRevealBudgets, Pc4BagRevealCursor, Pc4BagRevealFamily,
    Pc4BagRevealGuard, Pc4BagRevealPageBudgetKind, Pc4BagRevealPageError,
    Pc4BagRevealPrepareBudgetKind, Pc4BagRevealPrepareError, Pc4BagRevealSequence,
    Pc4ExactProbability, PC4_BAG_REVEAL_ABSOLUTE_DRAW_LIMIT,
};
pub use fixed_queue_hold::{
    expand_fixed_queue_hold, FixedQueueHoldBudgetExceeded, FixedQueueHoldBudgetKind,
    FixedQueueHoldBudgets, FixedQueueHoldDecision, FixedQueueHoldExpansionError,
    FixedQueueHoldExpansionGuard, FixedQueueHoldExpansionRequest, FixedQueueHoldExpansionResult,
    FixedQueueHoldPath, FixedQueueHoldState, FixedQueueHoldStep,
};
pub use fixed_queue_traversal::{
    traverse_fixed_queue, FixedQueueAdjacencyQuery, FixedQueueBudgetExceeded, FixedQueueBudgetKind,
    FixedQueueGraphPath, FixedQueueTerminalPredicate, FixedQueueTerminalQuery,
    FixedQueueTraversalBudgets, FixedQueueTraversalError, FixedQueueTraversalGuard,
    FixedQueueTraversalRequest, FixedQueueTraversalResult, FixedQueueTraversalSemanticError,
    QualifiedCompleteAdjacency, QualifiedCompleteAdjacencyProvider, TerminalDepthContract,
};
pub use generation_registry::{
    Pc4ClosedStageDisposition, Pc4CurrentGeneration, Pc4GenerationCancellationOutcome,
    Pc4GenerationFailureOutcome, Pc4GenerationIdentityDrift, Pc4GenerationPreparationFailure,
    Pc4GenerationPromotionOutcome, Pc4GenerationRegistry, Pc4GenerationRegistryError,
    Pc4GenerationRegistrySlot, Pc4GenerationRegistryVersion, Pc4GenerationRetentionChange,
    Pc4GenerationRetentionLimit, Pc4GenerationRetentionLimitError, Pc4GenerationRollbackOutcome,
    Pc4GenerationStageOutcome, Pc4GenerationStageToken, PinnedPc4Generation,
    MAX_RETAINED_PC4_GENERATIONS,
};
pub use graph::{
    clearra_board64_mask_to_hydra_field_hash_v1, decode_graph_target_sequence,
    decode_hydra_graph_record_v1, hydra_field_hash_v1_to_clearra_board64_mask,
    DecodedHydraGraphRecordV1, GraphTargetDecodeError, HydraFieldHashOutsideDomain,
    HydraGraphRecordDecodeError,
};
pub use lazy_fixed_queue_traversal::{
    prepare_fixed_queue_traversal_family, FixedQueueTraversalCursor, FixedQueueTraversalFamily,
    FixedQueueTraversalFamilyRequest, FixedQueueTraversalPage, FixedQueueTraversalPageBudgets,
    FixedQueueTraversalPageError, FixedQueueTraversalPrepareError,
};
pub use lazy_materialized_path::{
    prepare_fixed_queue_concrete_family, ConcretePathMaterializationBudgetKind,
    ConcretePathMaterializationBudgets, ConcretePathMaterializationError,
    ConcretePathMaterializationSemanticError, ConcretePathPageError, FixedQueueConcretePath,
    FixedQueueConcretePathCursor, FixedQueueConcretePathFamily,
    FixedQueuePathMaterializationRequest,
};
pub use lookup::{
    FormatMismatch, LookupFailure, LookupHit, LookupMachine, LookupStartError, LookupStep,
    SupplyError,
};
pub use manifest::{
    ActivatedSnapshot, ActivationError, ArtifactDescriptor, DatasetSnapshotManifest,
    DatasetSnapshotVerifier, FieldIdIndexRelation, GraphTargetEncoding, ManifestContentIdentity,
    ManifestError, Pc4ArtifactRole, Pc4ProfileManifest, Pc4RuleProfile, Pc4TargetLines,
    Pc4TerminalUseCase, ProfileAvailability, ProfileQualification,
    ProfileTargetCompletenessQualification, QualifiedPc4TargetIdentity, QualifiedSnapshotIdentity,
    SnapshotIdentity, SnapshotVerificationAttestation, SnapshotVerificationBinding,
    SnapshotVerificationFailure, SnapshotVerificationRequest, TargetQualificationError,
    UnsupportedProfileReason,
};
pub use materializer::{
    materialize_qualified_graph_edge, ClearraPlacementIdentity, MaterializationGuard,
    MaterializationOutput, Pc4GraphPiece, Pc4PlacementMaterializer, PlacementIdentityError,
    PlacementMaterializationError, PlacementMaterializationSemanticError, PlacementRotation,
    QualifiedPc4GraphEdge,
};
pub use observation_frontier::{
    prepare_pc4_observation_frontier, Pc4ObservationFrontierBudgets, Pc4ObservationFrontierCursor,
    Pc4ObservationFrontierEntry, Pc4ObservationFrontierFamily, Pc4ObservationFrontierGuard,
    Pc4ObservationFrontierPage, Pc4ObservationFrontierPageBudgetKind,
    Pc4ObservationFrontierPageError, Pc4ObservationFrontierPrepareBudgetKind,
    Pc4ObservationFrontierPrepareError, Pc4ObservationFrontierRequest,
};
pub use protocol::{
    LookupSessionId, RangeRequest, RangeResponse, RangeResponseKind, RangeTransportFailure,
};
pub use range_admission::{
    RangeAdmissionAttempt, RangeAdmissionBinding, RangeAdmissionBudgetKind, RangeAdmissionError,
    RangeAdmissionGuard, RangeAdmissionInput, RangeAdmissionLimits, RangeAdmissionOutcome,
    RangeAdmissionSession, RangeAdmissionUsage, RangeHttpResponse,
};
pub use range_fragment_cache::{
    RangeFragmentCache, RangeFragmentCacheBudgetKind, RangeFragmentCacheError,
    RangeFragmentCacheGuard, RangeFragmentCacheInsert, RangeFragmentCacheLimits,
    RangeFragmentResponseBinding,
};
