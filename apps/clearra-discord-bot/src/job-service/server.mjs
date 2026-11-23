import { createHash, timingSafeEqual } from "node:crypto";
import { createServer } from "node:http";

import {
  assertDiscordNoTieArguments,
  canonicalClearraOperationalCommand,
  searchTimeoutClass,
  searchTimeoutMsForArguments,
} from "../clearra/command.mjs";
import { writeOperationalLog } from "../operational-log.mjs";
import { armDeadline, executionDeadline } from "./execution-deadline.mjs";
import {
  PLATFORM_DEADLINE_CODE,
  PLATFORM_RESPONSE_RESERVE_MS,
  platformDeadlineError,
  platformResponseDeadline,
} from "./platform-deadline.mjs";
import {
  normalizeRuntimeIdentity,
  runtimeIdentityMatches,
} from "./runtime-identity.mjs";

const JOB_PROTOCOL = "clearra.job.v1";
const JOB_ID_PATTERN = /^[A-Za-z0-9._:-]{1,128}$/;

export class ClearraJobService {
  constructor(config, runner, options = {}) {
    this.config = config;
    this.runner = runner;
    this.now = options.now ?? Date.now;
    this.clock = {
      now: this.now,
      setTimeout: options.setTimeout ?? setTimeout,
      clearTimeout: options.clearTimeout ?? clearTimeout,
    };
    this.logger = options.logger ?? console;
    this.operationalScope = options.operationalScope ?? null;
    this.runtimeIdentity = config.runtimeIdentity
      ? normalizeRuntimeIdentity(config.runtimeIdentity)
      : null;
    this.jobs = new Map();
    this.activeJobs = 0;
    this.server = createServer((request, response) => {
      const receivedAt = this.now();
      void this.handle(request, response, receivedAt).catch((error) => {
        if (!response.headersSent) {
          const status = error instanceof RequestError ? error.status : 500;
          sendJson(response, status, {
            protocol: JOB_PROTOCOL,
            state: "failed",
            error: safeErrorMessage(error),
          });
        } else if (!response.writableEnded) {
          response.destroy(error instanceof Error ? error : undefined);
        }
      });
    });
  }

  listen() {
    return new Promise((resolve, reject) => {
      const onError = (error) => reject(error);
      this.server.once("error", onError);
      this.server.listen(this.config.port, this.config.host, () => {
        this.server.removeListener("error", onError);
        resolve(this.server.address());
      });
    });
  }

  async close() {
    for (const entry of this.jobs.values()) {
      if (entry.completedAt === null) entry.controller.abort();
    }
    await Promise.all([
      ...[...this.jobs.values()].map((entry) => entry.execution),
      new Promise((resolve, reject) => {
        this.server.close((error) => (error ? reject(error) : resolve()));
      }),
    ]);
  }

  async handle(request, response, receivedAt = this.now()) {
    this.pruneCompletedJobs();
    const url = new URL(request.url || "/", "http://localhost");
    if (
      request.method === "GET" &&
      (url.pathname === "/health" || url.pathname === "/healthz")
    ) {
      sendJson(response, 200, {
        status: "ok",
        activeJobs: this.activeJobs,
        workerLimit: this.config.searchWorkersPerSession,
        runtime: this.runtimeIdentity,
      });
      return;
    }
    if (!this.authorized(request)) {
      sendJson(response, 401, {
        protocol: JOB_PROTOCOL,
        state: "failed",
        error: "unauthorized",
      });
      return;
    }
    if (request.method === "POST" && url.pathname === "/jobs") {
      await this.handleCreate(request, response, receivedAt);
      return;
    }
    const jobId = jobIdFromPath(url.pathname);
    if (jobId && request.method === "GET") {
      this.handleRead(jobId, response);
      return;
    }
    if (jobId && request.method === "DELETE") {
      this.handleCancel(jobId, response);
      return;
    }
    sendJson(response, 404, {
      protocol: JOB_PROTOCOL,
      state: "failed",
      error: "not_found",
    });
  }

  async handleCreate(request, response, receivedAt = this.now()) {
    const body = await readJsonBody(request, this.config.maxRequestBodyBytes);
    const job = validateJobRequest(body, this.config, this.now(), receivedAt);
    const idempotencyKey = request.headers["idempotency-key"];
    if (idempotencyKey !== job.id) {
      sendJson(response, 400, this.jobEnvelope(job.id, "failed", {
        error: "idempotency-key must match the job ID",
      }));
      return;
    }
    const digest = jobDigest(job);
    const existing = this.jobs.get(job.id);
    if (existing) {
      if (existing.digest !== digest) {
        sendJson(response, 409, this.jobEnvelope(job.id, "failed", {
          error: "job ID already refers to a different request",
        }));
        return;
      }
      const terminal = await existing.promise;
      if (!response.writableEnded) sendJson(response, 200, terminal);
      return;
    }
    if (this.activeJobs >= this.config.maxConcurrentJobs) {
      sendJson(response, 429, this.jobEnvelope(job.id, "failed", {
        error: "job service is at its configured concurrency limit",
      }));
      return;
    }
    if (job.deadlineUnixMs !== 0 && job.deadlineUnixMs <= this.now()) {
      throw new RequestError(400, "job deadline has expired");
    }

    const controller = new AbortController();
    let resolveTerminal;
    const entry = {
      digest,
      state: "running",
      createdAt: this.now(),
      completedAt: null,
      controller,
      promise: new Promise((resolve) => { resolveTerminal = resolve; }),
      resolveTerminal: (terminal) => resolveTerminal(terminal),
      execution: null,
      cancelPlatformDeadline: () => {},
      terminal: null,
    };
    this.jobs.set(job.id, entry);
    this.activeJobs += 1;
    controller.signal.addEventListener("abort", () => {
      this.settleEntry(job, entry, "cancelled");
    }, { once: true });
    entry.cancelPlatformDeadline = armDeadline(job.platformDeadlineUnixMs, () => {
      const error = platformDeadlineError();
      if (this.settleEntry(job, entry, "failed", {
        error: error.message,
        errorCode: PLATFORM_DEADLINE_CODE,
      })) controller.abort();
    }, this.clock);
    entry.execution = this.runJob(job, entry);

    const abortOnDisconnect = () => {
      if (!response.writableEnded && entry.state === "running") {
        controller.abort();
      }
    };
    request.once("aborted", abortOnDisconnect);
    response.once("close", abortOnDisconnect);
    const terminal = await entry.promise;
    request.removeListener("aborted", abortOnDisconnect);
    response.removeListener("close", abortOnDisconnect);
    if (!response.writableEnded && !response.destroyed) {
      sendJson(response, 200, terminal);
    }
  }

  async runJob(job, entry) {
    try {
      if (entry.controller.signal.aborted) return;
      const result = await this.runner.execute(job, {
        signal: entry.controller.signal,
      });
      this.settleEntry(job, entry, "completed", { result });
    } catch (error) {
      if (error?.name === "AbortError") {
        this.settleEntry(job, entry, "cancelled");
      } else {
        this.settleEntry(job, entry, "failed", {
          error: safeErrorMessage(error),
        });
      }
    } finally {
      // A terminal reply does not release the execution slot. The runner owns
      // its child until close (including TERM/KILL and artifact cleanup).
      entry.completedAt = this.now();
      this.activeJobs = Math.max(0, this.activeJobs - 1);
      if (this.operationalScope) {
        writeOperationalLog(this.logger, {
          scope: this.operationalScope,
          kind: "search",
          command: canonicalClearraOperationalCommand(job.arguments),
          timeoutClass: job.timeoutClass,
          timeoutMs: job.timeoutLimitMs,
          status:
            entry.state === "completed"
              ? "succeeded"
              : entry.state === "cancelled"
                ? "cancelled"
                : "failed",
          durationMs: entry.completedAt - entry.createdAt,
        });
      }
    }
    return entry.terminal;
  }

  settleEntry(job, entry, state, additional = {}) {
    if (entry.terminal !== null) return false;
    entry.state = state;
    entry.terminal = this.jobEnvelope(job.id, state, additional);
    entry.cancelPlatformDeadline();
    entry.resolveTerminal(entry.terminal);
    return true;
  }

  handleRead(jobId, response) {
    const entry = this.jobs.get(jobId);
    if (!entry) {
      sendJson(response, 404, this.jobEnvelope(jobId, "failed", {
        error: "job not found on this service instance",
      }));
      return;
    }
    if (entry.terminal) {
      sendJson(response, 200, entry.terminal);
      return;
    }
    sendJson(response, 200, this.jobEnvelope(jobId, entry.state));
  }

  handleCancel(jobId, response) {
    const entry = this.jobs.get(jobId);
    if (!entry) {
      response.writeHead(404).end();
      return;
    }
    if (entry.state !== "running") {
      response.writeHead(409).end();
      return;
    }
    entry.controller.abort();
    response.writeHead(204).end();
  }

  authorized(request) {
    if (this.config.allowUnauthenticated) return true;
    const authorization = request.headers.authorization || "";
    if (!authorization.startsWith("Bearer ")) return false;
    const provided = Buffer.from(authorization.slice(7));
    const expected = Buffer.from(this.config.authorizationToken || "");
    return provided.length === expected.length && timingSafeEqual(provided, expected);
  }

  pruneCompletedJobs() {
    const cutoff = this.now() - this.config.completedJobTtlMs;
    for (const [jobId, entry] of this.jobs) {
      if (entry.completedAt !== null && entry.completedAt < cutoff) {
        this.jobs.delete(jobId);
      }
    }
    if (this.jobs.size <= this.config.maxRetainedJobs) return;
    const completed = [...this.jobs.entries()]
      .filter(([, entry]) => entry.completedAt !== null)
      .sort((left, right) => left[1].completedAt - right[1].completedAt);
    while (this.jobs.size > this.config.maxRetainedJobs && completed.length > 0) {
      const [jobId] = completed.shift();
      this.jobs.delete(jobId);
    }
  }

  jobEnvelope(id, state, additional = {}) {
    return {
      protocol: JOB_PROTOCOL,
      id,
      state,
      runtime: this.runtimeIdentity,
      ...additional,
    };
  }
}

function validateJobRequest(value, config, now, receivedAt) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new RequestError(400, "job request must be a JSON object");
  }
  if (value.protocol !== JOB_PROTOCOL) {
    throw new RequestError(400, "unsupported job protocol");
  }
  if (value.kind !== "clearra.command") {
    throw new RequestError(400, "unsupported job kind");
  }
  if (
    config.runtimeIdentity &&
    !runtimeIdentityMatches(value.expectedRuntime, config.runtimeIdentity)
  ) {
    throw new RequestError(409, "job runtime identity does not match");
  }
  if (typeof value.id !== "string" || !JOB_ID_PATTERN.test(value.id)) {
    throw new RequestError(400, "invalid job ID");
  }
  if (!Array.isArray(value.arguments) || value.arguments.length === 0 || value.arguments.length > 256) {
    throw new RequestError(400, "invalid Clearra argument array");
  }
  let argumentBytes = 0;
  for (const argument of value.arguments) {
    if (typeof argument !== "string" || argument.includes("\0")) {
      throw new RequestError(400, "invalid Clearra argument");
    }
    argumentBytes += Buffer.byteLength(argument);
  }
  try {
    assertDiscordNoTieArguments(value.arguments);
  } catch {
    throw new RequestError(400, "alternative-result paging is unavailable");
  }
  if (argumentBytes > 8192) {
    throw new RequestError(400, "Clearra arguments are too large");
  }
  let timeoutClass;
  try {
    timeoutClass = searchTimeoutClass(value.arguments, value.timeoutClass);
  } catch {
    throw new RequestError(400, "job timeout class does not match its Clearra arguments");
  }
  if (!Number.isSafeInteger(value.deadlineUnixMs) || value.deadlineUnixMs < 0) {
    throw new RequestError(400, "job deadline is invalid");
  }
  if (!Number.isSafeInteger(value.maxOutputBytes) || value.maxOutputBytes < 1) {
    throw new RequestError(400, "invalid job output limit");
  }
  if (!Number.isSafeInteger(value.maxArtifactBytes) || value.maxArtifactBytes < 1) {
    throw new RequestError(400, "invalid job artifact limit");
  }
  const deadlineUnixMs = executionDeadline(
    now,
    searchTimeoutMsForArguments(value.arguments, config, timeoutClass),
    value.deadlineUnixMs,
  );
  const timeoutLimitMs = deadlineUnixMs === 0 ? 0 : Math.max(0, deadlineUnixMs - now);
  const platformTimeoutMs = config.platformRequestTimeoutMs ?? 0;
  if (platformTimeoutMs !== 0 &&
      (!Number.isSafeInteger(value.requestStartedUnixMs) || value.requestStartedUnixMs < 0)) {
    throw new RequestError(400, "a platform request requires its submission timestamp");
  }
  // Submission precedes ingress and therefore includes network/startup/body
  // time. A future client timestamp must never extend the server's own budget.
  const platformDeadlineUnixMs = platformResponseDeadline(
    Math.min(value.requestStartedUnixMs ?? receivedAt, receivedAt),
    platformTimeoutMs,
    config.platformResponseReserveMs ?? PLATFORM_RESPONSE_RESERVE_MS,
  );
  return {
    protocol: JOB_PROTOCOL,
    id: value.id,
    kind: value.kind,
    arguments: [...value.arguments],
    timeoutClass,
    timeoutLimitMs,
    deadlineUnixMs,
    platformDeadlineUnixMs,
    maxOutputBytes: Math.min(value.maxOutputBytes, config.maxOutputBytes),
    maxArtifactBytes: Math.min(
      value.maxArtifactBytes,
      config.maxArtifactBytes ?? config.maxOutputBytes,
    ),
  };
}

async function readJsonBody(request, maxBytes) {
  const declaredLength = Number(request.headers["content-length"]);
  if (Number.isFinite(declaredLength) && declaredLength > maxBytes) {
    throw new RequestError(413, "request body is too large");
  }
  let received = 0;
  const chunks = [];
  for await (const chunk of request) {
    received += chunk.length;
    if (received > maxBytes) {
      throw new RequestError(413, "request body is too large");
    }
    chunks.push(chunk);
  }
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    throw new RequestError(400, "request body is not valid JSON");
  }
}

function jobIdFromPath(pathname) {
  const match = /^\/jobs\/([^/]+)$/.exec(pathname);
  if (!match) return null;
  try {
    const jobId = decodeURIComponent(match[1]);
    return JOB_ID_PATTERN.test(jobId) ? jobId : null;
  } catch {
    return null;
  }
}

function jobDigest(job) {
  // A retry may have less time remaining, but it must still join the original
  // semantic command. The first accepted request continues to own its deadline.
  const {
    deadlineUnixMs: _deadlineUnixMs,
    timeoutLimitMs: _timeoutLimitMs,
    platformDeadlineUnixMs: _platformDeadlineUnixMs,
    ...identity
  } = job;
  return createHash("sha256")
    .update(JSON.stringify(identity))
    .digest("hex");
}

function sendJson(response, status, value) {
  const body = Buffer.from(JSON.stringify(value));
  response.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "content-length": body.length,
    "cache-control": "no-store",
  });
  response.end(body);
}

function safeErrorMessage(error) {
  if (error instanceof RequestError) return error.message;
  if (error instanceof Error && error.message) return error.message.slice(0, 512);
  return "Clearra job service failed unexpectedly.";
}

class RequestError extends Error {
  constructor(status, message) {
    super(message);
    this.name = "RequestError";
    this.status = status;
  }
}
