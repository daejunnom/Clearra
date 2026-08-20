//! Stable Rust-side ABI mirror types and native C core wrappers.

/// Native archive content identity used to invalidate dependent link products.
///
pub mod board;
pub mod buildup;
pub mod core_rows;
pub mod diagnostics;
pub mod gpu;
pub mod memory;
pub mod native;
pub mod packing;
mod packing_candidate_batch;
pub mod packing_problem;
pub mod problem;
mod raw;
pub mod rules;
pub mod supply;
pub mod version;

pub use board::{
    CBoard128Descriptor, CBoard256Descriptor, CBoard64Layout, CBoard64LineClearResult,
    CBoard64Status, CBoardBackendCapability, CBoardStatus, CGenericBoardMask,
    CStandardPcExtendedBoardDescriptor, CWideBoardDescriptor, C_BOARD_BACKEND_BOARD128,
    C_BOARD_BACKEND_BOARD256, C_BOARD_BACKEND_BOARD64, C_BOARD_BACKEND_WIDE,
    C_BOARD_UNSUPPORTED_REASON_BOARD_BACKEND_NOT_CONNECTED,
    C_BOARD_UNSUPPORTED_REASON_BOARD_WIDTH_OUT_OF_SCOPE, C_BOARD_UNSUPPORTED_REASON_NONE,
    C_BOARD_UNSUPPORTED_REASON_WIDE_BOARD_RUNTIME_NOT_CONNECTED, C_STANDARD_PC_BOARD_WIDTH,
    C_STANDARD_PC_BOARD_WORD_CAPACITY, C_STANDARD_PC_COMPACT_MAX_LINES,
    C_STANDARD_PC_EXTENDED_MIN_LINES, C_STANDARD_PC_MAX_LINES,
};
pub use buildup::{
    CBuildUpEvent, CBuildUpEventKind, CBuildUpState, CBuildUpTraceStep, CBuildVariantView,
    CBuildVariantViewError, CCoverageOverlapReport, CCoverageRowView, CKickEvidenceView,
    CPatternBitSet, CReachabilityEvidenceView, CResultReducerCounts,
    OwnedCorePatternBitSetSnapshot, C_BUILDUP_HOLD_BRANCH_CURRENT,
    C_BUILDUP_HOLD_BRANCH_RELEASE_HELD_AT_TERMINAL, C_BUILDUP_HOLD_BRANCH_STORE_CURRENT,
    C_BUILDUP_HOLD_BRANCH_SWAP_HELD, C_COVERAGE_MAX_PATTERNS, C_COVERAGE_MAX_WORDS,
    C_COVERAGE_ROW_KIND_BUILD, C_SCORE_MATRIX_CAPACITY_EXCEEDED, C_SPIN_COVERAGE_CAPACITY_EXCEEDED,
};
pub use core_rows::{CBuildUpResult, CCoverageRow};
#[cfg(feature = "experimental-native-gpu")]
pub use gpu::{
    CGpuPackingBatchDescriptorDebugSnapshot, CGpuPackingBatchDescriptorView,
    CGpuPackingBatchDescriptorViewError, CGpuPieceMultisetWindow, CGpuWorkerRequestView,
};
pub use gpu::{
    NativeGpuCapabilityQuery, NativeGpuCapabilityQueryError, NativeGpuSearchCapability,
    NativeGpuUnavailableReason,
};
pub use memory::{
    BatchScope, CClrMemContext, CClrMemLeakReport, CClrMemStatus, CClrScope, CClrScopeKind,
    ContractBatchScope, ContractCoreContext, ContractSearchScope, CoreContext, CoreLeakReport,
    CoreMemoryError, CoreScopeKind, MemoryBackendKind, NativeBatchScope, NativeCoreContext,
    NativeLeakReport, NativeMemoryBindingStatus, NativeMemoryError, NativeScopeKind,
    NativeSearchScope, ReleaseSignal, ReleaseSignalSnapshot, SearchScope,
};
pub use native::{
    BuildUpGeometryLanguage, BuildUpGeometryLanguageEdge, BuildUpGeometryLanguageEdgeV2,
    BuildUpGeometryLanguageNode, BuildUpGeometryLanguageNodeV2, BuildUpGeometryLanguageV2,
    BuildUpGeometryTransitionMode, CNativeBuildUpCountLimits, CNativeBuildUpCountReport,
    CNativeBuildUpEnumerationLimits, CNativeBuildUpSearchMetrics, CNativeBuildUpVerification,
    CNativeBuildVariantBuffer, CNativeBuildVariantView, CNativeBuildableGeometryStreamReport,
    CNativePruningMinimalRecord, CNativePruningProofLedger, CNativePruningProofLedgerEntry,
    CNativeResourceReport, CoreCNative, NativeBuildUpCountOutcome, NativeBuildUpOutcome,
    NativeBuildUpWorkspace, NativeBuildUpWorkspaceOutcome, NativeBuildableGeometryTaskOutcome,
    NativeCandidateReducer, NativeCoreError, NativeGeometryCatalog, NativeGeometryCatalogIdentity,
    NativeGeometryCatalogView, NativeGeometryGraphSearchOutcome, NativeGeometryPathConsumer,
    NativeGeometryPathSinkError, NativeGeometrySolutionGraph, NativeGeometrySolutionTask,
    NativePackingCandidateConsumer, NativePackingCandidateContext, NativePackingCandidateSinkError,
    NativePruningEvidence, NativePruningLedger, NativePruningLedgerError,
    NativePruningMinimalRecord, CLR_BUILDUP_CAPACITY_EXCEEDED, CLR_BUILDUP_ENUMERATION_TRUNCATED,
    CLR_BUILDUP_MODE_COUNT_VARIANTS, CLR_BUILDUP_MODE_ENUMERATE_VARIANTS,
    CLR_BUILDUP_MODE_VERIFY_FIRST, CLR_BUILDUP_TRACE_COMPLETENESS_KICK_EVIDENCE_MISSING,
    CLR_KICK_EVIDENCE_BUFFER_EXHAUSTED, C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT,
    C_BUILDUP_STATUS_CANCELLED, C_BUILDUP_STATUS_CAPACITY_EXCEEDED,
    C_BUILDUP_STATUS_ENUMERATION_TRUNCATED, C_BUILDUP_STATUS_HOLD_DISABLED_IMPOSSIBLE,
    C_BUILDUP_STATUS_INVALID_ARGUMENT, C_BUILDUP_STATUS_INVALID_ORDER,
    C_BUILDUP_STATUS_INVALID_PROBLEM, C_BUILDUP_STATUS_KICK_EVIDENCE_BUFFER_EXHAUSTED,
    C_BUILDUP_STATUS_LOGICAL_REJECT_MAX, C_BUILDUP_STATUS_LOGICAL_REJECT_MIN, C_BUILDUP_STATUS_OK,
    C_BUILDUP_STATUS_UNSUPPORTED_RUNTIME_SCOPE, C_NATIVE_PRUNING_LEDGER_MAX_ENTRIES,
    C_NATIVE_PRUNING_REASON_COUNT,
};
#[cfg(any(test, feature = "test-support"))]
pub use native::{
    CNativePackingCandidateBuffer, CNativePackingGeometryPath,
    NativeGeometryMaterializationOutcome, NativeGeometryStreamOutcome, NativePackingOutcome,
    NativePackingStreamOutcome, C_NATIVE_PACKING_GEOMETRY_PATH_MAX_OPERATIONS,
};
#[cfg(feature = "search-stage-profiling")]
pub use native::{NativeSearchProfileError, NativeSearchProfileSession, NativeSearchProfileStage};
pub use packing::CPackingState;
pub use packing_candidate_batch::{
    PackingCandidateBatch, PackingCandidateBatchError, PackingCandidateIdentityError,
    PackingCandidateIter, PackingCandidateView,
};
pub use packing_problem::{CPackingCandidate, CPackingOperation, CPackingProblem};
pub use problem::{
    buildup_operation_set_runtime_status, buildup_runtime_status_for_board, CBackendRequest,
    CBagWindow, CBoardDescriptor, CBuildUpOperation, CBuildUpOperationSet, CBuildUpProblem,
    CBuildUpProblemBuilder, CBuildUpProblemTemplate, CCheckpointSpec, CHoldState,
    CKickOffsetDescriptor, CKickSequenceDescriptor, CKickTransitionDescriptor,
    CPackingProblemBuilder, CPieceWindowDescriptor, CProblemBudget, CQueueView,
    CRuleProfileDescriptor, DlxBuildUpBridge, DlxBuildUpBridgeError, DlxBuildUpOperationCandidate,
    FfiProblemError, C_BUILDUP_MAX_OPERATIONS, C_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE,
};
pub use rules::RuleDescriptorCompiler;
pub use supply::{
    CHoldAutomatonStateDescriptor, CPieceSourceDescriptor, CompactSupplyDescriptors,
    HoldAutomatonDescriptorCompiler, PieceSourceDescriptorCompiler, PieceSourceDescriptorError,
    SupplyDescriptorCompiler, C_HOLD_TRANSITION_STORE_CURRENT_THEN_USE_NEXT,
    C_HOLD_TRANSITION_SWAP_HELD, C_HOLD_TRANSITION_USE_CURRENT, C_PIECE_SOURCE_BAG_UNIVERSE,
    C_PIECE_SOURCE_FIXED_QUEUE, C_PIECE_SOURCE_MATERIALIZED_PATTERN_UNIVERSE,
    C_PIECE_SOURCE_OBSERVED_WINDOW, C_SUPPLY_TRUNCATION_MATERIALIZED_PATTERN_BUDGET_EXCEEDED,
    C_SUPPLY_TRUNCATION_NONE, C_SUPPLY_TRUNCATION_OBSERVED_WINDOW_BUDGET_EXCEEDED,
};
pub use version::{CoreAbiVersion, CLEARRA_CORE_ABI_VERSION, CLEARRA_CORE_VERSION};
