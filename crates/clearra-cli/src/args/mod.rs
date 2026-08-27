mod backend_fallback_override;
pub mod cli_args;
mod cli_command_parser;
pub mod cli_parser;
pub mod continue_args;
pub mod convert_args;
pub mod cover_args;
mod execution_backend_aliases;
pub mod failed_queue_args;
pub mod inspect_args;
mod parse_continue_args;
mod parse_convert_args;
mod parse_cover_args;
mod parse_failed_queue_args;
mod parse_option_value;
mod parse_path_args;
mod parse_pc_args;
mod parse_pc_scenario_args;
mod parse_percent_args;
mod parse_piece_arg;
mod parse_rules_args;
mod parse_scoring_args;
mod parse_setup_args;
mod parse_verify_args;
pub mod path_args;
pub mod pc_args;
pub mod pc_scenario_args;
mod pc_scenario_field_args;
mod pc_scenario_output_args;
mod pc_scenario_queue_args;
mod pc_scenario_rule_args;
pub mod percent_args;
pub mod rules_args;
pub mod scoring_args;
pub mod setup_args;
pub mod verify_args;

pub use cli_args::{CliArgs, CliCommand};
pub use cli_parser::{
    CliHelpTopic, CliParseError, CliParser, ExplicitTieOptions, ParsedCliCommand,
    ParsedCliInvocation, ProductHelpTopic,
};
pub use continue_args::ContinueArgs;
pub use convert_args::ConvertArgs;
pub use cover_args::CoverArgs;
pub use failed_queue_args::FailedQueueArgs;
pub use inspect_args::InspectArgs;
pub use path_args::PathArgs;
pub use pc_args::PcArgs;
pub use pc_scenario_args::PcScenarioArgs;
pub use percent_args::{PercentArgs, PercentQueueMode};
pub use rules_args::{RulesAction, RulesArgs};
pub use scoring_args::{ScoringAction, ScoringArgs};
pub use setup_args::SetupArgs;
pub use verify_args::VerifyArgs;

pub(crate) fn has_help(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--help" || arg == "-h")
}

pub(crate) fn is_positional(value: &str) -> bool {
    !value.starts_with('-')
}
