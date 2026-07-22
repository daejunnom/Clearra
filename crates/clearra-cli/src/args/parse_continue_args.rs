use super::{
    is_positional,
    parse_option_value::{option_value, unknown_option},
    CliHelpTopic, CliParseError, ContinueArgs, ParsedCliCommand,
};

pub(crate) fn parse_continue(args: &[String]) -> Result<ParsedCliCommand, CliParseError> {
    let mut token = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--token" | "-t" => {
                token = Some(option_value(args, index, "--token")?.to_owned());
                index += 2;
            }
            "--help" | "-h" => return Ok(ParsedCliCommand::Help(CliHelpTopic::Continue)),
            value if is_positional(value) && token.is_none() => {
                token = Some(value.to_owned());
                index += 1;
            }
            option => return Err(unknown_option("continue", option)),
        }
    }

    Ok(ParsedCliCommand::Continue(ContinueArgs::new(token)))
}
