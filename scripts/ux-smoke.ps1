param(
    [switch]$UseBuiltBinary,
    [string]$ExePath = "",
    [int]$OutputExcerptLines = 40,
    [string]$ExecutionSurface = "",
    [switch]$VerboseLog,
    [switch]$ShowCases
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Root = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
$UxGoldenRoot = Join-Path $Root "tests/golden/ux"
. (Join-Path $PSScriptRoot "lib/progress.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-path-helpers.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-execution-surface.ps1")
Assert-ClearraTrustedExecutionSurface $ExecutionSurface "UX smoke"
function Remove-StaleClearraCliBinary {
    $stalePaths = @(
        (Join-Path (Get-ClearraCargoTargetDir) "debug/clearra-cli.exe")
    )

    foreach ($stalePath in $stalePaths) {
        if (Test-Path -LiteralPath $stalePath) {
            Remove-Item -LiteralPath $stalePath -Force
        }
    }
}function Resolve-ClearraUxBinary {
    if (-not [string]::IsNullOrWhiteSpace($ExePath)) {
        return $ExePath
    }

    $candidates = @()
    if (-not [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
        $candidates += (Join-Path $env:CARGO_TARGET_DIR "debug/clearra.exe")
        $candidates += (Join-Path $env:CARGO_TARGET_DIR "debug/clearra")
    }
    $cacheTarget = Get-ClearraCargoTargetDir
    $candidates += (Join-Path $cacheTarget "debug/clearra.exe")
    $candidates += (Join-Path $cacheTarget "debug/clearra")

    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }
    }

    return (Join-Path (Get-ClearraCargoTargetDir) "debug/clearra.exe")
}function Get-UxExcerpt([string]$Text) {
    if ([string]::IsNullOrWhiteSpace($Text)) {
        return ""
    }
    return (($Text -split "`r?`n") | Select-Object -Last $OutputExcerptLines) -join "`n"
}function Invoke-ClearraUx {
    param(
        [Parameter(Mandatory)]
        [string[]]$CommandArgs
    )

    Push-Location $Root
    try {
        if ($UseBuiltBinary.IsPresent) {
            $resolvedExe = Resolve-ClearraUxBinary
            if ([System.IO.Path]::GetFileName($resolvedExe) -ieq "clearra-cli.exe") {
                throw "Refusing to launch stale binary '$resolvedExe'. Build or pass the release-facing clearra executable instead."
            }
            $nativeResult = Invoke-NativeWithProgress `
                -Scope $script:UxProgressScope `
                -Label $script:UxCurrentCaseName `
                -FileName $resolvedExe `
                -Arguments $CommandArgs
            $exitCode = $nativeResult.ExitCode
            $text = $nativeResult.Output
        } else {
            $cargoArgs = @(
                "run", "-q", "-p", "clearra-cli",
                "--features", "native-c-core,webgpu-search",
                "--bin", "clearra", "--"
            ) + $CommandArgs
            $previousCargoTargetDir = $env:CARGO_TARGET_DIR
            $setCargoTargetDir = $false
            if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
                $env:CARGO_TARGET_DIR = Get-ClearraCargoTargetDir
                $setCargoTargetDir = $true
            } else {
                Assert-ClearraCanonicalCargoTargetDir $previousCargoTargetDir | Out-Null
            }
            try {
                $nativeResult = Invoke-NativeWithProgress `
                    -Scope $script:UxProgressScope `
                    -Label $script:UxCurrentCaseName `
                    -FileName "cargo" `
                    -Arguments $cargoArgs
                $exitCode = $nativeResult.ExitCode
                $text = $nativeResult.Output
            } finally {
                if ($setCargoTargetDir) {
                    Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
                } else {
                    $env:CARGO_TARGET_DIR = $previousCargoTargetDir
                }
            }
        }

        return [pscustomobject]@{
            Args = $CommandArgs -join " "
            ExitCode = $exitCode
            Text = $text
        }
    } finally {
        Pop-Location
    }
}function Assert-ExitCode {
    param($Result, [int]$Expected)
    if ($Result.ExitCode -ne $Expected) {
        throw "UX command failed exit check: $($Result.Args). expected=$Expected actual=$($Result.ExitCode)`n$(Get-UxExcerpt $Result.Text)"
    }
}function Assert-Contains {
    param($Result, [string]$Needle)
    if ($Result.Text -notlike "*$Needle*") {
        throw "UX command output did not contain '$Needle': $($Result.Args)`n$(Get-UxExcerpt $Result.Text)"
    }
}function Assert-NotContains {
    param($Result, [string]$Needle)
    if ($Result.Text -like "*$Needle*") {
        throw "UX command output unexpectedly contained '$Needle': $($Result.Args)"
    }
}function Read-GoldenContains {
    param(
        [Parameter(Mandatory)]
        [string]$GoldenFile
    )

    $path = Join-Path $UxGoldenRoot $GoldenFile
    if (-not (Test-Path -LiteralPath $path)) {
        throw "UX golden contract file is missing: $path"
    }

    if ([System.IO.Path]::GetExtension($path) -ieq ".json") {
        $json = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
        if (-not ($json.PSObject.Properties.Name -contains "contains")) {
            throw "UX golden contract JSON must expose a contains array: $path"
        }
        $markers = @($json.contains | ForEach-Object { [string]$_ })
    } else {
        $markers = @(Get-Content -LiteralPath $path |
            ForEach-Object { $_.Trim() } |
            Where-Object { $_.Length -gt 0 -and -not $_.StartsWith("#") })
    }

    if ($markers.Count -eq 0) {
        throw "UX golden contract contains no markers: $path"
    }

    return @($markers)
}function Invoke-Case {
    param(
        [string]$Name,
        [string[]]$CommandArgs,
        [int]$ExitCode = 0,
        [string[]]$Contains = @(),
        [string[]]$NotContains = @(),
        [string]$GoldenFile = "",
        [switch]$PassThru
    )

    $resultHolder = [pscustomobject]@{ Value = $null }
    Invoke-ClearraProgressCase -Scope $script:UxProgressScope -Name $Name -Body {
        $script:UxCurrentCaseName = $Name
        if ($VerboseLog.IsPresent -or $ShowCases.IsPresent) {
            Write-ClearraProgressVerboseLine $script:UxProgressScope "[ux] running | $Name"
        }
        $resultHolder.Value = Invoke-ClearraUx -CommandArgs $CommandArgs
        Assert-ExitCode $resultHolder.Value $ExitCode

        foreach ($needle in $Contains) {
            Assert-Contains $resultHolder.Value $needle
        }

        if (-not [string]::IsNullOrWhiteSpace($GoldenFile)) {
            foreach ($needle in (Read-GoldenContains $GoldenFile)) {
                Assert-Contains $resultHolder.Value $needle
            }
        }

        foreach ($needle in $NotContains) {
            Assert-NotContains $resultHolder.Value $needle
        }

        if ($VerboseLog.IsPresent -or $ShowCases.IsPresent) {
            Write-ClearraProgressVerboseLine $script:UxProgressScope "[ux] passed  | $Name"
        }
    }
    if ($PassThru.IsPresent) {
        return $resultHolder.Value
    }
}
Remove-StaleClearraCliBinary

$fumenFixture = Join-Path $Root "tests/fixtures/fumens/clearra_pc_trace.fumen"
$uxCaseTotal = 23
if (Test-Path -LiteralPath $fumenFixture) {
    $uxCaseTotal += 1
}
$script:UxCurrentCaseName = ""
$script:UxProgressScope = New-ClearraProgressScope `
    -Name "ux" `
    -Total $uxCaseTotal `
    -Workers 1 `
    -VerboseLog:($VerboseLog.IsPresent -or $ShowCases.IsPresent)

Invoke-Case `
    -Name "top-level help" `
    -CommandArgs @("--help") `
    -Contains @("usage: clearra", "--format text|json|fumen-like", "global options may appear before or after the command") `
    -GoldenFile "help_en.txt"

Invoke-Case `
    -Name "korean help label" `
    -CommandArgs @("--lang", "ko", "--help") `
    -Contains @("Clearra", "--lang en|ko", "usage: clearra") `
    -GoldenFile "help_ko.txt"

Invoke-Case `
    -Name "pc text happy path" `
    -CommandArgs @("pc", "--lines", "2", "--queue", "IOTSZJL") `
    -Contains @("lines: 2", "queue_len: 7") `
    -GoldenFile "pc_text.txt" `
    -NotContains @(
        "executor_flow",
        "compact_problem_descriptor",
        "gpu_backend_scope",
        "hybrid_scheduler",
        "score_event_basis",
        "coverage_row_view",
        "backend_report",
        "raw_coverage_export_path"
    )

Invoke-Case `
    -Name "pc verbose text exposes executor flow" `
    -CommandArgs @("--verbose", "pc", "--lines", "2", "--queue", "IOTSZJL") `
    -Contains @("executor_flow", "route: search-problem-core-executor")

$jsonPc = Invoke-Case `
    -Name "pc json global format before command" `
    -CommandArgs @("--format", "json", "pc", "--lines", "2") `
    -Contains @('"schema_version":2', '"kind":"pc"', '"summary":', '"contract":') `
    -GoldenFile "pc_json.json" `
    -PassThru

try {
    $null = $jsonPc.Text | ConvertFrom-Json
} catch {
    throw "pc json output was not valid JSON: $($_.Exception.Message)"
}

Invoke-Case `
    -Name "pc json command-local format" `
    -CommandArgs @("pc", "--format", "json", "--lines", "2") `
    -Contains @('"schema_version":2', '"kind":"pc"')

Invoke-Case `
    -Name "pc fumen-like output" `
    -CommandArgs @("--format", "fumen-like", "pc", "--lines", "2") `
    -Contains @("v115@") `
    -GoldenFile "pc_fumen_like.txt"

Invoke-Case `
    -Name "scenario fixture" `
    -CommandArgs @("pc-scenario", "--fixture", "tests/fixtures/pc/example.json", "--verify-expected") `
    -Contains @("kind: pc-scenario")

Invoke-Case `
    -Name "path command" `
    -CommandArgs @("path", "--lines", "2", "--queue", "IIOOO", "--fixed", "--no-hold") `
    -Contains @("kind: path")

Invoke-Case `
    -Name "percent command" `
    -CommandArgs @("percent", "--queue", "IOTSZ", "--observed", "--max-patterns", "64") `
    -Contains @("kind: percent")

Invoke-Case `
    -Name "setup command" `
    -CommandArgs @("setup", "--queue", "IOTSZJL", "--fixed") `
    -Contains @("queue_len: 7")

Invoke-Case `
    -Name "cover command" `
    -CommandArgs @("cover", "--template", "basic") `
    -Contains @("basic")

Invoke-Case `
    -Name "rules list" `
    -CommandArgs @("rules", "list") `
    -Contains @("kind: rules")

Invoke-Case `
    -Name "scoring list" `
    -CommandArgs @("scoring", "list") `
    -Contains @("kind: scoring")

Invoke-Case `
    -Name "verify pc" `
    -CommandArgs @("verify", "pc") `
    -Contains @("kind: pc")

Invoke-Case `
    -Name "verify kicks" `
    -CommandArgs @("verify", "kicks") `
    -Contains @("kind: verify-kicks")

if (Test-Path -LiteralPath $fumenFixture) {
    $fumen = (Get-Content -LiteralPath $fumenFixture -Raw).Trim()
    Invoke-Case `
        -Name "convert fumen-like to json" `
        -CommandArgs @("convert", "--from", "fumen-like", "--to", "json", "--input", $fumen) `
        -Contains @('"kind":"convert"')
}

Invoke-Case `
    -Name "inspect reserved unsupported" `
    -CommandArgs @("inspect") `
    -ExitCode 3 `
    -Contains @("E_CLI_COMMAND_UNSUPPORTED", "inspect is reserved for a future inspection command")

Invoke-Case `
    -Name "unknown command error" `
    -CommandArgs @("wat") `
    -ExitCode 2 `
    -Contains @("E_CLI_COMMAND_UNKNOWN") `
    -GoldenFile "unknown_command.txt"

Invoke-Case `
    -Name "missing option value error" `
    -CommandArgs @("pc", "--lines") `
    -ExitCode 2 `
    -Contains @("E_CLI_MISSING_VALUE") `
    -GoldenFile "missing_value.txt"

Invoke-Case `
    -Name "unknown output format error" `
    -CommandArgs @("--format", "yaml", "pc", "--lines", "2") `
    -ExitCode 2 `
    -Contains @("E_CLI_OUTPUT_FORMAT_UNSUPPORTED") `
    -GoldenFile "unsupported_format.txt"

Invoke-Case `
    -Name "sensitive path guard" `
    -CommandArgs @("cover", "--template-file", "service-account.json") `
    -ExitCode 2 `
    -Contains @("sensitive-looking file path")

$temp = Join-Path ([System.IO.Path]::GetTempPath()) "clearra-ux-redacted.txt"

Invoke-Case `
    -Name "path redacted by default" `
    -CommandArgs @("cover", "--template-file", $temp) `
    -ExitCode 2 `
    -Contains @("clearra-ux-redacted") `
    -GoldenFile "redacted_path.txt" `
    -NotContains @([System.IO.Path]::GetTempPath().TrimEnd('\', '/'))

Invoke-Case `
    -Name "verbose paths show full path" `
    -CommandArgs @("--verbose-paths", "cover", "--template-file", $temp) `
    -ExitCode 2 `
    -Contains @($temp) `
    -GoldenFile "verbose_path.txt"

Remove-StaleClearraCliBinary

Complete-ClearraProgressLine $script:UxProgressScope
[Console]::Out.WriteLine("[ux] all smoke cases passed")
