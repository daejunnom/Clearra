import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import {
  ORACLE_CURRENT_AUTHORITY_CLASSIFICATION,
  classifyOracleCurrentAuthority,
} from "../scripts/classify-oracle-current-authority.mjs";
import {
  PRIOR_RUNTIME_IDENTITY_KIND,
  observePriorRuntimeAuthority,
} from "../scripts/oracle-runtime-authority.mjs";

const SOURCE = "0123456789abcdef0123456789abcdef01234567";
const PRIOR_SOURCE = "f".repeat(40);
const CANDIDATE_ID = "v0.8.0-0123456";
const CANDIDATE_REVISION = "clearra-current-job-v080-0123456";
const PRIOR_ID = "v0.8.0-fedcba9";
const PRIOR_REVISION = "clearra-current-job-v080-fedcba9";
const CANDIDATE_URL = "https://candidate.example.test";
const PRIOR_JOB_URL = "https://prior.example.test/jobs";
const CANDIDATE_RELEASE_SHA = "a".repeat(64);
const PRIOR_RELEASE_SHA = "b".repeat(64);

const candidateHealth = health(SOURCE);
const priorHealth = health(PRIOR_SOURCE);
const priorRuntime = observePriorRuntimeAuthority({
  kind: PRIOR_RUNTIME_IDENTITY_KIND,
  priorRevision: PRIOR_REVISION,
  priorOracleReleaseId: PRIOR_ID,
  health: priorHealth,
});

function health(commit) {
  return {
    activeJobs: 0,
    runtime: {
      schema: "clearra.runtime.identity.v2",
      sourceCommit: commit,
      engineBuildId: commit,
      contractSchemaVersion: "clearra.search.contract.v2",
      supplySemanticsId: "clearra.supply.projected-terminal-lookahead.v1",
      artifactSchemaVersion: "clearra.solution-data.v1",
    },
    status: "ok",
    workerLimit: 1,
  };
}

function settings(url) {
  return Buffer.from(`NODE_ENV=production\nCLEARRA_JOB_URL=${url}\n`, "utf8");
}

function sha(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function fixture(state, overrides = {}) {
  const activeId = state === "candidate" ? CANDIDATE_ID : PRIOR_ID;
  const activeJobUrl = state === "candidate" ? `${CANDIDATE_URL}/jobs` : PRIOR_JOB_URL;
  const settingsBytes = overrides.settingsBytes ?? settings(activeJobUrl);
  const activeReleaseSha = overrides.activeReleaseSha ??
    (state === "candidate" ? CANDIDATE_RELEASE_SHA : PRIOR_RELEASE_SHA);
  const activeHealth = overrides.health ?? (state === "candidate" ? candidateHealth : priorHealth);
  return {
    options: {
      sourceCommit: SOURCE,
      candidateUrl: CANDIDATE_URL,
      candidateRevision: CANDIDATE_REVISION,
      candidateReleaseId: CANDIDATE_ID,
      candidateReleaseSha256: CANDIDATE_RELEASE_SHA,
      candidateSettingsSha256: sha(settings(`${CANDIDATE_URL}/jobs`)),
      priorRevision: PRIOR_REVISION,
      priorReleaseId: PRIOR_ID,
      priorReleaseSha256: PRIOR_RELEASE_SHA,
      priorSettingsSha256: sha(settings(PRIOR_JOB_URL)),
      priorRuntimeAuthorityKind: priorRuntime.kind,
      priorRuntimeAuthoritySha256: priorRuntime.sha256,
      priorJobUrl: PRIOR_JOB_URL,
    },
    dependencies: {
      currentLink: "/opt/clearra/current",
      settingsPath: "/etc/clearra-gateway/settings",
      realpath: () => `/opt/clearra/releases/${activeId}`,
      lstat: (path) => ({
        isDirectory: () => path.startsWith("/opt/clearra/releases/"),
        isFile: () => path === "/etc/clearra-gateway/settings",
        isSymbolicLink: () => false,
        uid: 0,
      }),
      readFile: () => settingsBytes,
      releaseTreeSha256: () => activeReleaseSha,
      fetchHealth: () => activeHealth,
    },
  };
}

test("read-only classifier accepts only the exact sealed candidate authority", () => {
  const input = fixture("candidate");
  const result = classifyOracleCurrentAuthority(input.options, input.dependencies);
  assert.equal(result.contract, ORACLE_CURRENT_AUTHORITY_CLASSIFICATION);
  assert.equal(result.state, "candidate");
  assert.equal(result.activeReleaseId, CANDIDATE_ID);
  assert.equal(result.activeReleaseSha256, CANDIDATE_RELEASE_SHA);
  assert.equal(result.runtimeAuthorityKind, PRIOR_RUNTIME_IDENTITY_KIND);
  assert.match(result.runtimeAuthoritySha256, /^[0-9a-f]{64}$/u);
});

test("read-only classifier accepts the exact captured prior authority as a no-op", () => {
  const input = fixture("prior");
  const result = classifyOracleCurrentAuthority(input.options, input.dependencies);
  assert.equal(result.state, "prior");
  assert.equal(result.activeReleaseId, PRIOR_ID);
  assert.equal(result.runtimeAuthoritySha256, priorRuntime.sha256);
});

test("mixed release, settings, digest, and runtime states are all classified other", () => {
  const mixedSettings = fixture("candidate", { settingsBytes: settings(PRIOR_JOB_URL), health: priorHealth });
  assert.equal(
    classifyOracleCurrentAuthority(mixedSettings.options, mixedSettings.dependencies).state,
    "other",
  );
  const mixedDigest = fixture("candidate", { activeReleaseSha: "c".repeat(64) });
  assert.equal(
    classifyOracleCurrentAuthority(mixedDigest.options, mixedDigest.dependencies).state,
    "other",
  );
  const mixedRuntime = fixture("candidate", { health: priorHealth });
  assert.equal(
    classifyOracleCurrentAuthority(mixedRuntime.options, mixedRuntime.dependencies).state,
    "other",
  );
});
