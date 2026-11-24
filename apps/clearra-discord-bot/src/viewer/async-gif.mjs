import { Worker } from "node:worker_threads";
import { assertRequestActive, requestDeadlineError } from "../clearra/request-deadline.mjs";

const DEFAULT_TIMEOUT_MS = 10_000;
const DEFAULT_MAX_PENDING = 8;

export class BoundedGifRenderer {
  constructor(options = {}) {
    this.timeoutMs = positiveInteger(options.timeoutMs, DEFAULT_TIMEOUT_MS);
    this.maxPending = nonNegativeInteger(options.maxPending, DEFAULT_MAX_PENDING);
    this.createWorker = options.createWorker ?? createGifWorker;
    this.active = null;
    this.pending = [];
    this.stopped = false;
  }

  render(document, options = {}) {
    if (this.stopped) {
      return Promise.reject(renderAbortError("The GIF renderer has stopped."));
    }
    if (this.active && this.pending.length >= this.maxPending) {
      return Promise.reject(new Error("The GIF renderer queue is full."));
    }
    try { assertRequestActive(options); } catch (error) { return Promise.reject(error); }
    return new Promise((resolve, reject) => {
      const job = { document, options, resolve, reject, settled: false,
        deadlineTimer: null, abort: null };
      job.abort = () => this.#settle(job, renderAbortError("The GIF render was cancelled."));
      options.signal?.addEventListener("abort", job.abort, { once: true });
      if (options.deadlineUnixMs !== undefined) {
        job.deadlineTimer = setTimeout(() => this.#settle(job, requestDeadlineError()),
          Math.max(1, options.deadlineUnixMs - Date.now()));
        job.deadlineTimer.unref?.();
      }
      if (this.active) this.pending.push(job);
      else this.#start(job);
      if (options.signal?.aborted) job.abort();
    });
  }

  stop() {
    if (this.stopped) return;
    this.stopped = true;
    for (const job of this.pending.splice(0)) {
      this.#settle(job, renderAbortError("The GIF renderer has stopped."));
    }
    const active = this.active;
    if (!active) return;
    this.#settle(active.job, renderAbortError("The GIF renderer has stopped."));
  }

  #start(job) {
    let worker;
    try {
      assertRequestActive(job.options);
      const { signal: _signal, deadlineUnixMs: _deadline, stageDurationsMs: _timings,
        ...renderOptions } = job.options;
      worker = this.createWorker(job.document, renderOptions);
    } catch (error) {
      this.#settle(job, error?.name === "DeadlineExceededError" || error?.name === "AbortError"
        ? error : renderWorkerError(error));
      return;
    }

    const active = {
      job,
      worker,
      timeout: null,
    };
    this.active = active;
    worker.once("message", (message) => {
      try { assertRequestActive(job.options); } catch (error) { this.#settle(job, error); return; }
      if (message?.ok === true && message.bytes instanceof Uint8Array) {
        this.#settle(job, null, message.bytes);
        return;
      }
      const error = new Error(typeof message?.message === "string"
        ? message.message : "The image preview could not be rendered.");
      error.name = typeof message?.name === "string" ? message.name : "Error";
      this.#settle(job, error);
    });
    worker.once("error", (error) => {
      this.#settle(job, renderWorkerError(error));
    });
    worker.once("exit", (code) => {
      this.#settle(job, new Error("The GIF renderer worker exited without a result."));
    });
    active.timeout = setTimeout(() => {
      this.#settle(job, new Error("The GIF renderer exceeded its time limit."));
    }, this.timeoutMs);
    active.timeout.unref?.();
  }

  #settle(job, error, bytes) {
    if (job.settled) return;
    job.settled = true;
    clearTimeout(job.deadlineTimer);
    job.options.signal?.removeEventListener("abort", job.abort);
    const pendingIndex = this.pending.indexOf(job);
    if (pendingIndex >= 0) this.pending.splice(pendingIndex, 1);
    const active = this.active?.job === job ? this.active : null;
    const finish = () => {
      if (this.active === active) this.active = null;
      if (error) job.reject(error);
      else {
        try { assertRequestActive(job.options); job.resolve(bytes); }
        catch (lateError) { job.reject(lateError); }
      }
      this.#next();
    };
    if (!active) { finish(); return; }
    clearTimeout(active.timeout);
    active.worker.removeAllListeners();
    // Retain the renderer slot until the worker has actually drained.
    Promise.resolve(active.worker.terminate()).then(finish, finish);
  }

  #next() {
    if (this.stopped || this.active) return;
    const next = this.pending.shift();
    if (next) this.#start(next);
  }
}

function createGifWorker(document, options) {
  return new Worker(new URL("./gif-worker.mjs", import.meta.url), {
    // The renderer is a self-contained module and must not inherit flags such
    // as `--input-type`, `--inspect`, or test-runner hooks from the gateway
    // process. Some of those flags are invalid for a file-backed Worker and
    // otherwise make a valid preview silently fall back to a text-only reply.
    execArgv: [],
    workerData: { document, options },
    resourceLimits: {
      maxOldGenerationSizeMb: 64,
      maxYoungGenerationSizeMb: 16,
      stackSizeMb: 2,
    },
  });
}

function positiveInteger(value, fallback) {
  if (value === undefined) return fallback;
  if (!Number.isSafeInteger(value) || value < 1) {
    throw new Error("The GIF renderer timeout is invalid.");
  }
  return value;
}

function nonNegativeInteger(value, fallback) {
  if (value === undefined) return fallback;
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error("The GIF renderer queue limit is invalid.");
  }
  return value;
}

function renderWorkerError(error) {
  const detail = error instanceof Error && error.message
    ? `: ${error.message}`
    : "";
  return new Error(`The GIF renderer worker failed${detail}`);
}

function renderAbortError(message) {
  const error = new Error(message);
  error.name = "AbortError";
  return error;
}
