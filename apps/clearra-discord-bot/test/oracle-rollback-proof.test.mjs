import assert from "node:assert/strict";
import test from "node:test";

import {
  consumeOracleRollbackProof,
  verifyOracleRollbackProof,
} from "../scripts/verify-oracle-rollback-proof.mjs";

const expected = Object.freeze({
  priorRevision: "clearra-current-job-v074-0438d85",
  priorOracleReleaseId: "v0.7.4-0438d85-v6rollback",
  priorOracleReleaseSha256: "a".repeat(64),
  priorOracleSettingsSha256: "7".repeat(64),
  priorRuntimeIdentitySha256: "8".repeat(64),
  priorJobUrl: "https://clearra-current-job.example.run.app/jobs",
  deploymentNonce: "9".repeat(64),
});
const proof = Object.freeze({
  ...expected,
  gatewayReady: true,
  boundedJobSucceeded: true,
});

test("Oracle rollback proof binds prior Cloud, Oracle, settings, runtime, and job", () => {
  assert.deepEqual(verifyOracleRollbackProof(proof, expected), {
    priorRevision: expected.priorRevision,
    priorOracleReleaseId: expected.priorOracleReleaseId,
    priorJobUrl: expected.priorJobUrl,
  });
});

test("Oracle rollback proof rejects stale authority and missing live checks", () => {
  for (const [field, replacement] of [
    ["priorRevision", "clearra-current-job-v074-stale"],
    ["priorOracleReleaseId", "v0.7.4-stale"],
    ["priorOracleReleaseSha256", "3".repeat(64)],
    ["priorOracleSettingsSha256", "6".repeat(64)],
    ["priorRuntimeIdentitySha256", "5".repeat(64)],
    ["priorJobUrl", "https://stale.example.run.app/jobs"],
    ["deploymentNonce", "4".repeat(64)],
  ]) {
    assert.throws(
      () => verifyOracleRollbackProof(proof, { ...expected, [field]: replacement }),
      /does not match the captured prior deployment/u,
    );
  }
  assert.throws(
    () => verifyOracleRollbackProof({ ...proof, gatewayReady: false }, expected),
    /does not match the captured prior deployment/u,
  );
  assert.throws(
    () => verifyOracleRollbackProof({ ...proof, boundedJobSucceeded: false }, expected),
    /does not match the captured prior deployment/u,
  );
});

test("Oracle rollback proof file is root-only and consumed exactly once", () => {
  const path = `/run/clearra-deploy/clearra-oracle-rollback-${expected.deploymentNonce}.json`;
  let present = true;
  const dependencies = {
    resolvePath: (value) => value,
    lstat() {
      if (!present) throw new Error("missing");
      return {
        size: JSON.stringify(proof).length,
        uid: 0,
        mode: 0o100600,
        isFile: () => true,
        isSymbolicLink: () => false,
      };
    },
    readText: () => JSON.stringify(proof),
    remove() {
      present = false;
    },
  };
  consumeOracleRollbackProof(path, expected, dependencies);
  assert.equal(present, false);
  assert.throws(() => consumeOracleRollbackProof(path, expected, dependencies), /missing/u);
});
