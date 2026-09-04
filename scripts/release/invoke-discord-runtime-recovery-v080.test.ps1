$ErrorActionPreference = 'Stop'
$source = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'invoke-discord-runtime-recovery-v080.ps1') -Raw
$match = [regex]::Match($source, '(?ms)^function Invoke-NodeExact \{.*?^\}')
if (-not $match.Success) { throw 'Invoke-NodeExact function was not found' }
Invoke-Expression $match.Value

function global:node {
    Write-Output 'validator_status=passed'
    $global:LASTEXITCODE = 0
}
$value = Invoke-NodeExact fixture.mjs verify
if ($null -ne $value) { throw 'validator stdout escaped into the PowerShell return pipeline' }

function global:node {
    Write-Output 'validator_status=failed'
    $global:LASTEXITCODE = 9
}
$threw = $false
try { Invoke-NodeExact fixture.mjs verify } catch { $threw = $_.Exception.Message -ceq 'tracked recovery validator failed' }
if (-not $threw) { throw 'nonzero validator status did not fail closed' }
Write-Output 'discord_runtime_recovery_invoke_node_exact=passed'
