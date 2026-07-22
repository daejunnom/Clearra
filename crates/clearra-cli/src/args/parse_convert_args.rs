use super::{
    is_positional,
    parse_option_value::{option_value, unknown_option},
    CliHelpTopic, CliParseError, ConvertArgs, ParsedCliCommand,
};

pub(crate) fn parse_convert(args: &[String]) -> Result<ParsedCliCommand, CliParseError> {
    let mut input = None;
    let mut from = None;
    let mut to = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--input" | "-i" => {
                input = Some(option_value(args, index, "--input")?.to_owned());
                index += 2;
            }
            "--from" => {
                from = Some(option_value(args, index, "--from")?.to_owned());
                index += 2;
            }
            "--to" => {
                to = Some(option_value(args, index, "--to")?.to_owned());
                index += 2;
            }
            "--help" | "-h" => return Ok(ParsedCliCommand::Help(CliHelpTopic::Convert)),
            value if is_positional(value) && input.is_none() => {
                input = Some(value.to_owned());
                index += 1;
            }
            option => return Err(unknown_option("convert", option)),
        }
    }

    Ok(ParsedCliCommand::Convert(ConvertArgs::new(input, from, to)))
}
