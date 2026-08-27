//! Typed product boundary shared by CLI, GUI, WASM, and desktop hosts.
//!
//! This crate intentionally owns only serializable contract shapes. It does not
//! depend on `clearra-app`, solver crates, CLI code, native process APIs, or C
//! FFI bindings.

mod app_command_kind;
mod app_contract;
mod app_result;
mod backend_policy;
mod backend_report;
mod build_v2_product_payload;
mod capability_report;
mod continuation_report;
mod diagnostics_policy;
mod document_utility_payload;
mod execution_availability;
mod job_event;
mod locale_policy;
mod output_policy;
mod product_build_identity;
mod product_result_payload;
mod query_envelope;
mod render_capability_report;
mod resource_budget;
mod resource_report;
mod solution_set_artifact_payload;

pub use app_command_kind::AppCommandKind;
pub use app_contract::{AppRequest, AppResponse, AppStatus, Diagnostic, DiagnosticReport};
pub use app_result::AppResult;
pub use backend_policy::BackendPolicy;
pub use backend_report::BackendReport;
pub use build_v2_product_payload::{
    BuildV2CandidateCoveragePayload, BuildV2CompletenessPayload, BuildV2PayloadKind,
    BuildV2ProductPayload, BuildV2ProductPayloadError, BuildV2ScoreWinnerPayload,
};
pub use capability_report::CapabilityReport;
pub use continuation_report::ContinuationReport;
pub use diagnostics_policy::DiagnosticsPolicy;
pub use document_utility_payload::{
    FieldDocumentPayload, FieldDocumentSetPayload, ParityReportPagePayload, RenderArtifactPayload,
};
pub use execution_availability::{
    ExecutionAvailabilityReason, ExecutionAvailabilityReport, ExecutionAvailabilityState,
    ExecutionCompletenessState, ExecutionSurface,
};
pub use job_event::{
    BackendStatusReport, CancelledReport, DiagnosticEvent, JobEvent, JobProgress, JobStarted,
    PartialResult,
};
pub use locale_policy::LocalePolicy;
pub use output_policy::OutputPolicy;
pub use product_build_identity::{
    ProductBuildIdentity, ARTIFACT_SCHEMA_VERSION, COMPILED_ENGINE_BUILD_ID,
    COMPILED_SOURCE_COMMIT, CONTRACT_SCHEMA_VERSION, SUPPLY_SEMANTICS_ID, UNVERIFIED_LOCAL_BUILD,
};
pub use product_result_payload::{
    BuildCoverageCompletenessPayload, BuildCoveragePortfolioPayloadError,
    BuildCoveragePortfolioV2Payload, BuildSetupCandidateCoverageV1Payload,
    BuildSetupCompletenessPayload, BuildSetupFamilyPayloadError, BuildSetupFamilyV1Payload,
    CoveragePortfolioPagePayload, PcBestSavePayload, PcBestSaveWinnerPayload, PcPathFamilyPayload,
    PcPathStepPayload, PcPathWitnessPayload, PcSaveCompletenessPayload, PcSaveGroupPayload,
    PcSaveGroupsPayload, PcSavePieceMultisetPayload, PcSaveRunMetadataPayload,
    PcSaveWitnessPayload, ProductCandidateMemberPayload, ProductResultPayload,
    ProductResultPayloadContent, RankedFamilyPayloadError, ScorePatternWinnerFamilyPayload,
    ScorePatternWinnerPayload, SetupRankedCandidatePayload, SetupRankedFamilyPayload,
    SetupScoreCandidatePayload, SetupScoreRankingPayload, SetupScoreRankingPayloadError,
    SpinStructureCandidatePayload, SpinStructureFamilyPayload,
};
pub use query_envelope::QueryEnvelope;
pub use render_capability_report::RenderCapabilityReport;
pub use resource_budget::ResourceBudget;
pub use resource_report::ResourceReport;
pub use solution_set_artifact_payload::{
    SolutionSetArtifactFormatPayload, SolutionSetArtifactPayload, SolutionSetArtifactPayloadError,
    HOST_SOLUTION_SET_ARTIFACT_MAX_BYTES, SOLUTION_SET_ARTIFACT_CONTRACT,
};
