import assert from "node:assert/strict";
import test from "node:test";

import { ClearraDirectExecutor } from "../src/clearra/direct-executor.mjs";
import { loadDiscordBotConfig } from "../src/config.mjs";

test("Discord Gateway startup fails closed on Cloud Run", () => {
  assert.throws(
    () => loadDiscordBotConfig({
      K_SERVICE: "stale-interaction-service",
      DISCORD_TOKEN: "test-token",
    }, { availableParallelism: () => 4 }),
    /must run on Oracle, not Cloud Run/,
  );
});

test("Oracle Gateway ingress requires a bot token", () => {
  assert.throws(
    () => loadDiscordBotConfig({}),
    /DISCORD_TOKEN is required/,
  );
});

test("an explicit job URL preserves the remote execution seam", () => {
  const config = loadDiscordBotConfig({
    DISCORD_TOKEN: "test-token",
    CLEARRA_JOB_URL: "https://oracle.example.test/jobs",
    CLEARRA_JOB_TOKEN: "job-token",
  });

  assert.equal(config.jobEndpoint, "https://oracle.example.test/jobs");
  assert.equal(config.workerAuthority, "remote");
  assert.equal(config.searchWorkersPerSession, undefined);
});

test("interaction deadlines are capped below the Discord interaction-token window", () => {
  assert.throws(
    () => loadDiscordBotConfig({
      DISCORD_TOKEN: "test-token",
      CLEARRA_INTERACTION_DEADLINE_MS: String(14 * 60_000 + 1),
    }),
    /must not exceed 840000 milliseconds/,
  );
});

test("direct execution preserves the caller's absolute deadline and AbortSignal", async () => {
  const calls = [];
  const runner = {
    async execute(job, options) {
      calls.push({ job, options });
      return { exitCode: 0, signal: null, stdout: "done", stderr: "" };
    },
  };
  const executor = new ClearraDirectExecutor(
    directConfig({ interactionDeadlineMs: 5_000, maxOutputBytes: 128 }),
    { runner, now: () => 10_000 },
  );
  const controller = new AbortController();

  const result = await executor.execute(["pc", "--lines", "2"], {
    deadlineUnixMs: 12_000,
    signal: controller.signal,
  });

  assert.equal(result.stdout, "done");
  assert.deepEqual(calls[0].job, {
    arguments: ["pc", "--lines", "2"],
    deadlineUnixMs: 12_000,
    maxOutputBytes: 128,
  });
  assert.equal(calls[0].options.signal, controller.signal);
});

test("direct execution cannot extend a caller deadline or its configured total bound", async () => {
  const deadlines = [];
  const executor = new ClearraDirectExecutor(
    directConfig({ interactionDeadlineMs: 5_000 }),
    {
      now: () => 10_000,
      runner: {
        async execute(job) {
          deadlines.push(job.deadlineUnixMs);
          return { exitCode: 0, signal: null, stdout: "", stderr: "" };
        },
      },
    },
  );

  await executor.execute(["pc"], { deadlineUnixMs: 11_000 });
  await executor.execute(["pc"], { deadlineUnixMs: 99_000 });
  assert.deepEqual(deadlines, [11_000, 15_000]);
});

test("an expired queued deadline fails before Clearra is spawned", async () => {
  let spawned = false;
  const executor = new ClearraDirectExecutor(
    directConfig(),
    {
      now: () => 10_000,
      runnerOptions: {
        now: () => 10_000,
        spawn() {
          spawned = true;
          throw new Error("must not spawn");
        },
      },
    },
  );

  await assert.rejects(
    executor.execute(["pc"], { deadlineUnixMs: 9_999 }),
    { name: "DeadlineExceededError" },
  );
  assert.equal(spawned, false);
});

function directConfig(overrides = {}) {
  return {
    executable: "clearra",
    processLogicalProcessors: 4,
    searchWorkersPerSession: 4,
    useAllLogicalProcessors: true,
    searchTimeoutMs: 3_000,
    interactionDeadlineMs: 4_000,
    maxOutputBytes: 1024,
    terminationGraceMs: 100,
    ...overrides,
  };
}
