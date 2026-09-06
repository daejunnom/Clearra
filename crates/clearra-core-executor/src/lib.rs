//! Rust executor facade for the connected core-c packing path.

pub mod area;
pub mod backend;
pub mod board;
pub mod buildup;
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
pub mod memory;
pub mod order_language;
pub mod packing;
pub mod pc_chance_coverage_evidence;
pub mod pc_failed_queue_evidence;
pub mod performance;
pub mod problem_lowering;
pub mod resource;
pub mod result_views;
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
    canonical_wasm_candidate_packet_batch_sha256, compile_pc4_compact_tablebase,
    encode_canonical_wasm_candidate_packet_batch, install_pc4_compact_tablebase,
    release_pc4_compact_tablebase, Pc4CompactTablebase, Pc4CompactTablebaseArtifact,
    Pc4TablebaseError, Pc4TablebaseLookup, WasmBuildProbabilityAdvance,
    WasmBuildProbabilityBackend, WasmBuildProbabilityCandidateProducer,
    WasmBuildProbabilityDistributedResultMerger, WasmBuildProbabilityDistributedVerifier,
    WasmBuildProbabilitySession, WasmCandidatePacket, WasmCandidateProducerAdvance,
    WasmCpuCandidateProducer, WasmCpuSearchAdvance, WasmCpuSearchBackend, WasmCpuSearchError,
    WasmCpuSearchSession, WasmCpuSearchTerminalAuthority, WasmCpuTerminalResourceAuthority,
    WasmDistributedBackendExecution, WasmDistributedGeometrySummary, WasmDistributedProgress,
    WasmDistributedResultMerger, WasmDistributedVerifier, WasmPackedTilingIdentity,
    WasmProductSearchBackend, WasmSetupParallelCoordinator, WasmSetupParallelProduce,
    WasmSetupParallelWorker, WasmSetupParallelWorkerStep, WasmSetupSearchAdvance,
    WasmSetupSearchBackend, WasmSetupSearchSession, WasmTilingRootAdvance, WasmTilingRootChunk,
    WasmTilingRootProducer, WasmTilingRootResultMerger, WasmTilingRootWorker,
    PC4_COMPACT_TABLEBASE_MAX_BYTES,
};
pub use buildup::{
    BuildUpEvent, BuildUpReducerReport, BuildUpRunResult, BuildUpRunner, BuildUpState,
};
pub use clearra_replay::{
    ScoringExecutionEdge, ScoringExecutionNode, ScoringLockEvidence, SpinCoverageExecutionBatch,
    SpinCoverageExecutionGraph,
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
    FinesseRepresentativeWitness, FinesseSolutionAverage,
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
