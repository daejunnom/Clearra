pub mod build_probability_app_command;
pub mod build_v2_app_command;
pub mod continue_app_command;
pub mod convert_app_command;
pub mod cover_app_command;
mod execution_error_response;
pub mod forward_search_app_command;
pub(crate) use execution_error_response::core_execution_error_response;
pub mod field_document_transform_app_command;
pub mod fumen_app_command;
pub mod inspect_unsupported_app_command;
pub mod operation_sequence_app_command;
pub mod parity_app_command;
pub mod path_app_command;
pub mod pc_app_command;
pub mod percent_app_command;
pub mod render_app_command;
pub mod rules_app_command;
mod rules_app_handlers;
mod rules_app_import_export;
mod rules_app_output;
pub mod scenario_app_command;
mod scenario_app_expected;
mod scenario_app_field_policy;
pub mod scenario_app_render_contract;
mod scenario_app_validation_fields;
pub mod scoring_app_command;
pub mod sequence_dependencies_app_command;
pub mod setup_app_command;
pub mod setup_score_app_command;
pub mod spin_structure_app_command;
pub mod verify_app_command;

pub use build_probability_app_command::{BuildProbabilityAppCommand, BuildProbabilityResultMode};
pub use build_v2_app_command::{BuildV2AppCommand, BuildV2AppRequest};
pub use continue_app_command::ContinueAppCommand;
pub use convert_app_command::ConvertAppCommand;
pub use cover_app_command::CoverAppCommand;
pub use field_document_transform_app_command::{
    FieldDocumentTransformAppCommand, FieldDocumentTransformAppCommandError,
    FieldDocumentTransformKind,
};
pub use forward_search_app_command::{DamageAppCommand, RenAppCommand, SpinFinderAppCommand};
pub use fumen_app_command::{FumenAppCommand, FumenAppCommandError, FumenTransformKind};
pub use inspect_unsupported_app_command::InspectUnsupportedAppCommand;
pub use operation_sequence_app_command::OperationSequenceAppCommand;
pub use parity_app_command::ParityAppCommand;
pub use path_app_command::PathAppCommand;
pub use pc_app_command::PcAppCommand;
pub use percent_app_command::PercentAppCommand;
pub use render_app_command::{RenderAppCommand, RenderAppCommandError, RenderArtifactFormat};
pub use rules_app_command::RulesAppCommand;
pub use scenario_app_command::ScenarioAppCommand;
pub use scenario_app_expected::ScenarioAppExpected;
pub use scenario_app_render_contract::ScenarioAppRenderContract;
pub use scoring_app_command::ScoringAppCommand;
pub use sequence_dependencies_app_command::SequenceDependenciesAppCommand;
pub use setup_app_command::SetupAppCommand;
pub use setup_score_app_command::{
    SetupScoreAppCommand, SetupScoreAppCommandError, SETUP_SCORE_INPUT_CONTRACT,
    SETUP_SCORE_PROBLEM_CONTRACT, SETUP_SCORE_RESULT_CONTRACT,
};
pub use spin_structure_app_command::{SpinStructureAppCommand, SpinStructureProductMode};
pub use verify_app_command::VerifyAppCommand;

pub(crate) fn string_field(
    key: impl Into<String>,
    value: impl Into<String>,
) -> clearra_output::model::RenderField {
    clearra_output::model::RenderField::new(key, value.into())
}

pub(crate) fn bool_field(
    key: impl Into<String>,
    value: bool,
) -> clearra_output::model::RenderField {
    clearra_output::model::RenderField::new(key, value)
}

pub(crate) fn number_field(
    key: impl Into<String>,
    value: impl ToString,
) -> clearra_output::model::RenderField {
    clearra_output::model::RenderField::new(
        key,
        clearra_output::model::RenderFieldValue::number(value.to_string()),
    )
}
