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
            "crates/clearra-web-command/src/web_command_parser.rs",
            "crates/clearra-webgpu/src/embedded_pattern_bitset_union.wgsl",
            "crates/clearra-webgpu/src/shader_contract.rs",
            "crates/clearra-webgpu/src/webgpu_backend.rs",
            "crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs",
            "apps/clearra-desktop/src-tauri/src/commands.rs",
            "apps/clearra-desktop/src-tauri/src/main.rs",
            "packages/clearra-ui/src/lib/host/clearraDesktopHost.ts",
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
        Read-Text "crates/clearra-web-command/src/web_command_parser.rs"
        Read-Text "packages/clearra-ui/src/lib/wasm/wasmCommandClient.ts"
        Read-Text "apps/clearra-web/src/workers/clearraWorker.ts"
    ) -join "`n"
foreach ($requiredMarker in @(
            "command text",
            "WebCommandParser",
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
$desktopSurface = @(
        Read-Text "crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs"
        Read-Text "crates/clearra-gui-host/tests/gui_host_contract.rs"
        Read-Text "apps/clearra-desktop/src-tauri/src/commands.rs"
        Read-Text "apps/clearra-desktop/src-tauri/src/main.rs"
        Read-Text "packages/clearra-ui/src/lib/host/clearraDesktopHost.ts"
    ) -join "`n"
foreach ($requiredMarker in @(
            "run_request",
            "validate_request",
            "start_job",
            "cancel_job",
            "get_job_events",
            "DesktopTauriCommandBridge",
            "desktop_tauri_command_calls_gui_host_only",
            "gui_does_not_spawn_clearra_exe",
            "clearra-app/AppResponse"
        )) {
        if ($desktopSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "R Desktop host contract must expose marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @(
            "std::process::Command",
            "clearra.exe",
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
        if ($hostContractSurface -like "*$forbiddenMarker*" -or
            $wasmSurface -like "*$forbiddenMarker*" -or
            $webGpuSurface -like "*$forbiddenMarker*" -or
            $desktopSurface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "R host runtime contract forbids marker '$forbiddenMarker'"
        }
    }
}
