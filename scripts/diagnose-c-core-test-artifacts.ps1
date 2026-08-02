param(
    [string]$BuildDir,
    [string]$Configuration = "Debug",
    [string]$ReportPath
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Root = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
. (Join-Path $PSScriptRoot "lib/clearra-path-helpers.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-application-control.ps1")
$ResolvedBuildDir = if ([string]::IsNullOrWhiteSpace($BuildDir)) {
    Resolve-ClearraArtifactPath "core-c-test-cache" $Root
} else {
    Resolve-ClearraArtifactPath $BuildDir $Root
}

$ResolvedReportPath = Resolve-ClearraReportPath $ReportPath $Root
function Invoke-CTestJsonDiscovery([string]$BuildDirectory, [string]$BuildConfiguration) {
    $output = @(ctest --test-dir $BuildDirectory --build-config $BuildConfiguration --show-only=json-v1 2>&1)
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "ctest JSON discovery failed with exit code $exitCode`n$($output -join "`n")"
    }
    return ($output -join "`n") | ConvertFrom-Json
}function Invoke-CTestVerboseDiscovery([string]$BuildDirectory, [string]$BuildConfiguration) {
    $output = @(ctest --test-dir $BuildDirectory --build-config $BuildConfiguration -N -V 2>&1)
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "ctest verbose discovery failed with exit code $exitCode`n$($output -join "`n")"
    }
    return $output
}function Resolve-TestCommandPath([string]$CommandValue, [string]$BuildDirectory) {
    if ([string]::IsNullOrWhiteSpace($CommandValue)) {
        return $null
    }
    $candidate = $CommandValue.Trim('"')
    if ([System.IO.Path]::IsPathRooted($candidate)) {
        return $candidate
    }
    $fromBuild = Join-Path $BuildDirectory $candidate
    if (Test-Path -LiteralPath $fromBuild) {
        return (Get-Item -LiteralPath $fromBuild).FullName
    }
    return $candidate
}function Get-CTestCommandPathsFromJson([object]$CTestJson, [string]$BuildDirectory) {
    $paths = New-Object System.Collections.Generic.List[string]
    if ($null -eq $CTestJson.tests) {
        return $paths
    }

    foreach ($test in @($CTestJson.tests)) {
        $command = $null
        if ($null -ne $test.command) {
            $commandValues = @($test.command)
            if ($commandValues.Count -gt 0) {
                $command = [string]$commandValues[0]
            }
        } elseif ($null -ne $test.properties) {
            foreach ($property in @($test.properties)) {
                if ($property.name -eq "COMMAND") {
                    $commandValues = @($property.value)
                    if ($commandValues.Count -gt 0) {
                        $command = [string]$commandValues[0]
                    }
                }
            }
        }

        $resolved = Resolve-TestCommandPath $command $BuildDirectory
        if (-not [string]::IsNullOrWhiteSpace($resolved)) {
            $paths.Add($resolved)
        }
    }
    return $paths
}function Get-CTestCommandPathsFromVerboseOutput([string[]]$Output, [string]$BuildDirectory) {
    $paths = New-Object System.Collections.Generic.List[string]
    foreach ($line in $Output) {
        $command = $null
        if ($line -match '^\s*Test command:\s*(.+)$') {
            $command = $Matches[1].Trim()
        } elseif ($line -match '^\s*Command:\s*(.+)$') {
            $command = $Matches[1].Trim()
        }
        if ([string]::IsNullOrWhiteSpace($command)) {
            continue
        }
        $firstToken = if ($command.StartsWith('"')) {
            ([regex]::Match($command, '^"([^"]+)"')).Groups[1].Value
        } else {
            ($command -split '\s+')[0]
        }
        $resolved = Resolve-TestCommandPath $firstToken $BuildDirectory
        if (-not [string]::IsNullOrWhiteSpace($resolved)) {
            $paths.Add($resolved)
        }
    }
    return $paths
}function Get-UniqueExistingPaths([object[]]$Paths) {
    $set = New-Object System.Collections.Generic.HashSet[string]
    $result = New-Object System.Collections.Generic.List[string]
    foreach ($pathValue in $Paths) {
        if ([string]::IsNullOrWhiteSpace($pathValue)) {
            continue
        }
        $fullPath = if (Test-Path -LiteralPath $pathValue) {
            (Get-Item -LiteralPath $pathValue).FullName
        } else {
            $pathValue
        }
        if ($set.Add($fullPath)) {
            $result.Add($fullPath)
        }
    }
    return $result
}
if ($null -eq (Get-Command ctest -ErrorAction SilentlyContinue)) {
    throw "ctest was not found. Install CMake/CTest or add it to PATH."
}

if (-not (Test-Path -LiteralPath $ResolvedBuildDir)) {
    throw "C core build directory does not exist: $ResolvedBuildDir"
}

if (-not (Test-Path -LiteralPath (Join-Path $ResolvedBuildDir "CTestTestfile.cmake"))) {
    throw "CTest registry is missing in C core build directory: $ResolvedBuildDir"
}

$discoveryMode = "json-v1"
$commandPaths = $null
try {
    $ctestJson = Invoke-CTestJsonDiscovery $ResolvedBuildDir $Configuration
    $commandPaths = Get-CTestCommandPathsFromJson $ctestJson $ResolvedBuildDir
} catch {
    $discoveryMode = "verbose"
    $verboseOutput = Invoke-CTestVerboseDiscovery $ResolvedBuildDir $Configuration
    $commandPaths = Get-CTestCommandPathsFromVerboseOutput $verboseOutput $ResolvedBuildDir
}

$artifactPaths = @(Get-UniqueExistingPaths $commandPaths)
if ($artifactPaths.Count -eq 0) {
    throw "CTest discovery found zero executable command paths in $ResolvedBuildDir"
}

$applicationControl = Get-ClearraApplicationControlStatus
$runtimeTrustReports = @($artifactPaths | ForEach-Object {
        Get-ClearraWindowsRuntimeArtifactTrustReport $_ 'core-c CTest artifact diagnosis'
    })

$diagnosticTempDir = New-TransientBuildDir 'clearra-c-core-artifacts'
$diagnosticTempPath = Join-Path $diagnosticTempDir 'report.json'
try {
    & (Join-Path $PSScriptRoot "diagnose-windows-block.ps1") -Path $artifactPaths -ReportPath $diagnosticTempPath | Out-Null
    $artifactDiagnostics = Get-Content -LiteralPath $diagnosticTempPath -Raw | ConvertFrom-Json
} finally {
    Remove-TransientBuildDir $diagnosticTempDir
}

$report = [ordered]@{
    status = "ok"
    build_dir = $ResolvedBuildDir
    configuration = $Configuration
    discovery_mode = $discoveryMode
    artifact_count = $artifactPaths.Count
    artifacts = @($artifactDiagnostics)
    application_control = $applicationControl
    runtime_trust = $runtimeTrustReports
}

$json = $report | ConvertTo-Json -Depth 10

if (-not [string]::IsNullOrWhiteSpace($ResolvedReportPath)) {
    $reportDirectory = Split-Path -Parent $ResolvedReportPath
    if (-not [string]::IsNullOrWhiteSpace($reportDirectory) -and
        -not (Test-Path -LiteralPath $reportDirectory)) {
        New-Item -ItemType Directory -Path $reportDirectory | Out-Null
    }

    $json | Set-Content -LiteralPath $ResolvedReportPath -Encoding UTF8
}

$json
