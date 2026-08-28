import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { EventEmitter } from "node:events";
import { existsSync, readFileSync } from "node:fs";
import { writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import { PassThrough } from "node:stream";
import test from "node:test";

import { prepareClearraArguments } from "../src/clearra/command.mjs";
import { loadDiscordBotConfig } from "../src/config.mjs";
import { loadClearraJobServiceConfig } from "../src/job-service/config.mjs";
import { ClearraCommandRunner } from "../src/job-service/runner.mjs";
import { ClearraJobService } from "../src/job-service/server.mjs";
import {
  ARTIFACT_SCHEMA_VERSION,
  LEGACY_ARTIFACT_SCHEMA_VERSION,
  LEGACY_SEARCH_CONTRACT_REVISION,
  LEGACY_SUPPLY_SEMANTICS_ID,
  normalizeRuntimeIdentity,
  productBuildIdentityFromRuntime,
  RUNTIME_IDENTITY_SCHEMA,
  runtimeIdentityMatches,
  SEARCH_CONTRACT_REVISION,
  SUPPLY_SEMANTICS_ID,
} from "../src/job-service/runtime-identity.mjs";

const TEST_RUNTIME_IDENTITY = Object.freeze({
  schema: RUNTIME_IDENTITY_SCHEMA,
  sourceCommit: "a".repeat(40),
  engineBuildId: "a".repeat(40),
  contractSchemaVersion: SEARCH_CONTRACT_REVISION,
  supplySemanticsId: SUPPLY_SEMANTICS_ID,
  artifactSchemaVersion: ARTIFACT_SCHEMA_VERSION,
});

test("runtime identity preserves distinct source and engine commits", () => {
  const identity = normalizeRuntimeIdentity({
    ...TEST_RUNTIME_IDENTITY,
    engineBuildId: "b".repeat(40),
  });

  assert.equal(identity.sourceCommit, "a".repeat(40));
  assert.equal(identity.engineBuildId, "b".repeat(40));
});

test("job runner requires working finesse search and score capabilities", async () => {
  const invocations = [];
  const runner = new ClearraCommandRunner(
    {
      executable: "clearra",
      searchTimeoutMs: 5_000,
      runtimeIdentity: TEST_RUNTIME_IDENTITY,
    },
    {
      execFile: (_executable, arguments_, options, callback) => {
        invocations.push({ arguments_, options });
        callback(null, JSON.stringify({
          runtime_identity: productBuildIdentityFromRuntime(TEST_RUNTIME_IDENTITY),
          finesse_report: {
            mode: arguments_[1],
            metric: "inputs",
          },
        }), "");
      },
    },
  );

  await runner.verifyCapabilities();

  assert.deepEqual(
    invocations.map(({ arguments_ }) => arguments_.slice(0, 2)),
    [["finesse", "search"], ["finesse", "score"]],
  );
  for (const invocation of invocations) {
    assert.equal(invocation.options.shell, false);
    assert.equal(invocation.options.windowsHide, true);
    assert.equal(invocation.arguments_.includes("--workers"), true);
    assert.equal(invocation.arguments_.includes("--format"), true);
  }
});

test("job runner rejects a stale CLI build identity before listening", async () => {
  const runner = new ClearraCommandRunner(
    {
      executable: "clearra",
      searchTimeoutMs: 5_000,
      runtimeIdentity: TEST_RUNTIME_IDENTITY,
    },
    {
      execFile: (_executable, arguments_, _options, callback) => {
        callback(null, JSON.stringify({
          runtime_identity: {
            ...productBuildIdentityFromRuntime(TEST_RUNTIME_IDENTITY),
            supply_semantics_id: "clearra.supply.stale",
          },
          finesse_report: { mode: arguments_[1], metric: "inputs" },
        }), "");
      },
    },
  );

  await assert.rejects(
    runner.verifyCapabilities(),
    /^Error: Clearra executable identity does not match the configured job runtime identity\.$/,
  );
});

test("job runner fails closed when a legacy CLI lacks finesse", async () => {
  const runner = new ClearraCommandRunner(
    {
      executable: "clearra-v0.5.1",
      searchTimeoutMs: 5_000,
    },
    {
      execFile: (_executable, _arguments, _options, callback) => {
        const error = new Error("unsupported command");
        error.code = 2;
        callback(error, "", "unsupported command");
      },
    },
  );

  await assert.rejects(
    runner.verifyCapabilities(),
    /^Error: Clearra engine capability check failed\.$/,
  );
});

test("both job-service Docker paths smoke-test finesse before runtime", () => {
  const imagePaths = [
    [
      "Dockerfile.current-job-service",
      "cloudbuild-current-job-service.yaml",
      SEARCH_CONTRACT_REVISION,
    ],
    [
      "Dockerfile.job-service",
      "cloudbuild-job-service.yaml",
      LEGACY_SEARCH_CONTRACT_REVISION,
    ],
  ];
  for (const [dockerName, cloudBuildName, contractRevision] of imagePaths) {
    const dockerfile = readFileSync(
      new URL(`../${dockerName}`, import.meta.url),
      "utf8",
    );
    assert.match(dockerfile, /finesse search --base-mask/);
    assert.match(dockerfile, /finesse score --initial-mask/);
    assert.match(dockerfile, /grep -q '\"mode\":\"search\"'/);
    assert.match(dockerfile, /grep -q '\"mode\":\"score\"'/);
    const cloudBuild = readFileSync(
      new URL(`../${cloudBuildName}`, import.meta.url),
      "utf8",
    );
    assert.match(
      cloudBuild,
      new RegExp(`- apps/clearra-discord-bot/${dockerName.replaceAll(".", "\\.")}`),
    );
    assert.match(dockerfile, /CLEARRA_SOURCE_COMMIT/);
    assert.match(dockerfile, /CLEARRA_ENGINE_BUILD_ID/);
    assert.match(dockerfile, /CLEARRA_SUPPLY_SEMANTICS_ID/);
    assert.match(dockerfile, /CLEARRA_ARTIFACT_SCHEMA_VERSION/);
    assert.match(dockerfile, new RegExp(contractRevision.replaceAll(".", "\\.")));
    assert.match(cloudBuild, /CLEARRA_SOURCE_COMMIT=\$\{_SOURCE_COMMIT\}/);
    assert.match(
      cloudBuild,
      /CLEARRA_ENGINE_BUILD_ID=\$\{_(?:ENGINE_BUILD_ID|SOURCE_COMMIT)\}/,
    );
  }
  const currentCloudBuild = readFileSync(
    new URL("../cloudbuild-current-job-service.yaml", import.meta.url),
    "utf8",
  );
  assert.doesNotMatch(currentCloudBuild, /_TAG:\s*latest/);
  const currentDocker = readFileSync(
    new URL("../Dockerfile.current-job-service", import.meta.url),
    "utf8",
  );
  assert.match(
    currentDocker,
    /CLEARRA_SEARCH_CONTRACT_REVISION!==?'clearra\.search\.contract\.v2'/,
  );
  assert.match(
    currentDocker,
    /source_commit.*CLEARRA_SOURCE_COMMIT/,
  );
  assert.match(
    currentDocker,
    /"supply_semantics_id":"clearra\.supply\.projected-terminal-lookahead\.v1"/,
  );
  const legacyDocker = readFileSync(
    new URL("../Dockerfile.job-service", import.meta.url),
    "utf8",
  );
  const legacyCloudBuild = readFileSync(
    new URL("../cloudbuild-job-service.yaml", import.meta.url),
    "utf8",
  );
  assert.match(legacyDocker, /sha256sum --check --strict/);
  assert.doesNotMatch(legacyDocker, /ARG CLEARRA_CLI_SHA256=""/);
  assert.match(legacyCloudBuild, /_CLEARRA_CLI_SHA256:\s*required/);
  assert.doesNotMatch(legacyCloudBuild, /clearra\.search\.contract\.v2/);

  const main = readFileSync(
    new URL("../src/job-service/main.mjs", import.meta.url),
    "utf8",
  );
  const gateOffset = main.indexOf("await runner.verifyCapabilities()");
  const listenOffset = main.indexOf("await service.listen()");
  assert.notEqual(gateOffset, -1);
  assert.notEqual(listenOffset, -1);
  assert.ok(
    gateOffset < listenOffset,
    "the capability gate must finish before the service listens",
  );
});

test("remote job execution is not capped by the Oracle gateway CPU count", () => {
  const config = loadDiscordBotConfig(
    {
      DISCORD_TOKEN: "test-token",
      CLEARRA_JOB_URL: "https://jobs.example.test/jobs",
      CLEARRA_JOB_TOKEN: "job-token",
      CLEARRA_EXPECTED_JOB_SOURCE_COMMIT: "a".repeat(40),
      CLEARRA_EXPECTED_ENGINE_BUILD_ID: "a".repeat(40),
      CLEARRA_EXPECTED_JOB_CONTRACT_REVISION: SEARCH_CONTRACT_REVISION,
      CLEARRA_EXPECTED_SUPPLY_SEMANTICS_ID: SUPPLY_SEMANTICS_ID,
      CLEARRA_EXPECTED_ARTIFACT_SCHEMA_VERSION: ARTIFACT_SCHEMA_VERSION,
      CLEARRA_MAX_CONCURRENT_REMOTE_JOBS: "4",
    },
    { availableParallelism: () => 2 },
  );

  assert.equal(config.workerAuthority, "remote");
  assert.equal(config.maxConcurrentSearches, 4);
  assert.equal(config.searchWorkersPerSession, undefined);
  assert.equal(config.useAllLogicalProcessors, false);
  assert.deepEqual(
    prepareClearraArguments(["pc", "--lines", "4"], {
      workers: config.searchWorkersPerSession,
    }),
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
});

test("gateway worker authority preserves host-local CPU allocation", () => {
  const config = loadDiscordBotConfig(
    {
      DISCORD_TOKEN: "test-token",
      CLEARRA_JOB_URL: "https://jobs.example.test/jobs",
      CLEARRA_JOB_TOKEN: "job-token",
      CLEARRA_WORKER_AUTHORITY: "gateway",
      CLEARRA_EXPECTED_JOB_SOURCE_COMMIT: "a".repeat(40),
      CLEARRA_EXPECTED_ENGINE_BUILD_ID: "a".repeat(40),
      CLEARRA_EXPECTED_JOB_CONTRACT_REVISION: SEARCH_CONTRACT_REVISION,
      CLEARRA_EXPECTED_SUPPLY_SEMANTICS_ID: SUPPLY_SEMANTICS_ID,
      CLEARRA_EXPECTED_ARTIFACT_SCHEMA_VERSION: ARTIFACT_SCHEMA_VERSION,
      CLEARRA_MAX_CONCURRENT_SEARCHES: "2",
    },
    { availableParallelism: () => 8 },
  );

  assert.equal(config.workerAuthority, "gateway");
  assert.equal(config.maxConcurrentSearches, 2);
  assert.equal(config.searchWorkersPerSession, 4);
});

test("job service uses every Cloud Run logical processor by default", () => {
  const config = loadClearraJobServiceConfig(
    {
      K_SERVICE: "clearra-job-service",
      CLEARRA_JOB_TOKEN: "job-token",
      CLEARRA_SEARCH_WORKERS_PER_SESSION: "auto",
    },
    { availableParallelism: () => 6 },
  );

  assert.equal(config.processLogicalProcessors, 6);
  assert.equal(config.expectedVcpus, 6);
  assert.equal(config.searchWorkersPerSession, 6);
  assert.equal(config.useAllLogicalProcessors, true);
  assert.equal(config.maxConcurrentJobs, 1);
  assert.equal(config.port, 8787);
  assert.equal(config.searchTimeoutMs, 180_000);
  assert.equal(config.diagnosticTimeoutMs, 180_000);
  assert.equal(config.pcSearchTimeoutMs, 300_000);
  assert.equal(config.reverseSearchTimeoutMs, 300_000);
  assert.equal(config.buildSearchTimeoutMs, 900_000);
  assert.equal(config.setupSearchTimeoutMs, 900_000);
  assert.equal(config.forwardSearchTimeoutMs, 900_000);
  assert.equal(config.structureSearchTimeoutMs, 900_000);

  const timeoutOverrides = loadClearraJobServiceConfig(
    {
      CLEARRA_JOB_TOKEN: "job-token",
      CLEARRA_SEARCH_TIMEOUT_MS: "1000",
      CLEARRA_PC_SEARCH_TIMEOUT_MS: "2000",
      CLEARRA_BUILD_SEARCH_TIMEOUT_MS: "3000",
      CLEARRA_SETUP_SEARCH_TIMEOUT_MS: "4000",
      CLEARRA_FORWARD_SEARCH_TIMEOUT_MS: "5000",
      CLEARRA_STRUCTURE_SEARCH_TIMEOUT_MS: "6000",
      CLEARRA_DIAGNOSTIC_TIMEOUT_MS: "7000",
    },
    { availableParallelism: () => 6 },
  );
  assert.equal(timeoutOverrides.searchTimeoutMs, 1_000);
  assert.equal(timeoutOverrides.diagnosticTimeoutMs, 7_000);
  assert.equal(timeoutOverrides.pcSearchTimeoutMs, 2_000);
  assert.equal(timeoutOverrides.reverseSearchTimeoutMs, 2_000);
  assert.equal(timeoutOverrides.buildSearchTimeoutMs, 3_000);
  assert.equal(timeoutOverrides.setupSearchTimeoutMs, 4_000);
  assert.equal(timeoutOverrides.forwardSearchTimeoutMs, 5_000);
  assert.equal(timeoutOverrides.structureSearchTimeoutMs, 6_000);

  const legacyTimeouts = loadClearraJobServiceConfig(
    {
      CLEARRA_JOB_TOKEN: "job-token",
      CLEARRA_REVERSE_SEARCH_TIMEOUT_MS: "2100",
      CLEARRA_FORWARD_SEARCH_TIMEOUT_MS: "9100",
    },
    { availableParallelism: () => 6 },
  );
  assert.equal(legacyTimeouts.pcSearchTimeoutMs, 2_100);
  assert.equal(legacyTimeouts.buildSearchTimeoutMs, 9_100);
  assert.equal(legacyTimeouts.setupSearchTimeoutMs, 9_100);
  assert.equal(legacyTimeouts.forwardSearchTimeoutMs, 9_100);
  assert.equal(legacyTimeouts.structureSearchTimeoutMs, 9_100);

  const reserveCore = loadClearraJobServiceConfig(
    {
      CLEARRA_JOB_TOKEN: "job-token",
      CLEARRA_SEARCH_WORKERS_PER_SESSION: "auto",
      CLEARRA_USE_ALL_LOGICAL_PROCESSORS: "0",
    },
    { availableParallelism: () => 6 },
  );
  assert.equal(reserveCore.searchWorkersPerSession, 5);
  assert.equal(reserveCore.useAllLogicalProcessors, false);
  assert.equal(reserveCore.expectedVcpus, undefined);
  assert.deepEqual(
    prepareClearraArguments(["pc", "--lines", "1"], {
      workers: reserveCore.searchWorkersPerSession,
      useAllLogicalProcessors: reserveCore.useAllLogicalProcessors,
      logicalProcessors: reserveCore.processLogicalProcessors,
    }),
    [
      "pc",
      "--lines",
      "1",
      "--no-tablebase",
      "--no-build-dependency-dag",
      "--auto-workers",
      "5",
      "--format",
      "text",
    ],
  );

  assert.throws(
    () => loadClearraJobServiceConfig(
      {
        CLEARRA_JOB_TOKEN: "job-token",
        CLEARRA_SEARCH_WORKERS_PER_SESSION: "7",
      },
      { availableParallelism: () => 6 },
    ),
    /per-job runtime limit of 6/,
  );
  assert.throws(
    () => loadClearraJobServiceConfig(
      {
        CLEARRA_JOB_TOKEN: "job-token",
        CLEARRA_USE_ALL_LOGICAL_PROCESSORS: "yes",
      },
      { availableParallelism: () => 6 },
    ),
    /boolean setting is invalid/,
  );
});

test("explicit 8-worker Cloud Run policy survives startup CPU boost visibility", () => {
  const config = loadClearraJobServiceConfig(
    {
      K_SERVICE: "clearra-current-job",
      CLEARRA_JOB_TOKEN: "job-token",
      CLEARRA_EXPECTED_VCPUS: "8",
      CLEARRA_SEARCH_WORKERS_PER_SESSION: "8",
      CLEARRA_USE_ALL_LOGICAL_PROCESSORS: "1",
      CLEARRA_MAX_CONCURRENT_JOBS: "1",
    },
    { availableParallelism: () => 9 },
  );

  assert.equal(config.processLogicalProcessors, 9);
  assert.equal(config.expectedVcpus, 8);
  assert.equal(config.searchWorkersPerSession, 8);
  assert.equal(config.useAllLogicalProcessors, true);
  assert.deepEqual(
    prepareClearraArguments(["pc", "--lines", "2", "--queue", "IJLOO"], {
      workers: config.searchWorkersPerSession,
      useAllLogicalProcessors: config.useAllLogicalProcessors,
      logicalProcessors: config.processLogicalProcessors,
      outputFormat: "json",
      includeSolutionData: true,
    }),
    [
      "pc",
      "--lines",
      "2",
      "--queue",
      "IJLOO",
      "--no-tablebase",
      "--no-build-dependency-dag",
      "--auto-workers",
      "8",
      "--use-all-cpu-threads",
      "--format",
      "json",
      "--include-solution-data",
    ],
  );

  assert.throws(
    () =>
      loadClearraJobServiceConfig(
        {
          K_SERVICE: "clearra-current-job",
          CLEARRA_JOB_TOKEN: "job-token",
          CLEARRA_EXPECTED_VCPUS: "8",
          CLEARRA_SEARCH_WORKERS_PER_SESSION: "9",
          CLEARRA_USE_ALL_LOGICAL_PROCESSORS: "1",
          CLEARRA_MAX_CONCURRENT_JOBS: "1",
        },
        { availableParallelism: () => 9 },
      ),
    /per-job runtime limit of 8/,
  );

  const nonCloud = loadClearraJobServiceConfig(
    {
      CLEARRA_JOB_TOKEN: "job-token",
      CLEARRA_EXPECTED_VCPUS: "8",
      CLEARRA_SEARCH_WORKERS_PER_SESSION: "auto",
      CLEARRA_USE_ALL_LOGICAL_PROCESSORS: "1",
    },
    { availableParallelism: () => 9 },
  );
  assert.equal(nonCloud.processLogicalProcessors, 9);
  assert.equal(nonCloud.expectedVcpus, 8);
  assert.equal(nonCloud.searchWorkersPerSession, 8);
});

test("unauthenticated local job service is restricted to loopback", () => {
  assert.throws(
    () => loadClearraJobServiceConfig({
      CLEARRA_JOB_SERVICE_ALLOW_UNAUTHENTICATED: "1",
    }),
    /limited to a loopback listen host/,
  );
  const config = loadClearraJobServiceConfig({
    CLEARRA_JOB_SERVICE_ALLOW_UNAUTHENTICATED: "1",
    CLEARRA_LISTEN_HOST: "127.0.0.1",
  });
  assert.equal(config.host, "127.0.0.1");
  assert.equal(config.authorizationToken, null);
});

test("job service partitions its CPU limit across concurrent jobs", () => {
  const config = loadClearraJobServiceConfig(
    {
      CLEARRA_JOB_TOKEN: "job-token",
      CLEARRA_MAX_CONCURRENT_JOBS: "2",
      CLEARRA_SEARCH_WORKERS_PER_SESSION: "auto",
    },
    { availableParallelism: () => 6 },
  );

  assert.equal(config.maxConcurrentJobs, 2);
  assert.equal(config.searchWorkersPerSession, 3);
  assert.equal(config.useAllLogicalProcessors, true);
  assert.deepEqual(
    prepareClearraArguments(["pc", "--lines", "1"], {
      workers: config.searchWorkersPerSession,
      useAllLogicalProcessors: config.useAllLogicalProcessors,
      logicalProcessors: config.processLogicalProcessors,
    }),
    [
      "pc",
      "--lines",
      "1",
      "--no-tablebase",
      "--no-build-dependency-dag",
      "--auto-workers",
      "3",
      "--use-all-cpu-threads",
      "--format",
      "text",
    ],
  );
  assert.throws(
    () => loadClearraJobServiceConfig(
      {
        CLEARRA_JOB_TOKEN: "job-token",
        CLEARRA_MAX_CONCURRENT_JOBS: "2",
        CLEARRA_SEARCH_WORKERS_PER_SESSION: "4",
      },
      { availableParallelism: () => 6 },
    ),
    /per-job runtime limit of 3/,
  );
});

test("job runner sends curated sfinder argv without shell interpretation", async () => {
  let invocation;
  const runner = new ClearraCommandRunner(
    {
      executable: "clearra",
      processLogicalProcessors: 6,
      expectedVcpus: 6,
      searchWorkersPerSession: 6,
      useAllLogicalProcessors: true,
      searchTimeoutMs: 5_000,
      maxOutputBytes: 1024 * 1024,
      terminationGraceMs: 100,
    },
    {
      spawn: (executable, arguments_, options) => {
        invocation = { executable, arguments_, options };
        const child = new EventEmitter();
        child.stdout = new PassThrough();
        child.stderr = new PassThrough();
        child.exitCode = null;
        child.signalCode = null;
        child.kill = () => true;
        queueMicrotask(() => {
          child.exitCode = 0;
          child.emit("close", 0, null);
        });
        return child;
      },
    },
  );

  const result = await runner.execute({
    arguments: [
      "sfinder",
      "chance",
      "v115@vhAAgH",
      "P7P3",
      "4",
      "--workers",
      "99",
      "--format",
      "json",
    ],
    deadlineUnixMs: Date.now() + 5_000,
    maxOutputBytes: 1024 * 1024,
  });

  assert.equal(result.exitCode, 0);
  assert.equal(invocation.executable, "clearra");
  assert.equal(invocation.options.shell, false);
  assert.equal(invocation.options.env.CLEARRA_EXPECTED_VCPUS, "6");
  assert.deepEqual(invocation.arguments_, [
    "sfinder",
    "chance",
    "v115@vhAAgH",
    "P7P3",
    "4",
    "--auto-workers",
    "6",
    "--use-all-cpu-threads",
    "--format",
    "json",
    "--include-solution-data",
  ]);
});

test("job runner transports one exact bounded render artifact and cleans its private path", async () => {
  const bytes = Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    Buffer.from("runner-artifact"),
  ]);
  const sha256 = createHash("sha256").update(bytes).digest("hex");
  let outputPath;
  const runner = new ClearraCommandRunner(
    {
      executable: "clearra",
      processLogicalProcessors: 1,
      searchWorkersPerSession: 1,
      useAllLogicalProcessors: false,
      utilitySearchTimeoutMs: 5_000,
      maxOutputBytes: 1024 * 1024,
      maxArtifactBytes: 4096,
      terminationGraceMs: 100,
    },
    {
      spawn: (_executable, arguments_) => {
        outputPath = arguments_[arguments_.indexOf("--output") + 1];
        const child = new EventEmitter();
        child.stdout = new PassThrough();
        child.stderr = new PassThrough();
        child.exitCode = null;
        child.signalCode = null;
        child.kill = () => true;
        queueMicrotask(() => {
          void writeFile(outputPath, bytes).then(() => {
            child.stdout.end(JSON.stringify({
              kind: "render-artifact.v1",
              contract_id: "render-artifact.v1",
              result_kind: "render",
              payload_kind: "render-artifact",
              payload: {
                document_format: "ctk3",
                artifact_format: "png",
                selected_page_number: 1,
                document_page_count: 1,
                media_type: "image/png",
                filename: "clearra-render-page-0001.png",
                byte_length: bytes.length,
                sha256,
                render_exact: true,
                skin_id: "clearra-exact-v1",
                product_max_bytes: 4096,
                transport_max_bytes: 4096,
              },
              generated_files: [{
                target: outputPath,
                bytes: bytes.length,
                sha256,
                target_owned: true,
              }],
            }));
            child.exitCode = 0;
            child.emit("close", 0, null);
          });
        });
        return child;
      },
    },
  );

  const result = await runner.execute({
    arguments: [
      "utility",
      "render",
      "--format",
      "ctk3",
      "--document",
      "ctk3b_test",
      "--artifact-format",
      "png",
      "--page",
      "1",
    ],
    timeoutClass: "utility_bounded",
    deadlineUnixMs: Date.now() + 5_000,
    maxOutputBytes: 1024 * 1024,
    maxArtifactBytes: 4096,
  });

  assert.equal(result.artifact.mediaType, "image/png");
  assert.equal(result.artifact.sha256, sha256);
  assert.equal(Buffer.from(result.artifact.bytesBase64, "base64").equals(bytes), true);
  assert.equal(JSON.parse(result.stdout).generated_files, undefined);
  assert.equal(result.stdout.includes(outputPath), false);
  assert.equal(existsSync(outputPath), false);
  assert.equal(existsSync(dirname(outputPath)), false);
});

test("render runner cleans its private output on start failure, timeout, and cancellation", async () => {
  for (const mode of ["start-failure", "timeout", "cancel"]) {
    let outputPath;
    const controller = new AbortController();
    const runner = new ClearraCommandRunner(
      {
        executable: "clearra",
        processLogicalProcessors: 1,
        searchWorkersPerSession: 1,
        useAllLogicalProcessors: false,
        utilitySearchTimeoutMs: mode === "timeout" ? 5 : 5_000,
        maxOutputBytes: 1024 * 1024,
        maxArtifactBytes: 4096,
        terminationGraceMs: 100,
      },
      {
        // A real ChildProcess keeps the event loop alive until close. This
        // synthetic child does not, so keep its deadline timer referenced to
        // make the timeout branch deterministic on every supported Node 22+
        // platform while preserving the runner's production unref policy.
        setTimeout: (callback, timeoutMs) => {
          const timer = setTimeout(callback, timeoutMs);
          timer.unref = () => timer;
          return timer;
        },
        spawn: (_executable, arguments_) => {
          outputPath = arguments_[arguments_.indexOf("--output") + 1];
          if (mode === "start-failure") throw new Error("fixture start failure");
          const child = new EventEmitter();
          child.stdout = new PassThrough();
          child.stderr = new PassThrough();
          child.exitCode = null;
          child.signalCode = null;
          child.kill = (signal) => {
            queueMicrotask(() => {
              child.signalCode = signal;
              child.emit("close", null, signal);
            });
            return true;
          };
          return child;
        },
      },
    );
    const pending = runner.execute(
      {
        arguments: [
          "utility", "render",
          "--format", "ctk3",
          "--document", "ctk3b_test",
          "--artifact-format", "png",
          "--page", "1",
        ],
        timeoutClass: "utility_bounded",
        deadlineUnixMs: Date.now() + 5_000,
        maxOutputBytes: 1024 * 1024,
        maxArtifactBytes: 4096,
      },
      { signal: controller.signal },
    );
    if (mode === "cancel") controller.abort();
    await assert.rejects(pending, mode === "start-failure" ? /start/u : /cancelled|deadline/u);
    assert.equal(typeof outputPath, "string");
    assert.equal(existsSync(outputPath), false, `${mode} artifact residue`);
    assert.equal(existsSync(dirname(outputPath)), false, `${mode} directory residue`);
  }
});

test("job runner clamps every canonical timeout family and preserves legacy inference", async () => {
  const observedTimeouts = [];
  const runner = new ClearraCommandRunner(
    {
      executable: "clearra",
      processLogicalProcessors: 1,
      searchWorkersPerSession: 1,
      useAllLogicalProcessors: false,
      searchTimeoutMs: 2_000,
      pcSearchTimeoutMs: 5_000,
      buildSearchTimeoutMs: 6_000,
      setupSearchTimeoutMs: 7_000,
      forwardSearchTimeoutMs: 8_000,
      structureSearchTimeoutMs: 9_000,
      diagnosticTimeoutMs: 3_000,
      maxOutputBytes: 1024 * 1024,
      terminationGraceMs: 100,
    },
    {
      now: () => 10_000,
      setTimeout: (_callback, timeoutMs) => {
        observedTimeouts.push(timeoutMs);
        return { unref() {} };
      },
      clearTimeout() {},
      spawn: () => {
        const child = new EventEmitter();
        child.stdout = new PassThrough();
        child.stderr = new PassThrough();
        child.exitCode = null;
        child.signalCode = null;
        child.kill = () => true;
        queueMicrotask(() => {
          child.exitCode = 0;
          child.emit("close", 0, null);
        });
        return child;
      },
    },
  );
  const execute = (arguments_, deadlineUnixMs = 99_000, timeoutClass) => runner.execute({
    arguments: arguments_,
    ...(timeoutClass ? { timeoutClass } : {}),
    deadlineUnixMs,
    maxOutputBytes: 1024 * 1024,
  });

  // Omitted timeoutClass is the compatibility path for older job requests.
  await execute(["pc"]);
  await execute(["build-probability"], 99_000, "build_long");
  await execute(["setup-finder", "--remaining", "TI"], 99_000, "setup_long");
  await execute(["damage"], 99_000, "forward_long");
  await execute(["spin-structure"], 99_000, "structure_long");
  await execute(["sfinder", "verify", "pc"], 99_000, "diagnostic");
  await execute(["damage"], 10_750, "forward_long");

  assert.deepEqual(observedTimeouts, [5_000, 6_000, 7_000, 8_000, 9_000, 3_000, 750]);
  await assert.rejects(
    execute(["damage"], 99_000, "pc_reverse"),
    /does not match/,
  );
});

test("job runner retains its slot until a cancelled process closes", async () => {
  const child = new EventEmitter();
  child.stdout = new PassThrough();
  child.stderr = new PassThrough();
  child.exitCode = null;
  child.signalCode = null;
  child.signals = [];
  child.kill = (signal) => {
    child.signals.push(signal);
    if (signal === "SIGTERM") {
      setTimeout(() => {
        child.signalCode = signal;
        child.emit("close", null, signal);
      }, 25);
    }
    return true;
  };
  const runner = new ClearraCommandRunner(
    {
      executable: "clearra",
      searchWorkersPerSession: 5,
      useAllLogicalProcessors: false,
      searchTimeoutMs: 5_000,
      maxOutputBytes: 1024 * 1024,
      terminationGraceMs: 100,
    },
    { spawn: () => child },
  );
  const controller = new AbortController();
  let outcome;
  const observed = runner.execute(
    {
      arguments: ["pc", "--lines", "4"],
      deadlineUnixMs: Date.now() + 5_000,
      maxOutputBytes: 1024 * 1024,
    },
    { signal: controller.signal },
  ).then(
    (value) => { outcome = { value }; },
    (error) => { outcome = { error }; },
  );

  controller.abort();
  await new Promise((resolve) => setTimeout(resolve, 5));
  assert.equal(outcome, undefined);
  await observed;
  assert.match(outcome.error.message, /cancelled/);
  assert.deepEqual(child.signals, ["SIGTERM"]);
});

test("job service executes an authenticated synchronous idempotent job", async (t) => {
  let executions = 0;
  const acceptedJobs = [];
  const operationalLines = [];
  const config = {
    host: "127.0.0.1",
    port: 0,
    authorizationToken: "job-token",
    allowUnauthenticated: false,
    maxRequestBodyBytes: 64 * 1024,
    maxOutputBytes: 1024 * 1024,
    searchTimeoutMs: 5_000,
    terminationGraceMs: 10,
    maxConcurrentJobs: 1,
    completedJobTtlMs: 60_000,
    maxRetainedJobs: 16,
    processLogicalProcessors: 6,
    searchWorkersPerSession: 5,
    useAllLogicalProcessors: false,
  };
  const runner = {
    async execute(job) {
      executions += 1;
      acceptedJobs.push(job);
      return {
        exitCode: 0,
        signal: null,
        stdout: `${job.arguments.join(" ")} complete`,
        stderr: "",
      };
    },
  };
  const service = new ClearraJobService(config, runner, {
    operationalScope: "job",
    logger: {
      info(value) { operationalLines.push(value); },
      error(value) { operationalLines.push(value); },
    },
  });
  const address = await service.listen();
  t.after(() => service.close());
  const endpoint = `http://127.0.0.1:${address.port}/jobs`;
  const body = {
    protocol: "clearra.job.v1",
    id: "discord-123",
    kind: "clearra.command",
    arguments: ["pc", "--lines", "4", "--format", "text"],
    deadlineUnixMs: Date.now() + 5_000,
    maxOutputBytes: 1024 * 1024,
    maxArtifactBytes: 1024 * 1024,
  };
  const request = (overrides = {}) => {
    const payload = { ...body, ...overrides };
    return fetch(endpoint, {
      method: "POST",
      headers: {
        authorization: "Bearer job-token",
        "content-type": "application/json",
        "idempotency-key": payload.id,
      },
      body: JSON.stringify(payload),
    });
  };

  const first = await request();
  assert.equal(first.status, 200);
  const firstJob = await first.json();
  assert.equal(firstJob.state, "completed");
  assert.equal(firstJob.result.exitCode, 0);
  assert.match(firstJob.result.stdout, /pc --lines 4/);
  assert.equal(acceptedJobs[0].timeoutClass, "pc_reverse");
  assert.ok(acceptedJobs[0].timeoutLimitMs > 0);
  assert.ok(acceptedJobs[0].timeoutLimitMs <= 5_000);

  const second = await request({
    deadlineUnixMs: body.deadlineUnixMs + 1_000,
  });
  assert.equal(second.status, 200);
  assert.equal((await second.json()).state, "completed");
  assert.equal(executions, 1);

  const conflicting = await request({ arguments: ["pc", "--lines", "2"] });
  assert.equal(conflicting.status, 409);
  assert.equal(executions, 1);

  const mismatchedClass = await request({
    id: "discord-timeout-mismatch",
    arguments: ["damage"],
    timeoutClass: "pc_reverse",
  });
  assert.equal(mismatchedClass.status, 400);
  assert.equal(executions, 1);

  const explicitClass = await request({
    id: "discord-forward",
    arguments: ["damage"],
    timeoutClass: "forward_long",
    deadlineUnixMs: Date.now() + 5_000,
  });
  assert.equal(explicitClass.status, 200);
  assert.equal(executions, 2);
  assert.equal(acceptedJobs[1].timeoutClass, "forward_long");
  assert.ok(acceptedJobs[1].timeoutLimitMs > 0);
  assert.ok(acceptedJobs[1].timeoutLimitMs <= 5_000);

  assert.equal(operationalLines.length, 2);
  const forwardRecord = JSON.parse(operationalLines[1]);
  assert.equal(forwardRecord.timeoutClass, "forward_long");
  assert.equal(forwardRecord.timeoutMs, acceptedJobs[1].timeoutLimitMs);
  assert.doesNotMatch(operationalLines[1], /arguments|--lines|discord-forward/i);
});

test("job service exposes and enforces the exact runtime identity before execution", async (t) => {
  let executions = 0;
  const config = {
    host: "127.0.0.1",
    port: 0,
    authorizationToken: "job-token",
    allowUnauthenticated: false,
    maxRequestBodyBytes: 64 * 1024,
    maxOutputBytes: 1024 * 1024,
    searchTimeoutMs: 5_000,
    terminationGraceMs: 10,
    maxConcurrentJobs: 1,
    completedJobTtlMs: 60_000,
    maxRetainedJobs: 16,
    processLogicalProcessors: 6,
    searchWorkersPerSession: 5,
    useAllLogicalProcessors: false,
    runtimeIdentity: TEST_RUNTIME_IDENTITY,
  };
  const service = new ClearraJobService(config, {
    async execute() {
      executions += 1;
      return { exitCode: 0, signal: null, stdout: "ok", stderr: "" };
    },
  });
  const address = await service.listen();
  t.after(() => service.close());
  const base = `http://127.0.0.1:${address.port}`;

  const health = await (await fetch(`${base}/health`)).json();
  assert.deepEqual(health.runtime, TEST_RUNTIME_IDENTITY);

  const body = {
    protocol: "clearra.job.v1",
    id: "identity-job",
    kind: "clearra.command",
    arguments: ["pc", "--lines", "4"],
    deadlineUnixMs: Date.now() + 5_000,
    maxOutputBytes: 1024 * 1024,
    maxArtifactBytes: 1024 * 1024,
  };
  const submit = (expectedRuntime) => fetch(`${base}/jobs`, {
    method: "POST",
    headers: {
      authorization: "Bearer job-token",
      "content-type": "application/json",
      "idempotency-key": body.id,
    },
    body: JSON.stringify({ ...body, expectedRuntime }),
  });

  assert.equal((await submit(null)).status, 409);
  assert.equal(executions, 0);
  const accepted = await submit(TEST_RUNTIME_IDENTITY);
  assert.equal(accepted.status, 200);
  const terminal = await accepted.json();
  assert.deepEqual(terminal.runtime, TEST_RUNTIME_IDENTITY);
  assert.equal(executions, 1);
});

test("production job-service configuration fails closed without immutable identity", () => {
  assert.throws(
    () => loadClearraJobServiceConfig({
      NODE_ENV: "production",
      CLEARRA_JOB_SERVICE_ALLOW_UNAUTHENTICATED: "1",
      CLEARRA_LISTEN_HOST: "127.0.0.1",
      CLEARRA_SOURCE_COMMIT: "a".repeat(40),
      CLEARRA_ENGINE_BUILD_ID: "a".repeat(40),
    }),
    /must declare its contract schema version/,
  );
  assert.throws(
    () => loadClearraJobServiceConfig({
      NODE_ENV: "production",
      CLEARRA_JOB_SERVICE_ALLOW_UNAUTHENTICATED: "1",
      CLEARRA_LISTEN_HOST: "127.0.0.1",
      CLEARRA_SEARCH_CONTRACT_REVISION: SEARCH_CONTRACT_REVISION,
      CLEARRA_SUPPLY_SEMANTICS_ID: SUPPLY_SEMANTICS_ID,
      CLEARRA_ARTIFACT_SCHEMA_VERSION: ARTIFACT_SCHEMA_VERSION,
    }),
    /full Git commit SHA/,
  );
  const config = loadClearraJobServiceConfig({
    NODE_ENV: "production",
    CLEARRA_JOB_SERVICE_ALLOW_UNAUTHENTICATED: "1",
    CLEARRA_LISTEN_HOST: "127.0.0.1",
    CLEARRA_SOURCE_COMMIT: "a".repeat(40),
    CLEARRA_ENGINE_BUILD_ID: "a".repeat(40),
    CLEARRA_SEARCH_CONTRACT_REVISION: SEARCH_CONTRACT_REVISION,
    CLEARRA_SUPPLY_SEMANTICS_ID: SUPPLY_SEMANTICS_ID,
    CLEARRA_ARTIFACT_SCHEMA_VERSION: ARTIFACT_SCHEMA_VERSION,
  });
  assert.deepEqual(config.runtimeIdentity, TEST_RUNTIME_IDENTITY);

  const legacy = normalizeRuntimeIdentity({
    ...TEST_RUNTIME_IDENTITY,
    contractSchemaVersion: LEGACY_SEARCH_CONTRACT_REVISION,
    supplySemanticsId: LEGACY_SUPPLY_SEMANTICS_ID,
    artifactSchemaVersion: LEGACY_ARTIFACT_SCHEMA_VERSION,
  });
  assert.notDeepEqual(legacy, TEST_RUNTIME_IDENTITY);
  assert.equal(runtimeIdentityMatches(legacy, TEST_RUNTIME_IDENTITY), false);
});

test("job service rejects unauthenticated work", async (t) => {
  const config = {
    host: "127.0.0.1",
    port: 0,
    authorizationToken: "job-token",
    allowUnauthenticated: false,
    maxRequestBodyBytes: 64 * 1024,
    maxOutputBytes: 1024 * 1024,
    searchTimeoutMs: 5_000,
    terminationGraceMs: 10,
    maxConcurrentJobs: 1,
    completedJobTtlMs: 60_000,
    maxRetainedJobs: 16,
    processLogicalProcessors: 6,
    searchWorkersPerSession: 5,
    useAllLogicalProcessors: false,
  };
  const service = new ClearraJobService(config, {
    async execute() {
      throw new Error("must not run");
    },
  });
  const address = await service.listen();
  t.after(() => service.close());

  const response = await fetch(`http://127.0.0.1:${address.port}/jobs`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: "{}",
  });
  assert.equal(response.status, 401);
});
