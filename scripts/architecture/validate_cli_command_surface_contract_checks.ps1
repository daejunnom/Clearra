# Ordered SRP validators share the caller function scope.

$cliEntrypoint = Read-Text "crates/clearra-cli/src/lib.rs"
$cliParser = Read-Text "crates/clearra-cli/src/args/cli_parser.rs"
$cliCommandParser = Read-Text "crates/clearra-cli/src/args/cli_command_parser.rs"
$cliParserRouteSurface = Get-CliArgsParserSurface
$pcScenarioArgs = Read-Text "crates/clearra-cli/src/args/pc_scenario_args.rs"
$pcScenarioCommand = Read-Text "crates/clearra-cli/src/commands/pc_scenario_command.rs"
$pcScenarioCommandTests = Read-Text "crates/clearra-cli/src/commands/pc_scenario_command_tests.rs"
$pcScenarioQueryAssembler = Read-Text "crates/clearra-cli/src/assemble/pc_scenario_query_assembler.rs"
$ruleProfileAssembler = Read-Text "crates/clearra-cli/src/assemble/rule_profile_assembler.rs"
$pieceSequenceAssembler = Read-Text "crates/clearra-cli/src/assemble/piece_sequence_assembler.rs"
$pcScenarioFixture = Read-Text "crates/clearra-cli/src/fixture/pc_scenario_fixture.rs"
$pcScenarioExpected = Read-Text "crates/clearra-cli/src/fixture/pc_scenario_expected.rs"
$pcScenarioUnsupported = Read-Text "crates/clearra-cli/src/fixture/pc_scenario_unsupported.rs"
$fileInputGuard = Read-Text "crates/clearra-cli/src/input/file_input_guard.rs"
$pathCommand = Read-Text "crates/clearra-cli/src/commands/path_command.rs"
$percentCommand = Read-Text "crates/clearra-cli/src/commands/percent_command.rs"
$rulesCommand = Read-Text "crates/clearra-cli/src/commands/rules_command.rs"
$rulesCommandTests = Read-Text "crates/clearra-cli/src/commands/rules_command_tests.rs"
$rulesListAction = Read-Text "crates/clearra-cli/src/rules/rules_list_action.rs"
$rulesInspectAction = Read-Text "crates/clearra-cli/src/rules/rules_inspect_action.rs"
$rulesVerifyAction = Read-Text "crates/clearra-cli/src/rules/rules_verify_action.rs"
$rulesImportAction = Read-Text "crates/clearra-cli/src/rules/rules_import_action.rs"
$rulesExportAction = Read-Text "crates/clearra-cli/src/rules/rules_export_action.rs"
$rulesOutputFields = Read-Text "crates/clearra-cli/src/rules/rules_output_fields.rs"
$rulesCommandSurface = "$rulesCommand`n$rulesCommandTests`n$rulesListAction`n$rulesInspectAction`n$rulesVerifyAction`n$rulesImportAction`n$rulesExportAction`n$rulesOutputFields"
$scoringCommand = Read-Text "crates/clearra-cli/src/commands/scoring_command.rs"
$scoringCommandTests = Read-Text "crates/clearra-cli/src/commands/scoring_command_tests.rs"
$scoringListAction = Read-Text "crates/clearra-cli/src/scoring/scoring_list_action.rs"
$scoringInspectAction = Read-Text "crates/clearra-cli/src/scoring/scoring_inspect_action.rs"
$scoringImportAction = Read-Text "crates/clearra-cli/src/scoring/scoring_import_action.rs"
$scoringExportAction = Read-Text "crates/clearra-cli/src/scoring/scoring_export_action.rs"
$scoringOutputFields = Read-Text "crates/clearra-cli/src/scoring/scoring_output_fields.rs"
$scoringCommandSurface = "$scoringCommand`n$scoringCommandTests`n$scoringListAction`n$scoringInspectAction`n$scoringImportAction`n$scoringExportAction`n$scoringOutputFields"
$setupCommand = Read-Text "crates/clearra-cli/src/commands/setup_command.rs"
$setupAppCommand = Read-Text "crates/clearra-app/src/commands/setup_app_command.rs"
$setupBackend = Read-Text "crates/clearra-core-executor/src/backend/wasm_setup_search_backend.rs"
$setupFinder = Read-Text "crates/clearra-core-executor/src/backend/wasm_cpu/setup_finder.rs"
$setupPartialBuild = Read-Text "crates/clearra-core-executor/src/backend/wasm_cpu/setup_partial_build.rs"
$setupCoverageGraph = Read-Text "crates/clearra-core-executor/src/backend/wasm_cpu/setup_coverage_graph.rs"
$cliErrorCode = Read-Text "crates/clearra-cli/src/error/cli_error_code.rs"
$cliCommandsMod = Read-Text "crates/clearra-cli/src/commands/mod.rs"
$continueCommand = Read-Text "crates/clearra-cli/src/commands/continue_command.rs"
$pcContinuationToken = Read-Text "crates/clearra-pc-graph/src/request/pc_continuation_token.rs"
$openingContinuationToken = Read-Text "crates/clearra-pc-graph/src/request/opening_continuation_token.rs"
$scenarioContinuationToken = Read-Text "crates/clearra-pc-graph/src/request/scenario_continuation_token.rs"
$continuationTokenV1 = Read-Text "crates/clearra-pc-graph/src/request/continuation_token_v1.rs"
$continuationTokenError = Read-Text "crates/clearra-pc-graph/src/request/continuation_token_error.rs"
$continuationTokenSegments = Read-Text "crates/clearra-pc-graph/src/request/continuation_token_segments.rs"
$continuationKickProfileCodec = Read-Text "crates/clearra-pc-graph/src/request/continuation_kick_profile_codec.rs"
$pcContinuationTokenTests = Read-Text "crates/clearra-pc-graph/src/request/pc_continuation_token_tests.rs"
$cliProcessE2e = Read-Text "crates/clearra-cli/tests/process_e2e.rs"
$argsMod = Read-Text "crates/clearra-cli/src/args/mod.rs"
$parsePcArgs = Read-Text "crates/clearra-cli/src/args/parse_pc_args.rs"
$parsePcScenarioArgs = Read-Text "crates/clearra-cli/src/args/parse_pc_scenario_args.rs"
$parsePathArgs = Read-Text "crates/clearra-cli/src/args/parse_path_args.rs"
$parsePercentArgs = Read-Text "crates/clearra-cli/src/args/parse_percent_args.rs"
$parseSetupArgs = Read-Text "crates/clearra-cli/src/args/parse_setup_args.rs"
$parseCoverArgs = Read-Text "crates/clearra-cli/src/args/parse_cover_args.rs"
$parseRulesArgs = Read-Text "crates/clearra-cli/src/args/parse_rules_args.rs"
$parseScoringArgs = Read-Text "crates/clearra-cli/src/args/parse_scoring_args.rs"
$parseConvertArgs = Read-Text "crates/clearra-cli/src/args/parse_convert_args.rs"
$parseContinueArgs = Read-Text "crates/clearra-cli/src/args/parse_continue_args.rs"
$parseVerifyArgs = Read-Text "crates/clearra-cli/src/args/parse_verify_args.rs"
$parseOptionValue = Read-Text "crates/clearra-cli/src/args/parse_option_value.rs"
$parsePieceArg = Read-Text "crates/clearra-cli/src/args/parse_piece_arg.rs"
$parseHelpers = Read-Text "crates/clearra-cli/src/args/parse_helpers.rs"
$splitParserFiles = @{
    "parse_pc_args.rs" = $parsePcArgs
    "parse_pc_scenario_args.rs" = $parsePcScenarioArgs
    "parse_path_args.rs" = $parsePathArgs
    "parse_percent_args.rs" = $parsePercentArgs
    "parse_setup_args.rs" = $parseSetupArgs
    "parse_cover_args.rs" = $parseCoverArgs
    "parse_rules_args.rs" = $parseRulesArgs
    "parse_scoring_args.rs" = $parseScoringArgs
    "parse_convert_args.rs" = $parseConvertArgs
    "parse_continue_args.rs" = $parseContinueArgs
    "parse_verify_args.rs" = $parseVerifyArgs
}
foreach ($requiredModule in @("mod cli_command_parser;", "mod parse_pc_args;", "mod parse_pc_scenario_args;", "mod parse_path_args;", "mod parse_percent_args;", "mod parse_setup_args;", "mod parse_cover_args;", "mod parse_rules_args;", "mod parse_scoring_args;", "mod parse_convert_args;", "mod parse_continue_args;", "mod parse_verify_args;", "mod parse_option_value;", "mod parse_piece_arg;", "mod parse_helpers;")) {
    if ($argsMod -notlike "*$requiredModule*") {
        Add-ArchitectureError "args/mod.rs must declare split CLI parser module marker '$requiredModule'"
    }
}
foreach ($requiredMarker in @('"pc" => parse_pc', '"pc-scenario" => parse_pc_scenario', '"path" => parse_path', '"percent" => parse_percent', '"setup" => parse_setup', '"cover" => parse_cover', '"rules" => parse_rules', '"scoring" => parse_scoring', '"convert" => parse_convert', '"continue" => parse_continue', '"verify" => parse_verify', 'ParsedCliCommand::Help', 'ParsedCliCommand::Unsupported', 'UnknownCommand')) {
    if ($cliCommandParser -notlike "*$requiredMarker*") {
        Add-ArchitectureError "cli_command_parser.rs must stay the top-level dispatch table and include marker '$requiredMarker'"
    }
}
$cliCommandParserLineCount = ($cliCommandParser -split "`n").Count
if ($cliCommandParserLineCount -gt 90) {
    Add-ArchitectureError "cli_command_parser.rs must stay a thin dispatcher; current line count is $cliCommandParserLineCount"
}
foreach ($forbiddenMarker in @('"--lines"', '"--queue"', '"--field"', '"--template-json"', '"--profile"', '"--input"', "option_value", "parse_usize_option", "parse_single_char", "PcArgs::new", "PcScenarioArgs::new", "CoverArgs::new", "RulesAction::parse", "ScoringAction::parse")) {
    if ($cliCommandParser -like "*$forbiddenMarker*") {
        Add-ArchitectureError "cli_command_parser.rs must not own command option parsing or args construction marker '$forbiddenMarker'"
    }
}
foreach ($parser in $splitParserFiles.GetEnumerator()) {
    $lineCount = ($parser.Value -split "`n").Count
    if ($lineCount -gt 180) {
        Add-ArchitectureError "$($parser.Key) must stay a command parser, not a mini command app; current line count is $lineCount"
    }
}
foreach ($requiredMarker in @("parse_pc_args", "PcArgs::new", '"--lines"', '"--queue"', '"--objective"', '"--rule"', '"--kick-profile-json"')) {
    if ($parsePcArgs -notlike "*$requiredMarker*") {
        Add-ArchitectureError "parse_pc_args.rs must own PC args marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("PcScenarioArgs::new", '"--fixture"', '"--field"', '"--max-pieces"', '"--exact-pieces"', '"--min-remaining-queue"', '"--count-policy"', '"--retained-trace-limit"', '"--verify-expected"', "parse_single_char")) {
    if ($parsePcScenarioArgs -notlike "*$requiredMarker*") {
        Add-ArchitectureError "parse_pc_scenario_args.rs must own pc-scenario args marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("PathArgs::new", "parse_pc_args")) {
    if ($parsePathArgs -notlike "*$requiredMarker*") {
        Add-ArchitectureError "parse_path_args.rs must reuse opening PC parser marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("PercentArgs::new", "PercentQueueMode::Observed", '"--bag-aligned"', '"--max-patterns"')) {
    if ($parsePercentArgs -notlike "*$requiredMarker*") {
        Add-ArchitectureError "parse_percent_args.rs must own percent args marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("SetupArgs::new", '"--fixed"', '"--observed"')) {
    if ($parseSetupArgs -notlike "*$requiredMarker*") {
        Add-ArchitectureError "parse_setup_args.rs must own setup args marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("CoverArgs::new", '"--template-json"', '"--template-file"', '"--export-template-json"')) {
    if ($parseCoverArgs -notlike "*$requiredMarker*") {
        Add-ArchitectureError "parse_cover_args.rs must own cover args marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("RulesAction::parse", "RulesArgs::new", '"--profile"', '"--input"')) {
    if ($parseRulesArgs -notlike "*$requiredMarker*") {
        Add-ArchitectureError "parse_rules_args.rs must own rules args marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("ScoringAction::parse", "ScoringArgs::new", '"--profile"', '"--input"')) {
    if ($parseScoringArgs -notlike "*$requiredMarker*") {
        Add-ArchitectureError "parse_scoring_args.rs must own scoring args marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("ConvertArgs::new", '"--from"', '"--to"')) {
    if ($parseConvertArgs -notlike "*$requiredMarker*") {
        Add-ArchitectureError "parse_convert_args.rs must own convert args marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("ContinueArgs::new", '"--token"')) {
    if ($parseContinueArgs -notlike "*$requiredMarker*") {
        Add-ArchitectureError "parse_continue_args.rs must own continue args marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("VerifyArgs::new", '"kicks"')) {
    if ($parseVerifyArgs -notlike "*$requiredMarker*") {
        Add-ArchitectureError "parse_verify_args.rs must own verify args marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("option_value", "unknown_option", "parse_u8_option", "parse_u16_option", "parse_usize_option", "MissingValue", "InvalidValue")) {
    if ($parseOptionValue -notlike "*$requiredMarker*") {
        Add-ArchitectureError "parse_option_value.rs must own common option-value parsing marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("parse_single_char", "InvalidValue")) {
    if ($parsePieceArg -notlike "*$requiredMarker*") {
        Add-ArchitectureError "parse_piece_arg.rs must own single-character CLI argument marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("has_help", "is_positional")) {
    if ($parseHelpers -notlike "*$requiredMarker*") {
        Add-ArchitectureError "parse_helpers.rs must own generic parser helper marker '$requiredMarker'"
    }
}
foreach ($forbiddenMarker in @("RuleProfileAssembler", "parse_rule(", "parse_count_policy", "parse_verified_kick_profile", "KickImport", "VerifiedKickTableProfile", "RuleProfileId::", "PcScenarioQuery::new")) {
    if ($cliParserRouteSurface -like "*$forbiddenMarker*") {
        Add-ArchitectureError "args parser files must only populate args structs; semantic query/rule/kick parsing belongs in assemblers, forbidden marker '$forbiddenMarker'"
    }
}
$obsoleteScenarioTraceKeyContract = "accepted_" + "sample_trace_keys"
foreach ($surface in @(
    @{ Name = "PcScenarioCommand tests"; Contents = $pcScenarioCommandTests },
    @{ Name = "PcScenarioFixture DTO"; Contents = $pcScenarioFixture },
    @{ Name = "PcScenarioExpected verifier"; Contents = $pcScenarioExpected },
    @{ Name = "pc-scenario process E2E"; Contents = $cliProcessE2e }
)) {
    if ($surface.Contents.Contains($obsoleteScenarioTraceKeyContract)) {
        Add-ArchitectureError "$($surface.Name) must use accepted_retained_trace_keys; the removed trace-key contract is ambiguous"
    }
}
$cliCommandEnum = Read-Text "crates/clearra-cli/src/args/cli_args.rs"
$cliCargo = Read-Text "crates/clearra-cli/Cargo.toml"
if ($cliCargo -notlike '*[[bin]]*' -or $cliCargo -notlike '*name = "clearra"*') {
    Add-ArchitectureError "clearra-cli package must build the release-facing binary named clearra"
}
if ($cliCargo -match '(?s)\[\[bin\]\].*?name\s*=\s*"clearra-cli"') {
    Add-ArchitectureError "clearra-cli package binary must be named clearra, not clearra-cli"
}
if ($cliErrorCode -notlike "*pub enum CliErrorCode*" -or $cliErrorCode -notlike "*default_exit_code*") {
    Add-ArchitectureError "clearra-cli must keep stable CLI adapter error codes in error::CliErrorCode"
}
foreach ($file in Get-RustFiles "crates/clearra-cli/src") {
    $relativePath = Resolve-Path -LiteralPath $file.FullName -Relative
    if ($relativePath -like "*error\cli_error_code.rs") {
        continue
    }
    $contents = Get-Content -LiteralPath $file.FullName -Raw
    if ($contents -cmatch 'E_(CLI|CONVERT|PC|SETUP|VERIFY|PATH|PERCENT|RULES|SCORING|CONTINUE)_') {
        Add-ArchitectureError "$($file.FullName) must use CliErrorCode instead of hard-coded CLI adapter error code strings"
    }
}
if ($cliEntrypoint -notlike "*pub fn run_with_args*") {
    Add-ArchitectureError "clearra-cli must expose run_with_args for testable argv routing"
}
if ($cliEntrypoint -notlike "*std::env::args()*") {
    Add-ArchitectureError "clearra-cli::run must pass std::env::args() into the shared router"
}
if ($cliEntrypoint -notlike "*CliParser::parse*") {
    Add-ArchitectureError "clearra-cli::run_with_args must delegate argv parsing to args::CliParser"
}
if ($cliParser -notlike "*extract_global_options*" -or $cliParser -notlike "*RenderFormatSelector::parse*" -or $cliParser -notlike '*"--verbose-paths"*') {
    Add-ArchitectureError "args::CliParser must own global output/path option extraction before command routing"
}
foreach ($forbiddenMarker in @("fn parse_pc", "fn parse_setup", "fn parse_cover", "fn parse_convert", "fn parse_verify", "fn extract_render_format", "fn missing_value", "RenderFormatSelector")) {
    if ($cliEntrypoint -like "*$forbiddenMarker*") {
        Add-ArchitectureError "clearra-cli/src/lib.rs must stay an entrypoint/router and not contain parser marker '$forbiddenMarker'"
    }
}
$concreteCliRoutes = @{
    "pc" = "parse_pc"
    "pc-scenario" = "parse_pc_scenario"
    "path" = "parse_path"
    "percent" = "parse_percent"
    "setup" = "parse_setup"
    "cover" = "parse_cover"
    "rules" = "parse_rules"
    "scoring" = "parse_scoring"
    "convert" = "parse_convert"
    "continue" = "parse_continue"
    "verify" = "parse_verify"
}
$unsupportedCliRoutes = @("inspect")
$helpCliRoutes = @("help")
$knownCliRoutes = @($concreteCliRoutes.Keys) + $unsupportedCliRoutes + $helpCliRoutes
foreach ($commandName in (Get-CliCommandNames $cliCommandEnum)) {
    if (-not ($knownCliRoutes -contains $commandName)) {
        Add-ArchitectureError "CliCommand::$commandName must be classified as concrete, unsupported, or help in validate_architecture.ps1"
        continue
    }

    if ($concreteCliRoutes.ContainsKey($commandName)) {
        $parserName = $concreteCliRoutes[$commandName]
        if ($cliParserRouteSurface -notlike "*`"$commandName`" => $parserName*") {
            Add-ArchitectureError "CliCommand::$commandName must route to $parserName in args::CliParser"
        }
        $variantName = (Get-Culture).TextInfo.ToTitleCase($commandName).Replace("-", "")
        if ($cliEntrypoint -notlike "*ParsedCliCommand::$variantName*") {
            Add-ArchitectureError "clearra-cli/src/lib.rs must route ParsedCliCommand::$variantName to its command handler"
        }
        continue
    }

    if ($unsupportedCliRoutes -contains $commandName) {
        if ($cliParserRouteSurface -notlike "*`"$commandName`"*") {
            Add-ArchitectureError "CliCommand::$commandName must appear in the explicit unsupported parser route"
        }
        continue
    }

    if ($commandName -eq "help" -and (
        $cliParserRouteSurface -notlike '*"help" | "--help" | "-h"*' -or
        $cliParserRouteSurface -notlike '*ParsedCliCommand::Help*'
    )) {
        Add-ArchitectureError "CliCommand::help must route to a parser-owned help topic"
    }
}
foreach ($commandName in $unsupportedCliRoutes) {
    $pattern = "(?s)`"$([regex]::Escape($commandName))`".{0,160}ParsedCliCommand::Unsupported"
    if ($cliParserRouteSurface -notmatch $pattern) {
        Add-ArchitectureError "unsupported CLI command '$commandName' must parse into ParsedCliCommand::Unsupported"
    }
}
if ($cliEntrypoint -notlike "*ParsedCliCommand::Unsupported*" -or $cliEntrypoint -notlike "*UnsupportedCommand::run*") {
    Add-ArchitectureError "clearra-cli/src/lib.rs must route ParsedCliCommand::Unsupported through UnsupportedCommand::run"
}
foreach ($requiredCommand in @("ConvertCommand", "PcScenarioCommand", "PathCommand", "PercentCommand", "RulesCommand", "ScoringCommand", "SetupCommand", "ContinueCommand")) {
    if ($cliCommandsMod -notlike "*$requiredCommand*") {
        Add-ArchitectureError "clearra-cli/src/commands/mod.rs must export concrete handler '$requiredCommand'"
    }
}
if ($cliParserRouteSurface -notlike '*"pc-scenario" => parse_pc_scenario*') {
    Add-ArchitectureError "args::CliParser must route pc-scenario to parse_pc_scenario"
}
foreach ($requiredMarker in @(
    '"path" => parse_path',
    '"percent" => parse_percent',
    '"rules" => parse_rules',
    '"scoring" => parse_scoring',
    "ParsedCliCommand::Path",
    "ParsedCliCommand::Percent",
    "ParsedCliCommand::Rules",
    "ParsedCliCommand::Scoring"
)) {
    if ($cliParserRouteSurface -notlike "*$requiredMarker*" -and $cliEntrypoint -notlike "*$requiredMarker*") {
        Add-ArchitectureError "CLI parser/router must expose MVP2 route marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("--verify-expected", "with_verify_expected", "verify_expected")) {
    if ($cliParserRouteSurface -notlike "*$requiredMarker*" -and $pcScenarioArgs -notlike "*$requiredMarker*") {
        Add-ArchitectureError "pc-scenario CLI args/parser must keep expected fixture verification marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("PcScenarioQueryAssembler::assemble", "validate_pc_scenario_query", "ProblemCompiler::compile_scenario_pc", "CoreExecutor::execute", "PcScenarioExpectedVerifier::verify", "PcScenarioUnsupportedVerifier::verify_validation", "CommandRenderer::render", "scenario_search_error_message")) {
    if ($pcScenarioCommand -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PcScenarioCommand must stay a thin command flow marker '$requiredMarker'"
    }
}
$pcScenarioCommandLineCount = ($pcScenarioCommand -split "`n").Count
if ($pcScenarioCommandLineCount -gt 140) {
    Add-ArchitectureError "PcScenarioCommand must stay thin; current line count is $pcScenarioCommandLineCount"
}
foreach ($forbiddenMarker in @("struct ScenarioFixture", "ScenarioFixtureSource", "ScenarioFixtureExpected", "serde::Deserialize", "fs::read_to_string", "parse_fixed_sequence", "KickImport", "VerifiedKickTableProfile", "compare_accepted_retained_trace_keys", "queue.chars().map(parse_piece)")) {
    if ($pcScenarioCommand -like "*$forbiddenMarker*") {
        Add-ArchitectureError "PcScenarioCommand must not own fixture/query/parser details marker '$forbiddenMarker'"
    }
}
foreach ($requiredMarker in @("PcScenarioQueryAssembler", "PcScenarioAssembly", "inline_query", "query_from_fixture", "input_mode", "PcScenarioQuery::new", "PcScenarioBoard::new", "PieceWindow::new", "RuleProfileAssembler::parse_rule", "RuleProfileAssembler::parse_verified_kick_profile", "PieceSequenceAssembler::parse_fixed_sequence", "with_verified_kick_table_profile", "with_min_remaining_queue", "with_count_policy", "with_retained_trace_limit")) {
    if ($pcScenarioQueryAssembler -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PcScenarioQueryAssembler must own inline/fixture args-to-query adapter marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("PcScenarioFixture", "ScenarioFixtureSource", "ScenarioFixtureExpected", "ScenarioFixtureInput", "deny_unknown_fields", "expected_total_solution_count", "exact_pieces", "min_remaining_queue", "allow_hold", "count_policy", "retained_trace_limit", "accepted_retained_trace_keys", "source_fields", "read_json_file", "rejects_unknown_root_source_and_scenario_fields", "read_rejects_sensitive_file_names_before_opening")) {
    if ($pcScenarioFixture -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PcScenarioFixture must own fixture DTO/source/file IO marker '$requiredMarker'"
    }
}
foreach ($forbiddenMarker in @("fs::read_to_string")) {
    if ($pcScenarioFixture -like "*$forbiddenMarker*") {
        Add-ArchitectureError "PcScenarioFixture must use file_input_guard instead of direct file IO marker '$forbiddenMarker'"
    }
}
foreach ($requiredMarker in @("read_json_file", "validate_json_file_path", "display_input_path", "redacted_path", "with_verbose_paths", "MAX_JSON_INPUT_BYTES", "UnsupportedExtension", "SensitivePath", "Symlink", "TooLarge", "sensitive-looking file path")) {
    if ($fileInputGuard -notlike "*$requiredMarker*") {
        Add-ArchitectureError "file_input_guard.rs must guard CLI JSON file input marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @('"--verbose-paths"', "verbose_paths", "with_verbose_paths")) {
    if ($cliParser -notlike "*$requiredMarker*" -and $cliEntrypoint -notlike "*$requiredMarker*") {
        Add-ArchitectureError "CLI entry/parser must expose explicit verbose path opt-in marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("PcScenarioExpectedVerifier", "verify", "accepted_retained_trace_keys", "retained_trace_keys", "compare_accepted_retained_trace_keys", "retained_trace_keys_checked", "retained_trace_keys_match", "inline_scenario_has_no_expected_contract", "PcScenarioUnsupportedVerifier::verify_search")) {
    if ($pcScenarioExpected -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PcScenarioExpectedVerifier must own supported fixture expected matching marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("PcScenarioUnsupportedVerifier", "verify_validation", "verify_search", "validation_fields", "search_unsupported_reason", "unsupported_stage", "scenario-unsupported-expected", "actual_unsupported_reason", "DiagnosticReport")) {
    if ($pcScenarioUnsupported -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PcScenarioUnsupportedVerifier must own expected unsupported matching marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("RuleProfileAssembler", "parse_rule", "parse_optional_rule", "parse_verified_kick_profile", "KickImport", "VerifiedKickTableProfile", "srs-90", "UnverifiedKickProfile", "rejects_unverified_kick_profile_override_before_search_query_runs")) {
    if ($ruleProfileAssembler -notlike "*$requiredMarker*") {
        Add-ArchitectureError "RuleProfileAssembler must own rule/kick CLI adapter marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("PieceSequenceAssembler", "parse_fixed_sequence", "parse_piece", "QueueParseError", "parses_piece_sequences_with_opening_style_separators")) {
    if ($pieceSequenceAssembler -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PieceSequenceAssembler must own CLI piece sequence parsing marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("pc_scenario_command_fixture_queue_accepts_opening_style_separators", "pc_scenario_command_verifies_accepted_retained_trace_keys", "pc_scenario_command_rejects_unaccepted_retained_trace_keys", "pc_scenario_command_marks_empty_retained_trace_key_expectation_as_not_checked", "pc_scenario_command_treats_expected_unsupported_fixture_as_verified_success")) {
    if ($pcScenarioCommandTests -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PcScenarioCommand tests must preserve scenario command behavior marker '$requiredMarker'"
    }
}
if ($cliErrorCode -notlike "*PcScenarioExpectedMismatch*" -or
    $cliErrorCode -notlike "*E_PC_SCENARIO_EXPECTED_MISMATCH*") {
    Add-ArchitectureError "CliErrorCode must include E_PC_SCENARIO_EXPECTED_MISMATCH for pc-scenario fixture contract failures"
}
foreach ($requiredMarker in @(
    "PathSearchInternal",
    "E_PATH_SEARCH_INTERNAL",
    "PathNoSolution",
    "E_PATH_NO_SOLUTION",
    "PercentQueryInvalid",
    "E_PERCENT_QUERY_INVALID",
    "RulesProfileUnknown",
    "RulesInputRequired",
    "RulesInputInvalid",
    "RulesExportUnsupported",
    "ScoringProfileUnknown",
    "ScoringInputRequired",
    "ScoringInputInvalid"
)) {
    if ($cliErrorCode -notlike "*$requiredMarker*") {
        Add-ArchitectureError "CliErrorCode must include MVP2 CLI adapter code marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("--field", "--max-pieces", "--exact-pieces", "--min-remaining-queue", "--count-policy", "--retained-trace-limit", "--kick-profile-json")) {
    if ($cliParserRouteSurface -notlike "*$requiredMarker*") {
        Add-ArchitectureError "pc-scenario parser must support inline scenario input marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("inline_query", "input_mode", "PcScenarioQuery::new", "PieceWindow::new", "with_verified_kick_profile", "with_min_remaining_queue", "with_count_policy", "with_retained_trace_limit")) {
    if ($pcScenarioQueryAssembler -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PcScenarioQueryAssembler must support inline scenario adapter contract marker '$requiredMarker'"
    }
}
if ($pcScenarioCommandTests -notlike "*pc_scenario_command_accepts_verified_kick_profile_override*") {
    Add-ArchitectureError "PcScenarioCommand tests must cover verified kick profile override behavior"
}
if ($cliParserRouteSurface -notlike '*"convert" => parse_convert*') {
    Add-ArchitectureError "args::CliParser must route convert to parse_convert"
}
foreach ($line in ($cliEntrypoint -split "`n")) {
    if ($line -like '*"convert"*' -and $line -like '*UnsupportedCommand*') {
        Add-ArchitectureError "convert must not be left on the generic unsupported command route"
    }
}
if ($cliCommandsMod -notlike "*UnsupportedCommand*") {
    Add-ArchitectureError "clearra-cli must keep an explicit UnsupportedCommand handler for enum commands outside MVP1"
}
foreach ($requiredMarker in @("CARGO_BIN_EXE_clearra", "Command::new", "process_e2e_pc_command_writes_stdout_and_zero_exit", "process_e2e_opening_pc_json_accepts_duplicate_fixed_sequence", "process_e2e_pc_scenario_fixture_json_counts_solutions", "process_e2e_pc_scenario_expected_unsupported_fixture_succeeds", "--verify-expected", "expected_match", "process_e2e_unknown_command_writes_stderr_and_validation_exit")) {
    if ($cliProcessE2e -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-cli process E2E tests must execute the real binary marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "ContinueCommand",
    "PcContinuationToken",
    "PcContinuationTokenCodec::parse",
    "validate_opening_pc_search_query",
    "validate_pc_scenario_query",
    "ProblemCompiler::compile_opening_pc",
    "ProblemCompiler::compile_scenario_pc",
    "CoreExecutor::execute",
    "interactive_prompt",
    "continue_command_runs_next_pc_from_token_without_prompting"
)) {
    if ($continueCommand -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ContinueCommand must be a concrete non-interactive continuation route marker '$requiredMarker'"
    }
}
if ($continueCommand -like "*UnsupportedCommand*" -or $cliParserRouteSurface -match '(?s)"continue".{0,160}ParsedCliCommand::Unsupported') {
    Add-ArchitectureError "continue must not be left on the unsupported MVP command route"
}
foreach ($requiredMarker in @("sc2:", "sr2:", "scenario-continued-searched", "scenario-replayed-searched", "continuation_kind", "scenario", "scenario-replay", "exact_pieces", "retained_trace_limit")) {
    if ($continueCommand -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ContinueCommand must support non-interactive scenario continuation marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "PcContinuationTokenCodec",
    "encode_opening_continuation",
    "encode_scenario_continuation",
    "encode_scenario_replay",
    "PcContinuationTokenError",
    "PcContinuationToken",
    "parse(token"
)) {
    if ($pcContinuationToken -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PcContinuationToken facade must expose public versioned codec surface marker '$requiredMarker'"
    }
}
foreach ($forbiddenMarker in @("fn parse_opening_v2", "fn parse_scenario_v2", "fn parse_opening_v1", "fn parse_scenario_v1", "format_kick_profile", "parse_kick_profile", "hex_encode", "KickImport", "BoardProfileId", "PieceSetProfileId", "BagProfileId")) {
    if ($pcContinuationToken -like "*$forbiddenMarker*") {
        Add-ArchitectureError "PcContinuationToken facade must not own split codec detail marker '$forbiddenMarker'"
    }
}
$pcContinuationTokenLineCount = ($pcContinuationToken -split "`n").Count
if ($pcContinuationTokenLineCount -gt 120) {
    Add-ArchitectureError "PcContinuationToken facade must stay thin after codec split; current line count is $pcContinuationTokenLineCount"
}
foreach ($requiredMarker in @(
    "encode_opening_continuation",
    "parse_opening_v2",
    "pc2:",
    "BoardProfileId",
    "PieceSetProfileId",
    "BagProfileId",
    "objective_name",
    "parse_objective"
)) {
    if ($openingContinuationToken -notlike "*$requiredMarker*") {
        Add-ArchitectureError "opening_continuation_token.rs must own pc2 opening codec marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "ScenarioContinuationTokenKind",
    "Continuation",
    "Replay",
    "encode_scenario_continuation",
    "encode_scenario_replay",
    "parse_scenario_continuation_v2",
    "parse_scenario_replay_v2",
    "parse_scenario_v2",
    '"sc2"',
    '"sr2"',
    ":k{}",
    "exact_pieces",
    "min_remaining_queue",
    "allow_hold",
    "requires_180",
    "count_policy",
    "retained_trace_limit"
)) {
    if ($scenarioContinuationToken -notlike "*$requiredMarker*") {
        Add-ArchitectureError "scenario_continuation_token.rs must own sc2/sr2 scenario codec marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("parse_opening_v1", "parse_scenario_v1", "pc1", "sc1")) {
    if ($continuationTokenV1 -notlike "*$requiredMarker*") {
        Add-ArchitectureError "continuation_token_v1.rs must own version 1 compatibility marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("PcContinuationTokenError", "message", "impl Error", "impl fmt::Display")) {
    if ($continuationTokenError -notlike "*$requiredMarker*") {
        Add-ArchitectureError "continuation_token_error.rs must own continuation token error marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("parse_target", "parse_hold_piece", "parse_queue", "parse_rule_profile", "parse_objective", "parse_count_policy", "parse_completion_goal", "parse_mask_prefixed", "format_piece_sequence", "format_hold_piece")) {
    if ($continuationTokenSegments -notlike "*$requiredMarker*") {
        Add-ArchitectureError "continuation_token_segments.rs must own shared token segment marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("format_kick_profile", "parse_kick_profile", "hex_encode", "hex_decode", "KickImport", "VerifiedKickTableProfile")) {
    if ($continuationKickProfileCodec -notlike "*$requiredMarker*") {
        Add-ArchitectureError "continuation_kick_profile_codec.rs must own verified kick profile codec marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "v1_tokens_migrate_to_current_encoding",
    "opening_v2_token_preserves_rule_objective_and_profile_contract",
    "scenario_v2_token_preserves_full_query_contract",
    "scenario_replay_token_is_separate_from_continuation_token",
    "scenario_v2_token_preserves_verified_kick_profile_override"
)) {
    if ($pcContinuationTokenTests -notlike "*$requiredMarker*") {
        Add-ArchitectureError "pc_continuation_token_tests.rs must preserve versioned continuation token fidelity test marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("clearra-scoring")) {
    if (-not (Test-DependencyLine $cliCargo $requiredMarker)) {
        Add-ArchitectureError "clearra-cli must depend on $requiredMarker for MVP2 scoring commands"
    }
}
foreach ($requiredMarker in @("ProblemCompiler::compile_opening_pc", "CoreExecutor::execute", "PathTraceUnavailable", "sample_trace_available=true", "path_steps()", "CommandRenderer::render", "validate_opening_pc_search_query")) {
    if ($pathCommand -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PathCommand must stay a thin trace adapter around opening search marker '$requiredMarker'"
    }
}
foreach ($forbiddenMarker in @("CheckpointDag", "SearchOrchestrator", "AllCollector", "ObjectivePolicy", "fn run_pc_search", "struct PcSearchSummary")) {
    if ($pathCommand -like "*$forbiddenMarker*") {
        Add-ArchitectureError "PathCommand must not own PC search orchestration marker '$forbiddenMarker'"
    }
}
foreach ($requiredMarker in @("PercentQueryAssembler", "AppRequest::new", "AppCommand::Percent", "PercentAppCommand::new", "AppResponseRenderer::render")) {
    if ($percentCommand -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PercentCommand must own CLI percent adapter marker '$requiredMarker'"
    }
}
if ($percentCommand -like "*ObservedQueueExpansion*") {
    Add-ArchitectureError "PercentCommand must not materialize observed supply outside clearra-supply"
}
foreach ($requiredMarker in @("RulesAction::List", "RulesAction::Inspect", "RulesAction::Verify", "RulesAction::Import", "RulesAction::Export", "RulesListAction::run", "RulesInspectAction::run", "RulesVerifyAction::run", "RulesImportAction::run", "RulesExportAction::run")) {
    if ($rulesCommand -notlike "*$requiredMarker*") {
        Add-ArchitectureError "RulesCommand must stay a thin action dispatcher marker '$requiredMarker'"
    }
}
$rulesCommandLineCount = ($rulesCommand -split "`n").Count
if ($rulesCommandLineCount -gt 80) {
    Add-ArchitectureError "RulesCommand must stay thin after action split; current line count is $rulesCommandLineCount"
}
foreach ($forbiddenMarker in @("KickProfileRegistry", "KickImport", "KickContractReport", "RuleCapability", "VerifiedKickTableProfile", "invalid_import_profile", "CommandRenderer::render", "builtin_kick_profile")) {
    if ($rulesCommand -like "*$forbiddenMarker*") {
        Add-ArchitectureError "RulesCommand must not own rules feature adapter marker '$forbiddenMarker'"
    }
}
foreach ($requiredMarker in @("KickProfileRegistry", "source_kind", "source_description", "capability_fields")) {
    if ($rulesListAction -notlike "*$requiredMarker*") {
        Add-ArchitectureError "RulesListAction must own rules list adapter marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("KickProfileRegistry", "RuleCapability", "effective_kick_model", "requires_lock_reachability", "requires_spawn_reachability", "supports_exact_180", "c_compact_descriptor_ready", "unsupported_backend_reason")) {
    if ($rulesInspectAction -notlike "*$requiredMarker*") {
        Add-ArchitectureError "RulesInspectAction must own rules inspect adapter marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("KickImport", "KickContractReport", "verify_imported_profile", "kick_verification_failures", "transition_complete", "verified_profile", "c_compact_descriptor_ready", "unsupported_backend_reason")) {
    if ($rulesVerifyAction -notlike "*$requiredMarker*") {
        Add-ArchitectureError "RulesVerifyAction must own rules verify adapter marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("VerifiedKickTableProfile", "VerifiedKickTableProfile::try_new", "invalid_import_profile", "verified_profile", "supports_exact_180", "c_compact_descriptor_ready", "unsupported_backend_reason")) {
    if ($rulesImportAction -notlike "*$requiredMarker*") {
        Add-ArchitectureError "RulesImportAction must accept only verified kick profiles marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("KickImport", "builtin_kick_profile", "RulesExportUnsupported", "to_json")) {
    if ($rulesExportAction -notlike "*$requiredMarker*") {
        Add-ArchitectureError "RulesExportAction must own rules export adapter marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("CommandRenderer::render", "SummaryRenderContract::render_fields", "KickTableProfileId", "builtin_kick_profile", "KickProfileCapability", "supports_exact_180", "c_compact_descriptor_ready", "unsupported_backend_reason")) {
    if ($rulesOutputFields -notlike "*$requiredMarker*") {
        Add-ArchitectureError "rules_output_fields.rs must own rules render/profile field helpers marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("rules_verify_input_reports_issues_without_failing", "rules_import_rejects_unverified_imported_profiles", "rules_command_discloses_generic_srs_plus_and_unsupported_extension_backends", "rules_import_marks_verified_exact_180_profile_as_c_descriptor_ready")) {
    if ($rulesCommandTests -notlike "*$requiredMarker*") {
        Add-ArchitectureError "RulesCommand tests must preserve rules command behavior marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("ScoringAction::List", "ScoringAction::Inspect", "ScoringAction::Import", "ScoringAction::Export", "ScoringListAction::run", "ScoringInspectAction::run", "ScoringImportAction::run", "ScoringExportAction::run")) {
    if ($scoringCommand -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoringCommand must stay a thin action dispatcher marker '$requiredMarker'"
    }
}
$scoringCommandLineCount = ($scoringCommand -split "`n").Count
if ($scoringCommandLineCount -gt 70) {
    Add-ArchitectureError "ScoringCommand must stay thin after action split; current line count is $scoringCommandLineCount"
}
foreach ($forbiddenMarker in @("ScoreProfileRegistry", "ScoreProfileImport", "ScoreProfileExport", "validate_score_profile", "profile_fields", "CommandRenderer::render")) {
    if ($scoringCommand -like "*$forbiddenMarker*") {
        Add-ArchitectureError "ScoringCommand must not own scoring feature adapter marker '$forbiddenMarker'"
    }
}
foreach ($requiredMarker in @("ScoreProfileRegistry", "profile_fields", "profile_count")) {
    if ($scoringListAction -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoringListAction must own scoring list adapter marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("ScoreProfileRegistry", "ScoringProfileUnknown", "profile_fields")) {
    if ($scoringInspectAction -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoringInspectAction must own scoring inspect adapter marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("ScoreProfileImport", "validate_score_profile", "diagnostic_count", "profile_fields")) {
    if ($scoringImportAction -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoringImportAction must own scoring import/validation adapter marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("ScoreProfileExport", "ScoreProfileRegistry", "ScoringProfileUnknown", "to_json")) {
    if ($scoringExportAction -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoringExportAction must own scoring export adapter marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("CommandRenderer::render", "SummaryRenderContract::render_fields", "accuracy_level", "profile_specific_exact", "accuracy_reason", "combo_score_bonus_per_combo", "b2b_attack_bonus")) {
    if ($scoringOutputFields -notlike "*$requiredMarker*") {
        Add-ArchitectureError "scoring_output_fields.rs must own scoring render/profile field helpers marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("scoring_command_lists_and_inspects_canonical_profiles", "scoring_command_imports_and_exports_json_profiles", "basic-approximation")) {
    if ($scoringCommandTests -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoringCommand tests must preserve scoring command behavior marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("SetupQueryAssembler::assemble", "AppCommand::Setup", "SetupAppCommand::new", "AppResponseRenderer::render")) {
    if ($setupCommand -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SetupCommand must remain a thin setup product adapter marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("validate_setup_search_query", "execute_setup_with_control", "AppRenderModel::Setup")) {
    if ($setupAppCommand -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SetupAppCommand must validate and execute the exact setup backend marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("WasmSetupSearchBackend", "WasmSetupSearchSession", "execute_with_control")) {
    if ($setupBackend -notlike "*$requiredMarker*") {
        Add-ArchitectureError "setup backend must own cooperative WASM execution marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("SetupCoverageSession", "SetupSupplyTransitionCatalog", "merge_exact_state_coverage", "setup_coverage_semantics", '"oracle"', "representative_paths")) {
    if ($setupFinder -notlike "*$requiredMarker*") {
        Add-ArchitectureError "setup finder must own family-quotient and exact product coverage marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("PartialBuildGraph", "PartialBuildGraphBuilder", "GeometryCompletionOracle", "compact_live_graph", "1..=9")) {
    if ($setupPartialBuild -notlike "*$requiredMarker*") {
        Add-ArchitectureError "setup partial BuildUp graph must own partial-state transition marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("SetupCoverageGraph", "intern_node", "source_classes", "node_edges", "edge_scratch.sort_unstable", "edge_scratch.dedup")) {
    if ($setupCoverageGraph -notlike "*$requiredMarker*") {
        Add-ArchitectureError "setup coverage quotient must preserve exact canonical graph marker '$requiredMarker'"
    }
}
foreach ($obsoletePath in @(
    "crates/clearra-setup-search/src/service/setup_search_service.rs",
    "crates/clearra-setup-search/src/service/setup_shape_packer.rs"
)) {
    if (Test-Path -LiteralPath (Join-Path $Root $obsoletePath)) {
        Add-ArchitectureError "obsolete setup candidate fabrication path must not exist: $obsoletePath"
    }
}
foreach ($requiredMarker in @("process_e2e_pc_scenario_inline_json_counts_solutions", "process_e2e_mvp2_cli_commands_are_routed", "process_e2e_continue_command_accepts_scenario_token_without_prompting")) {
    if ($cliProcessE2e -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-cli process E2E must cover MVP2 CLI route marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @('\"schema_version\":2', '\"summary\":{', '\"contract\":{\"command\"', '\"solution_found\":true', '\"total_solution_count\":1', '\"count_complete\":true')) {
    if (-not $cliProcessE2e.Contains($requiredMarker) -and -not $cliEntrypoint.Contains($requiredMarker)) {
        Add-ArchitectureError "clearra-cli tests must assert typed MVP2 JSON contract marker '$requiredMarker'"
    }
}
foreach ($crateName in @("clearra-pc-graph", "clearra-problem", "clearra-core-executor", "clearra-objectives")) {
    if (-not (Test-DependencyLine $cliCargo $crateName)) {
        Add-ArchitectureError "clearra-cli pc command must depend on $crateName so pc does not stop at validation"
    }
}
foreach ($crateName in @("clearra-search", "clearra-geometry", "clearra-two-line")) {
    if (Test-DependencyLine $cliCargo $crateName) {
        Add-ArchitectureError "clearra-cli must not depend on low-level PC orchestration crate $crateName; PC execution details belong to clearra-problem plus clearra-core-executor"
    }
}
