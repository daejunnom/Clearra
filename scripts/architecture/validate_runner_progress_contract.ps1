function Invoke-RunnerProgressContractValidation() {
$testPolicy = Read-Text "docs/test-policy.md"
foreach ($requiredDocMarker in @(
        "progress scope",
        "[scope] done | running | pending | failed | workers",
        "Acceptance progress delegates to child scopes",
        "parent scopes only show their own worker count",
        "child scopes report their own worker counts",
        "VerboseLog restores command output",
        "ShowCases restores per-case output",
        "Native command heartbeat"
    )) {
        if (-not $testPolicy.Contains($requiredDocMarker)) {
            Add-ArchitectureError "docs/test-policy.md must document runner progress policy marker '$requiredDocMarker'"
        }
    }
$progressLoader = Read-Text "scripts/lib/progress.ps1"
$progressScope = Read-Text "scripts/lib/progress/progress_scope.ps1"
$progressRender = Read-Text "scripts/lib/progress/progress_render.ps1"
$progressCaseRunner = Read-Text "scripts/lib/progress/progress_case_runner.ps1"
$progressNativeRunner = Read-Text "scripts/lib/progress/native_progress_runner.ps1"
$progressSurface = @(
        $progressScope,
        $progressRender,
        $progressCaseRunner,
        $progressNativeRunner
    ) -join "`n"
foreach ($requiredLoaderMarker in @(
        "progress/progress_scope.ps1",
        "progress/progress_render.ps1",
        "progress/progress_case_runner.ps1",
        "progress/native_progress_runner.ps1"
    )) {
        if (-not $progressLoader.Contains($requiredLoaderMarker)) {
            Add-ArchitectureError "scripts/lib/progress.ps1 must dot-source progress helper marker '$requiredLoaderMarker'"
        }
    }
foreach ($requiredProgressMarker in @(
        "New-ClearraProgressScope",
        "Write-ClearraProgressLine",
        "Complete-ClearraProgressLine",
        "LastLineLength",
        "Format-ClearraProgressElapsed"
    )) {
        if (-not $progressSurface.Contains($requiredProgressMarker)) {
            Add-ArchitectureError "progress helper must contain marker '$requiredProgressMarker'"
        }
    }
foreach ($requiredNativeRunnerMarker in @(
        "ConvertTo-ClearraProcessArgument",
        "ConvertTo-ClearraProcessArgumentString",
        "Resolve-ClearraNativeFileName",
        "System.Diagnostics.ProcessStartInfo",
        "RedirectStandardOutput",
        "RedirectStandardError",
        "ReadToEndAsync",
        "Invoke-NativeWithProgress"
    )) {
        if (-not $progressNativeRunner.Contains($requiredNativeRunnerMarker)) {
            Add-ArchitectureError "scripts/lib/progress/native_progress_runner.ps1 must preserve direct native progress runner marker '$requiredNativeRunnerMarker'"
        }
    }
foreach ($requiredCaseRunnerMarker in @(
        '[$($Scope.Name)] failed | $Name',
        "PreserveOutput",
        "Complete-ClearraProgressLine",
        "throw"
    )) {
        if (-not $progressCaseRunner.Contains($requiredCaseRunnerMarker)) {
            Add-ArchitectureError "scripts/lib/progress/progress_case_runner.ps1 must preserve failure reporting marker '$requiredCaseRunnerMarker'"
        }
    }
foreach ($forbiddenNativeRunnerMarker in @(
        "cmd.exe",
        "ComSpec",
        "Start-Process",
        "New-ClearraProgressTempPath",
        "1>",
        "2>"
    )) {
        if ($progressNativeRunner.Contains($forbiddenNativeRunnerMarker)) {
            Add-ArchitectureError "scripts/lib/progress/native_progress_runner.ps1 must not reintroduce shell/redirection runner marker '$forbiddenNativeRunnerMarker'"
        }
    }
$clearraScript = Read-Text "scripts/clearra.ps1"
$startTests = Read-Text "scripts/start-tests.ps1"
$testPolicy = Read-Text "docs/test-policy.md"
$forbiddenDefaultProgressMarkers = @(
        ("inner" + "-workers"),
        ("task" + "-workers"),
        ("-Inner" + "Workers"),
        ("-Task" + "Workers"),
        ("Inner" + "Workers"),
        ("Task" + "Workers")
    )
$defaultProgressSurface = @(
        $clearraScript,
        $startTests,
        $testPolicy
    ) -join "`n"
foreach ($forbiddenMarker in $forbiddenDefaultProgressMarkers) {
        if ($defaultProgressSurface.Contains($forbiddenMarker)) {
            Add-ArchitectureError "default runner progress must not print or document obsolete worker marker '$forbiddenMarker'; child scopes report their own workers"
        }
    }
}
