use super::{
    parse_option_value::{option_value, unknown_option},
    CliHelpTopic, CliParseError, ParsedCliCommand, SetupArgs,
};

pub(crate) fn parse_setup(args: &[String]) -> Result<ParsedCliCommand, CliParseError> {
    let mut queue = String::new();
    let mut fixed_queue = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--queue" | "-q" => {
                queue = option_value(args, index, "--queue")?.to_owned();
                index += 2;
            }
            "--fixed" | "--fixed-queue" => {
                fixed_queue = true;
                index += 1;
            }
            "--observed" | "--observed-queue" => {
                fixed_queue = false;
                index += 1;
            }
            "--help" | "-h" => return Ok(ParsedCliCommand::Help(CliHelpTopic::Setup)),
            option => return Err(unknown_option("setup", option)),
        }
    }

    Ok(ParsedCliCommand::Setup(SetupArgs::new(queue, fixed_queue)))
}
