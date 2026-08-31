import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  sealDiscordDeploymentState,
  verifyDiscordDeploymentState,
} from "./discord-deployment-state.mjs";

const SOURCE = "0123456789abcdef0123456789abcdef01234567";
const PREPARED_NAMES = [
  "accepted_ctk3_manifest",
  "canonical_acceptance_evidence",
  "cloud_build_authority",
  "cloud_build_readback",
  "ctk3_actions_archive",
  "dependencies_actions_archive",
  "exact_source_archive",
  "recovery_debt_clearance",
];
const SYNCHRONIZED_NAMES = [
  "discord_catalog",
  "discord_prior_catalog",
  "discord_sync_authority",
  "discord_sync_report",
  "oracle_end_observation",
  "pages_deployment_authority",
  "production_observation",
  "production_probe_authority",
  "production_probe_spec",
  "promoted_state",
];

function canonicalJson(value) {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  return `{${Object.keys(value).sort().map((key) =>
    `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
}

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "clearra-discord-state-"));
  const evidence = join(root, "evidence");
  await mkdir(evidence);
  const bindings = [];
  for (const name of PREPARED_NAMES) {
    const path = join(evidence, `${name}.bin`);
    await writeFile(path, `sealed:${name}\n`, { mode: 0o600 });
    bindings.push(`${name}=${path}`);
  }
  return { root, evidence, bindings };
}

function identity(bindings, overrides = {}) {
  return {
    stage: "prepared",
    sourceCommit: SOURCE,
    workflowRunId: "91",
    workflowRunAttempt: "1",
    acceptedRunId: "81",
    acceptedRunAttempt: "1",
    deploymentNonce: "a".repeat(64),
    bindings,
    ...overrides,
  };
}

test("seals and verifies the exact closed prepared artifact state", async () => {
  const value = await fixture();
  try {
    const report = await sealDiscordDeploymentState(identity(value.bindings));
    const reportPath = join(value.evidence, "candidate-state.json");
    await writeFile(reportPath, `${canonicalJson(report)}\n`, { mode: 0o600 });
    const verified = await verifyDiscordDeploymentState(
      reportPath,
      identity([...value.bindings].reverse()),
    );
    assert.equal(verified.report.source_commit, SOURCE);
    assert.equal(verified.report.workflow_run_attempt, "1");
    assert.equal(verified.report.bindings.length, PREPARED_NAMES.length);
    assert.match(verified.fileSha256, /^[0-9a-f]{64}$/u);
  } finally {
    await rm(value.root, { recursive: true, force: true });
  }
});

test("binds exact workflow rerun attempts and rejects incomplete binding sets", async () => {
  const value = await fixture();
  try {
    const report = await sealDiscordDeploymentState(
      identity(value.bindings, { workflowRunAttempt: "2" }),
    );
    assert.equal(report.workflow_run_attempt, "2");
    await assert.rejects(
      sealDiscordDeploymentState(identity(value.bindings.slice(1))),
      /bindings are not the closed set/u,
    );
  } finally {
    await rm(value.root, { recursive: true, force: true });
  }
});

test("rejects mutation after the state is sealed", async () => {
  const value = await fixture();
  try {
    const report = await sealDiscordDeploymentState(identity(value.bindings));
    const reportPath = join(value.evidence, "candidate-state.json");
    await writeFile(reportPath, `${canonicalJson(report)}\n`, { mode: 0o600 });
    const firstPath = value.bindings[0].slice(value.bindings[0].indexOf("=") + 1);
    await writeFile(firstPath, "changed\n", { mode: 0o600 });
    await assert.rejects(
      verifyDiscordDeploymentState(reportPath, identity(value.bindings)),
      /bound files differ/u,
    );
    assert.match(await readFile(reportPath, "utf8"), /report_sha256/u);
  } finally {
    await rm(value.root, { recursive: true, force: true });
  }
});

test("synchronized state closes over the sole canonical four-surface observation", async () => {
  const root = await mkdtemp(join(tmpdir(), "clearra-discord-state-sync-"));
  try {
    const bindings = [];
    for (const name of SYNCHRONIZED_NAMES) {
      const path = join(root, `${name}.json`);
      await writeFile(path, `sealed:${name}\n`, { mode: 0o600 });
      bindings.push(`${name}=${path}`);
    }
    const report = await sealDiscordDeploymentState({
      stage: "synchronized",
      sourceCommit: SOURCE,
      workflowRunId: "91",
      workflowRunAttempt: "1",
      acceptedRunId: "81",
      acceptedRunAttempt: "1",
      deploymentNonce: "a".repeat(64),
      verifiedAfter: "2026-08-31T00:00:00.000Z",
      bindings,
    });
    assert.deepEqual(
      report.bindings.map(({ name }) => name),
      SYNCHRONIZED_NAMES,
    );
    await assert.rejects(
      sealDiscordDeploymentState({
        stage: "synchronized",
        sourceCommit: SOURCE,
        workflowRunId: "91",
        workflowRunAttempt: "1",
        acceptedRunId: "81",
        acceptedRunAttempt: "1",
        deploymentNonce: "a".repeat(64),
        verifiedAfter: "2026-08-31T00:00:00.000Z",
        bindings: bindings.filter((entry) => !entry.startsWith("production_observation=")),
      }),
      /bindings are not the closed set/u,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
