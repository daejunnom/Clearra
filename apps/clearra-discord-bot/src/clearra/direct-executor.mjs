import { executionDeadline } from "../job-service/execution-deadline.mjs";
import { ClearraCommandRunner } from "../job-service/runner.mjs";
import {
  assertDiscordCanonicalOnlyResult,
  assertDiscordNoTieArguments,
  searchTimeoutClass,
  searchTimeoutMsForArguments,
} from "./command.mjs";

export class ClearraDirectExecutor {
  #executeJob;

  constructor(config, options = {}) {
    this.now = options.now ?? Date.now;
    this.timeoutPolicy = {
      searchTimeoutMs: nonNegativeInteger(
        config.searchTimeoutMs,
        "search timeout",
      ),
      ...(config.reverseSearchTimeoutMs === undefined
        ? {}
        : {
            reverseSearchTimeoutMs: nonNegativeInteger(
              config.reverseSearchTimeoutMs,
              "reverse-search timeout",
            ),
          }),
      ...(config.pcSearchTimeoutMs === undefined
        ? {}
        : {
            pcSearchTimeoutMs: nonNegativeInteger(
              config.pcSearchTimeoutMs,
              "PC-search timeout",
            ),
          }),
      ...(config.buildSearchTimeoutMs === undefined
        ? {}
        : {
            buildSearchTimeoutMs: nonNegativeInteger(
              config.buildSearchTimeoutMs,
              "build-search timeout",
            ),
          }),
      ...(config.setupSearchTimeoutMs === undefined
        ? {}
        : {
            setupSearchTimeoutMs: nonNegativeInteger(
              config.setupSearchTimeoutMs,
              "setup-search timeout",
            ),
          }),
      ...(config.forwardSearchTimeoutMs === undefined
        ? {}
        : {
            forwardSearchTimeoutMs: nonNegativeInteger(
              config.forwardSearchTimeoutMs,
              "forward-search timeout",
            ),
          }),
      ...(config.structureSearchTimeoutMs === undefined
        ? {}
        : {
            structureSearchTimeoutMs: nonNegativeInteger(
              config.structureSearchTimeoutMs,
              "structure-search timeout",
            ),
          }),
      ...(config.utilitySearchTimeoutMs === undefined
        ? {}
        : {
            utilitySearchTimeoutMs: nonNegativeInteger(
              config.utilitySearchTimeoutMs,
              "utility-search timeout",
            ),
          }),
      ...(config.diagnosticTimeoutMs === undefined
        ? {}
        : {
            diagnosticTimeoutMs: nonNegativeInteger(
              config.diagnosticTimeoutMs,
              "diagnostic timeout",
            ),
          }),
    };
    this.maxOutputBytes = positiveInteger(
      config.maxOutputBytes,
      "output limit",
    );
    this.maxArtifactBytes = positiveInteger(
      config.maxGifBytes ?? config.maxArtifactBytes ?? config.maxOutputBytes,
      "artifact limit",
    );
    const externalRunner = options.runner;
    const runner = externalRunner ?? new ClearraCommandRunner({
      executable: config.executable,
      processLogicalProcessors: config.processLogicalProcessors,
      expectedVcpus: config.expectedVcpus,
      searchWorkersPerSession: config.searchWorkersPerSession,
      useAllLogicalProcessors: config.useAllLogicalProcessors,
      ...this.timeoutPolicy,
      maxOutputBytes: config.maxOutputBytes,
      maxArtifactBytes: this.maxArtifactBytes,
      terminationGraceMs: config.terminationGraceMs,
    }, options.runnerOptions);
    // The runner constructed here owns and enforces the canonical result
    // boundary. A supplied runner has not crossed that boundary yet.
    this.#executeJob = externalRunner == null
      ? (job, executionOptions) => runner.execute(job, executionOptions)
      : (job, executionOptions) => runner.execute(job, executionOptions).then(assertDiscordCanonicalOnlyResult);
  }

  execute(arguments_, options = {}) {
    validateArguments(arguments_);
    assertDiscordNoTieArguments(arguments_);
    const now = this.now();
    const timeoutClass = searchTimeoutClass(arguments_, options.timeoutClass);
    const commandTimeoutMs = searchTimeoutMsForArguments(
      arguments_,
      this.timeoutPolicy,
      timeoutClass,
    );
    const deadlineUnixMs = executionDeadline(
      now,
      commandTimeoutMs,
      options.deadlineUnixMs === undefined ? 0 : absoluteDeadline(options.deadlineUnixMs),
    );

    return this.#executeJob(
      {
        arguments: [...arguments_],
        timeoutClass,
        deadlineUnixMs,
        maxOutputBytes: this.maxOutputBytes,
        maxArtifactBytes: this.maxArtifactBytes,
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

function nonNegativeInteger(value, label) {
  const parsed = Number(value ?? 0);
  if (!Number.isSafeInteger(parsed) || parsed < 0) {
    throw new Error(`Clearrabot received an invalid direct ${label}.`);
  }
  return parsed;
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
  if (!Number.isSafeInteger(parsed) || parsed < 0) {
    throw new Error("Clearrabot received an invalid interaction deadline.");
  }
  return parsed;
}
