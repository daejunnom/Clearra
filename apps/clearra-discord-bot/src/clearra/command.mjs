import { randomUUID } from "node:crypto";

const JOB_PROTOCOL = "clearra.job.v1";
const DEFAULT_JOB_ENDPOINT = "http://127.0.0.1:8787/jobs";
const TERMINAL_JOB_STATES = new Set(["completed", "failed", "cancelled"]);

const ALLOWED_COMMANDS = new Set(["pc", "setup", "path", "percent", "cover"]);
const PARALLEL_SEARCH_COMMANDS = new Set(["pc", "setup", "path"]);
const FILE_OPTIONS = new Set([
  "--fixture",
  "--input",
  "--output",
  "--wgsl",
  "--kick-profile-json",
]);
const CONTROLLED_OPTIONS = new Map([
  ["--tablebase", 0],
  ["--no-tablebase", 0],
  ["--build-dependency-dag", 0],
  ["--no-build-dependency-dag", 0],
  ["--workers", 1],
  ["--auto-workers", 1],
  ["--cpu-threads", 1],
  ["--use-all-cpu-threads", 0],
  ["--format", 1],
]);

export function parseClearraMessage(content, prefix = "!", execution = {}) {
  const trimmed = content.trim();
  if (!trimmed.startsWith(prefix)) return null;
  const body = trimmed.slice(prefix.length).trim();
  if (!body) return null;
  const tokens = tokenizeCommand(body);
  const first = tokens[0]?.toLowerCase();
  if (first === "clearra") tokens.shift();
  else if (!ALLOWED_COMMANDS.has(first)) return null;
  return prepareClearraArguments(tokens, execution);
}

export function prepareClearraArguments(tokens, execution = {}) {
  if (!Array.isArray(tokens) || tokens.length === 0) {
    throw new Error("Enter a Clearra command.");
  }
  if (tokens.length > 256) throw new Error("The command has too many arguments.");
  const command = tokens[0].toLowerCase();
  if (!ALLOWED_COMMANDS.has(command)) {
    throw new Error("Discord supports pc, setup, path, percent, and cover searches.");
  }

  const output = [command];
  for (let index = 1; index < tokens.length; index += 1) {
    const token = tokens[index];
    if (FILE_OPTIONS.has(token.toLowerCase())) {
      throw new Error("File and custom-code inputs are not available through Discord.");
    }
    const controlledWidth = CONTROLLED_OPTIONS.get(token.toLowerCase());
    if (controlledWidth !== undefined) {
      index += controlledWidth;
      continue;
    }
    output.push(token);
  }

  if (command === "pc") {
    output.push("--no-tablebase", "--no-build-dependency-dag");
  } else if (command === "setup") {
    output.push("--no-tablebase");
  }
  if (PARALLEL_SEARCH_COMMANDS.has(command) && execution.workers !== undefined) {
    const workers = Number(execution.workers);
    if (!Number.isSafeInteger(workers) || workers < 1) {
      throw new Error("Clearrabot received an invalid search worker allocation.");
    }
    output.push("--auto-workers", String(workers));
    if (execution.useAllLogicalProcessors) {
      output.push("--use-all-cpu-threads");
    }
  }
  output.push("--format", "text");
  return output;
}

export function tilingOnlyRequested(arguments_) {
  if (!Array.isArray(arguments_)) return false;
  for (let index = 0; index < arguments_.length; index += 1) {
    const token = String(arguments_[index]).toLowerCase();
    if (token === "--tiling-only") return true;
    if (
      token === "--objective" &&
      String(arguments_[index + 1] ?? "").toLowerCase() === "tiling"
    ) {
      return true;
    }
  }
  return false;
}

export function tokenizeCommand(source) {
  if (source.length > 8192) throw new Error("The command is too long.");
  const tokens = [];
  let token = "";
  let quote = null;
  let escaped = false;

  for (const character of source) {
    if (escaped) {
      token += character;
      escaped = false;
      continue;
    }
    if (character === "\\") {
      escaped = true;
      continue;
    }
    if (quote) {
      if (character === quote) quote = null;
      else token += character;
      continue;
    }
    if (character === "'" || character === '"') {
      quote = character;
      continue;
    }
    if (/\s/.test(character)) {
      if (token) {
        tokens.push(token);
        token = "";
      }
      continue;
    }
    token += character;
  }
  if (escaped) token += "\\";
  if (quote) throw new Error("The command contains an unterminated quote.");
  if (token) tokens.push(token);
  return tokens;
}

export class ClearraJobExecutor {
  constructor(options = {}) {
    this.endpoint = normalizeJobEndpoint(
      options.endpoint ?? DEFAULT_JOB_ENDPOINT,
    );
    this.authorizationToken = options.authorizationToken ?? null;
    this.timeoutMs = positiveExecutorOption(options.timeoutMs, 3 * 60_000);
    this.maxOutputBytes = positiveExecutorOption(
      options.maxOutputBytes,
      4 * 1024 * 1024,
    );
    this.pollIntervalMs = positiveExecutorOption(options.pollIntervalMs, 250);
    this.cancelTimeoutMs = positiveExecutorOption(options.cancelTimeoutMs, 2_000);
    this.fetch = options.fetch ?? globalThis.fetch?.bind(globalThis);
    this.createJobId = options.createJobId ?? randomUUID;
    if (typeof this.fetch !== "function") {
      throw new Error("Clearrabot requires an HTTP fetch implementation.");
    }
  }

  async execute(arguments_, options = {}) {
    validateJobArguments(arguments_);
    const jobId = String(options.jobId ?? this.createJobId());
    if (!jobId || jobId.length > 128) {
      throw new Error("Clearrabot could not allocate a valid job ID.");
    }

    const controller = new AbortController();
    let timedOut = false;
    let submitted = false;
    const abort = () => controller.abort();
    const timeout = setTimeout(() => {
      timedOut = true;
      controller.abort();
    }, this.timeoutMs);
    options.signal?.addEventListener("abort", abort, { once: true });

    try {
      if (options.signal?.aborted) controller.abort();
      const deadlineUnixMs = Date.now() + this.timeoutMs;
      submitted = true;
      let response = await this.request(this.endpoint, {
        method: "POST",
        jobId,
        signal: controller.signal,
        body: JSON.stringify({
          protocol: JOB_PROTOCOL,
          id: jobId,
          kind: "clearra.command",
          arguments: [...arguments_],
          deadlineUnixMs,
          maxOutputBytes: this.maxOutputBytes,
        }),
      });
      let job = await readJobResponse(response, this.maxOutputBytes);
      validateJobIdentity(job, jobId);

      while (!TERMINAL_JOB_STATES.has(job.state)) {
        if (job.state !== "running" && job.state !== "accepted") {
          throw new Error("Clearra job service returned an invalid pending state.");
        }
        await abortableDelay(this.pollIntervalMs, controller.signal);
        response = await this.request(this.jobUrl(jobId), {
          method: "GET",
          jobId,
          signal: controller.signal,
        });
        job = await readJobResponse(response, this.maxOutputBytes);
        validateJobIdentity(job, jobId);
      }

      return terminalJobResult(job, this.maxOutputBytes);
    } catch (error) {
      if (submitted) await this.cancel(jobId);
      if (controller.signal.aborted) {
        if (timedOut) {
          throw new Error(
            `Clearrabot search exceeded the ${timeoutLabel(this.timeoutMs)} time limit.`,
          );
        }
        throw abortError("Clearra search was cancelled.");
      }
      if (error instanceof Error) throw error;
      throw new Error("Clearra job service request failed.");
    } finally {
      clearTimeout(timeout);
      options.signal?.removeEventListener("abort", abort);
    }
  }

  async request(url, options) {
    let response;
    try {
      response = await this.fetch(url, {
        method: options.method,
        headers: this.headers(options.jobId, options.body !== undefined),
        body: options.body,
        signal: options.signal,
        cache: "no-store",
        redirect: "error",
      });
    } catch (error) {
      if (options.signal?.aborted) throw error;
      const detail = error instanceof Error ? `: ${error.message}` : "";
      throw new Error(`Clearra job service could not be reached${detail}`);
    }
    if (!response.ok) {
      const detail = await readBoundedText(response, 16 * 1024).catch(() => "");
      const suffix = detail ? `: ${detail.slice(0, 512)}` : "";
      throw new Error(
        `Clearra job service rejected the request (${response.status})${suffix}`,
      );
    }
    return response;
  }

  headers(jobId, hasBody) {
    const headers = {
      accept: "application/json",
      "idempotency-key": jobId,
    };
    if (hasBody) headers["content-type"] = "application/json";
    if (this.authorizationToken) {
      headers.authorization = `Bearer ${this.authorizationToken}`;
    }
    return headers;
  }

  jobUrl(jobId) {
    const url = new URL(this.endpoint);
    url.pathname = `${url.pathname.replace(/\/$/, "")}/${encodeURIComponent(jobId)}`;
    return url;
  }

  async cancel(jobId) {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), this.cancelTimeoutMs);
    try {
      const response = await this.fetch(this.jobUrl(jobId), {
        method: "DELETE",
        headers: this.headers(jobId, false),
        signal: controller.signal,
        cache: "no-store",
        redirect: "error",
      });
      if (!response.ok && response.status !== 404 && response.status !== 409) {
        throw new Error(`job cancellation returned ${response.status}`);
      }
    } catch {
      // The submitted deadline remains the service-side fail-close boundary.
    } finally {
      clearTimeout(timeout);
    }
  }
}

function normalizeJobEndpoint(value) {
  let url;
  try {
    url = new URL(String(value));
  } catch {
    throw new Error("Clearrabot received an invalid Clearra job endpoint.");
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("Clearra job endpoint must use HTTP or HTTPS.");
  }
  url.hash = "";
  return url;
}

function positiveExecutorOption(value, fallback) {
  const parsed = value ?? fallback;
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error("Clearrabot received an invalid job executor setting.");
  }
  return parsed;
}

function validateJobArguments(arguments_) {
  if (!Array.isArray(arguments_) || arguments_.length === 0) {
    throw new Error("Clearrabot cannot submit an empty Clearra job.");
  }
  for (const argument of arguments_) {
    if (typeof argument !== "string" || argument.includes("\0")) {
      throw new Error("Clearrabot received an invalid Clearra job argument.");
    }
  }
}

async function readJobResponse(response, maxOutputBytes) {
  const text = await readBoundedText(
    response,
    maxOutputBytes * 6 + 64 * 1024,
  );
  let value;
  try {
    value = JSON.parse(text);
  } catch {
    throw new Error("Clearra job service returned invalid JSON.");
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Clearra job service returned an invalid job response.");
  }
  if (value.protocol !== JOB_PROTOCOL) {
    throw new Error("Clearra job service protocol is not compatible with Clearrabot.");
  }
  if (typeof value.state !== "string") {
    throw new Error("Clearra job service omitted the job state.");
  }
  return value;
}

async function readBoundedText(response, maxBytes) {
  const declaredLength = Number(response.headers.get("content-length"));
  if (Number.isFinite(declaredLength) && declaredLength > maxBytes) {
    throw new Error("Clearra produced too much Discord output.");
  }
  if (!response.body) return "";

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let received = 0;
  let text = "";
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      received += value.byteLength;
      if (received > maxBytes) {
        await reader.cancel();
        throw new Error("Clearra produced too much Discord output.");
      }
      text += decoder.decode(value, { stream: true });
    }
    text += decoder.decode();
    return text;
  } finally {
    reader.releaseLock();
  }
}

function validateJobIdentity(job, expectedId) {
  if (job.id !== expectedId) {
    throw new Error("Clearra job service returned a mismatched job ID.");
  }
}

function terminalJobResult(job, maxOutputBytes) {
  if (job.state === "cancelled") {
    throw abortError("Clearra job service cancelled the search.");
  }
  if (job.state === "failed") {
    const message = typeof job.error === "string" ? job.error : "remote job failed";
    throw new Error(`Clearra job service failed the search: ${message}`);
  }
  const result = job.result;
  if (!result || typeof result !== "object" || Array.isArray(result)) {
    throw new Error("Clearra job service omitted the completed result.");
  }
  if (!Number.isSafeInteger(result.exitCode)) {
    throw new Error("Clearra job service returned an invalid exit code.");
  }
  const stdout = typeof result.stdout === "string" ? result.stdout : "";
  const stderr = typeof result.stderr === "string" ? result.stderr : "";
  if (Buffer.byteLength(stdout) + Buffer.byteLength(stderr) > maxOutputBytes) {
    throw new Error("Clearra produced too much Discord output.");
  }
  if (result.signal !== null && result.signal !== undefined && typeof result.signal !== "string") {
    throw new Error("Clearra job service returned an invalid process signal.");
  }
  return {
    exitCode: result.exitCode,
    signal: result.signal ?? null,
    stdout: stdout.trim(),
    stderr: stderr.trim(),
  };
}

function abortableDelay(milliseconds, signal) {
  return new Promise((resolve, reject) => {
    const abort = () => {
      clearTimeout(timeout);
      reject(signal.reason ?? abortError("Clearra search was cancelled."));
    };
    const timeout = setTimeout(() => {
      signal.removeEventListener("abort", abort);
      resolve();
    }, milliseconds);
    signal.addEventListener("abort", abort, { once: true });
    if (signal.aborted) abort();
  });
}

function timeoutLabel(milliseconds) {
  if (milliseconds % 60_000 === 0) return `${milliseconds / 60_000}-minute`;
  if (milliseconds % 1_000 === 0) return `${milliseconds / 1_000}-second`;
  return `${milliseconds}-millisecond`;
}

function abortError(message) {
  const error = new Error(message);
  error.name = "AbortError";
  return error;
}
