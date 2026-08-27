use super::{
    parse_option_value::{option_value, unknown_option},
    CliParseError, ParsedCliCommand, VerifyArgs,
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
            "--help" | "-h" => return Err(unknown_option("verify", args[index].as_str())),
            option => return Err(unknown_option("verify", option)),
        }
    }

    Ok(ParsedCliCommand::Verify(VerifyArgs::new(target)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_i18n::LanguageId;

    #[test]
    fn exact_hidden_verify_syntax_still_routes() {
        let parsed = parse_verify(&["pc".to_owned()]).expect("known hidden verify syntax");
        let ParsedCliCommand::Verify(args) = parsed else {
            panic!("expected hidden verify command");
        };
        assert_eq!(args.target(), Some("pc"));
    }

    #[test]
    fn verify_help_is_not_a_discovery_surface() {
        assert!(matches!(
            parse_verify(&["--help".to_owned()]),
            Err(CliParseError::UnknownOption { command, option })
                if command == "verify" && option == "--help"
        ));
        assert!(matches!(
            parse_verify(&["-h".to_owned()]),
            Err(CliParseError::UnknownOption { command, option })
                if command == "verify" && option == "-h"
        ));
    }

    #[test]
    fn published_help_topics_do_not_name_the_hidden_diagnostic() {
        for topic in [
            super::super::CliHelpTopic::TopLevel,
            super::super::CliHelpTopic::Sfinder,
        ] {
            let output = topic.into_output(LanguageId::En);
            assert!(!output.stdout().contains("verify"), "{}", output.stdout());
            assert!(
                !output.stdout().contains("--diagnostics"),
                "{}",
                output.stdout()
            );
        }
    }
}
