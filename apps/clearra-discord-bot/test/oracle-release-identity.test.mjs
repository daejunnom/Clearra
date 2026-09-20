import assert from "node:assert/strict";
import test from "node:test";

import {
  oracleCandidateReleaseId,
  oracleReleaseTagFromCandidateId,
  oracleSettingsBackupPath,
  requireOracleReleaseTag,
} from "../scripts/oracle-release-identity.mjs";

const COMMIT = "0123456789abcdef0123456789abcdef01234567";
const NONCE = "a".repeat(64);

test("Oracle identities retain the exact product release tag", () => {
  assert.equal(requireOracleReleaseTag("v0.8.1"), "v0.8.1");
  assert.equal(oracleCandidateReleaseId("v0.8.1", COMMIT), "v0.8.1-0123456");
  assert.equal(
    oracleReleaseTagFromCandidateId("v0.8.1-0123456", COMMIT),
    "v0.8.1",
  );
  assert.equal(
    oracleSettingsBackupPath("v0.8.1", NONCE),
    `/etc/clearra-gateway/settings.pre-v0.8.1-${NONCE}`,
  );
});

test("Oracle identities reject stale or noncanonical release bindings", () => {
  assert.throws(() => requireOracleReleaseTag("v0.8"), /release tag/u);
  assert.throws(
    () => oracleReleaseTagFromCandidateId("v0.8.0-7654321", COMMIT),
    /differs from source commit/u,
  );
  assert.throws(
    () => oracleSettingsBackupPath("v0.8.1", "b".repeat(63)),
    /deployment nonce/u,
  );
});
