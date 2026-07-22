//! UI schema contracts for Clearra editors and explorers.

pub mod build_editor;
pub mod capability;
pub mod disabled_reason;
pub mod dropdown;
pub mod gui_editor_schema_v2;
pub mod i18n;
pub mod render;
pub mod rule_editor;
pub mod schema_snapshot;
pub mod score_editor;
pub mod setup_explorer;

pub use build_editor::{
    BuildCellSchema, BuildCoverageSummarySchema, BuildEditorSchema, BuildFieldSchema,
    BuildFieldType, BuildPreviewBoardSchema, BuildSlotSchema, BuildValidationDiagnosticSchema,
};
pub use capability::{CapabilityReportEntrySchema, CapabilityState};
pub use disabled_reason::UiDisabledReason;
pub use dropdown::{DropdownOption, ProfileDropdowns};
pub use gui_editor_schema_v2::{
    DiagnosticPanelSchema, GuiContractFieldSchema, GuiEditorSchemaV2, RenderOptionsSchema,
};
pub use i18n::{LanguageOptionSchema, LanguageSelectorSchema, LocalizedLabelSchema};
pub use render::{CustomSkinThemeEditorFieldSchema, CustomSkinThemeEditorSchema};
pub use rule_editor::{
    CustomRuleEditorSchema, CustomRuleEditorSectionSchema, KickTableEditorSchema,
    KickTableImportExportSchema, KickTablePreviewSchema, KickTableVerificationSchema,
    RuleEditorSchema,
};
pub type ScoreEditorSchema = ScoreProfileEditorSchema;
pub use schema_snapshot::{UiSchemaSnapshot, UI_SCHEMA_SNAPSHOT_VERSION};
pub use score_editor::{
    ScoreEvaluationScopeOptionSchema, ScoreExpectationSchema, ScoreProfileEditorSchema,
    ScoreProfileImportExportSchema, ScoringFieldSchema, ScoringFieldType,
    SpecialSpinCaseOptionSchema, SpecialSpinCaseSchema, SpinClassifierOptionSchema,
    SpinClassifierSchema, SpinTargetOptionSchema, SpinTargetSchema,
};
pub use setup_explorer::{
    BackendOptionsSchema, BackendPresetSchema, ExecutionOptionsSchema, ProblemPresetOptionSchema,
    ProblemPresetOptionsSchema, ScenarioEditorFieldSchema, ScenarioEditorFieldType,
    ScenarioEditorSchema, SetupExplorerSchema, SetupFilterSchema, SetupResultColumnSchema,
    SetupResultColumnSource, SetupResultColumnType, SpinProbabilityColumnSchema,
    SpinTargetFilterSchema,
};
