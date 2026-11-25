import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { existsSync } from "node:fs";
import { dirname } from "node:path";
import { PassThrough } from "node:stream";
import test from "node:test";
import { ClearraCommandRunner } from "../src/job-service/runner.mjs";
import { ClearraJobService } from "../src/job-service/server.mjs";
import { writeOperationalLog } from "../src/operational-log.mjs";

const config = { executable: "clearra", processLogicalProcessors: 8,
  searchWorkersPerSession: 8, useAllLogicalProcessors: true,
  searchTimeoutMs: 5000, maxOutputBytes: 4096, maxArtifactBytes: 4096,
  terminationGraceMs: 100 };

function closedChild(code, signal, beforeClose = () => {}) {
  const child = new EventEmitter();
  child.stdout = new PassThrough(); child.stderr = new PassThrough();
  child.exitCode = null; child.signalCode = null; child.kill = () => true;
  queueMicrotask(() => {
    child.stdout.end("incomplete JSON from failed engine");
    child.stderr.end("private diagnostic input");
    beforeClose();
    child.exitCode = code; child.signalCode = signal;
    child.emit("close", code, signal);
  });
  return child;
}

test("nonzero and signal exits retain the engine cause instead of parsing a missing artifact", async () => {
  for (const [code, signal, expected] of [[2, null, "engine-exit"], [null, "SIGKILL", "engine-signal"], [0, "SIGTERM", "engine-signal"]]) {
    let directory;
    const runner = new ClearraCommandRunner(config, {
      readOomKillCount: async () => 7,
      spawn(_executable, arguments_) {
        directory = dirname(arguments_[arguments_.indexOf("--solution-output") + 1]);
        return closedChild(code, signal);
      },
    });
    const result = await runner.execute({ arguments: ["pc"],
      deadlineUnixMs: Date.now() + 5000, maxOutputBytes: 4096, maxArtifactBytes: 4096 });
    assert.equal(result.failureCode, expected);
    assert.equal(result.failureStage, "execute");
    assert.equal(result.exitCode, code ?? -1);
    assert.equal(result.signal, signal);
    assert.equal(result.stderr, "private diagnostic input");
    assert.equal(existsSync(directory), false);
    const logs = [];
    const service = new ClearraJobService({}, { execute: async () => result }, {
      operationalScope: "job", logger: { error(value) { logs.push(value); } },
    });
    const terminal = await service.runJob({ id: "failure", arguments: ["pc"],
      timeoutClass: "pc_reverse", timeoutLimitMs: 5000 }, {
      createdAt: Date.now(), controller: new AbortController(),
    });
    assert.equal(terminal.state, "failed");
    assert.equal(terminal.failureCode, expected);
    assert.equal(terminal.failureStage, "execute");
    assert.equal(JSON.parse(logs[0]).failureCode, expected);
    assert.equal(JSON.parse(logs[0]).failureStage, "execute");
    assert.equal(logs[0].includes("private diagnostic"), false);
  }
});

test("preparation time consumes the absolute deadline before the process is started", async () => {
  let now = 1000;
  let spawned = false;
  const runner = new ClearraCommandRunner(config, {
    now: () => now,
    readOomKillCount: async () => { now = 1101; return null; },
    spawn() { spawned = true; throw new Error("must not spawn after deadline"); },
  });
  await assert.rejects(runner.execute({ arguments: ["pc", "path"],
    deadlineUnixMs: 1100, maxOutputBytes: 4096, maxArtifactBytes: 4096 }), (error) => {
    assert.equal(error.name, "DeadlineExceededError");
    assert.equal(error.failureStage, "prepare");
    assert.equal(error.stageDurationsMs.prepare, 101);
    return true;
  });
  assert.equal(spawned, false);
});

test("post-close diagnostics cannot publish a result after the original deadline", async () => {
  let now = 1000;
  let reads = 0;
  const runner = new ClearraCommandRunner(config, {
    now: () => now,
    readOomKillCount: async () => { if (reads++ > 0) now = 1101; return 7; },
    spawn() { return closedChild(null, "SIGKILL", () => { now = 1005; }); },
  });
  await assert.rejects(runner.execute({ arguments: ["pc", "path"],
    deadlineUnixMs: 1100, maxOutputBytes: 4096, maxArtifactBytes: 4096 }), (error) => {
    assert.equal(error.name, "DeadlineExceededError");
    assert.equal(error.failureStage, "artifact");
    assert.equal(error.stageDurationsMs.execute, 5);
    assert.equal(error.stageDurationsMs.artifact, 96);
    return true;
  });
});

test("phase diagnostics keep only bounded known metadata", () => {
  const lines = [];
  writeOperationalLog({ error(value) { lines.push(value); } }, {
    scope: "job", kind: "search", command: "pc", status: "failed", durationMs: 40,
    failureCode: "engine-oom", failureStage: "artifact", exitCode: -1, signal: "SIGKILL",
    stageDurationsMs: { prepare: 1, execute: 30, artifact: 9, token: 100, delivery: -1 },
    stderr: "private", argv: ["private"],
  });
  const record = JSON.parse(lines[0]);
  assert.deepEqual(record.stageDurationsMs, { prepare: 1, execute: 30, artifact: 9 });
  assert.equal(record.failureStage, "artifact");
  assert.equal(record.signal, "SIGKILL");
  assert.equal(lines[0].includes("private"), false);
});

test("an output limit failure survives process draining and terminal diagnostics", async () => {
  let closed = false;
  const signals = [];
  const runner = new ClearraCommandRunner({ ...config, maxOutputBytes: 16 }, {
    readOomKillCount: async () => null,
    spawn() {
      const child = new EventEmitter();
      child.stdout = new PassThrough(); child.stderr = new PassThrough();
      child.exitCode = null; child.signalCode = null;
      child.kill = (signal) => {
        signals.push(signal);
        queueMicrotask(() => {
          closed = true;
          child.signalCode = signal;
          child.emit("close", null, signal);
        });
        return true;
      };
      queueMicrotask(() => child.stdout.end("private output exceeds the configured limit"));
      return child;
    },
  });
  const logs = [];
  const service = new ClearraJobService({}, runner, {
    operationalScope: "job", logger: { error(value) { logs.push(value); } },
  });
  const terminal = await service.runJob({ id: "output-limit", arguments: ["pc", "path"],
    timeoutClass: "pc_reverse", timeoutLimitMs: 5000,
    deadlineUnixMs: Date.now() + 5000, maxOutputBytes: 16, maxArtifactBytes: 4096 }, {
    createdAt: Date.now(), controller: new AbortController(),
  });
  assert.equal(closed, true);
  assert.deepEqual(signals, ["SIGTERM"]);
  assert.equal(terminal.state, "failed");
  assert.equal(terminal.failureCode, "output-limit");
  assert.equal(terminal.failureStage, "execute");
  assert.equal(JSON.parse(logs[0]).failureCode, "output-limit");
  assert.equal(logs[0].includes("private output"), false);
});
