# Product E2E command execution and backend case stage.

function Invoke-ProductE2EClearra {
    param(
        [Parameter(Mandatory)]
        [string[]]$CommandArgs
    )

    Push-Location $Root
    try {
        if ($UseBuiltBinary.IsPresent) {
            $resolvedExe = Resolve-ProductE2EBinary
            if ([string]::IsNullOrWhiteSpace($resolvedExe)) {
                throw "Product E2E executable path is empty."
            }
            if ([System.IO.Path]::GetFileName($resolvedExe) -in @("clearra-cli.exe", "clearra-cli")) {
                throw "Product E2E accepts only the release-facing clearra executable."
            }
            $nativeResult = Invoke-NativeWithProgress `
                -Scope $script:ProductE2EProgressScope `
                -Label $script:ProductE2ECurrentCaseName `
                -FileName $resolvedExe `
                -Arguments $CommandArgs
            $exitCode = $nativeResult.ExitCode
            $text = $nativeResult.Output
        } else {
            $previousCargoTargetDir = $env:CARGO_TARGET_DIR
            $previousWindowsRustFlags = $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS
            $setCargoTargetDir = $false
            if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
                $env:CARGO_TARGET_DIR = Get-ClearraCargoTargetDir
                $setCargoTargetDir = $true
            } else {
                Assert-ClearraCanonicalCargoTargetDir $previousCargoTargetDir | Out-Null
            }
            $nativeLibraryDir = Resolve-ProductE2ENativeLibraryDir
            Sync-ClearraNativeCargoLinkState `
                -LibraryDirectory $nativeLibraryDir `
                -CargoTargetDirectory $env:CARGO_TARGET_DIR `
                -CargoPath 'cargo' `
                -WorkspaceRoot $Root
            $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS =
                Add-ClearraWindowsNativeRustLinkFlags $previousWindowsRustFlags $nativeLibraryDir
            $cargoArgs = @(
                "run", "-q", "-p", "clearra-cli", "--features", "native-c-core,webgpu-search", "--"
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
                if ([string]::IsNullOrWhiteSpace($previousWindowsRustFlags)) {
                    Remove-Item Env:\CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS -ErrorAction SilentlyContinue
                } else {
                    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = $previousWindowsRustFlags
                }
            }
        }

        return [pscustomobject]@{
            Command = "clearra $($CommandArgs -join ' ')"
            ExitCode = $exitCode
            Output = $text
        }
    } finally {
        Pop-Location
    }
}

function Invoke-ProductE2ECommandCase(
    [string]$Name,
    [string]$FixturePath,
    [string]$GoldenPath,
    [string[]]$CommandArgs,
    [int]$ExpectedExitCode = 0
) {
    Invoke-ClearraProgressCase -Scope $script:ProductE2EProgressScope -Name $Name -Body {
        $script:ProductE2ECurrentCaseName = $Name
        $started = Get-Date
        if ($VerboseLog.IsPresent) {
            Write-ClearraProgressVerboseLine $script:ProductE2EProgressScope "[product-e2e] running | $Name"
        }
        $commandResult = Invoke-ProductE2EClearra $CommandArgs
        $duration = (Get-Date) - $started
        $status = "passed"
        $errorMessage = $null
        $markerCount = 0

        try {
            if ($commandResult.ExitCode -ne $ExpectedExitCode) {
                throw "expected exit $ExpectedExitCode but got $($commandResult.ExitCode)`n$(Get-ProductE2EExcerpt $commandResult.Output)"
            }
            $markerText = ConvertTo-ProductE2EMarkerText $commandResult.Output
            $requiredMarkers = Read-ProductE2ERequiredMarkers $GoldenPath
            $markerCount = $requiredMarkers.Count
            Assert-ProductE2EMarkers $Name $markerText $requiredMarkers
            Assert-ProductE2ETypedCommandAssertions $Name $commandResult.Output
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
                Write-ClearraProgressVerboseLine $script:ProductE2EProgressScope "[product-e2e] failed  | $Name | $([math]::Round($duration.TotalSeconds, 2))s"
            }
            throw $errorMessage
        } finally {
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

function Invoke-ProductE2EBackendParityCase {
    Invoke-ClearraProgressCase -Scope $script:ProductE2EProgressScope -Name "backend fallback parity matches CPU product contract" -Body {
        $script:ProductE2ECurrentCaseName = "backend fallback parity matches CPU product contract"
        $started = Get-Date
        $status = "passed"
        $errorMessage = $null
        $commands = @(
            [pscustomobject]@{
                Label = "cpu"
                Args = @("--format", "json", "pc", "--lines", "2", "--queue", "IIOOOIIOOO", "--fixed", "--no-hold", "--objective", "min-cover", "--backend", "cpu")
            },
            [pscustomobject]@{
                Label = "gpu"
                Args = @("--format", "json", "pc", "--lines", "2", "--queue", "IIOOOIIOOO", "--fixed", "--no-hold", "--objective", "min-cover", "--backend", "gpu", "--allow-backend-fallback")
            },
            [pscustomobject]@{
                Label = "hybrid"
                Args = @("--format", "json", "pc", "--lines", "2", "--queue", "IIOOOIIOOO", "--fixed", "--no-hold", "--objective", "min-cover", "--backend", "hybrid", "--allow-backend-fallback")
            }
        )
        $outputs = @{}
        $json = @{}
        $lastOutput = ""

        try {
            foreach ($command in $commands) {
                $result = Invoke-ProductE2EClearra $command.Args
                $outputs[$command.Label] = $result
                $lastOutput = $result.Output
                if ($result.ExitCode -ne 0) {
                    throw "$($command.Label) backend command failed with exit $($result.ExitCode)"
                }
                $json[$command.Label] = ConvertFrom-ProductE2EJsonOutput $result.Output
            }

            foreach ($field in @("coverage_probability", "covered_pattern_count", "next_pc_available", "continuation_token_available", "continuation_token_version", "continue_hint")) {
                Assert-ProductE2EJsonFieldSame $json["cpu"] $json["gpu"] $field
                Assert-ProductE2EJsonFieldSame $json["cpu"] $json["hybrid"] $field
            }

            Assert-ProductE2EJsonFieldEquals $json["cpu"] "backend_requested" "cpu"
            Assert-ProductE2EJsonFieldEquals $json["cpu"] "backend_selected" "cpu"
            Assert-ProductE2EJsonFieldEquals $json["cpu"] "backend_fallback_used" "false"
            Assert-ProductE2EJsonFieldEquals $json["gpu"] "backend_requested" "gpu"
            Assert-ProductE2EJsonFieldEquals $json["gpu"] "backend_selected" "cpu"
            Assert-ProductE2EJsonFieldEquals $json["gpu"] "backend_fallback_used" "true"
            Assert-ProductE2EJsonFieldEquals $json["gpu"] "backend_fallback_reason" "gpu_kernel_unavailable"
            Assert-ProductE2EJsonFieldEquals $json["hybrid"] "backend_requested" "hybrid"
            Assert-ProductE2EJsonFieldEquals $json["hybrid"] "backend_selected" "cpu"
            Assert-ProductE2EJsonFieldEquals $json["hybrid"] "backend_fallback_used" "true"
            Assert-ProductE2EJsonFieldEquals $json["hybrid"] "backend_fallback_reason" "gpu_kernel_unavailable"
        } catch {
            $status = "failed"
            $errorMessage = New-ProductE2EFailureMessage `
                -CaseName "backend fallback parity matches CPU product contract" `
                -Reason $_.Exception.Message `
                -Command "clearra pc --backend cpu|gpu|hybrid" `
                -FixturePath "tests/fixtures/continuation/pc_then_next_pc_available.json" `
                -GoldenPath "" `
                -Output $lastOutput
            throw $errorMessage
        } finally {
            $ProductResults.Add([pscustomobject]@{
                name = "backend fallback parity matches CPU product contract"
                kind = "cli-product"
                status = $status
                command = "clearra pc --backend cpu|gpu|hybrid"
                exit_code = $null
                fixture = "tests/fixtures/continuation/pc_then_next_pc_available.json"
                golden = $null
                marker_count = 0
                duration_ms = [int][math]::Round(((Get-Date) - $started).TotalMilliseconds)
                error = $errorMessage
            })
        }
    }
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
                Args = @("--format", "json", "pc", "--lines", "2", "--queue", "IIOOOIIOOO", "--fixed", "--no-hold", "--objective", "min-cover", "--backend", "cpu")
            },
            [pscustomobject]@{
                Label = "gpu"
                Args = @("--format", "json", "pc", "--lines", "2", "--queue", "IIOOOIIOOO", "--fixed", "--no-hold", "--objective", "min-cover", "--backend", "gpu", "--allow-backend-fallback")
            },
            [pscustomobject]@{
                Label = "hybrid"
                Args = @("--format", "json", "pc", "--lines", "2", "--queue", "IIOOOIIOOO", "--fixed", "--no-hold", "--objective", "min-cover", "--backend", "hybrid", "--allow-backend-fallback")
            }
        )
        $json = @{}
        $lastOutput = ""

        try {
            foreach ($command in $commands) {
                $result = Invoke-ProductE2EClearra $command.Args
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
    [string]$FixturePath
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
                $result = Invoke-ProductE2EClearra $command.Args
                $lastOutput = $result.Output
                if ($result.ExitCode -ne 0) {
                    throw "$($command.Label) backend command failed with exit $($result.ExitCode)"
                }
                $json[$command.Label] = ConvertFrom-ProductE2EJsonOutput $result.Output
            }

            foreach ($field in @("coverage_probability", "covered_pattern_count", "next_pc_available", "continuation_token_available", "continuation_token_version", "continue_hint")) {
                Assert-ProductE2EJsonFieldSame $json["cpu"] $json["gpu"] $field
                Assert-ProductE2EJsonFieldSame $json["cpu"] $json["hybrid"] $field
            }

            Assert-ProductE2EJsonFieldEquals $json["cpu"] "backend_requested" "cpu"
            Assert-ProductE2EJsonFieldEquals $json["cpu"] "backend_selected" "cpu"
            Assert-ProductE2EJsonFieldEquals $json["cpu"] "backend_fallback_used" "false"
            Assert-ProductE2EJsonFieldEquals $json["gpu"] "backend_requested" "gpu"
            Assert-ProductE2EJsonFieldEquals $json["gpu"] "backend_selected" "cpu"
            Assert-ProductE2EJsonFieldEquals $json["gpu"] "backend_fallback_used" "true"
            Assert-ProductE2EJsonFieldEquals $json["gpu"] "backend_fallback_reason" "gpu_kernel_unavailable"
            Assert-ProductE2EJsonFieldEquals $json["hybrid"] "backend_requested" "hybrid"
            Assert-ProductE2EJsonFieldEquals $json["hybrid"] "backend_selected" "cpu"
            Assert-ProductE2EJsonFieldEquals $json["hybrid"] "backend_fallback_used" "true"
            Assert-ProductE2EJsonFieldEquals $json["hybrid"] "backend_fallback_reason" "gpu_kernel_unavailable"
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
        -CpuArgs @("--format", "json", "pc", "--lines", "2", "--queue", "IIOOOIIOOO", "--fixed", "--no-hold", "--objective", "min-cover", "--backend", "cpu") `
        -GpuArgs @("--format", "json", "pc", "--lines", "2", "--queue", "IIOOOIIOOO", "--fixed", "--no-hold", "--objective", "min-cover", "--backend", "gpu", "--allow-backend-fallback") `
        -HybridArgs @("--format", "json", "pc", "--lines", "2", "--queue", "IIOOOIIOOO", "--fixed", "--no-hold", "--objective", "min-cover", "--backend", "hybrid", "--allow-backend-fallback")
}

function Invoke-ProductE2EScenario4LBackendEquivalenceCase {
    Invoke-ProductE2EBackendEquivalenceCase `
        -Name "product_backend_cpu_gpu_hybrid_same_scenario_4l" `
        -FixturePath "tests/fixtures/pc/scenario_simple_4l.json" `
        -CpuArgs @("--format", "json", "pc-scenario", "--fixture", "tests/fixtures/pc/scenario_simple_4l.json", "--verify-expected", "--backend", "cpu") `
        -GpuArgs @("--format", "json", "pc-scenario", "--fixture", "tests/fixtures/pc/scenario_simple_4l.json", "--verify-expected", "--backend", "gpu", "--allow-backend-fallback") `
        -HybridArgs @("--format", "json", "pc-scenario", "--fixture", "tests/fixtures/pc/scenario_simple_4l.json", "--verify-expected", "--backend", "hybrid", "--allow-backend-fallback")
}

function Invoke-ProductE2EGpuNoFallbackCase {
    Invoke-ClearraProgressCase -Scope $script:ProductE2EProgressScope -Name "gpu no-backend-fallback reports diagnostic reason" -Body {
        $script:ProductE2ECurrentCaseName = "gpu no-backend-fallback reports diagnostic reason"
        $started = Get-Date
        $status = "passed"
        $errorMessage = $null
        $commandArgs = @("--format", "json", "pc", "--lines", "2", "--queue", "IIOOOIIOOO", "--fixed", "--no-hold", "--objective", "min-cover", "--backend", "gpu", "--no-backend-fallback")
        $result = $null

        try {
            $result = Invoke-ProductE2EClearra $commandArgs
            if ($result.ExitCode -ne 3) {
                throw "expected exit 3 but got $($result.ExitCode)"
            }
            if ($result.Output -notlike "*E_BACKEND_GPU_UNAVAILABLE*") {
                throw "missing diagnostic code E_BACKEND_GPU_UNAVAILABLE"
            }
            if ($result.Output -notlike "*gpu_kernel_unavailable*") {
                throw "missing diagnostic reason gpu_kernel_unavailable"
            }
            if ($result.Output -like "*backend_selected=cpu*" -or $result.Output -like "*selected_backend=cpu-geometry-exact-cover*") {
                throw "GPU no-fallback output must not report CPU selection"
            }
        } catch {
            $status = "failed"
            $errorMessage = New-ProductE2EFailureMessage `
                -CaseName "gpu no-backend-fallback reports diagnostic reason" `
                -Reason $_.Exception.Message `
                -Command "clearra $($commandArgs -join ' ')" `
                -FixturePath "tests/fixtures/continuation/pc_then_next_pc_available.json" `
                -GoldenPath "" `
                -Output $(if ($null -eq $result) { "" } else { $result.Output })
            throw $errorMessage
        } finally {
            $ProductResults.Add([pscustomobject]@{
                name = "gpu no-backend-fallback reports diagnostic reason"
                kind = "cli-product-error"
                status = $status
                command = "clearra $($commandArgs -join ' ')"
                exit_code = if ($null -eq $result) { $null } else { $result.ExitCode }
                fixture = "tests/fixtures/continuation/pc_then_next_pc_available.json"
                golden = $null
                marker_count = 0
                duration_ms = [int][math]::Round(((Get-Date) - $started).TotalMilliseconds)
                error = $errorMessage
            })
        }
    }
}

function Invoke-ProductE2EGpuNoFallbackUnavailableCase {
    Invoke-ClearraProgressCase -Scope $script:ProductE2EProgressScope -Name "product_gpu_no_fallback_returns_error_when_unavailable" -Body {
        $script:ProductE2ECurrentCaseName = "product_gpu_no_fallback_returns_error_when_unavailable"
        $started = Get-Date
        $status = "passed"
        $errorMessage = $null
        $commandArgs = @("--format", "json", "pc", "--lines", "2", "--queue", "IIOOOIIOOO", "--fixed", "--no-hold", "--objective", "min-cover", "--backend", "gpu", "--no-backend-fallback")
        $result = $null

        try {
            $result = Invoke-ProductE2EClearra $commandArgs
            if ($result.ExitCode -ne 3) {
                throw "expected exit 3 but got $($result.ExitCode)"
            }
            if ($result.Output -notlike "*E_BACKEND_GPU_UNAVAILABLE*") {
                throw "missing diagnostic code E_BACKEND_GPU_UNAVAILABLE"
            }
            if ($result.Output -notlike "*gpu_kernel_unavailable*") {
                throw "missing diagnostic reason gpu_kernel_unavailable"
            }
            if ($result.Output -like "*backend_selected=cpu*" -or $result.Output -like "*selected_backend=cpu-geometry-exact-cover*") {
                throw "GPU no-fallback output must not report CPU selection"
            }
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
        $commandArgs = @("--format", "json", "pc", "--lines", "2", "--queue", "IIOOOIIOOO", "--fixed", "--no-hold", "--objective", "min-cover", "--backend", "gpu", "--allow-backend-fallback")
        $result = $null

        try {
            $result = Invoke-ProductE2EClearra $commandArgs
            if ($result.ExitCode -ne 0) {
                throw "expected exit 0 but got $($result.ExitCode)"
            }
            $json = ConvertFrom-ProductE2EJsonOutput $result.Output
            Assert-ProductE2EJsonFieldEquals $json "backend_requested" "gpu"
            Assert-ProductE2EJsonFieldEquals $json "backend_selected" "cpu"
            Assert-ProductE2EJsonFieldEquals $json "backend_fallback_used" "true"
            Assert-ProductE2EJsonFieldEquals $json "backend_fallback_reason" "gpu_kernel_unavailable"
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
        $commandArgs = @("--format", "json", "pc", "--lines", "2", "--queue", "IIOOOIIOOO", "--fixed", "--no-hold", "--objective", "min-cover", "--backend", "gpu", "--allow-backend-fallback")
        $result = $null

        try {
            $result = Invoke-ProductE2EClearra $commandArgs
            if ($result.ExitCode -ne 0) {
                throw "expected exit 0 but got $($result.ExitCode)"
            }
            $json = ConvertFrom-ProductE2EJsonOutput $result.Output
            Assert-ProductE2EJsonFieldEquals $json "gpu_trust_state" "fallback-used"
            Assert-ProductE2EJsonFieldEquals $json "gpu_failure_class" "unavailable"
            Assert-ProductE2EJsonFieldEquals $json "fallback_backend" "cpu"
            Assert-ProductE2EJsonFieldEquals $json "backend_fallback_reason" "gpu_kernel_unavailable"
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
