use super::{
    parse_option_value::{option_value, unknown_option},
    CliHelpTopic, CliParseError, CoverArgs, ParsedCliCommand,
};

pub(crate) fn parse_cover(args: &[String]) -> Result<ParsedCliCommand, CliParseError> {
    let mut template = None;
    let mut template_json = None;
    let mut template_file = None;
    let mut export_template_json = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--template" | "-t" => {
                template = Some(option_value(args, index, "--template")?.to_owned());
                index += 2;
            }
            "--template-json" => {
                template_json = Some(option_value(args, index, "--template-json")?.to_owned());
                index += 2;
            }
            "--template-file" => {
                template_file = Some(option_value(args, index, "--template-file")?.to_owned());
                index += 2;
            }
            "--export-template-json" => {
                export_template_json = true;
                index += 1;
            }
            "--help" | "-h" => return Ok(ParsedCliCommand::Help(CliHelpTopic::Cover)),
            option => return Err(unknown_option("cover", option)),
        }
    }

    Ok(ParsedCliCommand::Cover(
        CoverArgs::new(template)
            .with_template_json(template_json)
            .with_template_file(template_file)
            .with_export_template_json(export_template_json),
    ))
}
