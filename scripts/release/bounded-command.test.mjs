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
