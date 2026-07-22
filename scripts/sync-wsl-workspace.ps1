[CmdletBinding()]
param(
    [string]$Distribution = 'Ubuntu',
    [switch]$AsJson
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
. (Join-Path $PSScriptRoot 'lib/clearra-path-helpers.ps1')
. (Join-Path $PSScriptRoot 'lib/clearra-artifact-cache.ps1')
. (Join-Path $PSScriptRoot 'lib/clearra-runtime-environment.ps1')

$result = Sync-ClearraWslExt4Workspace $Root $Distribution
if ($AsJson.IsPresent) {
    $result | ConvertTo-Json -Depth 4
} else {
    $result
}
