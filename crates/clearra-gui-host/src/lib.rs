//! Typed host boundary for the Tauri desktop product.
//!
//! The crate converts desktop form state into `clearra-app` requests, executes
//! them through the application boundary, and exposes owned response/job models.

pub mod desktop_host;
pub mod display;
pub mod host_language_resolver;
pub mod job;
pub mod model;
pub mod request;
pub mod settings;
pub mod validation;

pub use desktop_host::{
    prewarm_search_backend, DesktopTauriCommandBridge, DesktopTauriCommandError,
};
pub use display::{
    FumenCopyButtonModel, FumenOutputView, FumenPageListView, GuiBackendReportPanel,
    GuiCoveragePanel, GuiDiagnosticEntry, GuiDiagnosticEvidence, GuiDiagnosticPanel,
    GuiExportPanel, GuiFumenPanel, GuiGpuBackendChoiceView, GuiGpuBackpressureView,
    GuiGpuFallbackReasonView, GuiGpuMemoryTicketView, GuiGpuStatusViewModel, GuiGpuTrustStateView,
    GuiMemoryReportPanel, GuiReplayPanel, GuiReplayStep, GuiResultModel, GuiSolutionRow,
    GuiSolutionTable, GuiSummaryPanel, RenderCapabilityView, RenderExportView, RenderPreviewView,
    ReplayBoardSnapshot, ReplayLineClearView, ReplayPieceOwnershipView, ReplayStepView,
    ReplayTimelineView, SkinSelectorView,
};
pub use host_language_resolver::GuiHostLanguageResolver;
pub use job::{
    BackendStatus, BudgetStatus, GuiJob, GuiJobCancelHandle, GuiJobCancelToken, GuiJobEvent,
    GuiJobHandle, GuiJobProgress, GuiJobQueue, GuiJobQueueError, GuiJobQueueErrorCode,
    GuiJobResult, GuiJobRunner, GuiJobStatus, MemoryStatus,
};
pub use model::{
    GuiAppState, GuiBackendChoice, GuiBackendForm, GuiBuildCoverageForm, GuiCopyPolicy,
    GuiExecutionPhase, GuiExecutionState, GuiExportPolicy, GuiJobId, GuiOpeningPcForm,
    GuiOutputForm, GuiOutputFormat, GuiProblemForm, GuiRenderForm, GuiScenarioPcForm, GuiScreen,
    GuiSetupSearchForm, GuiUserPreferences,
};
pub use request::{
    BackendRequestBuilder, CoverRequestBuilder, GuiAppRequestBuild, GuiOutputRequestBuild,
    GuiToAppRequest, OutputRequestBuilder, PcRequestBuilder, RequestBuildError,
    RequestBuildErrorCode, ScenarioRequestBuilder, SetupRequestBuilder,
};
pub use settings::{
    BackendSettings, LanguageSettings, LoadedSettings, OutputSettings, SettingsError,
    SettingsErrorCode, SettingsModel, SettingsStore, SettingsTheme, SETTINGS_SCHEMA_VERSION,
};
pub use validation::{
    GuiBackendValidator, GuiFilePathValidator, GuiFormValidator, GuiRenderValidator,
    GuiValidationDiagnostic, GuiValidationSummary,
};
