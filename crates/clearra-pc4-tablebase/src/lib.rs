//! Pure protocol and reader core for the online PC4 solution tablebase.
//!
//! The crate intentionally has no I/O dependencies. Hosts resolve a dynamic
//! upstream snapshot, qualify it, and satisfy the emitted bounded Range
//! requests. No dataset revision or digest is compiled into the product.

mod fixed_queue_traversal;
mod graph;
mod lazy_materialized_path;
mod lookup;
mod manifest;
mod materializer;
mod protocol;

pub use fixed_queue_traversal::{
    traverse_fixed_queue, FixedQueueAdjacencyQuery, FixedQueueBudgetExceeded, FixedQueueBudgetKind,
    FixedQueueGraphPath, FixedQueueTerminalPredicate, FixedQueueTerminalQuery,
    FixedQueueTraversalBudgets, FixedQueueTraversalError, FixedQueueTraversalGuard,
    FixedQueueTraversalRequest, FixedQueueTraversalResult, FixedQueueTraversalSemanticError,
    QualifiedCompleteAdjacency, QualifiedCompleteAdjacencyProvider, TerminalDepthContract,
};
pub use graph::{decode_graph_target_sequence, GraphTargetDecodeError};
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
    GraphTargetEncoding, ManifestError, Pc4ArtifactRole, Pc4ProfileManifest, Pc4RuleProfile,
    ProfileAvailability, ProfileQualification, SnapshotIdentity, UnsupportedProfileReason,
};
pub use materializer::{
    materialize_qualified_graph_edge, ClearraPlacementIdentity, MaterializationGuard,
    MaterializationOutput, Pc4GraphPiece, Pc4PlacementMaterializer, PlacementIdentityError,
    PlacementMaterializationError, PlacementMaterializationSemanticError, PlacementRotation,
    QualifiedPc4GraphEdge,
};
pub use protocol::{
    LookupSessionId, RangeRequest, RangeResponse, RangeResponseKind, RangeTransportFailure,
};
