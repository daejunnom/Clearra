# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# U7 keeps the browser runtime command-compatible without native path/process shortcuts.

function Invoke-WasmCommandRuntimeContractValidation() {
foreach ($requiredFile in @(
            "crates/clearra-web-command/Cargo.toml",
            "crates/clearra-web-command/src/lib.rs",
            "crates/clearra-web-command/src/web_command_parser.rs",
            "crates/clearra-web-command/src/web_command_request.rs",
            "crates/clearra-web-command/src/web_virtual_file.rs",
            "crates/clearra-wasm/Cargo.toml",
            "crates/clearra-wasm-abi/Cargo.toml",
            "crates/clearra-wasm-abi/src/lib.rs",
            "crates/clearra-wasm/src/lib.rs",
            "crates/clearra-app/src/app_response.rs",
            "crates/clearra-wasm/src/wasm_command_runtime.rs",
            "crates/clearra-wasm/src/wasm_worker_job.rs",
            "crates/clearra-core-executor/src/backend/wasm_cpu_search_backend.rs",
            "crates/clearra-wasm/tests/wasm_host_contract.rs",
            "scripts/wasm-command-runtime-check.ps1",
            "docs/app-boundary.md",
            "docs/test-policy.md",
            "packages/clearra-ui/src/lib/wasm/index.ts",
            "packages/clearra-ui/src/lib/wasm/wasmCommandClient.ts",
            "packages/clearra-ui/src/lib/wasm/wasmWorkerStore.ts",
            "packages/clearra-ui/src/lib/wasm/WasmTerminalWorkerController.ts",
            "packages/clearra-ui/src/lib/wasm/WasmTerminalShell.svelte",
            "apps/clearra-web/package.json",
            "apps/clearra-web/src/routes/+page.svelte",
            "apps/clearra-web/src/workers/clearraWorker.ts",
            "apps/clearra-web/src/workers/WasmJobRunner.ts",
            "apps/clearra-web/src/workers/DistributedWasmJobRunner.ts",
            "apps/clearra-web/src/workers/ClearraProductJobRunner.ts",
            "apps/clearra-web/src/workers/clearraWasmRuntime.ts"
        )) {
        if (-not (Test-Path -LiteralPath $requiredFile)) {
            Add-ArchitectureError "U7 required WASM command runtime file missing: $requiredFile"
        }
    }
$rustSurface = @(
        Read-PhysicalText "crates/clearra-web-command/src/lib.rs"
        Read-PhysicalText "crates/clearra-web-command/src/web_command_parser.rs"
        Read-PhysicalText "crates/clearra-web-command/src/web_command_request.rs"
        Read-PhysicalText "crates/clearra-web-command/src/web_virtual_file.rs"
        Read-PhysicalText "crates/clearra-wasm/src/lib.rs"
        Read-PhysicalText "crates/clearra-app/src/app_response.rs"
        Read-PhysicalText "crates/clearra-wasm/src/wasm_command_runtime.rs"
        Read-PhysicalText "crates/clearra-wasm/src/wasm_worker_job.rs"
        Read-PhysicalText "crates/clearra-wasm-abi/src/lib.rs"
        Read-PhysicalText "crates/clearra-wasm/tests/wasm_host_contract.rs"
        Read-PhysicalText "scripts/wasm-command-runtime-check.ps1"
    ) -join "`n"
foreach ($requiredMarker in @(
            "WebCommandParser",
            "WebCommandRequest",
            "WebVirtualFileHandle",
            "browser-file-input",
            "AppRequest",
            "AppResponse",
            "WasmExecutionResult",
            "to_host_response",
            "diagnostic_report",
            "WasmCancellationToken",
            "scope_released",
            "pub fn start_job",
            "pub fn advance_job",
            "pub fn cancel_job",
            "clearra_wasm_drain_job_events",
            "wasm_target=compiled",
            'host_contract_tests=$hostContractMode',
            "wasm_exact_execution=launched",
            "Node WASM cooperative cancellation E2E",
            "browser_bundle=built"
        )) {
        if ($rustSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U7 Rust WASM command runtime must expose marker '$requiredMarker'"
        }
    }
$webSurface = @(
        Read-PhysicalText "packages/clearra-ui/src/lib/wasm/index.ts"
        Read-PhysicalText "packages/clearra-ui/src/lib/wasm/wasmCommandClient.ts"
        Read-PhysicalText "packages/clearra-ui/src/lib/wasm/wasmWorkerStore.ts"
        Read-PhysicalText "packages/clearra-ui/src/lib/wasm/WasmTerminalWorkerController.ts"
        Read-PhysicalText "packages/clearra-ui/src/lib/wasm/WasmTerminalShell.svelte"
        Read-PhysicalText "apps/clearra-web/package.json"
        Read-PhysicalText "apps/clearra-web/src/routes/+page.svelte"
        Read-PhysicalText "apps/clearra-web/src/workers/clearraWorker.ts"
        Read-PhysicalText "apps/clearra-web/src/workers/WasmJobRunner.ts"
        Read-PhysicalText "apps/clearra-web/src/workers/DistributedWasmJobRunner.ts"
        Read-PhysicalText "apps/clearra-web/src/workers/ClearraProductJobRunner.ts"
        Read-PhysicalText "apps/clearra-web/src/workers/clearraWasmRuntime.ts"
    ) -join "`n"
foreach ($requiredMarker in @(
            "run_command_text",
            "cancel_job",
            "wasm.start_job(commandText)",
            "this.wasm.advance_job(this.jobId, SEARCH_WORK_BUDGET)",
            "this.wasm.cancel_job(this.jobId)",
            "this.wasm.drain_job_events_json(this.jobId)",
            "event.response",
            "event.diagnostics",
            "scope_released",
            "wasm:build",
            "browser-file-input",
            "ClearraHostAppResponse"
        )) {
        if ($webSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U7 web runtime surface must expose marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @(
            "std::process::Command",
            "clearra.exe",
            "CliParser",
            "run_with_args",
            "core-c",
            "clearra_packing_",
            "clr_buildup_",
            "clearra_board64_",
            "localized_output_keys",
            "E_WASM_RUNTIME_NOT_CONNECTED",
            "ClearraWasmRuntimeEnvelope",
            "defaultWebGpuBackendReport",
            "final_response_matches_app_response_contract",
            "wasm_command_compiles_to_app_request",
            "preview_json_builder",
            "downloadable_output",
            "wasm-pack build",
            "compile-and-architecture-only",
            "test_executable_launched=false"
        )) {
        if ($rustSurface -like "*$forbiddenMarker*" -or $webSurface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "U7 WASM command runtime forbids marker '$forbiddenMarker'"
        }
    }

    $productWorker = Read-PhysicalText "apps/clearra-web/src/workers/clearraWorker.ts"
    if ($productWorker -like "*event: 'final_response'*") {
        Add-ArchitectureError "U7 product worker must forward Rust final responses instead of constructing them"
    }

    $wasmCpuSurface = @(
        Read-PhysicalText "crates/clearra-core-executor/src/backend/wasm_cpu_search_backend.rs"
        Read-PhysicalText "crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs"
        Read-PhysicalText "crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs"
        Read-PhysicalText "crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs"
        Read-PhysicalText "crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage.rs"
    ) -join "`n"
    foreach ($requiredMarker in @(
            "WasmCpuSearchBackend",
            "WasmExactSearchSession",
            "GeometryCatalog",
            "GeometrySearch",
            "BuildOrder",
            "StandardBagCoverage"
        )) {
        if ($wasmCpuSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "WASM exact-cover runtime must expose '$requiredMarker'"
        }
    }
    foreach ($forbiddenMarker in @(
            "portable_reference_packing_fallback_allowed",
            "portable example witness",
            "fixture-specific fallback",
            "preview candidate",
            "wasm_geometry_exact_cover_backend_not_connected"
        )) {
        if ($wasmCpuSurface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "WASM exact CPU product backend forbids '$forbiddenMarker'"
        }
    }

    $wasmDocs = @(
        Read-PhysicalText "docs/app-boundary.md"
        Read-PhysicalText "docs/test-policy.md"
    ) -join "`n"
    foreach ($requiredMarker in @(
            "WASM Geometry Exact-Cover Scope",
            "canonical exact-cover",
            "WASM CPU executor"
        )) {
        if ($wasmDocs -notlike "*$requiredMarker*") {
            Add-ArchitectureError "WASM product documentation must pin exact runtime marker '$requiredMarker'"
        }
    }
    foreach ($removedLegacyPath in @(
            "crates/clearra-core-executor/src/backend/wasm_layered_packing.rs",
            "crates/clearra-core-executor/src/backend/wasm_buildup_search.rs",
            "crates/clearra-core-executor/src/backend/wasm_supply_language.rs"
        )) {
        if (Test-Path -LiteralPath $removedLegacyPath) {
            Add-ArchitectureError "removed WASM legacy algorithm resurfaced: $removedLegacyPath"
        }
    }
    if ($webSurface -like "*this.worker.terminate()*") {
        Add-ArchitectureError "U7 cooperative WASM cancellation must not terminate the worker"
    }
}
