import { availableParallelism as nodeAvailableParallelism } from "node:os";

import { normalizeDiscordLocale } from "./discord/i18n.mjs";
import {
  ARTIFACT_SCHEMA_VERSION,
  CONTRACT_SCHEMA_VERSION,
  runtimeIdentityFromEnvironment,
  SUPPLY_SEMANTICS_ID,
} from "./job-service/runtime-identity.mjs";

export function loadDiscordBotConfig(environment = process.env, runtime = {}) {
  assertOracleGatewayHost(environment.K_SERVICE);
  assertGatewayIngress(environment.CLEARRA_DISCORD_INGRESS);
  const registerCommands = booleanSetting(
    environment.CLEARRA_REGISTER_COMMANDS,
    false,
  );
  const token = required(environment, "DISCORD_TOKEN");
  const workerAuthority = workerAuthoritySetting(
    environment.CLEARRA_WORKER_AUTHORITY,
    environment.CLEARRA_JOB_URL ? "remote" : "gateway",
  );
  const jobEndpoint = environment.CLEARRA_JOB_URL
    ? httpEndpoint(environment.CLEARRA_JOB_URL)
    : httpEndpoint("http://127.0.0.1:8787/jobs");
  assertProductionRemoteJobEndpoint(environment, workerAuthority, jobEndpoint);
  const externalJobEndpoint =
    Boolean(environment.CLEARRA_JOB_URL) &&
    !isLoopbackHostname(new URL(jobEndpoint).hostname);
  const remoteIdentityRequired =
    externalJobEndpoint ||
    (environment.NODE_ENV === "production" && workerAuthority === "remote");
  const jobToken = environment.CLEARRA_JOB_TOKEN || null;
  if (environment.CLEARRA_JOB_URL && !jobToken) {
    throw new Error("CLEARRA_JOB_TOKEN is required with CLEARRA_JOB_URL.");
  }
  const expectedJobRuntimeIdentity = environment.CLEARRA_JOB_URL
    ? runtimeIdentityFromEnvironment(
        {
          NODE_ENV: environment.NODE_ENV,
          CLEARRA_SOURCE_COMMIT: environment.CLEARRA_EXPECTED_JOB_SOURCE_COMMIT,
          CLEARRA_ENGINE_BUILD_ID: environment.CLEARRA_EXPECTED_ENGINE_BUILD_ID,
          CLEARRA_SEARCH_CONTRACT_REVISION:
            environment.CLEARRA_EXPECTED_JOB_CONTRACT_REVISION,
          CLEARRA_SUPPLY_SEMANTICS_ID:
            environment.CLEARRA_EXPECTED_SUPPLY_SEMANTICS_ID,
          CLEARRA_ARTIFACT_SCHEMA_VERSION:
            environment.CLEARRA_EXPECTED_ARTIFACT_SCHEMA_VERSION,
        },
        { required: remoteIdentityRequired },
      )
    : null;
  assertProductionRemoteJobIdentity(
    remoteIdentityRequired,
    expectedJobRuntimeIdentity,
  );
  const oracleRenderEnabled = booleanSetting(
    environment.CLEARRA_ORACLE_RENDER_ENABLED,
    false,
  );
  const oracleTextEnabled = booleanSetting(
    environment.CLEARRA_ORACLE_TEXT_ENABLED,
    false,
  );
  if (oracleTextEnabled && !oracleRenderEnabled) {
    throw new Error(
      "CLEARRA_ORACLE_TEXT_ENABLED requires CLEARRA_ORACLE_RENDER_ENABLED=1.",
    );
  }
  if (oracleTextEnabled && !environment.CLEARRA_JOB_URL) {
    throw new Error(
      "CLEARRA_ORACLE_TEXT_ENABLED requires an explicit CLEARRA_JOB_URL.",
    );
  }
  const oracleAllowedChannelIds = snowflakeList(
    environment.CLEARRA_ORACLE_ALLOWED_CHANNEL_IDS,
    "CLEARRA_ORACLE_ALLOWED_CHANNEL_IDS",
  );
  const oracleSfinderManGuildIds = snowflakeList(
    environment.CLEARRA_ORACLE_SFINDER_MAN_GUILD_IDS,
    "CLEARRA_ORACLE_SFINDER_MAN_GUILD_IDS",
  );
  if (
    oracleSfinderManGuildIds.length > 0 &&
    !oracleRenderEnabled &&
    !oracleTextEnabled
  ) {
    throw new Error(
      "CLEARRA_ORACLE_SFINDER_MAN_GUILD_IDS requires Oracle message ingress.",
    );
  }
  const discordAdminUserIds = snowflakeList(
    environment.CLEARRA_DISCORD_ADMIN_USER_IDS,
    "CLEARRA_DISCORD_ADMIN_USER_IDS",
  );
  const oracleAllowAllTextChannels = booleanSetting(
    environment.CLEARRA_ORACLE_ALLOW_ALL_TEXT_CHANNELS,
    false,
  );
  if (
    oracleTextEnabled &&
    oracleAllowedChannelIds.length === 0 &&
    !oracleAllowAllTextChannels
  ) {
    throw new Error(
      "Oracle text ingress requires CLEARRA_ORACLE_ALLOWED_CHANNEL_IDS or explicit CLEARRA_ORACLE_ALLOW_ALL_TEXT_CHANNELS=1.",
    );
  }
  const processLogicalProcessors = runtimeLogicalProcessorCount(runtime);
  const useAllLogicalProcessors = booleanSetting(
    environment.CLEARRA_USE_ALL_LOGICAL_PROCESSORS,
    true,
  );
  const searchTimeoutMs = positiveInteger(
    environment.CLEARRA_SEARCH_TIMEOUT_MS,
    14 * 60_000,
  );
  const pcSearchTimeoutMs = positiveInteger(
    environment.CLEARRA_PC_SEARCH_TIMEOUT_MS ??
      environment.CLEARRA_REVERSE_SEARCH_TIMEOUT_MS,
    14 * 60_000,
  );
  const legacyLongSearchTimeout = environment.CLEARRA_FORWARD_SEARCH_TIMEOUT_MS;
  const buildSearchTimeoutMs = positiveInteger(
    environment.CLEARRA_BUILD_SEARCH_TIMEOUT_MS ?? legacyLongSearchTimeout,
    15 * 60_000,
  );
  const setupSearchTimeoutMs = positiveInteger(
    environment.CLEARRA_SETUP_SEARCH_TIMEOUT_MS ?? legacyLongSearchTimeout,
    15 * 60_000,
  );
  const forwardSearchTimeoutMs = positiveInteger(
    legacyLongSearchTimeout,
    15 * 60_000,
  );
  const structureSearchTimeoutMs = positiveInteger(
    environment.CLEARRA_STRUCTURE_SEARCH_TIMEOUT_MS ?? legacyLongSearchTimeout,
    15 * 60_000,
  );
  const utilitySearchTimeoutMs = positiveInteger(
    environment.CLEARRA_UTILITY_SEARCH_TIMEOUT_MS,
    15 * 60_000,
  );
  const diagnosticTimeoutMs = positiveInteger(
    environment.CLEARRA_DIAGNOSTIC_TIMEOUT_MS,
    searchTimeoutMs,
  );
  const interactionDeadlineMs = boundedPositiveInteger(
    environment.CLEARRA_INTERACTION_DEADLINE_MS,
    14 * 60_000,
    14 * 60_000,
    "CLEARRA_INTERACTION_DEADLINE_MS",
    " milliseconds",
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
    ingressMode: "gateway",
    jobEndpoint,
    jobToken,
    expectedJobRuntimeIdentity,
    jobPollIntervalMs: positiveInteger(
      environment.CLEARRA_JOB_POLL_INTERVAL_MS,
      250,
    ),
    jobCancelTimeoutMs: positiveInteger(
      environment.CLEARRA_JOB_CANCEL_TIMEOUT_MS,
      2_000,
    ),
    viewerBaseUrl:
      environment.CLEARRA_VIEWER_URL || "https://daejunnom.github.io/Clearra/",
    defaultLocale: discordLocaleSetting(
      environment.CLEARRA_DISCORD_DEFAULT_LOCALE,
    ),
    localeStorePath: environment.CLEARRA_DISCORD_LOCALE_STORE?.trim() || null,
    accessStorePath: environment.CLEARRA_DISCORD_ACCESS_STORE?.trim() || null,
    discordAdminUserIds,
    registerCommands,
    searchTimeoutMs,
    pcSearchTimeoutMs,
    // Compatibility projection for existing integrations. New code selects
    // pcSearchTimeoutMs through the pc_reverse canonical class.
    reverseSearchTimeoutMs: pcSearchTimeoutMs,
    buildSearchTimeoutMs,
    setupSearchTimeoutMs,
    forwardSearchTimeoutMs,
    structureSearchTimeoutMs,
    utilitySearchTimeoutMs,
    diagnosticTimeoutMs,
    setupProgressNoticeMs: positiveInteger(
      environment.CLEARRA_SETUP_PROGRESS_NOTICE_MS,
      5 * 60_000,
    ),
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
    oracleRenderEnabled,
    oracleTextEnabled,
    oracleAllowedChannelIds,
    oracleAllowAllTextChannels,
    // Coexistence is configured by guild instead of inferred from response
    // timing. That keeps ownership deterministic when both bots receive the
    // same Discord dispatch and prevents a late Sfinder-man response from
    // racing a Clearra search or GIF render.
    oracleSfinderManGuildIds,
    oracleCommandPrefixes: Object.freeze(["$", ">"]),
    oracleMaxInputChars: boundedPositiveInteger(
      environment.CLEARRA_ORACLE_MAX_INPUT_CHARS,
      2_000,
      2_000,
      "CLEARRA_ORACLE_MAX_INPUT_CHARS",
    ),
    oracleMaxPages: boundedPositiveInteger(
      environment.CLEARRA_ORACLE_MAX_PAGES,
      128,
      128,
      "CLEARRA_ORACLE_MAX_PAGES",
    ),
    oracleMaxCtk3FileBytes: boundedPositiveInteger(
      environment.CLEARRA_ORACLE_MAX_CTK3_FILE_BYTES,
      8 * 1024 * 1024,
      10 * 1024 * 1024,
      "CLEARRA_ORACLE_MAX_CTK3_FILE_BYTES",
    ),
    oracleMaxGifBytes: boundedPositiveInteger(
      environment.CLEARRA_ORACLE_MAX_GIF_BYTES,
      8 * 1024 * 1024,
      10 * 1024 * 1024,
      "CLEARRA_ORACLE_MAX_GIF_BYTES",
    ),
    oracleRenderTimeoutMs: boundedPositiveInteger(
      environment.CLEARRA_ORACLE_RENDER_TIMEOUT_MS,
      10_000,
      30_000,
      "CLEARRA_ORACLE_RENDER_TIMEOUT_MS",
      " milliseconds",
    ),
    oracleMaxConcurrentMessages: boundedPositiveInteger(
      environment.CLEARRA_ORACLE_MAX_CONCURRENT_MESSAGES,
      2,
      8,
      "CLEARRA_ORACLE_MAX_CONCURRENT_MESSAGES",
    ),
    oracleMaxPendingMessages: boundedPositiveInteger(
      environment.CLEARRA_ORACLE_MAX_PENDING_MESSAGES,
      4,
      32,
      "CLEARRA_ORACLE_MAX_PENDING_MESSAGES",
    ),
    oracleMaxPendingSelfMessages: boundedPositiveInteger(
      environment.CLEARRA_ORACLE_MAX_PENDING_SELF_MESSAGES,
      4,
      32,
      "CLEARRA_ORACLE_MAX_PENDING_SELF_MESSAGES",
    ),
    oracleUserCooldownMs: boundedNonNegativeInteger(
      environment.CLEARRA_ORACLE_USER_COOLDOWN_MS,
      5_000,
      60_000,
      "CLEARRA_ORACLE_USER_COOLDOWN_MS",
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

function boundedPositiveInteger(value, fallback, maximum, name, unit = "") {
  const parsed = positiveInteger(value, fallback);
  if (parsed > maximum) {
    throw new Error(`${name} must not exceed ${maximum}${unit}.`);
  }
  return parsed;
}

function boundedNonNegativeInteger(value, fallback, maximum, name) {
  if (value === undefined || value === "") return fallback;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 0 || parsed > maximum) {
    throw new Error(`${name} must be between 0 and ${maximum}.`);
  }
  return parsed;
}

function automaticWorkerSetting(value) {
  return (
    value === undefined ||
    value === "" ||
    (typeof value === "string" && value.toLowerCase() === "auto")
  );
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

function assertProductionRemoteJobEndpoint(
  environment,
  workerAuthority,
  jobEndpoint,
) {
  if (workerAuthority === "remote" && !environment.CLEARRA_JOB_URL) {
    throw new Error(
      "Remote Clearra execution requires an explicit CLEARRA_JOB_URL.",
    );
  }
  if (
    environment.NODE_ENV === "production" &&
    workerAuthority === "remote" &&
    (!environment.CLEARRA_JOB_URL || new URL(jobEndpoint).protocol !== "https:")
  ) {
    throw new Error(
      "Production remote Clearra execution requires an explicit HTTPS CLEARRA_JOB_URL.",
    );
  }
}

function assertProductionRemoteJobIdentity(
  remoteIdentityRequired,
  identity,
) {
  if (
    remoteIdentityRequired &&
    (identity?.contractSchemaVersion !== CONTRACT_SCHEMA_VERSION ||
      identity?.supplySemanticsId !== SUPPLY_SEMANTICS_ID ||
      identity?.artifactSchemaVersion !== ARTIFACT_SCHEMA_VERSION)
  ) {
    throw new Error(
      `External or production remote Clearra execution requires ${CONTRACT_SCHEMA_VERSION}, ${SUPPLY_SEMANTICS_ID}, and ${ARTIFACT_SCHEMA_VERSION}.`,
    );
  }
}

function assertGatewayIngress(value) {
  if (value === undefined || value === "" || value === "gateway") return;
  throw new Error(
    "CLEARRA_DISCORD_INGRESS no longer supports HTTP interaction ingress; use gateway.",
  );
}

function assertOracleGatewayHost(cloudRunService) {
  if (
    cloudRunService === undefined ||
    cloudRunService === null ||
    String(cloudRunService).trim() === ""
  ) {
    return;
  }
  throw new Error(
    "The Discord Gateway runtime must run on Oracle, not Cloud Run.",
  );
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
  if (url.username || url.password) {
    throw new Error("CLEARRA_JOB_URL must not contain credentials.");
  }
  if (url.protocol === "http:" && !isLoopbackHostname(url.hostname)) {
    throw new Error(
      "CLEARRA_JOB_URL must use HTTPS unless it targets loopback.",
    );
  }
  url.hash = "";
  return url.href;
}

function isLoopbackHostname(hostname) {
  const normalized = hostname.toLowerCase();
  return (
    normalized === "localhost" ||
    normalized === "::1" ||
    normalized === "[::1]" ||
    /^127(?:\.\d{1,3}){3}$/.test(normalized)
  );
}

function snowflakeList(value, settingName) {
  if (value === undefined || value.trim() === "") return Object.freeze([]);
  const ids = value.split(/[\s,]+/).filter(Boolean);
  if (ids.some((id) => !/^\d{17,20}$/.test(id))) {
    throw new Error(`${settingName} is invalid.`);
  }
  return Object.freeze([...new Set(ids)]);
}

function discordLocaleSetting(value) {
  if (value === undefined || value === "") return "en";
  const candidate = String(value).trim().toLowerCase().replaceAll("_", "-");
  if (!/^(?:en|ko)(?:-|$)/.test(candidate)) {
    throw new Error("CLEARRA_DISCORD_DEFAULT_LOCALE must be en or ko.");
  }
  return normalizeDiscordLocale(candidate);
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
