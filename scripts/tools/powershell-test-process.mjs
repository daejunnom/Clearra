import { spawnSync } from "node:child_process";

// Hosted runners can spend over 20 seconds starting a cold PowerShell process
// while the release test pool is busy. This is a harness bound, not a product
// performance threshold. Keep a real subprocess timeout even for synchronous
// calls: node:test's timer cannot interrupt a blocked spawnSync.
export const POWERSHELL_TEST_TIMEOUT_MS = 60_000;

export function runPowerShellTest(args, {
  cwd,
  spawnImplementation = spawnSync,
} = {}) {
  const result = spawnImplementation(
    process.platform === "win32" ? "pwsh.exe" : "pwsh",
    ["-NoLogo", "-NoProfile", "-NonInteractive", ...args],
    {
      ...(cwd === undefined ? {} : { cwd }),
      encoding: "utf8",
      maxBuffer: 8 * 1024 * 1024,
      shell: false,
      // These fixtures take arguments/files, never interactive or piped input.
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
      timeout: POWERSHELL_TEST_TIMEOUT_MS,
      killSignal: "SIGKILL",
    },
  );
  if (result.error || result.status === null || result.signal) {
    const code = result.error?.code ?? "PROCESS_TERMINATED";
    const error = new Error(
      `PowerShell behavior process failed: code=${code} ` +
      `exit_code=${String(result.status)} signal=${String(result.signal)} ` +
      `timeout_ms=${POWERSHELL_TEST_TIMEOUT_MS}`,
    );
    error.code = code;
    throw error;
  }
  return result;
}

export function powerShellTestOptions({ ci = process.env.CI, ...options } = {}) {
  let probe;
  try {
    probe = runPowerShellTest(["-Command", "$PSVersionTable.PSVersion.Major"], options);
  } catch (error) {
    // A local machine may lack pwsh. Timeouts/crashes are failures everywhere,
    // and CI must execute the behavioral coverage, including denied paths.
    if (error.code === "ENOENT" && ci !== "true") {
      return { skip: "PowerShell is not installed in this local test environment" };
    }
    throw error;
  }
  if (probe.status !== 0 || !/^[1-9]\d*$/u.test(probe.stdout.trim())) {
    throw new Error(`PowerShell behavior probe failed: exit_code=${probe.status}`);
  }
  return { skip: false };
}
