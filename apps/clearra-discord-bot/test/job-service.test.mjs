import assert from "node:assert/strict";
import test from "node:test";

import { prepareClearraArguments } from "../src/clearra/command.mjs";
import { loadDiscordBotConfig } from "../src/config.mjs";
import { loadClearraJobServiceConfig } from "../src/job-service/config.mjs";
import { ClearraJobService } from "../src/job-service/server.mjs";

test("remote job execution is not capped by the Oracle gateway CPU count", () => {
  const config = loadDiscordBotConfig(
    {
      DISCORD_TOKEN: "test-token",
      CLEARRA_JOB_URL: "https://jobs.example.test/jobs",
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
      CLEARRA_WORKER_AUTHORITY: "gateway",
      CLEARRA_MAX_CONCURRENT_SEARCHES: "2",
    },
    { availableParallelism: () => 8 },
  );

  assert.equal(config.workerAuthority, "gateway");
  assert.equal(config.maxConcurrentSearches, 2);
  assert.equal(config.searchWorkersPerSession, 3);
});

test("job service derives Clearra workers from the Cloud Run CPU limit", () => {
  const config = loadClearraJobServiceConfig(
    {
      CLEARRA_JOB_TOKEN: "job-token",
      CLEARRA_SEARCH_WORKERS_PER_SESSION: "auto",
    },
    { availableParallelism: () => 6 },
  );

  assert.equal(config.processLogicalProcessors, 6);
  assert.equal(config.searchWorkersPerSession, 5);
  assert.equal(config.maxConcurrentJobs, 1);
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
  const request = () =>
    fetch(endpoint, {
      method: "POST",
      headers: {
        authorization: "Bearer job-token",
        "content-type": "application/json",
        "idempotency-key": body.id,
      },
      body: JSON.stringify(body),
    });

  const first = await request();
  assert.equal(first.status, 200);
  const firstJob = await first.json();
  assert.equal(firstJob.state, "completed");
  assert.equal(firstJob.result.exitCode, 0);
  assert.match(firstJob.result.stdout, /pc --lines 4/);

  const second = await request();
  assert.equal(second.status, 200);
  assert.equal((await second.json()).state, "completed");
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
