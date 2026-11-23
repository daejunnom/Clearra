import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import test from "node:test";

import { Clearrabot } from "../src/bot.mjs";
import { ClearraJobExecutor } from "../src/clearra/command.mjs";
import { ClearraDirectExecutor } from "../src/clearra/direct-executor.mjs";
import { loadDiscordBotConfig } from "../src/config.mjs";
import { assertDiscordCatalogComplete, operationErrorText } from "../src/discord/i18n.mjs";
import { loadClearraJobServiceConfig } from "../src/job-service/config.mjs";
import { armDeadline } from "../src/job-service/execution-deadline.mjs";
import {
  PLATFORM_DEADLINE_CODE,
  platformDeadlineError,
} from "../src/job-service/platform-deadline.mjs";
import { ClearraCommandRunner } from "../src/job-service/runner.mjs";
import { ClearraJobService } from "../src/job-service/server.mjs";

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function fakeClock(initial = 10_000_000) {
  let current = initial;
  const timers = new Map();
  const clock = {
    now: () => current,
    setTimeout(callback, delay) {
      const token = { unref() {} };
      timers.set(token, { at: current + delay, callback });
      return token;
    },
    clearTimeout: (token) => timers.delete(token),
    advance(delta) {
      current += delta;
      while (true) {
        const due = [...timers].find(([, timer]) => timer.at <= current);
        if (!due) break;
        timers.delete(due[0]);
        due[1].callback();
      }
    },
    timers,
  };
  return clock;
}

const flush = () => new Promise((resolve) => setImmediate(resolve));
const result = () => ({ exitCode: 0, signal: null, stdout: "done", stderr: "" });
const envelope = (id, extra = {}) => new Response(JSON.stringify({
  protocol: "clearra.job.v1", id, state: "completed", result: result(), ...extra,
}), { headers: { "content-type": "application/json" } });

function config(overrides = {}) {
  return {
    ...loadClearraJobServiceConfig({
      CLEARRA_JOB_SERVICE_ALLOW_UNAUTHENTICATED: "1",
      CLEARRA_LISTEN_HOST: "127.0.0.1",
    }, { availableParallelism: () => 4 }),
    port: 0,
    ...overrides,
  };
}

function job(id, requestStartedUnixMs = 0) {
  return {
    protocol: "clearra.job.v1", id, kind: "clearra.command",
    arguments: ["pc", "path", "--lines", "4"], deadlineUnixMs: 0,
    requestStartedUnixMs, maxOutputBytes: 1024, maxArtifactBytes: 1024,
  };
}

function submit(endpoint, payload) {
  return fetch(endpoint, {
    method: "POST",
    headers: { "content-type": "application/json", "idempotency-key": payload.id },
    body: JSON.stringify(payload),
  });
}

test("zero computation defaults are independent of local, Discord, and Cloud transport clocks", () => {
  const local = config();
  assert.equal(local.searchTimeoutMs, 0);
  assert.equal(local.pcSearchTimeoutMs, 0);
  assert.equal(local.utilitySearchTimeoutMs, 0);
  assert.equal(local.platformRequestTimeoutMs, 0);
  const cloud = loadClearraJobServiceConfig({ K_SERVICE: "job", CLEARRA_JOB_TOKEN: "fixture" });
  assert.equal(cloud.searchTimeoutMs, 0);
  assert.equal(cloud.platformRequestTimeoutMs, 900_000);
  assert.equal(cloud.platformResponseReserveMs, 15_000);
  const discord = loadDiscordBotConfig({ DISCORD_TOKEN: "fixture" });
  assert.equal(discord.searchTimeoutMs, 0);
  assert.equal(discord.platformRequestTimeoutMs, 0);
  assert.equal(discord.interactionDeadlineMs, 840_000);
  assert.throws(() => loadClearraJobServiceConfig({
    CLEARRA_JOB_TOKEN: "fixture", CLEARRA_JOB_REQUEST_TIMEOUT_MS: "-1",
  }));
});

test("zero arms no timer and very long explicit deadlines are scheduled in safe chunks", () => {
  const clock = fakeClock();
  let calls = 0;
  armDeadline(0, () => calls++, clock);
  assert.equal(clock.timers.size, 0);
  armDeadline(clock.now() + 2_147_483_650, () => calls++, clock);
  clock.advance(2_147_483_647);
  assert.equal(calls, 0);
  clock.advance(3);
  assert.equal(calls, 1);
});

test("remote and direct unlimited execution submit zero without a computation timer", async () => {
  const clock = fakeClock();
  let submitted;
  const remote = new ClearraJobExecutor({
    ...clock, createJobId: () => "unlimited",
    fetch: async (_url, request) => { submitted = JSON.parse(request.body); return envelope("unlimited"); },
  });
  assert.equal((await remote.execute(["pc", "path"])).stdout, "done");
  assert.equal(submitted.deadlineUnixMs, 0);
  assert.equal(submitted.requestStartedUnixMs, clock.now());
  assert.equal(clock.timers.size, 0);
  let directJob;
  const direct = new ClearraDirectExecutor({
    searchTimeoutMs: 0, interactionDeadlineMs: 1, maxOutputBytes: 1024,
  }, { now: clock.now, runner: { async execute(value) { directJob = value; return result(); } } });
  await direct.execute(["pc", "path"]);
  assert.equal(directJob.deadlineUnixMs, 0);
  await direct.execute(["pc", "path"], { deadlineUnixMs: clock.now() + 42 });
  assert.equal(directJob.deadlineUnixMs, clock.now() + 42);
});

test("an unlimited runner creates no calculation alarm", async () => {
  const runner = new ClearraCommandRunner(config(), {
    readOomKillCount: async () => null,
    setTimeout() { assert.fail("unlimited execution must not create a calculation alarm"); },
    spawn() {
      const child = new EventEmitter();
      child.stdout = new PassThrough();
      child.stderr = new PassThrough();
      child.exitCode = null;
      child.signalCode = null;
      child.kill = () => assert.fail("a completed unlimited execution must not be killed");
      queueMicrotask(() => { child.exitCode = 0; child.emit("close", 0, null); });
      return child;
    },
  });
  assert.equal((await runner.execute(job("unlimited-runner"))).exitCode, 0);
});

test("Cloud boundary replies once before cleanup and discards late solver success", async (t) => {
  const clock = fakeClock();
  const started = deferred();
  const solver = deferred();
  let executions = 0;
  let signal;
  const service = new ClearraJobService(config({
    platformRequestTimeoutMs: 900_000, platformResponseReserveMs: 15_000,
  }), { execute(_job, options) { executions++; signal = options.signal; started.resolve(); return solver.promise; } }, clock);
  const address = await service.listen();
  t.after(async () => { solver.resolve(result()); await service.close(); });
  const endpoint = `http://127.0.0.1:${address.port}/jobs`;
  const original = job("cloud-race", clock.now() - 2_000);
  const response = submit(endpoint, original);
  await started.promise;
  clock.advance(883_000);
  const failed = await (await response).json();
  assert.equal(failed.state, "failed");
  assert.equal(failed.errorCode, PLATFORM_DEADLINE_CODE);
  assert.equal(signal.aborted, true);
  assert.equal(service.activeJobs, 1);
  assert.equal(service.jobs.get(original.id).completedAt, null);
  assert.deepEqual(await (await fetch(`${endpoint}/${original.id}`)).json(), failed);
  const duplicate = await submit(endpoint, { ...original, requestStartedUnixMs: clock.now() });
  assert.deepEqual(await duplicate.json(), failed);
  assert.equal(executions, 1);
  solver.resolve(result());
  await service.jobs.get(original.id).execution;
  assert.equal(service.activeJobs, 0);
  assert.deepEqual(service.jobs.get(original.id).terminal, failed);
});

test("startup already consuming the Cloud budget cannot launch a solver", async (t) => {
  const clock = fakeClock();
  let executions = 0;
  const service = new ClearraJobService(config({
    platformRequestTimeoutMs: 900_000, platformResponseReserveMs: 15_000,
  }), { async execute() { executions++; return result(); } }, clock);
  const address = await service.listen();
  t.after(() => service.close());
  const response = await submit(`http://127.0.0.1:${address.port}/jobs`,
    job("already-spent", clock.now() - 885_000));
  assert.equal((await response.json()).errorCode, PLATFORM_DEADLINE_CODE);
  assert.equal(executions, 0);
  assert.equal(service.activeJobs, 0);
});

test("future caller timing cannot extend the server budget and success cancels its timer", async (t) => {
  const clock = fakeClock();
  let accepted;
  const service = new ClearraJobService(config({
    platformRequestTimeoutMs: 900_000, platformResponseReserveMs: 15_000,
  }), { async execute(value) { accepted = value; return result(); } }, clock);
  const address = await service.listen();
  t.after(() => service.close());
  const response = await submit(`http://127.0.0.1:${address.port}/jobs`, job("success", clock.now() + 100_000));
  const completed = await response.json();
  assert.equal(completed.state, "completed");
  assert.equal(accepted.platformDeadlineUnixMs, clock.now() + 885_000);
  assert.equal(clock.timers.size, 0);
  clock.advance(900_000);
  assert.deepEqual(service.jobs.get("success").terminal, completed);
});

test("client platform failure wins even if POST and bounded DELETE ignore abort", async () => {
  const clock = fakeClock();
  const post = deferred();
  const calls = [];
  const executor = new ClearraJobExecutor({
    ...clock, platformRequestTimeoutMs: 900_000, platformResponseReserveMs: 15_000,
    cancelTimeoutMs: 2_000, createJobId: () => "client-race",
    fetch(_url, request) {
      calls.push(request);
      return request.method === "POST" ? post.promise : new Promise(() => {});
    },
  });
  const pending = executor.execute(["pc", "path"]);
  const rejected = assert.rejects(pending, { code: PLATFORM_DEADLINE_CODE });
  clock.advance(885_000);
  await flush();
  assert.equal(calls[0].signal.aborted, true);
  assert.equal(calls[1].method, "DELETE");
  clock.advance(2_000);
  await rejected;
  post.resolve(envelope("client-race"));
  await flush();
  assert.equal(calls.length, 2);
  assert.equal(clock.timers.size, 0);
});

test("canonical remote platform failure needs no duplicate cancellation", async () => {
  const requests = [];
  const executor = new ClearraJobExecutor({
    createJobId: () => "failed", fetch: async (_url, request) => {
      requests.push(request.method);
      return envelope("failed", { state: "failed", errorCode: PLATFORM_DEADLINE_CODE, error: "expired" });
    },
  });
  await assert.rejects(executor.execute(["pc"]), { code: PLATFORM_DEADLINE_CODE });
  assert.deepEqual(requests, ["POST"]);
});

test("pre-cancelled clients and a runner cancelled during preparation never submit or spawn", async () => {
  const controller = new AbortController();
  controller.abort();
  const client = new ClearraJobExecutor({ fetch() { assert.fail("must not submit"); } });
  await assert.rejects(client.execute(["pc"], { signal: controller.signal }), { name: "AbortError" });
  const diagnosticStarted = deferred();
  const diagnostic = deferred();
  const preparing = new AbortController();
  const runner = new ClearraCommandRunner(config(), {
    spawn() { assert.fail("must not spawn"); },
    readOomKillCount() { diagnosticStarted.resolve(); return diagnostic.promise; },
  });
  const run = runner.execute(job("preparation"), { signal: preparing.signal });
  const rejected = assert.rejects(run, { name: "AbortError" });
  await diagnosticStarted.promise;
  preparing.abort();
  diagnostic.resolve(null);
  await rejected;
});

test("unlimited queued work remains owned until admission or cancellation", async () => {
  const first = deferred();
  const bot = new Clearrabot({}, { maxConcurrentSearches: 1 }, { executor: {} });
  const active = bot.withSearchSlot(() => first.promise, { deadlineUnixMs: 0 });
  let invoked = false;
  const next = bot.withSearchSlot(async () => { invoked = true; }, { deadlineUnixMs: 0 });
  assert.equal(bot.pendingSearches.length, 1);
  assert.equal(invoked, false);
  first.resolve();
  await Promise.all([active, next]);
  assert.equal(invoked, true);
  assert.equal(bot.activeSearches, 0);
  bot.stop();
});

test("platform failure has complete localized terminal wording", () => {
  assertDiscordCatalogComplete();
  for (const locale of ["en", "ko", "ja"]) {
    const message = operationErrorText(platformDeadlineError(), locale);
    assert.match(message, /Cloud Run/u);
  }
});
