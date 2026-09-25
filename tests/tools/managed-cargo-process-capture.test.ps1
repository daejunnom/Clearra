$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$source = Join-Path $root 'scripts/tools/install-managed-cargo-tool.ps1'
$tokens = $null
$parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($source, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count -ne 0) { throw 'Managed Cargo installer has parse errors' }
$functions = @($ast.FindAll({
    param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst] -and
        $node.Name -eq 'Invoke-ClearraManagedToolProcess'
}, $true))
if ($functions.Count -ne 1) { throw 'Expected exactly one managed process capture function' }
# Load the real producer function, not an independent copy of its algorithm.
# Do not execute the surrounding installer or download any tools in this test.
. ([ScriptBlock]::Create($functions[0].Extent.Text))
$nativeShell = (Get-Process -Id $PID).Path

$LASTEXITCODE = 123
$success = Invoke-ClearraManagedToolProcess -FilePath $nativeShell -ArgumentList @(
    '-NoProfile', '-NonInteractive', '-Command',
    "[Console]::Error.WriteLine('install-progress'); [Console]::Out.WriteLine('installed'); exit 0"
)
if ($success.ExitCode -ne 0 -or $success.StandardOutput.Trim() -ne 'installed' -or
    $success.StandardError.Trim() -ne 'install-progress') {
    throw 'Normal stderr must not turn a successful native install into failure'
}
$failure = Invoke-ClearraManagedToolProcess -FilePath $nativeShell -ArgumentList @(
    '-NoProfile', '-NonInteractive', '-Command',
    "[Console]::Error.WriteLine('install-failed'); exit 17"
)
if ($failure.ExitCode -ne 17 -or $failure.StandardError.Trim() -ne 'install-failed') {
    throw 'The actual nonzero native exit code must be retained'
}
$threw = $false
try {
    Invoke-ClearraManagedToolProcess -FilePath (Join-Path $root '__nonexistent_managed_capture_test_executable__') -ArgumentList @('--version') | Out-Null
} catch {
    $threw = $true
}
if (-not $threw) { throw 'A failed process launch must fail closed' }
if ($ErrorActionPreference -ne 'Stop') { throw 'Native capture changed the caller error policy' }
Write-Output 'managed-cargo-process-capture: 4 behavioral checks passed'
