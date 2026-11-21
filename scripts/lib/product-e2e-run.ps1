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
                "--features", "wasm-cpu-runtime,webgpu-search", "--bin", "clearra", "--"
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

function Get-ProductE2EMissingAdapterArgs([switch]$DenyFallback) {
    # The maximum representable device index is intentionally absent on the
    # product runners. Supplying it tests adapter selection, not compile flags.
    $commandArgs = @(Get-ProductE2EOpeningSearchArgs "gpu")
    $commandArgs[-1] = if ($DenyFallback.IsPresent) { "--no-backend-fallback" } else { "--allow-backend-fallback" }
    return $commandArgs + @("--gpu-device", "255")
}

function Invoke-ProductE2EBackendCapabilityReportCase {
    Invoke-ClearraProgressCase -Scope $script:ProductE2EProgressScope -Name "backend_report_present_in_json" -Body {
        $script:ProductE2ECurrentCaseName = "backend_report_present_in_json"
        $started = Get-Date
        $status = "passed"
        $errorMessage = $null
        $commands = @(
            [pscustomobject]@{
                Label = "cpu"
                Args = @(Get-ProductE2EOpeningSearchArgs "cpu")
            },
            [pscustomobject]@{
                Label = "gpu"
                Args = @(Get-ProductE2EOpeningSearchArgs "gpu")
            },
            [pscustomobject]@{
                Label = "hybrid"
                Args = @(Get-ProductE2EOpeningSearchArgs "hybrid")
            }
        )
        $json = @{}
        $lastOutput = ""

        try {
            foreach ($command in $commands) {
                $result = Invoke-ProductE2EClearra `
                    -CommandArgs $command.Args `
                    -ReuseExactReadOnlyProbe
                $lastOutput = $result.Output
                if ($result.ExitCode -ne 0) {
                    throw "$($command.Label) backend command failed with exit $($result.ExitCode)"
                }
                $json[$command.Label] = ConvertFrom-ProductE2EJsonOutput $result.Output
            }

            Assert-ProductE2EU0BackendCapabilityReport $json["cpu"] $json["gpu"] $json["hybrid"]
        } catch {
            $status = "failed"
            $errorMessage = New-ProductE2EFailureMessage `
                -CaseName "backend_report_present_in_json" `
                -Reason $_.Exception.Message `
                -Command "clearra pc --backend cpu|gpu|hybrid" `
                -FixturePath "tests/fixtures/pc/opening_2l_empty.json" `
                -GoldenPath "" `
                -Output $lastOutput
            throw $errorMessage
        } finally {
            $ProductResults.Add([pscustomobject]@{
                name = "backend_report_present_in_json"
                kind = "cli-product"
                status = $status
                command = "clearra pc --backend cpu|gpu|hybrid"
                exit_code = $null
                fixture = "tests/fixtures/pc/opening_2l_empty.json"
                golden = $null
                marker_count = 0
                duration_ms = [int][math]::Round(((Get-Date) - $started).TotalMilliseconds)
                error = $errorMessage
            })
        }
    }
}

function Invoke-ProductE2EBackendEquivalenceCase(
    [string]$Name,
    [string[]]$CpuArgs,
    [string[]]$GpuArgs,
    [string[]]$HybridArgs,
    [string]$FixturePath,
    [string[]]$ExtraEquivalenceFields,
    [switch]$ReuseExactReadOnlyProbes
) {
    Invoke-ClearraProgressCase -Scope $script:ProductE2EProgressScope -Name $Name -Body {
        $script:ProductE2ECurrentCaseName = $Name
        $started = Get-Date
        $status = "passed"
        $errorMessage = $null
        $commands = @(
            [pscustomobject]@{
                Label = "cpu"
                Args = $CpuArgs
            },
            [pscustomobject]@{
                Label = "gpu"
                Args = $GpuArgs
            },
            [pscustomobject]@{
                Label = "hybrid"
                Args = $HybridArgs
            }
        )
        $json = @{}
        $lastOutput = ""

        try {
            foreach ($command in $commands) {
                $result = Invoke-ProductE2EClearra `
                    -CommandArgs $command.Args `
                    -ReuseExactReadOnlyProbe:$ReuseExactReadOnlyProbes.IsPresent
                $lastOutput = $result.Output
                if ($result.ExitCode -ne 0) {
                    throw "$($command.Label) backend command failed with exit $($result.ExitCode)"
                }
                $json[$command.Label] = ConvertFrom-ProductE2EJsonOutput $result.Output
            }

            $exactFields = @(
                "count_complete", "probability_complete", "resource_truncated",
                "coverage_probability", "covered_pattern_count",
                "normalized_unique_solution_count", "normalized_solution_set_hash"
            ) + $ExtraEquivalenceFields
            Assert-ProductE2EJsonFieldUniqueEquals $json["cpu"] "count_complete" $true
            Assert-ProductE2EJsonFieldUniqueEquals $json["cpu"] "probability_complete" $true
            Assert-ProductE2EJsonFieldUniqueEquals $json["cpu"] "resource_truncated" $false
            foreach ($field in $exactFields) {
                Assert-ProductE2EJsonFieldSame $json["cpu"] $json["gpu"] $field
                Assert-ProductE2EJsonFieldSame $json["cpu"] $json["hybrid"] $field
            }

            Assert-ProductE2ECpuSelectionReport $json["cpu"]
            Assert-ProductE2EGpuSelectionReport $json["gpu"] "gpu"
            Assert-ProductE2EGpuSelectionReport $json["hybrid"] "hybrid"
        } catch {
            $status = "failed"
            $errorMessage = New-ProductE2EFailureMessage `
                -CaseName $Name `
                -Reason $_.Exception.Message `
                -Command "clearra product backend equivalence cpu|gpu|hybrid" `
                -FixturePath $FixturePath `
                -GoldenPath "" `
                -Output $lastOutput
            throw $errorMessage
        } finally {
            $ProductResults.Add([pscustomobject]@{
                name = $Name
                kind = "cli-product-backend-equivalence"
                status = $status
                command = "clearra product backend equivalence cpu|gpu|hybrid"
                exit_code = $null
                fixture = $FixturePath
                golden = $null
                marker_count = 0
                duration_ms = [int][math]::Round(((Get-Date) - $started).TotalMilliseconds)
                error = $errorMessage
            })
        }
    }
}

function Invoke-ProductE2EOpening2LBackendEquivalenceCase {
    Invoke-ProductE2EBackendEquivalenceCase `
        -Name "product_backend_cpu_gpu_hybrid_same_opening_2l" `
        -FixturePath "tests/fixtures/pc/opening_2l_empty.json" `
        -CpuArgs @(Get-ProductE2EOpeningSearchArgs "cpu") `
        -GpuArgs @(Get-ProductE2EOpeningSearchArgs "gpu") `
        -HybridArgs @(Get-ProductE2EOpeningSearchArgs "hybrid") `
        -ExtraEquivalenceFields @("next_pc_available", "continuation_token_available", "continuation_token_version", "continuation_token", "continue_hint") `
        -ReuseExactReadOnlyProbes
}

function Invoke-ProductE2EScenario4LBackendEquivalenceCase {
    Invoke-ProductE2EBackendEquivalenceCase `
        -Name "product_backend_cpu_gpu_hybrid_same_scenario_4l" `
        -FixturePath "tests/fixtures/pc/scenario_simple_4l.json" `
        -CpuArgs @(Get-ProductE2EScenario4LSearchArgs "cpu") `
        -GpuArgs @(Get-ProductE2EScenario4LSearchArgs "gpu") `
        -HybridArgs @(Get-ProductE2EScenario4LSearchArgs "hybrid") `
        -ExtraEquivalenceFields @("scenario_replay_token_version", "scenario_replay_token") `
        -ReuseExactReadOnlyProbes
}

function Invoke-ProductE2EGpuNoFallbackUnavailableCase {
    Invoke-ClearraProgressCase -Scope $script:ProductE2EProgressScope -Name "product_gpu_no_fallback_returns_error_when_unavailable" -Body {
        $script:ProductE2ECurrentCaseName = "product_gpu_no_fallback_returns_error_when_unavailable"
        $started = Get-Date
        $status = "passed"
        $errorMessage = $null
        $commandArgs = @(Get-ProductE2EMissingAdapterArgs -DenyFallback)
        $result = $null

        try {
            $result = Invoke-ProductE2EClearra $commandArgs
            if ($result.ExitCode -ne 3) {
                throw "expected exit 3 but got $($result.ExitCode)"
            }
            Assert-ProductE2EMissingGpuDenial $result.Output
        } catch {
            $status = "failed"
            $errorMessage = New-ProductE2EFailureMessage `
                -CaseName "product_gpu_no_fallback_returns_error_when_unavailable" `
                -Reason $_.Exception.Message `
                -Command "clearra $($commandArgs -join ' ')" `
                -FixturePath "tests/fixtures/pc/opening_2l_empty.json" `
                -GoldenPath "" `
                -Output $(if ($null -eq $result) { "" } else { $result.Output })
            throw $errorMessage
        } finally {
            $ProductResults.Add([pscustomobject]@{
                name = "product_gpu_no_fallback_returns_error_when_unavailable"
                kind = "cli-product-error"
                status = $status
                command = "clearra $($commandArgs -join ' ')"
                exit_code = if ($null -eq $result) { $null } else { $result.ExitCode }
                fixture = "tests/fixtures/pc/opening_2l_empty.json"
                golden = $null
                marker_count = 0
                duration_ms = [int][math]::Round(((Get-Date) - $started).TotalMilliseconds)
                error = $errorMessage
            })
        }
    }
}

function Invoke-ProductE2EGpuAllowFallbackReasonCase {
    Invoke-ClearraProgressCase -Scope $script:ProductE2EProgressScope -Name "product_gpu_allow_fallback_reports_reason" -Body {
        $script:ProductE2ECurrentCaseName = "product_gpu_allow_fallback_reports_reason"
        $started = Get-Date
        $status = "passed"
        $errorMessage = $null
        $commandArgs = @(Get-ProductE2EMissingAdapterArgs)
        $result = $null

        try {
            $result = Invoke-ProductE2EClearra `
                -CommandArgs $commandArgs `
                -ReuseExactReadOnlyProbe
            if ($result.ExitCode -ne 0) {
                throw "expected exit 0 but got $($result.ExitCode)"
            }
            $json = ConvertFrom-ProductE2EJsonOutput $result.Output
            Assert-ProductE2EGpuCpuFallbackReport $json -MissingDevice
            $cpu = Invoke-ProductE2EClearra `
                -CommandArgs @(Get-ProductE2EOpeningSearchArgs "cpu") `
                -ReuseExactReadOnlyProbe
            if ($cpu.ExitCode -ne 0) {
                throw "CPU reference failed with exit $($cpu.ExitCode)"
            }
            $cpuJson = ConvertFrom-ProductE2EJsonOutput $cpu.Output
            Assert-ProductE2ECpuSelectionReport $cpuJson
            foreach ($field in @("count_complete", "probability_complete", "resource_truncated", "coverage_probability", "covered_pattern_count", "normalized_unique_solution_count", "normalized_solution_set_hash", "continuation_token")) {
                Assert-ProductE2EJsonFieldSame $cpuJson $json $field
            }
        } catch {
            $status = "failed"
            $errorMessage = New-ProductE2EFailureMessage `
                -CaseName "product_gpu_allow_fallback_reports_reason" `
                -Reason $_.Exception.Message `
                -Command "clearra $($commandArgs -join ' ')" `
                -FixturePath "tests/fixtures/pc/opening_2l_empty.json" `
                -GoldenPath "" `
                -Output $(if ($null -eq $result) { "" } else { $result.Output })
            throw $errorMessage
        } finally {
            $ProductResults.Add([pscustomobject]@{
                name = "product_gpu_allow_fallback_reports_reason"
                kind = "cli-product"
                status = $status
                command = "clearra $($commandArgs -join ' ')"
                exit_code = if ($null -eq $result) { $null } else { $result.ExitCode }
                fixture = "tests/fixtures/pc/opening_2l_empty.json"
                golden = $null
                marker_count = 0
                duration_ms = [int][math]::Round(((Get-Date) - $started).TotalMilliseconds)
                error = $errorMessage
            })
        }
    }
}

function Invoke-ProductE2EGpuBackendTrustStateCase {
    Invoke-ClearraProgressCase -Scope $script:ProductE2EProgressScope -Name "product_gpu_backend_report_includes_trust_state" -Body {
        $script:ProductE2ECurrentCaseName = "product_gpu_backend_report_includes_trust_state"
        $started = Get-Date
        $status = "passed"
        $errorMessage = $null
        $commandArgs = @(Get-ProductE2EMissingAdapterArgs)
        $result = $null

        try {
            $result = Invoke-ProductE2EClearra `
                -CommandArgs $commandArgs `
                -ReuseExactReadOnlyProbe
            if ($result.ExitCode -ne 0) {
                throw "expected exit 0 but got $($result.ExitCode)"
            }
            $json = ConvertFrom-ProductE2EJsonOutput $result.Output
            Assert-ProductE2EGpuCpuFallbackReport $json -MissingDevice
        } catch {
            $status = "failed"
            $errorMessage = New-ProductE2EFailureMessage `
                -CaseName "product_gpu_backend_report_includes_trust_state" `
                -Reason $_.Exception.Message `
                -Command "clearra $($commandArgs -join ' ')" `
                -FixturePath "tests/fixtures/pc/opening_2l_empty.json" `
                -GoldenPath "" `
                -Output $(if ($null -eq $result) { "" } else { $result.Output })
            throw $errorMessage
        } finally {
            $ProductResults.Add([pscustomobject]@{
                name = "product_gpu_backend_report_includes_trust_state"
                kind = "cli-product"
                status = $status
                command = "clearra $($commandArgs -join ' ')"
                exit_code = if ($null -eq $result) { $null } else { $result.ExitCode }
                fixture = "tests/fixtures/pc/opening_2l_empty.json"
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
