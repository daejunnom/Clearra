import assert from "node:assert/strict";
import test from "node:test";

import {
  ClearraJobExecutor,
  parseClearraMessage,
  prepareClearraArguments,
  tilingOnlyRequested,
  tokenizeCommand,
} from "../src/clearra/command.mjs";
import {
  defaultSearchWorkersPerSession,
  loadDiscordBotConfig,
} from "../src/config.mjs";

test("Clearrabot defaults each search session to a three-minute timeout", () => {
  const config = loadDiscordBotConfig({ DISCORD_TOKEN: "test-token" });
  assert.equal(config.ingressMode, "gateway");
  assert.equal(config.searchTimeoutMs, 180_000);
  assert.equal(config.jobEndpoint, "http://127.0.0.1:8787/jobs");
  assert.equal(config.jobPollIntervalMs, 250);
  assert.equal(config.jobCancelTimeoutMs, 2_000);
});

test("Cloud Run ingress requires the Discord application public key", () => {
  assert.throws(
    () =>
      loadDiscordBotConfig({
        DISCORD_TOKEN: "test-token",
        CLEARRA_DISCORD_INGRESS: "cloud-run",
      }),
    /DISCORD_PUBLIC_KEY is required/,
  );
  const config = loadDiscordBotConfig({
    DISCORD_TOKEN: "test-token",
    DISCORD_PUBLIC_KEY: "01".repeat(32),
    DISCORD_APPLICATION_ID: "application-id",
    CLEARRA_DISCORD_INGRESS: "cloud-run",
    CLEARRA_DISCORD_INTERACTION_PATH: "/discord/interactions",
    PORT: "9090",
  });
  assert.equal(config.ingressMode, "cloud-run");
  assert.equal(config.registerCommands, false);
  assert.equal(config.applicationId, "application-id");
  assert.equal(config.port, 9090);
  assert.equal(config.interactionPath, "/discord/interactions");
});

test("Clearrabot validates and configures the HTTP job service", () => {
  const config = loadDiscordBotConfig({
    DISCORD_TOKEN: "test-token",
    CLEARRA_JOB_URL: "https://jobs.example.test/clearra/jobs",
    CLEARRA_JOB_TOKEN: "opaque-test-token",
    CLEARRA_JOB_POLL_INTERVAL_MS: "40",
  });
  assert.equal(config.jobEndpoint, "https://jobs.example.test/clearra/jobs");
  assert.equal(config.jobToken, "opaque-test-token");
  assert.equal(config.jobPollIntervalMs, 40);
  assert.throws(
    () =>
      loadDiscordBotConfig({
        DISCORD_TOKEN: "test-token",
        CLEARRA_JOB_URL: "file:///tmp/clearra.sock",
      }),
    /must use HTTP or HTTPS/,
  );
});

test("Clearrabot uses every logical processor by default with an explicit reserve-core opt-out", () => {
  const runtime = { availableParallelism: () => 8 };
  const config = loadDiscordBotConfig({ DISCORD_TOKEN: "test-token" }, runtime);
  assert.equal(config.processLogicalProcessors, 8);
  assert.equal(config.searchWorkersPerSession, undefined);
  assert.equal(config.useAllLogicalProcessors, true);
  assert.equal(defaultSearchWorkersPerSession(8, 2), 4);
  assert.equal(
    loadDiscordBotConfig(
      {
        DISCORD_TOKEN: "test-token",
        CLEARRA_SEARCH_WORKERS_PER_SESSION: "auto",
      },
      runtime,
    ).searchWorkersPerSession,
    undefined,
  );

  const reserveCore = loadDiscordBotConfig(
    {
      DISCORD_TOKEN: "test-token",
      CLEARRA_USE_ALL_LOGICAL_PROCESSORS: "0",
    },
    runtime,
  );
  assert.equal(reserveCore.searchWorkersPerSession, undefined);
  assert.equal(reserveCore.useAllLogicalProcessors, false);

  const shared = loadDiscordBotConfig(
    {
      DISCORD_TOKEN: "test-token",
      CLEARRA_MAX_CONCURRENT_SEARCHES: "2",
      CLEARRA_SEARCH_WORKERS_PER_SESSION: "4",
    },
    runtime,
  );
  assert.equal(shared.searchWorkersPerSession, 4);
  assert.equal(shared.useAllLogicalProcessors, true);
  const sharedAuto = loadDiscordBotConfig(
    {
      DISCORD_TOKEN: "test-token",
      CLEARRA_MAX_CONCURRENT_SEARCHES: "2",
      CLEARRA_SEARCH_WORKERS_PER_SESSION: "auto",
    },
    runtime,
  );
  assert.equal(sharedAuto.searchWorkersPerSession, 4);
  assert.equal(sharedAuto.useAllLogicalProcessors, true);
  const explicit = loadDiscordBotConfig(
    {
      DISCORD_TOKEN: "test-token",
      CLEARRA_SEARCH_WORKERS_PER_SESSION: "6",
    },
    runtime,
  );
  assert.equal(explicit.searchWorkersPerSession, 6);
  assert.equal(explicit.useAllLogicalProcessors, true);
  assert.throws(
    () =>
      loadDiscordBotConfig(
        {
          DISCORD_TOKEN: "test-token",
          CLEARRA_MAX_CONCURRENT_SEARCHES: "2",
          CLEARRA_SEARCH_WORKERS_PER_SESSION: "5",
        },
        runtime,
      ),
    /runtime limit of 4/,
  );
  assert.throws(
    () =>
      loadDiscordBotConfig(
        {
          DISCORD_TOKEN: "test-token",
          CLEARRA_MAX_CONCURRENT_SEARCHES: "9",
        },
        runtime,
      ),
    /runtime CPU capacity of 8/,
  );
  assert.throws(
    () =>
      loadDiscordBotConfig(
        {
          DISCORD_TOKEN: "test-token",
          CLEARRA_SEARCH_WORKERS_PER_SESSION: "9",
        },
        runtime,
      ),
    /runtime limit of 8/,
  );
  assert.throws(
    () =>
      loadDiscordBotConfig(
        {
          DISCORD_TOKEN: "test-token",
          CLEARRA_USE_ALL_LOGICAL_PROCESSORS: "yes",
        },
        runtime,
      ),
    /boolean setting is invalid/,
  );
});

test("command tokenizer preserves quoted queue syntax", () => {
  assert.deepEqual(
    tokenizeCommand('pc --lines 4 --patterns "[LOJ]!P7"'),
    ["pc", "--lines", "4", "--patterns", "[LOJ]!P7"],
  );
});

test("Discord commands always use the Clearra exact product path", () => {
  assert.deepEqual(
    prepareClearraArguments([
      "pc",
      "--lines",
      "4",
      "--tablebase",
      "--build-dependency-dag",
      "--format",
      "json",
    ]),
    [
      "pc",
      "--lines",
      "4",
      "--no-tablebase",
      "--no-build-dependency-dag",
      "--format",
      "text",
    ],
  );
  assert.deepEqual(
    parseClearraMessage("!setup --remaining SZ --priority pc"),
    [
      "setup",
      "--remaining",
      "SZ",
      "--priority",
      "pc",
      "--no-tablebase",
      "--format",
      "text",
    ],
  );
  assert.deepEqual(
    prepareClearraArguments(
      ["pc", "--lines", "2", "--format", "text", "--include-solution-data"],
      { outputFormat: "json", includeSolutionData: true },
    ),
    [
      "pc",
      "--lines",
      "2",
      "--no-tablebase",
      "--no-build-dependency-dag",
      "--format",
      "json",
      "--include-solution-data",
    ],
  );
});

test("Discord owns an adaptive worker ceiling instead of accepting a user override", () => {
  const runtimeSelected = prepareClearraArguments(
    ["pc", "--lines", "2", "--auto-workers", "99"],
    { workers: undefined, useAllLogicalProcessors: true },
  );
  assert.deepEqual(runtimeSelected, [
    "pc",
    "--lines",
    "2",
    "--no-tablebase",
    "--no-build-dependency-dag",
    "--use-all-cpu-threads",
    "--format",
    "text",
  ]);
  assert.deepEqual(
    prepareClearraArguments(runtimeSelected, {
      workers: undefined,
      useAllLogicalProcessors: true,
    }),
    runtimeSelected,
  );
  assert.deepEqual(
    prepareClearraArguments(
      [
        "pc",
        "--lines",
        "4",
        "--workers",
        "99",
        "--use-all-cpu-threads",
      ],
      { workers: 3 },
    ),
    [
      "pc",
      "--lines",
      "4",
      "--no-tablebase",
      "--no-build-dependency-dag",
      "--auto-workers",
      "3",
      "--format",
      "text",
    ],
  );
  assert.deepEqual(
    prepareClearraArguments(
      [
        "setup",
        "--remaining",
        "SZ",
        "--workers",
        "99",
        "--use-all-cpu-threads",
      ],
      { workers: 3 },
    ),
    [
      "setup",
      "--remaining",
      "SZ",
      "--no-tablebase",
      "--auto-workers",
      "3",
      "--format",
      "text",
    ],
  );
  assert.deepEqual(
    prepareClearraArguments(["percent", "--queue", "P7"], { workers: 3 }),
    ["percent", "--queue", "P7", "--format", "text"],
  );
  assert.deepEqual(
    prepareClearraArguments(["pc", "--workers=99", "--lines", "2"], { workers: 3 }),
    [
      "pc",
      "--lines",
      "2",
      "--no-tablebase",
      "--no-build-dependency-dag",
      "--auto-workers",
      "3",
      "--format",
      "text",
    ],
  );
  assert.deepEqual(
    prepareClearraArguments(
      [
        "build-probability",
        "--base-mask",
        "0",
        "--target-mask",
        "0xf",
        "--height",
        "1",
        "--workers",
        "99",
      ],
      { workers: 3 },
    ),
    [
      "build-probability",
      "--base-mask",
      "0",
      "--target-mask",
      "0xf",
      "--height",
      "1",
      "--auto-workers",
      "3",
      "--format",
      "text",
    ],
  );
  assert.throws(
    () =>
      prepareClearraArguments(["pc", "--lines", "4"], {
        workers: 9,
        useAllLogicalProcessors: true,
        logicalProcessors: 8,
      }),
    /hard limit of 8 logical processors/,
  );
});

test("Discord exposes curated sfinder commands through native worker policy", () => {
  const prepared = prepareClearraArguments(
    [
      "sfinder",
      "path",
      "v115@vhAAgH",
      "P7P3",
      "4",
      "--workers",
      "99",
      "--format",
      "json",
    ],
    { workers: 3 },
  );
  assert.deepEqual(prepared, [
    "sfinder",
    "path",
    "v115@vhAAgH",
    "P7P3",
    "4",
    "--auto-workers",
    "3",
    "--format",
    "text",
  ]);
  assert.deepEqual(
    prepareClearraArguments(prepared, { workers: 3 }),
    prepared,
  );
  assert.deepEqual(
    prepareClearraArguments(["sfinder", "verify", "kicks"], { workers: 3 }),
    ["sfinder", "verify", "kicks", "--format", "text"],
  );
  assert.deepEqual(
    parseClearraMessage("!sfinder pc_setup IOTS", "!", { workers: 2 }),
    [
      "sfinder",
      "pc_setup",
      "IOTS",
      "--auto-workers",
      "2",
      "--format",
      "text",
    ],
  );
});

test("Discord rejects unrepresented sfinder contracts", () => {
  for (const command of ["ren", "util", "parity", "render", "special-minimals"]) {
    assert.throws(
      () => prepareClearraArguments(["sfinder", command]),
      /does not expose the sfinder/,
    );
  }
  assert.throws(
    () => prepareClearraArguments(["sfinder"]),
    /require a subcommand/,
  );
});

test("tiling-only commands are recognized after Discord argument normalization", () => {
  assert.equal(
    tilingOnlyRequested(prepareClearraArguments(["pc", "--lines", "4", "--tiling-only"])),
    true,
  );
  assert.equal(
    tilingOnlyRequested(
      prepareClearraArguments(["pc", "--lines", "4", "--objective", "tiling"]),
    ),
    true,
  );
  assert.equal(
    tilingOnlyRequested(prepareClearraArguments(["pc", "--lines", "4"])),
    false,
  );
});

test("file-backed and unrelated commands are rejected", () => {
  assert.throws(
    () => prepareClearraArguments(["pc-scenario", "--fixture", "field.json"]),
    /curated Clearra PC, build, setup/,
  );
  assert.throws(
    () => prepareClearraArguments(["pc", "--fixture", "field.json"]),
    /File and custom-code inputs/,
  );
  assert.throws(
    () => prepareClearraArguments(["cover", "--template-file", "field.json"]),
    /File and custom-code inputs/,
  );
  assert.throws(
    () => prepareClearraArguments(["cover", "--template-file=field.json"]),
    /File and custom-code inputs/,
  );
});

test("Clearra executor submits an idempotent POST job without shell interpretation", async () => {
  const requests = [];
  const executor = new ClearraJobExecutor({
    endpoint: "https://jobs.example.test/v1/jobs",
    timeoutMs: 5_000,
    createJobId: () => "job-literal-1",
    fetch: async (url, request) => {
      requests.push({ url: String(url), request });
      return jobResponse({
        id: "job-literal-1",
        state: "completed",
        result: {
          exitCode: 0,
          signal: null,
          stdout: "done",
          stderr: "",
        },
      });
    },
  });
  const result = await executor.execute([
    "pc",
    "queue with spaces",
    "literal;&|$()",
  ]);

  assert.equal(result.exitCode, 0);
  assert.equal(result.stdout, "done");
  assert.equal(requests.length, 1);
  assert.equal(requests[0].url, "https://jobs.example.test/v1/jobs");
  assert.equal(requests[0].request.method, "POST");
  assert.equal(requests[0].request.headers["idempotency-key"], "job-literal-1");
  const job = JSON.parse(requests[0].request.body);
  assert.equal(job.protocol, "clearra.job.v1");
  assert.equal(job.id, "job-literal-1");
  assert.equal(job.kind, "clearra.command");
  assert.deepEqual(job.arguments, [
    "pc",
    "queue with spaces",
    "literal;&|$()",
  ]);
  assert.equal(job.deadlineUnixMs > Date.now(), true);
});

test("Clearra executor polls an accepted POST job to its terminal result", async () => {
  const requests = [];
  const responses = [
    jobResponse({ id: "job-poll-1", state: "accepted" }, 202),
    jobResponse({ id: "job-poll-1", state: "running" }, 200),
    jobResponse({
      id: "job-poll-1",
      state: "completed",
      result: { exitCode: 2, signal: null, stdout: "", stderr: "invalid queue" },
    }),
  ];
  const executor = new ClearraJobExecutor({
    endpoint: "https://jobs.example.test/jobs",
    pollIntervalMs: 1,
    createJobId: () => "job-poll-1",
    fetch: async (url, request) => {
      requests.push([String(url), request.method]);
      return responses.shift();
    },
  });

  const result = await executor.execute(["pc", "--lines", "4"]);
  assert.equal(result.exitCode, 2);
  assert.equal(result.stderr, "invalid queue");
  assert.deepEqual(requests, [
    ["https://jobs.example.test/jobs", "POST"],
    ["https://jobs.example.test/jobs/job-poll-1", "GET"],
    ["https://jobs.example.test/jobs/job-poll-1", "GET"],
  ]);
});

test("Clearra executor cancels the remote job when the Clearrabot timeout expires", async () => {
  const requests = [];
  const executor = new ClearraJobExecutor({
    endpoint: "https://jobs.example.test/jobs",
    timeoutMs: 50,
    cancelTimeoutMs: 100,
    createJobId: () => "job-timeout-1",
    fetch: async (url, request) => {
      requests.push([String(url), request.method]);
      if (request.method === "DELETE") return new Response(null, { status: 204 });
      return await pendingUntilAbort(request.signal);
    },
  });

  await assert.rejects(
    executor.execute(["pc", "--lines", "4"]),
    /Clearrabot search exceeded the 50-millisecond time limit/,
  );
  assert.deepEqual(requests.at(-1), [
    "https://jobs.example.test/jobs/job-timeout-1",
    "DELETE",
  ]);
});

test("Clearra executor forwards caller cancellation to the remote job", async () => {
  const controller = new AbortController();
  const requests = [];
  const executor = new ClearraJobExecutor({
    endpoint: "https://jobs.example.test/jobs",
    timeoutMs: 5_000,
    createJobId: () => "job-cancel-1",
    fetch: async (url, request) => {
      requests.push([String(url), request.method]);
      if (request.method === "DELETE") return new Response(null, { status: 204 });
      return await pendingUntilAbort(request.signal);
    },
  });

  const execution = executor.execute(["setup", "--remaining", "SZ"], {
    signal: controller.signal,
  });
  controller.abort();
  await assert.rejects(execution, { name: "AbortError" });
  assert.deepEqual(requests.at(-1), [
    "https://jobs.example.test/jobs/job-cancel-1",
    "DELETE",
  ]);
});

function jobResponse(job, status = 200) {
  return new Response(
    JSON.stringify({ protocol: "clearra.job.v1", ...job }),
    {
      status,
      headers: { "content-type": "application/json" },
    },
  );
}

function pendingUntilAbort(signal) {
  return new Promise((resolve, reject) => {
    const abort = () => reject(signal.reason ?? new Error("aborted"));
    signal.addEventListener("abort", abort, { once: true });
    if (signal.aborted) abort();
  });
}
