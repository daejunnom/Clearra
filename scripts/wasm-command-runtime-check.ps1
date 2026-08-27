param(
    [string]$CargoPath = "cargo",
    [string]$NpmPath = "",
    [int]$Workers = 1,
    [string]$ExecutionSurface = "",
    [switch]$VerboseLog
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ([string]::IsNullOrWhiteSpace($NpmPath)) {
    $NpmPath = if ($env:OS -eq "Windows_NT") { "npm.cmd" } else { "npm" }
}

$Root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
. (Join-Path $PSScriptRoot "lib/progress.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-path-helpers.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-execution-surface.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-application-control.ps1")
Assert-ClearraTrustedExecutionSurface $ExecutionSurface "WASM command runtime"

$previousCargoTargetDir = $env:CARGO_TARGET_DIR
$cargoTargetDir = if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
    Get-ClearraCargoTargetDir
} else {
    Assert-ClearraCanonicalCargoTargetDir $previousCargoTargetDir
}
$env:CARGO_TARGET_DIR = $cargoTargetDir
$wasmArtifact = Join-Path $cargoTargetDir "wasm32-unknown-unknown/release/clearra_wasm.wasm"
$webWasmDir = Join-Path $Root "apps/clearra-web/static/wasm"
$wasmBindings = Join-Path $webWasmDir "clearra_wasm.js"
$boundWasmArtifact = Join-Path $webWasmDir "clearra_wasm_bg.wasm"

function Invoke-CheckedCommand {
    param(
        [Parameter(Mandatory)]
        [string]$Label,

        [Parameter(Mandatory)]
        [string]$FileName,

        [Parameter(Mandatory)]
        [string[]]$Arguments
    )

    Invoke-ClearraProgressCase `
        -Scope $script:Scope `
        -Name $Label `
        -Body {
            $result = Invoke-NativeWithProgress `
                -Scope $script:Scope `
                -Label $Label `
                -FileName $FileName `
                -Arguments $Arguments

            if ($VerboseLog.IsPresent -and -not [string]::IsNullOrWhiteSpace($result.Output)) {
                Complete-ClearraProgressLine $script:Scope
                Write-Output $result.Output
            }

            if ($result.ExitCode -ne 0) {
                if (Test-ClearraApplicationControlBlockOutput $result.Output) {
                    throw (New-ClearraLocalSourceBuildBlockedMessage $Label (Get-ClearraApplicationControlStatus))
                }
                throw "$Label failed with exit $($result.ExitCode)`n$($result.Output)"
            }
        }
}

$script:Scope = New-ClearraProgressScope `
    -Name "wasm-command" `
    -Total 9 `
    -Workers ([Math]::Max(1, $Workers)) `
    -VerboseLog:$VerboseLog.IsPresent

Push-Location $Root
try {
    Invoke-CheckedCommand `
        -Label "cargo check -p clearra-web-command" `
        -FileName $CargoPath `
        -Arguments @("check", "-p", "clearra-web-command")

    Invoke-CheckedCommand `
        -Label "cargo check -p clearra-wasm" `
        -FileName $CargoPath `
        -Arguments @("check", "-p", "clearra-wasm")

    Invoke-CheckedCommand `
        -Label "cargo check wasm32 clearra-wasm-abi" `
        -FileName $CargoPath `
        -Arguments @("check", "--target", "wasm32-unknown-unknown", "-p", "clearra-wasm-abi")

    Invoke-CheckedCommand `
        -Label "cargo test clearra-wasm host contract" `
        -FileName $CargoPath `
        -Arguments @("test", "-p", "clearra-wasm", "--test", "wasm_host_contract")
    $hostContractMode = "launched"

    Invoke-CheckedCommand `
        -Label "cargo build wasm32 clearra-wasm-abi" `
        -FileName $CargoPath `
        -Arguments @(
            "build", "--target", "wasm32-unknown-unknown", "--release",
            "-p", "clearra-wasm-abi"
        )
    if (-not (Test-Path -LiteralPath $wasmArtifact -PathType Leaf) -or
        (Get-Item -LiteralPath $wasmArtifact).Length -le 0) {
        throw "WASM command runtime artifact is missing or empty: $wasmArtifact"
    }

    $nodeName = if ($env:OS -eq "Windows_NT") { "node.exe" } else { "node" }
    $nodeCommand = Get-Command $nodeName -ErrorAction SilentlyContinue
    if ($null -eq $nodeCommand) {
        throw "WASM command runtime requires Node.js on PATH"
    }
    Invoke-CheckedCommand `
        -Label "stage wasm-bindgen web runtime" `
        -FileName $nodeCommand.Source `
        -Arguments @(
            (Join-Path $Root "scripts/tools/stage-clearra-wasm.mjs"),
            $webWasmDir
        )
    foreach ($artifact in @($wasmBindings, $boundWasmArtifact)) {
        if (-not (Test-Path -LiteralPath $artifact -PathType Leaf) -or
            (Get-Item -LiteralPath $artifact).Length -le 0) {
            throw "Bound WASM command runtime artifact is missing or empty: $artifact"
        }
    }
    $nodeProbe = Join-Path $cargoTargetDir "wasm-command-cancellation-probe.mjs"
    $nodeProbeSource = @'
import fs from 'node:fs';
import { pathToFileURL } from 'node:url';

const [bindingsPath, wasmPath] = process.argv.slice(2);
const bindings = await import(pathToFileURL(bindingsPath).href);
const wasm = await bindings.default({ module_or_path: fs.readFileSync(wasmPath) });
const readOutput = () => {
  try {
    return new TextDecoder().decode(new Uint8Array(
      wasm.memory.buffer,
      wasm.clearra_wasm_output_ptr(),
      wasm.clearra_wasm_output_len(),
    ));
  } finally {
    wasm.clearra_wasm_output_release();
  }
};
const command = new TextEncoder().encode(
  'clearra pc --lines 6 --backend cpu --queue IOTSZJLIOTSZJLI',
);
if (wasm.clearra_wasm_input_resize(command.byteLength) !== 0) throw new Error(readOutput());
new Uint8Array(wasm.memory.buffer, wasm.clearra_wasm_input_ptr(), command.byteLength).set(command);
const jobId = wasm.clearra_wasm_start_job();
if (jobId === 0) throw new Error(readOutput());
const first = wasm.clearra_wasm_advance_job(jobId, 64);
const second = wasm.clearra_wasm_advance_job(jobId, 1);
if (first !== 0 || second !== 0) {
  throw new Error(`6L search did not yield: ${first}/${second}`);
}
if (wasm.clearra_wasm_cancel_job(jobId) !== 0) throw new Error(readOutput());
if (wasm.clearra_wasm_drain_job_events(jobId) !== 0) throw new Error(readOutput());
const events = JSON.parse(readOutput());
const cancelled = events.find((event) => event.event === 'cancelled');
if (!cancelled || cancelled.scope_released !== true) {
  throw new Error('cancelled computation scope was not released');
}
if (events.some((event) => event.event === 'final_response')) {
  throw new Error('cancelled job emitted a final response');
}
console.log('wasm_exact_execution=launched cancellation=cooperative scope_released=true final_response=false');
'@
    [System.IO.File]::WriteAllText($nodeProbe, $nodeProbeSource, [System.Text.UTF8Encoding]::new($false))
    try {
        Invoke-CheckedCommand `
            -Label "Node WASM cooperative cancellation E2E" `
            -FileName $nodeCommand.Source `
            -Arguments @(
                $nodeProbe,
                $wasmBindings,
                $boundWasmArtifact
            )
    } finally {
        Remove-Item -LiteralPath $nodeProbe -Force -ErrorAction SilentlyContinue
    }

    Invoke-CheckedCommand `
        -Label "npm build @clearra/web" `
        -FileName $NpmPath `
        -Arguments @("exec", "--workspace", "@clearra/web", "--", "vite", "build")

    Invoke-CheckedCommand `
        -Label "architecture U7 WASM Command Runtime" `
        -FileName "powershell" `
        -Arguments @(
            "-NoProfile",
            "-File", "scripts/validate_architecture.ps1",
            "-TaskName", "U7 WASM Command Runtime"
        )

    Complete-ClearraProgressLine $script:Scope
    Write-Output "[wasm-command] passed | wasm_target=compiled | host_contract_tests=$hostContractMode | wasm_exact_execution=launched | browser_bundle=built | architecture_validation=passed | wsl_used=false"
}
finally {
    Pop-Location
    if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
        Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    } else {
        $env:CARGO_TARGET_DIR = $previousCargoTargetDir
    }
}
