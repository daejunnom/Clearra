use super::{
    is_positional,
    parse_option_value::{option_value, unknown_option},
    CliHelpTopic, CliParseError, ParsedCliCommand, ScoringAction, ScoringArgs,
};

pub(crate) fn parse_scoring(args: &[String]) -> Result<ParsedCliCommand, CliParseError> {
    let mut action = ScoringAction::List;
    let mut profile = None;
    let mut input = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "list" | "inspect" | "import" | "export" if index == 0 => {
                action = ScoringAction::parse(args[index].as_str()).expect("known action");
                index += 1;
            }
            "--profile" | "-p" => {
                profile = Some(option_value(args, index, "--profile")?.to_owned());
                index += 2;
            }
            "--input" | "-i" => {
                input = Some(option_value(args, index, "--input")?.to_owned());
                index += 2;
            }
            "--help" | "-h" => return Ok(ParsedCliCommand::Help(CliHelpTopic::Scoring)),
            value if is_positional(value) && profile.is_none() => {
                profile = Some(value.to_owned());
                index += 1;
            }
            option => return Err(unknown_option("scoring", option)),
        }
    }

    Ok(ParsedCliCommand::Scoring(
        ScoringArgs::new(action)
            .with_profile(profile)
            .with_input(input),
    ))
}
