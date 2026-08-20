import assert from "node:assert/strict";
import test from "node:test";

import {
  canonicalClearraOperationalCommand,
  ClearraJobExecutor,
  isSetupSearchArguments,
  parseClearraMessage,
  prepareClearraArguments,
  searchTimeoutClass,
  searchTimeoutMsForArguments,
  tilingOnlyRequested,
  tokenizeCommand,
} from "../src/clearra/command.mjs";
import {
  defaultSearchWorkersPerSession,
  loadDiscordBotConfig,
} from "../src/config.mjs";
import { findSlashCommand } from "../src/discord/slash-command-catalog.mjs";
import { DISCORD_PUBLIC_SEARCH_CONTRACT } from "../src/discord/public-search-contract.mjs";
import {
  ARTIFACT_SCHEMA_VERSION,
  LEGACY_SEARCH_CONTRACT_REVISION,
  RUNTIME_IDENTITY_SCHEMA,
  SEARCH_CONTRACT_REVISION,
  SUPPLY_SEMANTICS_ID,
} from "../src/job-service/runtime-identity.mjs";

const TEST_RUNTIME_IDENTITY = Object.freeze({
  schema: RUNTIME_IDENTITY_SCHEMA,
  sourceCommit: "1".repeat(40),
  engineBuildId: "1".repeat(40),
  contractSchemaVersion: SEARCH_CONTRACT_REVISION,
  supplySemanticsId: SUPPLY_SEMANTICS_ID,
  artifactSchemaVersion: ARTIFACT_SCHEMA_VERSION,
});
const TEST_REMOTE_RUNTIME_ENVIRONMENT = Object.freeze({
  CLEARRA_EXPECTED_JOB_SOURCE_COMMIT: TEST_RUNTIME_IDENTITY.sourceCommit,
  CLEARRA_EXPECTED_ENGINE_BUILD_ID: TEST_RUNTIME_IDENTITY.engineBuildId,
  CLEARRA_EXPECTED_JOB_CONTRACT_REVISION:
    TEST_RUNTIME_IDENTITY.contractSchemaVersion,
  CLEARRA_EXPECTED_SUPPLY_SEMANTICS_ID:
    TEST_RUNTIME_IDENTITY.supplySemanticsId,
  CLEARRA_EXPECTED_ARTIFACT_SCHEMA_VERSION:
    TEST_RUNTIME_IDENTITY.artifactSchemaVersion,
});

test("Clearrabot applies direction-specific search and interaction limits", () => {
  const config = loadDiscordBotConfig({ DISCORD_TOKEN: "test-token" });
  assert.equal(config.ingressMode, "gateway");
  assert.equal(config.searchTimeoutMs, 180_000);
  assert.equal(config.reverseSearchTimeoutMs, 300_000);
  assert.equal(config.forwardSearchTimeoutMs, 900_000);
  assert.equal(config.setupProgressNoticeMs, 300_000);
  assert.equal(config.interactionDeadlineMs, 840_000);
  assert.equal(config.jobEndpoint, "http://127.0.0.1:8787/jobs");
  assert.equal(config.jobPollIntervalMs, 250);
  assert.equal(config.jobCancelTimeoutMs, 2_000);
  assert.equal(config.accessStorePath, null);
});

test("search timeout policy classifies native and sfinder argv consistently", () => {
  const examples = [
    [["pc", "--lines", "4"], "reverse"],
    [["pc-scenario", "--fixture", "opening"], "reverse"],
    [["failed-queue"], "reverse"],
    [["sfinder", "score-finder"], "reverse"],
    [["sfinder", "best_save"], "reverse"],
    [["damage"], "forward"],
    [["spin-structure"], "forward"],
    [["build-probability"], "forward"],
    [["sfinder", "spin-cover"], "forward"],
    [["sfinder", "setup"], "forward"],
    [["finesse", "search"], "forward"],
    [["finesse", "score"], "forward"],
    [["setup-finder", "--remaining", "TI"], "setup"],
    [["setup-finder"], "setup"],
    [["verify", "pc"], "default"],
  ];
  for (const [arguments_, expected] of examples) {
    assert.equal(
      searchTimeoutClass(arguments_),
      expected,
      arguments_.join(" "),
    );
  }
  assert.equal(isSetupSearchArguments(["setup-finder"]), true);
  assert.equal(isSetupSearchArguments(["setup-finder"]), true);
  assert.equal(isSetupSearchArguments(["sfinder", "setup"]), false);
});

test("every represented sfinder search keeps its direction-specific deadline", () => {
  const reverse = [
    "path",
    "chance",
    "percent",
    "minimals",
    "score",
    "score-minimals",
    "saves",
    "best-save",
    "score-finder",
  ];
  const forward = [
    "cover",
    "setup",
    "congruent",
    "congruent-cover",
    "setup-cover",
    "cover-percent",
    "special-cover",
    "spin-cover",
    "spin",
  ];

  for (const command of reverse) {
    assert.equal(searchTimeoutClass(["sfinder", command]), "reverse", command);
    assert.equal(
      searchTimeoutMsForArguments(["sfinder", command]),
      300_000,
      command,
    );
  }
  for (const command of forward) {
    assert.equal(searchTimeoutClass(["sfinder", command]), "forward", command);
    assert.equal(
      searchTimeoutMsForArguments(["sfinder", command]),
      900_000,
      command,
    );
  }
  for (const command of ["pc-setup", "best-setup", "dpc-finder"]) {
    assert.equal(searchTimeoutClass(["setup-finder"]), "setup", command);
    assert.equal(
      searchTimeoutMsForArguments(["setup-finder"]),
      900_000,
      command,
    );
  }
});

test("search timeout policy gives reverse five minutes and forward/setup fifteen", () => {
  assert.equal(searchTimeoutMsForArguments(["pc"]), 300_000);
  assert.equal(searchTimeoutMsForArguments(["damage"]), 900_000);
  assert.equal(searchTimeoutMsForArguments(["setup-finder"]), 900_000);
  assert.equal(searchTimeoutMsForArguments(["finesse", "search"]), 900_000);
  assert.equal(searchTimeoutMsForArguments(["finesse", "score"]), 900_000);
  assert.equal(searchTimeoutMsForArguments(["verify"]), 180_000);

  const policy = {
    searchTimeoutMs: 11,
    reverseSearchTimeoutMs: 22,
    forwardSearchTimeoutMs: 33,
  };
  assert.equal(searchTimeoutMsForArguments(["verify"], policy), 11);
  assert.equal(searchTimeoutMsForArguments(["sfinder", "path"], policy), 22);
  assert.equal(searchTimeoutMsForArguments(["spin-finder"], policy), 33);
  assert.equal(searchTimeoutMsForArguments(["setup-finder"], policy), 33);
});

test("the frozen public Discord contract matches every runtime timeout class", () => {
  assert.equal(DISCORD_PUBLIC_SEARCH_CONTRACT.length, 26);
  for (const { id, timeoutClass } of DISCORD_PUBLIC_SEARCH_CONTRACT) {
    const arguments_ = id.startsWith("finesse-")
      ? ["finesse", id.slice("finesse-".length)]
      : findSlashCommand(id).argvPrefix;
    assert.equal(searchTimeoutClass(arguments_), timeoutClass, id);
    assert.equal(
      searchTimeoutMsForArguments(arguments_),
      timeoutClass === "reverse"
        ? 300_000
        : timeoutClass === "forward" || timeoutClass === "setup"
          ? 900_000
          : 180_000,
      id,
    );
  }
});

test("directional search timeout settings remain independently configurable", () => {
  const config = loadDiscordBotConfig({
    DISCORD_TOKEN: "test-token",
    CLEARRA_SEARCH_TIMEOUT_MS: "1000",
    CLEARRA_REVERSE_SEARCH_TIMEOUT_MS: "2000",
    CLEARRA_FORWARD_SEARCH_TIMEOUT_MS: "3000",
    CLEARRA_SETUP_PROGRESS_NOTICE_MS: "4000",
  });
  assert.equal(config.searchTimeoutMs, 1_000);
  assert.equal(config.reverseSearchTimeoutMs, 2_000);
  assert.equal(config.forwardSearchTimeoutMs, 3_000);
  assert.equal(config.setupProgressNoticeMs, 4_000);
  assert.throws(
    () =>
      loadDiscordBotConfig({
        DISCORD_TOKEN: "test-token",
        CLEARRA_FORWARD_SEARCH_TIMEOUT_MS: "0",
      }),
    /numeric setting is invalid/,
  );
});

test("Discord locale and access persistence paths stay independently configurable", () => {
  const config = loadDiscordBotConfig({
    DISCORD_TOKEN: "test-token",
    CLEARRA_DISCORD_LOCALE_STORE: " /state/locale.json ",
    CLEARRA_DISCORD_ACCESS_STORE: " /state/access.json ",
  });
  assert.equal(config.localeStorePath, "/state/locale.json");
  assert.equal(config.accessStorePath, "/state/access.json");
});

test("Discord bot administrator IDs are an immutable validated allow-list", () => {
  const config = loadDiscordBotConfig({
    DISCORD_TOKEN: "test-token",
    CLEARRA_DISCORD_ADMIN_USER_IDS:
      "123456789012345678, 223456789012345678 123456789012345678",
  });
  assert.deepEqual(config.discordAdminUserIds, [
    "123456789012345678",
    "223456789012345678",
  ]);
  assert.equal(Object.isFrozen(config.discordAdminUserIds), true);
  assert.throws(
    () =>
      loadDiscordBotConfig({
        DISCORD_TOKEN: "test-token",
        CLEARRA_DISCORD_ADMIN_USER_IDS: "not-a-snowflake",
      }),
    /CLEARRA_DISCORD_ADMIN_USER_IDS is invalid/,
  );
});

test("Discord HTTP interaction ingress is retired in favor of Oracle Gateway", () => {
  assert.throws(
    () =>
      loadDiscordBotConfig({
        DISCORD_TOKEN: "test-token",
        CLEARRA_DISCORD_INGRESS: "cloud-run",
      }),
    /no longer supports HTTP interaction ingress/,
  );
  const config = loadDiscordBotConfig({
    DISCORD_TOKEN: "test-token",
    DISCORD_PUBLIC_KEY: "01".repeat(32),
    DISCORD_APPLICATION_ID: "application-id",
  });
  assert.equal(config.ingressMode, "gateway");
  assert.equal(config.registerCommands, false);
  assert.equal(config.applicationId, "application-id");
  assert.equal(config.jobEndpoint, "http://127.0.0.1:8787/jobs");
  assert.equal("publicKey" in config, false);
  assert.equal("interactionPath" in config, false);
  assert.throws(
    () =>
      loadDiscordBotConfig({
        K_SERVICE: "stale-interaction-service",
        DISCORD_TOKEN: "test-token",
      }),
    /must run on Oracle, not Cloud Run/,
  );
});

test("Clearrabot validates and configures the HTTP job service", () => {
  const config = loadDiscordBotConfig({
    DISCORD_TOKEN: "test-token",
    ...TEST_REMOTE_RUNTIME_ENVIRONMENT,
    CLEARRA_JOB_URL: "https://jobs.example.test/clearra/jobs",
    CLEARRA_JOB_TOKEN: "opaque-test-token",
    CLEARRA_JOB_POLL_INTERVAL_MS: "40",
  });
  assert.equal(config.jobEndpoint, "https://jobs.example.test/clearra/jobs");
  assert.equal(config.jobToken, "opaque-test-token");
  assert.equal(config.jobPollIntervalMs, 40);
  assert.throws(
    () =>
      loadDiscordBotConfig({
        DISCORD_TOKEN: "test-token",
        CLEARRA_JOB_URL: "file:///tmp/clearra.sock",
      }),
    /must use HTTP or HTTPS/,
  );
  assert.throws(
    () =>
      loadDiscordBotConfig({
        DISCORD_TOKEN: "test-token",
        CLEARRA_JOB_URL: "https://jobs.example.test/jobs",
      }),
    /CLEARRA_JOB_TOKEN is required/,
  );
  assert.throws(
    () =>
      loadDiscordBotConfig({
        DISCORD_TOKEN: "test-token",
        CLEARRA_JOB_URL: "http://jobs.example.test/jobs",
        CLEARRA_JOB_TOKEN: "job-token",
      }),
    /must use HTTPS unless it targets loopback/,
  );
});

test("production remote execution requires exact source and engine identities", () => {
  assert.throws(
    () =>
      loadDiscordBotConfig({
        NODE_ENV: "production",
        DISCORD_TOKEN: "test-token",
        CLEARRA_JOB_URL: "https://jobs.example.test/jobs",
        CLEARRA_JOB_TOKEN: "job-token",
        CLEARRA_EXPECTED_JOB_CONTRACT_REVISION: SEARCH_CONTRACT_REVISION,
        CLEARRA_EXPECTED_SUPPLY_SEMANTICS_ID: SUPPLY_SEMANTICS_ID,
        CLEARRA_EXPECTED_ARTIFACT_SCHEMA_VERSION: ARTIFACT_SCHEMA_VERSION,
      }),
    /full Git commit SHA/,
  );
  const config = loadDiscordBotConfig({
    NODE_ENV: "production",
    DISCORD_TOKEN: "test-token",
    CLEARRA_JOB_URL: "https://jobs.example.test/jobs",
    CLEARRA_JOB_TOKEN: "job-token",
    ...TEST_REMOTE_RUNTIME_ENVIRONMENT,
  });
  assert.deepEqual(config.expectedJobRuntimeIdentity, TEST_RUNTIME_IDENTITY);
});

test("external remote execution requires explicit endpoint and exact current identity without NODE_ENV", () => {
  assert.throws(
    () =>
      loadDiscordBotConfig({
        DISCORD_TOKEN: "test-token",
        CLEARRA_WORKER_AUTHORITY: "remote",
        CLEARRA_JOB_TOKEN: "job-token",
      }),
    /requires an explicit CLEARRA_JOB_URL/,
  );
  assert.throws(
    () =>
      loadDiscordBotConfig({
        DISCORD_TOKEN: "test-token",
        CLEARRA_JOB_URL: "https://jobs.example.test/jobs",
        CLEARRA_JOB_TOKEN: "job-token",
      }),
    /must declare its contract schema version/,
  );
  assert.throws(
    () =>
      loadDiscordBotConfig({
        DISCORD_TOKEN: "test-token",
        CLEARRA_WORKER_AUTHORITY: "gateway",
        CLEARRA_JOB_URL: "https://jobs.example.test/jobs",
        CLEARRA_JOB_TOKEN: "job-token",
      }),
    /must declare its contract schema version/,
  );
  assert.throws(
    () =>
      loadDiscordBotConfig({
        DISCORD_TOKEN: "test-token",
        CLEARRA_JOB_URL: "https://jobs.example.test/jobs",
        CLEARRA_JOB_TOKEN: "job-token",
        CLEARRA_EXPECTED_JOB_SOURCE_COMMIT: "1".repeat(40),
        CLEARRA_EXPECTED_ENGINE_BUILD_ID: "1".repeat(40),
        CLEARRA_EXPECTED_JOB_CONTRACT_REVISION:
          LEGACY_SEARCH_CONTRACT_REVISION,
        CLEARRA_EXPECTED_SUPPLY_SEMANTICS_ID:
          "clearra.supply.legacy-v1",
        CLEARRA_EXPECTED_ARTIFACT_SCHEMA_VERSION:
          "clearra.solution-data.legacy-v1",
      }),
    /External or production remote Clearra execution requires clearra\.search\.contract\.v2/,
  );
});

test("production remote execution requires an explicit HTTPS job URL", () => {
  assert.throws(
    () =>
      loadDiscordBotConfig({
        NODE_ENV: "production",
        DISCORD_TOKEN: "test-token",
        CLEARRA_JOB_TOKEN: "job-token",
        CLEARRA_WORKER_AUTHORITY: "remote",
        CLEARRA_EXPECTED_JOB_SOURCE_COMMIT: "1".repeat(40),
        CLEARRA_EXPECTED_ENGINE_BUILD_ID: "1".repeat(40),
        CLEARRA_EXPECTED_JOB_CONTRACT_REVISION: SEARCH_CONTRACT_REVISION,
      }),
    /requires an explicit CLEARRA_JOB_URL/,
  );
  assert.throws(
    () =>
      loadDiscordBotConfig({
        NODE_ENV: "production",
        DISCORD_TOKEN: "test-token",
        CLEARRA_JOB_URL: "http://127.0.0.1:8787/jobs",
        CLEARRA_JOB_TOKEN: "job-token",
        CLEARRA_WORKER_AUTHORITY: "remote",
        CLEARRA_EXPECTED_JOB_SOURCE_COMMIT: "1".repeat(40),
        CLEARRA_EXPECTED_ENGINE_BUILD_ID: "1".repeat(40),
        CLEARRA_EXPECTED_JOB_CONTRACT_REVISION: SEARCH_CONTRACT_REVISION,
      }),
    /requires an explicit HTTPS CLEARRA_JOB_URL/,
  );
});

test("production remote execution requires the current search contract", () => {
  const environment = {
    NODE_ENV: "production",
    DISCORD_TOKEN: "test-token",
    CLEARRA_JOB_URL: "https://jobs.example.test/jobs",
    CLEARRA_JOB_TOKEN: "job-token",
    CLEARRA_EXPECTED_JOB_SOURCE_COMMIT: "1".repeat(40),
    CLEARRA_EXPECTED_ENGINE_BUILD_ID: "1".repeat(40),
  };
  assert.throws(
    () => loadDiscordBotConfig(environment),
    /must declare its contract schema version/,
  );
  assert.throws(
    () =>
      loadDiscordBotConfig({
        ...environment,
        CLEARRA_EXPECTED_JOB_CONTRACT_REVISION: LEGACY_SEARCH_CONTRACT_REVISION,
        CLEARRA_EXPECTED_SUPPLY_SEMANTICS_ID:
          "clearra.supply.legacy-v1",
        CLEARRA_EXPECTED_ARTIFACT_SCHEMA_VERSION:
          "clearra.solution-data.legacy-v1",
      }),
    new RegExp(`requires ${SEARCH_CONTRACT_REVISION.replaceAll(".", "\\.")}`),
  );
});

test("Oracle rendering is explicit while ambient text stays allow-listed and remote", () => {
  const defaults = loadDiscordBotConfig({ DISCORD_TOKEN: "test-token" });
  assert.equal(defaults.oracleRenderEnabled, false);
  assert.equal(defaults.oracleTextEnabled, false);
  assert.deepEqual(defaults.oracleAllowedChannelIds, []);
  assert.deepEqual(defaults.oracleSfinderManGuildIds, []);

  const automaticRenderer = loadDiscordBotConfig({
    DISCORD_TOKEN: "test-token",
    CLEARRA_ORACLE_RENDER_ENABLED: "1",
  });
  assert.equal(automaticRenderer.oracleRenderEnabled, true);
  assert.deepEqual(automaticRenderer.oracleAllowedChannelIds, []);
  assert.throws(
    () =>
      loadDiscordBotConfig({
        DISCORD_TOKEN: "test-token",
        CLEARRA_ORACLE_TEXT_ENABLED: "1",
      }),
    /requires CLEARRA_ORACLE_RENDER_ENABLED=1/,
  );
  assert.throws(
    () =>
      loadDiscordBotConfig({
        DISCORD_TOKEN: "test-token",
        ...TEST_REMOTE_RUNTIME_ENVIRONMENT,
        CLEARRA_ORACLE_RENDER_ENABLED: "1",
        CLEARRA_ORACLE_TEXT_ENABLED: "1",
        CLEARRA_ORACLE_ALLOWED_CHANNEL_IDS: "123456789012345678",
      }),
    /requires an explicit CLEARRA_JOB_URL/,
  );
  assert.throws(
    () =>
      loadDiscordBotConfig({
        DISCORD_TOKEN: "test-token",
        ...TEST_REMOTE_RUNTIME_ENVIRONMENT,
        CLEARRA_ORACLE_RENDER_ENABLED: "1",
        CLEARRA_ORACLE_TEXT_ENABLED: "1",
        CLEARRA_JOB_URL: "https://jobs.example.test/jobs",
        CLEARRA_JOB_TOKEN: "opaque-job-token",
      }),
    /requires CLEARRA_ORACLE_ALLOWED_CHANNEL_IDS or explicit CLEARRA_ORACLE_ALLOW_ALL_TEXT_CHANNELS=1/,
  );
  const renderer = loadDiscordBotConfig({
    DISCORD_TOKEN: "test-token",
    CLEARRA_ORACLE_RENDER_ENABLED: "1",
    CLEARRA_ORACLE_ALLOWED_CHANNEL_IDS:
      "123456789012345678, 234567890123456789 123456789012345678",
  });
  assert.equal(renderer.oracleRenderEnabled, true);
  assert.equal(renderer.oracleTextEnabled, false);
  assert.deepEqual(renderer.oracleAllowedChannelIds, [
    "123456789012345678",
    "234567890123456789",
  ]);
  assert.deepEqual(renderer.oracleSfinderManGuildIds, []);
  assert.equal(renderer.oracleMaxInputChars, 2_000);
  assert.equal(renderer.oracleMaxPages, 128);
  assert.equal(renderer.oracleMaxCtk3FileBytes, 8 * 1024 * 1024);
  assert.equal(renderer.oracleMaxGifBytes, 8 * 1024 * 1024);
  assert.equal(renderer.oracleMaxPendingSelfMessages, 4);

  const textProxy = loadDiscordBotConfig({
    DISCORD_TOKEN: "test-token",
    ...TEST_REMOTE_RUNTIME_ENVIRONMENT,
    CLEARRA_ORACLE_RENDER_ENABLED: "1",
    CLEARRA_ORACLE_TEXT_ENABLED: "1",
    CLEARRA_ORACLE_ALLOWED_CHANNEL_IDS: "123456789012345678",
    CLEARRA_JOB_URL: "https://jobs.example.test/jobs",
    CLEARRA_JOB_TOKEN: "opaque-job-token",
  });
  assert.equal(textProxy.oracleTextEnabled, true);
  assert.equal(textProxy.workerAuthority, "remote");
  assert.deepEqual(textProxy.oracleCommandPrefixes, ["$", ">"]);

  const sfinderManCoexistence = loadDiscordBotConfig({
    DISCORD_TOKEN: "test-token",
    CLEARRA_ORACLE_RENDER_ENABLED: "1",
    CLEARRA_ORACLE_SFINDER_MAN_GUILD_IDS:
      "345678901234567890, 456789012345678901 345678901234567890",
  });
  assert.deepEqual(sfinderManCoexistence.oracleSfinderManGuildIds, [
    "345678901234567890",
    "456789012345678901",
  ]);
  assert.throws(
    () =>
      loadDiscordBotConfig({
        DISCORD_TOKEN: "test-token",
        CLEARRA_ORACLE_SFINDER_MAN_GUILD_IDS: "345678901234567890",
      }),
    /requires Oracle message ingress/,
  );
  assert.throws(
    () =>
      loadDiscordBotConfig({
        DISCORD_TOKEN: "test-token",
        CLEARRA_ORACLE_RENDER_ENABLED: "1",
        CLEARRA_ORACLE_SFINDER_MAN_GUILD_IDS: "not-a-snowflake",
      }),
    /CLEARRA_ORACLE_SFINDER_MAN_GUILD_IDS is invalid/,
  );

  const globalTextProxy = loadDiscordBotConfig({
    DISCORD_TOKEN: "test-token",
    ...TEST_REMOTE_RUNTIME_ENVIRONMENT,
    CLEARRA_ORACLE_RENDER_ENABLED: "1",
    CLEARRA_ORACLE_TEXT_ENABLED: "1",
    CLEARRA_ORACLE_ALLOW_ALL_TEXT_CHANNELS: "1",
    CLEARRA_JOB_URL: "https://jobs.example.test/jobs",
    CLEARRA_JOB_TOKEN: "opaque-job-token",
  });
  assert.equal(globalTextProxy.oracleAllowAllTextChannels, true);
  assert.deepEqual(globalTextProxy.oracleAllowedChannelIds, []);
});

test("Clearrabot uses every logical processor by default with an explicit reserve-core opt-out", () => {
  const runtime = { availableParallelism: () => 8 };
  const config = loadDiscordBotConfig({ DISCORD_TOKEN: "test-token" }, runtime);
  assert.equal(config.processLogicalProcessors, 8);
  assert.equal("expectedVcpus" in config, false);
  assert.equal(config.searchWorkersPerSession, undefined);
  assert.equal(config.useAllLogicalProcessors, true);
  assert.equal(defaultSearchWorkersPerSession(8, 2), 4);
  assert.equal(
    loadDiscordBotConfig(
      {
        DISCORD_TOKEN: "test-token",
        CLEARRA_SEARCH_WORKERS_PER_SESSION: "auto",
      },
      runtime,
    ).searchWorkersPerSession,
    undefined,
  );

  const reserveCore = loadDiscordBotConfig(
    {
      DISCORD_TOKEN: "test-token",
      CLEARRA_USE_ALL_LOGICAL_PROCESSORS: "0",
    },
    runtime,
  );
  assert.equal(reserveCore.searchWorkersPerSession, undefined);
  assert.equal(reserveCore.useAllLogicalProcessors, false);

  const shared = loadDiscordBotConfig(
    {
      DISCORD_TOKEN: "test-token",
      CLEARRA_MAX_CONCURRENT_SEARCHES: "2",
      CLEARRA_SEARCH_WORKERS_PER_SESSION: "4",
    },
    runtime,
  );
  assert.equal(shared.searchWorkersPerSession, 4);
  assert.equal(shared.useAllLogicalProcessors, true);
  const sharedAuto = loadDiscordBotConfig(
    {
      DISCORD_TOKEN: "test-token",
      CLEARRA_MAX_CONCURRENT_SEARCHES: "2",
      CLEARRA_SEARCH_WORKERS_PER_SESSION: "auto",
    },
    runtime,
  );
  assert.equal(sharedAuto.searchWorkersPerSession, 4);
  assert.equal(sharedAuto.useAllLogicalProcessors, true);
  const explicit = loadDiscordBotConfig(
    {
      DISCORD_TOKEN: "test-token",
      CLEARRA_SEARCH_WORKERS_PER_SESSION: "6",
    },
    runtime,
  );
  assert.equal(explicit.searchWorkersPerSession, 6);
  assert.equal(explicit.useAllLogicalProcessors, true);
  assert.throws(
    () =>
      loadDiscordBotConfig(
        {
          DISCORD_TOKEN: "test-token",
          CLEARRA_MAX_CONCURRENT_SEARCHES: "2",
          CLEARRA_SEARCH_WORKERS_PER_SESSION: "5",
        },
        runtime,
      ),
    /runtime limit of 4/,
  );
  assert.throws(
    () =>
      loadDiscordBotConfig(
        {
          DISCORD_TOKEN: "test-token",
          CLEARRA_MAX_CONCURRENT_SEARCHES: "9",
        },
        runtime,
      ),
    /runtime CPU capacity of 8/,
  );
  assert.throws(
    () =>
      loadDiscordBotConfig(
        {
          DISCORD_TOKEN: "test-token",
          CLEARRA_SEARCH_WORKERS_PER_SESSION: "9",
        },
        runtime,
      ),
    /runtime limit of 8/,
  );
  assert.throws(
    () =>
      loadDiscordBotConfig(
        {
          DISCORD_TOKEN: "test-token",
          CLEARRA_USE_ALL_LOGICAL_PROCESSORS: "yes",
        },
        runtime,
      ),
    /boolean setting is invalid/,
  );
});

test("command tokenizer preserves quoted queue syntax", () => {
  assert.deepEqual(tokenizeCommand('pc --lines 4 --patterns "[LOJ]!P7"'), [
    "pc",
    "--lines",
    "4",
    "--patterns",
    "[LOJ]!P7",
  ]);
});

test("Discord commands always use the Clearra exact product path", () => {
  assert.deepEqual(
    prepareClearraArguments([
      "pc",
      "--lines",
      "4",
      "--tablebase",
      "--build-dependency-dag",
      "--format",
      "json",
    ]),
    [
      "pc",
      "--lines",
      "4",
      "--no-tablebase",
      "--no-build-dependency-dag",
      "--format",
      "text",
    ],
  );
  assert.equal(
    parseClearraMessage("!setup --remaining SZ --priority pc"),
    null,
  );
  assert.equal(parseClearraMessage("!clearra pc --lines 2"), null);
  assert.deepEqual(
    prepareClearraArguments(
      ["pc", "--lines", "2", "--format", "text", "--include-solution-data"],
      { outputFormat: "json", includeSolutionData: true },
    ),
    [
      "pc",
      "--lines",
      "2",
      "--no-tablebase",
      "--no-build-dependency-dag",
      "--format",
      "json",
      "--include-solution-data",
    ],
  );
});

test("Discord owns an adaptive worker ceiling instead of accepting a user override", () => {
  const runtimeSelected = prepareClearraArguments(
    ["pc", "--lines", "2", "--auto-workers", "99"],
    { workers: undefined, useAllLogicalProcessors: true },
  );
  assert.deepEqual(runtimeSelected, [
    "pc",
    "--lines",
    "2",
    "--no-tablebase",
    "--no-build-dependency-dag",
    "--use-all-cpu-threads",
    "--format",
    "text",
  ]);
  assert.deepEqual(
    prepareClearraArguments(runtimeSelected, {
      workers: undefined,
      useAllLogicalProcessors: true,
    }),
    runtimeSelected,
  );
  assert.deepEqual(
    prepareClearraArguments(
      ["pc", "--lines", "4", "--workers", "99", "--use-all-cpu-threads"],
      { workers: 3 },
    ),
    [
      "pc",
      "--lines",
      "4",
      "--no-tablebase",
      "--no-build-dependency-dag",
      "--auto-workers",
      "3",
      "--format",
      "text",
    ],
  );
  assert.deepEqual(
    prepareClearraArguments(
      [
        "setup",
        "--remaining",
        "SZ",
        "--workers",
        "99",
        "--use-all-cpu-threads",
      ],
      { workers: 3 },
    ),
    [
      "setup",
      "--remaining",
      "SZ",
      "--no-tablebase",
      "--auto-workers",
      "3",
      "--format",
      "text",
    ],
  );
  assert.deepEqual(
    prepareClearraArguments(["percent", "--queue", "P7"], { workers: 3 }),
    ["percent", "--queue", "P7", "--format", "text"],
  );
  assert.deepEqual(
    prepareClearraArguments(["pc", "--workers=99", "--lines", "2"], {
      workers: 3,
    }),
    [
      "pc",
      "--lines",
      "2",
      "--no-tablebase",
      "--no-build-dependency-dag",
      "--auto-workers",
      "3",
      "--format",
      "text",
    ],
  );
  assert.deepEqual(
    prepareClearraArguments(
      [
        "build-probability",
        "--base-mask",
        "0",
        "--target-mask",
        "0xf",
        "--height",
        "1",
        "--workers",
        "99",
      ],
      { workers: 3 },
    ),
    [
      "build-probability",
      "--base-mask",
      "0",
      "--target-mask",
      "0xf",
      "--height",
      "1",
      "--auto-workers",
      "3",
      "--format",
      "text",
    ],
  );
  assert.throws(
    () =>
      prepareClearraArguments(["pc", "--lines", "4"], {
        workers: 9,
        useAllLogicalProcessors: true,
        logicalProcessors: 8,
      }),
    /hard limit of 8 logical processors/,
  );
});

test("Discord exposes curated sfinder commands through native worker policy", () => {
  const prepared = prepareClearraArguments(
    [
      "sfinder",
      "path",
      "v115@vhAAgH",
      "P7P3",
      "4",
      "--workers",
      "99",
      "--format",
      "json",
    ],
    { workers: 3 },
  );
  assert.deepEqual(prepared, [
    "sfinder",
    "path",
    "v115@vhAAgH",
    "P7P3",
    "4",
    "--auto-workers",
    "3",
    "--format",
    "text",
  ]);
  assert.deepEqual(prepareClearraArguments(prepared, { workers: 3 }), prepared);
  assert.deepEqual(
    prepareClearraArguments(["sfinder", "verify", "kicks"], { workers: 3 }),
    ["sfinder", "verify", "kicks", "--format", "text"],
  );
  assert.equal(
    parseClearraMessage("!sfinder pc_setup IOTS", "!", { workers: 2 }),
    null,
  );
  assert.throws(
    () => prepareClearraArguments(["sfinder", "pc_setup", "IOTS"]),
    /does not expose/,
  );
});

test("spin-structure uses the native parallel policy and a canonical operational label", () => {
  const prepared = prepareClearraArguments(
    [
      "spin-structure",
      "--board-mask-v1",
      "0",
      "--pieces",
      "TTIO",
      "--lines",
      "1+",
      "--spin-profile",
      "all-mini",
      "--workers",
      "99",
    ],
    { workers: 3 },
  );
  assert.deepEqual(prepared, [
    "spin-structure",
    "--board-mask-v1",
    "0",
    "--pieces",
    "TTIO",
    "--lines",
    "1+",
    "--spin-profile",
    "all-mini",
    "--auto-workers",
    "3",
    "--format",
    "text",
  ]);
  assert.equal(canonicalClearraOperationalCommand(prepared), "spin-structure");
  assert.equal(
    canonicalClearraOperationalCommand("spin-structure"),
    "spin-structure",
  );
});

test("Discord rejects unrepresented sfinder contracts", () => {
  for (const command of [
    "ren",
    "util",
    "parity",
    "render",
    "special-minimals",
  ]) {
    assert.throws(
      () => prepareClearraArguments(["sfinder", command]),
      /does not expose the sfinder/,
    );
  }
  assert.throws(
    () => prepareClearraArguments(["sfinder"]),
    /require a subcommand/,
  );
});

test("tiling-only commands are recognized after Discord argument normalization", () => {
  assert.equal(
    tilingOnlyRequested(
      prepareClearraArguments(["pc", "--lines", "4", "--tiling-only"]),
    ),
    true,
  );
  assert.equal(
    tilingOnlyRequested(
      prepareClearraArguments(["pc", "--lines", "4", "--objective", "tiling"]),
    ),
    true,
  );
  assert.equal(
    tilingOnlyRequested(prepareClearraArguments(["pc", "--lines", "4"])),
    false,
  );
});

test("file-backed and unrelated commands are rejected", () => {
  assert.throws(
    () => prepareClearraArguments(["pc-scenario", "--fixture", "field.json"]),
    /curated Clearra PC, build, setup/,
  );
  assert.throws(
    () => prepareClearraArguments(["pc", "--fixture", "field.json"]),
    /File and custom-code inputs/,
  );
  assert.throws(
    () => prepareClearraArguments(["cover", "--template-file", "field.json"]),
    /File and custom-code inputs/,
  );
  assert.throws(
    () => prepareClearraArguments(["cover", "--template-file=field.json"]),
    /File and custom-code inputs/,
  );
});

test("Clearra executor submits an idempotent POST job without shell interpretation", async () => {
  const requests = [];
  const executor = new ClearraJobExecutor({
    endpoint: "https://jobs.example.test/v1/jobs",
    authorizationToken: "job-token",
    timeoutMs: 5_000,
    createJobId: () => "job-literal-1",
    fetch: async (url, request) => {
      requests.push({ url: String(url), request });
      return jobResponse({
        id: "job-literal-1",
        state: "completed",
        result: {
          exitCode: 0,
          signal: null,
          stdout: "done",
          stderr: "",
        },
      });
    },
  });
  const result = await executor.execute([
    "pc",
    "queue with spaces",
    "literal;&|$()",
  ]);

  assert.equal(result.exitCode, 0);
  assert.equal(result.stdout, "done");
  assert.equal(requests.length, 1);
  assert.equal(requests[0].url, "https://jobs.example.test/v1/jobs");
  assert.equal(requests[0].request.method, "POST");
  assert.equal(requests[0].request.headers.authorization, "Bearer job-token");
  assert.equal(requests[0].request.headers["idempotency-key"], "job-literal-1");
  const job = JSON.parse(requests[0].request.body);
  assert.equal(job.protocol, "clearra.job.v1");
  assert.equal(job.id, "job-literal-1");
  assert.equal(job.kind, "clearra.command");
  assert.deepEqual(job.arguments, ["pc", "queue with spaces", "literal;&|$()"]);
  assert.equal(job.deadlineUnixMs > Date.now(), true);
});

test("Clearra executor submits argv-specific reverse and forward deadlines", async () => {
  const submitted = [];
  let jobIndex = 0;
  const executor = new ClearraJobExecutor({
    endpoint: "https://jobs.example.test/jobs",
    authorizationToken: "job-token",
    searchTimeoutMs: 2_000,
    reverseSearchTimeoutMs: 5_000,
    forwardSearchTimeoutMs: 15_000,
    now: () => 10_000,
    createJobId: () => `job-policy-${++jobIndex}`,
    fetch: async (_url, request) => {
      const job = JSON.parse(request.body);
      submitted.push(job);
      return jobResponse({
        id: job.id,
        state: "completed",
        result: {
          exitCode: 0,
          signal: null,
          stdout: "",
          stderr: "",
        },
      });
    },
  });

  await executor.execute(["pc"]);
  await executor.execute(["damage"]);
  await executor.execute(["setup-finder", "--remaining", "TI"]);
  await executor.execute(["verify", "pc"]);

  assert.deepEqual(
    submitted.map(({ deadlineUnixMs }) => deadlineUnixMs),
    [15_000, 25_000, 25_000, 12_000],
  );
});

test("Clearra executor sends and verifies the exact runtime identity", async () => {
  let submitted;
  const executor = new ClearraJobExecutor({
    endpoint: "https://jobs.example.test/jobs",
    authorizationToken: "job-token",
    expectedRuntimeIdentity: TEST_RUNTIME_IDENTITY,
    createJobId: () => "job-identity-1",
    fetch: async (_url, request) => {
      submitted = JSON.parse(request.body);
      return jobResponse({
        id: submitted.id,
        state: "completed",
        runtime: TEST_RUNTIME_IDENTITY,
        result: { exitCode: 0, signal: null, stdout: "", stderr: "" },
      });
    },
  });
  await executor.execute(["pc", "--lines", "4"]);
  assert.deepEqual(submitted.expectedRuntime, TEST_RUNTIME_IDENTITY);

  const mismatched = new ClearraJobExecutor({
    endpoint: "https://jobs.example.test/jobs",
    authorizationToken: "job-token",
    expectedRuntimeIdentity: TEST_RUNTIME_IDENTITY,
    createJobId: () => "job-identity-2",
    fetch: async () =>
      jobResponse({
        id: "job-identity-2",
        state: "completed",
        runtime: {
          ...TEST_RUNTIME_IDENTITY,
          sourceCommit: "2".repeat(40),
          engineBuildId: "2".repeat(40),
        },
        result: { exitCode: 0, signal: null, stdout: "", stderr: "" },
      }),
  });
  await assert.rejects(
    mismatched.execute(["pc", "--lines", "4"]),
    /runtime identity does not match/,
  );
});

test("remote Clearra job endpoints fail closed without HTTPS and application auth", () => {
  assert.throws(
    () =>
      new ClearraJobExecutor({ endpoint: "https://jobs.example.test/jobs" }),
    /requires an authorization token/,
  );
  assert.throws(
    () =>
      new ClearraJobExecutor({
        endpoint: "http://jobs.example.test/jobs",
        authorizationToken: "job-token",
      }),
    /must use HTTPS unless it targets loopback/,
  );
  assert.throws(
    () =>
      new ClearraJobExecutor({
        endpoint: "https://user:password@jobs.example.test/jobs",
        authorizationToken: "job-token",
      }),
    /must not contain credentials/,
  );
});

test("Clearra executor polls an accepted POST job to its terminal result", async () => {
  const requests = [];
  const responses = [
    jobResponse({ id: "job-poll-1", state: "accepted" }, 202),
    jobResponse({ id: "job-poll-1", state: "running" }, 200),
    jobResponse({
      id: "job-poll-1",
      state: "completed",
      result: {
        exitCode: 2,
        signal: null,
        stdout: "",
        stderr: "invalid queue",
      },
    }),
  ];
  const executor = new ClearraJobExecutor({
    endpoint: "https://jobs.example.test/jobs",
    authorizationToken: "job-token",
    pollIntervalMs: 1,
    createJobId: () => "job-poll-1",
    fetch: async (url, request) => {
      requests.push([String(url), request.method]);
      return responses.shift();
    },
  });

  const result = await executor.execute(["pc", "--lines", "4"]);
  assert.equal(result.exitCode, 2);
  assert.equal(result.stderr, "invalid queue");
  assert.deepEqual(requests, [
    ["https://jobs.example.test/jobs", "POST"],
    ["https://jobs.example.test/jobs/job-poll-1", "GET"],
    ["https://jobs.example.test/jobs/job-poll-1", "GET"],
  ]);
});

test("Clearra executor cancels the remote job when the Clearrabot timeout expires", async () => {
  const requests = [];
  const executor = new ClearraJobExecutor({
    endpoint: "https://jobs.example.test/jobs",
    authorizationToken: "job-token",
    timeoutMs: 50,
    cancelTimeoutMs: 100,
    createJobId: () => "job-timeout-1",
    fetch: async (url, request) => {
      requests.push([String(url), request.method]);
      if (request.method === "DELETE")
        return new Response(null, { status: 204 });
      return await pendingUntilAbort(request.signal);
    },
  });

  await assert.rejects(
    executor.execute(["pc", "--lines", "4"]),
    /Clearrabot search exceeded the 50-millisecond time limit/,
  );
  assert.deepEqual(requests.at(-1), [
    "https://jobs.example.test/jobs/job-timeout-1",
    "DELETE",
  ]);
});

test("Clearra executor forwards caller cancellation to the remote job", async () => {
  const controller = new AbortController();
  const requests = [];
  const executor = new ClearraJobExecutor({
    endpoint: "https://jobs.example.test/jobs",
    authorizationToken: "job-token",
    timeoutMs: 5_000,
    createJobId: () => "job-cancel-1",
    fetch: async (url, request) => {
      requests.push([String(url), request.method]);
      if (request.method === "DELETE")
        return new Response(null, { status: 204 });
      return await pendingUntilAbort(request.signal);
    },
  });

  const execution = executor.execute(["setup", "--remaining", "SZ"], {
    signal: controller.signal,
  });
  controller.abort();
  await assert.rejects(execution, { name: "AbortError" });
  assert.deepEqual(requests.at(-1), [
    "https://jobs.example.test/jobs/job-cancel-1",
    "DELETE",
  ]);
});

function jobResponse(job, status = 200) {
  return new Response(JSON.stringify({ protocol: "clearra.job.v1", ...job }), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function pendingUntilAbort(signal) {
  return new Promise((resolve, reject) => {
    const abort = () => reject(signal.reason ?? new Error("aborted"));
    signal.addEventListener("abort", abort, { once: true });
    if (signal.aborted) abort();
  });
}
