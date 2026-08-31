import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  ORACLE_OBSERVATION_CONTRACT,
  observeOracleCandidate,
} from "../scripts/observe-oracle-candidate.mjs";

const sourceCommit = "0123456789abcdef0123456789abcdef01234567";
const candidateUrl = "https://candidate.example.test";
const candidateRevision = "clearra-current-job-v080-0123456";
const oracleReleaseId = "v0.8.0-0123456";
const oracleReleaseSha256 = "a".repeat(64);
const oracleSettingsSha256 = "b".repeat(64);
const deploymentNonce = "c".repeat(64);
const verifiedAfter = "2026-08-30T00:00:00.000Z";
const freshOperationAt = "2026-08-30T00:00:01.000Z";
const observedAt = "2026-08-30T00:00:02.000Z";
const launcherPath = fileURLToPath(
  new URL("../../../scripts/release/oracle/clearra-oracle-release-deploy-v080", import.meta.url),
);

function fixture(overrides = {}) {
  return {
    sourceCommit,
    candidateUrl,
    candidateRevision,
    oracleReleaseId,
    oracleReleaseSha256,
    oracleSettingsSha256,
    deploymentNonce,
    verifiedAfter,
    ...overrides,
  };
}

function dependencies(activeOverrides = {}, commandOverrides = {}) {
  return {
    inspectActiveOracle: () => ({
      activeReleasePath: `/opt/clearra/releases/${oracleReleaseId}`,
      activeReleaseSha256: oracleReleaseSha256,
      activeSettingsSha256: oracleSettingsSha256,
      gatewayPid: "4312",
      readyRecordObserved: true,
      freshOperationAt,
      ...activeOverrides,
    }),
    now: () => new Date(observedAt),
    run: (command, arguments_) => {
      const key = `${command} ${arguments_.join(" ")}`;
      if (key in commandOverrides) return commandOverrides[key];
      if (command === "/usr/bin/systemctl") return "987654321\n";
      if (command === "/usr/bin/cat") {
        return "123e4567-e89b-42d3-a456-426614174000\n";
      }
      throw new Error(`unexpected command: ${key}`);
    },
  };
}

test("produces a closed read-only Oracle candidate observation", () => {
  const observation = observeOracleCandidate(fixture(), dependencies());
  assert.equal(observation.contract, ORACLE_OBSERVATION_CONTRACT);
  assert.equal(observation.activeReleasePath, `/opt/clearra/releases/${oracleReleaseId}`);
  assert.equal(observation.oracleReleaseSha256, oracleReleaseSha256);
  assert.equal(observation.oracleSettingsSha256, oracleSettingsSha256);
  assert.equal(observation.gatewayPid, 4312);
  assert.equal(observation.gatewayStartMonotonicUsec, 987654321);
  assert.equal(observation.bootId, "123e4567-e89b-42d3-a456-426614174000");
  assert.equal(observation.readyRecordObserved, true);
  assert.equal(observation.verifiedAfter, verifiedAfter);
  assert.equal(observation.freshOperationAt, freshOperationAt);
  assert.equal(observation.observedAt, observedAt);
  assert.deepEqual(Object.keys(observation), [
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
    "verifiedAfter",
    "gatewayPid",
    "gatewayStartMonotonicUsec",
    "bootId",
    "readyRecordObserved",
    "freshOperationAt",
    "observedAt",
    "runtimeIdentity",
  ]);
});

test("rejects stale operation, process, release, settings, and key drift", () => {
  assert.throws(
    () => observeOracleCandidate(
      fixture({ verifiedAfter: "2026-08-30T00:00:00Z" }),
      dependencies(),
    ),
    /canonical ISO timestamp/u,
  );
  assert.throws(
    () => observeOracleCandidate(
      fixture(),
      dependencies({ freshOperationAt: "2026-08-29T23:59:59.000Z" }),
    ),
    /predates the observation authority/u,
  );
  assert.throws(
    () => observeOracleCandidate(fixture(), dependencies({ gatewayPid: "0" })),
    /Gateway PID is invalid/u,
  );
  assert.throws(
    () => observeOracleCandidate(
      fixture(),
      dependencies({ gatewayPid: "9007199254740992" }),
    ),
    /Gateway PID is invalid/u,
  );
  assert.throws(
    () => observeOracleCandidate(
      fixture(),
      dependencies({ activeReleaseSha256: "d".repeat(64) }),
    ),
    /release SHA-256 is invalid/u,
  );
  assert.throws(
    () => observeOracleCandidate(
      fixture(),
      dependencies({ activeSettingsSha256: "e".repeat(64) }),
    ),
    /settings SHA-256 is invalid/u,
  );
  assert.throws(
    () => observeOracleCandidate(
      fixture(),
      dependencies({ unexpected: true }),
    ),
    /keys do not match the closed schema/u,
  );
});

test("rejects process-instance and observation freshness drift", () => {
  assert.throws(
    () => observeOracleCandidate(
      fixture(),
      dependencies({}, { "/usr/bin/systemctl show --property ExecMainStartTimestampMonotonic --value clearra-gateway.service": "0\n" }),
    ),
    /monotonic start is invalid/u,
  );
  assert.throws(
    () => observeOracleCandidate(
      fixture(),
      dependencies({}, { "/usr/bin/systemctl show --property ExecMainStartTimestampMonotonic --value clearra-gateway.service": "9007199254740992\n" }),
    ),
    /monotonic start is invalid/u,
  );
  assert.throws(
    () => observeOracleCandidate(
      fixture(),
      dependencies({}, { "/usr/bin/cat /proc/sys/kernel/random/boot_id": "not-a-boot-id\n" }),
    ),
    /boot ID is invalid/u,
  );
  assert.throws(
    () => observeOracleCandidate(
      fixture(),
      { ...dependencies(), now: () => new Date("2026-08-30T00:00:00.500Z") },
    ),
    /timestamp predates its fresh operation/u,
  );
});

test("remote observation launcher operation remains read-only", async () => {
  const launcher = await readFile(launcherPath, "utf8");
  const match = launcher.match(
    /\n  observe-candidate\)\n(?<body>[\s\S]*?)\n    ;;\n\n  restore-prior-and-verify\)/u,
  );
  assert.ok(match?.groups?.body, "observe-candidate launcher block is unavailable");
  assert.match(
    match.groups.body,
    /exec "\$node_path" "\$observer_script"/u,
  );
  assert.doesNotMatch(
    match.groups.body,
    /(?:systemctl_path|\bmv\b|\brm\b|temporary_settings|current_link|proof_directory)/u,
  );
  assert.match(
    launcher,
    /require_root_regular_readonly \\\n+    "\$script_release\/apps\/clearra-discord-bot\/scripts\/observe-oracle-candidate\.mjs"/u,
  );
});
