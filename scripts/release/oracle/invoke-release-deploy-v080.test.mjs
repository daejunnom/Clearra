import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const TEST_SCRIPT = fileURLToPath(
  new URL("./invoke-release-deploy-v080.test.ps1", import.meta.url),
);
const REPOSITORY_ROOT = resolve(
  fileURLToPath(new URL("../../..", import.meta.url)),
);

test(
  "executes the shell-free Oracle prestage transport and watchdog regression exactly once",
  { timeout: 120_000 },
  () => {
    const result = spawnSync(
      process.platform === "win32" ? "pwsh.exe" : "pwsh",
      ["-NoLogo", "-NoProfile", "-NonInteractive", "-File", TEST_SCRIPT],
      {
        cwd: REPOSITORY_ROOT,
        encoding: "utf8",
        maxBuffer: 8 * 1024 * 1024,
        shell: false,
        stdio: ["ignore", "pipe", "pipe"],
        windowsHide: true,
      },
    );
    assert.ifError(result.error);
    assert.equal(
      result.status,
      0,
      `PowerShell regression failed.\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
    );
    assert.equal(result.signal, null);
    assert.equal(result.stderr, "");
    assert.equal(result.stdout.trim(), "oracle_release_deploy_wrapper_test=pass");
    assert.equal(
      result.stdout.match(/oracle_release_deploy_wrapper_test=pass/gu)?.length,
      1,
    );
  },
);
