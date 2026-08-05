pub mod build_probability_app_command;
pub mod continue_app_command;
pub mod convert_app_command;
pub mod cover_app_command;
mod execution_error_response;
pub mod forward_search_app_command;
pub(crate) use execution_error_response::core_execution_error_response;
pub mod inspect_unsupported_app_command;
pub mod path_app_command;
pub mod pc_app_command;
pub mod percent_app_command;
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
pub mod setup_app_command;
pub mod spin_structure_app_command;
pub mod verify_app_command;

pub use build_probability_app_command::BuildProbabilityAppCommand;
pub use continue_app_command::ContinueAppCommand;
pub use convert_app_command::ConvertAppCommand;
pub use cover_app_command::CoverAppCommand;
pub use forward_search_app_command::{DamageAppCommand, SpinFinderAppCommand};
pub use inspect_unsupported_app_command::InspectUnsupportedAppCommand;
pub use path_app_command::PathAppCommand;
pub use pc_app_command::PcAppCommand;
pub use percent_app_command::PercentAppCommand;
pub use rules_app_command::RulesAppCommand;
pub use scenario_app_command::ScenarioAppCommand;
pub use scenario_app_expected::ScenarioAppExpected;
pub use scenario_app_render_contract::ScenarioAppRenderContract;
pub use scoring_app_command::ScoringAppCommand;
pub use setup_app_command::SetupAppCommand;
pub use spin_structure_app_command::SpinStructureAppCommand;
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
