import { Worker } from "node:worker_threads";

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
    return new Promise((resolve, reject) => {
      const job = { document, options, resolve, reject };
      if (this.active) this.pending.push(job);
      else this.#start(job);
    });
  }

  stop() {
    if (this.stopped) return;
    this.stopped = true;
    for (const job of this.pending.splice(0)) {
      job.reject(renderAbortError("The GIF renderer has stopped."));
    }
    const active = this.active;
    if (!active) return;
    this.active = null;
    clearTimeout(active.timeout);
    active.worker.removeAllListeners();
    void active.worker.terminate();
    active.job.reject(renderAbortError("The GIF renderer has stopped."));
  }

  #start(job) {
    let worker;
    try {
      worker = this.createWorker(job.document, job.options);
    } catch (error) {
      job.reject(renderWorkerError(error));
      this.#next();
      return;
    }

    const active = {
      job,
      worker,
      settled: false,
      timeout: null,
    };
    this.active = active;
    const settle = (callback) => {
      if (active.settled) return;
      active.settled = true;
      clearTimeout(active.timeout);
      worker.removeAllListeners();
      if (this.active === active) this.active = null;
      callback();
      this.#next();
    };
    worker.once("message", (message) => {
      settle(() => {
        if (message?.ok === true && message.bytes instanceof Uint8Array) {
          job.resolve(message.bytes);
          return;
        }
        const error = new Error(
          typeof message?.message === "string"
            ? message.message
            : "The image preview could not be rendered.",
        );
        error.name = typeof message?.name === "string" ? message.name : "Error";
        job.reject(error);
      });
    });
    worker.once("error", (error) => {
      settle(() => job.reject(renderWorkerError(error)));
    });
    worker.once("exit", (code) => {
      if (code === 0 || active.settled) return;
      settle(() => job.reject(new Error("The GIF renderer worker exited unexpectedly.")));
    });
    active.timeout = setTimeout(() => {
      settle(() => {
        void worker.terminate();
        job.reject(new Error("The GIF renderer exceeded its time limit."));
      });
    }, this.timeoutMs);
    active.timeout.unref?.();
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
