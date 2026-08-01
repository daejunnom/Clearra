import { availableParallelism as nodeAvailableParallelism } from "node:os";

export function loadDiscordBotConfig(environment = process.env, runtime = {}) {
  const token = required(environment, "DISCORD_TOKEN");
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
    prefix: environment.CLEARRA_DISCORD_PREFIX || "!",
    executable: environment.CLEARRA_EXECUTABLE || "clearra",
    viewerBaseUrl:
      environment.CLEARRA_VIEWER_URL ||
      "https://daejunnom.github.io/Clearra/",
    registerCommands: environment.CLEARRA_REGISTER_COMMANDS !== "0",
    searchTimeoutMs: positiveInteger(
      environment.CLEARRA_SEARCH_TIMEOUT_MS,
      3 * 60_000,
    ),
    maxGifBytes: positiveInteger(
      environment.CLEARRA_MAX_GIF_BYTES,
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
