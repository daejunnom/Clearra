use super::{
    is_positional,
    parse_option_value::{option_value, parse_usize_option, unknown_option},
    CliHelpTopic, CliParseError, ParsedCliCommand, PercentArgs, PercentQueueMode,
};

pub(crate) fn parse_percent(args: &[String]) -> Result<ParsedCliCommand, CliParseError> {
    let mut queue = String::new();
    let mut mode = PercentQueueMode::Observed;
    let mut minimum_len = None;
    let mut max_patterns = 0;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--queue" | "-q" => {
                queue = option_value(args, index, "--queue")?.to_owned();
                index += 2;
            }
            "--observed" => {
                mode = PercentQueueMode::Observed;
                index += 1;
            }
            "--bag-aligned" | "--bag" => {
                mode = PercentQueueMode::BagAligned;
                index += 1;
            }
            "--fixed" => {
                mode = PercentQueueMode::Fixed;
                index += 1;
            }
            "--min-len" | "--minimum-len" => {
                minimum_len = Some(parse_usize_option(args, index, "--min-len")?);
                index += 2;
            }
            "--max-patterns" => {
                max_patterns = parse_usize_option(args, index, "--max-patterns")?;
                index += 2;
            }
            "--help" | "-h" => return Ok(ParsedCliCommand::Help(CliHelpTopic::Percent)),
            value if is_positional(value) && queue.is_empty() => {
                queue = value.to_owned();
                index += 1;
            }
            option => return Err(unknown_option("percent", option)),
        }
    }

    Ok(ParsedCliCommand::Percent(
        PercentArgs::new(queue)
            .with_mode(mode)
            .with_minimum_len(minimum_len)
            .with_max_patterns(max_patterns),
    ))
}
