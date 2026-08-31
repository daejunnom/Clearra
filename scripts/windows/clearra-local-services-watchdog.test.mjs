import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createServer } from "node:net";
import { spawn } from "node:child_process";
import { mkdtemp, readFile as readFileCallback, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "..", "..");
const watcherPath = join(import.meta.dirname, "clearra-local-services-watchdog.ps1");
const installerPath = join(import.meta.dirname, "install-clearra-local-services-watchdog.ps1");
const launcherPath = join(import.meta.dirname, "launch-clearra-local-services-watchdog.vbs");

test("local-services watcher is one hidden 60-second owner for ports 4194 and 8790", async () => {
  const [watcher, installer, launcher] = await Promise.all([
    readFile(watcherPath, "utf8"),
    readFile(installerPath, "utf8"),
    readFile(launcherPath, "utf8"),
  ]);

  assert.match(watcher, /\[int\]\$PollSeconds = 60/u);
  assert.match(watcher, /\[int\]\$GuiPort = 4194/u);
  assert.match(watcher, /\[int\]\$TunnelPort = 8790/u);
  assert.match(watcher, /Local\\ClearraLocalServicesWatchdog-v2/u);
  assert.match(watcher, /Get-NetTCPConnection -State Listen -LocalPort \$Port/u);
  assert.match(watcher, /Test-OwnedProcessRunning/u);
  assert.match(watcher, /Test-ExistingGuiStartup/u);
  assert.match(watcher, /Test-ExistingTunnelStartup/u);
  assert.match(watcher, /WindowStyle = "Hidden"/u);
  assert.doesNotMatch(watcher, /cmd\.exe|npm\.cmd/iu);

  assert.match(launcher, /exitCode = shell\.Run\(command, 0, True\)/u);
  assert.match(launcher, /WScript\.Quit exitCode/u);
  assert.match(installer, /"Clearra Local Runtime", "Clearra Local Services Watchdog"/u);
  assert.match(installer, /-MultipleInstances IgnoreNew/u);
  assert.match(installer, /-Hidden/u);
  assert.match(installer, /-RestartCount 3/u);
  assert.match(installer, /-RestartInterval \(New-TimeSpan -Minutes 1\)/u);
  assert.match(installer, /clearra-local-services-watchdog\.json/u);
  assert.match(launcher, /-ConfigPath/u);
  assert.match(installer, /wscript\.exe/u);
  assert.doesNotMatch(installer, /Register-ScheduledTask[\s\S]*cmd\.exe/iu);
});

test("occupied GUI port is preserved without attempting a replacement process", { skip: process.platform !== "win32" }, async () => {
  const directory = await mkdtemp(join(tmpdir(), "clearra-watchdog-test-"));
  const logPath = join(directory, "watchdog.log");
  const server = createServer();
  await new Promise((resolveListen, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolveListen);
  });
  const address = server.address();
  assert.ok(address && typeof address === "object");

  try {
    const before = await listenerOwner(address.port);
    assert.equal(before, process.pid);
    const result = await runPowerShell([
      "-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
      "-File", watcherPath,
      "-Once", "-DisableTunnel",
      "-GuiPort", String(address.port),
      "-RepoRoot", root,
      "-NodePath", join(directory, "missing-node.exe"),
      "-NpmCliPath", join(directory, "missing-npm-cli.js"),
      "-EventLogPath", logPath,
    ]);
    assert.equal(result.code, 0, result.stderr);
    const log = await readFileCallback(logPath, "utf8");
    assert.match(log, new RegExp(`gui preserved: port=${address.port} already-in-use`, "u"));
    assert.doesNotMatch(log, /gui start requested|gui start failed/u);
    assert.equal(server.listening, true, "the occupied listener must remain active");
    assert.equal(await listenerOwner(address.port), before, "the listener PID must be preserved");
  } finally {
    await new Promise((resolveClose) => server.close(resolveClose));
    await rm(directory, { recursive: true, force: true });
  }
});

async function listenerOwner(port) {
  const result = await runPowerShell([
    "-NoLogo", "-NoProfile", "-NonInteractive", "-Command",
    `(Get-NetTCPConnection -State Listen -LocalPort ${port} | Select-Object -First 1 -ExpandProperty OwningProcess)`,
  ]);
  assert.equal(result.code, 0, result.stderr);
  return Number.parseInt(result.stdout.trim(), 10);
}

function runPowerShell(args) {
  return new Promise((resolveRun, reject) => {
    const child = spawn("powershell.exe", args, { windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.once("error", reject);
    child.once("close", (code) => resolveRun({ code, stdout, stderr }));
  });
}
