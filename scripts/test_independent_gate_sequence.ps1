$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'lib/independent-gate-sequence.ps1')

# Mocks deliberately never compile or launch product code.
function Invoke-ClearraProgressCase {
    param($Scope, [string]$Name, [switch]$PreserveOutput, [scriptblock]$Body)
    & $Body
}
function Complete-ClearraProgressLine { param($Scope) }
function New-TestScope { return @{ Name = 'mock'; Pending = 0; Running = 0 } }
function Assert-Test([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
function New-Stage([string]$Name, [scriptblock]$Body, [string[]]$Requires = @()) {
    return @{ Name = $Name; Body = $Body; Requires = $Requires }
}

$script:visited = New-Object 'System.Collections.Generic.List[string]'
$output = New-Object 'System.Collections.Generic.List[string]'
$errorText = ''
try {
    Invoke-ClearraIndependentGateSequence -Scope (New-TestScope) -Stages @(
        (New-Stage RustExactTests { $script:visited.Add('rust'); throw "rust failure`nfull detail" }),
        (New-Stage ProductE2E { $script:visited.Add('product'); throw 'product failure' }),
        (New-Stage RenderGolden { $script:visited.Add('render') })
    ) | ForEach-Object { $output.Add([string]$_) }
} catch { $errorText = $_.Exception.Message }
Assert-Test (($script:visited -join ',') -eq 'rust,product,render') 'Independent stages did not all execute once.'
Assert-Test ($errorText.Contains('rust failure') -and $errorText.Contains('product failure')) 'Final failure lost one of the stage failures.'
Assert-Test (($output -join "`n").Contains('stage=RenderGolden status=passed')) 'Later passing stage is missing from diagnostic output.'
Assert-Test (-not ($output -join "`n").Contains('stage=RustExactTests status=passed')) 'Failed gate was promoted to passed.'
Write-Output 'independent_gate_test=all-independent-failures status=passed'

$script:visited.Clear()
$errorText = ''
try {
    Invoke-ClearraIndependentGateSequence -Scope (New-TestScope) -Stages @(
        (New-Stage Build { throw 'build failure' }),
        (New-Stage Consumer { $script:visited.Add('unsafe-consumer') } @('Build')),
        (New-Stage Receipt { $script:visited.Add('unsafe-receipt') } @('Consumer')),
        (New-Stage SourceCheck { $script:visited.Add('source') })
    ) | Out-Null
} catch { $errorText = $_.Exception.Message }
Assert-Test (($script:visited -join ',') -eq 'source') 'Failed artifact consumer or transitive receipt ran.'
Assert-Test ($errorText.Contains('Consumer: blocked by Build') -and $errorText.Contains('Receipt: blocked by Consumer')) 'Blocked dependencies were not recorded.'
Write-Output 'independent_gate_test=failed-artifact-and-transitive-consumer-blocked status=passed'

foreach ($invalid in @(
    @((New-Stage A {} @('Missing'))),
    @((New-Stage A {}), (New-Stage A {})),
    @((New-Stage A {} @('B')), (New-Stage B {}))
)) {
    $rejected = $false
    try { Invoke-ClearraIndependentGateSequence -Scope (New-TestScope) -Stages $invalid | Out-Null }
    catch { $rejected = $true }
    Assert-Test $rejected 'Invalid gate dependency plan was accepted.'
}
Write-Output 'independent_gate_test=plan-validated-before-work status=passed'

$script:visited.Clear()
$cancelled = $false
try {
    Invoke-ClearraIndependentGateSequence -Scope (New-TestScope) -Stages @(
        (New-Stage Cancel { throw (New-Object System.OperationCanceledException) }),
        (New-Stage Later { $script:visited.Add('after-cancel') })
    ) | Out-Null
} catch [System.OperationCanceledException] { $cancelled = $true }
Assert-Test ($cancelled -and $script:visited.Count -eq 0) 'Cancellation started later work.'
Write-Output 'independent_gate_test=cancellation-stops-work status=passed'

$script:visited.Clear()
$success = @(Invoke-ClearraIndependentGateSequence -Scope (New-TestScope) -Stages @(
    (New-Stage Build { $script:visited.Add('build') }),
    (New-Stage Consumer { $script:visited.Add('consumer') } @('Build'))
))
Assert-Test (($script:visited -join ',') -eq 'build,consumer') 'Successful dependency order changed.'
Assert-Test (($success -join "`n").Contains('release_authority=false')) 'Diagnostic summary claimed release authority.'
Write-Output 'independent_gate_test=all-success-remains-diagnostic status=passed'

$entrypoint = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'clearra.ps1') -Raw
Assert-Test ($entrypoint.Contains('if ($collectReleaseAcceptanceFailures)')) 'Continuation escaped the canonical acceptance boundary.'
Assert-Test ($entrypoint.Contains('param([string]$gateTaskName) Invoke-ClearraTask $gateTaskName $Root')) 'Deferred gate body lost its explicit task identity.'
Write-Output 'independent_gate_test=canonical-entrypoint-contract status=passed'

# Exercise the actual deferred-plan source with only a mocked task dispatcher.
# This catches PowerShell closure/scope mistakes without starting a real gate.
$tasks = @('RustExactTests', 'ProductE2E', 'RenderGolden')
$Root = 'mock-root'
$script:visited.Clear()
function Invoke-ClearraTask([string]$TaskName, [string]$TaskRoot) {
    Assert-Test ($TaskRoot -eq 'mock-root') 'Deferred gate lost its source root.'
    $script:visited.Add($TaskName)
}
$planMatch = [regex]::Match($entrypoint, '(?s)(\$stages = @\(\$tasks \| ForEach-Object \{.*?\}\))\s*Invoke-ClearraIndependentGateSequence')
Assert-Test $planMatch.Success 'Canonical deferred plan source not found.'
. ([scriptblock]::Create($planMatch.Groups[1].Value))
Invoke-ClearraIndependentGateSequence -Stages $stages -Scope (New-TestScope) | Out-Null
Assert-Test (($script:visited -join ',') -eq ($tasks -join ',')) 'Canonical closure dispatched wrong stage identities.'
Write-Output 'independent_gate_test=actual-entrypoint-plan-with-mocked-dispatch status=passed'
