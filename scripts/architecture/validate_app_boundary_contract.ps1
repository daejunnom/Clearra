function Assert-AppBoundaryTextContains(
    [string]$Path,
    [string[]]$Markers,
    [string]$ErrorPrefix
) {
    $text = Read-Text $Path
    foreach ($marker in $Markers) {
        if ($text -notlike "*$marker*") {
            Add-ArchitectureError "$ErrorPrefix must contain marker '$marker' in $Path"
        }
    }
}function Invoke-AppBoundaryContractValidation {
foreach ($requiredFile in @(
            "docs/app-boundary.md",
            "crates/clearra-host-contract/Cargo.toml",
            "crates/clearra-host-contract/src/lib.rs",
            "crates/clearra-app/src/request.rs",
            "crates/clearra-app/src/response.rs",
            "crates/clearra-app/src/run_request.rs",
            "scripts/architecture/validate_app_boundary_contract.ps1",
            "crates/clearra-invariant-tests/tests/app_boundary_contract_tests.rs"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "C app boundary contract required file missing: $requiredFile"
        }
    }
Assert-AppBoundaryTextContains "docs/app-boundary.md" @(
        "CLI / GUI / WASM Command Runtime / Desktop host -> AppRequest -> clearra-app -> validation -> executor -> AppResponse",
        "AppCommandKind",
        "QueryEnvelope",
        "BackendPolicy",
        "OutputPolicy",
        "DiagnosticsPolicy",
        "LocalePolicy",
        "ResourceBudget",
        "PcScenario",
        "VerifyKicks",
        "Validation errors return",
        "Warnings may execute",
        "cli_pc_builds_app_request",
        "gui_form_builds_app_request",
        "wasm_command_builds_app_request",
        "app_validation_runs_before_executor",
        "app_error_does_not_execute_solver",
        "output_consumes_app_response_only"
    ) "C app boundary docs"
$hostContract = Read-Text "crates/clearra-host-contract/src/lib.rs"
foreach ($requiredMarker in @(
            "pub enum AppCommandKind",
            "Pc",
            "Path",
            "Percent",
            "Setup",
            "Cover",
            "Continue",
            "Rules",
            "Scoring",
            "Convert",
            "InspectUnsupported",
            "Verify",
            "VerifyKicks",
            "pub enum QueryEnvelope",
            "pub struct BackendPolicy",
            "pub struct OutputPolicy",
            "pub struct DiagnosticsPolicy",
            "pub struct LocalePolicy",
            "pub struct ResourceBudget",
            "pub struct BackendReport",
            "pub struct ResourceReport",
            "pub struct CapabilityReport",
            "pub struct ContinuationReport"
        )) {
        if ($hostContract -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-host-contract must expose C app boundary marker '$requiredMarker'"
        }
    }
$appRequest = Read-Text "crates/clearra-app/src/app_request.rs"
foreach ($requiredMarker in @(
            "command: AppCommand",
            "query: QueryEnvelope",
            "backend_policy: BackendPolicy",
            "output_policy: AppOutputPolicy",
            "diagnostics_policy: DiagnosticsPolicy",
            "locale_policy: LocalePolicy",
            "resource_budget: ResourceBudget",
            "command_kind",
            "backend_policy",
            "resource_budget"
        )) {
        if ($appRequest -notlike "*$requiredMarker*") {
            Add-ArchitectureError "AppRequest must expose typed contract marker '$requiredMarker'"
        }
    }
$appResponse = Read-Text "crates/clearra-app/src/app_response.rs"
foreach ($requiredMarker in @(
            "command: Option<AppCommandKind>",
            "result: Option<AppResult>",
            "backend_report: BackendReport",
            "resource_report: ResourceReport",
            "capability_report: CapabilityReport",
            "continuation: Option<ContinuationReport>",
            "with_contract_context"
        )) {
        if ($appResponse -notlike "*$requiredMarker*") {
            Add-ArchitectureError "AppResponse must expose typed contract marker '$requiredMarker'"
        }
    }
$appContext = Read-Text "crates/clearra-app/src/app_context.rs"
if ($appContext -notmatch "(?s)let validation_report = command.validate\(\).*if validation_report.has_errors\(\).*AppResponse::validation_failed.*else.*command.run\(&execution_context\)") {
        Add-ArchitectureError "app_validation_runs_before_executor: AppContext::run must validate before command.run"
    }
if ($appContext -notlike "*with_contract_context(command_kind)*") {
        Add-ArchitectureError "AppContext::run must attach command kind to AppResponse"
    }
$cliAssembler = Read-Text "crates/clearra-cli/src/assemble/app_request_assembler.rs"
foreach ($requiredMarker in @(
            "cli_pc_builds_app_request",
            "AppRequest::new(AppCommand::Pc",
            "AppCommand::VerifyKicks"
        )) {
        if ($cliAssembler -notlike "*$requiredMarker*") {
            Add-ArchitectureError "CLI assembler must build typed AppRequest marker '$requiredMarker'"
        }
    }
$guiRequestBuilder = Read-Text "crates/clearra-gui-host/src/request/gui_to_app_request.rs"
foreach ($requiredMarker in @(
            "GuiFormValidator::validate_state",
            "AppRequest::new(command)",
            "with_language",
            "with_output_policy"
        )) {
        if ($guiRequestBuilder -notlike "*$requiredMarker*") {
            Add-ArchitectureError "gui_form_builds_app_request: GUI request builder must contain marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @("std::process::Command", "clearra.exe", "CliParser", "serde_json::from_str::<AppRequest")) {
        if ($guiRequestBuilder -like "*$forbiddenMarker*") {
            Add-ArchitectureError "GUI request builder must not use forbidden marker '$forbiddenMarker'"
        }
    }
$wasmRuntime = (Read-PhysicalText "crates/clearra-wasm/src/wasm_command_runtime.rs") + (Read-PhysicalText "crates/clearra-web-command/src/web_command_request.rs")
foreach ($requiredMarker in @(
            "compile_command_text",
            "run_command_text",
            "WebCommandParser::parse",
            "to_app_request",
            "AppRequest::new(AppCommand::Pc",
            "AppRequest::new(AppCommand::Verify",
            "self.app_context.run(request)",
            "into_host_app_response"
        )) {
        if ($wasmRuntime -notlike "*$requiredMarker*") {
            Add-ArchitectureError "wasm_command_builds_app_request: WASM runtime must contain marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @("std::process::Command", "ExitCode", "clearra.exe")) {
        if ($wasmRuntime -like "*$forbiddenMarker*") {
            Add-ArchitectureError "WASM command runtime must not use native process marker '$forbiddenMarker'"
        }
    }
$cliVerifyCommand = Read-Text "crates/clearra-cli/src/commands/verify_command.rs"
foreach ($forbiddenMarker in @("PcCommand::run", "SetupCommand::run", "CoverCommand::run", "KickContractReport::verify_builtin_contracts")) {
        if ($cliVerifyCommand -like "*$forbiddenMarker*") {
            Add-ArchitectureError "CLI verify command must not bypass AppRequest with marker '$forbiddenMarker'"
        }
    }
$responseRenderer = Read-Text "crates/clearra-cli/src/output/app_response_renderer.rs"
foreach ($requiredMarker in @("AppResponse", "response.status()", "response.render_model()", "response.diagnostics()")) {
        if ($responseRenderer -notlike "*$requiredMarker*") {
            Add-ArchitectureError "output_consumes_app_response_only: AppResponseRenderer must contain marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @("validate_opening_pc_search_query", "ProblemCompiler::", "CoreExecutor")) {
        if ($responseRenderer -like "*$forbiddenMarker*") {
            Add-ArchitectureError "output_consumes_app_response_only: AppResponseRenderer must not validate or execute marker '$forbiddenMarker'"
        }
    }
}
