use crate::{
    args::CliParser,
    cli_routing::route_invocation,
    output::{CliOutput, CliOutputDispatcher},
};

pub fn run() -> i32 {
    CliOutputDispatcher::dispatch(&run_with_args(std::env::args()))
}

pub fn run_with_args<I, S>(args: I) -> CliOutput
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<String>>();
    let selected_language = args
        .windows(2)
        .find(|pair| pair[0] == "--lang")
        .and_then(|pair| clearra_i18n::LanguageId::parse(&pair[1]));
    let error_language = clearra_i18n::LanguageResolver::resolve_from_selected(selected_language);
    match CliParser::parse(args) {
        Ok(invocation) => route_invocation(invocation),
        Err(error) => error.into_output().localized_for(error_language),
    }
}

#[cfg(test)]
mod japanese_i18n_tests {
    use super::*;

    #[test]
    fn released_japanese_localizes_help_and_parse_failures() {
        let help = run_with_args(["clearra", "--lang", "ja", "--help"]);
        assert!(help.stdout().contains("Clearraコマンドライン"));
        assert!(help.stdout().contains("使い方: clearra"));
        assert!(!help.stdout().contains("usage: clearra"));

        let invalid = run_with_args(["clearra", "--lang", "ja", "--format", "invalid", "--help"]);
        assert!(invalid.stderr().starts_with("error "));
        assert!(invalid.stderr().contains("出力形式"));
        assert!(!invalid.stderr().contains("output format"));
    }
}

#[cfg(test)]
#[path = "cli_entry_sequence_dependencies_tests.rs"]
mod sequence_dependencies_tests;

#[cfg(test)]
#[path = "cli_entry_document_utility_tests.rs"]
mod document_utility_tests;

#[cfg(all(test, feature = "native-c-core"))]
#[path = "cli_entry_tests.rs"]
mod tests;
