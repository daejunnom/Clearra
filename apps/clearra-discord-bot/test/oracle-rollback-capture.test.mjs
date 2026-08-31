import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import { captureOracleRollbackAuthority } from "../scripts/capture-oracle-rollback-authority.mjs";
import {
  observePriorRuntimeAuthority,
  PRIOR_RUNTIME_IDENTITY_KIND,
  PRIOR_RUNTIME_LEGACY_HEALTH_KIND,
} from "../scripts/oracle-runtime-authority.mjs";

const nonce = "9".repeat(64);
const priorRevision = "clearra-current-job-v075-701454b";
const priorOracleReleaseId = "v0.7.4-701454b";
const settings = Buffer.from(
  "CLEARRA_JOB_URL=https://stable.example.run.app/jobs\nCLEARRA_DISCORD_SECRET_OCID=non-secret-reference\n",
  "utf8",
);
const legacyHealth = Object.freeze({
  status: "ok",
  activeJobs: 0,
  workerLimit: 8,
});
const runtime = Object.freeze({
  schema: "clearra.runtime.identity.v2",
  sourceCommit: "4".repeat(40),
  engineBuildId: "4".repeat(40),
  contractSchemaVersion: "clearra.search.contract.v2",
  supplySemanticsId: "clearra.supply.projected-terminal-lookahead.v1",
  artifactSchemaVersion: "clearra.solution-data.v1",
});
const runtimeHealth = Object.freeze({ ...legacyHealth, runtime });

test("rollback authority capture freezes exact v0.7.4 legacy authority without inventing identity", () => {
  let backup;
  const captured = captureOracleRollbackAuthority(
    {
      priorRevision,
      priorRuntimeAuthorityKind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND,
      deploymentNonce: nonce,
    },
    captureDependencies({
      health: legacyHealth,
      writeBackup(path, bytes) {
        backup = { path, bytes };
      },
    }),
  );
  const expectedAuthority = observePriorRuntimeAuthority({
    kind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND,
    priorRevision,
    priorOracleReleaseId,
    health: legacyHealth,
  });

  assert.equal(captured.priorOracleReleaseId, priorOracleReleaseId);
  assert.equal(captured.priorOracleReleaseSha256, "a".repeat(64));
  assert.equal(
    captured.priorOracleSettingsSha256,
    createHash("sha256").update(settings).digest("hex"),
  );
  assert.equal(captured.priorRuntimeAuthorityKind, expectedAuthority.kind);
  assert.equal(captured.priorRuntimeAuthoritySha256, expectedAuthority.sha256);
  assert.equal(captured.priorJobUrl, "https://stable.example.run.app/jobs");
  assert.equal(
    backup.path,
    `/etc/clearra-gateway/settings.pre-v0.8.0-${nonce}`,
  );
  assert.equal(backup.bytes, settings);
});

test("rollback authority capture preserves the exact runtime-identity path", () => {
  const captured = captureOracleRollbackAuthority(
    {
      priorRevision,
      priorRuntimeAuthorityKind: PRIOR_RUNTIME_IDENTITY_KIND,
      deploymentNonce: nonce,
    },
    captureDependencies({ health: runtimeHealth }),
  );
  const expected = observePriorRuntimeAuthority({
    kind: PRIOR_RUNTIME_IDENTITY_KIND,
    priorRevision,
    priorOracleReleaseId,
    health: runtimeHealth,
  });
  assert.equal(captured.priorRuntimeAuthorityKind, expected.kind);
  assert.equal(captured.priorRuntimeAuthoritySha256, expected.sha256);
});

test("rollback capture cleans bounded stale authority before writing the nonce-bound backup", () => {
  const operations = [];
  captureOracleRollbackAuthority(
    {
      priorRevision,
      priorRuntimeAuthorityKind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND,
      deploymentNonce: nonce,
    },
    captureDependencies({
      health: legacyHealth,
      cleanupBackups(path) {
        operations.push(`cleanup:${path}`);
      },
      writeBackup(path) {
        operations.push(`write:${path}`);
      },
    }),
  );
  const expected = `/etc/clearra-gateway/settings.pre-v0.8.0-${nonce}`;
  assert.deepEqual(operations, [`cleanup:${expected}`, `write:${expected}`]);
});

test("legacy authority excludes dynamic active-job count but binds worker capacity", () => {
  const observe = (health) =>
    observePriorRuntimeAuthority({
      kind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND,
      priorRevision,
      priorOracleReleaseId,
      health,
    }).sha256;
  assert.equal(
    observe({ ...legacyHealth, activeJobs: 0 }),
    observe({ ...legacyHealth, activeJobs: 1 }),
  );
  assert.notEqual(
    observe({ ...legacyHealth, workerLimit: 8 }),
    observe({ ...legacyHealth, workerLimit: 7 }),
  );
  assert.doesNotThrow(() =>
    observe({
      ...legacyHealth,
      activeJobs: Number.MAX_SAFE_INTEGER,
      workerLimit: Number.MAX_SAFE_INTEGER,
    }),
  );
});

test("rollback capture rejects every implicit or malformed legacy downgrade before backup", () => {
  for (const scenario of [
    {
      label: "missing explicit kind",
      options: {},
      health: legacyHealth,
      error: /authority kind is invalid/u,
    },
    {
      label: "identity kind without identity",
      options: { priorRuntimeAuthorityKind: PRIOR_RUNTIME_IDENTITY_KIND },
      health: legacyHealth,
      error: /identity is unavailable/u,
    },
    {
      label: "legacy kind with identity",
      options: { priorRuntimeAuthorityKind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND },
      health: runtimeHealth,
      error: /must not contain runtime identity/u,
    },
    {
      label: "extra health field",
      options: { priorRuntimeAuthorityKind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND },
      health: { ...legacyHealth, version: "unknown" },
      error: /fields do not match/u,
    },
    {
      label: "missing health field",
      options: { priorRuntimeAuthorityKind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND },
      health: { status: "ok", activeJobs: 0 },
      error: /fields do not match/u,
    },
    {
      label: "unready health",
      options: { priorRuntimeAuthorityKind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND },
      health: { ...legacyHealth, status: "starting" },
      error: /status is not ready/u,
    },
    {
      label: "invalid active jobs",
      options: { priorRuntimeAuthorityKind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND },
      health: { ...legacyHealth, activeJobs: -1 },
      error: /active job count is invalid/u,
    },
    {
      label: "fractional active jobs",
      options: { priorRuntimeAuthorityKind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND },
      health: { ...legacyHealth, activeJobs: 0.5 },
      error: /active job count is invalid/u,
    },
    {
      label: "unsafe active jobs",
      options: { priorRuntimeAuthorityKind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND },
      health: { ...legacyHealth, activeJobs: Number.MAX_SAFE_INTEGER + 1 },
      error: /active job count is invalid/u,
    },
    {
      label: "invalid worker limit",
      options: { priorRuntimeAuthorityKind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND },
      health: { ...legacyHealth, workerLimit: 0 },
      error: /worker limit is invalid/u,
    },
    {
      label: "fractional worker limit",
      options: { priorRuntimeAuthorityKind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND },
      health: { ...legacyHealth, workerLimit: 1.5 },
      error: /worker limit is invalid/u,
    },
    {
      label: "unsafe worker limit",
      options: { priorRuntimeAuthorityKind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND },
      health: { ...legacyHealth, workerLimit: Number.MAX_SAFE_INTEGER + 1 },
      error: /worker limit is invalid/u,
    },
    {
      label: "future release downgrade",
      options: { priorRuntimeAuthorityKind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND },
      health: legacyHealth,
      releaseId: "v0.7.5-701454b",
      error: /restricted to the matching v0\.7\.4/u,
    },
    {
      label: "suffixed legacy release",
      options: { priorRuntimeAuthorityKind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND },
      health: legacyHealth,
      releaseId: "v0.7.4-701454b-extra",
      error: /restricted to the matching v0\.7\.4/u,
    },
    {
      label: "generic revision",
      options: {
        priorRevision: "clearra-current-job-v074-701454b",
        priorRuntimeAuthorityKind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND,
      },
      health: legacyHealth,
      error: /restricted to the matching v0\.7\.4/u,
    },
    {
      label: "mismatched Cloud revision",
      options: {
        priorRevision: "clearra-current-job-v075-deadbee",
        priorRuntimeAuthorityKind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND,
      },
      health: legacyHealth,
      error: /restricted to the matching v0\.7\.4/u,
    },
  ]) {
    let wrote = false;
    assert.throws(
      () =>
        captureOracleRollbackAuthority(
          {
            priorRevision,
            deploymentNonce: nonce,
            ...scenario.options,
          },
          captureDependencies({
            health: scenario.health,
            releaseId: scenario.releaseId,
            writeBackup() {
              wrote = true;
            },
          }),
        ),
      scenario.error,
      scenario.label,
    );
    assert.equal(wrote, false, scenario.label);
  }
});

test("runtime identity authority rejects health or identity key drift before backup", () => {
  for (const health of [
    { ...runtimeHealth, extra: true },
    { status: "ok", activeJobs: 0, runtime },
    { ...runtimeHealth, runtime: { ...runtime, extra: true } },
    {
      ...runtimeHealth,
      runtime: Object.fromEntries(
        Object.entries(runtime).filter(
          ([key]) => key !== "artifactSchemaVersion",
        ),
      ),
    },
    { ...runtimeHealth, status: "starting" },
    { ...runtimeHealth, activeJobs: 0.5 },
    { ...runtimeHealth, workerLimit: Number.POSITIVE_INFINITY },
  ]) {
    let wrote = false;
    assert.throws(() =>
      captureOracleRollbackAuthority(
        {
          priorRevision,
          priorRuntimeAuthorityKind: PRIOR_RUNTIME_IDENTITY_KIND,
          deploymentNonce: nonce,
        },
        captureDependencies({
          health,
          writeBackup() {
            wrote = true;
          },
        }),
      ),
    );
    assert.equal(wrote, false);
  }
});

test("rollback authority capture fails before backup on invalid active authority", () => {
  let wrote = false;
  assert.throws(
    () =>
      captureOracleRollbackAuthority(
        {
          priorRevision,
          priorRuntimeAuthorityKind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND,
          deploymentNonce: nonce,
        },
        {
          realpath: () => "/tmp/mutable-release",
          writeBackup() {
            wrote = true;
          },
        },
      ),
    /outside the immutable release root/u,
  );
  assert.equal(wrote, false);
});

function captureDependencies({
  health,
  releaseId = priorOracleReleaseId,
  writeBackup = () => {},
  cleanupBackups = () => {},
}) {
  return {
    realpath: () => `/opt/clearra/releases/${releaseId}`,
    releaseTreeSha256: () => "a".repeat(64),
    lstat: () => ({
      uid: 0,
      isFile: () => true,
      isSymbolicLink: () => false,
    }),
    readSettings: () => settings,
    run: () => JSON.stringify(health),
    cleanupBackups,
    writeBackup,
  };
}
