use super::{
    has_help,
    parse_option_value::{option_value, parse_usize_option, unknown_option},
    parse_pc_args::parse_pc_args,
    CliHelpTopic, CliParseError, FailedQueueArgs, ParsedCliCommand,
};

pub(crate) fn parse_failed_queue(args: &[String]) -> Result<ParsedCliCommand, CliParseError> {
    if has_help(args) {
        return Ok(ParsedCliCommand::Help(CliHelpTopic::FailedQueue));
    }

    let mut failed_pattern_limit = usize::MAX;
    let mut patterns = None;
    let mut pc_args = Vec::with_capacity(args.len());
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--failed-count" | "--limit" => {
                failed_pattern_limit = parse_usize_option(args, index, "--failed-count")?;
                index += 2;
            }
            "--patterns" | "--pattern" => {
                patterns = Some(option_value(args, index, "--patterns")?.to_owned());
                index += 2;
            }
            "--objective"
            | "--tiling-only"
            | "--score"
            | "--score-profile"
            | "--spin-profile"
            | "--initial-b2b"
            | "--solution-probabilities" => {
                return Err(CliParseError::InvalidValue {
                    option: "failed-queue",
                    value: format!("{} is not available for failed-queue search", args[index]),
                });
            }
            option if option.starts_with("--failed-") => {
                return Err(unknown_option("failed-queue", option));
            }
            _ => {
                pc_args.push(args[index].clone());
                index += 1;
            }
        }
    }

    let pc = parse_pc_args(&pc_args)?.with_objective("all");
    if patterns.is_some() && !pc.queue().trim().is_empty() {
        return Err(CliParseError::InvalidValue {
            option: "--patterns",
            value: "cannot be combined with --queue".to_owned(),
        });
    }
    Ok(ParsedCliCommand::FailedQueue(FailedQueueArgs::new(
        pc,
        patterns,
        failed_pattern_limit,
    )))
}
