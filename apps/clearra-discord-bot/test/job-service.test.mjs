import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { readFileSync } from "node:fs";
import { PassThrough } from "node:stream";
import test from "node:test";

import { prepareClearraArguments } from "../src/clearra/command.mjs";
import { loadDiscordBotConfig } from "../src/config.mjs";
import { loadClearraJobServiceConfig } from "../src/job-service/config.mjs";
import { ClearraCommandRunner } from "../src/job-service/runner.mjs";
import { ClearraJobService } from "../src/job-service/server.mjs";

test("job runner requires working finesse search and score capabilities", async () => {
  const invocations = [];
  const runner = new ClearraCommandRunner(
    {
      executable: "clearra",
      searchTimeoutMs: 5_000,
    },
    {
      execFile: (_executable, arguments_, options, callback) => {
        invocations.push({ arguments_, options });
        callback(null, JSON.stringify({
          finesse_report: {
            mode: arguments_[1],
            metric: "inputs",
          },
        }), "");
      },
    },
  );

  await runner.verifyCapabilities();

  assert.deepEqual(
    invocations.map(({ arguments_ }) => arguments_.slice(0, 2)),
    [["finesse", "search"], ["finesse", "score"]],
  );
  for (const invocation of invocations) {
    assert.equal(invocation.options.shell, false);
    assert.equal(invocation.options.windowsHide, true);
    assert.equal(invocation.arguments_.includes("--workers"), true);
    assert.equal(invocation.arguments_.includes("--format"), true);
  }
});

test("job runner fails closed when a legacy CLI lacks finesse", async () => {
  const runner = new ClearraCommandRunner(
    {
      executable: "clearra-v0.5.1",
      searchTimeoutMs: 5_000,
    },
    {
      execFile: (_executable, _arguments, _options, callback) => {
        const error = new Error("unsupported command");
        error.code = 2;
        callback(error, "", "unsupported command");
      },
    },
  );

  await assert.rejects(
    runner.verifyCapabilities(),
    /^Error: Clearra engine capability check failed\.$/,
  );
});

test("both job-service Docker paths smoke-test finesse before runtime", () => {
  const imagePaths = [
    ["Dockerfile.current-job-service", "cloudbuild-current-job-service.yaml"],
    ["Dockerfile.job-service", "cloudbuild-job-service.yaml"],
  ];
  for (const [dockerName, cloudBuildName] of imagePaths) {
    const dockerfile = readFileSync(
      new URL(`../${dockerName}`, import.meta.url),
      "utf8",
    );
    assert.match(dockerfile, /finesse search --base-mask/);
    assert.match(dockerfile, /finesse score --initial-mask/);
    assert.match(dockerfile, /grep -q '\"mode\":\"search\"'/);
    assert.match(dockerfile, /grep -q '\"mode\":\"score\"'/);
    const cloudBuild = readFileSync(
      new URL(`../${cloudBuildName}`, import.meta.url),
      "utf8",
    );
    assert.match(
      cloudBuild,
      new RegExp(`- apps/clearra-discord-bot/${dockerName.replaceAll(".", "\\.")}`),
    );
  }

  const main = readFileSync(
    new URL("../src/job-service/main.mjs", import.meta.url),
    "utf8",
  );
  const gateOffset = main.indexOf("await runner.verifyCapabilities()");
  const listenOffset = main.indexOf("await service.listen()");
  assert.notEqual(gateOffset, -1);
  assert.notEqual(listenOffset, -1);
  assert.ok(
    gateOffset < listenOffset,
    "the capability gate must finish before the service listens",
  );
});

test("remote job execution is not capped by the Oracle gateway CPU count", () => {
  const config = loadDiscordBotConfig(
    {
      DISCORD_TOKEN: "test-token",
      CLEARRA_JOB_URL: "https://jobs.example.test/jobs",
      CLEARRA_JOB_TOKEN: "job-token",
      CLEARRA_MAX_CONCURRENT_REMOTE_JOBS: "4",
    },
    { availableParallelism: () => 2 },
  );

  assert.equal(config.workerAuthority, "remote");
  assert.equal(config.maxConcurrentSearches, 4);
  assert.equal(config.searchWorkersPerSession, undefined);
  assert.equal(config.useAllLogicalProcessors, false);
  assert.deepEqual(
    prepareClearraArguments(["pc", "--lines", "4"], {
      workers: config.searchWorkersPerSession,
    }),
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
});

test("gateway worker authority preserves host-local CPU allocation", () => {
  const config = loadDiscordBotConfig(
    {
      DISCORD_TOKEN: "test-token",
      CLEARRA_JOB_URL: "https://jobs.example.test/jobs",
      CLEARRA_JOB_TOKEN: "job-token",
      CLEARRA_WORKER_AUTHORITY: "gateway",
      CLEARRA_MAX_CONCURRENT_SEARCHES: "2",
    },
    { availableParallelism: () => 8 },
  );

  assert.equal(config.workerAuthority, "gateway");
  assert.equal(config.maxConcurrentSearches, 2);
  assert.equal(config.searchWorkersPerSession, 4);
});

test("job service uses every Cloud Run logical processor by default", () => {
  const config = loadClearraJobServiceConfig(
    {
      K_SERVICE: "clearra-job-service",
      CLEARRA_JOB_TOKEN: "job-token",
      CLEARRA_SEARCH_WORKERS_PER_SESSION: "auto",
    },
    { availableParallelism: () => 6 },
  );

  assert.equal(config.processLogicalProcessors, 6);
  assert.equal(config.expectedVcpus, 6);
  assert.equal(config.searchWorkersPerSession, 6);
  assert.equal(config.useAllLogicalProcessors, true);
  assert.equal(config.maxConcurrentJobs, 1);
  assert.equal(config.port, 8787);
  assert.equal(config.searchTimeoutMs, 170_000);
  assert.equal(config.reverseSearchTimeoutMs, 300_000);
  assert.equal(config.forwardSearchTimeoutMs, 900_000);

  const timeoutOverrides = loadClearraJobServiceConfig(
    {
      CLEARRA_JOB_TOKEN: "job-token",
      CLEARRA_SEARCH_TIMEOUT_MS: "1000",
      CLEARRA_REVERSE_SEARCH_TIMEOUT_MS: "2000",
      CLEARRA_FORWARD_SEARCH_TIMEOUT_MS: "3000",
    },
    { availableParallelism: () => 6 },
  );
  assert.equal(timeoutOverrides.searchTimeoutMs, 1_000);
  assert.equal(timeoutOverrides.reverseSearchTimeoutMs, 2_000);
  assert.equal(timeoutOverrides.forwardSearchTimeoutMs, 3_000);

  const reserveCore = loadClearraJobServiceConfig(
    {
      CLEARRA_JOB_TOKEN: "job-token",
      CLEARRA_SEARCH_WORKERS_PER_SESSION: "auto",
      CLEARRA_USE_ALL_LOGICAL_PROCESSORS: "0",
    },
    { availableParallelism: () => 6 },
  );
  assert.equal(reserveCore.searchWorkersPerSession, 5);
  assert.equal(reserveCore.useAllLogicalProcessors, false);
  assert.equal(reserveCore.expectedVcpus, undefined);

  assert.throws(
    () => loadClearraJobServiceConfig(
      {
        CLEARRA_JOB_TOKEN: "job-token",
        CLEARRA_SEARCH_WORKERS_PER_SESSION: "7",
      },
      { availableParallelism: () => 6 },
    ),
    /per-job runtime limit of 6/,
  );
  assert.throws(
    () => loadClearraJobServiceConfig(
      {
        CLEARRA_JOB_TOKEN: "job-token",
        CLEARRA_USE_ALL_LOGICAL_PROCESSORS: "yes",
      },
      { availableParallelism: () => 6 },
    ),
    /boolean setting is invalid/,
  );
});

test("unauthenticated local job service is restricted to loopback", () => {
  assert.throws(
    () => loadClearraJobServiceConfig({
      CLEARRA_JOB_SERVICE_ALLOW_UNAUTHENTICATED: "1",
    }),
    /limited to a loopback listen host/,
  );
  const config = loadClearraJobServiceConfig({
    CLEARRA_JOB_SERVICE_ALLOW_UNAUTHENTICATED: "1",
    CLEARRA_LISTEN_HOST: "127.0.0.1",
  });
  assert.equal(config.host, "127.0.0.1");
  assert.equal(config.authorizationToken, null);
});

test("job service partitions its CPU limit across concurrent jobs", () => {
  const config = loadClearraJobServiceConfig(
    {
      CLEARRA_JOB_TOKEN: "job-token",
      CLEARRA_MAX_CONCURRENT_JOBS: "2",
      CLEARRA_SEARCH_WORKERS_PER_SESSION: "auto",
    },
    { availableParallelism: () => 6 },
  );

  assert.equal(config.maxConcurrentJobs, 2);
  assert.equal(config.searchWorkersPerSession, 3);
  assert.throws(
    () => loadClearraJobServiceConfig(
      {
        CLEARRA_JOB_TOKEN: "job-token",
        CLEARRA_MAX_CONCURRENT_JOBS: "2",
        CLEARRA_SEARCH_WORKERS_PER_SESSION: "4",
      },
      { availableParallelism: () => 6 },
    ),
    /per-job runtime limit of 3/,
  );
});

test("job runner sends curated sfinder argv without shell interpretation", async () => {
  let invocation;
  const runner = new ClearraCommandRunner(
    {
      executable: "clearra",
      processLogicalProcessors: 6,
      expectedVcpus: 6,
      searchWorkersPerSession: 6,
      useAllLogicalProcessors: true,
      searchTimeoutMs: 5_000,
      maxOutputBytes: 1024 * 1024,
      terminationGraceMs: 100,
    },
    {
      spawn: (executable, arguments_, options) => {
        invocation = { executable, arguments_, options };
        const child = new EventEmitter();
        child.stdout = new PassThrough();
        child.stderr = new PassThrough();
        child.exitCode = null;
        child.signalCode = null;
        child.kill = () => true;
        queueMicrotask(() => {
          child.exitCode = 0;
          child.emit("close", 0, null);
        });
        return child;
      },
    },
  );

  const result = await runner.execute({
    arguments: [
      "sfinder",
      "chance",
      "v115@vhAAgH",
      "P7P3",
      "4",
      "--workers",
      "99",
      "--format",
      "json",
    ],
    deadlineUnixMs: Date.now() + 5_000,
    maxOutputBytes: 1024 * 1024,
  });

  assert.equal(result.exitCode, 0);
  assert.equal(invocation.executable, "clearra");
  assert.equal(invocation.options.shell, false);
  assert.equal(invocation.options.env.CLEARRA_EXPECTED_VCPUS, "6");
  assert.deepEqual(invocation.arguments_, [
    "sfinder",
    "chance",
    "v115@vhAAgH",
    "P7P3",
    "4",
    "--auto-workers",
    "6",
    "--use-all-cpu-threads",
    "--format",
    "json",
    "--include-solution-data",
  ]);
});

test("job runner clamps execution timers by search direction and absolute deadline", async () => {
  const observedTimeouts = [];
  const runner = new ClearraCommandRunner(
    {
      executable: "clearra",
      processLogicalProcessors: 1,
      searchWorkersPerSession: 1,
      useAllLogicalProcessors: false,
      searchTimeoutMs: 2_000,
      reverseSearchTimeoutMs: 5_000,
      forwardSearchTimeoutMs: 15_000,
      maxOutputBytes: 1024 * 1024,
      terminationGraceMs: 100,
    },
    {
      now: () => 10_000,
      setTimeout: (_callback, timeoutMs) => {
        observedTimeouts.push(timeoutMs);
        return { unref() {} };
      },
      clearTimeout() {},
      spawn: () => {
        const child = new EventEmitter();
        child.stdout = new PassThrough();
        child.stderr = new PassThrough();
        child.exitCode = null;
        child.signalCode = null;
        child.kill = () => true;
        queueMicrotask(() => {
          child.exitCode = 0;
          child.emit("close", 0, null);
        });
        return child;
      },
    },
  );
  const execute = (arguments_, deadlineUnixMs = 99_000) => runner.execute({
    arguments: arguments_,
    deadlineUnixMs,
    maxOutputBytes: 1024 * 1024,
  });

  await execute(["pc"]);
  await execute(["damage"]);
  await execute(["setup-finder", "--remaining", "TI"]);
  await execute(["sfinder", "verify", "pc"]);
  await execute(["damage"], 10_750);

  assert.deepEqual(observedTimeouts, [5_000, 15_000, 15_000, 2_000, 750]);
});

test("job runner retains its slot until a cancelled process closes", async () => {
  const child = new EventEmitter();
  child.stdout = new PassThrough();
  child.stderr = new PassThrough();
  child.exitCode = null;
  child.signalCode = null;
  child.signals = [];
  child.kill = (signal) => {
    child.signals.push(signal);
    if (signal === "SIGTERM") {
      setTimeout(() => {
        child.signalCode = signal;
        child.emit("close", null, signal);
      }, 25);
    }
    return true;
  };
  const runner = new ClearraCommandRunner(
    {
      executable: "clearra",
      searchWorkersPerSession: 5,
      useAllLogicalProcessors: false,
      searchTimeoutMs: 5_000,
      maxOutputBytes: 1024 * 1024,
      terminationGraceMs: 100,
    },
    { spawn: () => child },
  );
  const controller = new AbortController();
  let outcome;
  const observed = runner.execute(
    {
      arguments: ["pc", "--lines", "4"],
      deadlineUnixMs: Date.now() + 5_000,
      maxOutputBytes: 1024 * 1024,
    },
    { signal: controller.signal },
  ).then(
    (value) => { outcome = { value }; },
    (error) => { outcome = { error }; },
  );

  controller.abort();
  await new Promise((resolve) => setTimeout(resolve, 5));
  assert.equal(outcome, undefined);
  await observed;
  assert.match(outcome.error.message, /cancelled/);
  assert.deepEqual(child.signals, ["SIGTERM"]);
});

test("job service executes an authenticated synchronous idempotent job", async (t) => {
  let executions = 0;
  const config = {
    host: "127.0.0.1",
    port: 0,
    authorizationToken: "job-token",
    allowUnauthenticated: false,
    maxRequestBodyBytes: 64 * 1024,
    maxOutputBytes: 1024 * 1024,
    searchTimeoutMs: 5_000,
    terminationGraceMs: 10,
    maxConcurrentJobs: 1,
    completedJobTtlMs: 60_000,
    maxRetainedJobs: 16,
    processLogicalProcessors: 6,
    searchWorkersPerSession: 5,
    useAllLogicalProcessors: false,
  };
  const runner = {
    async execute(job) {
      executions += 1;
      return {
        exitCode: 0,
        signal: null,
        stdout: `${job.arguments.join(" ")} complete`,
        stderr: "",
      };
    },
  };
  const service = new ClearraJobService(config, runner);
  const address = await service.listen();
  t.after(() => service.close());
  const endpoint = `http://127.0.0.1:${address.port}/jobs`;
  const body = {
    protocol: "clearra.job.v1",
    id: "discord-123",
    kind: "clearra.command",
    arguments: ["pc", "--lines", "4", "--format", "text"],
    deadlineUnixMs: Date.now() + 5_000,
    maxOutputBytes: 1024 * 1024,
  };
  const request = (overrides = {}) =>
    fetch(endpoint, {
      method: "POST",
      headers: {
        authorization: "Bearer job-token",
        "content-type": "application/json",
        "idempotency-key": body.id,
      },
      body: JSON.stringify({ ...body, ...overrides }),
    });

  const first = await request();
  assert.equal(first.status, 200);
  const firstJob = await first.json();
  assert.equal(firstJob.state, "completed");
  assert.equal(firstJob.result.exitCode, 0);
  assert.match(firstJob.result.stdout, /pc --lines 4/);

  const second = await request({
    deadlineUnixMs: body.deadlineUnixMs + 1_000,
  });
  assert.equal(second.status, 200);
  assert.equal((await second.json()).state, "completed");
  assert.equal(executions, 1);

  const conflicting = await request({ arguments: ["pc", "--lines", "2"] });
  assert.equal(conflicting.status, 409);
  assert.equal(executions, 1);
});

test("job service rejects unauthenticated work", async (t) => {
  const config = {
    host: "127.0.0.1",
    port: 0,
    authorizationToken: "job-token",
    allowUnauthenticated: false,
    maxRequestBodyBytes: 64 * 1024,
    maxOutputBytes: 1024 * 1024,
    searchTimeoutMs: 5_000,
    terminationGraceMs: 10,
    maxConcurrentJobs: 1,
    completedJobTtlMs: 60_000,
    maxRetainedJobs: 16,
    processLogicalProcessors: 6,
    searchWorkersPerSession: 5,
    useAllLogicalProcessors: false,
  };
  const service = new ClearraJobService(config, {
    async execute() {
      throw new Error("must not run");
    },
  });
  const address = await service.listen();
  t.after(() => service.close());

  const response = await fetch(`http://127.0.0.1:${address.port}/jobs`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: "{}",
  });
  assert.equal(response.status, 401);
});
