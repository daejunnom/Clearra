use super::{
    parse_continue_args::parse_continue, parse_convert_args::parse_convert,
    parse_cover_args::parse_cover, parse_failed_queue_args::parse_failed_queue,
    parse_path_args::parse_path, parse_pc_args::parse_pc,
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
        "pc-replay" | "path" => parse_path(command_args),
        "percent" => parse_percent(command_args),
        "failed-queue" | "failed_queue" => parse_failed_queue(command_args),
        "setup-finder" | "setup" => parse_setup(command_args),
        "build-coverage" | "cover" => parse_cover(command_args),
        "rules" => parse_rules(command_args),
        "scoring" => parse_scoring(command_args),
        "convert" => parse_convert(command_args),
        "continue" => parse_continue(command_args),
        "verify" => parse_verify(command_args),
        "spin-structure" if has_help(command_args) => {
            Ok(ParsedCliCommand::Help(CliHelpTopic::SpinStructure))
        }
        "build-probability" | "finesse" | "damage" | "spin-finder" | "spin-structure"
        | "chance" | "minimals" | "score" | "special-minimals" | "special_minimals"
        | "special-cover" | "special_cover" | "score-minimals" | "score_minimals" | "saves"
        | "best-save" | "best_save" | "score-finder" | "score_finder" | "spin-cover"
        | "spincover" | "setup-cover" | "setupcover" | "congruent" | "congruent-cover"
        | "congruent_cover" | "cover-percent" | "cover_percent" | "pc-setup" | "pcsetup"
        | "best-setup" | "bestsetup" | "dpc-finder" | "dpcfinder" | "parity" | "to-gray"
        | "togray" | "to-fumen" | "tofumen" | "render"
            if has_help(command_args) =>
        {
            Ok(ParsedCliCommand::Help(CliHelpTopic::Product(
                product_help_topic(command),
            )))
        }
        "build-probability" | "finesse" | "damage" | "spin-finder" | "spin-structure"
        | "chance" | "minimals" | "score" | "special-minimals" | "special_minimals"
        | "special-cover" | "special_cover" | "score-minimals" | "score_minimals" | "saves"
        | "best-save" | "best_save" | "score-finder" | "score_finder" | "spin-cover"
        | "spincover" | "setup-cover" | "setupcover" | "congruent" | "congruent-cover"
        | "congruent_cover" | "cover-percent" | "cover_percent" | "pc-setup" | "pcsetup"
        | "best-setup" | "bestsetup" | "dpc-finder" | "dpcfinder" | "parity" | "to-gray"
        | "togray" | "to-fumen" | "tofumen" | "render" => Ok(ParsedCliCommand::Product(
            product_tokens(command, command_args),
        )),
        "sfinder"
            if command_args
                .first()
                .is_some_and(|arg| matches!(arg.as_str(), "--help" | "-h")) =>
        {
            Ok(ParsedCliCommand::Help(CliHelpTopic::Sfinder))
        }
        "sfinder" => Ok(ParsedCliCommand::Product(product_tokens(
            command,
            command_args,
        ))),
        "help" | "--help" | "-h" => Ok(ParsedCliCommand::Help(CliHelpTopic::TopLevel)),
        "inspect" => Ok(ParsedCliCommand::Unsupported(command.to_owned())),
        _ => Err(CliParseError::UnknownCommand {
            command: command.to_owned(),
        }),
    }
}

fn has_help(command_args: &[String]) -> bool {
    command_args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
}

fn product_help_topic(command: &str) -> super::ProductHelpTopic {
    use super::ProductHelpTopic;

    match command {
        "build-probability" => ProductHelpTopic::BuildProbability,
        "finesse" => ProductHelpTopic::Finesse,
        "damage" => ProductHelpTopic::Damage,
        "spin-finder" => ProductHelpTopic::SpinFinder,
        _ => ProductHelpTopic::MappedCompatibility,
    }
}

fn product_tokens(command: &str, command_args: &[String]) -> Vec<String> {
    let mut tokens = Vec::with_capacity(command_args.len() + 2);
    tokens.push("clearra".to_owned());
    tokens.push(command.to_owned());
    tokens.extend_from_slice(command_args);
    tokens
}
