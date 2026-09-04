# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# R keeps native CLI, desktop, WASM, and WebGPU on the same typed product contract.

function Invoke-HostRuntimeContractValidation() {
foreach ($requiredFile in @(
            "crates/clearra-host-contract/src/app_contract.rs",
            "crates/clearra-host-contract/src/job_event.rs",
            "crates/clearra-wasm/src/host_contract_bridge.rs",
            "crates/clearra-wasm/src/wasm_command_runtime.rs",
            "crates/clearra-wasm/src/wasm_worker_job.rs",
            "crates/clearra-wasm/tests/wasm_host_contract.rs",
            "crates/clearra-cli-command/src/web_command_parser.rs",
            "crates/clearra-webgpu/src/embedded_pattern_bitset_union.wgsl",
            "crates/clearra-webgpu/src/shader_contract.rs",
            "crates/clearra-webgpu/src/webgpu_backend.rs",
            "crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs",
            "crates/clearra-gui-host/src/job/gui_job_runner.rs",
            "crates/clearra-gui-host/tests/desktop_cli_boundary.rs",
            "apps/clearra-desktop/src-tauri/src/commands.rs",
            "apps/clearra-desktop/src-tauri/src/main.rs",
            "packages/clearra-ui/src/lib/host/clearraDesktopHost.ts",
            "packages/clearra-ui/src/lib/stores/desktopJobStore.ts",
            "packages/clearra-ui/src/lib/wasm/wasmCommandClient.ts",
            "docs/app-boundary.md"
        )) {
        if (-not (Test-Path -LiteralPath $requiredFile)) {
            Add-ArchitectureError "R required host runtime contract file missing: $requiredFile"
        }
    }
$hostContractSurface = @(
        Read-Text "crates/clearra-host-contract/src/lib.rs"
        Read-Text "crates/clearra-host-contract/src/app_contract.rs"
        Read-Text "crates/clearra-host-contract/src/job_event.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
            "pub struct AppRequest",
            "pub struct AppResponse",
            "pub enum JobEvent",
            "Started(JobStarted)",
            "Progress(JobProgress)",
            "BackendStatus(BackendStatusReport)",
            "ResourceStatus(ResourceReport)",
            "PartialResult(PartialResult)",
            "Diagnostic(DiagnosticEvent)",
            "Completed(AppResponse)",
            "Cancelled(CancelledReport)",
            "Failed(DiagnosticReport)",
            "cli_gui_wasm_share_app_request_schema",
            "job_event_reports_resource_budget",
            "job_event_reports_search_and_post_backend"
        )) {
        if ($hostContractSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "R HostContract surface must expose marker '$requiredMarker'"
        }
    }
$wasmSurface = @(
        Read-Text "crates/clearra-wasm/src/host_contract_bridge.rs"
        Read-Text "crates/clearra-wasm/src/wasm_command_runtime.rs"
        Read-Text "crates/clearra-wasm/src/wasm_worker_job.rs"
        Read-Text "crates/clearra-wasm/tests/wasm_host_contract.rs"
        Read-Text "crates/clearra-cli-command/src/web_command_parser.rs"
        Read-Text "packages/clearra-ui/src/lib/wasm/wasmCommandClient.ts"
        Read-Text "apps/clearra-web/src/workers/clearraWorker.ts"
    ) -join "`n"
foreach ($requiredMarker in @(
            "command text",
            "CliCommandParser",
            "AppRequest",
            "Web Worker",
            "WASM CPU",
            "WebGPU",
            "AppResponse",
            "JobEvent",
            "wasm_runtime_does_not_spawn_process"
        )) {
        if ($wasmSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "R WASM command runtime must expose marker '$requiredMarker'"
        }
    }
$webGpuSurface = @(
        Read-Text "crates/clearra-webgpu/src/shader_contract.rs"
        Read-Text "crates/clearra-webgpu/src/webgpu_backend.rs"
        Read-Text "crates/clearra-webgpu/src/embedded_pattern_bitset_union.wgsl"
        Read-Text "crates/clearra-wasm/src/webgpu/webgpu_backend_report.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
            "user_provided_wgsl_rejected",
            "no_runtime_shader_injection",
            "embedded_reviewed",
            "shader_hash",
            "shader_version",
            "WebGpuBatchOutcome",
            "Connected",
            "Unavailable",
            "RejectedMismatch",
            "dispatch_workgroups",
            "cpu_confirmed",
            "fallback_backend",
            "wasm-cpu"
        )) {
        if ($webGpuSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "R WebGPU contract must expose marker '$requiredMarker'"
        }
    }
$desktopRustRequestSurface = @(
        Read-PhysicalText "crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs"
        Read-PhysicalText "crates/clearra-gui-host/tests/desktop_cli_boundary.rs"
) -join "`n"
foreach ($requiredMarker in @(
            "CliCommandParser::parse_tokens",
            '"clearra-cli/CommandRequest"',
            "production_entrypoint_accepts_only_the_complete_cli_envelope",
            "production_entrypoint_preserves_literal_exact_argv_without_shell_interpretation"
        )) {
        if ($desktopRustRequestSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "R Desktop Rust request boundary must expose CLI-only marker '$requiredMarker'"
        }
    }
$desktopUiRequestSurface = @(
        Read-PhysicalText "packages/clearra-ui/src/lib/host/clearraDesktopHost.ts"
        Read-PhysicalText "packages/clearra-ui/src/lib/stores/desktopJobStore.ts"
) -join "`n"
foreach ($requiredMarker in @(
            "ClearraDesktopCliCommandRequest",
            "clearra-cli/CommandRequest",
            "requireCompleteDesktopCliRequest"
        )) {
        if ($desktopUiRequestSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "R Desktop UI request boundary must expose complete CLI envelope marker '$requiredMarker'"
        }
    }
$desktopRequestSurface = @($desktopRustRequestSurface, $desktopUiRequestSurface) -join "`n"
$desktopRustResponseSurface = @(
        Read-PhysicalText "crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs"
        Read-PhysicalText "crates/clearra-gui-host/src/job/gui_job_runner.rs"
) -join "`n"
foreach ($requiredMarker in @(
            "AppResponse",
            "response.to_host_response_with_solution_set_artifact",
            "serde_json::to_string"
        )) {
        if ($desktopRustResponseSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "R Desktop Rust response boundary must expose typed AppResponse marker '$requiredMarker'"
        }
    }
$desktopClientResponseSurface = Read-PhysicalText "packages/clearra-ui/src/lib/host/clearraDesktopHost.ts"
foreach ($requiredMarker in @(
            "export type ClearraDesktopAppResponse",
            "runtime_identity: ClearraProductBuildIdentity",
            "capability_report:",
            "app_request_boundary: string",
            "JSON.parse(response) as ClearraDesktopAppResponse"
        )) {
        if ($desktopClientResponseSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "R Desktop client response boundary must expose typed AppResponse marker '$requiredMarker'"
        }
    }
$desktopResponseSurface = @($desktopRustResponseSurface, $desktopClientResponseSurface) -join "`n"
$desktopRouteSurface = @(
        Read-PhysicalText "crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs"
        Read-PhysicalText "crates/clearra-gui-host/src/gui_host_contract_tests.rs"
        Read-PhysicalText "apps/clearra-desktop/src-tauri/src/commands.rs"
        Read-PhysicalText "apps/clearra-desktop/src-tauri/src/main.rs"
        Read-PhysicalText "packages/clearra-ui/src/lib/host/clearraDesktopHost.ts"
    ) -join "`n"
foreach ($requiredMarker in @(
            "run_request",
            "validate_request",
            "start_job",
            "cancel_job",
            "get_job_events",
            "DesktopTauriCommandBridge",
            "desktop_tauri_command_calls_gui_host_only"
        )) {
        if ($desktopRouteSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "R Desktop Tauri route must expose marker '$requiredMarker'"
        }
    }
$desktopSurface = @(
        $desktopRequestSurface
        $desktopResponseSurface
        $desktopRouteSurface
) -join "`n"
foreach ($forbiddenMarker in @(
            "std::process::Command",
            "run_with_args",
            "CliParser",
            "userProvidedWgsl",
            "runtimeShaderInjection",
            "shaderTextFromUser",
            "createShaderModule({ code: data",
            "localized_output_keys",
            "clearra_packing_",
            "clr_buildup_",
            "clearra_board64_"
        )) {
        if ($hostContractSurface.IndexOf($forbiddenMarker, [System.StringComparison]::OrdinalIgnoreCase) -ge 0 -or
            $wasmSurface.IndexOf($forbiddenMarker, [System.StringComparison]::OrdinalIgnoreCase) -ge 0 -or
            $webGpuSurface.IndexOf($forbiddenMarker, [System.StringComparison]::OrdinalIgnoreCase) -ge 0 -or
            $desktopSurface.IndexOf($forbiddenMarker, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
            Add-ArchitectureError "R host runtime contract forbids marker '$forbiddenMarker'"
        }
    }

# Match the executable as a complete path/token component. The host schema id
# `clearra.execution-resource-authority.v1` intentionally begins with the same
# characters and must not be mistaken for a child-process reference.
$clearraExecutablePattern = '(?i)(?<![A-Za-z0-9_.-])clearra\.exe(?![A-Za-z0-9_.-])'
if ([regex]::IsMatch("clearra.execution-resource-authority.v1", $clearraExecutablePattern) -or
    -not ([regex]::IsMatch('Command::new("clearra.exe")', $clearraExecutablePattern))) {
    Add-ArchitectureError "R host runtime clearra.exe token matcher failed its positive/negative regression contract"
}
foreach ($surface in @($hostContractSurface, $wasmSurface, $webGpuSurface, $desktopSurface)) {
    if ([regex]::IsMatch($surface, $clearraExecutablePattern)) {
        Add-ArchitectureError "R host runtime contract forbids executable token 'clearra.exe'"
    }
}
}
