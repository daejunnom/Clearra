import { spawn as nodeSpawn } from "node:child_process";

import { prepareClearraArguments } from "../clearra/command.mjs";

export class ClearraCommandRunner {
  constructor(config, options = {}) {
    this.config = config;
    this.spawn = options.spawn ?? nodeSpawn;
    this.now = options.now ?? Date.now;
  }

  execute(job, options = {}) {
    const deadlineRemainingMs = job.deadlineUnixMs - this.now();
    if (deadlineRemainingMs <= 0) {
      return Promise.reject(deadlineError());
    }
    const timeoutMs = Math.min(
      this.config.searchTimeoutMs,
      deadlineRemainingMs,
    );
    const maxOutputBytes = Math.min(
      this.config.maxOutputBytes,
      job.maxOutputBytes,
    );
    const arguments_ = prepareClearraArguments(job.arguments, {
      workers: this.config.searchWorkersPerSession,
      useAllLogicalProcessors: this.config.useAllLogicalProcessors,
      logicalProcessors: this.config.processLogicalProcessors,
      outputFormat: "json",
      includeSolutionData: true,
    });

    return new Promise((resolve, reject) => {
      let child;
      try {
        child = this.spawn(this.config.executable, arguments_, {
          shell: false,
          windowsHide: true,
          stdio: ["ignore", "pipe", "pipe"],
        });
      } catch (error) {
        reject(clearraStartError(error));
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
        forceKillTimer = setTimeout(() => {
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
        if (timeout) clearTimeout(timeout);
        if (forceKillTimer) clearTimeout(forceKillTimer);
        options.signal?.removeEventListener("abort", abort);
        callback();
      };
      const fail = (error) => {
        if (settled || pendingFailure) return;
        pendingFailure = error;
        if (timeout) clearTimeout(timeout);
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
      timeout = setTimeout(() => fail(deadlineError()), timeoutMs);
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
          finish(() => reject(pendingFailure));
          return;
        }
        finish(() => resolve({
          exitCode: code ?? -1,
          signal: signal ?? null,
          stdout: Buffer.concat(stdout).toString("utf8").trim(),
          stderr: Buffer.concat(stderr).toString("utf8").trim(),
        }));
      });
      if (options.signal?.aborted) abort();
    });
  }
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
