param(
    [ValidateSet('pco-6p', 'tsar-cannon', 'empty-4l')]
    [string]$Scenario = 'pco-6p',

    [ValidateSet('srs-plus', 'srs')]
    [string]$Rule = 'srs-plus',

    [ValidateSet('auto', 'cpu', 'gpu', 'hybrid')]
    [string]$Backend = 'auto',

    [string]$GpuDevice = 'auto',

    [ValidateSet('default', 'all', 'unique')]
    [string]$Count = 'default',

    [int]$Workers = [Math]::Max(1, [Environment]::ProcessorCount - 1),
    [int]$MaxPatterns = 0,
    [string]$OutputDirectory = '',
    [string]$SfinderOutputPath = '',
    [string]$ExecutablePath = '',
    [string]$ExecutionSurface = '',
    [long]$MaxCandidates = 0,
    [long]$MaxFrontierStates = 0,
    [switch]$NoFallback,
    [switch]$UseAllCpuThreads,
    [switch]$CpuWarmup,
    [switch]$ListGpuDevices,
    [switch]$PrewarmGpu,
    [switch]$SkipFumen,
    [switch]$ProfileStages,
    [switch]$VerboseLog,
    [ValidateSet('NativeCReference', 'RustFamily')]
    [string]$Engine = 'NativeCReference',
    [ValidateRange(0, 7200)]
    [int]$RustTimeoutSeconds = 0
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Root = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')
. (Join-Path $PSScriptRoot 'lib/clearra-path-helpers.ps1')
. (Join-Path $PSScriptRoot 'lib/clearra-execution-surface.ps1')
. (Join-Path $PSScriptRoot 'lib/clearra-application-control.ps1')
$Root = Resolve-ClearraBuildSourceRoot

Assert-ClearraBuildEnvironmentBeforeMutation $Root
Assert-ClearraTrustedExecutionSurface $ExecutionSurface 'PC artifact export'
if ($Workers -lt 1) {
    throw '-Workers must be at least 1.'
}
$logicalProcessors = [Math]::Max(1, [Environment]::ProcessorCount)
$defaultWorkerLimit = [Math]::Max(1, $logicalProcessors - 1)
if ($Workers -gt $logicalProcessors) {
    throw "-Workers $Workers exceeds the hard limit of $logicalProcessors logical processors."
}
if ($Workers -gt $defaultWorkerLimit -and -not $UseAllCpuThreads.IsPresent) {
    throw "-Workers $Workers uses the reserved logical processor; pass -UseAllCpuThreads explicitly."
}
if ($MaxPatterns -lt 0) {
    throw '-MaxPatterns must be zero or greater.'
}
if ($MaxCandidates -lt 0 -or $MaxCandidates -gt 4294967294) {
    throw '-MaxCandidates must be 0 (memory-bounded auto) or between 1 and 4294967294.'
}
if ($MaxFrontierStates -lt 0 -or $MaxFrontierStates -gt 4294967294) {
    throw '-MaxFrontierStates must be 0 (memory-bounded auto) or between 1 and 4294967294.'
}
if ($GpuDevice -ne 'auto' -and $GpuDevice -notmatch '^\d+$') {
    throw '-GpuDevice must be auto or a non-negative adapter index.'
}
if ($Engine -eq 'RustFamily' -and
    ($Backend -in @('gpu', 'hybrid') -or $GpuDevice -ne 'auto' -or
     $ListGpuDevices.IsPresent -or $PrewarmGpu.IsPresent -or $ProfileStages.IsPresent -or
     -not [string]::IsNullOrWhiteSpace($SfinderOutputPath))) {
    throw 'RustFamily export is CPU-only; use NativeCReference for GPU, stage profiling, or the legacy Sfinder comparison.'
}

$applicationControl = Get-ClearraApplicationControlStatus

$artifactNamespace = if ($Engine -eq 'RustFamily') { 'pc-family-artifacts' } else { 'pc-artifacts' }
$resolvedOutput = if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    Resolve-ClearraReportPath (Join-Path $artifactNamespace $Scenario) $Root
} else {
    Resolve-ClearraReportPath $OutputDirectory $Root
}
if ($Engine -eq 'RustFamily') {
    . (Join-Path $PSScriptRoot 'lib/clearra-manage-command.ps1')
    $manager = Get-ClearraManageExecutable -RepositoryRoot $Root
    & $manager --root $Root storage verify --path $resolvedOutput
    if ($LASTEXITCODE -ne 0) {
        throw "Clearra manager refused the family output path: $resolvedOutput"
    }
}
New-Item -ItemType Directory -Force -Path $resolvedOutput | Out-Null
if ($Engine -eq 'RustFamily') {
    foreach ($name in @('family-summary.json', 'family-solutions.jsonl', 'family-solutions.fumen')) {
        if (Test-Path -LiteralPath (Join-Path $resolvedOutput $name)) {
            throw 'RustFamily does not replace previous output. Select a new -OutputDirectory.'
        }
    }
}

$sfinderHtml = $null
if (-not [string]::IsNullOrWhiteSpace($SfinderOutputPath)) {
    $candidate = [System.IO.Path]::GetFullPath($SfinderOutputPath)
    if (Test-Path -LiteralPath $candidate -PathType Container) {
        $candidate = Join-Path $candidate 'path_unique.html'
    }
    if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
        throw "Sfinder reference HTML does not exist: $candidate"
    }
    $sfinderHtml = $candidate
}

$binaryName = if ($Engine -eq 'RustFamily') { 'clearra-pc-family-artifact' } else { 'clearra-pc-artifact' }
$cargoFeature = if ($Engine -eq 'RustFamily') { 'rust-family-export' } else { 'native-c-reference' }
$resolvedExecutable = if ([string]::IsNullOrWhiteSpace($ExecutablePath)) {
    # Export is a read-only artifact consumer. Selecting its default must not
    # create an owner that replaces the very experiment being consumed.
    Join-Path (Get-ClearraBuildTransactionRoot -RepositoryRoot $Root) "cargo-target/release/$binaryName.exe"
} else {
    [System.IO.Path]::GetFullPath($ExecutablePath)
}
if (-not (Test-Path -LiteralPath $resolvedExecutable -PathType Leaf)) {
    throw (
        "E_PC_ARTIFACT_BINARY_UNAVAILABLE: $Engine export requires a prebuilt " +
        "$binaryName binary (Cargo feature $cargoFeature) and never compiles " +
        "source at request time. Missing artifact: $resolvedExecutable"
    )
}
$runtimeAuthorization = Assert-ClearraWindowsRuntimeArtifactAllowed `
    $resolvedExecutable `
    'PC artifact export'
Write-Output ((
        '[pc-artifact] native-windows | umci={0} | build_attempted=false | ' +
        'runtime_artifact={1} | signature_status={2} | wsl_fallback=false | ' +
        'policy_or_signing_mutation=false'
    ) -f `
        $applicationControl.user_mode_code_integrity_policy,
        $resolvedExecutable,
        $runtimeAuthorization.signature_status)

$arguments = [System.Collections.Generic.List[string]]::new()
$arguments.AddRange([string[]]@(
    '--scenario', $Scenario,
    '--rule', $Rule,
    '--backend', $Backend,
    '--gpu-device', $GpuDevice,
    '--workers', $Workers.ToString(),
    '--output-dir', $resolvedOutput
))
if ($ProfileStages.IsPresent) {
    $arguments.Add('--profile-stages')
}
if ($MaxCandidates -gt 0) {
    $arguments.Add('--max-candidates')
    $arguments.Add(
        $MaxCandidates.ToString([Globalization.CultureInfo]::InvariantCulture)
    )
}
if ($MaxFrontierStates -gt 0) {
    $arguments.Add('--max-frontier-states')
    $arguments.Add(
        $MaxFrontierStates.ToString([Globalization.CultureInfo]::InvariantCulture)
    )
}
if ($Count -ne 'default') {
    $arguments.Add('--count')
    $arguments.Add($Count)
}
if ($MaxPatterns -gt 0) {
    $arguments.Add('--max-patterns')
    $arguments.Add($MaxPatterns.ToString())
}
if ($NoFallback.IsPresent) {
    $arguments.Add('--no-fallback')
}
if ($UseAllCpuThreads.IsPresent) {
    $arguments.Add('--use-all-cpu-threads')
}
if ($CpuWarmup.IsPresent) {
    $arguments.Add('--cpu-warmup')
}
if ($ListGpuDevices.IsPresent) {
    $arguments.Add('--list-gpu-devices')
}
if ($PrewarmGpu.IsPresent) {
    $arguments.Add('--prewarm-gpu')
}
if ($SkipFumen.IsPresent) {
    $arguments.Add('--skip-fumen')
}
if ($null -ne $sfinderHtml) {
    $arguments.Add('--sfinder-html')
    $arguments.Add($sfinderHtml)
}

Push-Location $Root
try {
    $program = $resolvedExecutable
    $programArgs = $arguments.ToArray()
    if ($Engine -eq 'RustFamily') {
        $program = $manager
        $programArgs = @(
            '--root', $Root, 'runtime', 'run',
            '--producer', 'benchmark', '--profile', 'benchmark-search',
            '--timeout', [string]$RustTimeoutSeconds, '--', $resolvedExecutable
        ) + $programArgs
    }
    if ($VerboseLog.IsPresent) {
        Write-Output "==> $program $($programArgs -join ' ')"
    }
    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $runtimeOutput = [System.Collections.Generic.List[string]]::new()
    try {
        & $program @programArgs 2>&1 | ForEach-Object {
            $line = $_.ToString()
            $runtimeOutput.Add($line)
            Write-Output $line
        }
        $runtimeExitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }
    if ($runtimeExitCode -ne 0) {
        if (Test-ClearraApplicationControlBlockOutput $runtimeOutput) {
            throw (New-ClearraRuntimeArtifactBlockedMessage `
                    'PC artifact export' `
                    $resolvedExecutable `
                    $applicationControl)
        }
        throw "Native Windows PC artifact export failed with exit code $runtimeExitCode"
    }
}
finally {
    Pop-Location
}

if ($Engine -eq 'RustFamily') {
    $receiptPath = Join-Path $resolvedOutput 'family-summary.json'
    $receipt = Get-Content -LiteralPath $receiptPath -Raw | ConvertFrom-Json
    if ($receipt.schema -cne 'clearra.rust-pc-family-artifact.v1' -or
        $receipt.execution_engine -cne 'rust-cpu' -or $receipt.scenario -cne $Scenario) {
        throw 'Rust family export did not produce a matching Rust CPU family receipt.'
    }
}
Write-Output "[pc-artifact] complete | engine=$Engine | scenario=$Scenario | output=$resolvedOutput | runner_surface=windows-native | wsl_used=false"
