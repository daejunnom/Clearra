import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import {
  produceOracleCandidateProof,
  produceOracleRollbackProof,
} from "../scripts/produce-oracle-deployment-proof.mjs";
import {
  observePriorRuntimeAuthority,
  PRIOR_RUNTIME_IDENTITY_KIND,
  PRIOR_RUNTIME_LEGACY_HEALTH_KIND,
} from "../scripts/oracle-runtime-authority.mjs";

const sourceCommit = "7".repeat(40);
const deploymentNonce = "9".repeat(64);
const verifiedAfter = "2026-08-20T00:00:00.000Z";
const candidateJobUrl = "https://candidate.example.run.app/jobs";
const candidateSettings = [
  `CLEARRA_JOB_URL=${candidateJobUrl}`,
  `CLEARRA_EXPECTED_JOB_SOURCE_COMMIT=${sourceCommit}`,
  `CLEARRA_EXPECTED_ENGINE_BUILD_ID=${sourceCommit}`,
  "CLEARRA_EXPECTED_JOB_CONTRACT_REVISION=clearra.search.contract.v2",
  "CLEARRA_EXPECTED_SUPPLY_SEMANTICS_ID=clearra.supply.projected-terminal-lookahead.v1",
  "CLEARRA_EXPECTED_ARTIFACT_SCHEMA_VERSION=clearra.solution-data.v1",
  "",
].join("\n");
const candidateSettingsSha256 = createHash("sha256")
  .update(candidateSettings, "utf8")
  .digest("hex");
const priorRevision = "clearra-current-job-v075-0438d85";
const priorReleaseId = "v0.7.4-0438d85";
const legacyHealth = Object.freeze({
  status: "ok",
  activeJobs: 0,
  workerLimit: 8,
});
const runtimeIdentity = Object.freeze({
  schema: "clearra.runtime.identity.v2",
  sourceCommit: "4".repeat(40),
  engineBuildId: "4".repeat(40),
  contractSchemaVersion: "clearra.search.contract.v2",
  supplySemanticsId: "clearra.supply.projected-terminal-lookahead.v1",
  artifactSchemaVersion: "clearra.solution-data.v1",
});

test("trusted Oracle producer observes active candidate and fresh bounded operation", () => {
  let written;
  const proof = produceOracleCandidateProof(
    {
      proofPath: "/tmp/clearra-oracle-candidate-test.json",
      sourceCommit,
      candidateUrl: "https://candidate.example.run.app",
      candidateRevision: "clearra-current-job-v075-701454b",
      oracleReleaseId: "v0.7.5-701454b-private-v6",
      oracleReleaseSha256: "a".repeat(64),
      oracleSettingsSha256: candidateSettingsSha256,
      deploymentNonce,
      verifiedAfter,
    },
    fakeRuntime({
      releaseId: "v0.7.5-701454b-private-v6",
      releaseSha256: "a".repeat(64),
      settings: candidateSettings,
      operationAt: "2026-08-20T00:00:01.000Z",
      writeProof(_path, value) {
        written = value;
      },
    }),
  );
  assert.equal(proof.gatewayReady, true);
  assert.equal(proof.boundedJobSucceeded, true);
  assert.equal(proof.jobUrl, candidateJobUrl);
  assert.equal(written, proof);
});

test("trusted Oracle producer rejects stale settings, process, and operation evidence", () => {
  const options = {
    proofPath: "/tmp/clearra-oracle-candidate-test.json",
    sourceCommit,
    candidateUrl: "https://candidate.example.run.app",
    candidateRevision: "clearra-current-job-v075-701454b",
    oracleReleaseId: "v0.7.5-701454b-private-v6",
    oracleReleaseSha256: "a".repeat(64),
    oracleSettingsSha256: candidateSettingsSha256,
    deploymentNonce,
    verifiedAfter,
  };
  assert.throws(
    () =>
      produceOracleCandidateProof(
        options,
        fakeRuntime({
          releaseId: "v0.7.5-stale",
          releaseSha256: "a".repeat(64),
          settings: candidateSettings,
        }),
      ),
    /active Oracle symlink/u,
  );
  assert.throws(
    () =>
      produceOracleCandidateProof(
        options,
        fakeRuntime({
          releaseId: options.oracleReleaseId,
          releaseSha256: "a".repeat(64),
          settings: candidateSettings.replace(sourceCommit, "6".repeat(40)),
        }),
      ),
    /settings digest/u,
  );
  assert.throws(
    () =>
      produceOracleCandidateProof(
        options,
        fakeRuntime({
          releaseId: options.oracleReleaseId,
          releaseSha256: "a".repeat(64),
          settings: candidateSettings,
          operationAt: "2026-08-19T23:59:59.000Z",
        }),
      ),
    /fresh successful bounded/u,
  );
});

test("trusted Oracle producer binds rollback to restored prior deployment", () => {
  const priorSettings = "CLEARRA_JOB_URL=https://stable.example.run.app/jobs\n";
  const priorSettingsSha256 = createHash("sha256")
    .update(priorSettings, "utf8")
    .digest("hex");
  const priorRuntimeAuthority = observePriorRuntimeAuthority({
    kind: PRIOR_RUNTIME_LEGACY_HEALTH_KIND,
    priorRevision,
    priorOracleReleaseId: priorReleaseId,
    health: legacyHealth,
  });
  const options = {
    proofPath: "/tmp/clearra-oracle-rollback-test.json",
    priorRevision,
    priorOracleReleaseId: priorReleaseId,
    priorOracleReleaseSha256: "b".repeat(64),
    priorOracleSettingsSha256: priorSettingsSha256,
    priorRuntimeAuthorityKind: priorRuntimeAuthority.kind,
    priorRuntimeAuthoritySha256: priorRuntimeAuthority.sha256,
    priorJobUrl: "https://stable.example.run.app/jobs",
    deploymentNonce,
    verifiedAfter,
  };
  const proof = produceOracleRollbackProof(
    options,
    fakeRuntime({
      releaseId: priorReleaseId,
      releaseSha256: "b".repeat(64),
      settings: priorSettings,
      operationAt: "2026-08-20T00:00:02.000Z",
      health: { ...legacyHealth, activeJobs: 1 },
    }),
  );
  assert.equal(proof.priorRevision, priorRevision);
  assert.equal(
    proof.priorRuntimeAuthorityKind,
    PRIOR_RUNTIME_LEGACY_HEALTH_KIND,
  );
  assert.equal(proof.priorRuntimeAuthoritySha256, priorRuntimeAuthority.sha256);
  assert.equal(proof.gatewayReady, true);
  assert.equal(proof.boundedJobSucceeded, true);

  for (const [label, health, error] of [
    ["worker drift", { ...legacyHealth, workerLimit: 7 }, /runtime authority/u],
    [
      "identity appeared",
      { ...legacyHealth, runtime: runtimeIdentity },
      /must not contain/u,
    ],
    ["profile drift", { ...legacyHealth, extra: true }, /fields do not match/u],
  ]) {
    assert.throws(
      () =>
        produceOracleRollbackProof(
          options,
          fakeRuntime({
            releaseId: priorReleaseId,
            releaseSha256: "b".repeat(64),
            settings: priorSettings,
            health,
          }),
        ),
      error,
      label,
    );
  }
});

test("trusted Oracle rollback producer preserves strict v2 runtime authority", () => {
  const priorSettings = "CLEARRA_JOB_URL=https://stable.example.run.app/jobs\n";
  const priorSettingsSha256 = createHash("sha256")
    .update(priorSettings, "utf8")
    .digest("hex");
  const capturedHealth = { ...legacyHealth, runtime: runtimeIdentity };
  const authority = observePriorRuntimeAuthority({
    kind: PRIOR_RUNTIME_IDENTITY_KIND,
    priorRevision,
    priorOracleReleaseId: priorReleaseId,
    health: capturedHealth,
  });
  const options = {
    proofPath: "/tmp/clearra-oracle-rollback-test.json",
    priorRevision,
    priorOracleReleaseId: priorReleaseId,
    priorOracleReleaseSha256: "b".repeat(64),
    priorOracleSettingsSha256: priorSettingsSha256,
    priorRuntimeAuthorityKind: authority.kind,
    priorRuntimeAuthoritySha256: authority.sha256,
    priorJobUrl: "https://stable.example.run.app/jobs",
    deploymentNonce,
    verifiedAfter,
  };
  const dependencies = {
    releaseId: priorReleaseId,
    releaseSha256: "b".repeat(64),
    settings: priorSettings,
  };

  assert.doesNotThrow(() =>
    produceOracleRollbackProof(
      options,
      fakeRuntime({
        ...dependencies,
        health: { ...capturedHealth, activeJobs: 1 },
      }),
    ),
  );
  assert.throws(
    () =>
      produceOracleRollbackProof(
        options,
        fakeRuntime({
          ...dependencies,
          health: {
            ...capturedHealth,
            runtime: { ...runtimeIdentity, extra: true },
          },
        }),
      ),
    /identity fields do not match/u,
  );
  assert.throws(
    () =>
      produceOracleRollbackProof(
        options,
        fakeRuntime({
          ...dependencies,
          health: legacyHealth,
        }),
      ),
    /identity is unavailable/u,
  );
});

function fakeRuntime({
  releaseId,
  releaseSha256 = "a".repeat(64),
  settings,
  operationAt = "2026-08-20T00:00:01.000Z",
  health = { ...legacyHealth, runtime: runtimeIdentity },
  writeProof = () => {},
}) {
  const releasePath = `/opt/clearra/releases/${releaseId}`;
  return {
    readText() {
      return settings;
    },
    releaseTreeSha256() {
      return releaseSha256;
    },
    realpath(path) {
      if (path === "/opt/clearra/current") return releasePath;
      if (path === "/proc/4242/cwd")
        return `${releasePath}/apps/clearra-discord-bot`;
      throw new Error(`unexpected realpath ${path}`);
    },
    run(command, arguments_) {
      if (command === "/usr/bin/systemctl" && arguments_[0] === "is-active")
        return "active\n";
      if (command === "/usr/bin/systemctl" && arguments_[0] === "show")
        return "4242\n";
      if (command === "/usr/bin/journalctl") {
        return [
          "Oracle Gateway connected as ClearraBot; Gateway slash ingress enabled.",
          JSON.stringify({
            event: "clearra.operation",
            at: operationAt,
            scope: "gateway",
            kind: "slash",
            command: "path",
            status: "succeeded",
            durationMs: 42,
          }),
          "",
        ].join("\n");
      }
      if (command === "/usr/bin/curl") {
        return JSON.stringify(health);
      }
      throw new Error(`unexpected command ${command}`);
    },
    writeProof,
  };
}
