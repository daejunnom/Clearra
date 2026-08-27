import {
  execFile as nodeExecFile,
  spawn as nodeSpawn,
} from "node:child_process";
import { createHash } from "node:crypto";
import {
  lstat,
  mkdtemp,
  readFile,
  realpath,
  rmdir,
  unlink,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";

import {
  assertDiscordCanonicalOnlyResult,
  prepareClearraArguments,
  searchTimeoutMsForArguments,
} from "../clearra/command.mjs";
import { productBuildIdentityMatchesRuntime } from "./runtime-identity.mjs";

export class ClearraCommandRunner {
  constructor(config, options = {}) {
    this.config = config;
    this.spawn = options.spawn ?? nodeSpawn;
    this.execFile = options.execFile ?? nodeExecFile;
    this.now = options.now ?? Date.now;
    this.setTimeout = options.setTimeout ?? setTimeout;
    this.clearTimeout = options.clearTimeout ?? clearTimeout;
  }

  async verifyCapabilities() {
    for (const probe of FINESSE_CAPABILITY_PROBES) {
      const output = await executeCapabilityProbe(
        this.execFile,
        this.config,
        probe.arguments,
      );
      let payload;
      try {
        payload = JSON.parse(output);
      } catch {
        throw capabilityError();
      }
      if (
        payload?.finesse_report?.mode !== probe.mode ||
        payload?.finesse_report?.metric !== "inputs"
      ) {
        throw capabilityError();
      }
      if (
        this.config.runtimeIdentity &&
        !productBuildIdentityMatchesRuntime(
          payload?.runtime_identity,
          this.config.runtimeIdentity,
        )
      ) {
        throw executableIdentityError();
      }
    }
  }

  async execute(job, options = {}) {
    const deadlineRemainingMs = job.deadlineUnixMs - this.now();
    if (deadlineRemainingMs <= 0) {
      return Promise.reject(deadlineError());
    }
    const timeoutMs = Math.min(
      searchTimeoutMsForArguments(
        job.arguments,
        this.config,
        job.timeoutClass,
      ),
      deadlineRemainingMs,
    );
    const maxOutputBytes = Math.min(
      this.config.maxOutputBytes,
      job.maxOutputBytes,
    );
    const maxArtifactBytes = Math.min(
      this.config.maxArtifactBytes ?? this.config.maxOutputBytes,
      job.maxArtifactBytes ?? job.maxOutputBytes,
    );
    const preparedArguments = prepareClearraArguments(job.arguments, {
      workers: this.config.searchWorkersPerSession,
      useAllLogicalProcessors: this.config.useAllLogicalProcessors,
      logicalProcessors: this.config.processLogicalProcessors,
      outputFormat: "json",
      includeSolutionData: true,
    });
    const artifactPlan = await createRenderArtifactPlan(
      preparedArguments,
      maxArtifactBytes,
    );
    const arguments_ = artifactPlan
      ? [...preparedArguments, "--output", artifactPlan.outputPath]
      : preparedArguments;

    try {
      return await new Promise((resolveJob, rejectJob) => {
      let child;
      try {
        const childEnvironment = expectedVcpuEnvironment(
          this.config.expectedVcpus,
        );
        child = this.spawn(this.config.executable, arguments_, {
          shell: false,
          windowsHide: true,
          stdio: ["ignore", "pipe", "pipe"],
          ...(childEnvironment ? { env: childEnvironment } : {}),
        });
      } catch (error) {
        rejectJob(clearraStartError(error));
        return;
      }
      const stdout = [];
      const stderr = [];
      let outputBytes = 0;
      let settled = false;
      let pendingFailure = null;
      let forceKillTimer = null;
      let timeout = null;

      const terminate = () => {
        if (child.exitCode !== null || child.signalCode !== null) return;
        try {
          child.kill("SIGTERM");
        } catch {
          // A concurrent process exit is confirmed by the close event below.
        }
        forceKillTimer = this.setTimeout(() => {
          if (child.exitCode === null && child.signalCode === null) {
            try {
              child.kill("SIGKILL");
            } catch {
              // Keep the slot owned until close confirms process termination.
            }
          }
        }, this.config.terminationGraceMs);
        forceKillTimer.unref?.();
      };
      const finish = (callback) => {
        if (settled) return;
        settled = true;
        if (timeout) this.clearTimeout(timeout);
        if (forceKillTimer) this.clearTimeout(forceKillTimer);
        options.signal?.removeEventListener("abort", abort);
        callback();
      };
      const fail = (error) => {
        if (settled || pendingFailure) return;
        pendingFailure = error;
        if (timeout) this.clearTimeout(timeout);
        options.signal?.removeEventListener("abort", abort);
        terminate();
      };
      const append = (chunks, chunk) => {
        if (settled || pendingFailure) return;
        const bytes = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
        if (outputBytes + bytes.length > maxOutputBytes) {
          fail(new Error("Clearra produced more output than the job allows."));
          return;
        }
        outputBytes += bytes.length;
        chunks.push(bytes);
      };
      const abort = () => fail(abortError("Clearra job was cancelled."));
      timeout = this.setTimeout(() => fail(deadlineError()), timeoutMs);
      timeout.unref?.();

      options.signal?.addEventListener("abort", abort, { once: true });
      child.stdout?.on("data", (chunk) => {
        append(stdout, chunk);
      });
      child.stderr?.on("data", (chunk) => {
        append(stderr, chunk);
      });
      child.on("error", (error) => {
        fail(clearraStartError(error));
      });
      child.on("close", (code, signal) => {
        if (pendingFailure) {
          finish(() => rejectJob(pendingFailure));
          return;
        }
        const stdoutText = Buffer.concat(stdout).toString("utf8").trim();
        const stderrText = Buffer.concat(stderr).toString("utf8").trim();
        void (async () => {
          const artifact = code === 0 && artifactPlan
            ? await readRenderArtifact(artifactPlan, stdoutText)
            : null;
          const sanitizedStdout = artifact
            ? sanitizeRenderStdout(stdoutText)
            : stdoutText;
          finish(() => resolveJob(assertDiscordCanonicalOnlyResult({
            exitCode: code ?? -1,
            signal: signal ?? null,
            stdout: sanitizedStdout,
            stderr: stderrText,
            ...(artifact ? { artifact } : {}),
          })));
        })().catch((error) => {
          finish(() => rejectJob(error));
        });
      });
      if (options.signal?.aborted) abort();
      });
    } finally {
      await cleanupRenderArtifactPlan(artifactPlan);
    }
  }
}

const RENDER_ARTIFACT_CONTRACT = "clearra.discord-render-artifact.v1";
const RENDER_TEMP_PREFIX = "clearra-discord-render-";
const PNG_SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

async function createRenderArtifactPlan(arguments_, maximumBytes) {
  if (arguments_?.[0] !== "utility" || arguments_?.[1] !== "render") return null;
  if (!Number.isSafeInteger(maximumBytes) || maximumBytes < 1) {
    throw new Error("The Discord render artifact limit is invalid.");
  }
  const format = uniqueArgumentValue(arguments_, "--artifact-format");
  if (!new Set(["png", "gif"]).has(format)) return null;
  const directory = await mkdtemp(join(tmpdir(), RENDER_TEMP_PREFIX));
  const canonicalDirectory = await realpath(directory);
  if (resolve(directory) !== resolve(canonicalDirectory)) {
    await rmdir(directory).catch(() => {});
    throw new Error("The Discord render temporary directory is not canonical.");
  }
  return Object.freeze({
    directory,
    canonicalDirectory,
    outputPath: join(directory, `artifact.${format}`),
    format,
    mediaType: format === "png" ? "image/png" : "image/gif",
    maximumBytes,
  });
}

async function readRenderArtifact(plan, stdout) {
  const metadata = await lstat(plan.outputPath).catch(() => null);
  if (
    !metadata ||
    !metadata.isFile() ||
    metadata.isSymbolicLink() ||
    !Number.isSafeInteger(metadata.size) ||
    metadata.size < 1 ||
    metadata.size > plan.maximumBytes
  ) {
    throw new Error("Clearra did not produce one bounded regular render artifact.");
  }
  const bytes = await readFile(plan.outputPath);
  if (bytes.byteLength !== metadata.size || !validRenderSignature(bytes, plan.format)) {
    throw new Error("Clearra produced a render artifact with an invalid binary signature.");
  }
  const digest = createHash("sha256").update(bytes).digest("hex");
  const document = parseRenderStdout(stdout);
  const payload = document.payload;
  if (
    payload.artifact_format !== plan.format ||
    payload.media_type !== plan.mediaType ||
    payload.byte_length !== bytes.byteLength ||
    payload.sha256 !== digest ||
    payload.render_exact !== true ||
    !Number.isSafeInteger(payload.product_max_bytes) ||
    !Number.isSafeInteger(payload.transport_max_bytes) ||
    bytes.byteLength > payload.product_max_bytes ||
    bytes.byteLength > payload.transport_max_bytes ||
    !safeRenderFilename(payload.filename, plan.format)
  ) {
    throw new Error("Clearra render metadata does not identify the produced artifact.");
  }
  return Object.freeze({
    contract: RENDER_ARTIFACT_CONTRACT,
    artifactFormat: plan.format,
    mediaType: plan.mediaType,
    filename: payload.filename,
    byteLength: bytes.byteLength,
    sha256: digest,
    bytesBase64: bytes.toString("base64"),
    renderExact: true,
  });
}

function parseRenderStdout(stdout) {
  let document;
  try {
    document = JSON.parse(stdout);
  } catch {
    throw new Error("Clearra render stdout omitted its typed JSON metadata.");
  }
  if (
    !document ||
    typeof document !== "object" ||
    Array.isArray(document) ||
    document.kind !== "render-artifact.v1" ||
    document.contract_id !== "render-artifact.v1" ||
    document.result_kind !== "render" ||
    document.payload_kind !== "render-artifact" ||
    !document.payload ||
    typeof document.payload !== "object" ||
    Array.isArray(document.payload) ||
    !Array.isArray(document.generated_files) ||
    document.generated_files.length !== 1 ||
    document.generated_files[0]?.target_owned !== true
  ) {
    throw new Error("Clearra render stdout has an invalid typed artifact contract.");
  }
  return document;
}

function sanitizeRenderStdout(stdout) {
  const document = parseRenderStdout(stdout);
  const sanitized = { ...document };
  delete sanitized.generated_files;
  return JSON.stringify(sanitized);
}

function validRenderSignature(bytes, format) {
  if (format === "png") {
    return bytes.byteLength >= PNG_SIGNATURE.byteLength &&
      bytes.subarray(0, PNG_SIGNATURE.byteLength).equals(PNG_SIGNATURE);
  }
  return bytes.byteLength >= 6 &&
    ["GIF87a", "GIF89a"].includes(bytes.subarray(0, 6).toString("ascii"));
}

function safeRenderFilename(value, format) {
  return typeof value === "string" &&
    value.length <= 128 &&
    /^[a-z0-9][a-z0-9._-]*$/i.test(value) &&
    value.toLowerCase().endsWith(`.${format}`) &&
    basename(value) === value;
}

function uniqueArgumentValue(arguments_, option) {
  let found = null;
  for (let index = 0; index < arguments_.length; index += 1) {
    if (arguments_[index] !== option) continue;
    if (found !== null) return null;
    found = arguments_[index + 1] ?? null;
    index += 1;
  }
  return found;
}

async function cleanupRenderArtifactPlan(plan) {
  if (!plan) return;
  try {
    const directoryMetadata = await lstat(plan.directory);
    if (!directoryMetadata.isDirectory() || directoryMetadata.isSymbolicLink()) return;
    const canonical = await realpath(plan.directory);
    if (
      resolve(canonical) !== resolve(plan.canonicalDirectory) ||
      dirname(plan.outputPath) !== plan.directory ||
      !basename(plan.directory).startsWith(RENDER_TEMP_PREFIX)
    ) return;
    await unlink(plan.outputPath).catch((error) => {
      if (error?.code !== "ENOENT") throw error;
    });
    await rmdir(plan.directory);
  } catch (error) {
    if (error?.code !== "ENOENT") {
      throw new Error("The Discord render temporary artifact could not be cleaned up.");
    }
  }
}

const FINESSE_CAPABILITY_PROBES = Object.freeze([
  Object.freeze({
    mode: "search",
    arguments: Object.freeze([
      "finesse", "search",
      "--base-mask", "0",
      "--target-mask", "0xf",
      "--height", "1",
      "--queue", "I",
      "--no-hold",
      "--pattern-knowledge", "oracle",
      "--rule", "srs-plus",
      "--workers", "1",
      "--format", "json",
    ]),
  }),
  Object.freeze({
    mode: "score",
    arguments: Object.freeze([
      "finesse", "score",
      "--initial-mask", "0",
      "--height", "4",
      "--placements", "O:spawn:4:0",
      "--queue", "O",
      "--no-hold",
      "--pattern-knowledge", "both",
      "--rule", "srs-plus",
      "--workers", "1",
      "--format", "json",
    ]),
  }),
]);

function executeCapabilityProbe(execFile, config, arguments_) {
  const timeout = Math.max(
    1,
    Math.min(Number(config.searchTimeoutMs) || 30_000, 30_000),
  );
  const childEnvironment = expectedVcpuEnvironment(config.expectedVcpus);
  return new Promise((resolve, reject) => {
    execFile(
      config.executable,
      arguments_,
      {
        shell: false,
        windowsHide: true,
        timeout,
        maxBuffer: 256 * 1024,
        encoding: "utf8",
        ...(childEnvironment ? { env: childEnvironment } : {}),
      },
      (error, stdout) => {
        if (error || typeof stdout !== "string") {
          reject(capabilityError());
          return;
        }
        resolve(stdout.trim());
      },
    );
  });
}

function capabilityError() {
  return new Error("Clearra engine capability check failed.");
}

function executableIdentityError() {
  return new Error(
    "Clearra executable identity does not match the configured job runtime identity.",
  );
}

function expectedVcpuEnvironment(value) {
  const expected = Number(value);
  if (!Number.isSafeInteger(expected) || expected < 1) return null;
  return {
    ...process.env,
    CLEARRA_EXPECTED_VCPUS: String(expected),
  };
}

function clearraStartError(error) {
  const detail = error instanceof Error ? error.message : String(error);
  return new Error(`Clearra could not start: ${detail}`);
}

function abortError(message) {
  const error = new Error(message);
  error.name = "AbortError";
  return error;
}

function deadlineError() {
  const error = new Error("Clearra job exceeded its execution deadline.");
  error.name = "DeadlineExceededError";
  return error;
}
