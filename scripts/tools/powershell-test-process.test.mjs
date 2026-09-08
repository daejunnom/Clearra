import assert from "node:assert/strict";
import test from "node:test";
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
