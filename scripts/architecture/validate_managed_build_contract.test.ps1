$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$source = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'validate_workspace_surface_base.ps1') -Raw
# Exercise only the build ownership rules, not unrelated product architecture.
$boundary = $source.IndexOf('$clearraRunner =')
if ($boundary -lt 0) { throw 'Managed build policy boundary missing' }
$policy = [scriptblock]::Create($source.Substring(0, $boundary))
$script:overrides = @{}
function Read-Text([string]$path) {
    if ($script:overrides.ContainsKey($path)) { return $script:overrides[$path] }
    return Get-Content -LiteralPath (Join-Path $Root $path) -Raw
}
function Add-ArchitectureError([string]$message) { $script:errors.Add($message) }
function Assert-Policy([bool]$fails) {
    $script:errors = [Collections.Generic.List[string]]::new()
    & $policy
    if (($script:errors.Count -gt 0) -ne $fails) { throw "Unexpected managed policy result: $($script:errors -join '; ')" }
}
Assert-Policy $false
$config = Read-Text '.cargo/config.toml'
$script:overrides['.cargo/config.toml'] = $config.Replace('clearra-build-root-required', 'rustc')
Assert-Policy $true
$script:overrides.Clear()
$cache = Read-Text 'scripts/lib/clearra-artifact-cache.ps1'
$script:overrides['scripts/lib/clearra-artifact-cache.ps1'] = $cache.Replace('Select-Object -Skip 5', 'Select-Object -Skip 50')
Assert-Policy $true
$script:overrides.Clear()
$helpers = Read-Text 'scripts/lib/clearra-path-helpers.ps1'
$script:overrides['scripts/lib/clearra-path-helpers.ps1'] = $helpers.Replace(
    "throw 'Repository-local legacy cleanup is forbidden during build initialization; use an explicit reviewed cleanup plan.'",
    "Remove-Item -LiteralPath 'not-executed' -Recurse")
Assert-Policy $true
'managed build architecture policy: 4 cases passed'
