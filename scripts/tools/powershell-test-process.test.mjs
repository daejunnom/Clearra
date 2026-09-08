import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  POWERSHELL_TEST_TIMEOUT_MS, powerShellTestOptions, runPowerShellTest,
} from "./powershell-test-process.mjs";

const passed = () => ({ status: 0, signal: null, stdout: "7\n", stderr: "" });
const failed = (code) => ({ error: { code }, status: null, signal: null });

test("PowerShell fixtures use a bounded shell-free process with stdin closed", () => {
  const calls = [];
  const result = runPowerShellTest(["-File", "fixture with spaces.ps1"], {
    cwd: "fixture-repository",
    spawnImplementation(...args) { calls.push(args); return passed(); },
  });
  assert.deepEqual(result, passed());
  assert.equal(calls.length, 1);
  const [command, args, options] = calls[0];
  assert.equal(command, process.platform === "win32" ? "pwsh.exe" : "pwsh");
  assert.deepEqual(args, ["-NoLogo", "-NoProfile", "-NonInteractive", "-File", "fixture with spaces.ps1"]);
  assert.equal(options.cwd, "fixture-repository");
  assert.equal(options.shell, false);
  assert.equal(options.windowsHide, true);
  assert.deepEqual(options.stdio, ["ignore", "pipe", "pipe"]);
  assert.equal(options.timeout, 60_000);
  assert.equal(options.timeout, POWERSHELL_TEST_TIMEOUT_MS);
  assert.equal(options.killSignal, "SIGKILL");
});

test("expected PowerShell assertion failures retain their exit code and output", () => {
  const failure = { status: 1, signal: null, stdout: "", stderr: "fixture-denial" };
  assert.equal(runPowerShellTest(["-File", "fixture.ps1"], {
    spawnImplementation: () => failure,
  }), failure);
});

test("process failures expose bounded diagnostics without echoing invocation data or retrying", () => {
  for (const code of ["ETIMEDOUT", "ENOENT", "EACCES"]) {
    let calls = 0;
    assert.throws(() => runPowerShellTest(["-Command", "fixture-private-argument"], {
      spawnImplementation() { calls += 1; return failed(code); },
    }), (error) => {
      assert.equal(error.code, code);
      assert.match(error.message, /exit_code=null signal=null timeout_ms=60000/u);
      assert.ok(error.message.includes(`code=${code}`));
      assert.doesNotMatch(error.message, /fixture-private-argument/u);
      return true;
    });
    assert.equal(calls, 1);
  }
  assert.throws(() => runPowerShellTest([], {
    spawnImplementation: () => ({ status: null, signal: "SIGTERM" }),
  }), /code=PROCESS_TERMINATED exit_code=null signal=SIGTERM/u);
});

test("only a missing local PowerShell binary is skippable, never CI failures or local timeouts", () => {
  assert.deepEqual(powerShellTestOptions({ ci: "true", spawnImplementation: passed }), { skip: false });
  assert.equal(typeof powerShellTestOptions({ ci: "false", spawnImplementation: () => failed("ENOENT") }).skip, "string");
  for (const ci of ["true", "false"]) {
    for (const code of ["ETIMEDOUT", "EACCES"]) {
      assert.throws(() => powerShellTestOptions({ ci, spawnImplementation: () => failed(code) }), { code });
    }
    for (const result of [{ ...passed(), status: 1 }, { ...passed(), stdout: "" }]) {
      assert.throws(() => powerShellTestOptions({ ci, spawnImplementation: () => result }), /behavior probe failed/u);
    }
  }
  assert.throws(() => powerShellTestOptions({ ci: "true", spawnImplementation: () => failed("ENOENT") }), { code: "ENOENT" });
});

test("release static validation follows the Oracle delegation and rejects weakened process boundaries", () => {
  // Execute the real PowerShell validator against in-memory mutations. Do not
  // patch the checkout or launch the full product gate from this focused test.
  const script = String.raw`
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile(
    (Join-Path $PWD 'scripts/architecture/validate_release_static_contract.ps1'),
    [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count -ne 0) { throw 'release validator has syntax errors' }
$definition = $ast.Find({ param($node)
    $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
    $node.Name -ceq 'Assert-ReleasePowerShellRegressionBoundary'
}, $false)
if ($null -eq $definition) { throw 'release process boundary validator is missing' }
. ([scriptblock]::Create($definition.Extent.Text))
$gate = $ast.Find({ param($node)
    $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
    $node.Name -ceq 'Invoke-ReleaseIdentityGateValidation'
}, $false)
$calls = @($gate.Body.FindAll({ param($node)
    $node -is [System.Management.Automation.Language.CommandAst] -and
    $node.GetCommandName() -ceq 'Assert-ReleasePowerShellRegressionBoundary'
}, $true))
if ($calls.Count -ne 1 -or $calls[0].Extent.Text -cne
    'Assert-ReleasePowerShellRegressionBoundary -Caller $oracleDeployInvokerNodeTest -Runner $powerShellTestProcess') {
    throw 'release identity gate must execute the boundary exactly once with both owners'
}
$script:boundaryErrors = [System.Collections.Generic.List[string]]::new()
function Add-ArchitectureError([string]$Message) { $script:boundaryErrors.Add($Message) }
$caller = [IO.File]::ReadAllText((Join-Path $PWD 'scripts/release/oracle/invoke-release-deploy-v080.test.mjs'))
$runner = [IO.File]::ReadAllText((Join-Path $PWD 'scripts/tools/powershell-test-process.mjs'))
Assert-ReleasePowerShellRegressionBoundary -Caller $caller -Runner $runner
if ($script:boundaryErrors.Count -ne 0) { throw ($script:boundaryErrors -join [Environment]::NewLine) }
$mutations = @(
    @{ Owner = 'Caller'; From = '../../tools/powershell-test-process.mjs'; To = '../../tools/other-runner.mjs' },
    @{ Owner = 'Caller'; From = 'runPowerShellTest(["-File", TEST_SCRIPT], { cwd: REPOSITORY_ROOT })'; To = 'skippedFixture()' },
    @{ Owner = 'Caller'; From = 'oracle_release_deploy_wrapper_test=pass'; To = 'unchecked-marker' },
    @{ Owner = 'Caller'; From = 'result.stdout.match('; To = 'uncheckedOutput(' },
    @{ Owner = 'Runner'; From = 'import { spawnSync } from "node:child_process";'; To = '' },
    @{ Owner = 'Runner'; From = 'export function runPowerShellTest(args, {'; To = 'function unusedRunner(args, {' },
    @{ Owner = 'Runner'; From = 'spawnImplementation = spawnSync'; To = 'spawnImplementation = fakeSuccess' },
    @{ Owner = 'Runner'; From = 'const result = spawnImplementation('; To = 'const result = fakeSuccess(' },
    @{ Owner = 'Runner'; From = '"-NoProfile", '; To = '' },
    @{ Owner = 'Runner'; From = 'shell: false'; To = 'shell: true' },
    @{ Owner = 'Runner'; From = 'stdio: ["ignore", "pipe", "pipe"]'; To = 'stdio: "inherit"' },
    @{ Owner = 'Runner'; From = 'windowsHide: true'; To = 'windowsHide: false' },
    @{ Owner = 'Runner'; From = 'POWERSHELL_TEST_TIMEOUT_MS = 60_000'; To = 'POWERSHELL_TEST_TIMEOUT_MS = 0' },
    @{ Owner = 'Runner'; From = 'timeout: POWERSHELL_TEST_TIMEOUT_MS'; To = 'timeout: 0' },
    @{ Owner = 'Runner'; From = 'killSignal: "SIGKILL"'; To = '' },
    @{ Owner = 'Runner'; From = 'if (result.error || result.status === null || result.signal)'; To = 'if (false)' },
    @{ Owner = 'Runner'; From = 'throw error;'; To = 'return fakeSuccess();' },
    @{ Owner = 'Runner'; From = 'return result;'; To = 'return fakeSuccess();' }
)
foreach ($mutation in $mutations) {
    $script:boundaryErrors.Clear()
    $inputs = @{ Caller = $caller; Runner = $runner }
    $before = $inputs[$mutation.Owner]
    $inputs[$mutation.Owner] = $before.Replace($mutation.From, $mutation.To)
    if ($inputs[$mutation.Owner] -ceq $before) { throw 'boundary mutation was not applied' }
    Assert-ReleasePowerShellRegressionBoundary @inputs
    if ($script:boundaryErrors.Count -eq 0) { throw "weakened boundary accepted: $($mutation.From)" }
}
[Console]::WriteLine("powershell_regression_boundary=passed mutations=$($mutations.Count)")
`;
  const result = runPowerShellTest(["-EncodedCommand", Buffer.from(script, "utf16le").toString("base64")], {
    cwd: fileURLToPath(new URL("../../", import.meta.url)),
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stderr, "");
  assert.equal(result.stdout.trim(), "powershell_regression_boundary=passed mutations=18");
});
