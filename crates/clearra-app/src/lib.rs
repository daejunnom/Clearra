//! Typed application facade shared by CLI, GUI, and downstream Clearra apps.

pub mod app_command;
pub mod app_context;
pub mod app_error;
pub mod app_request;
pub mod app_response;
pub mod app_services;
mod build_colored_target_document;
mod build_setup_product_projection;
mod build_solution_probability_result;
mod build_v2_product_projection;
pub mod cli_compat;
pub mod commands;
mod cooperative_execution;
pub mod diagnostics;
mod distributed_forward_execution;
mod distributed_search_execution;
mod distributed_setup_execution;
mod document_utility_encoding;
mod execution_constraint_postprocess;
#[cfg(test)]
mod execution_resource_test_support;
mod field_document_parity;
pub mod gui_bridge;
pub mod io;
pub mod language;
#[cfg(all(not(target_arch = "wasm32"), feature = "parallel"))]
mod native_build_probability_execution;
#[cfg(not(target_arch = "wasm32"))]
mod native_forward_execution;
#[cfg(not(target_arch = "wasm32"))]
mod native_spin_structure_execution;
mod objective_contract;
mod parity_page_store;
mod pc_allspin_result;
mod pc_chance_probability_result;
mod pc_failed_queue_result;
mod pc_minimum_cover_result;
mod pc_path_result;
mod pc_result_projection;
mod pc_save_result;
mod pc_score_minimum_cover_result;
mod pc_score_postprocess;
mod pc_score_summary_result;
mod pc_score_winner_result;
mod pc_tiling_family_result;
mod portfolio_alternative_store;
pub mod product_capability_contract;
mod product_capability_result;
mod ranked_family_product_projection;
pub mod render;
pub mod request;
mod request_profile_selection;
mod resource_contract;
pub mod response;
pub mod run_request;
pub mod search_backend_warmup;
mod search_output_surface_postprocess;
mod setup_ranked_family_result;
#[cfg(test)]
mod setup_ranked_fixture;
mod setup_ranking_contract;
mod setup_ranking_facade;
mod setup_score_document;
mod solution_set_audit_postprocess;
mod spin_structure_coverage_result;
mod spin_structure_search_result;
pub mod tablebase_runtime;
mod typed_document_utility;

/// The command value is public for request construction and inspection, but
/// execution remains owned by the checked [`AppContext`] request gateways.
///
/// ```compile_fail
/// use clearra_app::{AppExecutionContext, AppRequest};
///
/// fn bypass(request: AppRequest, context: &AppExecutionContext<'_>) {
///     request.into_command().run(context);
/// }
/// ```
pub use app_command::AppCommand;
pub use app_context::{AppContext, AppExecutionContext};
pub use app_error::{AppError, AppErrorCode};
pub use app_request::{AppOutputPolicy, AppRequest};
pub use app_response::{AppEffect, AppResponse, AppStatus, ExitCodeHint, GovernedAppResponse};
#[cfg(all(not(target_family = "wasm"), feature = "parallel"))]
pub use app_services::{
    register_native_build_probability_host, register_system_native_build_probability_host,
    NativeBuildProbabilityAdmissionProvider, NativeBuildProbabilityAdmissionRequest,
    NativeBuildProbabilityHostProviderError, NativeBuildProbabilityHostRegistration,
    NativeBuildProbabilityHostRegistrationError, NativeBuildProbabilityProviderMeasurement,
    SystemNativeBuildProbabilityAdmissionProvider,
};
pub use app_services::{
    AppClock, AppCoreExecutorService, AppDiagnosticSink, AppLanguageResolverService, AppServices,
};
pub use build_colored_target_document::{
    BuildColoredTargetDocument, BuildColoredTargetDocumentError,
};
pub use build_setup_product_projection::{
    project_build_setup_v1, BuildSetupProductProjectionError,
};
pub use build_solution_probability_result::build_v2_facade::{
    BuildColoredTargetCandidateCoverageV1, BuildColoredTargetCompleteness,
    BuildColoredTargetScoreWinnerV1, BuildColoredTargetSetError, BuildColoredTargetSetV1,
    BuildColoredTargetV1FacadeError, BuildCongruentCoverV1, BuildCongruentCoverV1Request,
    BuildCongruentV1, BuildCongruentV1Request, BuildCoverV2FacadeError, BuildCoverV2Request,
    BuildCoveragePortfolioV2, BuildEvaluateB2bCoverV1Request,
    BuildEvaluateCoverPercentV1FacadeError, BuildEvaluateCoverPercentV1Request,
    BuildEvaluateCoverV1FacadeError, BuildEvaluateCoverV1Request,
    BuildEvaluateMinimalsV1FacadeError, BuildEvaluateMinimalsV1Request,
    BuildEvaluateScoreV1FacadeError, BuildEvaluateScoreV1Request, BuildObjective,
    BuildQueueKnowledge, BuildScoreProfile, BuildSetupCoverPercentV1,
    BuildSetupCoverPercentV1Request, BuildSetupCoverScoreV1, BuildSetupCoverScoreV1Request,
    BuildSetupCoverV1, BuildSetupCoverV1Request, BuildSetupV1, BuildSetupV1Request,
    BuildSuppliedCandidateCoverageV1, BuildSuppliedCoverPercentV1, BuildSuppliedCoverageV1,
    BuildSuppliedMinimumCoverV1, BuildSuppliedProbabilityCompleteness,
    BuildSuppliedReplayCompleteness, BuildSuppliedScoreV1, BuildSuppliedScoreWinnerV1,
    BuildSuppliedSolutionSetError, BuildSuppliedSolutionSetV1,
};
pub use build_v2_product_projection::{
    project_build_congruent_cover_v1, project_build_congruent_v1,
    project_build_setup_cover_percent_v1, project_build_setup_cover_score_v1,
    project_build_setup_cover_v1, project_build_supplied_cover_percent_v1,
    project_build_supplied_coverage_v1, project_build_supplied_minimum_cover_v1,
    project_build_supplied_score_v1, BuildV2ProductProjectionError, ProjectedBuildV2Product,
};
pub use clearra_core_domain::execution_cancellation::{
    CancellationHandle, CancellationToken, ExecutionCancellationHandle, ExecutionCancellationToken,
    ExecutionControl, ExecutionPartition, ExecutionProgress, ProgressSink,
};
pub use clearra_core_executor::order_language::sequence_dependencies::{
    ConcreteDocumentOperation, OperationDocumentProblem,
};
#[cfg(feature = "wasm-stage-profiling")]
pub use clearra_core_executor::{
    ExecutorSearchProfileError, ExecutorSearchProfileSession, ExecutorSearchProfileStage,
};
pub use clearra_core_executor::{
    FinesseReport, FinesseReportInput, FinesseReportPlacement, FinesseRepresentativeWitness,
};
pub use clearra_host_contract::{
    AppCommandKind, AppResult, BackendPolicy, BackendReport, CapabilityReport, ContinuationReport,
    DiagnosticsPolicy, LocalePolicy, OutputPolicy, ProductBuildIdentity, QueryEnvelope,
    ResourceBudget, ResourceReport,
};
pub use clearra_output::{
    decode_ctk3_exact, encode_ctk3_compact, Ctk3Color, Ctk3Document, Ctk3Operation, Ctk3Page,
    Ctk3PageFlags, Ctk3Piece, Ctk3Rotation,
};
pub use commands::{
    BuildProbabilityAppCommand, BuildV2AppCommand, BuildV2AppRequest, ContinueAppCommand,
    ConvertAppCommand, CoverAppCommand, DamageAppCommand, FieldDocumentTransformAppCommand,
    FieldDocumentTransformAppCommandError, FieldDocumentTransformKind, FumenAppCommand,
    FumenAppCommandError, FumenTransformKind, InspectUnsupportedAppCommand,
    OperationSequenceAppCommand, ParityAppCommand, PathAppCommand, PcAppCommand, PercentAppCommand,
    RenAppCommand, RenderAppCommand, RenderAppCommandError, RenderArtifactFormat, RulesAppCommand,
    ScenarioAppCommand, ScenarioAppExpected, ScenarioAppRenderContract, ScoringAppCommand,
    SequenceDependenciesAppCommand, SetupAppCommand, SetupScoreAppCommand,
    SetupScoreAppCommandError, SpinFinderAppCommand, SpinStructureAppCommand,
    SpinStructureProductMode, VerifyAppCommand, SETUP_SCORE_INPUT_CONTRACT,
    SETUP_SCORE_PROBLEM_CONTRACT, SETUP_SCORE_RESULT_CONTRACT,
};
pub use cooperative_execution::{
    CooperativeAppAdvance, CooperativeAppExecution, FiniteCooperativeCallerMemory,
    FiniteCooperativeCallerMemoryRejection,
};
pub use distributed_forward_execution::{
    DistributedForwardPreparation, PreparedDistributedForwardSearch,
};
pub use distributed_search_execution::{
    DistributedSearchPreparation, PreparedDistributedSearch, PreparedDistributedSearchCompletion,
};
pub use distributed_setup_execution::{
    DistributedSetupPreparation, PreparedDistributedSetupSearch,
};
pub use field_document_parity::{
    FieldDocumentFormat, FieldDocumentParityError, FieldDocumentParityPage,
    FieldDocumentParityReport,
};
pub use gui_bridge::{
    GuiAppRequestPreview, GuiBackendCapabilityView, GuiBridgeError, GuiBridgeErrorCode,
    GuiCommandPreview, GuiDisabledReason, GuiFormState, GuiFormValidation, GuiGpuBackendOptionView,
    GuiStatePersistenceContract, GuiValidatedForm,
};
pub use parity_page_store::{ParityReportPageSource, ParityReportPageStore};
pub use pc_allspin_result::{PcAllSpinResultReport, PcAllSpinWitness};
pub use pc_chance_probability_result::{
    PcChanceIngressOrigin, PcChanceProblemPreset, PcChanceQuerySnapshot,
    PcProbabilityCompletenessEvidence, PcProbabilityV2Result,
};
pub use pc_failed_queue_result::{
    PcFailedQueueIngressOrigin, PcFailedQueueProblemPreset, PcFailedQueueQuerySnapshot,
    PcFailedQueueV2Example, PcFailedQueueV2MemoryEvidence, PcFailedQueueV2Result,
};
pub use pc_minimum_cover_result::{
    PcMinimalsIngressOrigin, PcMinimumCoverCompletenessEvidence, PcMinimumCoverProblemPreset,
    PcMinimumCoverQuerySnapshot, PcMinimumCoverV2Result, PC_MINIMUM_COVER_INPUT_CONTRACT,
    PC_MINIMUM_COVER_PROBLEM_CONTRACT, PC_MINIMUM_COVER_RESULT_CONTRACT,
};
pub use pc_path_result::{
    PcPathCompletenessEvidence, PcPathFamilyV2Result, PcPathIngressOrigin, PcPathProblemPreset,
    PcPathQuerySnapshot, PcPathStepV2, PcPathWitnessV2, PC_PATH_FAMILY_RESULT_CONTRACT,
    PC_PATH_ORDERING, PC_PATH_WITNESS_CONTRACT,
};
pub use pc_result_projection::{
    PcResultProjection, PC_SCORE_MAX_PATTERNS, PC_SCORE_MAX_PATTERN_BYTES,
    PC_SCORE_MAX_SOURCE_PIECES,
};
pub use pc_save_result::{
    PcBestSaveV2Result, PcBestSaveWinnerV2, PcSaveCompletenessEvidence, PcSaveExactProbability,
    PcSaveGroupV2, PcSaveGroupsV2Result, PcSaveIngressOrigin, PcSavePieceMultiset,
    PcSaveProblemPreset, PcSaveQuerySnapshot, PcSaveResultMode, PcSaveWitness,
    PC_BEST_SAVE_RESULT_CONTRACT, PC_BEST_SAVE_SCHEMA, PC_SAVE_GROUPS_RESULT_CONTRACT,
};
pub use pc_score_minimum_cover_result::{
    PcScoreEligibleCandidateV2, PcScoreEligiblePatternV2, PcScoreMinimalsIngressOrigin,
    PcScorePortfolioCompletenessEvidence, PcScorePortfolioV2Result,
    PcScorePortfolioValidationError, PC_SCORE_PORTFOLIO_RESULT_CONTRACT,
};
pub use pc_score_summary_result::{
    PcScoreCompletenessEvidence, PcScoreIngressOrigin, PcScoreProblemPreset, PcScoreQuerySnapshot,
    PcScoreSummaryV2Result,
};
pub use pc_score_winner_result::{
    PcScorePatternWinnerV1, PC_SCORE_INFORMATIONAL_ATTACK_BASIS, PC_SCORE_PATTERN_WINNER_CONTRACT,
};
pub use pc_tiling_family_result::{
    PcTilingCompletenessEvidence, PcTilingFamilyV1Result, PcTilingIngressOrigin,
    PcTilingProblemPreset, PcTilingQuerySnapshot, PC_TILING_FAMILY_RESULT_CONTRACT,
    PC_TILING_INITIAL_PAGE_LIMIT, PC_TILING_INPUT_CONTRACT,
};
pub use portfolio_alternative_store::{
    CoveragePortfolioAlternativeSet, CoveragePortfolioAlternativeStore, CoveragePortfolioPageStore,
    PortfolioAlternative, PortfolioAlternativeAdvance, PortfolioAlternativeCheckpoint,
    PortfolioAlternativeError, PortfolioAlternativePage, PortfolioAlternativeSetIdentity,
    PortfolioCandidate, PortfolioEnumerationStop, PortfolioMember, PortfolioMemberPage,
    ProductPageSourceOwner, ProductPageStore, PORTFOLIO_ALTERNATIVE_PAGE_CONTRACT,
    PORTFOLIO_ALTERNATIVE_SET_CONTRACT, PORTFOLIO_MEMBER_PAGE_CONTRACT, PORTFOLIO_MEMBER_PAGE_SIZE,
    PORTFOLIO_SNAPSHOT_CONTRACT,
};
pub use product_capability_contract::{ProductCapabilityContract, ProductCapabilityContractError};
pub use product_capability_result::{
    ProductCapabilityResourceEvidence, ProductCapabilityResult, ProductCapabilityResultKind,
};
pub use render::{AppMessage, AppRenderModel, AppResultKind, SetupRenderModel};
pub use request_profile_selection::{
    RequestProfileSelection, RequestProfileSelectionError, RequestStructuralProfiles,
};
pub use search_backend_warmup::{prewarm_search_backend, GpuSearchWarmupReport};
pub use setup_ranked_family_result::{
    setup_ranked_candidate_id, SetupRankedCandidateIdentity, SetupRankedFamilyResult,
    SetupRankedFamilyResultError, SetupRankedFamilySnapshot,
};
pub use setup_ranking_contract::{
    SetupRankingContract, SetupRankingContractError, SetupRankingIdentities, SetupRankingKind,
};
pub use setup_ranking_facade::SetupRankingFacade;
pub use setup_score_document::{
    SetupScoreDocumentCandidateV1, SetupScoreDocumentError, SetupScoreDocumentV1,
};
pub use spin_structure_search_result::{
    spin_structure_search_candidate_id, SpinStructureSearchCandidateIdentity,
    SpinStructureSearchIdentities, SpinStructureSearchResult, SpinStructureSearchResultError,
};
pub use tablebase_runtime::{AppTablebaseInstallError, AppTablebaseSession};
pub use typed_document_utility::{
    TypedFieldDocument, TypedFieldDocumentError, FIELD_DOCUMENT_MAX_INPUT_BYTES,
    FIELD_DOCUMENT_MAX_PAGES,
};

#[cfg(test)]
#[path = "product_capability_contract_tests.rs"]
mod product_capability_contract_tests;
