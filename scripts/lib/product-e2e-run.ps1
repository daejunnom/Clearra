# Product E2E command execution and backend case stage.

function Invoke-ProductE2EClearra {
    param(
        [Parameter(Mandatory)]
        [string[]]$CommandArgs,
        [switch]$ReuseExactReadOnlyProbe
    )

    Push-Location $Root
    try {
        # ProductE2E owns one immutable executable for the lifetime of this script.
        # Reuse is opt-in so unrelated cases remain independent process evidence.
        $cacheKey = $null
        if ($ReuseExactReadOnlyProbe.IsPresent) {
            $cacheVariable = Get-Variable `
                -Name ProductE2EExactProbeCache `
                -Scope Script `
                -ErrorAction SilentlyContinue
            if ($null -eq $cacheVariable) {
                $script:ProductE2EExactProbeCache =
                    [System.Collections.Generic.Dictionary[string, object]]::new(
                        [System.StringComparer]::Ordinal
                    )
            }
            if ($null -eq (Get-Variable -Name ProductE2EExactProbeExecutions -Scope Script -ErrorAction SilentlyContinue)) {
                $script:ProductE2EExactProbeExecutions = 0
            }
            if ($null -eq (Get-Variable -Name ProductE2EExactProbeReuses -Scope Script -ErrorAction SilentlyContinue)) {
                $script:ProductE2EExactProbeReuses = 0
            }
            $cacheIdentity = [ordered]@{
                schema = 1
                execution = if ($UseBuiltBinary.IsPresent) { 'built-release-binary' } else { 'cargo-run' }
                arguments = @($CommandArgs)
            }
            $cacheKey = $cacheIdentity | ConvertTo-Json -Compress -Depth 3
            if ($script:ProductE2EExactProbeCache.ContainsKey($cacheKey)) {
                $cached = $script:ProductE2EExactProbeCache[$cacheKey]
                $script:ProductE2EExactProbeReuses += 1
                return [pscustomobject]@{
                    Command = $cached.Command
                    ExitCode = $cached.ExitCode
                    Output = $cached.Output
                    ProbeReused = $true
                }
            }
        }

        if ($UseBuiltBinary.IsPresent) {
            $resolvedExe = Resolve-ProductE2EBinary
            if ([string]::IsNullOrWhiteSpace($resolvedExe)) {
                throw "Product E2E executable path is empty."
            }
            if ([System.IO.Path]::GetFileName($resolvedExe) -in @("clearra-cli.exe", "clearra-cli")) {
                throw "Product E2E accepts only the release-facing clearra executable."
            }
            # Bound only the direct built product probe. The selected
            # commands run in-process, without external child solvers.
            $nativeResult = Invoke-NativeWithProgress `
                -Scope $script:ProductE2EProgressScope `
                -Label $script:ProductE2ECurrentCaseName `
                -FileName $resolvedExe `
                -Arguments $CommandArgs `
                -TimeoutMs 15000
            $exitCode = $nativeResult.ExitCode
            $text = $nativeResult.Output
        } else {
            $previousCargoTargetDir = $env:CARGO_TARGET_DIR
            $setCargoTargetDir = $false
            if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
                $env:CARGO_TARGET_DIR = Get-ClearraCargoTargetDir
                $setCargoTargetDir = $true
            } else {
                Assert-ClearraCanonicalCargoTargetDir $previousCargoTargetDir | Out-Null
            }
            $cargoArgs = @(
                "run", "--quiet", "--locked", "--release", "-p", "clearra-cli",
                "--features", "wasm-cpu-runtime", "--bin", "clearra", "--"
            ) + $CommandArgs
            try {
                $nativeResult = Invoke-NativeWithProgress `
                    -Scope $script:ProductE2EProgressScope `
                    -Label $script:ProductE2ECurrentCaseName `
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

        $result = [pscustomobject]@{
            Command = "clearra $($CommandArgs -join ' ')"
            ExitCode = $exitCode
            Output = $text
            ProbeReused = $false
        }
        if ($ReuseExactReadOnlyProbe.IsPresent) {
            $script:ProductE2EExactProbeExecutions += 1
            if ($result.ExitCode -eq 0) {
                $script:ProductE2EExactProbeCache[$cacheKey] = $result
            }
        }
        return $result
    } finally {
        Pop-Location
    }
}

function Invoke-ProductE2ECommandCase(
    [string]$Name,
    [string]$FixturePath,
    [string]$GoldenPath,
    [string[]]$CommandArgs,
    [int]$ExpectedExitCode = 0,
    [scriptblock]$CheckOutput,
    [switch]$ReuseExactReadOnlyProbe
) {
    Invoke-ClearraProgressCase -Scope $script:ProductE2EProgressScope -Name $Name -Body {
        $script:ProductE2ECurrentCaseName = $Name
        $started = Get-Date
        if ($VerboseLog.IsPresent) {
            Write-ClearraProgressVerboseLine $script:ProductE2EProgressScope "[product-e2e] running | $Name"
        }
        $status = "passed"
        $errorMessage = $null
        $markerCount = 0
        $commandResult = [pscustomobject]@{
            Command = "clearra $($CommandArgs -join ' ')"
            ExitCode = $null
            Output = ""
        }

        try {
            $commandResult = Invoke-ProductE2EClearra `
                -CommandArgs $CommandArgs `
                -ReuseExactReadOnlyProbe:$ReuseExactReadOnlyProbe.IsPresent
            if ($commandResult.ExitCode -ne $ExpectedExitCode) {
                throw "expected exit $ExpectedExitCode but got $($commandResult.ExitCode)`n$(Get-ProductE2EExcerpt $commandResult.Output)"
            }
            if ($null -ne $CheckOutput) {
                & $CheckOutput (ConvertFrom-ProductE2EJsonOutput $commandResult.Output)
            } else {
                $markerText = ConvertTo-ProductE2EMarkerText $commandResult.Output
                $requiredMarkers = Read-ProductE2ERequiredMarkers $GoldenPath
                $markerCount = $requiredMarkers.Count
                Assert-ProductE2EMarkers $Name $markerText $requiredMarkers
                Assert-ProductE2ETypedCommandAssertions $Name $commandResult.Output
            }
        } catch {
            $status = "failed"
            $errorMessage = New-ProductE2EFailureMessage `
                -CaseName $Name `
                -Reason $_.Exception.Message `
                -Command $commandResult.Command `
                -FixturePath $FixturePath `
                -GoldenPath $GoldenPath `
                -Output $commandResult.Output
            if ($VerboseLog.IsPresent) {
                Write-ClearraProgressVerboseLine $script:ProductE2EProgressScope "[product-e2e] failed  | $Name | $([math]::Round(((Get-Date) - $started).TotalSeconds, 2))s"
            }
            throw $errorMessage
        } finally {
            $duration = (Get-Date) - $started
            $ProductResults.Add([pscustomobject]@{
                name = $Name
                kind = "cli-product"
                status = $status
                command = $commandResult.Command
                exit_code = $commandResult.ExitCode
                fixture = $FixturePath
                golden = $GoldenPath
                marker_count = $markerCount
                duration_ms = [int][math]::Round($duration.TotalMilliseconds)
                error = $errorMessage
            })
        }

        if ($VerboseLog.IsPresent) {
            Write-ClearraProgressVerboseLine $script:ProductE2EProgressScope "[product-e2e] passed  | $Name | markers=$markerCount | $([math]::Round($duration.TotalSeconds, 2))s"
        }
    }
}

function Invoke-ProductE2EFixtureCase(
    [string]$Name,
    [string]$FixturePath,
    [string]$GoldenPath
) {
    Invoke-ClearraProgressCase -Scope $script:ProductE2EProgressScope -Name $Name -Body {
        $script:ProductE2ECurrentCaseName = $Name
        $started = Get-Date
        if ($VerboseLog.IsPresent) {
            Write-ClearraProgressVerboseLine $script:ProductE2EProgressScope "[product-e2e] running | $Name"
        }
        $status = "passed"
        $errorMessage = $null
        $markerCount = 0
        $material = ""

        try {
            $material = Get-ProductE2EFixtureMaterial $FixturePath
            $requiredMarkers = Read-ProductE2ERequiredMarkers $GoldenPath
            $markerCount = $requiredMarkers.Count
            Assert-ProductE2EMarkers $Name $material $requiredMarkers
            Assert-ProductE2ETypedFixtureAssertions $Name $Root $FixturePath
        } catch {
            $status = "failed"
            $errorMessage = New-ProductE2EFailureMessage `
                -CaseName $Name `
                -Reason $_.Exception.Message `
                -Command "" `
                -FixturePath $FixturePath `
                -GoldenPath $GoldenPath `
                -Output $material
            if ($VerboseLog.IsPresent) {
                Write-ClearraProgressVerboseLine $script:ProductE2EProgressScope "[product-e2e] failed  | $Name | $([math]::Round(((Get-Date) - $started).TotalSeconds, 2))s"
            }
            throw $errorMessage
        } finally {
            $ProductResults.Add([pscustomobject]@{
                name = $Name
                kind = "fixture-invariant"
                status = $status
                command = $null
                exit_code = $null
                fixture = $FixturePath
                golden = $GoldenPath
                marker_count = $markerCount
                duration_ms = [int][math]::Round(((Get-Date) - $started).TotalMilliseconds)
                error = $errorMessage
            })
        }

        if ($VerboseLog.IsPresent) {
            Write-ClearraProgressVerboseLine $script:ProductE2EProgressScope "[product-e2e] passed  | $Name | markers=$markerCount | $([math]::Round(((Get-Date) - $started).TotalSeconds, 2))s"
        }
    }
}

function Get-ProductE2EOpeningSearchArgs([string]$Backend) {
    $commandArgs = @(Get-FixtureCommandArgs "tests/fixtures/continuation/pc_then_next_pc_available.json") + @("--backend", $Backend)
    if ($Backend -ne "cpu") { $commandArgs += "--allow-backend-fallback" }
    return $commandArgs
}

function Get-ProductE2EScenario4LSearchArgs([string]$Backend) {
    $commandArgs = @("--format", "json", "pc-scenario", "--fixture", "tests/fixtures/pc/scenario_simple_4l.json", "--verify-expected", "--backend", $Backend)
    if ($Backend -ne "cpu") { $commandArgs += "--allow-backend-fallback" }
    return $commandArgs
}

function Invoke-ProductE2EBackendCapabilityReportCase {
    Invoke-ProductE2ECommandCase `
        -Name "backend_report_present_in_json" `
        -FixturePath "tests/fixtures/pc/opening_2l_empty.json" `
        -CommandArgs @(Get-ProductE2EOpeningSearchArgs "cpu") `
        -ReuseExactReadOnlyProbe `
        -CheckOutput { param($json) Assert-ProductE2ECpuSelectionReport $json }
}

function Invoke-ProductE2ERetiredBackendCase {
    Invoke-ClearraProgressCase -Scope $script:ProductE2EProgressScope -Name "product_gpu_and_hybrid_retired_without_fallback" -Body {
        $script:ProductE2ECurrentCaseName = "product_gpu_and_hybrid_retired_without_fallback"
        $started = Get-Date
        $status = "passed"
        $errorMessage = $null
        $lastOutput = ""
        try {
            foreach ($backend in @("gpu", "hybrid")) {
                foreach ($fallback in @("--allow-backend-fallback", "--no-backend-fallback")) {
                    foreach ($baseArgs in @(
                        ,@(Get-ProductE2EOpeningSearchArgs $backend)
                        ,@(Get-ProductE2EScenario4LSearchArgs $backend)
                    )) {
                        $commandArgs = @($baseArgs)
                        $commandArgs[-1] = $fallback
                        $result = Invoke-ProductE2EClearra -CommandArgs $commandArgs -ReuseExactReadOnlyProbe
                        $lastOutput = $result.Output
                        if ($result.ExitCode -ne 3 -or
                            $result.Output -notlike "*E_BACKEND_GPU_UNAVAILABLE*" -or
                            $result.Output -notlike "*webgpu_backend_retired*") {
                            throw "retired $backend request must return unsupported without CPU fallback: $lastOutput"
                        }
                    }
                }
            }
        } catch {
            $status = "failed"
            $errorMessage = $_.Exception.Message
            throw
        } finally {
            $ProductResults.Add([pscustomobject]@{
                name = "product_gpu_and_hybrid_retired_without_fallback"
                kind = "cli-product-error"
                status = $status
                command = "clearra pc/pc-scenario --backend gpu|hybrid"
                exit_code = $null
                fixture = $null
                golden = $null
                marker_count = 0
                duration_ms = [int][math]::Round(((Get-Date) - $started).TotalMilliseconds)
                error = $errorMessage
            })
        }
    }
}

function Get-FixtureCommandArgs([string]$FixturePath) {
    $fixture = Get-Content -LiteralPath (Join-Path $Root $FixturePath) -Raw | ConvertFrom-Json
    if (-not ($fixture.PSObject.Properties.Name -contains "command")) {
        throw "Fixture does not expose a command array: $FixturePath"
    }
    return @("--format", "json") + @($fixture.command | ForEach-Object { [string]$_ })
}
