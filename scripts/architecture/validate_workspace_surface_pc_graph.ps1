# This file is dot-sourced by Invoke-WorkspaceSurfaceArchitectureValidation.
# It intentionally contains ordered validation statements, not a standalone entrypoint.

$pcGraphCargo = Read-Text "crates/clearra-pc-graph/Cargo.toml"
foreach ($crateName in @("clearra-profiles", "clearra-rules", "clearra-supply", "clearra-objectives")) {
    if (-not (Test-DependencyLine $pcGraphCargo $crateName)) {
        Add-ArchitectureError "clearra-pc-graph must depend on $crateName because opening/scenario PC query types own canonical PC query contracts"
    }
}

$openingPcSearchQuery = Read-Text "crates/clearra-pc-graph/src/request/opening_pc_search_query.rs"
foreach ($requiredMarker in @(
    "pub struct OpeningPcSearchQuery",
    "target: PcTarget",
    "board: BoardProfile",
    "piece_set: PieceSetProfile",
    "bag: BagProfile",
    "queue: PcQueueInput",
    "hold_policy: PcHoldPolicy",
    "rule: RuleProfile",
    "verified_kick_profile: Option<VerifiedKickTableProfile>",
    "with_verified_kick_table_profile",
    "objective: ObjectivePolicy",
    "standard_mvp"
)) {
    if ($openingPcSearchQuery -notlike "*$requiredMarker*") {
        Add-ArchitectureError "OpeningPcSearchQuery must own empty-field opening PC query contract marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "opening_query_owns_empty_field_pc_search_contract",
    "opening_query_can_carry_cli_supplied_queue_hold_and_objective",
    "opening_query_can_carry_verified_imported_kick_profile_override"
)) {
    if ($openingPcSearchQuery -notlike "*$requiredMarker*") {
        Add-ArchitectureError "OpeningPcSearchQuery tests must lock the canonical opening PC query marker '$requiredMarker'"
    }
}
foreach ($forbiddenMarker in @("initial_board", "PcScenarioBoard", "occupied_mask", "Board64State", "BoardMask", "with_initial_board")) {
    if ($openingPcSearchQuery -like "*$forbiddenMarker*") {
        Add-ArchitectureError "OpeningPcSearchQuery must not accept scenario/initial-board input marker '$forbiddenMarker'"
    }
}

$pcScenarioQuery = Read-Text "crates/clearra-pc-graph/src/request/pc_scenario_query.rs"
foreach ($requiredMarker in @(
    "pub struct PcScenarioQuery",
    "initial_board: PcScenarioBoard",
    "remaining_queue: PcQueueInput",
    "hold_state: HoldSlot",
    "piece_window: PieceWindow",
    "verified_kick_profile: Option<VerifiedKickTableProfile>",
    "with_verified_kick_table_profile",
    "exact_pieces: Option<usize>",
    "min_remaining_queue: usize",
    "allow_hold: bool",
    "requires_180: bool",
    "completion_goal: PcCompletionGoal",
    "PcCompletionGoal::ClearToEmpty",
    "count_policy: PcCountPolicy",
    "retained_trace_limit: usize",
    "scenario_query_owns_setup_completion_contract_without_pc_target",
    "scenario_query_owns_completion_constraints",
    "scenario_query_can_carry_verified_imported_kick_profile_override"
)) {
    if ($pcScenarioQuery -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PcScenarioQuery must own setup completion scenario query contract marker '$requiredMarker'"
    }
}
foreach ($forbiddenMarker in @("target: PcTarget", "PcTarget", "CheckpointDag", "LinePartition", "PhaseIncrement")) {
    if ($pcScenarioQuery -like "*$forbiddenMarker*") {
        Add-ArchitectureError "PcScenarioQuery must not depend on opening target/DAG concepts marker '$forbiddenMarker'"
    }
}
foreach ($file in Get-ProductionRustFiles) {
    $relativePath = Resolve-Path -LiteralPath $file.FullName -Relative
    if ($relativePath -like "*crates\clearra-pc-graph\src\request\mod.rs") {
        continue
    }
    $contents = Get-Content -LiteralPath $file.FullName -Raw
    if ($contents -like "*PcScenarioQuery*" -and (
        $contents -like "*PcRequestNormalizer*" -or
        $contents -like "*CheckpointDag::from_query*" -or
        $contents -like "*CheckpointDag::from_opening_query*" -or
        $contents -like "*partitions_for_target*"
    )) {
        Add-ArchitectureError "$relativePath must not route PcScenarioQuery through opening normalizer/DAG code"
    }
}

$pcCanonicalOwnerDefinitions = @(
    @{
        Marker = "pub struct OpeningPcSearchQuery"
        Allowed = "crates/clearra-pc-graph/src/request/opening_pc_search_query.rs"
    },
    @{
        Marker = "pub struct PcScenarioQuery"
        Allowed = "crates/clearra-pc-graph/src/request/pc_scenario_query.rs"
    },
    @{
        Marker = "pub enum PcQueueInput"
        Allowed = "crates/clearra-pc-graph/src/request/pc_queue_input.rs"
    },
    @{
        Marker = "pub enum PcHoldPolicy"
        Allowed = "crates/clearra-pc-graph/src/request/pc_hold_policy.rs"
    }
)

foreach ($file in Get-RustFiles "crates") {
    $contents = Get-Content -LiteralPath $file.FullName -Raw
    $relativePath = $file.FullName.Substring($Root.Path.Length + 1).Replace("\", "/")
    if ($contents.Contains("pub struct PcSearchQuery")) {
        Add-ArchitectureError "$relativePath must not define generic PcSearchQuery; split opening and scenario query contracts"
    }
    foreach ($owner in $pcCanonicalOwnerDefinitions) {
        $ownerPattern = [regex]::Escape($owner.Marker) + '\b'
        if ($contents -match $ownerPattern -and $relativePath -ne $owner.Allowed) {
            Add-ArchitectureError "$relativePath must not define canonical PC query owner marker '$($owner.Marker)'; use $($owner.Allowed)"
        }
    }
}

$forbiddenPcQueryOwnerMarkers = @(
    "struct PcRuntimeInput",
    "struct PcCommandQuery",
    "struct SearchQuery",
    "struct PcSearchInput",
    "struct PcRuntimeQuery",
    "struct PcScenarioInput",
    "struct PcCompletionQuery",
    "enum PcRuntimeInput",
    "enum PcCommandQuery",
    "enum SearchQuery",
    "enum PcSearchInput",
    "enum PcRuntimeQuery",
    "enum PcScenarioInput",
    "enum PcCompletionQuery"
)

foreach ($file in Get-RustFiles "crates") {
    $contents = Get-Content -LiteralPath $file.FullName -Raw
    $relativePath = $file.FullName.Substring($Root.Path.Length + 1).Replace("\", "/")
    foreach ($marker in $forbiddenPcQueryOwnerMarkers) {
        if ($contents -match ([regex]::Escape($marker) + '\b')) {
            Add-ArchitectureError "$relativePath defines forbidden temporary PC query owner marker '$marker'; extend OpeningPcSearchQuery or PcScenarioQuery in clearra-pc-graph instead"
        }
    }
}

$pcAssembler = Read-Text "crates/clearra-cli/src/assemble/pc_query_assembler.rs"
foreach ($requiredMarker in @("OpeningPcSearchQuery", "PcQueueInput", "PcHoldPolicy", "parse_fixed_sequence", "parse_observed_queue", "parse_objective", "KickImport", "VerifiedKickTableProfile", "with_verified_kick_table_profile", "rejects_unverified_kick_profile_override_before_search_query_runs")) {
    if ($pcAssembler -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PcQueryAssembler must assemble CLI pc --lines input into canonical OpeningPcSearchQuery marker '$requiredMarker'"
    }
}

$pcArgs = Read-Text "crates/clearra-cli/src/args/pc_args.rs"
foreach ($requiredMarker in @("queue: String", "fixed_queue: bool", "hold_enabled: bool", "objective: String", "rule: Option<String>", "kick_profile_json: Option<String>")) {
    if ($pcArgs -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PcArgs must carry parser-owned PC query inputs including marker '$requiredMarker'"
    }
}

$pcCommand = Read-Text "crates/clearra-cli/src/commands/pc_command.rs"
foreach ($requiredMarker in @("PcQueryAssembler::assemble", "AppCommand::Pc", "PcAppCommand::new", "AppResponseRenderer::render")) {
    if ($pcCommand -notlike "*$requiredMarker*") {
        Add-ArchitectureError "PcCommand must stay a thin adapter around assemble -> clearra-app -> render marker '$requiredMarker'"
    }
}
foreach ($forbiddenMarker in @(
    "CheckpointDag",
    "SearchDispatcher",
    "SearchOrchestrator",
    "AllCollector",
    "ObjectivePolicy",
    "Board64Layout",
    "Board64State",
    "TwoLineCapabilityInput",
    "FullPcPathResult",
    "SearchMetrics",
    "fn run_pc_search",
    "struct PcSearchSummary",
    "objective_name"
)) {
    if ($pcCommand -like "*$forbiddenMarker*") {
        Add-ArchitectureError "PcCommand must not own PC search orchestration or result summarization marker '$forbiddenMarker'; use clearra-problem plus clearra-core-executor"
    }
}
if ($pcCommand -like "*validate_pc_target(query.target())*") {
    Add-ArchitectureError "PcCommand must validate the full OpeningPcSearchQuery, not only PcTarget"
}
if ($pcCommand -like '*"validated".to_owned()*') {
    Add-ArchitectureError "PcCommand success output must report searched results, not validation-only status"
}

$observedQueueExpansion = Read-Text "crates/clearra-supply/src/normalize/observed_queue_expansion.rs"
$observedQueueExpansionTests = Read-Text "crates/clearra-supply/src/normalize/observed_queue_expansion_tests.rs"
$observedSuffixEnumerator = Read-Text "crates/clearra-supply/src/normalize/observed_suffix_enumerator.rs"
$observedQueueExpansionSurface = "$observedQueueExpansion`n$observedQueueExpansionTests`n$observedSuffixEnumerator"
$ruleCapability = Read-Text "crates/clearra-rules/src/profile/rule_capability.rs"
$ruleValidator = @(
    Read-Text "crates/clearra-validation/src/validators/rule_validator.rs"
    Read-Text "crates/clearra-validation/src/validators/rule_capability_validator.rs"
    Read-Text "crates/clearra-validation/src/validators/rule_diagnostic_builder.rs"
    Read-Text "crates/clearra-validation/src/validators/rule_verified_kick_profile_validator.rs"
    Read-Text "crates/clearra-validation/src/validators/rule_validator_tests.rs"
) -join "`n"
$customRuleEditorContract = Read-Text "crates/clearra-rules/src/custom_rule/custom_rule_editor_contract.rs"
$customRuleMod = Read-Text "crates/clearra-rules/src/custom_rule/mod.rs"
$customRuleValidator = Read-Text "crates/clearra-validation/src/validators/custom_rule_validator.rs"
$kickTable = Read-Text "crates/clearra-rules/src/kicks/kick_table.rs"
$kickImport = Read-Text "crates/clearra-rules/src/kicks/kick_import.rs"
$kickProfileRegistry = Read-Text "crates/clearra-rules/src/kicks/kick_profile_registry.rs"
$kickVerification = Read-Text "crates/clearra-rules/src/kicks/kick_verification.rs"
$kickMod = Read-Text "crates/clearra-rules/src/kicks/mod.rs"
$noKick = Read-Text "crates/clearra-rules/src/kicks/no_kick.rs"
$srsKicks = Read-Text "crates/clearra-rules/src/kicks/srs_kicks.rs"
$srsKicksTests = Read-Text "crates/clearra-rules/src/kicks/srs_kicks_tests.rs"
$srsKicksSurface = "$srsKicks`n$srsKicksTests"
$kickContract = Read-Text "crates/clearra-rules/src/kicks/kick_contract.rs"
$rulesAndKicksDoc = Read-Text "docs/rules-and-kicks.md"
$verifyCommand = Read-Text "crates/clearra-cli/src/commands/verify_command.rs"
$cliParser = Read-Text "crates/clearra-cli/src/args/cli_parser.rs"
$cliCommandParser = Read-Text "crates/clearra-cli/src/args/cli_command_parser.rs"
