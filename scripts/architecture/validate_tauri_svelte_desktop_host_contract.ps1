# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# U6 keeps one desktop product: Tauri -> clearra-gui-host -> clearra-app.

function Invoke-TauriSvelteDesktopHostContractValidation() {
    $requiredFiles = @(
        "apps/clearra-desktop/package.json",
        "apps/clearra-desktop/svelte.config.js",
        "apps/clearra-desktop/src/routes/+page.svelte",
        "apps/clearra-desktop/src-tauri/Cargo.toml",
        "apps/clearra-desktop/src-tauri/build.rs",
        "apps/clearra-desktop/src-tauri/tauri.conf.json",
        "apps/clearra-desktop/src-tauri/src/main.rs",
        "apps/clearra-desktop/src-tauri/src/commands.rs",
        "packages/clearra-ui/src/lib/components/DesktopHostShell.svelte",
        "packages/clearra-ui/src/lib/host/clearraDesktopHost.ts",
        "packages/clearra-ui/src/lib/stores/desktopJobStore.ts",
        "crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs",
        "crates/clearra-gui-host/src/job/gui_job_runner.rs",
        "scripts/desktop-host-check.ps1",
        "scripts/desktop-ui-compile-check.mjs",
        "scripts/lib/clearra-application-control.ps1"
    )
    foreach ($relativePath in $requiredFiles) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $relativePath))) {
            Add-ArchitectureError "U6 required Tauri desktop file is missing: $relativePath"
        }
    }

    foreach ($removedSurface in @(
        "gui/clearra-gui",
        "scripts/gui-smoke.ps1",
        "scripts/gui-host-smoke.ps1",
        "scripts/gui-host-e2e.ps1",
        "crates/clearra-gui-host/src/main.rs"
    )) {
        if (Test-Path -LiteralPath (Join-Path $Root $removedSurface)) {
            Add-ArchitectureError "U6 duplicate desktop product surface still exists: $removedSurface"
        }
    }

    $rootCargo = Read-Text "Cargo.toml"
    $rootCmake = Read-Text "CMakeLists.txt"
    $tauriCargo = Read-Text "apps/clearra-desktop/src-tauri/Cargo.toml"
    $tauriMain = Read-Text "apps/clearra-desktop/src-tauri/src/main.rs"
    $tauriCommands = Read-Text "apps/clearra-desktop/src-tauri/src/commands.rs"
    $desktopBridge = @(
        Read-Text "crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs"
        Read-Text "crates/clearra-gui-host/src/job/gui_job_runner.rs"
        Read-Text "crates/clearra-gui-host/src/request/pc_request_builder.rs"
    ) -join "`n"
    $desktopClient = Read-Text "packages/clearra-ui/src/lib/host/clearraDesktopHost.ts"
    $desktopStore = Read-Text "packages/clearra-ui/src/lib/stores/desktopJobStore.ts"
    $desktopShell = Read-Text "packages/clearra-ui/src/lib/components/DesktopHostShell.svelte"
    $desktopGate = Read-Text "scripts/desktop-host-check.ps1"
    $desktopUiCompile = Read-Text "scripts/desktop-ui-compile-check.mjs"
    $applicationControl = Read-Text "scripts/lib/clearra-application-control.ps1"

    if (-not $rootCargo.Contains('exclude = ["apps/clearra-desktop/src-tauri"]')) {
        Add-ArchitectureError "U6 Tauri crate must stay outside the standard workspace execution surface"
    }
    if ($tauriCargo -notmatch '(?m)^clearra-gui-host\s*=') {
        Add-ArchitectureError "U6 Tauri crate must depend on clearra-gui-host"
    }
    $guiHostDependency = [regex]::Match(
        $tauriCargo,
        '(?m)^clearra-gui-host\s*=\s*\{[^\r\n]*$'
    ).Value
    if ($guiHostDependency -notmatch '"wasm-cpu-runtime"') {
        Add-ArchitectureError "U6 desktop product must enable the exact WASM CPU execution backend"
    }
    if ($guiHostDependency -match '"native-c-core"') {
        Add-ArchitectureError "U6 desktop product must not restore the retired Windows native C execution backend"
    }
    if ($guiHostDependency -notmatch '"webgpu-search"') {
        Add-ArchitectureError "U6 desktop product must enable the connected WebGPU search backend"
    }
    $tauriBuild = Read-Text "apps/clearra-desktop/src-tauri/build.rs"
    if ($tauriBuild -notlike '*tauri_build::build()*') {
        Add-ArchitectureError "U6 desktop build script must only generate the Tauri application context"
    }
    foreach ($forbiddenBuildMarker in @('build_native_core', 'cmake::Config', 'clearra_core')) {
        if ($tauriBuild -like "*$forbiddenBuildMarker*" -or $tauriCargo -match '(?m)^cmake\s*=') {
            Add-ArchitectureError "U6 desktop must not duplicate the runner-owned C build via '$forbiddenBuildMarker'"
        }
    }
    $unexpectedClearraDependency = [regex]::Match(
        $tauriCargo,
        '(?m)^(clearra-(?!gui-host)[A-Za-z0-9_-]+)\s*='
    )
    if ($unexpectedClearraDependency.Success) {
        Add-ArchitectureError "U6 Tauri crate bypasses clearra-gui-host via $($unexpectedClearraDependency.Groups[1].Value)"
    }

    foreach ($requiredMarker in @(
        "DesktopBridgeState::default()",
        "tauri::generate_handler!",
        "run_request",
        "validate_request",
        "start_job",
        "cancel_job",
        "get_job_events",
        "prewarm_search_backend"
    )) {
        if (($tauriMain + "`n" + $tauriCommands) -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U6 Tauri command surface is missing '$requiredMarker'"
        }
    }
    foreach ($requiredMarker in @(
        "DesktopTauriCommandBridge",
        ".run_request(&request_json)",
        ".validate_request(&request_json)",
        ".start_job(&request_json)",
        ".cancel_job(job_id)",
        ".get_job_events(job_id)",
        "clearra_gui_host::prewarm_search_backend"
    )) {
        if ($tauriCommands -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U6 Tauri command does not forward through clearra-gui-host marker '$requiredMarker'"
        }
    }
    foreach ($requiredMarker in @(
        "GuiToAppRequest::build",
        "self.app_context.run(request)",
        "response.to_host_response()",
        "serde_json::to_string",
        "PcQueueInput::fixed_sequence",
        "PcHoldPolicy::Disabled"
    )) {
        if ($desktopBridge -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U6 desktop bridge is missing real AppRequest/AppResponse marker '$requiredMarker'"
        }
    }
    foreach ($requiredMarker in @(
        "invoke<string>('run_request'",
        "JSON.parse(response)",
        "invoke<number>('start_job'",
        "invoke<void>('cancel_job'",
        "invoke<string>('get_job_events'",
        "invoke<string>('prewarm_search_backend'"
    )) {
        if ($desktopClient -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U6 desktop UI is missing Tauri invocation '$requiredMarker'"
        }
    }
    foreach ($requiredMarker in @(
        "getJobEvents",
        "events.reduce",
        "isTerminalEvent",
        "startDesktopJob",
        "cancelDesktopJob",
        "prewarmDesktopSearchBackend",
        "stopDesktopJobPolling"
    )) {
        if ($desktopStore -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U6 desktop async job store is missing '$requiredMarker'"
        }
    }
    foreach ($requiredMarker in @(
        'on:click={startDesktopJob}',
        'on:click={cancelDesktopJob}',
        '<progress',
        'state.diagnostics',
        'state.backendStatus',
        'state.memoryStatus',
        'state.resourceStatus'
    )) {
        if ($desktopShell -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U6 desktop async UI is missing '$requiredMarker'"
        }
    }
    foreach ($requiredMarker in @(
        "drain_job_events",
        "join_with_events",
        "finish(GuiJobId::new(job_id))",
        "self.active_job_id = None",
        "run_with_execution_control"
    )) {
        if ($desktopBridge -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U6 desktop lifecycle bridge is missing '$requiredMarker'"
        }
    }
    foreach ($requiredMarker in @(
        '"--features", "wasm-cpu-runtime,webgpu-search"',
        '"--lib"',
        'wasm_cpu_app_request=executed',
        'async_job_e2e=executed',
        'Get-ClearraCargoTargetDir',
        'Get-ClearraApplicationControlStatus',
        'Get-ClearraRecentGeneratedExecutableBlockEvidence',
        'Test-ClearraApplicationControlBlockOutput',
        'New-ClearraLocalSourceBuildBlockedMessage',
        'scripts/desktop-ui-compile-check.mjs',
        '"--manifest-path", "apps/clearra-desktop/src-tauri/Cargo.toml"',
        'wsl_used=false',
        "U6 Tauri Svelte Desktop Host"
    )) {
        if ($desktopGate -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U6 desktop release gate is missing '$requiredMarker'"
        }
    }
    foreach ($requiredMarker in @(
        "compile(processed.code",
        "transform(source",
        "artifactWrite = false",
        "desktop_ui_in_memory_compile=passed"
    )) {
        if ($desktopUiCompile -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U6 desktop dynamic UI compile gate is missing '$requiredMarker'"
        }
    }
    foreach ($requiredMarker in @(
        "Win32_DeviceGuard",
        "UsermodeCodeIntegrityPolicyEnforcementStatus",
        "Microsoft-Windows-CodeIntegrity/Operational",
        "E_WINDOWS_LOCAL_SOURCE_BUILD_BLOCKED",
        "local_source_build_policy",
        "policy_evidence_only"
    )) {
        if ($applicationControl -notlike "*$requiredMarker*") {
            Add-ArchitectureError "U6 application-control preflight is missing '$requiredMarker'"
        }
    }
    foreach ($forbiddenSkipMarker in @(
        'generatedExecutableAvailable',
        'tauri_compile_attempted=false',
        'Assert-ClearraTauriBuildExecutionAvailable'
    )) {
        if ($desktopGate -like "*$forbiddenSkipMarker*") {
            Add-ArchitectureError "U6 must execute the requested command instead of statically skipping via '$forbiddenSkipMarker'"
        }
    }
    foreach ($forbiddenTargetMarker in @(
        'Get-DesktopCargoTargetDir',
        'Get-ClearraReleaseCargoTargetDir',
        'tauri-target',
        'cargo-target-native',
        'gpu-worker-acceptance'
    )) {
        if ($desktopGate -like "*$forbiddenTargetMarker*") {
            Add-ArchitectureError "U6 desktop gate must not create task-specific Cargo target '$forbiddenTargetMarker'"
        }
    }

    $productSurface = $rootCmake + "`n" + $tauriCargo + "`n" + $tauriCommands + "`n" + $desktopBridge + "`n" + $desktopClient + "`n" + $desktopStore + "`n" + $desktopShell
    foreach ($forbiddenMarker in @(
        "CLEARRA_BUILD_GUI",
        "Clearra GUI shell scaffold",
        "std::process::Command",
        "clearra.exe",
        "CliParser",
        "run_with_args",
        "clearra_packing_",
        "clr_buildup_",
        "final_response_matches_app_response_contract",
        "tauri_command_calls_clearra_gui_host_only: true",
        "desktop_form_builds_app_request: true"
    )) {
        if ($productSurface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "U6 desktop product contains forbidden marker '$forbiddenMarker'"
        }
    }
    if ($productSurface -match "(?m)\bget_job_event\b" -or
        $productSurface -match "(?m)\bgetJobEvent\b" -or
        $productSurface -like "*runDesktopRequest*") {
        Add-ArchitectureError "U6 desktop product must use batched async job events, not the singular or synchronous UI route"
    }
}
