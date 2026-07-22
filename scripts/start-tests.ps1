param(
    [ValidateSet("Quick", "All", "COnly", "COnlySplit", "COnlyAsan", "COnlyUbsan", "UXSmoke", "DesktopHost", "WorkerE2E", "WorkerE2EStress", "WorkerAcceptance", "WorkerRelease", "ProductE2E", "ProductE2EBuilt", "Acceptance", "ReleaseAcceptance", "NoProductDebt", "AdversarialCorrectness", "CSanitizer", "RustExactTests", "WasmBuildTest", "RenderGolden", "GpuWorkerAcceptance", "GpuWorkerNative", "GpuWorkerRelease", "Mvp2Acceptance", "Mvp3Acceptance", "Validate", "Local", "Strict", "Security", "SecurityFull", "NativeLocal", "DiagnoseCArtifacts", "Events")]
    [string]$Mode = "Local",

    [int]$Workers = [Math]::Min([Environment]::ProcessorCount, 6),

    [switch]$VerboseLog,
    [switch]$ShowWarnings,
    [switch]$Json,
    [int]$OutputExcerptLines = 60,
    [int]$WarningDetailLimit = 5,

    [switch]$KeepBuildCache,
    [string]$CoreCBuildDir,
    [string]$CMakeBuildType,

    [string]$ReportDir,
    [ValidateSet("ManagedLocal", "Trusted")]
    [string]$ExecutionSurface = "ManagedLocal",
    [ValidateSet("auto", "windows", "wsl", "wasm")]
    [string]$RuntimeEnvironment = "auto",
    [string]$WslDistribution = "Ubuntu",

    [string]$CargoPath = "cargo",
    [string]$PowerShellPath = "powershell"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$arguments = @{
    Task = $Mode
    Workers = $Workers
    OutputExcerptLines = $OutputExcerptLines
    WarningDetailLimit = $WarningDetailLimit
    CargoPath = $CargoPath
    PowerShellPath = $PowerShellPath
    ExecutionSurface = $ExecutionSurface
    RuntimeEnvironment = $RuntimeEnvironment
    WslDistribution = $WslDistribution
}

if ($VerboseLog.IsPresent) {
    $arguments["VerboseLog"] = $true
}
if ($ShowWarnings.IsPresent) {
    $arguments["ShowWarnings"] = $true
}
if ($Json.IsPresent) {
    $arguments["Json"] = $true
}
if ($KeepBuildCache.IsPresent) {
    $arguments["KeepBuildCache"] = $true
}
if (-not [string]::IsNullOrWhiteSpace($CoreCBuildDir)) {
    $arguments["CoreCBuildDir"] = $CoreCBuildDir
}
if (-not [string]::IsNullOrWhiteSpace($CMakeBuildType)) {
    $arguments["CMakeBuildType"] = $CMakeBuildType
}
if (-not [string]::IsNullOrWhiteSpace($ReportDir)) {
    $arguments["ReportDir"] = $ReportDir
}
& (Join-Path $PSScriptRoot "clearra.ps1") @arguments
if ($?) {
    exit 0
}
exit 1
