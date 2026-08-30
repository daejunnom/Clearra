import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import {
  lstat,
  open,
  readFile,
} from "node:fs/promises";
import { isAbsolute, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  canonicalJson,
  canonicalSha256,
  canonicalTimestamp,
  rejectSecretMaterial,
  requireExactKeys,
  requireNonEmptyString,
  requirePlainObject,
  requireSha256,
  requireSourceCommit,
  sealCanonicalReport,
  verifyCanonicalReportHash,
} from "./canonical-release-evidence.mjs";

export const PRODUCTION_OBSERVATION_SCHEMA_ID =
  "clearra.production-observation.v1";
export const PRODUCTION_SURFACE_PROBE_SCHEMA_ID =
  "clearra.production-surface-probe.v1";
export const PRODUCTION_PROBE_SPEC_SCHEMA_ID =
  "clearra.production-observation-probe-spec.v1";
export const PRODUCTION_OBSERVATION_SECONDS = 1200;

const REQUIRED_SURFACES = Object.freeze([
  "cloud",
  "discord",
  "oracle",
  "pages",
]);
const IMAGE_DIGEST = /^sha256:[0-9a-f]{64}$/u;
const DISCORD_SNOWFLAKE = /^\d{17,20}$/u;
const RELEASE_VERSION = "0.8.0";
const CONTRACT_SCHEMA_VERSION = "clearra.search.contract.v2";
const SUPPLY_SEMANTICS_ID =
  "clearra.supply.projected-terminal-lookahead.v1";
const ARTIFACT_SCHEMA_VERSION = "clearra.solution-data.v1";
const MAX_PROBE_OUTPUT_BYTES = 64 * 1024;
const ORACLE_PROBE_SCHEMA_ID = "clearra.oracle.candidate-observation.v1";
const ORACLE_BOOT_ID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/u;
const ORACLE_NONCE = /^[0-9a-f]{64}$/u;
const SECRET_ARGUMENT = /(?:secret|token|password|credential|authorization|api[-_]?key|private[-_]?key|bearer)/iu;

export async function observeProductionSurfaces({
  sourceCommit,
  probes,
  probeSpec,
  durationSeconds = PRODUCTION_OBSERVATION_SECONDS,
  intervalSeconds = 30,
  clock = systemClock,
}) {
  const commit = requireSourceCommit(sourceCommit);
  const probeMap = validateProbeFunctions(probes);
  validateProductionProbeSpec(probeSpec, commit);
  const probeSpecSha256 = canonicalSha256(probeSpec);
  const probeAdapters = Object.freeze(probeSpec.probes.map((adapter) =>
    Object.freeze({ surface: adapter.surface, sha256: adapter.sha256 })));
  requirePositiveDuration(durationSeconds, "observation duration");
  requirePositiveDuration(intervalSeconds, "observation interval");
  if (intervalSeconds > durationSeconds) {
    throw new Error("production observation interval exceeds its duration");
  }
  if (
    !clock ||
    typeof clock.now !== "function" ||
    typeof clock.wait !== "function"
  ) {
    throw new Error("production observation clock is invalid");
  }

  const startedMilliseconds = exactClockMilliseconds(clock.now());
  const startedAt = new Date(startedMilliseconds).toISOString();
  const observations = new Map(
    REQUIRED_SURFACES.map((surface) => [surface, []]),
  );
  const identities = new Map();
  let sequence = 0;
  let lastObservedMilliseconds = startedMilliseconds;

  for (;;) {
    const results = await Promise.all(REQUIRED_SURFACES.map(async (surface) => {
      const result = await probeMap.get(surface)({
        surface,
        sourceCommit: commit,
        sequence,
      });
      return validateSurfaceProbeResult(result, {
        expectedSurface: surface,
        expectedSourceCommit: commit,
      });
    }));
    const observedMilliseconds = exactClockMilliseconds(clock.now());
    lastObservedMilliseconds = observedMilliseconds;
    if (observedMilliseconds < startedMilliseconds) {
      throw new Error("production observation clock moved backwards");
    }
    const observedAt = new Date(observedMilliseconds).toISOString();

    for (const result of results) {
      const identitySha256 = canonicalSha256(result.identity);
      const existingIdentity = identities.get(result.surface);
      if (
        existingIdentity &&
        (existingIdentity.sha256 !== identitySha256 ||
          canonicalJson(existingIdentity.value) !== canonicalJson(result.identity))
      ) {
        throw new Error(
          `${result.surface} production identity changed during observation`,
        );
      }
      if (!existingIdentity) {
        identities.set(result.surface, {
          sha256: identitySha256,
          value: result.identity,
        });
      }
      observations.get(result.surface).push(Object.freeze({
        sequence,
        observed_at: observedAt,
        identity_sha256: identitySha256,
        freshness: result.freshness,
      }));
    }
    sequence += 1;

    const elapsedMilliseconds = observedMilliseconds - startedMilliseconds;
    if (elapsedMilliseconds >= durationSeconds * 1000) break;
    const remainingMilliseconds = durationSeconds * 1000 - elapsedMilliseconds;
    await clock.wait(Math.min(intervalSeconds * 1000, remainingMilliseconds));
  }

  const endedMilliseconds = lastObservedMilliseconds;
  const duration = Math.floor((endedMilliseconds - startedMilliseconds) / 1000);
  if (duration < durationSeconds) {
    throw new Error("production observation clock did not reach its duration");
  }
  const report = sealCanonicalReport({
    schema_id: PRODUCTION_OBSERVATION_SCHEMA_ID,
    source_commit: commit,
    started_at: startedAt,
    ended_at: new Date(endedMilliseconds).toISOString(),
    duration_seconds: duration,
    interval_seconds: intervalSeconds,
    probe_spec_sha256: probeSpecSha256,
    probe_adapters: probeAdapters,
    status: "passed",
    surfaces: REQUIRED_SURFACES.map((surface) => {
      const identity = identities.get(surface);
      const samples = observations.get(surface);
      return Object.freeze({
        surface,
        identity: identity.value,
        identity_sha256: identity.sha256,
        observation_count: samples.length,
        observations: Object.freeze(samples),
      });
    }),
  });
  validateProductionObservationReport(report, {
    expectedSourceCommit: commit,
    minimumDurationSeconds: durationSeconds,
  });
  return report;
}

export function validateProductionObservationReport(
  value,
  {
    expectedSourceCommit,
    minimumDurationSeconds = PRODUCTION_OBSERVATION_SECONDS,
  } = {},
) {
  requireExactKeys(value, [
    "schema_id",
    "source_commit",
    "started_at",
    "ended_at",
    "duration_seconds",
    "interval_seconds",
    "probe_spec_sha256",
    "probe_adapters",
    "status",
    "surfaces",
    "report_sha256",
  ], "production observation report");
  if (value.schema_id !== PRODUCTION_OBSERVATION_SCHEMA_ID) {
    throw new Error("production observation report schema is invalid");
  }
  verifyCanonicalReportHash(value, "production observation report");
  requireSourceCommit(value.source_commit, "observation source commit");
  if (
    expectedSourceCommit !== undefined &&
    value.source_commit !== expectedSourceCommit
  ) {
    throw new Error("production observation source differs from the release source");
  }
  const startedAt = canonicalTimestamp(value.started_at, "observation start time");
  const endedAt = canonicalTimestamp(value.ended_at, "observation end time");
  if (
    !Number.isSafeInteger(value.duration_seconds) ||
    value.duration_seconds < minimumDurationSeconds
  ) {
    throw new Error(
      `production observation must last at least ${minimumDurationSeconds} seconds`,
    );
  }
  if (
    Math.floor((Date.parse(endedAt) - Date.parse(startedAt)) / 1000) !==
      value.duration_seconds
  ) {
    throw new Error("production observation duration differs from its timestamps");
  }
  requirePositiveDuration(value.interval_seconds, "observation interval");
  if (value.interval_seconds > value.duration_seconds) {
    throw new Error("production observation interval exceeds its duration");
  }
  if (value.status !== "passed") {
    throw new Error("production observation report did not pass");
  }
  requireSha256(value.probe_spec_sha256, "production probe spec SHA-256");
  validateProbeAdapterAuthority(value.probe_adapters);
  if (!Array.isArray(value.surfaces) || value.surfaces.length !== 4) {
    throw new Error("production observation must contain exactly four surfaces");
  }

  const surfaceNames = [];
  for (const surfaceReport of value.surfaces) {
    requireExactKeys(surfaceReport, [
      "surface",
      "identity",
      "identity_sha256",
      "observation_count",
      "observations",
    ], "production surface observation");
    if (!REQUIRED_SURFACES.includes(surfaceReport.surface)) {
      throw new Error(`unexpected production observation surface: ${String(surfaceReport.surface)}`);
    }
    surfaceNames.push(surfaceReport.surface);
    validateSurfaceIdentity(
      surfaceReport.surface,
      surfaceReport.identity,
      value.source_commit,
    );
    requireSha256(
      surfaceReport.identity_sha256,
      `${surfaceReport.surface} identity SHA-256`,
    );
    if (canonicalSha256(surfaceReport.identity) !== surfaceReport.identity_sha256) {
      throw new Error(`${surfaceReport.surface} observation identity SHA-256 differs`);
    }
    if (
      !Number.isSafeInteger(surfaceReport.observation_count) ||
      surfaceReport.observation_count < 2 ||
      !Array.isArray(surfaceReport.observations) ||
      surfaceReport.observations.length !== surfaceReport.observation_count
    ) {
      throw new Error(`${surfaceReport.surface} observation count is invalid`);
    }
    validateSurfaceSamples(
      surfaceReport.surface,
      surfaceReport.observations,
      surfaceReport.identity_sha256,
      startedAt,
      endedAt,
      surfaceReport.identity,
    );
  }
  assertExactIdentitySet(surfaceNames, REQUIRED_SURFACES, "production surfaces");
  return value;
}

export function validateSurfaceProbeResult(
  value,
  { expectedSurface, expectedSourceCommit } = {},
) {
  requireExactKeys(value, [
    "schema_id",
    "surface",
    "source_commit",
    "identity",
    "freshness",
  ], "production surface probe result");
  if (value.schema_id !== PRODUCTION_SURFACE_PROBE_SCHEMA_ID) {
    throw new Error("production surface probe schema is invalid");
  }
  if (!REQUIRED_SURFACES.includes(value.surface)) {
    throw new Error(`unexpected production probe surface: ${String(value.surface)}`);
  }
  if (expectedSurface !== undefined && value.surface !== expectedSurface) {
    throw new Error("production probe returned the wrong surface");
  }
  requireSourceCommit(value.source_commit, "production probe source commit");
  if (
    expectedSourceCommit !== undefined &&
    value.source_commit !== expectedSourceCommit
  ) {
    throw new Error("production probe source differs from the release source");
  }
  validateSurfaceIdentity(value.surface, value.identity, value.source_commit);
  validateFreshness(value.surface, value.freshness);
  rejectSecretMaterial(value, "production surface probe result");
  return value;
}

export function validateProductionProbeSpec(value, expectedSourceCommit) {
  requireExactKeys(value, [
    "schema_id",
    "source_commit",
    "interval_seconds",
    "probes",
  ], "production observation probe spec");
  if (value.schema_id !== PRODUCTION_PROBE_SPEC_SCHEMA_ID) {
    throw new Error("production observation probe spec schema is invalid");
  }
  requireSourceCommit(value.source_commit, "probe spec source commit");
  if (
    expectedSourceCommit !== undefined &&
    value.source_commit !== expectedSourceCommit
  ) {
    throw new Error("probe spec source differs from the release source");
  }
  requirePositiveDuration(value.interval_seconds, "probe spec interval");
  if (value.interval_seconds > PRODUCTION_OBSERVATION_SECONDS) {
    throw new Error("probe spec interval exceeds the production observation duration");
  }
  if (!Array.isArray(value.probes) || value.probes.length !== 4) {
    throw new Error("probe spec must contain exactly four surface probes");
  }
  const surfaces = [];
  for (const probe of value.probes) {
    requireExactKeys(probe, [
      "surface",
      "runtime",
      "path",
      "sha256",
      "arguments",
      "timeout_seconds",
    ], "production observation probe adapter");
    if (!REQUIRED_SURFACES.includes(probe.surface)) {
      throw new Error(`unexpected probe adapter surface: ${String(probe.surface)}`);
    }
    surfaces.push(probe.surface);
    if (!new Set(["native", "node", "powershell"]).has(probe.runtime)) {
      throw new Error(`${probe.surface} probe runtime must be native, node, or powershell`);
    }
    if (probe.runtime === "powershell" && probe.surface !== "oracle") {
      throw new Error("only the Oracle observation adapter may use powershell runtime");
    }
    if (typeof probe.path !== "string" || !isAbsolute(probe.path)) {
      throw new Error(`${probe.surface} probe path must be absolute`);
    }
    requireSha256(probe.sha256, `${probe.surface} probe SHA-256`);
    if (
      !Array.isArray(probe.arguments) ||
      probe.arguments.some((argument) =>
        typeof argument !== "string" || argument.length === 0 || SECRET_ARGUMENT.test(argument))
    ) {
      throw new Error(`${probe.surface} probe arguments must be non-secret strings`);
    }
    if (probe.surface === "oracle") {
      validateOraclePowerShellArguments(probe.arguments, value.source_commit);
    }
    if (
      !Number.isSafeInteger(probe.timeout_seconds) ||
      probe.timeout_seconds < 1 ||
      probe.timeout_seconds > 60
    ) {
      throw new Error(`${probe.surface} probe timeout must be 1 through 60 seconds`);
    }
  }
  assertExactIdentitySet(surfaces, REQUIRED_SURFACES, "probe adapter surfaces");
  rejectSecretMaterial(value, "production observation probe spec");
  return value;
}

export async function createCommandProbes(spec) {
  validateProductionProbeSpec(spec, spec.source_commit);
  const entries = await Promise.all(spec.probes.map(async (adapter) => {
    const path = resolve(adapter.path);
    await verifyProbeAdapterFile(adapter, path);
    return [adapter.surface, async ({ sequence }) => {
      await verifyProbeAdapterFile(adapter, path);
      const executable = adapter.runtime === "node"
        ? process.execPath
        : adapter.runtime === "powershell"
          ? (process.platform === "win32" ? "pwsh.exe" : "pwsh")
          : path;
      const arguments_ = adapter.runtime === "node"
        ? [path, ...adapter.arguments, "--observation-sequence", String(sequence)]
        : adapter.runtime === "powershell"
          ? ["-NoProfile", "-NonInteractive", "-File", path, ...adapter.arguments]
          : [...adapter.arguments, "--observation-sequence", String(sequence)];
      const raw = await runProbeCommand(
        executable,
        arguments_,
        adapter.timeout_seconds * 1000,
      );
      return adapter.surface === "oracle"
        ? normalizeOracleProbeResult(raw, spec.source_commit, adapter.arguments)
        : raw;
    }];
  }));
  return new Map(entries);
}

function validateSurfaceIdentity(surface, identity, sourceCommit) {
  requirePlainObject(identity, `${surface} production identity`);
  if (surface === "discord") {
    requireExactKeys(identity, [
      "source_commit",
      "application_id",
      "command_catalog_sha256",
      "command_catalog_prior_snapshot_sha256",
      "command_catalog_readback_sha256",
      "command_catalog_sync_report_sha256",
      "command_count",
      "command_names",
      "status",
    ], "Discord production identity");
    requireApplicationId(identity.application_id);
    for (const key of [
      "command_catalog_sha256",
      "command_catalog_prior_snapshot_sha256",
      "command_catalog_readback_sha256",
      "command_catalog_sync_report_sha256",
    ]) requireSha256(identity[key], `Discord identity ${key}`);
    if (
      !Array.isArray(identity.command_names) ||
      identity.command_names.length !== identity.command_count ||
      identity.command_count < 1 ||
      !Number.isSafeInteger(identity.command_count) ||
      identity.command_names.some((name) => typeof name !== "string" || name.length === 0)
    ) {
      throw new Error("Discord observation command name set is invalid");
    }
    const sortedNames = [...identity.command_names].sort((a, b) => a.localeCompare(b, "en"));
    if (
      new Set(sortedNames).size !== sortedNames.length ||
      canonicalJson(sortedNames) !== canonicalJson(identity.command_names)
    ) {
      throw new Error("Discord observation command names are not a sorted unique set");
    }
  } else if (surface === "oracle") {
    requireExactKeys(identity, [
      "source_commit",
      "release_id",
      "release_tree_sha256",
      "settings_sha256",
      "candidate_revision",
      "candidate_url",
      "job_url",
      "deployment_nonce",
      "gateway_pid",
      "gateway_start_monotonic_usec",
      "boot_id",
      "ready_record_observed",
      "status",
    ], "Oracle production identity");
    requireNonEmptyString(identity.release_id, "Oracle release ID");
    requireSha256(identity.release_tree_sha256, "Oracle release tree SHA-256");
    requireSha256(identity.settings_sha256, "Oracle settings SHA-256");
    requireNonEmptyString(identity.candidate_revision, "Oracle candidate revision");
    const candidateUrl = requireCredentialFreeHttpsUrl(
      identity.candidate_url,
      "Oracle candidate URL",
    );
    const candidate = new URL(candidateUrl);
    const jobUrl = requireCredentialFreeHttpsUrl(identity.job_url, "Oracle job URL");
    if (candidate.pathname !== "/" || jobUrl !== new URL("/jobs", candidate).toString()) {
      throw new Error("Oracle candidate and job URL authority is inconsistent");
    }
    if (!ORACLE_NONCE.test(identity.deployment_nonce)) {
      throw new Error("Oracle deployment nonce is invalid");
    }
    if (!Number.isSafeInteger(identity.gateway_pid) || identity.gateway_pid < 1) {
      throw new Error("Oracle process ID is invalid");
    }
    if (
      !Number.isSafeInteger(identity.gateway_start_monotonic_usec) ||
      identity.gateway_start_monotonic_usec < 1 ||
      !ORACLE_BOOT_ID.test(identity.boot_id)
    ) {
      throw new Error("Oracle process start authority is invalid");
    }
    if (identity.ready_record_observed !== true) {
      throw new Error("Oracle process is not READY");
    }
  } else if (surface === "cloud") {
    requireExactKeys(identity, [
      "source_commit",
      "engine_build_id",
      "revision",
      "image_digest",
      "traffic_percent",
      "cpu",
      "memory",
      "concurrency",
      "min_instances",
      "max_instances",
      "startup_cpu_boost",
      "contract_schema_version",
      "supply_semantics_id",
      "artifact_schema_version",
      "job_smoke_report_sha256",
      "stable_url",
      "tagged_url",
      "status",
    ], "Cloud production identity");
    requireNonEmptyString(identity.revision, "Cloud revision");
    if (!IMAGE_DIGEST.test(identity.image_digest)) {
      throw new Error("Cloud image digest is invalid");
    }
    if (
      identity.traffic_percent !== 100 ||
      identity.cpu !== "8" ||
      identity.memory !== "16Gi" ||
      identity.concurrency !== 1 ||
      identity.min_instances !== 0 ||
      identity.max_instances !== 4 ||
      identity.startup_cpu_boost !== true
    ) {
      throw new Error("Cloud traffic or resource authority differs from the release contract");
    }
    if (
      identity.engine_build_id !== sourceCommit ||
      identity.contract_schema_version !== CONTRACT_SCHEMA_VERSION ||
      identity.supply_semantics_id !== SUPPLY_SEMANTICS_ID ||
      identity.artifact_schema_version !== ARTIFACT_SCHEMA_VERSION
    ) {
      throw new Error("Cloud runtime identity differs from the release contract");
    }
    requireSha256(
      identity.job_smoke_report_sha256,
      "Cloud candidate job smoke report SHA-256",
    );
    requireCredentialFreeHttpsUrl(identity.stable_url, "Cloud stable URL");
    requireCredentialFreeHttpsUrl(identity.tagged_url, "Cloud tagged URL");
  } else {
    requireExactKeys(identity, [
      "source_commit",
      "engine_build_id",
      "version",
      "deployment_id",
      "artifact_sha256",
      "base_path",
      "url",
      "status",
    ], "Pages production identity");
    if (identity.engine_build_id !== sourceCommit || identity.version !== RELEASE_VERSION) {
      throw new Error("Pages source, engine, or version identity differs");
    }
    requireNonEmptyString(identity.deployment_id, "Pages deployment ID");
    requireSha256(identity.artifact_sha256, "Pages artifact SHA-256");
    if (
      typeof identity.base_path !== "string" ||
      !/^\/[A-Za-z0-9._-]+$/u.test(identity.base_path)
    ) {
      throw new Error("Pages base path is invalid");
    }
    requireCredentialFreeHttpsUrl(identity.url, "Pages URL");
  }
  if (identity.source_commit !== sourceCommit) {
    throw new Error(`${surface} identity source differs from the release source`);
  }
  if (identity.status !== "active") {
    throw new Error(`${surface} production identity is not active`);
  }
}

function validateSurfaceSamples(
  surface,
  observations,
  expectedIdentitySha256,
  startedAt,
  endedAt,
  identity,
) {
  let priorTimestamp = null;
  let priorOracleOperationAt = null;
  let priorOracleObservedAt = null;
  const freshnessIdentities = new Set();
  observations.forEach((observation, index) => {
    requireExactKeys(observation, [
      "sequence",
      "observed_at",
      "identity_sha256",
      "freshness",
    ], `${surface} observation sample`);
    if (observation.sequence !== index) {
      throw new Error(`${surface} observation sequence is not contiguous`);
    }
    const observedAt = canonicalTimestamp(
      observation.observed_at,
      `${surface} observation timestamp`,
    );
    if (
      Date.parse(observedAt) < Date.parse(startedAt) ||
      Date.parse(observedAt) > Date.parse(endedAt) ||
      (priorTimestamp !== null && Date.parse(observedAt) < Date.parse(priorTimestamp))
    ) {
      throw new Error(`${surface} observation timestamps are out of order or range`);
    }
    priorTimestamp = observedAt;
    if (observation.identity_sha256 !== expectedIdentitySha256) {
      throw new Error(`${surface} identity changed inside the observation samples`);
    }
    validateFreshness(surface, observation.freshness);
    const freshnessIdentity = canonicalJson(observation.freshness);
    if (freshnessIdentities.has(freshnessIdentity)) {
      throw new Error(`${surface} observation reused stale freshness evidence`);
    }
    freshnessIdentities.add(freshnessIdentity);
    if (surface === "oracle") {
      const expectedOperationMarker = canonicalSha256({
        contract: ORACLE_PROBE_SCHEMA_ID,
        source_commit: identity.source_commit,
        candidate_revision: identity.candidate_revision,
        fresh_operation_at: observation.freshness.fresh_operation_at,
        observed_at: observation.freshness.observed_at,
      });
      if (observation.freshness.operation_marker !== expectedOperationMarker) {
        throw new Error("Oracle operation marker differs from its canonical evidence");
      }
      if (
        Date.parse(observation.freshness.observed_at) > Date.parse(observedAt) ||
        Date.parse(observation.freshness.fresh_operation_at) < Date.parse(startedAt) ||
        Date.parse(observation.freshness.fresh_operation_at) >
          Date.parse(observation.freshness.observed_at)
      ) {
        throw new Error("Oracle fresh operation evidence is outside its observation sample");
      }
      if (
        priorOracleOperationAt !== null &&
        Date.parse(observation.freshness.fresh_operation_at) <=
          Date.parse(priorOracleOperationAt)
      ) {
        throw new Error("Oracle fresh operation time did not increase during observation");
      }
      if (
        priorOracleObservedAt !== null &&
        Date.parse(observation.freshness.observed_at) <= Date.parse(priorOracleObservedAt)
      ) {
        throw new Error("Oracle read-only observation time did not increase");
      }
      priorOracleOperationAt = observation.freshness.fresh_operation_at;
      priorOracleObservedAt = observation.freshness.observed_at;
    }
  });
  if (observations.at(-1).observed_at !== endedAt) {
    throw new Error(`${surface} final observation does not close the claimed window`);
  }
}

function validateFreshness(surface, freshness) {
  if (surface === "oracle") {
    requireExactKeys(freshness, [
      "operation_marker",
      "fresh_operation_at",
      "observed_at",
    ], "Oracle observation freshness");
    requireSha256(freshness.operation_marker, "Oracle operation marker");
    canonicalTimestamp(freshness.fresh_operation_at, "Oracle fresh operation time");
    canonicalTimestamp(freshness.observed_at, "Oracle read-only observation time");
  } else if (surface === "discord") {
    requireExactKeys(freshness, [
      "probe_id",
      "readback_sha256",
    ], "Discord observation freshness");
    requireSha256(freshness.probe_id, "Discord probe ID");
    requireSha256(freshness.readback_sha256, "Discord readback SHA-256");
  } else if (surface === "cloud") {
    requireExactKeys(freshness, [
      "probe_id",
      "service_readback_sha256",
      "revision_readback_sha256",
      "stable_health_sha256",
      "tagged_health_sha256",
    ], "Cloud observation freshness");
    for (const [key, value] of Object.entries(freshness)) {
      requireSha256(value, `Cloud freshness ${key}`);
    }
  } else {
    requireExactKeys(freshness, [
      "probe_id",
      "identity_readback_sha256",
    ], "Pages observation freshness");
    requireSha256(freshness.probe_id, "Pages probe ID");
    requireSha256(freshness.identity_readback_sha256, "Pages identity readback SHA-256");
  }
}

function validateOraclePowerShellArguments(arguments_, sourceCommit) {
  const expectedOptions = [
    "-Operation",
    "-ScriptReleaseId",
    "-ScriptReleaseSha256",
    "-SourceCommit",
    "-CandidateUrl",
    "-CandidateRevision",
    "-OracleReleaseId",
    "-OracleReleaseSha256",
    "-OracleSettingsSha256",
    "-DeploymentNonce",
    "-VerifiedAfter",
  ];
  if (arguments_.length !== expectedOptions.length * 2) {
    throw new Error("Oracle probe arguments differ from the closed read-only contract");
  }
  const values = new Map();
  for (let index = 0; index < arguments_.length; index += 2) {
    const option = arguments_[index];
    const value = arguments_[index + 1];
    if (option !== expectedOptions[index / 2] || values.has(option)) {
      throw new Error("Oracle probe arguments differ from the closed read-only contract");
    }
    values.set(option, value);
  }
  if (values.get("-Operation") !== "observe-candidate") {
    throw new Error("Oracle probe operation must be read-only observe-candidate");
  }
  if (values.get("-SourceCommit") !== sourceCommit) {
    throw new Error("Oracle probe source differs from the probe spec source");
  }
  requireNonEmptyString(values.get("-ScriptReleaseId"), "Oracle script release ID");
  requireSha256(values.get("-ScriptReleaseSha256"), "Oracle script release SHA-256");
  requireCredentialFreeHttpsUrl(values.get("-CandidateUrl"), "Oracle candidate URL");
  requireNonEmptyString(values.get("-CandidateRevision"), "Oracle candidate revision");
  requireNonEmptyString(values.get("-OracleReleaseId"), "Oracle release ID");
  requireSha256(values.get("-OracleReleaseSha256"), "Oracle release SHA-256");
  requireSha256(values.get("-OracleSettingsSha256"), "Oracle settings SHA-256");
  if (!ORACLE_NONCE.test(values.get("-DeploymentNonce") ?? "")) {
    throw new Error("Oracle deployment nonce is invalid");
  }
  canonicalTimestamp(values.get("-VerifiedAfter"), "Oracle verified-after time");
  if (
    values.get("-ScriptReleaseId") !== values.get("-OracleReleaseId") ||
    values.get("-ScriptReleaseSha256") !== values.get("-OracleReleaseSha256")
  ) {
    throw new Error("Oracle script release authority differs from the active release authority");
  }
  return values;
}

function normalizeOracleProbeResult(value, sourceCommit, arguments_) {
  requireExactKeys(value, [
    "contract",
    "sourceCommit",
    "candidateUrl",
    "candidateRevision",
    "jobUrl",
    "oracleReleaseId",
    "activeReleasePath",
    "oracleReleaseSha256",
    "oracleSettingsSha256",
    "deploymentNonce",
    "gatewayPid",
    "gatewayStartMonotonicUsec",
    "bootId",
    "readyRecordObserved",
    "freshOperationAt",
    "observedAt",
    "runtimeIdentity",
  ], "Oracle read-only observation adapter result");
  if (value.contract !== ORACLE_PROBE_SCHEMA_ID || value.sourceCommit !== sourceCommit) {
    throw new Error("Oracle observation adapter contract or source is invalid");
  }
  const expected = validateOraclePowerShellArguments(arguments_, sourceCommit);
  const candidateUrl = requireCredentialFreeHttpsUrl(
    value.candidateUrl,
    "Oracle observed candidate URL",
  );
  const candidate = new URL(candidateUrl);
  if (candidate.pathname !== "/") {
    throw new Error("Oracle candidate URL must be an HTTPS origin");
  }
  const jobUrl = requireCredentialFreeHttpsUrl(value.jobUrl, "Oracle observed job URL");
  if (jobUrl !== new URL("/jobs", candidate).toString()) {
    throw new Error("Oracle observed job URL differs from its candidate origin");
  }
  if (
    value.candidateRevision !== expected.get("-CandidateRevision") ||
    value.oracleReleaseId !== expected.get("-OracleReleaseId") ||
    value.oracleReleaseSha256 !== expected.get("-OracleReleaseSha256") ||
    value.oracleSettingsSha256 !== expected.get("-OracleSettingsSha256") ||
    value.deploymentNonce !== expected.get("-DeploymentNonce") ||
    candidateUrl !== requireCredentialFreeHttpsUrl(
      expected.get("-CandidateUrl"),
      "Oracle expected candidate URL",
    )
  ) {
    throw new Error("Oracle observation output differs from its hash-bound invocation authority");
  }
  if (value.activeReleasePath !== `/opt/clearra/releases/${value.oracleReleaseId}`) {
    throw new Error("Oracle active release path differs from the exact release ID");
  }
  requireSha256(value.oracleReleaseSha256, "Oracle observed release SHA-256");
  requireSha256(value.oracleSettingsSha256, "Oracle observed settings SHA-256");
  if (!ORACLE_NONCE.test(value.deploymentNonce ?? "")) {
    throw new Error("Oracle observed deployment nonce is invalid");
  }
  if (
    !Number.isSafeInteger(value.gatewayPid) ||
    value.gatewayPid < 1 ||
    !Number.isSafeInteger(value.gatewayStartMonotonicUsec) ||
    value.gatewayStartMonotonicUsec < 1 ||
    !ORACLE_BOOT_ID.test(value.bootId ?? "") ||
    value.readyRecordObserved !== true
  ) {
    throw new Error("Oracle observed Gateway PID/start/READY authority is invalid");
  }
  const freshOperationAt = canonicalTimestamp(
    value.freshOperationAt,
    "Oracle fresh operation time",
  );
  const observedAt = canonicalTimestamp(value.observedAt, "Oracle observation time");
  if (
    Date.parse(freshOperationAt) < Date.parse(expected.get("-VerifiedAfter")) ||
    Date.parse(freshOperationAt) > Date.parse(observedAt)
  ) {
    throw new Error("Oracle observation did not include a fresh successful operation");
  }
  requireExactKeys(value.runtimeIdentity, [
    "schema",
    "sourceCommit",
    "engineBuildId",
    "contractSchemaVersion",
    "supplySemanticsId",
    "artifactSchemaVersion",
  ], "Oracle runtime identity");
  if (
    value.runtimeIdentity.schema !== "clearra.runtime.identity.v2" ||
    value.runtimeIdentity.sourceCommit !== sourceCommit ||
    value.runtimeIdentity.engineBuildId !== sourceCommit ||
    value.runtimeIdentity.contractSchemaVersion !== CONTRACT_SCHEMA_VERSION ||
    value.runtimeIdentity.supplySemanticsId !== SUPPLY_SEMANTICS_ID ||
    value.runtimeIdentity.artifactSchemaVersion !== ARTIFACT_SCHEMA_VERSION
  ) {
    throw new Error("Oracle runtime identity differs from the release source");
  }
  rejectSecretMaterial(value, "Oracle observation adapter result");
  const operationMarker = canonicalSha256({
    contract: ORACLE_PROBE_SCHEMA_ID,
    source_commit: sourceCommit,
    candidate_revision: value.candidateRevision,
    fresh_operation_at: freshOperationAt,
    observed_at: observedAt,
  });
  return {
    schema_id: PRODUCTION_SURFACE_PROBE_SCHEMA_ID,
    surface: "oracle",
    source_commit: sourceCommit,
    identity: {
      source_commit: sourceCommit,
      release_id: value.oracleReleaseId,
      release_tree_sha256: value.oracleReleaseSha256,
      settings_sha256: value.oracleSettingsSha256,
      candidate_revision: value.candidateRevision,
      candidate_url: candidateUrl,
      job_url: jobUrl,
      deployment_nonce: value.deploymentNonce,
      gateway_pid: value.gatewayPid,
      gateway_start_monotonic_usec: value.gatewayStartMonotonicUsec,
      boot_id: value.bootId,
      ready_record_observed: true,
      status: "active",
    },
    freshness: {
      operation_marker: operationMarker,
      fresh_operation_at: freshOperationAt,
      observed_at: observedAt,
    },
  };
}

function validateProbeAdapterAuthority(value) {
  if (!Array.isArray(value) || value.length !== REQUIRED_SURFACES.length) {
    throw new Error("production probe adapter authority must contain four entries");
  }
  const surfaces = [];
  for (const adapter of value) {
    requireExactKeys(adapter, ["surface", "sha256"], "production probe adapter authority");
    if (!REQUIRED_SURFACES.includes(adapter.surface)) {
      throw new Error("production probe adapter authority has an unexpected surface");
    }
    surfaces.push(adapter.surface);
    requireSha256(adapter.sha256, `${adapter.surface} probe adapter authority SHA-256`);
  }
  assertExactIdentitySet(surfaces, REQUIRED_SURFACES, "probe adapter authority surfaces");
}

async function verifyProbeAdapterFile(adapter, path) {
  await assertSafeDirectoryChain(dirname(path));
  const metadata = await lstat(path);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error(`${adapter.surface} probe adapter must be a regular non-link file`);
  }
  const bytes = await readFile(path);
  const digest = createHash("sha256").update(bytes).digest("hex");
  if (digest !== adapter.sha256) {
    throw new Error(`${adapter.surface} probe adapter SHA-256 changed`);
  }
}

function validateProbeFunctions(probes) {
  const entries = probes instanceof Map
    ? [...probes.entries()]
    : Object.entries(probes ?? {});
  const map = new Map(entries);
  assertExactIdentitySet([...map.keys()], REQUIRED_SURFACES, "production probe functions");
  for (const [surface, probe] of map) {
    if (typeof probe !== "function") {
      throw new Error(`${surface} production probe must be a function`);
    }
  }
  return map;
}

async function runProbeCommand(executable, arguments_, timeoutMilliseconds) {
  return new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(executable, arguments_, {
      shell: false,
      windowsHide: true,
      stdio: ["ignore", "pipe", "ignore"],
    });
    const chunks = [];
    let size = 0;
    let settled = false;
    const timer = setTimeout(() => {
      child.kill();
      finish(new Error("production surface probe timed out"));
    }, timeoutMilliseconds);
    child.stdout.on("data", (chunk) => {
      size += chunk.length;
      if (size > MAX_PROBE_OUTPUT_BYTES) {
        child.kill();
        finish(new Error("production surface probe output exceeded its bound"));
        return;
      }
      chunks.push(chunk);
    });
    child.on("error", () => finish(new Error("production surface probe failed to start")));
    child.on("exit", (code, signal) => {
      if (code !== 0 || signal) {
        finish(new Error("production surface probe did not exit successfully"));
        return;
      }
      const output = Buffer.concat(chunks).toString("utf8");
      let value;
      try {
        value = JSON.parse(output);
      } catch {
        finish(new Error("production surface probe did not return one JSON object"));
        return;
      }
      if (output !== `${canonicalJson(value)}\n`) {
        finish(new Error("production surface probe output is not canonical JSON"));
        return;
      }
      finish(null, value);
    });

    function finish(error, value) {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (error) rejectPromise(error);
      else resolvePromise(value);
    }
  });
}

async function readCanonicalJson(path, label) {
  const target = resolve(requireNonEmptyString(path, `${label} path`));
  await assertSafeDirectoryChain(dirname(target));
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error(`${label} must be a regular non-link file`);
  }
  const raw = await readFile(target, "utf8");
  let value;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new Error(`${label} is not valid JSON`);
  }
  if (raw !== `${canonicalJson(value)}\n`) {
    throw new Error(`${label} bytes are not canonical JSON`);
  }
  return value;
}

async function writeCanonicalJsonNew(path, value) {
  const target = resolve(requireNonEmptyString(path, "output path"));
  await assertSafeDirectoryChain(dirname(target));
  const handle = await open(target, "wx", 0o600);
  try {
    await handle.writeFile(`${canonicalJson(value)}\n`, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }
}

async function assertSafeDirectoryChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const metadata = await lstat(current);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error(`release evidence path uses a non-directory or link: ${current}`);
    }
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
}

function requireCredentialFreeHttpsUrl(value, label) {
  let url;
  try {
    url = new URL(String(value ?? ""));
  } catch {
    throw new Error(`${label} is invalid`);
  }
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash
  ) {
    throw new Error(`${label} must be credential-free HTTPS without query or fragment`);
  }
  return url.toString();
}

function requireApplicationId(value) {
  if (typeof value !== "string" || !DISCORD_SNOWFLAKE.test(value)) {
    throw new Error("Discord application ID must be a 17-20 digit snowflake");
  }
  return value;
}

function requirePositiveDuration(value, label) {
  if (!Number.isSafeInteger(value) || value < 1) {
    throw new Error(`${label} must be a positive safe integer`);
  }
}

function exactClockMilliseconds(value) {
  const milliseconds = value instanceof Date ? value.getTime() : value;
  if (!Number.isSafeInteger(milliseconds) || milliseconds < 0) {
    throw new Error("production observation clock returned an invalid time");
  }
  return milliseconds;
}

function assertExactIdentitySet(actual, expected, label) {
  const sorted = [...actual].sort();
  const wanted = [...expected].sort();
  if (
    new Set(sorted).size !== sorted.length ||
    sorted.length !== wanted.length ||
    sorted.some((entry, index) => entry !== wanted[index])
  ) {
    throw new Error(`${label} differ from the required identity set`);
  }
}

const systemClock = Object.freeze({
  now: () => Date.now(),
  wait: (milliseconds) => new Promise((resolvePromise) =>
    setTimeout(resolvePromise, milliseconds)),
});

function parseCliArguments(args) {
  const values = {};
  const allowed = new Set(["--source-commit", "--probe-spec", "--output"]);
  for (let index = 0; index < args.length; index += 2) {
    const option = args[index];
    const value = args[index + 1];
    if (!allowed.has(option)) {
      throw new Error(`unsupported production observation argument: ${String(option)}`);
    }
    if (Object.hasOwn(values, option)) {
      throw new Error(`duplicate production observation argument: ${option}`);
    }
    if (typeof value !== "string" || value.length === 0 || value.startsWith("--")) {
      throw new Error(`${option} requires one value`);
    }
    values[option] = value;
  }
  for (const required of allowed) {
    if (!Object.hasOwn(values, required)) throw new Error(`${required} is required`);
  }
  return values;
}

async function main() {
  const values = parseCliArguments(process.argv.slice(2));
  const sourceCommit = values["--source-commit"];
  const spec = await readCanonicalJson(
    values["--probe-spec"],
    "production observation probe spec",
  );
  validateProductionProbeSpec(spec, sourceCommit);
  const probes = await createCommandProbes(spec);
  const report = await observeProductionSurfaces({
    sourceCommit,
    probes,
    probeSpec: spec,
    durationSeconds: PRODUCTION_OBSERVATION_SECONDS,
    intervalSeconds: spec.interval_seconds,
  });
  await writeCanonicalJsonNew(values["--output"], report);
  process.stdout.write(`${PRODUCTION_OBSERVATION_SCHEMA_ID} ${report.report_sha256}\n`);
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    await main();
  } catch (error) {
    process.stderr.write(
      `production_observation=failed reason=${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}
