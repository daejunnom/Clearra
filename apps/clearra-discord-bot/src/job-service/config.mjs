import { availableParallelism as nodeAvailableParallelism } from "node:os";

export function loadClearraJobServiceConfig(
  environment = process.env,
  runtime = {},
) {
  const processLogicalProcessors = runtimeLogicalProcessorCount(runtime);
  const useAllLogicalProcessors =
    environment.CLEARRA_USE_ALL_LOGICAL_PROCESSORS === "1";
  const workerLimit = useAllLogicalProcessors
    ? processLogicalProcessors
    : Math.max(1, processLogicalProcessors - 1);
  const searchWorkersPerSession = positiveIntegerOrAuto(
    environment.CLEARRA_SEARCH_WORKERS_PER_SESSION,
    workerLimit,
  );
  if (searchWorkersPerSession > workerLimit) {
    throw new Error(
      `CLEARRA_SEARCH_WORKERS_PER_SESSION exceeds the job-service runtime limit of ${workerLimit}.`,
    );
  }

  const allowUnauthenticated =
    environment.CLEARRA_JOB_SERVICE_ALLOW_UNAUTHENTICATED === "1";
  const authorizationToken = environment.CLEARRA_JOB_TOKEN || null;
  if (!allowUnauthenticated && !authorizationToken) {
    throw new Error(
      "CLEARRA_JOB_TOKEN is required unless CLEARRA_JOB_SERVICE_ALLOW_UNAUTHENTICATED=1.",
    );
  }

  return {
    host: environment.CLEARRA_LISTEN_HOST || "0.0.0.0",
    port: positiveInteger(environment.PORT, 8080),
    executable: environment.CLEARRA_EXECUTABLE || "/usr/local/bin/clearra",
    authorizationToken,
    allowUnauthenticated,
    maxRequestBodyBytes: positiveInteger(
      environment.CLEARRA_JOB_SERVICE_MAX_REQUEST_BODY_BYTES,
      1024 * 1024,
    ),
    maxOutputBytes: positiveInteger(
      environment.CLEARRA_MAX_OUTPUT_BYTES,
      4 * 1024 * 1024,
    ),
    searchTimeoutMs: positiveInteger(
      environment.CLEARRA_SEARCH_TIMEOUT_MS,
      170_000,
    ),
    terminationGraceMs: positiveInteger(
      environment.CLEARRA_JOB_TERMINATION_GRACE_MS,
      2_000,
    ),
    maxConcurrentJobs: positiveInteger(
      environment.CLEARRA_MAX_CONCURRENT_SEARCHES,
      1,
    ),
    completedJobTtlMs: positiveInteger(
      environment.CLEARRA_JOB_COMPLETED_TTL_MS,
      5 * 60_000,
    ),
    maxRetainedJobs: positiveInteger(
      environment.CLEARRA_JOB_MAX_RETAINED_RESULTS,
      64,
    ),
    processLogicalProcessors,
    searchWorkersPerSession,
    useAllLogicalProcessors:
      useAllLogicalProcessors &&
      searchWorkersPerSession > Math.max(1, processLogicalProcessors - 1),
  };
}

function positiveInteger(value, fallback) {
  if (value === undefined || value === "") return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error("A Clearra job-service numeric setting is invalid.");
  }
  return parsed;
}

function positiveIntegerOrAuto(value, fallback) {
  if (typeof value === "string" && value.toLowerCase() === "auto") return fallback;
  return positiveInteger(value, fallback);
}

function runtimeLogicalProcessorCount(runtime) {
  const readAvailableParallelism =
    runtime.availableParallelism ?? nodeAvailableParallelism;
  const value = Number(readAvailableParallelism());
  return Number.isSafeInteger(value) && value > 0 ? value : 1;
}
