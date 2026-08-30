import assert from "node:assert/strict";
import test from "node:test";

import { canonicalSha256 } from "./canonical-release-evidence.mjs";
import {
  observeProductionSurfaces,
  PRODUCTION_OBSERVATION_SCHEMA_ID,
  PRODUCTION_SURFACE_PROBE_SCHEMA_ID,
  validateProductionObservationReport,
  validateProductionProbeSpec,
} from "./observe-production-surfaces.mjs";

const COMMIT = "1".repeat(40);
const HASH = "a".repeat(64);

test("observes Discord, Oracle, Cloud, and Pages through a short injected clock", async () => {
  const clock = fakeClock("2026-08-30T00:00:00.000Z");
  const calls = new Map();
  const report = await observeProductionSurfaces({
    sourceCommit: COMMIT,
    durationSeconds: 2,
    intervalSeconds: 1,
    clock,
    probes: probeSet(calls),
    probeSpec: validProbeSpec(),
  });

  assert.equal(report.schema_id, PRODUCTION_OBSERVATION_SCHEMA_ID);
  assert.equal(report.duration_seconds, 2);
  assert.deepEqual(report.surfaces.map(({ surface }) => surface), [
    "cloud",
    "discord",
    "oracle",
    "pages",
  ]);
  assert.deepEqual([...calls.entries()], [
    ["cloud", 3],
    ["discord", 3],
    ["oracle", 3],
    ["pages", 3],
  ]);
  for (const surface of report.surfaces) {
    assert.equal(surface.observation_count, 3);
    assert.deepEqual(surface.observations.map(({ sequence }) => sequence), [0, 1, 2]);
  }
  assert.equal(
    validateProductionObservationReport(report, {
      expectedSourceCommit: COMMIT,
      minimumDurationSeconds: 2,
    }),
    report,
  );
  assert.doesNotMatch(JSON.stringify(report), /token|secret|password/iu);
});

test("fails closed when a surface identity changes during the window", async () => {
  const calls = new Map();
  const probes = probeSet(calls);
  const original = probes.pages;
  probes.pages = async (context) => {
    const result = await original(context);
    if (context.sequence === 1) {
      result.identity = { ...result.identity, deployment_id: "pages-drift" };
    }
    return result;
  };
  await assert.rejects(
    observeProductionSurfaces({
      sourceCommit: COMMIT,
      durationSeconds: 2,
      intervalSeconds: 1,
      clock: fakeClock("2026-08-30T00:00:00.000Z"),
      probes,
      probeSpec: validProbeSpec(),
    }),
    /pages production identity changed/u,
  );
});

test("rejects stale Oracle operation evidence and report hash tampering", async () => {
  const probes = probeSet(new Map());
  const original = probes.oracle;
  probes.oracle = async (context) => {
    const result = await original(context);
    const freshOperationAt = "2026-08-30T00:00:00.000Z";
    const observedAt = new Date(
      Date.parse("2026-08-30T00:00:00.000Z") + context.sequence * 1000,
    ).toISOString();
    result.freshness = {
      operation_marker: oracleOperationMarker(
        result.identity,
        freshOperationAt,
        observedAt,
      ),
      fresh_operation_at: freshOperationAt,
      observed_at: observedAt,
    };
    return result;
  };
  await assert.rejects(
    observeProductionSurfaces({
      sourceCommit: COMMIT,
      durationSeconds: 2,
      intervalSeconds: 1,
      clock: fakeClock("2026-08-30T00:00:00.000Z"),
      probes,
      probeSpec: validProbeSpec(),
    }),
    /operation time did not increase/u,
  );

  const report = await observeProductionSurfaces({
    sourceCommit: COMMIT,
    durationSeconds: 1,
    intervalSeconds: 1,
    clock: fakeClock("2026-08-30T00:00:00.000Z"),
    probes: probeSet(new Map()),
    probeSpec: validProbeSpec(),
  });
  const tampered = { ...report, duration_seconds: 1200 };
  assert.throws(
    () => validateProductionObservationReport(tampered, {
      minimumDurationSeconds: 1,
    }),
    /SHA-256 differs/u,
  );
});

test("rejects an Oracle probe whose fresh operation predates the claimed window", async () => {
  const probes = probeSet(new Map());
  const original = probes.oracle;
  probes.oracle = async (context) => {
    const result = await original(context);
    const freshOperationAt = "2026-08-29T23:59:59.999Z";
    const observedAt = "2026-08-30T00:00:00.000Z";
    result.freshness = {
      operation_marker: oracleOperationMarker(
        result.identity,
        freshOperationAt,
        observedAt,
      ),
      fresh_operation_at: freshOperationAt,
      observed_at: observedAt,
    };
    return result;
  };
  await assert.rejects(
    observeProductionSurfaces({
      sourceCommit: COMMIT,
      durationSeconds: 1,
      intervalSeconds: 1,
      clock: fakeClock("2026-08-30T00:00:00.000Z"),
      probes,
      probeSpec: validProbeSpec(),
    }),
    /outside its observation sample/u,
  );
});

test("probe spec requires four hash-bound adapters and forbids secret fields", () => {
  const spec = validProbeSpec();
  assert.equal(validateProductionProbeSpec(spec, COMMIT), spec);

  const missing = structuredClone(spec);
  missing.probes.pop();
  assert.throws(() => validateProductionProbeSpec(missing), /exactly four/u);

  const secret = structuredClone(spec);
  secret.probes[0].api_token = "forbidden";
  assert.throws(() => validateProductionProbeSpec(secret), /closed schema/u);

  const duplicate = structuredClone(spec);
  duplicate.probes[3].surface = "cloud";
  assert.throws(() => validateProductionProbeSpec(duplicate), /identity set/u);
});

function validProbeSpec() {
  const probes = ["cloud", "discord", "oracle", "pages"].map((surface) => ({
    surface,
    runtime: surface === "oracle" ? "powershell" : "node",
    path: process.platform === "win32" ? `C:\\probes\\${surface}.mjs` : `/probes/${surface}.mjs`,
    sha256: HASH,
    arguments: ["--source-commit", COMMIT],
    timeout_seconds: 15,
  }));
  probes[2].arguments = [
    "-Operation", "observe-candidate",
    "-ScriptReleaseId", "v0.8.0-1111111",
    "-ScriptReleaseSha256", "d".repeat(64),
    "-SourceCommit", COMMIT,
    "-CandidateUrl", "https://v080---clearra-current-job.example.run.app/",
    "-CandidateRevision", "clearra-current-job-v080-1111111",
    "-OracleReleaseId", "v0.8.0-1111111",
    "-OracleReleaseSha256", "d".repeat(64),
    "-OracleSettingsSha256", "e".repeat(64),
    "-DeploymentNonce", "9".repeat(64),
    "-VerifiedAfter", "2026-08-29T23:59:59.000Z",
  ];
  return {
    schema_id: "clearra.production-observation-probe-spec.v1",
    source_commit: COMMIT,
    interval_seconds: 30,
    probes,
  };
}

function probeSet(calls) {
  const identities = {
    discord: {
      source_commit: COMMIT,
      application_id: "223456789012345678",
      command_catalog_sha256: HASH,
      command_catalog_prior_snapshot_sha256: "a".repeat(64),
      command_catalog_readback_sha256: "b".repeat(64),
      command_catalog_sync_report_sha256: "c".repeat(64),
      command_count: 2,
      command_names: ["1:help", "3:Get original GIF"],
      status: "active",
    },
    oracle: {
      source_commit: COMMIT,
      release_id: "v0.8.0-1111111",
      release_tree_sha256: "d".repeat(64),
      settings_sha256: "e".repeat(64),
      candidate_revision: "clearra-current-job-v080-1111111",
      candidate_url: "https://v080---clearra-current-job.example.run.app/",
      job_url: "https://v080---clearra-current-job.example.run.app/jobs",
      deployment_nonce: "9".repeat(64),
      gateway_pid: 1234,
      gateway_start_monotonic_usec: 123456789,
      boot_id: "12345678-1234-1234-1234-123456789abc",
      ready_record_observed: true,
      status: "active",
    },
    cloud: {
      source_commit: COMMIT,
      engine_build_id: COMMIT,
      revision: "clearra-current-job-v080-1111111",
      image_digest: `sha256:${"f".repeat(64)}`,
      traffic_percent: 100,
      cpu: "8",
      memory: "16Gi",
      concurrency: 1,
      min_instances: 0,
      max_instances: 4,
      startup_cpu_boost: true,
      contract_schema_version: "clearra.search.contract.v2",
      supply_semantics_id: "clearra.supply.projected-terminal-lookahead.v1",
      artifact_schema_version: "clearra.solution-data.v1",
      job_smoke_report_sha256: "7".repeat(64),
      stable_url: "https://clearra-current-job.example.run.app/",
      tagged_url: "https://v080---clearra-current-job.example.run.app/",
      status: "active",
    },
    pages: {
      source_commit: COMMIT,
      engine_build_id: COMMIT,
      version: "0.8.0",
      deployment_id: "pages-123",
      artifact_sha256: "9".repeat(64),
      base_path: "/Clearra",
      url: "https://daejunnom.github.io/Clearra/",
      status: "active",
    },
  };
  return Object.fromEntries(Object.entries(identities).map(([surface, identity]) => [
    surface,
    async ({ sequence }) => {
      calls.set(surface, (calls.get(surface) ?? 0) + 1);
      return {
        schema_id: PRODUCTION_SURFACE_PROBE_SCHEMA_ID,
        surface,
        source_commit: COMMIT,
        identity: structuredClone(identity),
        freshness: surface === "oracle"
          ? oracleFreshness(identity, sequence)
          : freshnessFor(surface, sequence),
      };
    },
  ]));
}

function oracleFreshness(identity, sequence) {
  const freshOperationAt = new Date(
    Date.parse("2026-08-30T00:00:00.000Z") + sequence * 1000,
  ).toISOString();
  const observedAt = freshOperationAt;
  return {
    operation_marker: oracleOperationMarker(identity, freshOperationAt, observedAt),
    fresh_operation_at: freshOperationAt,
    observed_at: observedAt,
  };
}

function oracleOperationMarker(identity, freshOperationAt, observedAt) {
  return canonicalSha256({
    contract: "clearra.oracle.candidate-observation.v1",
    source_commit: identity.source_commit,
    candidate_revision: identity.candidate_revision,
    fresh_operation_at: freshOperationAt,
    observed_at: observedAt,
  });
}

function freshnessFor(surface, sequence) {
  const probeId = (sequence + 1).toString(16).padStart(64, "0");
  if (surface === "discord") {
    return { probe_id: probeId, readback_sha256: "b".repeat(64) };
  }
  if (surface === "cloud") {
    return {
      probe_id: probeId,
      service_readback_sha256: "1".repeat(64),
      revision_readback_sha256: "2".repeat(64),
      stable_health_sha256: "3".repeat(64),
      tagged_health_sha256: "4".repeat(64),
    };
  }
  return { probe_id: probeId, identity_readback_sha256: "5".repeat(64) };
}

function fakeClock(start) {
  let milliseconds = Date.parse(start);
  return {
    now: () => milliseconds,
    async wait(delay) { milliseconds += delay; },
  };
}
