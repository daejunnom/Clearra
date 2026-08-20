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

    $npmName = if ($env:OS -eq "Windows_NT") { "npm.cmd" } else { "npm" }
    $npmCommand = Get-Command $npmName -ErrorAction SilentlyContinue
    if ($null -eq $npmCommand) {
        throw "Discord terminal-supply product acceptance requires npm on PATH."
    }
    $ctk3BuildOutput = & $npmCommand.Source "run" "build" "--workspace" "ctk3" 2>&1
    $ctk3BuildExitCode = $LASTEXITCODE
    if ($ctk3BuildExitCode -ne 0) {
        throw "Discord terminal-supply product acceptance could not build ctk3 with exit $ctk3BuildExitCode`n$($ctk3BuildOutput -join "`n")"
    }

    $nodeCommand = Get-Command "node" -ErrorAction SilentlyContinue
    if ($null -eq $nodeCommand) {
        throw "Discord terminal-supply product acceptance requires node on PATH."
    }
    $probePath = Join-Path $Root "apps/clearra-discord-bot/scripts/verify-terminal-supply-product.mjs"
    $probeOutput = & $nodeCommand.Source $probePath "--clearra" $builtExePath 2>&1
    $probeExitCode = $LASTEXITCODE
    if ($probeExitCode -ne 0) {
        throw "Discord terminal-supply product acceptance failed with exit $probeExitCode`n$($probeOutput -join "`n")"
    }
    Write-Output ($probeOutput -join "`n")

    $uiProbePath = Join-Path $Root "packages/clearra-ui/scripts/verify-terminal-supply-product.mjs"
    $uiProbeOutput = & $nodeCommand.Source $uiProbePath "--clearra" $builtExePath 2>&1
    $uiProbeExitCode = $LASTEXITCODE
    if ($uiProbeExitCode -ne 0) {
        throw "UI terminal-supply product acceptance failed with exit $uiProbeExitCode`n$($uiProbeOutput -join "`n")"
    }
    Write-Output ($uiProbeOutput -join "`n")
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
