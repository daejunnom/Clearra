import { availableParallelism as nodeAvailableParallelism } from "node:os";

export function loadDiscordBotConfig(environment = process.env, runtime = {}) {
  const ingressMode = discordIngressMode(
    environment.CLEARRA_DISCORD_INGRESS,
    Boolean(environment.K_SERVICE),
  );
  const registerCommands = booleanSetting(
    environment.CLEARRA_REGISTER_COMMANDS,
    false,
  );
  const token = ingressMode === "gateway" || registerCommands
    ? required(environment, "DISCORD_TOKEN")
    : environment.DISCORD_TOKEN || null;
  const jobEndpoint = environment.CLEARRA_JOB_URL
    ? httpEndpoint(environment.CLEARRA_JOB_URL)
    : ingressMode === "cloud-run"
      ? null
      : httpEndpoint("http://127.0.0.1:8787/jobs");
  const workerAuthority = workerAuthoritySetting(
    environment.CLEARRA_WORKER_AUTHORITY,
    environment.CLEARRA_JOB_URL ? "remote" : "gateway",
  );
  const processLogicalProcessors = runtimeLogicalProcessorCount(runtime);
  const useAllLogicalProcessors = booleanSetting(
    environment.CLEARRA_USE_ALL_LOGICAL_PROCESSORS,
    true,
  );
  const searchTimeoutMs = positiveInteger(
    environment.CLEARRA_SEARCH_TIMEOUT_MS,
    3 * 60_000,
  );
  const interactionDeadlineMs = boundedPositiveInteger(
    environment.CLEARRA_INTERACTION_DEADLINE_MS,
    Math.min(14 * 60_000, searchTimeoutMs + 60_000),
    14 * 60_000,
    "CLEARRA_INTERACTION_DEADLINE_MS",
  );

  let maxConcurrentSearches;
  let searchWorkersPerSession;
  let effectiveUseAllLogicalProcessors;
  if (workerAuthority === "remote") {
    maxConcurrentSearches = positiveInteger(
      environment.CLEARRA_MAX_CONCURRENT_REMOTE_JOBS ??
        environment.CLEARRA_MAX_CONCURRENT_SEARCHES,
      1,
    );
    searchWorkersPerSession = undefined;
    effectiveUseAllLogicalProcessors = false;
  } else {
    maxConcurrentSearches = positiveInteger(
      environment.CLEARRA_MAX_CONCURRENT_SEARCHES,
      1,
    );
    const sharedWorkerCapacity = useAllLogicalProcessors
      ? processLogicalProcessors
      : Math.max(1, processLogicalProcessors - 1);
    if (maxConcurrentSearches > sharedWorkerCapacity) {
      throw new Error(
        `CLEARRA_MAX_CONCURRENT_SEARCHES exceeds the runtime CPU capacity of ${sharedWorkerCapacity}.`,
      );
    }
    const workerLimitPerSession = defaultSearchWorkersPerSession(
      processLogicalProcessors,
      maxConcurrentSearches,
      useAllLogicalProcessors,
    );
    const configuredWorkers = environment.CLEARRA_SEARCH_WORKERS_PER_SESSION;
    const runtimeSelectedWorkers = automaticWorkerSetting(configuredWorkers);
    searchWorkersPerSession = runtimeSelectedWorkers
      ? maxConcurrentSearches === 1
        ? undefined
        : workerLimitPerSession
      : positiveInteger(configuredWorkers, workerLimitPerSession);
    if (
      searchWorkersPerSession !== undefined &&
      searchWorkersPerSession > workerLimitPerSession
    ) {
      throw new Error(
        `CLEARRA_SEARCH_WORKERS_PER_SESSION exceeds the runtime limit of ${workerLimitPerSession}.`,
      );
    }
    effectiveUseAllLogicalProcessors = useAllLogicalProcessors;
  }

  return {
    token,
    applicationId: environment.DISCORD_APPLICATION_ID || null,
    publicKey:
      ingressMode === "cloud-run"
        ? required(environment, "DISCORD_PUBLIC_KEY")
        : environment.DISCORD_PUBLIC_KEY || null,
    ingressMode,
    listenHost: environment.CLEARRA_LISTEN_HOST || "0.0.0.0",
    port: positiveInteger(environment.PORT, 8080),
    interactionPath: httpPath(
      environment.CLEARRA_DISCORD_INTERACTION_PATH || "/interactions",
    ),
    maxInteractionBodyBytes: positiveInteger(
      environment.CLEARRA_MAX_INTERACTION_BODY_BYTES,
      1024 * 1024,
    ),
    jobEndpoint,
    jobToken: environment.CLEARRA_JOB_TOKEN || null,
    jobPollIntervalMs: positiveInteger(
      environment.CLEARRA_JOB_POLL_INTERVAL_MS,
      250,
    ),
    jobCancelTimeoutMs: positiveInteger(
      environment.CLEARRA_JOB_CANCEL_TIMEOUT_MS,
      2_000,
    ),
    viewerBaseUrl:
      environment.CLEARRA_VIEWER_URL ||
      "https://daejunnom.github.io/Clearra/",
    registerCommands,
    searchTimeoutMs,
    interactionDeadlineMs,
    maxPendingSearches: positiveInteger(
      environment.CLEARRA_MAX_PENDING_SEARCHES,
      8,
    ),
    executable: environment.CLEARRA_EXECUTABLE || "/usr/local/bin/clearra",
    maxOutputBytes: positiveInteger(
      environment.CLEARRA_MAX_OUTPUT_BYTES,
      4 * 1024 * 1024,
    ),
    terminationGraceMs: positiveInteger(
      environment.CLEARRA_JOB_TERMINATION_GRACE_MS,
      2_000,
    ),
    maxGifBytes: positiveInteger(
      environment.CLEARRA_MAX_GIF_BYTES,
      24 * 1024 * 1024,
    ),
    maxCtk3FileBytes: positiveInteger(
      environment.CLEARRA_MAX_CTK3_FILE_BYTES,
      24 * 1024 * 1024,
    ),
    workerAuthority,
    maxConcurrentSearches,
    processLogicalProcessors,
    searchWorkersPerSession,
    useAllLogicalProcessors: effectiveUseAllLogicalProcessors,
  };
}

export function defaultSearchWorkersPerSession(
  logicalProcessors,
  concurrentSearches = 1,
  useAllLogicalProcessors = true,
) {
  const processors = positiveRuntimeInteger(logicalProcessors);
  const sessions = positiveRuntimeInteger(concurrentSearches);
  const sharedCapacity = useAllLogicalProcessors
    ? processors
    : Math.max(1, processors - 1);
  return Math.max(1, Math.floor(sharedCapacity / sessions));
}

function required(environment, name) {
  const value = environment[name];
  if (!value) throw new Error(`${name} is required.`);
  return value;
}

function positiveInteger(value, fallback) {
  if (value === undefined || value === "") return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error("A Clearra Discord numeric setting is invalid.");
  }
  return parsed;
}

function boundedPositiveInteger(value, fallback, maximum, name) {
  const parsed = positiveInteger(value, fallback);
  if (parsed > maximum) {
    throw new Error(`${name} must not exceed ${maximum} milliseconds.`);
  }
  return parsed;
}

function automaticWorkerSetting(value) {
  return value === undefined || value === "" ||
    (typeof value === "string" && value.toLowerCase() === "auto");
}

function booleanSetting(value, fallback) {
  if (value === undefined || value === "") return fallback;
  if (value === "1") return true;
  if (value === "0") return false;
  throw new Error("A Clearra Discord boolean setting is invalid.");
}

function workerAuthoritySetting(value, fallback) {
  const authority = value || fallback;
  if (authority !== "gateway" && authority !== "remote") {
    throw new Error("CLEARRA_WORKER_AUTHORITY must be gateway or remote.");
  }
  return authority;
}

function discordIngressMode(value, runningOnCloudRun) {
  const mode = value || (runningOnCloudRun ? "cloud-run" : "gateway");
  if (mode !== "cloud-run" && mode !== "gateway") {
    throw new Error(
      "CLEARRA_DISCORD_INGRESS must be cloud-run or gateway.",
    );
  }
  return mode;
}

function httpPath(value) {
  if (
    typeof value !== "string" ||
    !value.startsWith("/") ||
    value.includes("?") ||
    value.includes("#")
  ) {
    throw new Error("CLEARRA_DISCORD_INTERACTION_PATH is invalid.");
  }
  return value;
}

function httpEndpoint(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error("CLEARRA_JOB_URL is invalid.");
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("CLEARRA_JOB_URL must use HTTP or HTTPS.");
  }
  url.hash = "";
  return url.href;
}

function runtimeLogicalProcessorCount(runtime) {
  const readAvailableParallelism =
    runtime.availableParallelism ?? nodeAvailableParallelism;
  return positiveRuntimeInteger(readAvailableParallelism());
}

function positiveRuntimeInteger(value) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1) return 1;
  return parsed;
}
