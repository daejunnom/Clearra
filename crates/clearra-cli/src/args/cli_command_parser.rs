use super::{
    parse_continue_args::parse_continue, parse_convert_args::parse_convert,
    parse_cover_args::parse_cover, parse_path_args::parse_path, parse_pc_args::parse_pc,
    parse_pc_scenario_args::parse_pc_scenario, parse_percent_args::parse_percent,
    parse_rules_args::parse_rules, parse_scoring_args::parse_scoring,
    parse_setup_args::parse_setup, parse_verify_args::parse_verify, CliHelpTopic, CliParseError,
    ParsedCliCommand,
};

pub(crate) fn parse_command(
    command: &str,
    command_args: &[String],
) -> Result<ParsedCliCommand, CliParseError> {
    match command {
        "pc" => parse_pc(command_args),
        "pc-scenario" => parse_pc_scenario(command_args),
        "path" => parse_path(command_args),
        "percent" => parse_percent(command_args),
        "setup" => parse_setup(command_args),
        "cover" => parse_cover(command_args),
        "rules" => parse_rules(command_args),
        "scoring" => parse_scoring(command_args),
        "convert" => parse_convert(command_args),
        "continue" => parse_continue(command_args),
        "verify" => parse_verify(command_args),
        "help" | "--help" | "-h" => Ok(ParsedCliCommand::Help(CliHelpTopic::TopLevel)),
        "inspect" => Ok(ParsedCliCommand::Unsupported(command.to_owned())),
        _ => Err(CliParseError::UnknownCommand {
            command: command.to_owned(),
        }),
    }
}
