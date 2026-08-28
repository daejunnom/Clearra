# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# U3 verifies the typed GUI boundary by inspecting the executable Tauri route.

function Invoke-GuiHostBoundaryContractValidation() {
    $requestBuilder = Read-Text "crates/clearra-gui-host/src/request/gui_to_app_request.rs"
    $desktopBridge = Read-Text "crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs"
    $tauriCommands = Read-Text "apps/clearra-desktop/src-tauri/src/commands.rs"
    $desktopClient = Read-Text "packages/clearra-ui/src/lib/host/clearraDesktopHost.ts"
    $uiSchemaSurface = @(
        Read-Text "crates/clearra-ui-schema/src/setup_explorer/backend_options_schema.rs"
        Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_backend_columns.rs"
    ) -join "`n"

    foreach ($requiredMarker in @(
        "GuiToAppRequest",
        "AppRequest",
        "AppContext::default().validate_request"
    )) {
        if ($requestBuilder -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U3 GUI request builder is missing typed boundary marker '$requiredMarker'"
        }
    }
    foreach ($requiredMarker in @(
        "desktop_form_builds_app_request",
        "GuiToAppRequest::build",
        "self.app_context.validate_request",
        "self.app_context.run(request)",
        "response.to_host_response_with_solution_set_artifact"
    )) {
        if ($desktopBridge -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U3 desktop bridge is missing executable boundary marker '$requiredMarker'"
        }
    }
    if ($tauriCommands -notlike "*DesktopTauriCommandBridge*") {
        Add-ArchitectureError "U3 Tauri commands must call clearra-gui-host"
    }
    if ($desktopClient -notlike "*invoke<string>('run_request'*") {
        Add-ArchitectureError "U3 desktop UI must invoke the typed run_request command"
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

    $productSurface = $requestBuilder + "`n" + $desktopBridge + "`n" + $tauriCommands + "`n" + $desktopClient
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
