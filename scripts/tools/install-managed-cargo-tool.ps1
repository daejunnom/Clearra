param(
    [ValidateSet('experiment', 'product')][string]$Purpose = 'experiment',
    [Parameter(Mandatory)][ValidateSet('wasm-bindgen-cli')][string]$ToolName,
    [Parameter(Mandatory)][ValidatePattern('^[0-9]+\.[0-9]+\.[0-9]+$')][string]$Version
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot '../lib/clearra-path-helpers.ps1')

$expected = if ($ToolName -eq 'wasm-bindgen-cli') { '0.2.126' } else { throw "Unsupported Cargo tool: $ToolName" }
if ($Version -ne $expected) {
    throw "Clearra requires $ToolName $expected; requested $Version"
}

$source = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$platform = if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
    'windows-x86_64'
} elseif ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq [Runtime.InteropServices.Architecture]::Arm64) {
    'linux-aarch64'
} else {
    'linux-x86_64'
}
$toolRoot = [IO.Path]::GetFullPath((Join-Path $source "build/tools/cargo/$ToolName/$Version/$platform"))
$managedPrefix = [IO.Path]::GetFullPath((Join-Path $source 'build/tools/cargo')).TrimEnd('\', '/') +
    [IO.Path]::DirectorySeparatorChar
$comparison = if ($platform.StartsWith('windows')) {
    [StringComparison]::OrdinalIgnoreCase
} else {
    [StringComparison]::Ordinal
}
if (-not $toolRoot.StartsWith($managedPrefix, $comparison)) {
    throw "Managed Cargo tool escaped the repository build/tools root: $toolRoot"
}
Assert-ClearraNoReparseBuildPath $toolRoot | Out-Null
$bin = Join-Path $toolRoot 'bin'
$executable = Join-Path $bin $(if ($ToolName -eq 'wasm-bindgen-cli') {
    if ($platform.StartsWith('windows')) { 'wasm-bindgen.exe' } else { 'wasm-bindgen' }
})
$expectedVersion = "wasm-bindgen $Version"

$lockRoot = [IO.Path]::GetFullPath((Join-Path $source '_local/state/management/tool-locks'))
Assert-ClearraNoReparseBuildPath $lockRoot | Out-Null
New-Item -ItemType Directory -Force -Path $lockRoot | Out-Null
$lockPath = Join-Path $lockRoot "$ToolName-$Version-$platform.lock"
$lock = $null
$deadline = [DateTime]::UtcNow.AddMinutes(30)
do {
    try {
        $lock = [IO.File]::Open($lockPath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    } catch [IO.IOException] {
        if ([DateTime]::UtcNow -ge $deadline) { throw "Timed out waiting for managed Cargo tool: $ToolName $Version" }
        Start-Sleep -Milliseconds 250
    }
} while ($null -eq $lock)

try {
    $accepted = (Test-Path -LiteralPath $executable -PathType Leaf) -and
        ((& $executable --version) -eq $expectedVersion)
    if (-not $accepted) {
        New-Item -ItemType Directory -Force -Path $toolRoot | Out-Null
        $cargoOutput = @(& cargo '+1.98.1' install $ToolName --version $Version --locked --root $toolRoot 2>&1)
        $installExit = $LASTEXITCODE
        $cargoOutput | ForEach-Object { Write-Output $_ }
        if ($installExit -ne 0) {
            $blocked = ($cargoOutput -join "`n") -match '(?:4551|application control|애플리케이션 제어 정책)'
            $shared = if ($blocked -and $platform.StartsWith('windows') -and $env:GITHUB_ACTIONS -cne 'true') {
                Get-Command wasm-bindgen.exe -ErrorAction SilentlyContinue
            } else {
                $null
            }
            if ($null -eq $shared -or (& $shared.Source --version) -ne $expectedVersion) {
                exit $installExit
            }
            New-Item -ItemType Directory -Force -Path $bin | Out-Null
            $files = @()
            foreach ($name in @('wasm-bindgen.exe', 'wasm-bindgen-test-runner.exe', 'wasm2es6js.exe')) {
                $candidate = Join-Path (Split-Path -Parent $shared.Source) $name
                if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
                    throw "Verified shared Cargo install is incomplete: $candidate"
                }
                $target = Join-Path $bin $name
                Copy-Item -LiteralPath $candidate -Destination $target -Force
                $item = Get-Item -LiteralPath $target
                $files += [ordered]@{
                    name = $name
                    sha256 = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant()
                    bytes = $item.Length
                }
            }
            [ordered]@{
                schema_id = 'clearra.managed-cargo-tool-bootstrap.v1'
                reason = 'windows-application-control-4551'
                version = $Version
                source_root = (Split-Path -Parent $shared.Source)
                files = $files
            } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $toolRoot 'install-provenance.json') -Encoding utf8
        }
        if ((& $executable --version) -ne $expectedVersion) {
            throw "Managed Cargo tool version readback failed: $executable"
        }
    }
    $env:PATH = "$bin$([IO.Path]::PathSeparator)$env:PATH"
    $env:WASM_BINDGEN = $executable
    if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_PATH)) {
        $bin | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
    }
    if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_ENV)) {
        "WASM_BINDGEN=$executable" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
    }
    Write-Output "managed_cargo_tool=$ToolName version=$Version path=$executable"
} finally {
    if ($null -ne $lock) { $lock.Dispose() }
}
