[CmdletBinding()]
param(
    [ValidateSet('auto', 'all', 'windows', 'wsl', 'wasm')]
    [string]$Environment = 'auto',
    [ValidateSet('auto', 'cpu', 'gpu', 'hybrid')]
    [string]$Backend = 'auto',
    [string]$GpuDevice = 'auto',
    [ValidateSet('auto', 'always', 'never')]
    [string]$GpuInventoryMode = 'auto',
    [int]$Workers = [Math]::Max(1, [Environment]::ProcessorCount),
    [long]$WasmMaxNodes = 100000000,
    [string]$Distribution = 'Clearra-Build',
    [string]$OutputDirectory = '',
    [string]$WindowsBinaryPath = '',
    [string]$WslBinaryPath = '',
    [string]$WasmModuleDirectory = '',
    [ValidateSet('windows')]
    [string]$PrepareEnvironment = 'windows',
    [switch]$Prepare,
    [switch]$ProfileStages
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$workersExplicitlyRequested = $PSBoundParameters.ContainsKey('Workers')
if ($WasmMaxNodes -le 0) {
    throw 'WasmMaxNodes must be greater than zero.'
}
$Root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
. (Join-Path $PSScriptRoot 'lib/clearra-path-helpers.ps1')
. (Join-Path $PSScriptRoot 'lib/clearra-artifact-cache.ps1')
. (Join-Path $PSScriptRoot 'lib/clearra-runtime-environment.ps1')
. (Join-Path $PSScriptRoot 'lib/clearra-application-control.ps1')
. (Join-Path $PSScriptRoot 'lib/clearra-build-wsl-dispatch.ps1')
$Root = Resolve-ClearraBuildSourceRoot
Assert-ClearraBuildEnvironmentBeforeMutation $Root

$runId = [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss')
$usesDefaultOutput = [string]::IsNullOrWhiteSpace($OutputDirectory)
$outputRoot = if ($usesDefaultOutput) {
    Resolve-ClearraReportPath (Join-Path 'runtime-environments' 'latest') $Root
} else {
    Resolve-ClearraReportPath $OutputDirectory $Root
}
$selectedEnvironments = if ($Environment -eq 'all') {
    @('windows', 'wsl', 'wasm')
} elseif ($Environment -eq 'auto') {
    @(Resolve-ClearraRuntimeEnvironment 'auto')
} else {
    @($Environment)
}
$needsWindowsBuildOwner = $Prepare.IsPresent -and 'wasm' -in $selectedEnvironments
if ($needsWindowsBuildOwner) {
    Assert-ClearraRequestedBuildPath -Path $WasmModuleDirectory -RepositoryRoot $Root | Out-Null
}
$cases = @(
    [pscustomobject]@{ id = 'pco-6p'; expected = 63; max_patterns = 840; count = 'all' },
    [pscustomobject]@{ id = 'tsar-cannon'; expected = 42; max_patterns = 5040; count = 'all' }
)

function Invoke-CapturedCommand(
    [string]$FilePath,
    [string[]]$Arguments,
    [string]$WorkingDirectory = $Root,
    [switch]$AllowNonZeroExit
) {
    $started = [Diagnostics.Stopwatch]::StartNew()
    Push-Location $WorkingDirectory
    try {
        $previousPreference = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try {
            $lines = @(& $FilePath @Arguments 2>&1)
            $exitCode = $LASTEXITCODE
            $commandSucceeded = $?
        } finally {
            $ErrorActionPreference = $previousPreference
        }
    } finally {
        Pop-Location
        $started.Stop()
    }
    $text = ($lines | ForEach-Object { $_.ToString() }) -join "`n"
    if ($null -eq $exitCode -and -not $commandSucceeded) {
        $exitCode = if (Test-ClearraApplicationControlBlockOutput $lines) { 4551 } else { 1 }
    }
    if ($exitCode -ne 0 -and -not $AllowNonZeroExit.IsPresent) {
        throw "Command failed ($exitCode): $FilePath $($Arguments -join ' ')`n$text"
    }
    return [pscustomobject]@{
        output = $text
        elapsed_ms = $started.Elapsed.TotalMilliseconds
        exit_code = $exitCode
    }
}

function Convert-GpuInventoryResult([object]$Inventory) {
    if ($Inventory.exit_code -ne 0) {
        return [pscustomobject]@{
            query_status = 'unavailable'
            unavailable_reason = $Inventory.output.Trim()
            query_elapsed_ms = [double]$Inventory.elapsed_ms
            adapters = @()
        }
    }
    $parsed = $Inventory.output | ConvertFrom-Json
    $adapters = [System.Collections.Generic.List[object]]::new()
    foreach ($adapter in @($parsed)) {
        $adapters.Add($adapter)
    }
    return [pscustomobject]@{
        query_status = 'ok'
        unavailable_reason = $null
        query_elapsed_ms = [double]$Inventory.elapsed_ms
        adapters = $adapters.ToArray()
    }
}

function Get-GpuInventoryRequested {
    if ($GpuInventoryMode -eq 'always') { return $true }
    if ($GpuInventoryMode -eq 'never') { return $false }
    return $Backend -in @('gpu', 'hybrid')
}

function New-SkippedGpuInventory {
    return [pscustomobject]@{
        query_status = 'not-requested'
        unavailable_reason = $null
        query_elapsed_ms = 0.0
        adapters = @()
    }
}

function Write-EnvironmentResult([string]$Name, [object]$Value) {
    $path = Join-Path $outputRoot "$Name.json"
    [System.IO.File]::WriteAllText(
        $path,
        ($Value | ConvertTo-Json -Depth 16),
        [System.Text.UTF8Encoding]::new($false)
    )
    return $path
}

function NativeBatchArtifactArguments([string]$BatchOutput, [int]$WorkerCount) {
    $arguments = [System.Collections.Generic.List[string]]::new()
    $arguments.AddRange([string[]]@(
        '--scenario', 'pco-6p',
        '--scenario', 'tsar-cannon',
        '--rule', 'srs-plus',
        '--count', 'all',
        '--backend', $Backend,
        '--gpu-device', $GpuDevice,
        '--workers', $WorkerCount.ToString(),
        '--max-patterns', '5040',
        '--output-dir', $BatchOutput
    ))
    if ($Backend -in @('gpu', 'hybrid')) {
        $arguments.Add('--prewarm-gpu')
    }
    if ($ProfileStages.IsPresent) {
        $arguments.Add('--profile-stages')
    }
    return [string[]]$arguments.ToArray()
}

function Invoke-WindowsEnvironment {
    Assert-ClearraRuntimeEnvironmentAvailable 'windows' | Out-Null
    $workerCount = [Math]::Max(1, $Workers)
    if ($workerCount -gt [Math]::Max(1, [Environment]::ProcessorCount)) {
        throw 'Requested workers exceed the Windows logical processor count; no silent reduction is permitted.'
    }
    $prepareMs = 0.0
    if ($Prepare.IsPresent) {
        $prepareArguments = [System.Collections.Generic.List[string]]::new()
        $prepareArguments.AddRange([string[]]@(
            '-NoProfile', '-File', (Join-Path $Root 'scripts/export-pc-artifact.ps1'),
            '-ListGpuDevices', '-ExecutionSurface', 'Trusted', '-Workers', $workerCount.ToString()
        ))
        if ($ProfileStages.IsPresent) {
            $prepareArguments.Add('-ProfileStages')
        }
        $prepareResult = Invoke-CapturedCommand 'powershell.exe' $prepareArguments.ToArray()
        $prepareMs = $prepareResult.elapsed_ms
    }
    $binary = if ([string]::IsNullOrWhiteSpace($WindowsBinaryPath)) {
        Join-Path (Get-ClearraBuildTransactionRoot -RepositoryRoot $Root) 'cargo-target/release/clearra-pc-artifact.exe'
    } else {
        [System.IO.Path]::GetFullPath($WindowsBinaryPath)
    }
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
        throw "Prepared Windows artifact executable is missing: $binary"
    }
    Assert-ClearraWindowsRuntimeArtifactAllowed `
        $binary `
        'Windows PC runtime comparison' | Out-Null
    $inventory = if (Get-GpuInventoryRequested) {
        Convert-GpuInventoryResult (
            Invoke-CapturedCommand $binary @('--list-gpu-devices') -AllowNonZeroExit
        )
    } else {
        New-SkippedGpuInventory
    }
    $batchOutput = Join-Path $outputRoot 'windows-batch'
    New-Item -ItemType Directory -Force -Path $batchOutput | Out-Null
    $batch = Invoke-CapturedCommand $binary (NativeBatchArtifactArguments $batchOutput $workerCount)
    [System.IO.File]::WriteAllText(
        (Join-Path $batchOutput 'stdout.log'),
        $batch.output,
        [System.Text.UTF8Encoding]::new($false)
    )
    $caseTimes = @{}
    foreach ($line in @(Get-Content -LiteralPath (Join-Path $batchOutput 'batch-times.tsv'))) {
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        $parts = $line -split "`t"
        if ($parts.Count -ne 2) {
            throw "Invalid Windows runtime batch timing record: $line"
        }
        $caseTimes[$parts[0]] = [double]$parts[1] / 1000000.0
    }
    $results = foreach ($case in $cases) {
        $caseOutput = Join-Path $batchOutput $case.id
        $summary = Get-Content -LiteralPath (Join-Path $caseOutput 'summary.json') -Raw | ConvertFrom-Json
        [pscustomobject]@{
            scenario = $case.id
            expected_unique_solution_count = $case.expected
            actual_unique_solution_count = $summary.actual_unique_solution_count
            correctness_match = $summary.actual_unique_solution_count -eq $case.expected
            covered_pattern_count = $summary.probability.covered_pattern_count
            pattern_count = $summary.probability.pattern_count
            coverage_probability = $summary.probability.coverage_probability
            probability_complete = $summary.probability.probability_complete
            renormalized = $summary.probability.renormalized
            process_elapsed_ms = $caseTimes[$case.id]
            search_elapsed_ms = [double]$summary.timings.total_wall_ns / 1000000.0
            summary = $summary
        }
    }
    return [pscustomobject]@{
        environment = 'windows-native'
        runtime_root = $Root
        runtime_artifact = $binary
        prepared_this_run = $Prepare.IsPresent
        preparation_excluded_from_case_timings = $true
        preparation_elapsed_ms = $prepareMs
        host_batch_elapsed_ms = $batch.elapsed_ms
        logical_processors = [Environment]::ProcessorCount
        workers_used_limit = $workerCount
        gpu_inventory = $inventory
        results = @($results)
    }
}

function Invoke-WslEnvironment {
    if (-not $Prepare.IsPresent) {
        throw 'The managed WSL comparison requires -Prepare; cached or caller-supplied WSL binaries are not accepted.'
    }
    if ($Distribution -cne 'Clearra-Build') {
        throw 'Only the managed Clearra-Build distribution is available to Clearra.'
    }
    if (-not [string]::IsNullOrWhiteSpace($WslBinaryPath)) {
        throw 'A caller-supplied WSL binary cannot bypass the managed build-and-batch entrypoint.'
    }
    $workerSelection = if ($workersExplicitlyRequested) {
        ([Math]::Max(1, $Workers)).ToString()
    } else {
        'auto'
    }
    $linuxReportRoot = '/home/clearra/.local/state/Clearra/reports/runtime-environments/latest'
    $inventoryMode = if (Get-GpuInventoryRequested) { 'query' } else { 'skip' }
    $profileMode = if ($ProfileStages.IsPresent) { 'profile' } else { 'no-profile' }
    $cargoFeatures = if ($ProfileStages.IsPresent) {
        'gpu-backend,stage-profiling'
    } else {
        'gpu-backend'
    }
    $sessionOutput = Join-Path $outputRoot '_wsl-managed-session'
    New-Item -ItemType Directory -Force -Path $sessionOutput | Out-Null
    $python = (Get-Command 'python' -ErrorAction Stop).Source
    $arguments = New-ClearraManagedWslEntryArguments `
        -RepositoryRoot $Root `
        -Entry 'pc-runtime-build-batch' `
        -CommandArguments @(
        '--host-output', $sessionOutput,
        $cargoFeatures, $linuxReportRoot, $Backend, $GpuDevice,
        $workerSelection, $inventoryMode, $profileMode
    )
    $batch = Invoke-CapturedCommand $python $arguments
    $prepareTime = [regex]::Match($batch.output, '(?m)^wsl_preparation_elapsed_ns=(\d+)\s*$')
    $batchTime = [regex]::Match($batch.output, '(?m)^wsl_host_batch_elapsed_ns=(\d+)\s*$')
    if (-not $prepareTime.Success -or -not $batchTime.Success) {
        throw 'Managed WSL build/runtime timing records are missing.'
    }
    $hostInfo = Get-Content -LiteralPath (Join-Path $sessionOutput 'clearra-runtime-host.json') -Raw | ConvertFrom-Json
    $workerCount = [int]$hostInfo.workers
    if ($workerCount -gt [int]$hostInfo.logical_processors) {
        throw 'The managed WSL session accepted an invalid worker count.'
    }
    $inventoryStatus = (Get-Content -LiteralPath (Join-Path $sessionOutput 'gpu-inventory.status') -Raw).Trim()
    $inventoryElapsedMs = [double]((Get-Content -LiteralPath (Join-Path $sessionOutput 'gpu-inventory-time-ns') -Raw).Trim()) / 1000000.0
    if ($inventoryStatus -eq 'not-requested') {
        $inventory = New-SkippedGpuInventory
    } elseif ([int]$inventoryStatus -eq 0) {
        $inventory = Convert-GpuInventoryResult ([pscustomobject]@{
            exit_code = 0
            output = Get-Content -LiteralPath (Join-Path $sessionOutput 'gpu-inventory.json') -Raw
            elapsed_ms = $inventoryElapsedMs
        })
    } else {
        $inventory = Convert-GpuInventoryResult ([pscustomobject]@{
            exit_code = [int]$inventoryStatus
            output = (Get-Content -LiteralPath (Join-Path $sessionOutput 'gpu-inventory.error') -Raw).Trim()
            elapsed_ms = $inventoryElapsedMs
        })
    }
    $caseTimes = @{}
    foreach ($line in @(Get-Content -LiteralPath (Join-Path $sessionOutput 'case-times.tsv'))) {
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        $parts = $line -split "`t"
        if ($parts.Count -ne 3 -or [int]$parts[2] -ne 0) {
            throw "Invalid managed WSL runtime batch timing record: $line"
        }
        $caseTimes[$parts[0]] = [double]$parts[1] / 1000000.0
    }
    $results = foreach ($case in $cases) {
        $sessionCaseOutput = Join-Path $sessionOutput $case.id
        $summaryJson = Get-Content -LiteralPath (Join-Path $sessionCaseOutput 'summary.json') -Raw
        $summary = $summaryJson | ConvertFrom-Json
        $hostCaseOutput = Join-Path $outputRoot "wsl-$($case.id)"
        New-Item -ItemType Directory -Force -Path $hostCaseOutput | Out-Null
        foreach ($artifactName in @(
            'summary.json', 'solutions.fumen', 'solution-probabilities.jsonl', 'timings.json'
        )) {
            Copy-Item -LiteralPath (Join-Path $sessionCaseOutput $artifactName) `
                -Destination (Join-Path $hostCaseOutput $artifactName) -Force
        }
        foreach ($artifactName in @('stdout.log', 'stderr.log')) {
            Copy-Item -LiteralPath (Join-Path $sessionOutput $artifactName) `
                -Destination (Join-Path $hostCaseOutput $artifactName) -Force
        }
        [pscustomobject]@{
            scenario = $case.id
            expected_unique_solution_count = $case.expected
            actual_unique_solution_count = $summary.actual_unique_solution_count
            correctness_match = $summary.actual_unique_solution_count -eq $case.expected
            covered_pattern_count = $summary.probability.covered_pattern_count
            pattern_count = $summary.probability.pattern_count
            coverage_probability = $summary.probability.coverage_probability
            probability_complete = $summary.probability.probability_complete
            renormalized = $summary.probability.renormalized
            process_elapsed_ms = $caseTimes[$case.id]
            search_elapsed_ms = [double]$summary.timings.total_wall_ns / 1000000.0
            summary = $summary
        }
    }
    return [pscustomobject]@{
        environment = 'wsl-native'
        distribution = 'Clearra-Build'
        runtime_root = $hostInfo.runtime_root
        runtime_artifact = 'managed:clearra-pc-artifact'
        prepared_this_run = $true
        runtime_filesystem = $hostInfo.runtime_filesystem
        windows_mount_used_by_runtime = $false
        windows_path_entries_used_by_runtime = $false
        source_sync_performed = $true
        preparation_excluded_from_case_timings = $true
        preparation_elapsed_ms = [double]$prepareTime.Groups[1].Value / 1000000.0
        host_batch_elapsed_ms = [double]$batchTime.Groups[1].Value / 1000000.0
        logical_processors = [int]$hostInfo.logical_processors
        workers_used_limit = $workerCount
        gpu_inventory = $inventory
        results = @($results)
    }
}

function Invoke-WasmEnvironment {
    Assert-ClearraRuntimeEnvironmentAvailable 'wasm' | Out-Null
    if ($Prepare.IsPresent) { Ensure-ClearraBuildArtifactCache -RepositoryRoot $Root }
    $wasmArtifactRoot = if ([string]::IsNullOrWhiteSpace($WasmModuleDirectory)) {
        Resolve-ClearraArtifactPath 'wasm-runtime-environment' $Root
    } elseif ($Prepare.IsPresent) {
        Resolve-ClearraArtifactPath $WasmModuleDirectory $Root
    } else {
        Assert-ClearraPathOutsideRepository `
            ([System.IO.Path]::GetFullPath($WasmModuleDirectory)) `
            $Root
    }
    $prepareMs = 0.0
    $prepareEnvironmentUsed = $null
    if ($Prepare.IsPresent) {
        $cargoTarget = Get-ClearraCargoTargetDir
        $prepareEnvironmentUsed = 'windows'
        if (Test-Path -LiteralPath $wasmArtifactRoot) {
            Assert-ClearraBuildTreeNoReparse $wasmArtifactRoot
            Remove-Item -LiteralPath $wasmArtifactRoot -Recurse -Force
        }
        New-Item -ItemType Directory -Force -Path $wasmArtifactRoot | Out-Null
        if ($null -eq (Get-Command 'cargo.exe' -ErrorAction SilentlyContinue)) {
            throw "Windows WASM preparation requires 'cargo.exe'."
        }
        $previousCargoTarget = $env:CARGO_TARGET_DIR
        try {
            $env:CARGO_TARGET_DIR = $cargoTarget
            $prepareResult = Invoke-CapturedCommand 'cargo.exe' @(
                'build', '--target', 'wasm32-unknown-unknown', '--release',
                '-p', 'clearra-wasm-abi'
            )
            $prepareMs += $prepareResult.elapsed_ms
            $stageResult = Invoke-CapturedCommand 'node.exe' @(
                (Join-Path $Root 'scripts/tools/stage-clearra-wasm.mjs'),
                $wasmArtifactRoot
            )
            $prepareMs += $stageResult.elapsed_ms
        } finally {
            if ([string]::IsNullOrWhiteSpace($previousCargoTarget)) {
                Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
            } else {
                $env:CARGO_TARGET_DIR = $previousCargoTarget
            }
        }
    }
    $bindings = Join-Path $wasmArtifactRoot 'clearra_wasm.js'
    $binary = Join-Path $wasmArtifactRoot 'clearra_wasm_bg.wasm'
    foreach ($artifact in @($bindings, $binary)) {
        if (-not (Test-Path $artifact -PathType Leaf) -or (Get-Item $artifact).Length -le 0) {
            throw "Prepared wasm-bindgen runtime is missing under $wasmArtifactRoot"
        }
    }
    $commands = @{
        'pco-6p' = "clearra pc --board-mask 0x000000e0f87e3f87 --height 4 --pieces 4 --hold I --count all --max-patterns 840 --max-candidates $WasmMaxNodes --backend $Backend"
        'tsar-cannon' = "clearra pc --board-mask 0x000300c0399e3fdf --height 5 --pieces 6 --hold empty --count all --max-patterns 5040 --max-candidates $WasmMaxNodes --backend $Backend"
    }
    $results = foreach ($case in $cases) {
        $run = Invoke-CapturedCommand 'node.exe' @(
            (Join-Path $Root 'scripts/tools/wasm-pc-environment-probe.mjs'),
            $bindings, $commands[$case.id], '8192', 'summary'
        )
        $probe = $run.output | ConvertFrom-Json
        [pscustomobject]@{
            scenario = $case.id
            expected_unique_solution_count = $case.expected
            actual_unique_solution_count = $probe.final.unique_solution_count
            correctness_match = $probe.final.unique_solution_count -eq $case.expected
            covered_pattern_count = $probe.final.covered_pattern_count
            pattern_count = $probe.final.materialized_pattern_count
            coverage_probability = $probe.final.coverage_probability
            probability_complete = $probe.final.probability_complete
            process_elapsed_ms = $run.elapsed_ms
            search_elapsed_ms = $probe.search_elapsed_ms
            summary = $probe
        }
    }
    return [pscustomobject]@{
        environment = 'wasm-bindgen-web-host'
        runtime_artifact = $binary
        prepared_this_run = $Prepare.IsPresent
        preparation_excluded_from_case_timings = $true
        preparation_elapsed_ms = $prepareMs
        preparation_environment = $prepareEnvironmentUsed
        logical_processors = 1
        workers_used_limit = 1
        gpu_inventory = New-SkippedGpuInventory
        results = @($results)
    }
}

try {
# Prebuilt Windows/native consumers do not initialize or replace build output.
# Only Windows WASM preparation owns this PowerShell transaction; WSL owns its
# independent ext4-source transaction through the dispatch helper.
Assert-ClearraNoReparseBuildPath $outputRoot | Out-Null
if ($usesDefaultOutput -and (Test-Path -LiteralPath $outputRoot)) {
    Assert-ClearraBuildTreeNoReparse $outputRoot
    Remove-Item -LiteralPath $outputRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
$environmentReports = [System.Collections.Generic.List[object]]::new()
$environmentFailures = [System.Collections.Generic.List[object]]::new()
foreach ($selected in $selectedEnvironments) {
    Write-Output "[runtime-environment] start $selected"
    try {
        $report = switch ($selected) {
            'windows' { Invoke-WindowsEnvironment }
            'wsl' { Invoke-WslEnvironment }
            'wasm' { Invoke-WasmEnvironment }
        }
    } catch {
        if (@($selectedEnvironments).Count -eq 1) {
            throw
        }
        $message = $_.Exception.Message
        $errorCode = if ($message -match '^([A-Z0-9_]+):') {
            $Matches[1]
        } else {
            'E_RUNTIME_ENVIRONMENT_FAILED'
        }
        $report = [pscustomobject]@{
            environment = $selected
            execution_status = 'unavailable'
            error_code = $errorCode
            unavailable_reason = $message
            results = @()
        }
        $environmentFailures.Add($report)
    }
    $environmentReports.Add($report)
    $path = Write-EnvironmentResult $selected $report
    Write-Output "[runtime-environment] complete $selected report=$path"
}

$comparison = [pscustomobject]@{
    schema_version = 1
    run_id = $runId
    requested_backend = $Backend
    gpu_device = $GpuDevice
    build_and_deployment_excluded_from_runtime_timings = $true
    preparation_requested = $Prepare.IsPresent
    preparation_environment_requested = $PrepareEnvironment
    wasm_max_nodes = $WasmMaxNodes
    stage_profiling_requested = $ProfileStages.IsPresent
    comparison_complete = $environmentFailures.Count -eq 0
    failed_environment_count = $environmentFailures.Count
    environments = @($environmentReports)
}
$comparisonPath = Write-EnvironmentResult 'comparison' $comparison
Write-Output "runtime environment comparison complete: $comparisonPath"
if ($environmentFailures.Count -gt 0) {
    $failedNames = @($environmentFailures | ForEach-Object { $_.environment }) -join ', '
    throw "Runtime environment comparison is incomplete for: $failedNames. Report: $comparisonPath"
}
if ($needsWindowsBuildOwner -and (Test-ClearraBuildTransactionOwner)) { Complete-ClearraBuildTransaction }
} finally {
    if ($needsWindowsBuildOwner) { Exit-ClearraBuildArtifactCacheUsage }
}
