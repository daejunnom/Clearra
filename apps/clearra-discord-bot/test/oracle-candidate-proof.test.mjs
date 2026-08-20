import assert from "node:assert/strict";
import test from "node:test";

import {
  consumeOracleCandidateProof,
  verifyOracleCandidateProof,
} from "../scripts/verify-oracle-candidate-proof.mjs";
import { currentRuntimeIdentityForCommit } from "../src/job-service/runtime-identity.mjs";

const expected = Object.freeze({
  sourceCommit: "7".repeat(40),
  candidateUrl: "https://candidate-clearra.example.run.app",
  candidateRevision: "clearra-current-job-v075-701454b",
  oracleReleaseId: "v0.7.5-701454b-private-v6",
  oracleReleaseSha256: "a".repeat(64),
  oracleSettingsSha256: "8".repeat(64),
  deploymentNonce: "9".repeat(64),
});
const proof = Object.freeze({
  sourceCommit: expected.sourceCommit,
  candidateUrl: expected.candidateUrl,
  candidateRevision: expected.candidateRevision,
  jobUrl: `${expected.candidateUrl}/jobs`,
  oracleReleaseId: expected.oracleReleaseId,
  oracleReleaseSha256: expected.oracleReleaseSha256,
  oracleSettingsSha256: expected.oracleSettingsSha256,
  deploymentNonce: expected.deploymentNonce,
  gatewayReady: true,
  boundedJobSucceeded: true,
  runtimeIdentity: currentRuntimeIdentityForCommit(expected.sourceCommit),
});

test("Oracle candidate proof binds source, candidate, release, settings, and runtime", () => {
  assert.deepEqual(verifyOracleCandidateProof(proof, expected), {
    sourceCommit: expected.sourceCommit,
    candidateUrl: expected.candidateUrl,
    candidateRevision: expected.candidateRevision,
    oracleReleaseId: expected.oracleReleaseId,
    oracleReleaseSha256: expected.oracleReleaseSha256,
    oracleSettingsSha256: expected.oracleSettingsSha256,
  });
});

test("Oracle candidate proof rejects every stale deployment authority", () => {
  for (const [field, replacement] of [
    ["sourceCommit", "6".repeat(40)],
    ["candidateUrl", "https://stale-clearra.example.run.app"],
    ["candidateRevision", "clearra-current-job-v075-stale"],
    ["oracleReleaseId", "v0.7.5-stale"],
    ["oracleReleaseSha256", "3".repeat(64)],
    ["oracleSettingsSha256", "6".repeat(64)],
    ["deploymentNonce", "5".repeat(64)],
  ]) {
    assert.throws(
      () => verifyOracleCandidateProof(proof, { ...expected, [field]: replacement }),
      /does not match this exact deployment/u,
    );
  }
  assert.throws(
    () => verifyOracleCandidateProof({ ...proof, boundedJobSucceeded: false }, expected),
    /does not match this exact deployment/u,
  );
  assert.throws(
    () =>
      verifyOracleCandidateProof(
        {
          ...proof,
          runtimeIdentity: {
            ...proof.runtimeIdentity,
            engineBuildId: "5".repeat(40),
          },
        },
        expected,
      ),
    /does not match this exact deployment/u,
  );
});

test("Oracle candidate proof file is root-only and consumed exactly once", () => {
  const path = `/run/clearra-deploy/clearra-oracle-candidate-${expected.deploymentNonce}.json`;
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
  consumeOracleCandidateProof(path, expected, dependencies);
  assert.equal(present, false);
  assert.throws(() => consumeOracleCandidateProof(path, expected, dependencies), /missing/u);
});
