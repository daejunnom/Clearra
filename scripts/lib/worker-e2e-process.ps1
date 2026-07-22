# This file is dot-sourced by scripts/worker-e2e.ps1.
function Resolve-WorkerE2EBinary {
    if (-not [string]::IsNullOrWhiteSpace($script:WorkerE2EExePath)) {
        return $script:WorkerE2EExePath
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
}function Assert-WorkerE2EBinaryAllowed([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path)) {
        throw "WorkerE2E executable path is empty."
    }
    $fileName = [System.IO.Path]::GetFileName($Path)
    if ($fileName -ieq "clearra-cli.exe" -or $fileName -ieq "clearra-cli") {
        throw "Refusing to launch stale CLI binary '$Path'. WorkerE2E accepts only clearra.exe/clearra or cargo package route."
    }
}function Remove-StaleWorkerE2EClearraCliBinary {
    foreach ($stalePath in @(
            (Join-Path (Get-ClearraCargoTargetDir) "debug/clearra-cli.exe")
        )) {
        if (Test-Path -LiteralPath $stalePath) {
            Remove-Item -LiteralPath $stalePath -Force
        }
    }
}function Invoke-WorkerE2EClearra {
    param(
        [Parameter(Mandatory)]
        [string[]]$CommandArgs
    )

    Push-Location $script:WorkerE2ERoot
    try {
        if ($script:WorkerE2EUseBuiltBinary) {
            $resolvedExe = Resolve-WorkerE2EBinary
            Assert-WorkerE2EBinaryAllowed $resolvedExe
            $nativeResult = Invoke-NativeWithProgress `
                -Scope $script:WorkerE2EProgressScope `
                -Label $script:WorkerE2ECurrentCaseName `
                -FileName $resolvedExe `
                -Arguments $CommandArgs
        } else {
            $previousCargoTargetDir = $env:CARGO_TARGET_DIR
            $setCargoTargetDir = $false
            if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
                $env:CARGO_TARGET_DIR = Get-ClearraCargoTargetDir
                $setCargoTargetDir = $true
            } else {
                Assert-ClearraCanonicalCargoTargetDir $previousCargoTargetDir | Out-Null
            }
            try {
                $cargoArgs = @(
                    "run", "-q", "-p", "clearra-cli",
                    "--features", "native-c-core,webgpu-search",
                    "--bin", "clearra", "--"
                ) + $CommandArgs
                $nativeResult = Invoke-NativeWithProgress `
                    -Scope $script:WorkerE2EProgressScope `
                    -Label $script:WorkerE2ECurrentCaseName `
                    -FileName "cargo" `
                    -Arguments $cargoArgs
            } finally {
                if ($setCargoTargetDir) {
                    Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
                } else {
                    $env:CARGO_TARGET_DIR = $previousCargoTargetDir
                }
            }
        }

        return [pscustomobject]@{
            Command = "clearra $($CommandArgs -join ' ')"
            ExitCode = $nativeResult.ExitCode
            Output = $nativeResult.Output
        }
    } finally {
        Pop-Location
    }
}
