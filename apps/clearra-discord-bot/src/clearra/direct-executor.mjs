import { ClearraCommandRunner } from "../job-service/runner.mjs";

export class ClearraDirectExecutor {
  constructor(config, options = {}) {
    this.now = options.now ?? Date.now;
    this.totalTimeoutMs = positiveInteger(
      config.interactionDeadlineMs ?? config.searchTimeoutMs,
      "interaction deadline",
    );
    this.maxOutputBytes = positiveInteger(
      config.maxOutputBytes,
      "output limit",
    );
    this.runner = options.runner ?? new ClearraCommandRunner({
      executable: config.executable,
      processLogicalProcessors: config.processLogicalProcessors,
      searchWorkersPerSession: config.searchWorkersPerSession,
      useAllLogicalProcessors: config.useAllLogicalProcessors,
      searchTimeoutMs: config.searchTimeoutMs,
      maxOutputBytes: config.maxOutputBytes,
      terminationGraceMs: config.terminationGraceMs,
    }, options.runnerOptions);
  }

  execute(arguments_, options = {}) {
    validateArguments(arguments_);
    const now = this.now();
    const latestDeadlineUnixMs = now + this.totalTimeoutMs;
    const callerDeadlineUnixMs = options.deadlineUnixMs === undefined
      ? latestDeadlineUnixMs
      : absoluteDeadline(options.deadlineUnixMs);

    return this.runner.execute(
      {
        arguments: [...arguments_],
        deadlineUnixMs: Math.min(callerDeadlineUnixMs, latestDeadlineUnixMs),
        maxOutputBytes: this.maxOutputBytes,
      },
      { signal: options.signal },
    );
  }
}

function validateArguments(arguments_) {
  if (!Array.isArray(arguments_) || arguments_.length === 0) {
    throw new Error("Clearrabot cannot execute an empty direct Clearra command.");
  }
  for (const argument of arguments_) {
    if (typeof argument !== "string" || argument.includes("\0")) {
      throw new Error("Clearrabot received an invalid direct Clearra argument.");
    }
  }
}

function positiveInteger(value, label) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error(`Clearrabot received an invalid direct ${label}.`);
  }
  return parsed;
}

function absoluteDeadline(value) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error("Clearrabot received an invalid interaction deadline.");
  }
  return parsed;
}
