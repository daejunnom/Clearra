use crate::{
    args::{
        ContinueArgs, ConvertArgs, CoverArgs, FailedQueueArgs, PcArgs, PcScenarioArgs, SetupArgs,
        VerifyArgs,
    },
    args::{PathArgs, PercentArgs, RulesArgs, ScoringArgs},
    error::CliErrorCode,
    output::{
        CliOutput, OutputVerbosity, RenderFormat, RenderFormatSelectionError, RenderFormatSelector,
        SolutionArtifactOutputFormat, SolutionArtifactOutputRequest,
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
    solution_stdout_format: Option<SolutionArtifactOutputFormat>,
    solution_artifact_output: Option<SolutionArtifactOutputRequest>,
    explicit_ties: ExplicitTieOptions,
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
            solution_stdout_format: None,
            solution_artifact_output: None,
            explicit_ties: ExplicitTieOptions::default(),
            command,
        }
    }
}
impl ParsedCliInvocation {
    pub fn with_solution_stdout_format(
        mut self,
        solution_stdout_format: Option<SolutionArtifactOutputFormat>,
    ) -> Self {
        self.solution_stdout_format = solution_stdout_format;
        self
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
    pub fn with_solution_artifact_output(
        mut self,
        solution_artifact_output: Option<SolutionArtifactOutputRequest>,
    ) -> Self {
        self.solution_artifact_output = solution_artifact_output;
        self
    }
}
impl ParsedCliInvocation {
    pub fn with_explicit_ties(mut self, explicit_ties: ExplicitTieOptions) -> Self {
        self.explicit_ties = explicit_ties;
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
    pub fn solution_artifact_output(&self) -> Option<&SolutionArtifactOutputRequest> {
        self.solution_artifact_output.as_ref()
    }
}
impl ParsedCliInvocation {
    pub const fn solution_stdout_format(&self) -> Option<SolutionArtifactOutputFormat> {
        self.solution_stdout_format
    }
}
impl ParsedCliInvocation {
    pub const fn explicit_ties(&self) -> &ExplicitTieOptions {
        &self.explicit_ties
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExplicitTieOptions {
    requested: bool,
    snapshot_path: Option<String>,
    cursor: Option<String>,
}

impl ExplicitTieOptions {
    pub const fn requested(&self) -> bool {
        self.requested
    }

    pub fn snapshot_path(&self) -> Option<&str> {
        self.snapshot_path.as_deref()
    }

    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    pub const fn active(&self) -> bool {
        self.requested || self.snapshot_path.is_some() || self.cursor.is_some()
    }
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
    SpinStructure,
    Sfinder,
    Product(ProductHelpTopic),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductHelpTopic {
    PcTiling,
    PcMinimals,
    PcPath,
    PcChance,
    PcScore,
    PcScoreFinder,
    PcScoreMinimals,
    PcSaves,
    PcBestSave,
    PcFailedQueue,
    PcAllSpinSolution,
    PcAllSpinPreservationChance,
    BuildV2,
    BuildProbability,
    Finesse,
    Damage,
    SpinFinder,
    Ren,
    MappedCompatibility,
}

impl CliHelpTopic {
    pub fn into_output(self, language: LanguageId) -> CliOutput {
        let catalog = TranslationCatalog::new(language);
        let title = catalog.get_or_fallback(
            &TranslationKey::new("cli.help.top_level"),
            "Clearra command line",
        );
        if let Self::Product(topic) = self {
            return CliOutput::success(format!("{title}\n{}", topic.help_body()));
        }
        CliOutput::success(format!(
            "{title}\n{}",
            match self {
                Self::TopLevel => {
                    "usage: clearra [--format text|json|ctk3|fumen] [--lang en|ko] [--verbose] [--verbose-paths] [--include-solution-data] [--solution-output PATH] [--solution-artifact-format compact|json|ctk3|fumen] <pc|pc-scenario|pc-replay|percent|failed-queue|setup-finder|build|build-probability|finesse|damage|spin-finder|ren|spin-structure|build-coverage|rules|scoring|convert|continue|sfinder> [options]\nglobal options may appear before or after the command\nfinesse search: clearra finesse search --base-mask HEX --target-mask HEX --height N (--queue QUEUE | --patterns PATTERN) [--hold empty|PIECE|--no-hold] [--pattern-knowledge both|oracle|visible-7] [--rule RULE]\nfinesse score: clearra finesse score --initial-mask HEX --height N --placements PIECE:rotation:x:y,... (--queue QUEUE | --patterns PATTERN) [--hold empty|PIECE|--no-hold] [--pattern-knowledge both|oracle|visible-7] [--rule RULE]\nbuild-probability finesse: add --finesse inputs [--pattern-knowledge both|oracle|visible-7]\nspin-structure searches an unordered piece inventory and keeps Regular and Mini structures separate; it does not change spin-finder, damage, or ren\nspin-structure usage: --pieces IOTSZ [--spin-profile t-spins|t-spins-plus|all-mini|all-mini-plus|all-spin|all-spin-plus] [--lines any|0..4|1+..4+] [--height 4..24] [--fill-bottom N --fill-top N] [--minimality subset-minimal|minimum-piece-count] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--workers N|--auto-workers N] [--use-all-cpu-threads]\n--include-solution-data is a JSON-only host integration surface for exact document export data\n--solution-output atomically creates a new typed solution-set artifact; compact is the default and --solution-artifact-format selects compact, JSON, native CTK3, or native Fumen. Existing targets and symlink parents are rejected\nNative CTK3/Fumen encoding has no JavaScript, subprocess, network, or browser runtime dependency\nlegacy Clearra aliases: path=pc-replay, setup=setup-finder, cover=build-coverage\nSfinder-man-style native mappings are isolated under: clearra sfinder <command>; they are not complete solution-finder 1.43 CLI parity\ntry opening preset: clearra pc --lines 2"
                }
                Self::Pc => {
                    "usage: clearra pc --lines 2 [--queue IOTSZJL] [--fixed|--observed] [--hold|--no-hold] [--queue-knowledge oracle|visible-7] [--objective all|unique|min-cover|tiling] [--tiling-only] [--solution-probabilities] [--score] [--score-profile tetrio|guideline|jstris-ultra] [--spin-profile t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus] [--initial-b2b N] [--preserve-b2b] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--kick-profile-json JSON] [--backend auto|cpu|gpu|hybrid] [--workers N|--cpu-threads N|--auto-workers N] [--use-all-cpu-threads] [--cpu-warmup] [--gpu-warmup] [--tablebase|--no-tablebase] [--build-dependency-dag|--no-build-dependency-dag] [--no-gpu] [--deterministic] [--max-candidates N] [--max-patterns N] [--max-memory-mib N] [--gpu-device auto|N] [--allow-backend-fallback|--no-backend-fallback]\nDedicated existential B2B forms: `clearra pc allspin-sol --help` and `clearra pc allspin-pres-chance --help`\n--auto-workers caps adaptive CPU parallelism without forcing small searches onto the parallel path\n--tiling-only enumerates exact geometry tilings without BuildUp or probability calculation; results may include solutions that cannot be built, hold still determines the reachable supply multiset, and rule, scoring, B2B, visible-7, tablebase, dependency-DAG, and per-solution probability options are unavailable\nASC and ARS remain inspectable rule-registry profiles but are not runnable search choices until spawn reachability is implemented"
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
                Self::SpinStructure => {
                    "usage: clearra spin-structure search --pieces IOTSZ [common options]\n   or: clearra spin-structure cover --pieces IOTSZ [--objective min-cover] [--max-patterns 1..100000] [common options] [--ties --tie-snapshot PATH]\n   or: clearra spin-structure guaranteed --pieces IOTSZ [--final-piece T] [--max-patterns 1..100000] [--dependency-report|--no-dependency-report] [common options]\ncommon options: [--board-mask-v1 HEX | --board-mask HEX] [--height 4..24] [--fill-bottom N --fill-top N] [--lines any|0..4|1+..4+] [--spin-profile t-spins|t-spins-plus|all-mini|all-mini-plus|all-spin|all-spin-plus] [--minimality subset-minimal|minimum-piece-count] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--workers N|--auto-workers N] [--use-all-cpu-threads]\nAll routes search an unordered no-hold inventory on CPU with backend fallback disabled. Queue/pattern, hold, GPU, tablebase, and explicit memory options are unavailable. Search returns the ordinary spin-structure family. Cover returns the exact minimum spin-structure portfolio; without --ties its deterministically ordered first portfolio is rendered, while explicit --ties --tie-snapshot PATH pages every equal-cardinality optimum. Guaranteed returns the ordinary guaranteed spin-structure family whose structure accepts every unique non-target order with the final piece last. Regular and Mini results remain separate partitions, and repeated piece letters preserve multiplicity."
                }
                Self::Sfinder => {
                    "usage: clearra sfinder <command> [legacy positional arguments] [--workers N|--cpu-threads N|--auto-workers N] [--use-all-cpu-threads]\nClearra-native mappings: path, chance, percent, minimals, score, score-minimals, saves, best-save, cover, setup, congruent, congruent-cover, cover-percent, special-cover, setup-cover, score-finder, pc-setup, best-setup, dpc-finder\nSfinder spin/spincover are unordered structural searches and intentionally fail until their structural search and cover result contracts are implemented; they are not aliases of Clearra's ordered forward spin-finder\nThis is a limited Sfinder-man-style dialect, not complete solution-finder 1.43 CLI parity; unsupported legacy parameters fail explicitly\nClearra path/setup/cover keep their historical Clearra meanings; use this namespace for the mapped legacy meanings\nSfinder queue spellings such as *p4, *!, and [OISZ]p2 are normalized at this boundary\n--auto-workers limits adaptive parallelism without forcing small searches into the worker path; --workers explicitly requests a fixed pool"
                }
                Self::Product(_) => unreachable!("product help returned above"),
            }
        ))
    }
}

impl ProductHelpTopic {
    fn help_body(self) -> &'static str {
        match self {
            Self::PcTiling => {
                "usage: clearra pc tiling --lines 2 [--patterns PATTERN | --queue QUEUE] [--no-hold] [--backend auto|cpu|gpu|hybrid] [--gpu-device auto|N] [--workers N|--auto-workers N] [--use-all-cpu-threads] [--cpu-warmup] [--gpu-warmup] [--max-patterns N] [--max-nodes N] [--max-frontier-states N] [--max-candidates N] [--max-memory-mib N] [--allow-backend-fallback|--no-backend-fallback]\n   or: clearra pc tiling --board-mask HEX --height 1..6 --pieces N --lines same-as-height [--patterns PATTERN | --queue QUEUE] [--hold empty|PIECE|--no-hold] [backend/resource options]\nRuns the dedicated PC tiling search and returns the exact supply-compatible geometry tiling family. BuildUp, reachability, coverage, probability, rule, spin, B2B, score, visible-7, tablebase, dependency-DAG, and execution-constraint semantics are unavailable. Results may include tilings that cannot be built. The generic `clearra pc --tiling-only` and `clearra pc --objective tiling` forms remain advanced generic PC requests and do not acquire the dedicated result semantics."
            }
            Self::PcMinimals => {
                "usage: clearra pc minimals --lines 2 [--patterns PATTERN | --queue IOTSZJL] [--hold|--no-hold] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--backend auto|cpu|gpu|hybrid] [--workers N|--auto-workers N] [--max-patterns N] [--max-nodes N] [--max-frontier-states N] [--max-candidates N]\n   or: clearra pc minimals --board-mask HEX --height 1..6 --pieces N --lines same-as-height [--patterns PATTERN | --queue QUEUE] [--hold empty|PIECE|--no-hold] [search options]\nRuns the dedicated minimum-solution search and returns the exact query-bound minimum cover after complete source-coverage replay. Explicit memory caps, scoring, tiling-only, visible-7, tablebase, and dependency-DAG semantics are unavailable. The top-level minimals and sfinder minimals commands remain legacy-compatible generic results."
            }
            Self::PcPath => {
                "usage: clearra pc path --lines 2|4|6 (--queue QUEUE | --patterns PATTERN) [--hold|--no-hold] [--rule RULE] [search options]\n   or: clearra pc path --board-mask HEX --height 1..6 --pieces N --lines same-as-height (--queue QUEUE | --patterns PATTERN) [--hold empty|PIECE|--no-hold] [--rule RULE] [search options]\nRuns the dedicated complete replay-path search with objective all and count all. Every witness preserves its placements, source sequence, hold/cursor transitions, consumed-piece count, and line clears. This is not an optimal-portfolio tie set and exposes no tie metadata or portfolio cursor."
            }
            Self::PcChance => {
                "usage: clearra pc chance --lines 2 [--patterns PATTERN | --queue IOTSZJL] [--hold|--no-hold] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--backend auto|cpu|gpu|hybrid] [--workers N|--auto-workers N] [--max-patterns N]\nRuns the dedicated PC probability search and returns its complete queue probability. The top-level chance and percent commands remain legacy-compatible generic results."
            }
            Self::PcScore => {
                "usage: clearra pc score --lines 2 [--patterns PATTERN | --queue IOTSZJL] [--hold|--no-hold] [--score-profile tetrio|guideline|jstris-ultra] [--spin-profile disabled|t-spin-simple|t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus] [--initial-b2b N] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--workers N|--auto-workers N] [--use-all-cpu-threads] [--cpu-warmup]\n   or: clearra pc score --board-mask HEX --height 1..6 --pieces N --lines same-as-height [--patterns PATTERN | --queue QUEUE] [score options] [CPU worker options]\nNative CPU execution uses the normal local worker policy: automatic execution reserves one logical processor unless --use-all-cpu-threads is set, while --workers requests a fixed width. Browser execution keeps N on the coordinator and normalizes each isolated WASM child to one worker, preventing nested worker pools. Input is limited to 16 source pieces and one factorized pattern expression; P7P7P2 is supported symbolically. Returns the PC field-average score result. Scores use a basic approximation and are not profile-specific exact values. The top-level score and sfinder score commands remain legacy-compatible generic results."
            }
            Self::PcScoreFinder => {
                "usage: clearra pc score-finder --lines 2|4|6 --queue QUEUE [--hold|--no-hold] [--initial-b2b 0|1] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--workers N|--auto-workers N] [--use-all-cpu-threads] [--cpu-warmup] [--ties]\n   or: clearra pc score-finder --board-mask HEX --height 1..6 --pieces N --lines same-as-height --queue QUEUE [--hold empty|PIECE|--no-hold] [--initial-b2b 0|1] [--rule RULE] [CPU worker options] [--ties]\nRuns the dedicated fixed-queue maximum-score search with the owned jstris-ultra score profile and t-spins spin profile. Native CPU execution uses the normal local worker policy: automatic execution reserves one logical processor unless --use-all-cpu-threads is set, while --workers requests a fixed width. Browser execution keeps N on the coordinator while each isolated WASM child uses one worker. Highest-score equality and ordering use integer score only; attack is informational and never breaks a tie. The ordinary result has no portfolio tie metadata. Explicit --ties renders every equal highest-score witness as ordinary family entries and does not accept --tie-snapshot."
            }
            Self::PcScoreMinimals => {
                "usage: clearra pc score-minimals --lines 2 [--patterns PATTERN | --queue IOTSZJL] [--hold|--no-hold] [--score-profile tetrio|guideline|jstris-ultra] [--spin-profile disabled|t-spin-simple|t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus] [--initial-b2b N] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--workers N|--auto-workers N] [--use-all-cpu-threads] [--cpu-warmup] [--ties --tie-snapshot PATH]\n   or: clearra pc score-minimals --board-mask HEX --height 1..6 --pieces N --lines same-as-height [--patterns PATTERN | --queue QUEUE] [score options] [CPU worker options] [--ties --tie-snapshot PATH]\nRuns the score-only B-option highest-score minimum-set search. Native CPU execution uses the normal local worker policy: automatic execution reserves one logical processor unless --use-all-cpu-threads is set, while --workers requests a fixed width. Browser execution keeps N on the coordinator while each isolated WASM child uses one worker. Score equality, eligibility, ordering, portfolio membership, and deterministic selection never use attack; attack is informational only. Without --ties the deterministically ordered first portfolio is rendered. The explicit --ties path creates a restartable exact snapshot so every equal-cardinality optimal portfolio can be paged with `clearra continue --tie-snapshot PATH --tie-cursor TOKEN`."
            }
            Self::PcSaves => {
                "usage: clearra pc saves --lines 2|4|6 [--patterns PATTERN] [--hold|--no-hold] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [search options]\n   or: clearra pc saves --board-mask HEX --height 1..6 --pieces N --lines same-as-height [--patterns PATTERN] [--hold empty|PIECE|--no-hold]\nReturns save groups. Each group is terminal hold plus the active-bag remainder multiset, deduplicated once per source pattern, with whole-universe unconditional probability and conditional probability among successful PC queues. Exact queues, observed/visible-seven sources, explicit memory caps, scoring, tiling, and per-solution probabilities fail closed because they do not provide the required fixed bag-boundary authority."
            }
            Self::PcBestSave => {
                "usage: clearra pc best-save --lines 2|4|6 [--patterns PATTERN] [--hold|--no-hold] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [search options]\n   or: clearra pc best-save --board-mask HEX --height 1..6 --pieces N --lines same-as-height [--patterns PATTERN] [--hold empty|PIECE|--no-hold]\nReturns the best save groups using the documented save weights. The lexicographic key is weighted total (T6/I4/O3/J1/L1/S0/Z0), then min(J,L), then whole-universe unconditional exact group probability. Every exact tied winner is an ordinary list entry; this result never uses portfolio tie semantics. Input authority restrictions match PC saves: fixed bag-boundary provenance is required, while exact queues and observed/visible-seven sources fail closed."
            }
            Self::PcFailedQueue => {
                "usage: clearra pc failed-queue --lines 4 [--patterns P7P3 | --queue IOTSZJL] [--failed-count N]\nRuns the dedicated failed-queue search. The top-level failed-queue and failed_queue commands remain legacy-compatible generic Percent requests."
            }
            Self::PcAllSpinSolution => {
                "usage: clearra pc allspin-sol --lines 2|4|6 --queue QUEUE --spin-profile t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus [--no-hold] [search options]\n   or: clearra pc allspin-sol --board-mask HEX --height 1..6 --pieces N [--lines same-as-height] --queue QUEUE --spin-profile PROFILE [--no-hold] [search options]\nsearch options: [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--backend auto|cpu|gpu|hybrid] [--gpu-device auto|N] [--workers N|--auto-workers N] [--use-all-cpu-threads] [--cpu-warmup] [--gpu-warmup] [--tablebase|--no-tablebase] [--build-dependency-dag|--no-build-dependency-dag] [--max-patterns N] [--max-nodes N] [--max-frontier-states N] [--max-candidates N] [--max-memory-mib N] [--allow-backend-fallback|--no-backend-fallback]\nRuns inverse-lock-clear PC search for exactly one fixed original queue and returns a deterministic B2B-preserving witness when one exists. The denominator is exactly one original materialized queue; hold/path multiplicity is not counted. The optional board-mask/height/pieces trio is an initial field; the clear-to-empty goal remains implicit and no target-field input exists. If --lines accompanies the trio it must equal --height. This form is oracle-fixed and rejects FILE/local paths, visible-7, score/objective selection, scenario hold-slot overrides, and caller-supplied --preserve-b2b. Clearra uses the selected explicit six-profile spin/replay contract; compatibility with sfinderbot allspin_sol_finder is command-intent only, not exact legacy recognition parity."
            }
            Self::PcAllSpinPreservationChance => {
                "usage: clearra pc allspin-pres-chance --lines 2|4|6 --patterns PATTERN --spin-profile t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus [--no-hold] [search options]\n   or: clearra pc allspin-pres-chance --board-mask HEX --height 1..6 --pieces N [--lines same-as-height] --patterns PATTERN --spin-profile PROFILE [--no-hold] [search options]\nsearch options: [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--backend auto|cpu|gpu|hybrid] [--gpu-device auto|N] [--workers N|--auto-workers N] [--use-all-cpu-threads] [--cpu-warmup] [--gpu-warmup] [--tablebase|--no-tablebase] [--build-dependency-dag|--no-build-dependency-dag] [--max-patterns N] [--max-nodes N] [--max-frontier-states N] [--max-candidates N] [--max-memory-mib N] [--allow-backend-fallback|--no-backend-fallback]\nRuns inverse-lock-clear PC search over the original materialized queue pattern and returns the existential B2B-preserving numerator, original-queue denominator, source probability, and completeness. Each original queue is counted once; hold/path multiplicity is not counted. The optional board-mask/height/pieces trio is an initial field; the clear-to-empty goal remains implicit and no target-field input exists. If --lines accompanies the trio it must equal --height. This form is oracle-fixed and rejects FILE/local paths, visible-7, score/objective selection, scenario hold-slot overrides, and caller-supplied --preserve-b2b. Clearra uses the selected explicit six-profile spin/replay contract; compatibility with sfinderbot allspin_pres_chance is command-intent only, not exact legacy recognition parity."
            }
            Self::BuildV2 => {
                "usage: clearra build cover --base-mask HEX --target-mask HEX --height N (--queue QUEUE | --patterns PATTERN) [--source-pieces N] [--objective min-cover|max-probability-minimum] [Build execution options]\n   or: clearra build <setup|congruent|congruent-cover|setup-cover|setup-cover-percent|setup-cover-score> --target-format ctk3|fumen --target-document DOCUMENT (--queue QUEUE | --patterns PATTERN) [typed Build options]\n   or: clearra build evaluate <cover|minimals|score|b2b-cover|cover-percent> --solution-format ctk3|fumen --solution-document DOCUMENT (--queue QUEUE | --patterns PATTERN) [typed Build options]\nTarget-document and supplied-solution document inputs are nominally distinct and cannot be substituted for one another. Every form requires exactly one of --queue or --patterns and accepts --queue-knowledge oracle|visible-7, --hold PIECE|--no-hold, --rule RULE, --max-patterns N, --max-nodes N, --max-frontier-states N, --max-candidates N, --workers N|--auto-workers N|--use-all-cpu-threads, and --cpu-warmup. Objectives are closed per form under --objective all|unique|min-cover|max-probability-minimum|max-score-cover; minimum-cover is the only compatibility alias. Score forms alone accept --score-profile tetrio|guideline|jstris-ultra and --initial-b2b 0..65535. Exact portfolio forms (cover, congruent-cover, setup-cover, setup-cover-score, evaluate minimals, and evaluate score) expose additional alternatives only through explicit --ties --tie-snapshot PATH; ordinary families and probability results never do. Score equality and ordering never use attack. Typed Build is CPU-only in v0.8 and rejects --max-memory-mib until its finite response authority is implemented."
            }
            Self::BuildProbability => {
                "usage: clearra build-probability --base-mask HEX --target-mask HEX --height 1..24 (--queue QUEUE | --patterns PATTERN) [--hold empty|PIECE|--no-hold] [--source-pieces N] [--aggregate buildability|tiling|spin] [--result-mode all-solutions|complete-replay-paths|field-average-score|fixed-queue-maximum-score|highest-score-minimum-set|failed-queues] [--paths|--score] [--score-profile tetrio|guideline|jstris-ultra] [--initial-b2b N] [--failed-count N] [--tiling-only] [--solution-probabilities] [--spin-profile t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus] [--preserve-b2b] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--build-dependency-dag|--no-build-dependency-dag] [--finesse off|inputs] [--pattern-knowledge both|oracle|visible-7] [--include-mirror|--no-mirror] [--backend auto|cpu|gpu|hybrid] [--workers N|--auto-workers N] [--use-all-cpu-threads] [--cpu-warmup] [--max-patterns N] [--max-candidates N] [--max-memory-mib N] [--allow-backend-fallback|--no-backend-fallback]\nEngine aggregation and result aggregation are separate concepts with an explicit compatibility matrix: every non-all result mode currently requires buildability aggregation and incompatible tiling/spin combinations fail closed. Complete replay paths are exhaustive operation/lock/clear witnesses and currently require a compact 1..6-row buildability query. Field-average score reports every successful normalized field plus the whole materialized-universe score with failed patterns contributing zero. Fixed-queue maximum score requires one exact queue and keeps all score-only ties. Highest-score minimum set computes an exact minimum portfolio over every candidate tying each successful pattern's maximum score; attack is informational only. Failed queues are the exact complement of successful Build coverage and --failed-count bounds only the displayed examples. Minimum build portfolios use `clearra build cover --objective min-cover`. The primary metric remains full-future oracle build probability; --solution-probabilities includes exact per-solution probabilities, --spin-profile requires --aggregate spin or --preserve-b2b, --pattern-knowledge requires --finesse inputs, and tiling aggregation rejects rule, spin, B2B, dependency-DAG, per-solution probability, and finesse options."
            }
            Self::Finesse => {
                "usage: clearra finesse search --base-mask HEX --target-mask HEX --height N (--queue QUEUE | --patterns PATTERN) [--hold empty|PIECE|--no-hold] [--pattern-knowledge both|oracle|visible-7] [--rule RULE] [--workers N|--auto-workers N]\n   or: clearra finesse score --initial-mask HEX --height N --placements PIECE:rotation:x:y,... (--queue QUEUE | --patterns PATTERN) [--hold empty|PIECE|--no-hold] [--pattern-knowledge both|oracle|visible-7] [--rule RULE]"
            }
            Self::Damage => {
                "usage: clearra damage --board-mask HEX --height 1..24 --queue QUEUE [--hold|--no-hold] [--spin-profile disabled|t-spin-simple|t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus] [--initial-combo 0..65535] [--initial-b2b 0..65535] [--preserve-b2b] [--minimum-damage 0..4294967295] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--workers N|--auto-workers N] [--use-all-cpu-threads]\ndefault spin profile: all-mini-plus; --minimum-damage selects at-least mode, otherwise the maximum-damage result is returned"
            }
            Self::SpinFinder => {
                "usage: clearra spin-finder --board-mask HEX --height 1..24 (--queue QUEUE | --patterns PATTERN) [--hold|--no-hold] [--spin-profile t-spin-simple|t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus] [--lines any|0..4|1+..4+] [--spin-category any|t|other] [--initial-combo 0..65535] [--initial-b2b 0..65535] [--preserve-b2b] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--workers N|--auto-workers N] [--use-all-cpu-threads]\ndefault spin profile: t-spins; --spin-category other requires an all-spin or all-mini profile"
            }
            Self::Ren => {
                "usage: clearra ren --board-mask HEX --height 1..24 --queue QUEUE [--hold|--no-hold] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--workers N|--auto-workers N] [--use-all-cpu-threads]\nFinds every canonical witness with the maximum exact REN for a fixed queue of at most 22 pieces. Initial complete rows are normalized without counting toward REN; every accepted lock must clear at least one line, and a non-clearing first lock yields no solution. Hold starts empty and is enabled by default. Spin and damage scoring options are intentionally unavailable."
            }
            Self::MappedCompatibility => {
                "usage: clearra <mapped-command> [legacy-compatible options]\nThis command is a curated compatibility mapping. Use `clearra sfinder --help` for the represented command inventory; parameters outside that inventory fail explicitly."
            }
        }
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

        let (
            format,
            language,
            output_verbosity,
            verbose_paths,
            include_solution_data,
            solution_stdout_format,
            solution_artifact_output,
            explicit_ties,
            args,
        ) = extract_global_options(raw_args.collect())?;
        let Some((command, command_args)) = args.split_first() else {
            validate_explicit_tie_options(
                &ParsedCliCommand::Help(CliHelpTopic::TopLevel),
                &explicit_ties,
            )?;
            return Ok(ParsedCliInvocation::new(
                format,
                language,
                ParsedCliCommand::Help(CliHelpTopic::TopLevel),
            )
            .with_output_verbosity(output_verbosity)
            .with_verbose_paths(verbose_paths)
            .with_solution_data(include_solution_data)
            .with_solution_stdout_format(solution_stdout_format)
            .with_solution_artifact_output(solution_artifact_output)
            .with_explicit_ties(explicit_ties));
        };

        let parsed_command = cli_command_parser::parse_command(command, command_args)?;
        validate_explicit_tie_options(&parsed_command, &explicit_ties)?;

        Ok(ParsedCliInvocation::new(format, language, parsed_command)
            .with_output_verbosity(output_verbosity)
            .with_verbose_paths(verbose_paths)
            .with_solution_data(include_solution_data)
            .with_solution_stdout_format(solution_stdout_format)
            .with_solution_artifact_output(solution_artifact_output)
            .with_explicit_ties(explicit_ties))
    }
}

type ExtractedGlobalOptions = (
    RenderFormat,
    LanguageId,
    OutputVerbosity,
    bool,
    bool,
    Option<SolutionArtifactOutputFormat>,
    Option<SolutionArtifactOutputRequest>,
    ExplicitTieOptions,
    Vec<String>,
);

fn extract_global_options(args: Vec<String>) -> Result<ExtractedGlobalOptions, CliParseError> {
    let mut format = None;
    let mut selected_language = None;
    let mut output_verbosity = OutputVerbosity::Default;
    let mut verbose_paths = false;
    let mut include_solution_data = false;
    let mut solution_stdout_format = None;
    let mut solution_output = None;
    let mut solution_artifact_format = None;
    let mut explicit_ties = ExplicitTieOptions::default();
    let mut routed_args = Vec::with_capacity(args.len());
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--format" | "-f" => {
                let value = option_value(&args, index, "--format")?;
                let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
                match normalized.as_str() {
                    "ctk3" => {
                        format = Some(RenderFormat::Text);
                        solution_stdout_format = Some(SolutionArtifactOutputFormat::Ctk3);
                    }
                    "fumen" => {
                        format = Some(RenderFormat::Text);
                        solution_stdout_format = Some(SolutionArtifactOutputFormat::Fumen);
                    }
                    _ => {
                        format =
                            Some(RenderFormatSelector::parse(Some(value)).map_err(format_error)?);
                        solution_stdout_format = None;
                    }
                }
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
            "--ties" => {
                if explicit_ties.requested {
                    return Err(CliParseError::InvalidValue {
                        option: "--ties",
                        value: "specified more than once".to_owned(),
                    });
                }
                explicit_ties.requested = true;
                index += 1;
            }
            "--tie-snapshot" => {
                if explicit_ties.snapshot_path.is_some() {
                    return Err(CliParseError::InvalidValue {
                        option: "--tie-snapshot",
                        value: "specified more than once".to_owned(),
                    });
                }
                let value = option_value(&args, index, "--tie-snapshot")?;
                if value.is_empty() {
                    return Err(CliParseError::InvalidValue {
                        option: "--tie-snapshot",
                        value: value.to_owned(),
                    });
                }
                explicit_ties.snapshot_path = Some(value.to_owned());
                index += 2;
            }
            "--tie-cursor" => {
                if explicit_ties.cursor.is_some() {
                    return Err(CliParseError::InvalidValue {
                        option: "--tie-cursor",
                        value: "specified more than once".to_owned(),
                    });
                }
                let value = option_value(&args, index, "--tie-cursor")?;
                if value.is_empty() {
                    return Err(CliParseError::InvalidValue {
                        option: "--tie-cursor",
                        value: value.to_owned(),
                    });
                }
                explicit_ties.cursor = Some(value.to_owned());
                index += 2;
            }
            "--solution-output" => {
                if solution_output.is_some() {
                    return Err(CliParseError::InvalidValue {
                        option: "--solution-output",
                        value: "specified more than once".to_owned(),
                    });
                }
                let value = option_value(&args, index, "--solution-output")?;
                if value.is_empty() {
                    return Err(CliParseError::InvalidValue {
                        option: "--solution-output",
                        value: value.to_owned(),
                    });
                }
                solution_output = Some(value.to_owned());
                index += 2;
            }
            "--solution-artifact-format" => {
                if solution_artifact_format.is_some() {
                    return Err(CliParseError::InvalidValue {
                        option: "--solution-artifact-format",
                        value: "specified more than once".to_owned(),
                    });
                }
                let value = option_value(&args, index, "--solution-artifact-format")?;
                solution_artifact_format =
                    Some(SolutionArtifactOutputFormat::parse(value).ok_or_else(|| {
                        CliParseError::InvalidValue {
                            option: "--solution-artifact-format",
                            value: value.to_owned(),
                        }
                    })?);
                index += 2;
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
    if solution_stdout_format.is_some() && include_solution_data {
        return Err(CliParseError::InvalidValue {
            option: "--include-solution-data",
            value: "is incompatible with native document stdout".to_owned(),
        });
    }
    if solution_stdout_format.is_some() && output_verbosity != OutputVerbosity::Default {
        return Err(CliParseError::InvalidValue {
            option: "--format",
            value: "native document stdout does not accept verbose profiles".to_owned(),
        });
    }
    if solution_output.is_none() && solution_artifact_format.is_some() {
        return Err(CliParseError::InvalidValue {
            option: "--solution-artifact-format",
            value: "requires --solution-output".to_owned(),
        });
    }
    if solution_output.is_some() && format == RenderFormat::FumenLike {
        return Err(CliParseError::InvalidValue {
            option: "--solution-output",
            value: "is incompatible with --format fumen-like".to_owned(),
        });
    }
    if solution_stdout_format.is_some() && solution_output.is_some() {
        return Err(CliParseError::InvalidValue {
            option: "--solution-output",
            value: "is incompatible with native document stdout".to_owned(),
        });
    }
    let solution_artifact_output = solution_output.map(|target| {
        SolutionArtifactOutputRequest::new(
            target,
            solution_artifact_format.unwrap_or(SolutionArtifactOutputFormat::Compact),
        )
    });

    Ok((
        format,
        LanguageResolver::resolve_from_selected(selected_language),
        output_verbosity,
        verbose_paths,
        include_solution_data,
        solution_stdout_format,
        solution_artifact_output,
        explicit_ties,
        routed_args,
    ))
}

fn validate_explicit_tie_options(
    command: &ParsedCliCommand,
    options: &ExplicitTieOptions,
) -> Result<(), CliParseError> {
    if !options.active() {
        return Ok(());
    }
    if options.cursor.is_some() {
        if options.requested
            || options.snapshot_path.is_none()
            || !matches!(command, ParsedCliCommand::Continue(args) if args.token().is_none())
        {
            return Err(CliParseError::InvalidValue {
                option: "--tie-cursor",
                value: "requires `clearra continue --tie-snapshot PATH` without a positional continuation token".to_owned(),
            });
        }
        return Ok(());
    }

    if !options.requested {
        return Err(CliParseError::InvalidValue {
            option: "--tie-snapshot",
            value: "requires --ties or --tie-cursor".to_owned(),
        });
    }
    match command {
        ParsedCliCommand::Product(tokens) if is_pc_product(tokens, "minimals") => {
            if options.snapshot_path.is_none() {
                return Err(CliParseError::InvalidValue {
                    option: "--ties",
                    value: "pc minimals requires --tie-snapshot PATH".to_owned(),
                });
            }
            Ok(())
        }
        ParsedCliCommand::Product(tokens) if is_pc_product(tokens, "score-minimals") => {
            if options.snapshot_path.is_none() {
                return Err(CliParseError::InvalidValue {
                    option: "--ties",
                    value: "pc score-minimals requires --tie-snapshot PATH".to_owned(),
                });
            }
            Ok(())
        }
        ParsedCliCommand::Product(tokens) if is_pc_product(tokens, "score") => {
            if options.snapshot_path.is_some() {
                return Err(CliParseError::InvalidValue {
                    option: "--tie-snapshot",
                    value: "pc score is a normal winner family, not a restartable portfolio"
                        .to_owned(),
                });
            }
            Ok(())
        }
        ParsedCliCommand::Product(tokens) if is_pc_product(tokens, "score-finder") => {
            if options.snapshot_path.is_some() {
                return Err(CliParseError::InvalidValue {
                    option: "--tie-snapshot",
                    value: "pc score-finder is a normal winner family, not a restartable portfolio"
                        .to_owned(),
                });
            }
            Ok(())
        }
        ParsedCliCommand::Product(tokens) if is_build_portfolio_product(tokens) => {
            if options.snapshot_path.is_none() {
                return Err(CliParseError::InvalidValue {
                    option: "--ties",
                    value: "Build portfolio requests require --tie-snapshot PATH".to_owned(),
                });
            }
            Ok(())
        }
        ParsedCliCommand::Product(tokens) if is_spin_structure_cover(tokens) => {
            if options.snapshot_path.is_none() {
                return Err(CliParseError::InvalidValue {
                    option: "--ties",
                    value: "spin-structure cover requires --tie-snapshot PATH".to_owned(),
                });
            }
            Ok(())
        }
        _ => Err(CliParseError::InvalidValue {
            option: "--ties",
            value: "is available only for exact PC/Build/spin-structure cover portfolio forms and explicit PC score winner-family views".to_owned(),
        }),
    }
}

fn is_pc_product(tokens: &[String], subcommand: &str) -> bool {
    matches!(
        tokens,
        [binary, command, actual, ..]
            if binary == "clearra" && command == "pc" && actual == subcommand
    )
}

fn is_build_portfolio_product(tokens: &[String]) -> bool {
    matches!(
        tokens,
        [binary, command, subcommand, ..]
            if binary == "clearra"
                && command == "build"
                && matches!(
                    subcommand.as_str(),
                    "cover"
                        | "congruent-cover"
                        | "setup-cover"
                        | "setup-cover-score"
                )
    ) || matches!(
        tokens,
        [binary, command, evaluate, subcommand, ..]
            if binary == "clearra"
                && command == "build"
                && evaluate == "evaluate"
                && matches!(subcommand.as_str(), "minimals" | "score")
    )
}

fn is_spin_structure_cover(tokens: &[String]) -> bool {
    matches!(
        tokens,
        [binary, command, subcommand, ..]
            if binary == "clearra"
                && command == "spin-structure"
                && subcommand == "cover"
    )
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
