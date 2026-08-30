# Top-level task workflow dispatch. Individual gate implementations live in
# their owning library modules; this file contains only task composition.

function Invoke-StrictProductPathTask([string]$Root) {
    $verifyArgs = Build-VerifyArgs $Root $true $false
    & (Join-Path $Root "scripts/verify.ps1") @verifyArgs
    Invoke-CoreCTestStartMode `
        $Root `
        "StrictCOnlySplit" `
        (Get-StartTestsCMakeConfigureArgs @("-DCLEARRA_CORE_SPLIT_TESTS=ON")) `
        -PersistentBuildName "core-c-split-cache" `
        -AggregateOnly:$false
    Invoke-NativeLocalMode $Root
    Invoke-ProductE2EBuiltTask $Root

    $desktopHostArgs = @{
        Workers = $Workers
        ExecutionSurface = $ExecutionSurface
    }
    if ($VerboseLog.IsPresent) {
        $desktopHostArgs["VerboseLog"] = $true
    }
    & (Join-Path $Root "scripts/desktop-host-check.ps1") @desktopHostArgs
}

function Invoke-ClearraTask([string]$TaskName, [string]$Root) {
    switch ($TaskName) {
        "Quick" {
            Invoke-CoreCTestStartMode $Root "Quick" (Get-StartTestsCMakeConfigureArgs)
        }
        "COnly" {
            Invoke-CoreCTestStartMode $Root "COnly" (Get-StartTestsCMakeConfigureArgs)
        }
        "COnlySplit" {
            Invoke-CoreCTestStartMode `
                $Root `
                "COnlySplit" `
                (Get-StartTestsCMakeConfigureArgs @("-DCLEARRA_CORE_SPLIT_TESTS=ON")) `
                -PersistentBuildName "core-c-split-cache" `
                -AggregateOnly:(-not (Test-ClearraTrustedExecutionSurface $ExecutionSurface))
        }
        "COnlyAsan" {
            Invoke-CoreCTestStartMode `
                $Root "COnlyAsan" `
                (Get-StartTestsCMakeConfigureArgs @("-DCLEARRA_CORE_ENABLE_ASAN=ON")) `
                -PersistentBuildName "core-c-asan-cache"
        }
        "COnlyUbsan" {
            Invoke-CoreCTestStartMode `
                $Root "COnlyUbsan" `
                (Get-StartTestsCMakeConfigureArgs @("-DCLEARRA_CORE_ENABLE_UBSAN=ON")) `
                -PersistentBuildName "core-c-ubsan-cache"
        }
        "UXSmoke" {
            $uxSmokeArgs = @{ OutputExcerptLines = $OutputExcerptLines }
            if ($VerboseLog.IsPresent) {
                $uxSmokeArgs["VerboseLog"] = $true
            }
            $releaseAcceptance = Get-Variable `
                -Name ClearraReleaseAcceptanceMode `
                -Scope Script `
                -ErrorAction SilentlyContinue
            if ($null -ne $releaseAcceptance -and [bool]$releaseAcceptance.Value) {
                Set-ClearraReleaseUxSmokeBinaryArgs $uxSmokeArgs $Root
            }
            & (Join-Path $Root "scripts/ux-smoke.ps1") @uxSmokeArgs
        }
        "DesktopHost" {
            $desktopHostArgs = @{
                Workers = $Workers
                ExecutionSurface = $ExecutionSurface
            }
            $noProductDebtArchitecture = Get-Variable `
                -Name ClearraNoProductDebtArchitecturePassed `
                -Scope Script `
                -ErrorAction SilentlyContinue
            if ($null -ne $noProductDebtArchitecture -and [bool]$noProductDebtArchitecture.Value) {
                $desktopHostArgs["ArchitectureValidatedByNoProductDebt"] = $true
            }
            if ($VerboseLog.IsPresent) {
                $desktopHostArgs["VerboseLog"] = $true
            }
            & (Join-Path $Root "scripts/desktop-host-check.ps1") @desktopHostArgs
        }
        "WorkerE2E" {
            $workerE2EArgs = @{
                Workers = $Workers
                OutputExcerptLines = $OutputExcerptLines
            }
            if ($VerboseLog.IsPresent) {
                $workerE2EArgs["VerboseLog"] = $true
            }
            & (Join-Path $Root "scripts/worker-e2e.ps1") @workerE2EArgs
        }
        "WorkerE2EStress" {
            $workerE2EArgs = @{
                Workers = $Workers
                OutputExcerptLines = $OutputExcerptLines
                Stress = $true
            }
            if ($VerboseLog.IsPresent) {
                $workerE2EArgs["VerboseLog"] = $true
            }
            & (Join-Path $Root "scripts/worker-e2e.ps1") @workerE2EArgs
        }
        "Acceptance" {
            foreach ($expandedTask in @(Expand-ClearraTasks @("Acceptance"))) {
                Invoke-ClearraTask $expandedTask $Root
            }
        }
        "ReleaseAcceptance" {
            foreach ($expandedTask in @(Expand-ClearraTasks @("ReleaseAcceptance"))) {
                Invoke-ClearraTask $expandedTask $Root
            }
        }
        "NoProductDebt" {
            Invoke-NoProductDebtGate `
                $Root `
                $CargoPath `
                $PowerShellPath `
                (Get-ClearraCargoTargetDir) `
                $Workers
            $script:ClearraNoProductDebtArchitecturePassed = $true
        }
        "AdversarialCorrectness" {
            Invoke-AdversarialCorrectnessGate `
                $Root `
                $CargoPath `
                $Workers `
                (Get-ClearraCargoTargetDir)
        }
        "CSanitizer" {
            Invoke-ClearraCSanitizerGate $Root
        }
        "RustExactTests" {
            Invoke-RustExactTestsGate `
                $Root `
                $CargoPath `
                (Get-ClearraCargoTargetDir) `
                $Workers
        }
        "WasmBuildTest" {
            Invoke-WasmBuildTestGate `
                $Root `
                $CargoPath `
                (Get-ClearraCargoTargetDir)
        }
        "RenderGolden" {
            Invoke-RenderGoldenGate `
                $CargoPath `
                (Get-ClearraCargoTargetDir)
        }
        "WorkerAcceptance" {
            foreach ($expandedTask in @(Expand-ClearraTasks @("WorkerAcceptance"))) {
                Invoke-ClearraTask $expandedTask $Root
            }
        }
        "WorkerRelease" {
            foreach ($expandedTask in @(Expand-ClearraTasks @("WorkerRelease"))) {
                Invoke-ClearraTask $expandedTask $Root
            }
        }
        "ProductE2E" {
            if (Test-ClearraTrustedExecutionSurface $ExecutionSurface) {
                Invoke-ProductE2EBuiltTask $Root
            } else {
                Invoke-ProductLibraryContractCheck `
                    $Root
            }
        }
        "ProductE2EBuilt" {
            Invoke-ProductE2EBuiltTask $Root
        }
        "GpuWorkerAcceptance" {
            Invoke-GpuWorkerAcceptanceTask $Root
        }
        "GpuWorkerNative" {
            Invoke-GpuWorkerNativeTask $Root
        }
        "GpuWorkerRelease" {
            foreach ($expandedTask in @(Expand-ClearraTasks @("GpuWorkerRelease"))) {
                Invoke-ClearraTask $expandedTask $Root
            }
        }
        "Mvp2Acceptance" {
            $mvp2AcceptanceArgs = @{
                Workers = $Workers
                OutputExcerptLines = $OutputExcerptLines
                CargoPath = $CargoPath
            }
            if ($VerboseLog.IsPresent) {
                $mvp2AcceptanceArgs["VerboseLog"] = $true
            }
            if ($ShowWarnings.IsPresent) {
                $mvp2AcceptanceArgs["ShowWarnings"] = $true
            }
            & (Join-Path $Root "scripts/mvp2-acceptance.ps1") @mvp2AcceptanceArgs
        }
        "Mvp3Acceptance" {
            $mvp3AcceptanceArgs = @{
                Workers = $Workers
                OutputExcerptLines = $OutputExcerptLines
                CargoPath = $CargoPath
            }
            if ($VerboseLog.IsPresent) {
                $mvp3AcceptanceArgs["VerboseLog"] = $true
            }
            if ($ShowWarnings.IsPresent) {
                $mvp3AcceptanceArgs["ShowWarnings"] = $true
            }
            & (Join-Path $Root "scripts/mvp3-acceptance.ps1") @mvp3AcceptanceArgs
        }
        "Local" {
            $verifyArgs = Build-VerifyArgs $Root $false $false
            & (Join-Path $Root "scripts/verify.ps1") @verifyArgs
        }
        "Strict" {
            Invoke-StrictProductPathTask $Root
        }
        "Security" {
            Invoke-RunnerSecurityTask $Root
        }
        "SecurityFull" {
            $verifyArgs = Build-VerifyArgs $Root $false $true
            & (Join-Path $Root "scripts/verify.ps1") @verifyArgs
        }
        "NativeLocal" {
            Invoke-NativeLocalMode $Root
        }
        "Validate" {
            Invoke-ClearraValidationTask $Root
        }
        "DiagnoseCArtifacts" {
            Invoke-DiagnoseCArtifactsTask $Root
        }
        "Events" {
            Invoke-CollectWindowsBlockEventsTask $Root
        }
        "CollectWindowsBlockEvents" {
            Invoke-CollectWindowsBlockEventsTask $Root
        }
        default {
            throw "Unknown Clearra task '$TaskName'"
        }
    }
}
