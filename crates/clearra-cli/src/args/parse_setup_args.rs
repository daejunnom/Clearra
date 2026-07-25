use super::{
    parse_option_value::{option_value, unknown_option},
    CliHelpTopic, CliParseError, ParsedCliCommand, SetupArgs,
};

pub(crate) fn parse_setup(args: &[String]) -> Result<ParsedCliCommand, CliParseError> {
    let mut remaining = "IOTSZJL".to_owned();
    let mut allow_post_cycle_borrow = false;
    let mut candidate_priority = clearra_setup_search::query::SetupCandidatePriority::All;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--remaining" | "--queue" | "-q" => {
                remaining = option_value(args, index, "--remaining")?.to_owned();
                index += 2;
            }
            "--allow-post-cycle-borrow" => {
                allow_post_cycle_borrow = true;
                index += 1;
            }
            "--priority" => {
                let value = option_value(args, index, "--priority")?;
                candidate_priority =
                    clearra_setup_search::query::SetupCandidatePriority::from_keyword(value)
                        .ok_or_else(|| CliParseError::InvalidValue {
                            option: "--priority",
                            value: value.to_owned(),
                        })?;
                index += 2;
            }
            "--help" | "-h" => return Ok(ParsedCliCommand::Help(CliHelpTopic::Setup)),
            option => return Err(unknown_option("setup", option)),
        }
    }

    Ok(ParsedCliCommand::Setup(
        SetupArgs::new(remaining, allow_post_cycle_borrow)
            .with_candidate_priority(candidate_priority),
    ))
}
