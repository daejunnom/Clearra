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
    });

    return new Promise((resolve, reject) => {
      const child = this.spawn(this.config.executable, arguments_, {
        shell: false,
        windowsHide: true,
        stdio: ["ignore", "pipe", "pipe"],
      });
      let stdout = "";
      let stderr = "";
      let outputBytes = 0;
      let settled = false;
      let timedOut = false;
      let forceKillTimer = null;

      const terminate = () => {
        if (child.exitCode !== null || child.signalCode !== null) return;
        child.kill("SIGTERM");
        forceKillTimer = setTimeout(() => {
          if (child.exitCode === null && child.signalCode === null) {
            child.kill("SIGKILL");
          }
        }, this.config.terminationGraceMs);
        forceKillTimer.unref?.();
      };
      const finish = (callback, keepForceKillTimer = false) => {
        if (settled) return;
        settled = true;
        clearTimeout(timeout);
        if (forceKillTimer && !keepForceKillTimer) clearTimeout(forceKillTimer);
        options.signal?.removeEventListener("abort", abort);
        callback();
      };
      const fail = (error) => {
        terminate();
        finish(() => reject(error), true);
      };
      const append = (current, chunk) => {
        outputBytes += chunk.length;
        if (outputBytes > maxOutputBytes) {
          fail(new Error("Clearra produced more output than the job allows."));
          return current;
        }
        return current + chunk.toString("utf8");
      };
      const abort = () => fail(abortError("Clearra job was cancelled."));
      const timeout = setTimeout(() => {
        timedOut = true;
        fail(deadlineError());
      }, timeoutMs);
      timeout.unref?.();

      options.signal?.addEventListener("abort", abort, { once: true });
      if (options.signal?.aborted) {
        abort();
        return;
      }
      child.stdout?.on("data", (chunk) => {
        stdout = append(stdout, chunk);
      });
      child.stderr?.on("data", (chunk) => {
        stderr = append(stderr, chunk);
      });
      child.on("error", (error) => {
        fail(new Error(`Clearra could not start: ${error.message}`));
      });
      child.on("close", (code, signal) => {
        if (timedOut || settled) return;
        finish(() =>
          resolve({
            exitCode: code ?? -1,
            signal: signal ?? null,
            stdout: stdout.trim(),
            stderr: stderr.trim(),
          }),
        );
      });
    });
  }
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
