import { randomUUID } from "node:crypto";

const JOB_PROTOCOL = "clearra.job.v1";
const DEFAULT_JOB_ENDPOINT = "http://127.0.0.1:8787/jobs";
const TERMINAL_JOB_STATES = new Set(["completed", "failed", "cancelled"]);

const NATIVE_COMMANDS = new Set([
  "pc",
  "failed-queue",
  "setup",
  "setup-finder",
  "path",
  "pc-replay",
  "percent",
  "cover",
  "build-coverage",
  "build-probability",
  "damage",
  "spin-finder",
  "spin-structure",
  "finesse",
]);
const SFINDER_SEARCH_COMMANDS = new Set([
  "path",
  "chance",
  "percent",
  "minimals",
  "score",
  "score-minimals",
  "saves",
  "best-save",
  "cover",
  "setup",
  "congruent",
  "congruent-cover",
  "setup-cover",
  "cover-percent",
  "special-cover",
  "spin-cover",
  "spin",
  "score-finder",
  "pc-setup",
  "best-setup",
  "dpc-finder",
]);
const SFINDER_COMMANDS = new Set([...SFINDER_SEARCH_COMMANDS, "verify"]);
const ALLOWED_COMMANDS = new Set([...NATIVE_COMMANDS, "sfinder"]);
const PARALLEL_SEARCH_COMMANDS = new Set([
  "pc",
  "failed-queue",
  "setup",
  "setup-finder",
  "path",
  "pc-replay",
  "build-probability",
  "damage",
  "spin-finder",
  "spin-structure",
  "finesse",
]);

/**
 * Returns the canonical, input-free command path that is safe to retain in an
 * operational record. The array form accepts a prepared Clearra argv; the
 * string form accepts only an already reduced command path.
 */
export function canonicalClearraOperationalCommand(value) {
  const pathInput = typeof value === "string";
  const tokens = Array.isArray(value)
    ? value
    : pathInput
      ? value.split(".")
      : [];
  const command = normalizedOperationalPart(tokens[0]);
  if (!command) return null;
  if (command === "finesse") {
    if (pathInput && tokens.length !== 2) return null;
    const subcommand = normalizedOperationalPart(tokens[1]);
    return subcommand === "search" || subcommand === "score"
      ? `finesse.${subcommand}`
      : null;
  }
  if (NATIVE_COMMANDS.has(command)) {
    return !pathInput || tokens.length === 1 ? command : null;
  }
  if (command !== "sfinder" || (pathInput && tokens.length !== 2)) return null;
  const subcommand = normalizedOperationalPart(tokens[1]);
  if (!subcommand) return null;
  const canonical = normalizeSfinderCommand(subcommand);
  return SFINDER_COMMANDS.has(canonical) ? `sfinder.${canonical}` : null;
}
const FILE_OPTIONS = new Set([
  "--fixture",
  "--file",
  "--input",
  "--output",
  "--output-base",
  "--template-file",
  "--field-path",
  "--patterns-path",
  "--log-path",
  "--wgsl",
  "--kick-profile-json",
  "--document",
  "-fp",
  "-pp",
  "-lp",
  "-o",
]);
const CONTROLLED_OPTIONS = new Map([
  ["--tablebase", 0],
  ["--no-tablebase", 0],
  ["--tb", 0],
  ["--no-tb", 0],
  ["--build-dependency-dag", 0],
  ["--no-build-dependency-dag", 0],
  ["--workers", 1],
  ["--auto-workers", 1],
  ["--cpu-threads", 1],
  ["--use-all-cpu-threads", 0],
  ["--format", 1],
  ["--include-solution-data", 0],
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
    throw new Error(
      "Discord supports curated Clearra PC, build, setup, coverage, forward, and sfinder searches.",
    );
  }
  const sfinderCommand = command === "sfinder"
    ? validateSfinderCommand(tokens[1])
    : null;
  if (command === "finesse" && !["search", "score"].includes(tokens[1]?.toLowerCase())) {
    throw new Error("Discord finesse calculations require a search or score subcommand.");
  }

  const output = [command];
  for (let index = 1; index < tokens.length; index += 1) {
    const token = tokens[index];
    const normalizedToken = token.toLowerCase();
    const equalsIndex = normalizedToken.indexOf("=");
    const optionName = equalsIndex < 0
      ? normalizedToken
      : normalizedToken.slice(0, equalsIndex);
    if (FILE_OPTIONS.has(optionName)) {
      throw new Error("File and custom-code inputs are not available through Discord.");
    }
    const controlledWidth = CONTROLLED_OPTIONS.get(optionName);
    if (controlledWidth !== undefined) {
      if (equalsIndex < 0) index += controlledWidth;
      continue;
    }
    output.push(token);
  }

  if (command === "pc" || command === "failed-queue") {
    output.push("--no-tablebase", "--no-build-dependency-dag");
  } else if (command === "setup" || command === "setup-finder") {
    output.push("--no-tablebase");
  }
  const parallelSearch = PARALLEL_SEARCH_COMMANDS.has(command) ||
    (command === "sfinder" && SFINDER_SEARCH_COMMANDS.has(sfinderCommand));
  if (parallelSearch) {
    if (execution.workers !== undefined) {
      const workers = Number(execution.workers);
      if (!Number.isSafeInteger(workers) || workers < 1) {
        throw new Error("Clearrabot received an invalid search worker allocation.");
      }
      if (execution.logicalProcessors !== undefined) {
        const logicalProcessors = Number(execution.logicalProcessors);
        if (!Number.isSafeInteger(logicalProcessors) || logicalProcessors < 1) {
          throw new Error("Clearrabot received an invalid logical processor limit.");
        }
        if (workers > logicalProcessors) {
          throw new Error(
            `Clearrabot worker allocation exceeds the hard limit of ${logicalProcessors} logical processors.`,
          );
        }
      }
      output.push("--auto-workers", String(workers));
    }
    if (execution.useAllLogicalProcessors) {
      output.push("--use-all-cpu-threads");
    }
  }
  const outputFormat = execution.outputFormat ?? "text";
  if (outputFormat !== "text" && outputFormat !== "json") {
    throw new Error("Clearrabot received an invalid output format policy.");
  }
  output.push("--format", outputFormat);
  if (execution.includeSolutionData === true) {
    output.push("--include-solution-data");
  }
  return output;
}

function validateSfinderCommand(value) {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error("Discord sfinder searches require a subcommand.");
  }
  const command = normalizeSfinderCommand(value);
  if (!SFINDER_COMMANDS.has(command)) {
    throw new Error(`Discord does not expose the sfinder '${command}' contract.`);
  }
  return command;
}

function normalizeSfinderCommand(value) {
  const normalized = value.trim().toLowerCase().replaceAll("_", "-");
  return ({
    bestsave: "best-save",
    bestsetup: "best-setup",
    congruentcover: "congruent-cover",
    coverpercent: "cover-percent",
    dpcfinder: "dpc-finder",
    pcsetup: "pc-setup",
    scoreminimals: "score-minimals",
    setupcover: "setup-cover",
    specialcover: "special-cover",
    spincover: "spin-cover",
  })[normalized] ?? normalized;
}

function normalizedOperationalPart(value) {
  if (typeof value !== "string") return null;
  const normalized = value.trim().toLowerCase().replaceAll("_", "-");
  return /^[a-z0-9][a-z0-9-]{0,31}$/.test(normalized)
    ? normalized
    : null;
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
    if (!this.authorizationToken && !isLoopbackHostname(this.endpoint.hostname)) {
      throw new Error("A remote Clearra job endpoint requires an authorization token.");
    }
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
    const startedAt = Date.now();
    const requestedDeadlineUnixMs = options.deadlineUnixMs === undefined
      ? startedAt + this.timeoutMs
      : validAbsoluteDeadline(options.deadlineUnixMs);
    const deadlineUnixMs = Math.min(
      requestedDeadlineUnixMs,
      startedAt + this.timeoutMs,
    );
    const remainingMs = deadlineUnixMs - startedAt;
    if (remainingMs <= 0) {
      throw new Error("Clearra interaction deadline expired before submission.");
    }
    let timedOut = false;
    let submitted = false;
    const abort = () => controller.abort();
    const timeout = setTimeout(() => {
      timedOut = true;
      controller.abort();
    }, remainingMs);
    options.signal?.addEventListener("abort", abort, { once: true });

    try {
      if (options.signal?.aborted) controller.abort();
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
  if (url.username || url.password) {
    throw new Error("Clearra job endpoint must not contain credentials.");
  }
  if (url.protocol === "http:" && !isLoopbackHostname(url.hostname)) {
    throw new Error("Clearra job endpoint must use HTTPS unless it targets loopback.");
  }
  url.hash = "";
  return url;
}

function isLoopbackHostname(hostname) {
  const normalized = String(hostname).toLowerCase();
  return normalized === "localhost" ||
    normalized === "::1" ||
    normalized === "[::1]" ||
    /^127(?:\.\d{1,3}){3}$/.test(normalized);
}

function positiveExecutorOption(value, fallback) {
  const parsed = value ?? fallback;
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error("Clearrabot received an invalid job executor setting.");
  }
  return parsed;
}

function validAbsoluteDeadline(value) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error("Clearrabot received an invalid interaction deadline.");
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
