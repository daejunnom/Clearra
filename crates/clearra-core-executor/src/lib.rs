//! Rust executor facade for the connected core-c packing path.

pub mod area;
pub mod backend;
pub mod board;
pub mod buildup;
mod conditioned_local_index;
mod conditioned_local_pack;
mod conditioned_local_product;
#[cfg(any(test, feature = "qualification-reference"))]
mod conditioned_local_qualification;
mod conditioned_local_relation;
mod conditioned_reachability;
pub mod core_execution_result;
pub mod core_executor;
pub mod core_postprocess_execution;
pub mod core_postprocess_score_cell;
pub mod core_postprocess_spin_coverage;
#[cfg(feature = "parallel")]
mod cpu_worker_pool;
pub mod diagnostics;
#[cfg(test)]
mod execution_worker_limit;
pub mod finesse_report;
mod legal_board;
pub mod memory;
pub mod order_language;
pub mod packing;
pub mod pc_chance_coverage_evidence;
pub mod pc_failed_queue_evidence;
pub mod performance;
pub mod problem_lowering;
#[cfg(any(test, feature = "qualification-reference"))]
pub mod reachability_reference;
pub mod resource;
pub mod result_views;
mod row_frame;
mod search_prune_policy;
pub mod service;
pub mod setup_finder_report;
pub mod solution_probability;
pub mod solution_set_audit;
pub mod spin;
#[cfg(test)]
pub(crate) mod terminal_supply_conformance;
pub mod tiling_solution_store;

#[cfg(feature = "webgpu-search")]
pub use backend::WasmWebGpuCandidateProducer;
pub use backend::{
    any_pc4_ilc_target_field, canonical_wasm_candidate_packet_batch_sha256,
    encode_canonical_wasm_candidate_packet_batch, enumerate_pc4_ilc_geometric_predecessor_fields,
    enumerate_pc4_ilc_predecessor_fields, enumerate_pc4_ilc_target_fields,
    materialize_pc4_ilc_transition, Pc4IlcForwardMembershipWorkspace, Pc4IlcMaterializationError,
    Pc4IlcPlacement, WasmBuildProbabilityAdvance, WasmBuildProbabilityBackend,
    WasmBuildProbabilityCandidateProducer, WasmBuildProbabilityDistributedResultMerger,
    WasmBuildProbabilityDistributedVerifier, WasmBuildProbabilitySession, WasmCandidatePacket,
    WasmCandidateProducerAdvance, WasmCpuCandidateProducer, WasmCpuSearchAdvance,
    WasmCpuSearchBackend, WasmCpuSearchError, WasmCpuSearchSession, WasmCpuSearchTerminalAuthority,
    WasmCpuTerminalResourceAuthority, WasmDistributedBackendExecution,
    WasmDistributedGeometrySummary, WasmDistributedProgress, WasmDistributedResultMerger,
    WasmDistributedVerifier, WasmPackedTilingIdentity, WasmPcRootProducer, WasmPcRootResultMerger,
    WasmProductSearchBackend, WasmSetupParallelCoordinator, WasmSetupParallelProduce,
    WasmSetupParallelWorker, WasmSetupParallelWorkerStep, WasmSetupSearchAdvance,
    WasmSetupSearchBackend, WasmSetupSearchSession, WasmTilingRootAdvance, WasmTilingRootChunk,
    WasmTilingRootProducer, WasmTilingRootResultMerger, WasmTilingRootWorker,
};
pub use buildup::{
    BuildUpEvent, BuildUpReducerReport, BuildUpRunResult, BuildUpRunner, BuildUpState,
};
pub use clearra_replay::{
    ScoringExecutionEdge, ScoringExecutionNode, ScoringLockEvidence, SpinCoverageExecutionBatch,
    SpinCoverageExecutionGraph,
};
pub use conditioned_local_index::{
    LocalRelationCandidateIndex, LocalRelationCandidateLookup, LocalRelationIndexError,
};
pub use conditioned_local_pack::{
    built_in_local_relation_binding, coalesce_identical_local_relation_records,
    encode_local_relation_candidate_pack, load_local_relation_candidate_pack, LocalRelationBinding,
    LocalRelationCandidatePack, LocalRelationPackError,
};
pub use conditioned_local_product::{
    active_qualified_local_relation_identity, install_qualified_local_relation_pack,
    remove_qualified_local_relation_pack, LocalRelationProductError, LocalRelationProductLookup,
    QualifiedLocalRelationPack, LOCAL_RELATION_COMPLETENESS_SCOPE,
};
#[cfg(any(test, feature = "qualification-reference"))]
pub use conditioned_local_qualification::{
    audit_candidate_local_relation_pack, audited_local_relation_candidate_pack,
    prove_candidate_local_relation_context_coverage, AuditedLocalRelationCandidatePack,
    AuditedLocalRelationRecordSet, LocalRelationCandidateAuditError, LocalRelationCoverageDomain,
    LocalRelationCoverageError, LocalRelationCoverageResult,
};
#[cfg(any(test, feature = "qualification-reference"))]
pub use conditioned_local_relation::solver_local_relation_spawn_entries;
pub use conditioned_local_relation::{
    derive_exact_conditioned_local_relation, derive_exact_conditioned_local_relation_with_frame,
    solver_local_relation_windows, ConditionedPoseWindow, ExactConditionedLocalRelation,
    LocalRelationRowFrame,
};
pub use conditioned_reachability::{
    active_conditioned_reachability_identity, built_in_conditioned_reachability_binding,
    derive_exact_conditioned_entry_lock_anchors, derive_exact_conditioned_reachability_record,
    encode_conditioned_reachability, install_conditioned_reachability_pack,
    remove_conditioned_reachability_pack, BoardConditionedReachability, ConditionedEntryPoseSet,
    ConditionedEvidenceLevel, ConditionedReachabilityAssetError, ConditionedReachabilityBinding,
    ConditionedReachabilityEntryPose, ConditionedReachabilityExpectation,
    ConditionedReachabilityLookup, ConditionedReachabilityQuery, ConditionedReachabilityRecord,
    ConditionedTargetScope, QualifiedBoardConditionedReachability,
    CONDITIONED_REACHABILITY_COMPLETENESS_SCOPE,
};
pub use core_execution_result::{
    CoreExecutionResult, CorePathStep, PcScoreDistributedMergeEvidence,
    PcTilingMemoryAdmissionEvidence,
};
pub use core_executor::{CoreExecutionError, CoreExecutor};
pub use core_postprocess_execution::CorePostProcessExecution;
pub use core_postprocess_score_cell::CorePostProcessScoreCell;
pub use core_postprocess_spin_coverage::CorePostProcessSpinCoverage;
pub use finesse_report::{
    FinessePolicyResult, FinesseReport, FinesseReportInput, FinesseReportPlacement,
    FinesseRepresentativeWitness, FinesseSearchSolutionFilterError, FinesseSolutionAverage,
};
pub use legal_board::{
    accelerator_profile_name, active_qualified_exact_legal_board_identity,
    built_in_binding as built_in_legal_board_binding,
    built_in_rule_identity as built_in_legal_board_rule_identity,
    encode_exact_intersection as encode_exact_legal_board_intersection,
    encode_exact_intersection_streaming as encode_exact_legal_board_intersection_streaming,
    install_qualified_exact_legal_board, remove_qualified_exact_legal_board, CompletionCapability,
    ExactLegalBoard, LegalBoardAssetError, LegalBoardBinding, LegalBoardDecision,
    LegalBoardExpectation, LegalBoardQuery, LegalBoardStreamEncodeError, OriginalRowFrame,
    ProviderStatus, QualifiedExactLegalBoard, RowCodecError, UnsupportedLegalBoardProfile,
    EXACT_LEGAL_BOARD_COMPLETENESS_SCOPE,
};
pub use memory::ScopeGuard;
pub use packing::{PackingExecutionPlan, PackingRunResult, PackingRunner, PackingState};
pub use pc_chance_coverage_evidence::{
    canonical_probability_v2, strict_coverage_pattern_bitset_from_words,
    DistributedPcChanceCoverageRows, DistributedPcChanceCoverageRowsError,
    PcChanceCoverageEvidence, PcChanceProblemEvidence, PcScoreProblemEvidence,
    StrictCoveragePatternWordsError,
};
pub use pc_failed_queue_evidence::{
    PcFailedQueueEvidence, PcFailedQueueEvidenceError, PcFailedQueueExampleEvidence,
    PcFailedQueueExecutionAuthority, PcFailedQueueIncompleteStage, PcFailedQueueMemoryReport,
    PcFailedQueueProbabilityClass,
};
#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
pub use performance::{
    ExecutorSearchProfileError, ExecutorSearchProfileSession, ExecutorSearchProfileStage,
};
pub use result_views::{
    BackendReport, BuildUpResult, BuildVariantView, CoverageResult, CoverageRowView,
    ObjectiveResult, PackingCandidateView, PackingResult, ReplayTrace, SearchExecutionReport,
};
#[cfg(feature = "local-search-ab")]
pub use search_prune_policy::{
    install_local_pc4_legal_board_index, local_search_prune_policy, set_local_search_prune_policy,
    LocalPc4LegalBoardIndex, LocalSearchPrunePolicy,
};
pub use service::{
    CoverService, CoverServiceError, PcFailedQueueExecution, PcFailedQueueExecutionError,
    PcService, PcServiceError, PercentService, PercentServiceError,
};
pub use setup_finder_report::{SetupCandidateReport, SetupFinderReport, SetupHoldConditionReport};
pub use solution_probability::{
    normalized_solution_probability_reports, solution_probability_pattern_weights,
    NormalizedSolutionCoverage, NormalizedSolutionProbabilityError, SolutionAverageScoreReport,
    SolutionCoverage, SolutionProbabilityPatternWeightsError, SolutionProbabilityReport,
};
pub use solution_set_audit::{
    EquivalentCoverageClass, SolutionAuditCandidate, SolutionAuditCheckpoint,
    SolutionPortfolioCursor, SolutionPortfolioFamily, SolutionPortfolioPage,
    SolutionPortfolioPageEntry, SolutionPortfolioPageError, SolutionPortfolioSelectionPolicy,
    SolutionPortfolioSnapshot, SolutionProductFamily, SolutionSemanticDimensions,
    SolutionSetAuditError, SolutionSetAuditFieldBuildError, SolutionSetAuditFieldProjection,
    SolutionSetAuditGuardedError, SolutionSetAuditInput, SolutionSetAuditMemoryGuardError,
    SolutionSetAuditMemoryProjection, SolutionSetAuditReport, SolutionSetAuditStage,
    SolutionSetAuditStageKind, SOLUTION_SET_AUDIT_SCHEMA,
};
pub use spin::{BuildVariantReplayEvidence, BuildVariantReplayEvidenceError};
pub use tiling_solution_store::TilingSolutionPageStore;

pub fn native_core_runtime_available() -> bool {
    clearra_core_ffi::CoreCNative::linked()
}
#[cfg(test)]
pub use spin::{
    SpinProbabilityResult, SpinTargetExecutionReport, SpinTargetRunResult, SpinTargetRunner,
    SpinTargetRunnerError,
};
