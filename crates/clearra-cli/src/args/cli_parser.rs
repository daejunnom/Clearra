use crate::{
    args::{ContinueArgs, ConvertArgs, CoverArgs, PcArgs, PcScenarioArgs, SetupArgs, VerifyArgs},
    args::{PathArgs, PercentArgs, RulesArgs, ScoringArgs},
    error::CliErrorCode,
    output::{
        CliOutput, OutputVerbosity, RenderFormat, RenderFormatSelectionError, RenderFormatSelector,
    },
};
use clearra_i18n::{LanguageId, LanguageResolver, TranslationCatalog, TranslationKey};

use super::{cli_command_parser, parse_option_value::option_value};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedCliInvocation {
    format: RenderFormat,
    language: LanguageId,
    output_verbosity: OutputVerbosity,
    verbose_paths: bool,
    command: ParsedCliCommand,
}

impl ParsedCliInvocation {
    pub fn new(format: RenderFormat, language: LanguageId, command: ParsedCliCommand) -> Self {
        Self {
            format,
            language,
            output_verbosity: OutputVerbosity::Default,
            verbose_paths: false,
            command,
        }
    }
}
impl ParsedCliInvocation {
    pub fn with_output_verbosity(mut self, output_verbosity: OutputVerbosity) -> Self {
        self.output_verbosity = output_verbosity;
        self
    }
}
impl ParsedCliInvocation {
    pub fn with_verbose_paths(mut self, verbose_paths: bool) -> Self {
        self.verbose_paths = verbose_paths;
        self
    }
}
impl ParsedCliInvocation {
    pub fn format(&self) -> RenderFormat {
        self.format
    }
}
impl ParsedCliInvocation {
    pub fn language(&self) -> LanguageId {
        self.language
    }
}
impl ParsedCliInvocation {
    pub fn output_verbosity(&self) -> OutputVerbosity {
        self.output_verbosity
    }
}
impl ParsedCliInvocation {
    pub fn verbose_paths(&self) -> bool {
        self.verbose_paths
    }
}
impl ParsedCliInvocation {
    pub fn into_command(self) -> ParsedCliCommand {
        self.command
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParsedCliCommand {
    Pc(PcArgs),
    PcScenario(PcScenarioArgs),
    Path(PathArgs),
    Percent(PercentArgs),
    Setup(SetupArgs),
    Cover(CoverArgs),
    Rules(RulesArgs),
    Scoring(ScoringArgs),
    Convert(ConvertArgs),
    Continue(ContinueArgs),
    Verify(VerifyArgs),
    Unsupported(String),
    Help(CliHelpTopic),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CliHelpTopic {
    TopLevel,
    Pc,
    PcScenario,
    Path,
    Percent,
    Setup,
    Cover,
    Rules,
    Scoring,
    Convert,
    Continue,
    Verify,
}

impl CliHelpTopic {
    pub fn into_output(self, language: LanguageId) -> CliOutput {
        let catalog = TranslationCatalog::new(language);
        let title = catalog.get_or_fallback(
            &TranslationKey::new("cli.help.top_level"),
            "Clearra command line",
        );
        CliOutput::success(format!(
            "{title}\n{}",
            match self {
                Self::TopLevel => {
                    "usage: clearra [--format text|json|fumen-like] [--lang en|ko] [--verbose] [--diagnostics] [--verbose-paths] <pc|pc-scenario|path|percent|setup|cover|rules|scoring|convert|continue|verify> [options]\nglobal options may appear before or after the command\ntry opening preset: clearra pc --lines 2"
                }
                Self::Pc => {
                    "usage: clearra pc --lines 2 [--queue IOTSZJL] [--fixed|--observed] [--hold|--no-hold] [--objective all|unique|min-cover] [--solution-probabilities] [--score] [--score-profile tetrio|guideline|jstris-ultra] [--spin-profile t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus] [--initial-b2b N] [--rule srs-plus|srs|srs-x|jstris-180|asc|ars|no-kick] [--kick-profile-json JSON] [--backend auto|cpu|gpu|hybrid] [--workers N|--cpu-threads N] [--use-all-cpu-threads] [--cpu-warmup] [--gpu-warmup] [--no-gpu] [--deterministic] [--max-candidates N] [--max-patterns N] [--max-memory-mib N] [--gpu-device auto|N] [--allow-backend-fallback|--no-backend-fallback]\ncompiles an opening preset into a SearchProblem; --solution-probabilities computes each normalized solution's pattern-union probability without reordering CLI output; --score-profile selects scoring independently from the kick rule and spin profile; guideline is fixed at level 1"
                }
                Self::PcScenario => {
                    "usage: clearra pc-scenario --fixture tests/fixtures/pc/example.json [--verify-expected] [--solution-probabilities] [--backend auto|cpu|gpu|hybrid] [--workers N|--cpu-threads N] [--use-all-cpu-threads] [--cpu-warmup] [--gpu-warmup] [--no-gpu]\n   or: clearra pc-scenario --field 0x... --queue IOTSZJ --max-pieces 6 [--solution-probabilities] [--rule srs-plus|srs|srs-x|jstris-180|asc|ars|no-kick] [--kick-profile-json JSON] [--workers N|--cpu-threads N] [--use-all-cpu-threads] [--cpu-warmup] [--gpu-warmup] [--no-gpu] [--deterministic] [--max-candidates N] [--max-patterns N] [--max-memory-mib N] [--gpu-device auto|N] [--allow-backend-fallback|--no-backend-fallback]\ncompiles a scenario preset into a SearchProblem; per-solution probability output preserves canonical CLI solution order"
                }
                Self::Path => {
                    "usage: clearra path --lines 2 [--queue IIOOO] [--fixed|--observed] [--no-hold]"
                }
                Self::Percent => {
                    "usage: clearra percent --queue IOTSZ [--observed|--bag-aligned|--fixed] [--min-len N] [--max-patterns N]"
                }
                Self::Setup => {
                    "usage: clearra setup --remaining IOTSZJL [--initial-hold empty|I|O|T|S|Z|J|L] [--mode oracle] [--next-cycle-remaining IOTS] [--rule srs-plus|srs|srs-x|jstris-180] [--priority all|build|pc] [--setup-length auto|longer|shorter] [--max-setup-pieces 1..10] [--allow-post-cycle-borrow]\n   or: clearra setup --remaining TI --mode qb --qb OS [--next-cycle-remaining OOSITZ] [--initial-hold empty|I|O|T|S|Z|J|L] [--rule srs-plus|srs|srs-x|jstris-180] [--priority all|build|pc] [--setup-length auto|longer|shorter] [--max-setup-pieces 1..10]\n--remaining is the unordered inventory before the next bag boundary, including the selected initial-hold piece. --initial-hold removes one matching piece from that inventory and starts it in hold; empty is the default. Without --initial-hold, each remaining piece kind must be unique. --qb is the observed next-bag piece group and enables queue-based search; its letter order is not a fixed draw order, and observed pieces may be used by a setup but need not all be locked. --next-cycle-remaining independently constrains the exact hold plus bag remainder left after this PC and is valid in oracle or QB mode. --rule selects the kick table used for every setup and completion BuildUp check. --max-setup-pieces defaults to 9; use 10 to include complete PC solutions. Priority all ranks joint Build x PC coverage. Setup length is independent; auto favors longer setups for all/build and shorter setups for pc"
                }
                Self::Cover => {
                    "usage: clearra cover [--template name|--template-json json|--template-file path] [--export-template-json]"
                }
                Self::Rules => {
                    "usage: clearra rules <list|inspect|verify|import|export> [--profile id] [--input json]"
                }
                Self::Scoring => {
                    "usage: clearra scoring <list|inspect|import|export> [--profile id] [--input json]"
                }
                Self::Convert => {
                    "usage: clearra convert --from fumen-like --to text|json --input <v115@...>"
                }
                Self::Continue => "usage: clearra continue <token>",
                Self::Verify => "usage: clearra verify [pc|setup|cover|kicks]",
            }
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CliParseError {
    MissingValue {
        option: &'static str,
    },
    InvalidValue {
        option: &'static str,
        value: String,
    },
    UnsupportedFormat {
        value: String,
    },
    UnknownCommand {
        command: String,
    },
    UnknownOption {
        command: &'static str,
        option: String,
    },
}

impl CliParseError {
    pub fn into_output(self) -> CliOutput {
        match self {
            Self::MissingValue { option } => CliOutput::error(
                CliErrorCode::CliMissingValue,
                format!("option '{option}' requires a value"),
            ),
            Self::InvalidValue { option, value } => CliOutput::error(
                CliErrorCode::CliInvalidValue,
                format!("option '{option}' got invalid value '{value}'"),
            ),
            Self::UnsupportedFormat { value } => CliOutput::error(
                CliErrorCode::CliOutputFormatUnsupported,
                format!("output format '{value}' is not supported"),
            ),
            Self::UnknownCommand { command } => CliOutput::error(
                CliErrorCode::CliCommandUnknown,
                format!("unknown command '{command}'"),
            ),
            Self::UnknownOption { command, option } => CliOutput::error(
                CliErrorCode::CliUnknownOption,
                format!("command '{command}' does not accept '{option}'"),
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CliParser;

impl CliParser {
    pub fn parse<I, S>(args: I) -> Result<ParsedCliInvocation, CliParseError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut raw_args = args.into_iter().map(Into::into);
        let _binary_name = raw_args.next();

        let (format, language, output_verbosity, verbose_paths, args) =
            extract_global_options(raw_args.collect())?;
        let Some((command, command_args)) = args.split_first() else {
            return Ok(ParsedCliInvocation::new(
                format,
                language,
                ParsedCliCommand::Help(CliHelpTopic::TopLevel),
            )
            .with_output_verbosity(output_verbosity)
            .with_verbose_paths(verbose_paths));
        };

        let parsed_command = cli_command_parser::parse_command(command, command_args)?;

        Ok(ParsedCliInvocation::new(format, language, parsed_command)
            .with_output_verbosity(output_verbosity)
            .with_verbose_paths(verbose_paths))
    }
}

fn extract_global_options(
    args: Vec<String>,
) -> Result<(RenderFormat, LanguageId, OutputVerbosity, bool, Vec<String>), CliParseError> {
    let mut format = None;
    let mut selected_language = None;
    let mut output_verbosity = OutputVerbosity::Default;
    let mut verbose_paths = false;
    let mut routed_args = Vec::with_capacity(args.len());
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--format" | "-f" => {
                let value = option_value(&args, index, "--format")?;
                format = Some(RenderFormatSelector::parse(Some(value)).map_err(format_error)?);
                index += 2;
            }
            "--verbose-paths" => {
                verbose_paths = true;
                index += 1;
            }
            "--verbose" => {
                output_verbosity = OutputVerbosity::Verbose;
                index += 1;
            }
            "--diagnostics" => {
                output_verbosity = OutputVerbosity::Diagnostics;
                index += 1;
            }
            "--lang" | "--language" => {
                let value = option_value(&args, index, "--lang")?;
                selected_language =
                    Some(
                        LanguageId::parse(value).ok_or_else(|| CliParseError::InvalidValue {
                            option: "--lang",
                            value: value.to_owned(),
                        })?,
                    );
                index += 2;
            }
            _ => {
                routed_args.push(args[index].clone());
                index += 1;
            }
        }
    }

    Ok((
        format.unwrap_or_default(),
        LanguageResolver::resolve_from_selected(selected_language),
        output_verbosity,
        verbose_paths,
        routed_args,
    ))
}

fn format_error(error: RenderFormatSelectionError) -> CliParseError {
    match error {
        RenderFormatSelectionError::UnsupportedFormat { value } => {
            CliParseError::UnsupportedFormat { value }
        }
    }
}

#[cfg(test)]
#[path = "cli_parser_tests.rs"]
mod tests;
