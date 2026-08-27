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
    CLEARRA_EXPECTED_JOB_SOURCE_COMMIT: "1".repeat(40),
    CLEARRA_EXPECTED_ENGINE_BUILD_ID: "1".repeat(40),
    CLEARRA_EXPECTED_JOB_CONTRACT_REVISION: "clearra.search.contract.v2",
    CLEARRA_EXPECTED_SUPPLY_SEMANTICS_ID:
      "clearra.supply.projected-terminal-lookahead.v1",
    CLEARRA_EXPECTED_ARTIFACT_SCHEMA_VERSION: "clearra.solution-data.v1",
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
    timeoutClass: "pc_reverse",
    deadlineUnixMs: 12_000,
    maxOutputBytes: 128,
    maxArtifactBytes: 128,
  });
  assert.equal(calls[0].options.signal, controller.signal);
});

test("direct execution never forwards tie paging flags", () => {
  let calls = 0;
  const executor = new ClearraDirectExecutor(directConfig(), {
    runner: {
      async execute() {
        calls += 1;
        return { exitCode: 0, signal: null, stdout: "", stderr: "" };
      },
    },
  });

  for (const arguments_ of [
    ["pc", "minimals", "--ties"],
    ["pc", "minimals", "--tie-snapshot=result.jsonl"],
    ["continue", "--tie-cursor", "cursor"],
  ]) {
    assert.throws(
      () => executor.execute(arguments_),
      /does not expose alternative-result paging options/,
    );
  }
  assert.equal(calls, 0);
});

test("direct execution rejects alternative metadata but permits normal families", async () => {
  const outputs = [
    JSON.stringify({ kind: "best-save", summary: { best_save_winners: [] } }),
    JSON.stringify({ kind: "pc-minimum-cover.v2", portfolio_alternative_page: {} }),
  ];
  const executor = new ClearraDirectExecutor(directConfig(), {
    runner: {
      async execute() {
        return { exitCode: 0, signal: null, stdout: outputs.shift(), stderr: "" };
      },
    },
  });

  await executor.execute(["pc", "best-save"]);
  await assert.rejects(
    executor.execute(["pc", "minimals"]),
    /alternative-result metadata/,
  );
});

test("direct execution cannot extend a caller deadline or its command-class bound", async () => {
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
  assert.deepEqual(deadlines, [11_000, 13_000]);
});

test("direct execution preserves every canonical family class and limit", async () => {
  const jobs = [];
  const executor = new ClearraDirectExecutor(
    directConfig({
      searchTimeoutMs: 2_000,
      pcSearchTimeoutMs: 5_000,
      buildSearchTimeoutMs: 6_000,
      setupSearchTimeoutMs: 7_000,
      forwardSearchTimeoutMs: 8_000,
      structureSearchTimeoutMs: 9_000,
      diagnosticTimeoutMs: 3_000,
      interactionDeadlineMs: 20_000,
    }),
    {
      now: () => 10_000,
      runner: {
        async execute(job) {
          jobs.push(job);
          return { exitCode: 0, signal: null, stdout: "", stderr: "" };
        },
      },
    },
  );

  await executor.execute(["pc"], { deadlineUnixMs: 99_000 });
  await executor.execute(["pc", "chance"], { deadlineUnixMs: 99_000 });
  await executor.execute(["pc", "score"], { deadlineUnixMs: 99_000 });
  await executor.execute(["pc", "failed-queue"], { deadlineUnixMs: 99_000 });
  await executor.execute(["build-probability"], { deadlineUnixMs: 99_000 });
  await executor.execute(["setup-finder"], { deadlineUnixMs: 99_000 });
  await executor.execute(["damage"], { deadlineUnixMs: 99_000 });
  await executor.execute(["spin-structure"], { deadlineUnixMs: 99_000 });
  await executor.execute(["sfinder", "verify", "pc"], { deadlineUnixMs: 99_000 });

  assert.deepEqual(
    jobs.map(({ timeoutClass, deadlineUnixMs }) => [timeoutClass, deadlineUnixMs]),
    [
      ["pc_reverse", 15_000],
      ["pc_reverse", 15_000],
      ["pc_reverse", 15_000],
      ["pc_reverse", 15_000],
      ["build_long", 16_000],
      ["setup_long", 17_000],
      ["forward_long", 18_000],
      ["structure_long", 19_000],
      ["diagnostic", 13_000],
    ],
  );
  assert.deepEqual(jobs[1].arguments, ["pc", "chance"]);
  assert.deepEqual(jobs[2].arguments, ["pc", "score"]);
  assert.deepEqual(jobs[3].arguments, ["pc", "failed-queue"]);
  assert.throws(
    () => executor.execute(["damage"], { timeoutClass: "pc_reverse" }),
    /does not match/,
  );
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
