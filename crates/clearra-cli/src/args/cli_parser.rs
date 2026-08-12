use crate::{
    args::{
        ContinueArgs, ConvertArgs, CoverArgs, FailedQueueArgs, PcArgs, PcScenarioArgs, SetupArgs,
        VerifyArgs,
    },
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
    include_solution_data: bool,
    command: ParsedCliCommand,
}

impl ParsedCliInvocation {
    pub fn new(format: RenderFormat, language: LanguageId, command: ParsedCliCommand) -> Self {
        Self {
            format,
            language,
            output_verbosity: OutputVerbosity::Default,
            verbose_paths: false,
            include_solution_data: false,
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
    pub fn with_solution_data(mut self, include_solution_data: bool) -> Self {
        self.include_solution_data = include_solution_data;
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
    pub fn include_solution_data(&self) -> bool {
        self.include_solution_data
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
    FailedQueue(FailedQueueArgs),
    Setup(SetupArgs),
    Cover(CoverArgs),
    Rules(RulesArgs),
    Scoring(ScoringArgs),
    Convert(ConvertArgs),
    Continue(ContinueArgs),
    Verify(VerifyArgs),
    Product(Vec<String>),
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
    FailedQueue,
    Setup,
    Cover,
    Rules,
    Scoring,
    Convert,
    Continue,
    Verify,
    SpinStructure,
    Sfinder,
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
                    "usage: clearra [--format text|json|fumen-like] [--lang en|ko] [--verbose] [--diagnostics] [--verbose-paths] [--include-solution-data] <pc|pc-scenario|pc-replay|percent|failed-queue|setup-finder|build-probability|finesse|damage|spin-finder|spin-structure|build-coverage|rules|scoring|convert|continue|verify|sfinder> [options]\nglobal options may appear before or after the command\nfinesse search: clearra finesse search --base-mask HEX --target-mask HEX --height N (--queue QUEUE | --patterns PATTERN) [--hold empty|--no-hold] [--pattern-knowledge both|oracle|visible-7] [--rule RULE]\nfinesse score: clearra finesse score --initial-mask HEX --height N --placements PIECE:rotation:x:y,... (--queue QUEUE | --patterns PATTERN) [--hold empty|--no-hold] [--pattern-knowledge both|oracle|visible-7] [--rule RULE]\nbuild-probability finesse: add --finesse inputs [--pattern-knowledge both|oracle|visible-7]\nspin-structure searches an unordered piece inventory and keeps Regular and Mini structures separate; it does not change spin-finder or damage\nspin-structure usage: --pieces IOTSZ [--spin-profile t-spins|t-spins-plus|all-mini|all-mini-plus|all-spin|all-spin-plus] [--lines any|0..4|1+..4+] [--height 4..24] [--fill-bottom N --fill-top N] [--minimality subset-minimal|minimum-piece-count] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--workers N|--auto-workers N] [--use-all-cpu-threads]\n--include-solution-data is a JSON-only host integration surface for exact CTK3 export data\nlegacy Clearra aliases: path=pc-replay, setup=setup-finder, cover=build-coverage\nSfinder-man-style native mappings are isolated under: clearra sfinder <command>; they are not complete solution-finder 1.43 CLI parity\ntry opening preset: clearra pc --lines 2"
                }
                Self::Pc => {
                    "usage: clearra pc --lines 2 [--queue IOTSZJL] [--fixed|--observed] [--hold|--no-hold] [--queue-knowledge oracle|visible-7] [--objective all|unique|min-cover|tiling] [--tiling-only] [--solution-probabilities] [--score] [--score-profile tetrio|guideline|jstris-ultra] [--spin-profile t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus] [--initial-b2b N] [--rule srs-plus|srs|srs-x|jstris-180|asc|ars|no-kick] [--kick-profile-json JSON] [--backend auto|cpu|gpu|hybrid] [--workers N|--cpu-threads N|--auto-workers N] [--use-all-cpu-threads] [--cpu-warmup] [--gpu-warmup] [--tablebase|--no-tablebase] [--build-dependency-dag|--no-build-dependency-dag] [--no-gpu] [--deterministic] [--max-candidates N] [--max-patterns N] [--max-memory-mib N] [--gpu-device auto|N] [--allow-backend-fallback|--no-backend-fallback]\n--auto-workers caps adaptive CPU parallelism without forcing small searches onto the parallel path\n--tiling-only enumerates exact geometry tilings without BuildUp or probability calculation; results may include solutions that cannot be built, hold still determines the reachable supply multiset, and rule, scoring, B2B, visible-7, tablebase, dependency-DAG, and per-solution probability options are unavailable"
                }
                Self::PcScenario => {
                    "usage: clearra pc-scenario --fixture tests/fixtures/pc/example.json [--verify-expected] [--solution-probabilities] [--backend auto|cpu|gpu|hybrid] [--workers N|--cpu-threads N] [--use-all-cpu-threads] [--cpu-warmup] [--gpu-warmup] [--no-gpu]\n   or: clearra pc-scenario --field 0x... --queue IOTSZJ --max-pieces 6 [--solution-probabilities] [--rule srs-plus|srs|srs-x|jstris-180|asc|ars|no-kick] [--kick-profile-json JSON] [--workers N|--cpu-threads N] [--use-all-cpu-threads] [--cpu-warmup] [--gpu-warmup] [--no-gpu] [--deterministic] [--max-candidates N] [--max-patterns N] [--max-memory-mib N] [--gpu-device auto|N] [--allow-backend-fallback|--no-backend-fallback]\ncompiles a scenario preset into a SearchProblem; per-solution probability output preserves canonical CLI solution order"
                }
                Self::Path => {
                    "usage: clearra pc-replay --lines 2 [--queue IIOOO] [--fixed|--observed] [--no-hold]\nreturns one retained representative replay; this is not Sfinder path, which enumerates all solution paths. 'path' remains a legacy alias"
                }
                Self::Percent => {
                    "usage: clearra percent --queue IOTSZ [--observed|--bag-aligned|--fixed] [--min-len N] [--max-patterns N] [--failed-count N]"
                }
                Self::FailedQueue => {
                    "usage: clearra failed-queue --lines 4 [--patterns P7P3 | --queue IOTSZJL --fixed|--observed] [--hold|--no-hold] [--queue-knowledge oracle|visible-7] [--rule srs-plus|srs|srs-x|jstris-180] [--backend auto|cpu|gpu|hybrid] [--workers N|--cpu-threads N|--auto-workers N] [--tablebase|--no-tablebase] [--build-dependency-dag|--no-build-dependency-dag] [--failed-count N]\nreturns the exact complement of queues that reach the requested reverse-search PC target; omitted --failed-count materializes every failed queue"
                }
                Self::Setup => {
                    "usage: clearra setup-finder --remaining IOTSZJL [--initial-hold empty|I|O|T|S|Z|J|L] [--mode oracle] [--queue-knowledge oracle|visible-7] [--next-cycle-remaining IOTS] [--rule srs-plus|srs|srs-x|jstris-180] [--priority all|build|pc] [--setup-length auto|longer|shorter] [--max-setup-pieces 1..10] [--workers N|--cpu-threads N|--auto-workers N] [--use-all-cpu-threads] [--tablebase|--no-tablebase] [--allow-post-cycle-borrow]\n   or: clearra setup-finder --remaining TI --mode qb --qb OS [--queue-knowledge oracle|visible-7] [--next-cycle-remaining OOSITZ] [--initial-hold empty|I|O|T|S|Z|J|L] [--rule srs-plus|srs|srs-x|jstris-180] [--priority all|build|pc] [--setup-length auto|longer|shorter] [--max-setup-pieces 1..10] [--workers N|--cpu-threads N|--auto-workers N] [--use-all-cpu-threads] [--tablebase|--no-tablebase]\nthis PC-family partial-BuildUp finder is not Sfinder setup's required-area placement command; 'setup' remains a legacy alias. --remaining is the unordered inventory before the next bag boundary and determines the PC cycle before any hold separation. At most one piece kind may appear twice; that duplicated piece is automatically placed in initial hold and still counts toward the cycle. --initial-hold remains a CLI-only explicit override and removes one matching piece from the same inventory. --qb is the observed next-bag piece group and enables queue-based setup generation; it is independent from --queue-knowledge, which selects full-future oracle coverage or an exact visible-seven action policy. --next-cycle-remaining independently constrains the exact hold plus bag remainder left after this PC and is valid in oracle or QB mode. --tablebase is opt-in and only rejects precomputed exact-dead PC4 completion states; all other states use the standard exact search. --rule selects the kick table used for every setup and completion BuildUp check. Setup search defaults to the process-visible logical processor count minus one; --auto-workers lowers that adaptive ceiling without forcing fixed parallel execution; --use-all-cpu-threads explicitly removes the reserved processor. --max-setup-pieces defaults to 9; use 10 to include complete PC solutions. Priority all ranks joint Build x PC coverage. Setup length is independent; auto favors longer setups for all/build and shorter setups for pc"
                }
                Self::Cover => {
                    "usage: clearra build-coverage [--template name|--template-json json|--template-file path] [--export-template-json]\nevaluates Clearra typed build templates; this is not Sfinder cover's operation/fumen input contract. 'cover' remains a legacy alias"
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
                Self::SpinStructure => {
                    "usage: clearra spin-structure --pieces IOTSZ [--board-mask-v1 HEX | --board-mask HEX] [--height 4..24] [--fill-bottom N --fill-top N] [--lines any|0..4|1+..4+] [--spin-profile t-spins|t-spins-plus|all-mini|all-mini-plus|all-spin|all-spin-plus] [--minimality subset-minimal|minimum-piece-count] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--workers N|--auto-workers N] [--use-all-cpu-threads]\nsearches an unordered piece inventory without changing damage or spin-finder\nT-Spins and T-Spins+ accept only terminal T placements; the Plus profile additionally admits the exact immobile fallback as Mini\nAll-Mini and All-Mini+ label qualifying non-T terminal spins as Mini; All-Spin and All-Spin+ label them as Regular\nRegular and Mini results are emitted as separate partitions; repeated piece letters preserve multiplicity"
                }
                Self::Sfinder => {
                    "usage: clearra sfinder <command> [legacy positional arguments] [--workers N|--cpu-threads N|--auto-workers N] [--use-all-cpu-threads]\nClearra-native mappings: path, chance, percent, minimals, score, score-minimals, saves, best-save, cover, setup, congruent, congruent-cover, cover-percent, special-cover, spin-cover, spin, setup-cover, score-finder, pc-setup, best-setup, dpc-finder, verify\nThis is a limited Sfinder-man-style dialect, not complete solution-finder 1.43 CLI parity; unsupported legacy parameters fail explicitly\nClearra path/setup/cover keep their historical Clearra meanings; use this namespace for the mapped legacy meanings\nSfinder queue spellings such as *p4, *!, and [OISZ]p2 are normalized at this boundary\n--auto-workers limits adaptive parallelism without forcing small searches into the worker path; --workers explicitly requests a fixed pool"
                }
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

        let (format, language, output_verbosity, verbose_paths, include_solution_data, args) =
            extract_global_options(raw_args.collect())?;
        let Some((command, command_args)) = args.split_first() else {
            return Ok(ParsedCliInvocation::new(
                format,
                language,
                ParsedCliCommand::Help(CliHelpTopic::TopLevel),
            )
            .with_output_verbosity(output_verbosity)
            .with_verbose_paths(verbose_paths)
            .with_solution_data(include_solution_data));
        };

        let parsed_command = cli_command_parser::parse_command(command, command_args)?;

        Ok(ParsedCliInvocation::new(format, language, parsed_command)
            .with_output_verbosity(output_verbosity)
            .with_verbose_paths(verbose_paths)
            .with_solution_data(include_solution_data))
    }
}

fn extract_global_options(
    args: Vec<String>,
) -> Result<
    (
        RenderFormat,
        LanguageId,
        OutputVerbosity,
        bool,
        bool,
        Vec<String>,
    ),
    CliParseError,
> {
    let mut format = None;
    let mut selected_language = None;
    let mut output_verbosity = OutputVerbosity::Default;
    let mut verbose_paths = false;
    let mut include_solution_data = false;
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
            "--include-solution-data" => {
                include_solution_data = true;
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

    let format = format.unwrap_or_default();
    if include_solution_data && format != RenderFormat::Json {
        return Err(CliParseError::InvalidValue {
            option: "--include-solution-data",
            value: "requires --format json".to_owned(),
        });
    }

    Ok((
        format,
        LanguageResolver::resolve_from_selected(selected_language),
        output_verbosity,
        verbose_paths,
        include_solution_data,
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
