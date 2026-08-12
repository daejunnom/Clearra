//! Typed application facade shared by CLI, GUI, and downstream Clearra apps.

pub mod app_command;
pub mod app_context;
pub mod app_error;
pub mod app_request;
pub mod app_response;
pub mod app_services;
pub mod cli_compat;
pub mod commands;
mod cooperative_execution;
pub mod diagnostics;
mod distributed_forward_execution;
mod distributed_search_execution;
mod distributed_setup_execution;
mod execution_constraint_postprocess;
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
pub mod render;
pub mod request;
mod resource_contract;
pub mod response;
pub mod run_request;
pub mod search_backend_warmup;
mod search_output_surface_postprocess;
pub mod tablebase_runtime;

pub use app_command::{AppCommand, RunnableAppCommand};
pub use app_context::{AppContext, AppExecutionContext};
pub use app_error::{AppError, AppErrorCode};
pub use app_request::{AppOutputPolicy, AppRequest};
pub use app_response::{AppEffect, AppResponse, AppStatus, ExitCodeHint};
pub use app_services::{
    AppClock, AppCoreExecutorService, AppDiagnosticSink, AppLanguageResolverService, AppServices,
};
pub use clearra_core_domain::execution_cancellation::{
    CancellationHandle, CancellationToken, ExecutionCancellationHandle, ExecutionCancellationToken,
    ExecutionControl, ExecutionPartition, ExecutionProgress, ProgressSink,
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
    DiagnosticsPolicy, LocalePolicy, OutputPolicy, QueryEnvelope, ResourceBudget, ResourceReport,
};
pub use commands::{
    BuildProbabilityAppCommand, ContinueAppCommand, ConvertAppCommand, CoverAppCommand,
    DamageAppCommand, InspectUnsupportedAppCommand, PathAppCommand, PcAppCommand,
    PercentAppCommand, RulesAppCommand, ScenarioAppCommand, ScenarioAppExpected,
    ScenarioAppRenderContract, ScoringAppCommand, SetupAppCommand, SpinFinderAppCommand,
    SpinStructureAppCommand, VerifyAppCommand,
};
pub use cooperative_execution::{CooperativeAppAdvance, CooperativeAppExecution};
pub use distributed_forward_execution::{
    DistributedForwardPreparation, PreparedDistributedForwardSearch,
};
pub use distributed_search_execution::{DistributedSearchPreparation, PreparedDistributedSearch};
pub use distributed_setup_execution::{
    DistributedSetupPreparation, PreparedDistributedSetupSearch,
};
pub use gui_bridge::{
    GuiAppRequestPreview, GuiBackendCapabilityView, GuiBridgeError, GuiBridgeErrorCode,
    GuiCommandPreview, GuiDisabledReason, GuiFormState, GuiFormValidation, GuiGpuBackendOptionView,
    GuiStatePersistenceContract, GuiValidatedForm,
};
pub use render::{AppMessage, AppRenderModel, AppResultKind};
pub use search_backend_warmup::{prewarm_search_backend, GpuSearchWarmupReport};
pub use tablebase_runtime::{AppTablebaseInstallError, AppTablebaseSession};
