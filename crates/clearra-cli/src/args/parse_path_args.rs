use super::{
    has_help, parse_pc_args::parse_pc_args, CliHelpTopic, CliParseError, ParsedCliCommand, PathArgs,
};

pub(crate) fn parse_path(args: &[String]) -> Result<ParsedCliCommand, CliParseError> {
    if has_help(args) {
        return Ok(ParsedCliCommand::Help(CliHelpTopic::Path));
    }
    parse_pc_args(args).map(|pc| ParsedCliCommand::Path(PathArgs::new(pc)))
}
