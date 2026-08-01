import { availableParallelism as nodeAvailableParallelism } from "node:os";

export function loadDiscordBotConfig(environment = process.env, runtime = {}) {
  const token = required(environment, "DISCORD_TOKEN");
  const ingressMode = discordIngressMode(
    environment.CLEARRA_DISCORD_INGRESS,
    Boolean(environment.K_SERVICE),
  );
  const maxConcurrentSearches = positiveInteger(
    environment.CLEARRA_MAX_CONCURRENT_SEARCHES,
    1,
  );
  const processLogicalProcessors = runtimeLogicalProcessorCount(runtime);
  const useAllLogicalProcessors =
    environment.CLEARRA_USE_ALL_LOGICAL_PROCESSORS === "1";
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
  const searchWorkersPerSession = positiveIntegerOrAuto(
    environment.CLEARRA_SEARCH_WORKERS_PER_SESSION,
    workerLimitPerSession,
  );
  if (searchWorkersPerSession > workerLimitPerSession) {
    throw new Error(
      `CLEARRA_SEARCH_WORKERS_PER_SESSION exceeds the runtime limit of ${workerLimitPerSession}.`,
    );
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
    jobEndpoint: httpEndpoint(
      environment.CLEARRA_JOB_URL || "http://127.0.0.1:8787/jobs",
    ),
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
    registerCommands: booleanSetting(
      environment.CLEARRA_REGISTER_COMMANDS,
      ingressMode === "gateway",
    ),
    searchTimeoutMs: positiveInteger(
      environment.CLEARRA_SEARCH_TIMEOUT_MS,
      3 * 60_000,
    ),
    maxGifBytes: positiveInteger(
      environment.CLEARRA_MAX_GIF_BYTES,
      24 * 1024 * 1024,
    ),
    maxCtk3FileBytes: positiveInteger(
      environment.CLEARRA_MAX_CTK3_FILE_BYTES,
      24 * 1024 * 1024,
    ),
    maxConcurrentSearches,
    processLogicalProcessors,
    searchWorkersPerSession,
    useAllLogicalProcessors:
      useAllLogicalProcessors &&
      searchWorkersPerSession > Math.max(1, processLogicalProcessors - 1),
  };
}

export function defaultSearchWorkersPerSession(
  logicalProcessors,
  concurrentSearches = 1,
  useAllLogicalProcessors = false,
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

function positiveIntegerOrAuto(value, fallback) {
  if (typeof value === "string" && value.toLowerCase() === "auto") return fallback;
  return positiveInteger(value, fallback);
}

function booleanSetting(value, fallback) {
  if (value === undefined || value === "") return fallback;
  if (value === "1") return true;
  if (value === "0") return false;
  throw new Error("A Clearra Discord boolean setting is invalid.");
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
