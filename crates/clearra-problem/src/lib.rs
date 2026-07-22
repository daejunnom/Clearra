//! Canonical problem layer between validated Rust queries and the core executor.

pub mod compile;
pub mod extended_pc_search_contract;
pub mod goal;
pub mod preset;
pub mod query;
pub mod search_problem;
mod search_problem_fields;

pub use compile::{
    PackingProblemCompiler, PackingProblemKind, PackingProblemSpec, ProblemCompileError,
    ProblemCompiler, SpinTargetCompiler,
};
pub use extended_pc_search_contract::{ExtendedPcSearchContract, ExtendedPcSearchContractError};
pub use goal::{
    spin_target_requires_score_profile, BuildTemplateGoal, CompositeGoal, RequiredClearKind,
    RequiredClearLines, RequiredSpinKind, SearchGoal, SpinMiniPolicy, SpinPieceSelector,
    SpinTargetRequest,
};
pub use preset::{
    BuildPreset, ContinuationPreset, OpeningPreset, OpeningPresetError, ScenarioPreset,
    SetupPostPcPreset, SetupPreset,
};
pub use query::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFieldError,
    BuildProbabilityQuery, BuildProblemLimits, BuildQuery, BuildTemplateBridge, GroupingMode,
    PcQuery, PieceBudget, PieceBudgetError, ScenarioQuery, ScenarioQuerySource, SetupHoldPolicy,
    SetupLimits, SetupLimitsError, SetupProbabilityFilter, SetupProbabilityFilterError,
    SetupQueueInput, SetupSearchQuery, SpinTargetBaseQuery, SpinTargetQuery, SpinTargetQuerySource,
    SpinTargetTraceRequirement,
};
pub use search_problem::{
    BackendPolicy, ContinuationPolicy, CountPolicy, ExactTargetPolicy, HoldAutomatonState,
    KickProfile, OccupancyField, PieceSource, ResourceBudget, RuleProfileSelection,
    SearchOutputPolicy, SearchProblem, SearchProblemBoard, SearchProblemBudget, SearchProblemId,
    SearchProblemKind, SearchProblemPreset, SearchReplayTracePolicy, SupplyProvenance, TracePolicy,
};
