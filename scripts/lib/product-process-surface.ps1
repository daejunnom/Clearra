# Product binary build and process-launch ownership. This surface requires
# Trusted execution because Cargo compilation itself runs generated helpers.
# The command executes once and fails closed without a retry or fallback route.

function Get-ClearraBuiltBinaryPath([string]$Root) {
    $builtCargoTargetDir = Get-ClearraCargoTargetDir
    return (Join-Path $builtCargoTargetDir "debug/clearra.exe")
}

function Ensure-ClearraBuiltBinary([string]$Root) {
    $builtExePath = Get-ClearraBuiltBinaryPath $Root
    $builtCargoTargetDir = Split-Path -Parent (Split-Path -Parent $builtExePath)
    $nativeBuildDir = Get-StartTestsPersistentBuildDir "core-c-library-cache"
    $nativeBuild = Invoke-CoreCBuild `
        -BuildDir $nativeBuildDir `
        -Configuration "Debug" `
        -ConfigureArgs (Get-StartTestsCMakeConfigureArgs @("-DBUILD_TESTING=OFF")) `
        -BuildWorkers ([Math]::Max(1, $Workers))
    if ($nativeBuild.Status -ne "Passed") {
        throw "release product binary native C core build failed: $($nativeBuild.Reason)"
    }
    $nativeLibraryDir = Find-CoreCLibraryDir $nativeBuildDir
    if ([string]::IsNullOrWhiteSpace($nativeLibraryDir)) {
        throw "release product binary could not find clearra_core under $nativeBuildDir"
    }

    $buildScope = New-ClearraProgressScope `
        -Name "release-build" `
        -Total 1 `
        -Workers 1 `
        -VerboseLog:$VerboseLog.IsPresent
    $previousCargoTargetDir = $env:CARGO_TARGET_DIR
    $previousWindowsRustFlags = $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS
    $env:CARGO_TARGET_DIR = Assert-ClearraCanonicalCargoTargetDir $builtCargoTargetDir
    $null = Sync-ClearraNativeCargoLinkState `
        -LibraryDirectory $nativeLibraryDir `
        -CargoTargetDirectory $env:CARGO_TARGET_DIR `
        -CargoPath $CargoPath `
        -WorkspaceRoot $Root
    $nativeWindowsRustFlags =
        Add-ClearraWindowsNativeRustLinkFlags $previousWindowsRustFlags $nativeLibraryDir
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = $nativeWindowsRustFlags

    try {
        Invoke-ClearraProgressCase `
            -Scope $buildScope `
            -Name "cargo build clearra" `
            -Body {
                $result = Invoke-NativeWithProgress `
                    -Scope $buildScope `
                    -Label "cargo build clearra" `
                    -FileName $CargoPath `
                    -Arguments @(
                        "build", "-p", "clearra-cli", "--features", "native-c-core,webgpu-search",
                        "--bin", "clearra"
                    )
                if ($VerboseLog.IsPresent -and -not [string]::IsNullOrWhiteSpace($result.Output)) {
                    Complete-ClearraProgressLine $buildScope
                    Write-Output $result.Output
                }
                if ($result.ExitCode -ne 0) {
                    throw "release product binary build failed with exit $($result.ExitCode)`n$($result.Output)"
                }
            }
    } finally {
        if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
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

    Complete-ClearraProgressLine $buildScope
    if (-not (Test-Path -LiteralPath $builtExePath)) {
        throw "release product binary was not produced: $builtExePath"
    }
    return $builtExePath
}

function Invoke-ProductE2EBuiltTask([string]$Root) {
    Assert-ClearraTrustedExecutionSurface $ExecutionSurface "built product E2E"
    $builtExePath = Ensure-ClearraBuiltBinary $Root
    $productE2EArgs = @{
        UseBuiltBinary = $true
        ExePath = $builtExePath
        OutputExcerptLines = $OutputExcerptLines
    }
    if ($VerboseLog.IsPresent) {
        $productE2EArgs["VerboseLog"] = $true
    }
    if (-not [string]::IsNullOrWhiteSpace($ReportPath)) {
        $productE2EArgs["ReportPath"] = $ReportPath
    }
    & (Join-Path $Root "scripts/product-e2e.ps1") @productE2EArgs

    $nodeCommand = Get-Command "node" -ErrorAction SilentlyContinue
    if ($null -eq $nodeCommand) {
        throw "Discord terminal-supply product acceptance requires node on PATH."
    }
    $acceptedCtk3Dist = $env:CLEARRA_ACCEPTED_CTK3_DIST
    $releaseModeVariable = Get-Variable `
        -Name ClearraReleaseAcceptanceMode `
        -Scope Script `
        -ErrorAction SilentlyContinue
    $releaseMode = $null -ne $releaseModeVariable -and [bool]$releaseModeVariable.Value
    $terminalSupplyScope = New-ClearraProgressScope `
        -Name "terminal-supply-product" `
        -Total 3 `
        -Workers 1 `
        -VerboseLog:$VerboseLog.IsPresent
    try {
        if (-not [string]::IsNullOrWhiteSpace($acceptedCtk3Dist)) {
            if (-not $releaseMode) {
                throw "Accepted CTK3 artifacts may only be consumed by ReleaseAcceptance."
            }
            if ($env:CLEARRA_SOURCE_COMMIT -notmatch '^[0-9a-f]{40}$') {
                throw "Accepted CTK3 release consumption requires an exact lowercase source commit."
            }
            if ($env:CLEARRA_ACCEPTED_RUN_ID -notmatch '^[1-9][0-9]{0,19}$') {
                throw "Accepted CTK3 release consumption requires the canonical acceptance run ID."
            }
            if ($env:CLEARRA_ACCEPTED_RUN_ATTEMPT -notmatch '^[1-9][0-9]{0,19}$') {
                throw "Accepted CTK3 release consumption requires the canonical acceptance run attempt."
            }
            $resolvedAcceptedCtk3Dist = (Resolve-Path -LiteralPath $acceptedCtk3Dist -ErrorAction Stop).Path
            $expectedAcceptedCtk3Dist = (Resolve-Path `
                -LiteralPath (Join-Path $Root "packages/ctk3/dist") `
                -ErrorAction Stop).Path
            $pathComparison = if ($env:OS -eq "Windows_NT") {
                [System.StringComparison]::OrdinalIgnoreCase
            } else {
                [System.StringComparison]::Ordinal
            }
            if (-not $resolvedAcceptedCtk3Dist.Equals($expectedAcceptedCtk3Dist, $pathComparison)) {
                throw "Accepted CTK3 distribution must be downloaded to packages/ctk3/dist."
            }
            $acceptedCtk3Verifier = Join-Path $Root "scripts/tools/accepted-ctk3-dist.mjs"
            if (-not (Test-Path -LiteralPath $acceptedCtk3Verifier -PathType Leaf)) {
                throw "Accepted CTK3 verifier is missing: $acceptedCtk3Verifier"
            }
            Invoke-ClearraProgressCase `
                -Scope $terminalSupplyScope `
                -Name "Verify accepted CTK3 distribution" `
                -Body {
                    $ctk3VerifyResult = Invoke-NativeWithProgress `
                        -Scope $terminalSupplyScope `
                        -Label "Verify accepted CTK3 distribution" `
                        -FileName $nodeCommand.Source `
                        -Arguments @(
                            $acceptedCtk3Verifier,
                            "--verify",
                            $resolvedAcceptedCtk3Dist,
                            "--expected-source-commit",
                            $env:CLEARRA_SOURCE_COMMIT,
                            "--expected-run-id",
                            $env:CLEARRA_ACCEPTED_RUN_ID,
                            "--expected-run-attempt",
                            $env:CLEARRA_ACCEPTED_RUN_ATTEMPT
                        )
                    if ($ctk3VerifyResult.ExitCode -ne 0) {
                        throw "ReleaseAcceptance rejected the accepted CTK3 distribution with exit $($ctk3VerifyResult.ExitCode)`n$($ctk3VerifyResult.Output)"
                    }
                }
        } else {
            $npmName = if ($env:OS -eq "Windows_NT") { "npm.cmd" } else { "npm" }
            $npmCommand = Get-Command $npmName -ErrorAction SilentlyContinue
            if ($null -eq $npmCommand) {
                throw "Discord terminal-supply product acceptance requires npm on PATH."
            }
            Invoke-ClearraProgressCase `
                -Scope $terminalSupplyScope `
                -Name "npm build ctk3" `
                -Body {
                    $ctk3BuildResult = Invoke-NativeWithProgress `
                        -Scope $terminalSupplyScope `
                        -Label "npm build ctk3" `
                        -FileName $npmCommand.Source `
                        -Arguments @("run", "build", "--workspace", "ctk3")
                    if ($ctk3BuildResult.ExitCode -ne 0) {
                        throw "Discord terminal-supply product acceptance could not build ctk3 with exit $($ctk3BuildResult.ExitCode)`n$($ctk3BuildResult.Output)"
                    }
                }
        }
        $probePath = Join-Path $Root "apps/clearra-discord-bot/scripts/verify-terminal-supply-product.mjs"
        # Release identity pins the exact artifact probe argv: $probePath "--clearra" $builtExePath
        $probeState = [pscustomobject]@{ Output = "" }
        Invoke-ClearraProgressCase `
            -Scope $terminalSupplyScope `
            -Name "Discord terminal-supply product probe" `
            -Body {
                $probeResult = Invoke-NativeWithProgress `
                    -Scope $terminalSupplyScope `
                    -Label "Discord terminal-supply product probe" `
                    -FileName $nodeCommand.Source `
                    -Arguments @($probePath, "--clearra", $builtExePath)
                if ($probeResult.ExitCode -ne 0) {
                    throw "Discord terminal-supply product acceptance failed with exit $($probeResult.ExitCode)`n$($probeResult.Output)"
                }
                $probeState.Output = $probeResult.Output
            }
        Write-Output $probeState.Output

        $uiProbePath = Join-Path $Root "packages/clearra-ui/scripts/verify-terminal-supply-product.mjs"
        # Release identity pins the exact artifact probe argv: $uiProbePath "--clearra" $builtExePath
        $uiProbeState = [pscustomobject]@{ Output = "" }
        Invoke-ClearraProgressCase `
            -Scope $terminalSupplyScope `
            -Name "UI terminal-supply product probe" `
            -Body {
                $uiProbeResult = Invoke-NativeWithProgress `
                    -Scope $terminalSupplyScope `
                    -Label "UI terminal-supply product probe" `
                    -FileName $nodeCommand.Source `
                    -Arguments @($uiProbePath, "--clearra", $builtExePath)
                if ($uiProbeResult.ExitCode -ne 0) {
                    throw "UI terminal-supply product acceptance failed with exit $($uiProbeResult.ExitCode)`n$($uiProbeResult.Output)"
                }
                $uiProbeState.Output = $uiProbeResult.Output
            }
        Write-Output $uiProbeState.Output
    } finally {
        Complete-ClearraProgressLine $terminalSupplyScope
    }
}

function Set-ClearraReleaseUxSmokeBinaryArgs([hashtable]$Arguments, [string]$Root) {
    Assert-ClearraTrustedExecutionSurface $ExecutionSurface "release UX smoke"
    $builtExePath = Ensure-ClearraBuiltBinary $Root
    $Arguments["UseBuiltBinary"] = $true
    $Arguments["ExePath"] = $builtExePath
}

function Invoke-ProductProcessE2ETask([string]$Root) {
    Assert-ClearraTrustedExecutionSurface $ExecutionSurface "process ProductE2E"
    $productE2EArgs = @{ OutputExcerptLines = $OutputExcerptLines }
    if ($VerboseLog.IsPresent) {
        $productE2EArgs["VerboseLog"] = $true
    }
    if (-not [string]::IsNullOrWhiteSpace($ReportPath)) {
        $productE2EArgs["ReportPath"] = $ReportPath
    }
    & (Join-Path $Root "scripts/product-e2e.ps1") @productE2EArgs
}
