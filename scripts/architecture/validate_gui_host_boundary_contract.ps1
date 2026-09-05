# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# U3 verifies the typed GUI boundary by inspecting the executable Tauri route.

function Invoke-GuiHostBoundaryContractValidation() {
    $desktopBridge = Read-Text "crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs"
    $guiJobRunner = Read-Text "crates/clearra-gui-host/src/job/gui_job_runner.rs"
    $productionBoundaryTest = Read-Text "crates/clearra-gui-host/tests/desktop_cli_boundary.rs"
    $guiHostLibrary = Read-Text "crates/clearra-gui-host/src/lib.rs"
    $requestModule = Read-Text "crates/clearra-gui-host/src/request/mod.rs"
    $tauriCommands = Read-Text "apps/clearra-desktop/src-tauri/src/commands.rs"
    $desktopClient = Read-Text "packages/clearra-ui/src/lib/host/clearraDesktopHost.ts"
    $desktopStore = Read-Text "packages/clearra-ui/src/lib/stores/desktopJobStore.ts"
    $guiPcProductSurface = @(
        Read-Text "packages/clearra-ui/src/lib/wasm/wasmCommandClient.ts"
        Read-Text "packages/clearra-ui/src/lib/workspace/productResultPager.ts"
        Read-Text "packages/clearra-ui/src/lib/workspace/ProductResultPager.svelte"
        Read-Text "packages/clearra-ui/src/lib/workspace/solverWorkspaceModel.ts"
        Read-Text "packages/clearra-ui/src/lib/workspace/SearchControls.svelte"
        Read-Text "packages/clearra-ui/src/lib/workspace/PcSolverStandalone.svelte"
    ) -join "`n"
    $uiSchemaSurface = @(
        Read-Text "crates/clearra-ui-schema/src/setup_explorer/backend_options_schema.rs"
        Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_backend_columns.rs"
    ) -join "`n"

    foreach ($requiredMarker in @(
        "mod cli_request_parser",
        '"clearra-cli/CommandRequest"',
        "CliCommandParser::parse_tokens",
        "to_app_request",
        "cfg(not(test))",
        "mod active_request_parser",
        "self.app_context.validate_request",
        "self.app_context.run(request)",
        "response.to_host_response()"
    )) {
        if ($desktopBridge -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U3 desktop bridge is missing executable boundary marker '$requiredMarker'"
        }
    }
    if ($desktopBridge -like "*to_host_response_with_solution_set_artifact*" -or
        $guiJobRunner -like "*to_host_response_with_solution_set_artifact*") {
        Add-ArchitectureError "U3 GUI completion must defer CTK3/Fumen documents to explicit page or export requests"
    }
    if ($tauriCommands -notlike "*DesktopTauriCommandBridge*") {
        Add-ArchitectureError "U3 Tauri commands must call clearra-gui-host"
    }
    if ($desktopClient -notlike "*invoke<string>('run_request'*") {
        Add-ArchitectureError "U3 desktop UI must invoke the typed run_request command"
    }
    foreach ($requiredMarker in @(
        "ClearraDesktopRequest = ClearraDesktopCliCommandRequest",
        "app_request_model: 'clearra-cli/CommandRequest'",
        "arguments: string"
    )) {
        if ($desktopClient -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U3 desktop client is missing closed CLI envelope marker '$requiredMarker'"
        }
    }
    foreach ($requiredMarker in @(
        "requireCompleteDesktopCliRequest",
        "Desktop requests require a complete canonical CLI argv envelope"
    )) {
        if ($desktopStore -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U3 desktop store is missing complete-request guard '$requiredMarker'"
        }
    }
    if ($requestModule -notmatch '(?s)#\[cfg\(test\)\]\s*mod gui_to_app_request;') {
        Add-ArchitectureError "U3 legacy GuiToAppRequest assembler must be compiled only for tests"
    }
    if ($guiHostLibrary -notmatch '(?s)#\[cfg\(test\)\]\s*pub mod request;') {
        Add-ArchitectureError "U3 legacy GUI request-builder layer must be absent from production builds"
    }
    if ($guiHostLibrary -notmatch '(?s)#\[cfg\(test\)\]\s*pub use request::\{\s*BackendRequestBuilder') {
        Add-ArchitectureError "U3 legacy GUI request-builder exports must be test-only"
    }
    foreach ($requiredMarker in @(
        "production_entrypoint_accepts_only_the_complete_cli_envelope",
        "production_entrypoint_does_not_reexpose_retired_gui_save_products",
        "DesktopTauriCommandBridge::default().validate_request"
    )) {
        if ($productionBoundaryTest -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U3 production-linked desktop boundary test is missing marker '$requiredMarker'"
        }
    }
    foreach ($forbiddenMarker in @(
        "clearra-app/AppRequest",
        "buildDesktopAppRequest",
        "ClearraDesktopRequestInput",
        "Partial<ClearraDesktopRequest>"
    )) {
        if ($desktopClient -like "*$forbiddenMarker*" -or $desktopStore -like "*$forbiddenMarker*") {
            Add-ArchitectureError "U3 production desktop TypeScript boundary contains legacy marker '$forbiddenMarker'"
        }
    }
    foreach ($retiredPcProductMarker in @(
        "pc-save-groups",
        "pc-best-save",
        "pc.saves",
        "pc.best-save",
        "ClearraPcSave",
        "ClearraPcBestSave"
    )) {
        if ($guiPcProductSurface -like "*$retiredPcProductMarker*") {
            Add-ArchitectureError "U3 retired PC product leaked into GUI production source: '$retiredPcProductMarker'"
        }
    }

    foreach ($requiredMarker in @(
        "BackendOptionsSchema",
        "gpu_status",
        "gpu_trust_state",
        "memory_pressure_level",
        "backend_fallback_reason"
    )) {
        if ($uiSchemaSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U3 GUI schema is missing result/status marker '$requiredMarker'"
        }
    }

    $productSurface = $desktopBridge + "`n" + $tauriCommands + "`n" + $desktopClient + "`n" + $desktopStore
    foreach ($forbiddenMarker in @(
        "std::process::Command",
        "clearra.exe",
        "run_with_args",
        "CliParser",
        "serde_json::from_str::<AppRequest",
        "serde_json::from_str::<AppResponse",
        "clearra_packing_",
        "clr_buildup_",
        "clearra_board64_",
        '#include "clr_',
        "core-c/include"
    )) {
        if ($productSurface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "U3 GUI boundary contains forbidden marker '$forbiddenMarker'"
        }
    }
}
