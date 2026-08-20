import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import { captureOracleRollbackAuthority } from "../scripts/capture-oracle-rollback-authority.mjs";
import { runtimeIdentitySha256 } from "../scripts/produce-oracle-deployment-proof.mjs";

const nonce = "9".repeat(64);
const settings = Buffer.from(
  "CLEARRA_JOB_URL=https://stable.example.run.app/jobs\nCLEARRA_DISCORD_SECRET_OCID=non-secret-reference\n",
  "utf8",
);
const runtime = {
  sourceCommit: "4".repeat(40),
  engineBuildId: "4".repeat(40),
  contractRevision: "clearra.search.contract.legacy-v1",
};

test("rollback authority capture freezes exact prior release, settings, job, and runtime", () => {
  let backup;
  const captured = captureOracleRollbackAuthority(
    {
      priorRevision: "clearra-current-job-v074-0438d85",
      deploymentNonce: nonce,
    },
    {
      realpath: () => "/opt/clearra/releases/v0.7.4-0438d85-v6rollback",
      releaseTreeSha256: () => "a".repeat(64),
      lstat: () => ({
        uid: 0,
        isFile: () => true,
        isSymbolicLink: () => false,
      }),
      readSettings: () => settings,
      run: () => JSON.stringify({ runtime }),
      writeBackup(path, bytes) {
        backup = { path, bytes };
      },
    },
  );
  assert.equal(captured.priorOracleReleaseId, "v0.7.4-0438d85-v6rollback");
  assert.equal(captured.priorOracleReleaseSha256, "a".repeat(64));
  assert.equal(
    captured.priorOracleSettingsSha256,
    createHash("sha256").update(settings).digest("hex"),
  );
  assert.equal(captured.priorRuntimeIdentitySha256, runtimeIdentitySha256(runtime));
  assert.equal(captured.priorJobUrl, "https://stable.example.run.app/jobs");
  assert.equal(
    backup.path,
    `/etc/clearra-gateway/settings.pre-v0.7.5-${nonce}`,
  );
  assert.equal(backup.bytes, settings);
});

test("rollback authority capture fails before backup on invalid active authority", () => {
  let wrote = false;
  assert.throws(
    () =>
      captureOracleRollbackAuthority(
        {
          priorRevision: "clearra-current-job-v074-0438d85",
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
