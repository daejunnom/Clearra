# GPU worker verification workflows. Search correctness remains in the native
# C tests and product library route; these functions orchestrate only the gate.

function Invoke-GpuWorkerCargoCheck([string]$Root, [string[]]$Packages) {
    $scope = New-ClearraProgressScope `
        -Name "gpu-worker-rust" `
        -Total $Packages.Count `
        -Workers 1 `
        -VerboseLog:$VerboseLog.IsPresent
    $previousCargoTargetDir = $env:CARGO_TARGET_DIR
    $env:CARGO_TARGET_DIR = Get-ClearraCargoTargetDir

    try {
        foreach ($packageName in $Packages) {
            $arguments = @("check", "-p", $packageName, "--lib", "--tests")
            if ($packageName -in @("clearra-core-executor", "clearra-app")) {
                $arguments += @("--features", "native-c-core,webgpu-search")
            }
            Invoke-ClearraProgressCase `
                -Scope $scope `
                -Name "cargo check $packageName --tests" `
                -Body {
                    $result = Invoke-NativeWithProgress `
                        -Scope $scope `
                        -Label "cargo check $packageName --tests" `
                        -FileName $CargoPath `
                        -Arguments $arguments
                    if ($VerboseLog.IsPresent -and -not [string]::IsNullOrWhiteSpace($result.Output)) {
                        Complete-ClearraProgressLine $scope
                        Write-Output $result.Output
                    }
                    if ($result.ExitCode -ne 0) {
                        throw "GPU worker Rust check failed for $packageName with exit $($result.ExitCode)`n$($result.Output)"
                    }
                }
        }
    } finally {
        if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
            Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
        } else {
            $env:CARGO_TARGET_DIR = $previousCargoTargetDir
        }
        Complete-ClearraProgressLine $scope
    }
}

function Invoke-GpuWorkerAcceptanceTask([string]$Root) {
    Invoke-GpuWorkerCargoCheck $Root @(
        "clearra-core-executor",
        "clearra-output",
        "clearra-app"
    )
    Invoke-CoreCTestStartMode $Root "GpuWorkerAcceptance" (Get-StartTestsCMakeConfigureArgs)
    Invoke-ProductE2EBuiltTask $Root
    & (Join-Path $Root "scripts/desktop-host-check.ps1") `
        -Workers $Workers `
        -ExecutionSurface $ExecutionSurface
    Invoke-ClearraValidationTask $Root
}

function Invoke-GpuWorkerNativeTask([string]$Root) {
    Invoke-NativeLocalMode $Root
    Invoke-CoreCTestStartMode `
        $Root `
        "GpuWorkerNativeSplit" `
        (Get-StartTestsCMakeConfigureArgs @("-DCLEARRA_CORE_SPLIT_TESTS=ON")) `
        -AggregateOnly:$false
}
