use super::{
    parse_option_value::{option_value, unknown_option},
    CliHelpTopic, CliParseError, ParsedCliCommand, VerifyArgs,
};

pub(crate) fn parse_verify(args: &[String]) -> Result<ParsedCliCommand, CliParseError> {
    let mut target = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--target" | "-t" => {
                target = Some(option_value(args, index, "--target")?.to_owned());
                index += 2;
            }
            "pc" | "setup" | "cover" | "build" | "kicks" if target.is_none() => {
                target = Some(args[index].clone());
                index += 1;
            }
            "--help" | "-h" => return Ok(ParsedCliCommand::Help(CliHelpTopic::Verify)),
            option => return Err(unknown_option("verify", option)),
        }
    }

    Ok(ParsedCliCommand::Verify(VerifyArgs::new(target)))
}
