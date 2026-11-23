import assert from "node:assert/strict";
import test from "node:test";
import { runBoundedCommand } from "./bounded-command.mjs";

test("external command deadlines and cancellation finish after terminating the child", async () => {
  const command = [
    "-e", 'process.on("SIGTERM", () => {}); setInterval(() => {}, 1000);',
  ];
  const started = performance.now();
  await assert.rejects(runBoundedCommand(process.execPath, command, {
    timeoutMs: 250, killGraceMs: 50, maxBytes: 4096, label: "stalled child",
  }), /timed out/u);
  assert.ok(performance.now() - started < 5_000);
  const controller = new AbortController();
  const pending = runBoundedCommand(process.execPath, command, {
    timeoutMs: 10_000, killGraceMs: 50, maxBytes: 4096, label: "cancelled child",
    signal: controller.signal,
  });
  controller.abort();
  await assert.rejects(pending, /was cancelled/u);
  assert.equal(process.listenerCount("SIGTERM"), 0);
  assert.equal(process.listenerCount("SIGINT"), 0);
});

test("an absent POSIX group is never signalled again while output drainage is pending", {
  skip: process.platform === "win32",
  timeout: 5_000,
}, async (t) => {
  const attempts = [];
  const actualKill = process.kill.bind(process);
  t.mock.method(process, "kill", (pid, signal) => {
    if (pid >= 0) return actualKill(pid, signal);
    attempts.push({ pid, signal });
    // Simulate the OS proving that our group is gone. A real, short-lived
    // child keeps the close event pending across the escalation timer.
    throw Object.assign(new Error("fixture: process group is absent"), { code: "ESRCH" });
  });
  await assert.rejects(runBoundedCommand(process.execPath, [
    "-e", "setTimeout(() => process.stdout.write('fixture finished'), 250);",
  ], {
    timeoutMs: 50, killGraceMs: 25, maxBytes: 4096, label: "absent group",
  }), /timed out/u);
  assert.deepEqual(attempts.map(({ signal }) => signal), ["SIGTERM"]);
  assert.ok(attempts[0].pid < 0);
  assert.equal(process.listenerCount("SIGTERM"), 0);
  assert.equal(process.listenerCount("SIGINT"), 0);
});

test("an uncertain POSIX termination still escalates and reaps the owned child", {
  skip: process.platform === "win32",
  timeout: 5_000,
}, async (t) => {
  const attempts = [];
  const actualKill = process.kill.bind(process);
  t.mock.method(process, "kill", (pid, signal) => {
    if (pid >= 0) return actualKill(pid, signal);
    attempts.push({ pid, signal });
    if (attempts.length === 1) {
      // An error other than ESRCH is not evidence that the group is gone.
      throw Object.assign(new Error("fixture: termination permission unknown"), { code: "EPERM" });
    }
    return actualKill(pid, signal);
  });
  await assert.rejects(runBoundedCommand(process.execPath, [
    "-e", "setTimeout(() => {}, 1000);",
  ], {
    timeoutMs: 100, killGraceMs: 25, maxBytes: 4096, label: "uncertain group",
  }), /timed out/u);
  assert.deepEqual(attempts.map(({ signal }) => signal), ["SIGTERM", "SIGKILL"]);
  assert.equal(attempts[0].pid, attempts[1].pid);
  assert.throws(() => actualKill(attempts[0].pid, 0), { code: "ESRCH" });
  assert.equal(process.listenerCount("SIGTERM"), 0);
  assert.equal(process.listenerCount("SIGINT"), 0);
});
