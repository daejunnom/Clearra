# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# U9 keeps desktop and web long-running work behind a common job/progress contract.

function Invoke-JobWorkerProgressContractValidation() {
foreach ($requiredFile in @(
            "crates/clearra-gui-host/src/job/common_job_model.rs",
            "crates/clearra-gui-host/src/job/gui_job_event.rs",
            "crates/clearra-gui-host/src/job/gui_job_progress.rs",
            "crates/clearra-gui-host/src/job/gui_job_runner.rs",
            "crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs",
            "crates/clearra-wasm/src/wasm_worker_job.rs",
            "packages/clearra-ui/src/lib/wasm/wasmCommandClient.ts",
            "packages/clearra-ui/src/lib/wasm/wasmWorkerStore.ts",
            "packages/clearra-ui/src/lib/wasm/WasmTerminalWorkerController.ts",
            "apps/clearra-web/src/workers/clearraWorker.ts",
            "apps/clearra-web/src/workers/WasmJobRunner.ts",
            "apps/clearra-web/src/workers/DistributedWasmJobRunner.ts",
            "apps/clearra-web/src/workers/ClearraProductJobRunner.ts",
            "scripts/job-worker-progress-check.ps1",
            "scripts/wasm-command-runtime-check.ps1"
        )) {
        if (-not (Test-Path -LiteralPath $requiredFile)) {
            Add-ArchitectureError "U9 required job/worker progress file missing: $requiredFile"
        }
    }
$desktopSurface = @(
        Read-Text "crates/clearra-gui-host/src/job/common_job_model.rs"
        Read-Text "crates/clearra-gui-host/src/job/gui_job_event.rs"
        Read-Text "crates/clearra-gui-host/src/job/gui_job_progress.rs"
        Read-Text "crates/clearra-gui-host/src/job/gui_job_runner.rs"
        Read-Text "crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs"
        Read-Text "apps/clearra-desktop/src-tauri/src/commands.rs"
        Read-Text "crates/clearra-gui-host/src/lib.rs"
    ) -join "`n"
$webSurface = @(
        Read-PhysicalText "crates/clearra-wasm/src/wasm_worker_job.rs"
        Read-PhysicalText "crates/clearra-wasm/src/lib.rs"
        Read-PhysicalText "packages/clearra-ui/src/lib/wasm/wasmCommandClient.ts"
        Read-PhysicalText "packages/clearra-ui/src/lib/wasm/wasmWorkerStore.ts"
        Read-PhysicalText "packages/clearra-ui/src/lib/wasm/WasmTerminalWorkerController.ts"
        Read-PhysicalText "apps/clearra-web/src/workers/clearraWorker.ts"
        Read-PhysicalText "apps/clearra-web/src/workers/WasmJobRunner.ts"
        Read-PhysicalText "apps/clearra-web/src/workers/DistributedWasmJobRunner.ts"
        Read-PhysicalText "apps/clearra-web/src/workers/ClearraProductJobRunner.ts"
        Read-PhysicalText "scripts/job-worker-progress-check.ps1"
        Read-PhysicalText "scripts/wasm-command-runtime-check.ps1"
    ) -join "`n"
foreach ($requiredMarker in @(
            "JobId",
            "JobStatus",
            "JobProgress",
            "JobDiagnosticEvent",
            "JobPartialResult",
            "JobFinalResponse",
            "CancelRequest",
            "BudgetStatus",
            "BackendStatus",
            "MemoryStatus",
            ".start_job(&request_json)",
            "GuiJobEvent::Completed",
            "response.to_host_response()",
            "WasmCancellationToken",
            "DiagnosticReport",
            "scope_released",
            "response: JobFinalResponse",
            "drain_job_events_json",
            "ExecutionControl",
            "SEARCH_WORK_BUDGET",
            'host_contract_tests=$hostContractMode',
            "wasm_exact_execution=launched",
            "raw_pointer_exposed",
            "active_jobs"
        )) {
        if (($desktopSurface + "`n" + $webSurface) -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U9 job/worker/progress contract must expose marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @(
            "std::process::Command",
            "clearra.exe",
            "run_with_args",
            "CliParser",
            "raw_pointer_exposed: true",
            '"raw_pointer_exposed": true',
            "partial_result_marked_partial: false",
            '"partial_result_marked_partial": false',
            "partial result final",
            "*mut clr_",
            "*mut Clr",
            "NonNull<Clr"
        )) {
        if ($desktopSurface -like "*$forbiddenMarker*" -or $webSurface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "U9 job/worker/progress contract forbids marker '$forbiddenMarker'"
        }
    }

    foreach ($removedSelfClaim in @(
            "web_worker_start_job_returns_job_id",
            "cancel_job_sets_cancelled_status",
            "progress_event_contains_budget_and_backend_status",
            "partial_result_marked_partial",
            "final_response_matches_app_response_contract",
            "compile-and-architecture-only",
            "test_executable_launched=false"
        )) {
        if ($webSurface -like "*$removedSelfClaim*") {
            Add-ArchitectureError "U9 web worker must not use self-declared contract marker '$removedSelfClaim'"
        }
    }
    if ($webSurface -like "*this.worker.terminate()*") {
        Add-ArchitectureError "U9 WASM cancellation must cooperatively release the active computation scope"
    }
}
