# This file is dot-sourced by an architecture validation wrapper.

function Invoke-CliProductPathValidation() {
foreach ($requiredPath in @(
        "crates/clearra-app/Cargo.toml",
        "crates/clearra-app/src/app_context.rs",
        "crates/clearra-app/src/app_context_tests.rs",
        "crates/clearra-app/src/app_request.rs",
        "crates/clearra-app/src/app_response.rs",
        "crates/clearra-app/src/app_command.rs",
        "crates/clearra-app/src/render/app_render_model.rs"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M18 typed app facade required file is missing: $requiredPath"
        }
    }
$workspaceCargo = Read-Text "Cargo.toml"
if ($workspaceCargo -notlike '*"crates/clearra-app"*') {
        Add-ArchitectureError "workspace must include clearra-app for the typed application API"
    }
$appCargo = Read-Text "crates/clearra-app/Cargo.toml"
foreach ($forbiddenDependency in @("clearra-cli", "clearra-ui-schema", "clearra-core-ffi")) {
        if (Test-DependencyLine $appCargo $forbiddenDependency) {
            Add-ArchitectureError "clearra-app must not depend on $forbiddenDependency"
        }
    }
foreach ($requiredDependency in @("clearra-problem", "clearra-core-executor", "clearra-validation", "clearra-output", "clearra-i18n")) {
        if (-not (Test-DependencyLine $appCargo $requiredDependency)) {
            Add-ArchitectureError "clearra-app must depend on $requiredDependency for the typed application facade"
        }
    }
$cliCargo = Read-Text "crates/clearra-cli/Cargo.toml"
if (-not (Test-DependencyLine $cliCargo "clearra-app")) {
        Add-ArchitectureError "clearra-cli must depend on clearra-app instead of owning app execution"
    }
foreach ($forbiddenDirectAppDependency in @(
            "clearra-core-executor",
            "clearra-problem"
        )) {
        if (Test-DependencyLine $cliCargo $forbiddenDirectAppDependency) {
            Add-ArchitectureError "clearra-cli must route product execution through clearra-app instead of depending directly on $forbiddenDirectAppDependency"
        }
    }
$appContext = (Read-Text "crates/clearra-app/src/app_context.rs") + (Read-Text "crates/clearra-app/src/app_context_tests.rs")
foreach ($requiredMarker in @("app_pc_request_runs_without_cli_parser", "app_scenario_request_runs_without_cli_parser", "app_response_contains_diagnostics_and_render_model", "app_services_exposes_real_di_slots", "AppContext::default().run", "AppRequest::new", "AppExecutionContext", "language_resolver", "diagnostic_sink")) {
        if ($appContext -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-app must expose/test typed App API marker '$requiredMarker'"
        }
    }
$appCommandSurface = (Read-Text "crates/clearra-app/src/app_command.rs") + (Read-Text "crates/clearra-app/src/commands/pc_app_command.rs") + (Read-Text "crates/clearra-app/src/commands/scenario_app_command.rs") + (Read-Text "crates/clearra-app/src/commands/setup_app_command.rs") + (Read-Text "crates/clearra-app/src/commands/cover_app_command.rs") + (Read-Text "crates/clearra-app/src/commands/percent_app_command.rs") + (Read-Text "crates/clearra-app/src/commands/path_app_command.rs") + (Read-Text "crates/clearra-app/src/commands/continue_app_command.rs")
foreach ($requiredMarker in @("validate_opening_pc_search_query", "validate_pc_scenario_query", "validate_setup_search_query", "validate_build_coverage_query", "ProblemCompiler::compile_opening_pc", "ProblemCompiler::compile_scenario_pc", "ProblemCompiler::compile_setup", "ProblemCompiler::compile_build", "core_executor().execute", "execute_percent_with_control", "AppRenderModel")) {
        if ($appCommandSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-app commands must own validation/problem/executor/render-model marker '$requiredMarker'"
        }
    }
$pcCommand = Read-Text "crates/clearra-cli/src/commands/pc_command.rs"
foreach ($requiredMarker in @("PcQueryAssembler::assemble", "AppContext::default().run", "AppCommand::Pc", "PcAppCommand::new", "AppResponseRenderer::render")) {
        if ($pcCommand -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M18 pc command must follow args -> assembler -> clearra-app -> output marker '$requiredMarker'"
        }
    }
$scenarioCommand = Read-Text "crates/clearra-cli/src/commands/pc_scenario_command.rs"
foreach ($requiredMarker in @("PcScenarioQueryAssembler::assemble", "AppCommand::Scenario", "ScenarioAppCommand::new", "expected_unsupported_output", "render_success")) {
        if ($scenarioCommand -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M18 pc-scenario command must execute through clearra-app while keeping fixture expected adapter marker '$requiredMarker'"
        }
    }
$pathCommand = Read-Text "crates/clearra-cli/src/commands/path_command.rs"
foreach ($requiredMarker in @("PcQueryAssembler::assemble", "AppCommand::Path", "PathAppCommand::new", "AppResponseRenderer::render")) {
        if ($pathCommand -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M18 path command must delegate trace execution/render model construction to clearra-app marker '$requiredMarker'"
        }
    }
$percentCommand = Read-Text "crates/clearra-cli/src/commands/percent_command.rs"
foreach ($requiredMarker in @("PercentQueryAssembler::assemble", "AppCommand::Percent", "PercentAppCommand::new", "AppResponseRenderer::render")) {
        if ($percentCommand -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M18 percent command must stay a thin adapter around assembler -> clearra-app marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @("ObservedQueueExpansion::expand", "parse_observed_queue", "parse_bag_aligned_pattern", "parse_fixed_sequence")) {
        if ($percentCommand -like "*$forbiddenMarker*") {
            Add-ArchitectureError "M18 percent command must not parse or expand supply directly marker '$forbiddenMarker'"
        }
    }
$setupCommand = Read-Text "crates/clearra-cli/src/commands/setup_command.rs"
foreach ($requiredMarker in @("SetupQueryAssembler::assemble", "AppCommand::Setup", "SetupAppCommand::new", "AppResponseRenderer::render")) {
        if ($setupCommand -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M18 setup command must execute through clearra-app marker '$requiredMarker'"
        }
    }
if ($setupCommand -like "*SetupSearchService*") {
        Add-ArchitectureError "M18 setup command must not call clearra-setup-search service directly"
    }
$coverCommand = Read-Text "crates/clearra-cli/src/commands/cover_command.rs"
foreach ($requiredMarker in @("CoverQueryAssembler::assemble", "AppCommand::Cover", "CoverAppCommand::new", "with_export_template_json", "AppResponseRenderer::render")) {
        if ($coverCommand -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M18 cover command must execute build coverage through clearra-app marker '$requiredMarker'"
        }
    }
if ($coverCommand -like '*"validated".to_owned()*') {
        Add-ArchitectureError "M18 cover command success output must not stop at validation-only status"
    }
foreach ($commandPath in @(
        "crates/clearra-cli/src/commands/pc_command.rs",
        "crates/clearra-cli/src/commands/pc_scenario_command.rs",
        "crates/clearra-cli/src/commands/path_command.rs",
        "crates/clearra-cli/src/commands/percent_command.rs",
        "crates/clearra-cli/src/commands/setup_command.rs",
        "crates/clearra-cli/src/commands/cover_command.rs",
        "crates/clearra-cli/src/commands/continue_command.rs"
    )) {
        $contents = Read-Text $commandPath
        foreach ($forbiddenMarker in @("CoreExecutor", "ProblemCompiler::", "validate_opening_pc_search_query", "validate_pc_scenario_query", "validate_setup_search_query", "validate_build_coverage_query", "PercentService::")) {
            if ($contents -like "*$forbiddenMarker*") {
                Add-ArchitectureError "$commandPath must not call app execution internals directly; use clearra-app marker '$forbiddenMarker'"
            }
        }
    }
$percentAssembler = Read-Text "crates/clearra-cli/src/assemble/percent_query_assembler.rs"
foreach ($requiredMarker in @("PcScenarioQuery::new", "PcQueueInput::observed", "PcQueueInput::bag_aligned_pattern", "PcQueueInput::fixed_sequence")) {
        if ($percentAssembler -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M18 percent assembler must lower percent input into canonical PcScenarioQuery marker '$requiredMarker'"
        }
    }
$percentService = Read-Text "crates/clearra-core-executor/src/service/percent_service.rs"
foreach ($requiredMarker in @("ObservedQueueExpansion::expand", "SearchProblemPreset::ScenarioPc", "percent_base_fields", "route")) {
        if ($percentService -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M18 PercentService must own percent expansion over compiled SearchProblem marker '$requiredMarker'"
        }
    }
$processE2E = Read-Text "crates/clearra-cli/tests/process_e2e.rs"
foreach ($requiredMarker in @("process_e2e_m18_cli_commands_use_search_problem_executor_route", "pc-scenario", "percent", "setup", "cover", "route: search-problem-core-executor")) {
        if ($processE2E -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M18 process E2E must verify CLI product path command '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @("M18 CLI Product Path", "args -> assembler -> clearra-app -> validation -> clearra-problem -> clearra-core-executor -> output", "percent uses PercentQueryAssembler", "cover lowers BuildCoverageQuery into BuildQuery")) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M18 CLI product path marker '$requiredMarker'"
        }
    }
}



