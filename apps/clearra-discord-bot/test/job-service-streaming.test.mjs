import assert from "node:assert/strict";
import test from "node:test";

import { ClearraJobExecutor } from "../src/clearra/command.mjs";
import { ClearraJobService } from "../src/job-service/server.mjs";

const config = {
  host: "127.0.0.1", port: 0,
  authorizationToken: "job-token", allowUnauthenticated: false,
  maxRequestBodyBytes: 4096, maxOutputBytes: 4096, maxArtifactBytes: 4096,
  searchTimeoutMs: 5000, maxConcurrentJobs: 1,
  completedJobTtlMs: 60_000, maxRetainedJobs: 4,
};

function submit(endpoint, id, signal) {
  return fetch(endpoint, {
    method: "POST", signal,
    headers: {
      authorization: "Bearer job-token",
      "content-type": "application/json",
      "idempotency-key": id,
    },
    body: JSON.stringify({
      protocol: "clearra.job.v1", id, kind: "clearra.command",
      arguments: ["pc", "path", "--patterns", "p7,p4"],
      deadlineUnixMs: Date.now() + 5000,
      maxOutputBytes: 4096, maxArtifactBytes: 4096,
    }),
  });
}

test("job service sends headers and heartbeats while the same search is pending", { timeout: 10_000 }, async (t) => {
  const completed = Promise.withResolvers();
  let heartbeat;
  let heartbeatCleared = false;
  const heartbeatTimer = { unref() {} };
  const service = new ClearraJobService(config, {
    execute: () => completed.promise,
  }, {
    setInterval(callback, milliseconds) {
      assert.equal(milliseconds, 15_000);
      heartbeat = callback;
      return heartbeatTimer;
    },
    clearInterval(timer) {
      assert.equal(timer, heartbeatTimer);
      heartbeatCleared = true;
    },
  });
  const address = await service.listen();
  t.after(async () => {
    completed.resolve({ exitCode: 0, signal: null, stdout: "done", stderr: "" });
    await service.close();
  });

  // A response that waits for the engine would hit Node fetch's independent
  // headers timeout on real long searches. Detect it without a five-minute run.
  const response = await submit(`http://127.0.0.1:${address.port}/jobs`,
    "pending-search", AbortSignal.timeout(5000));
  assert.equal(response.status, 200);
  assert.equal(service.activeJobs, 1);
  assert.equal(response.headers.get("content-length"), null);
  const reader = response.body.getReader();
  const first = await reader.read();
  assert.equal(Buffer.from(first.value).toString(), "\n");
  assert.equal(typeof heartbeat, "function");
  heartbeat();
  const next = await reader.read();
  assert.equal(Buffer.from(next.value).toString(), "\n");
  assert.equal(service.activeJobs, 1);

  completed.resolve({ exitCode: 0, signal: null, stdout: "done", stderr: "" });
  const chunks = [first.value, next.value];
  for (;;) {
    const chunk = await reader.read();
    if (chunk.done) break;
    chunks.push(chunk.value);
  }
  const terminal = JSON.parse(Buffer.concat(chunks).toString());
  assert.equal(terminal.state, "completed");
  assert.equal(terminal.result.stdout, "done");
  assert.equal(service.activeJobs, 0);
  assert.equal(heartbeatCleared, true);
});

test("streaming retries share ownership and a disconnect retains the slot until draining", { timeout: 10_000 }, async (t) => {
  const drained = Promise.withResolvers();
  const aborted = Promise.withResolvers();
  const timers = [];
  const owner = new AbortController();
  let executions = 0;
  let executionSignal;
  const service = new ClearraJobService(config, {
    execute(_job, { signal }) {
      executions += 1;
      executionSignal = signal;
      signal.addEventListener("abort", () => aborted.resolve(), { once: true });
      return drained.promise;
    },
  }, {
    setInterval() {
      const timer = { unref() {}, cleared: Promise.withResolvers() };
      timers.push(timer);
      return timer;
    },
    clearInterval(timer) { timer.cleared.resolve(); },
  });
  const address = await service.listen();
  t.after(async () => {
    owner.abort();
    drained.reject(Object.assign(new Error("cancelled"), { name: "AbortError" }));
    await service.close();
  });
  const endpoint = `http://127.0.0.1:${address.port}/jobs`;
  const ownerResponse = await submit(endpoint, "shared-search",
    AbortSignal.any([owner.signal, AbortSignal.timeout(5000)]));
  assert.equal(ownerResponse.status, 200);
  const entry = service.jobs.get("shared-search");
  const retryResponse = await submit(endpoint, "shared-search", AbortSignal.timeout(5000));
  assert.equal(retryResponse.status, 200);
  assert.equal(executions, 1);
  assert.equal(service.activeJobs, 1);

  const rejected = await submit(endpoint, "another-search", AbortSignal.timeout(5000));
  assert.equal(rejected.status, 429);
  assert.match((await rejected.json()).error, /concurrency limit/);

  await retryResponse.body.cancel();
  await timers[1].cleared.promise;
  assert.equal(executionSignal.aborted, false, "disconnecting a joined retry cannot cancel its owner");
  owner.abort();
  await aborted.promise;
  assert.equal(service.activeJobs, 1, "the process still owns its slot until it drains");
  await timers[0].cleared.promise;
  drained.reject(Object.assign(new Error("cancelled"), { name: "AbortError" }));
  assert.equal((await entry.promise).state, "cancelled");
  assert.equal(service.activeJobs, 0);
});

test("the existing executor reads a failed terminal result after successful streaming headers", { timeout: 10_000 }, async (t) => {
  const completed = Promise.withResolvers();
  const headersReceived = Promise.withResolvers();
  const service = new ClearraJobService(config, { execute: () => completed.promise });
  const address = await service.listen();
  t.after(async () => {
    completed.resolve({ exitCode: 0, signal: null, stdout: "done", stderr: "" });
    await service.close();
  });
  const executor = new ClearraJobExecutor({
    endpoint: `http://127.0.0.1:${address.port}/jobs`,
    authorizationToken: "job-token", timeoutMs: 5000,
    fetch: async (url, options) => {
      const response = await fetch(url, options);
      if (options.method === "POST") {
        assert.equal(response.status, 200);
        headersReceived.resolve();
      }
      return response;
    },
  });
  const execution = executor.execute(["pc", "path", "--patterns", "p7,p4"]);
  const rejected = assert.rejects(execution, (error) => {
    assert.equal(error.name, "DeadlineExceededError");
    assert.equal(error.failureCode, "deadline");
    assert.equal(error.failureStage, "execute");
    assert.deepEqual(error.stageDurationsMs, { prepare: 2, execute: 37 });
    return true;
  });
  await headersReceived.promise;
  completed.reject(Object.assign(new Error("execution deadline expired"), {
    name: "DeadlineExceededError", failureCode: "deadline", failureStage: "execute",
    stageDurationsMs: { prepare: 2, execute: 37 },
  }));
  await rejected;
  assert.equal(service.activeJobs, 0);
});
