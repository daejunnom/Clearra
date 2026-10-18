import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  canonicalJson,
  canonicalSha256,
  sealCanonicalReport,
} from "./canonical-release-evidence.mjs";
import {
  createDiscordSuccessfulDeploymentTopologyContract,
  DISCORD_CHECKPOINT_JOB_CAPTURE_STEP,
  DISCORD_SUCCESSFUL_DEPLOYMENT_JOB_NAMES,
  DISCORD_SUCCESSFUL_DEPLOYMENT_JOB_STEPS,
  validateDiscordCheckpointCandidatePrerequisites,
  validateDiscordCheckpointPrerequisiteProof,
} from "./discord-deployment-recovery.mjs";
import {
  checkpointCandidateArtifactName,
  DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_FILE,
  DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_SCHEMA_ID,
  validateDiscordProductionCheckpointCandidate,
} from "./discord-production-checkpoint-receipt.mjs";
import {
  observeProductionSurfaces,
  PRODUCTION_SURFACE_PROBE_SCHEMA_ID,
} from "./observe-production-surfaces.mjs";
import { checkpointInputsFromEnvironment, retryEvidenceWrite, snapshotEvidenceUpload } from "./production-evidence-retry.mjs";
import { CURRENT_PRODUCT_TAG, CURRENT_PRODUCT_VERSION, expectedProductArtifactName } from "./current-product-release.mjs";
import { CANONICAL_ACCEPTANCE_REQUIRED_JOB_NAMES } from "./canonical-acceptance-evidence.mjs";
import { sealDiscordCatalogRecoveryAuthority } from "./discord-catalog-recovery-authority.mjs";
import {
  captureDiscordCatalogSnapshot,
  createCanonicalDiscordCatalog,
} from "../../apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs";

const REPOSITORY = "daejunnom/Clearra";
export const SOURCE = "1".repeat(40);
export const RUN_ID = "50";
export const RUN_ATTEMPT = "1";
export const ACCEPTED_RUN_ID = "40";
export const ACCEPTED_RUN_ATTEMPT = "1";
const HASH = "a".repeat(64);
const APPLICATION_ID = "223456789012345678";

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
test("production evidence preflight and retry bind the current release and keep both run identities", () => {
  const root = resolve("fixture-workspace");
  const env = {
    GITHUB_REPOSITORY: REPOSITORY,
    SOURCE_COMMIT: SOURCE,
    GITHUB_RUN_ID: RUN_ID,
    GITHUB_RUN_ATTEMPT: RUN_ATTEMPT,
    ACCEPTED_RUN_ID,
    ACCEPTED_RUN_ATTEMPT,
    DISCORD_APPLICATION_ID: APPLICATION_ID,
    CATALOG_ARTIFACT_ID: "9900",
    CATALOG_ARTIFACT_DIGEST: `sha256:${HASH}`,
  };
  const options = checkpointInputsFromEnvironment(root, env);
  assert.equal(options.version, CURRENT_PRODUCT_VERSION);
  assert.equal(options.sourceCommit, SOURCE);
  assert.equal(options.workflowRunId, RUN_ID);
  assert.equal(options.workflowRunAttempt, RUN_ATTEMPT);
  assert.equal(options.acceptedRunId, ACCEPTED_RUN_ID);
  assert.equal(options.acceptedRunAttempt, ACCEPTED_RUN_ATTEMPT);
  assert.notEqual(options.workflowRunId, options.acceptedRunId);
  assert.equal(options.canonicalAcceptanceEvidence,
    resolve(root, "promoted/prepared/canonical-acceptance/clearra-canonical-acceptance-evidence.v1.json"));
});

test("the real checkpoint CLI seals and verifies exact inputs without a retry-module import deadlock", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "clearra-checkpoint-cli-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const options = await checkpointCliFixture(root);
  const cli = fileURLToPath(new URL("./discord-production-checkpoint-receipt.mjs", import.meta.url));
  const output = join(root, "candidate.json");
  const run = (operation, overrides = {}) => spawnSync(process.execPath, [
    cli, operation, ...Object.entries({ ...options, output, ...overrides })
      .flatMap(([name, value]) => [`--${name}`, value]),
  ], { encoding: "utf8", timeout: 10_000, maxBuffer: 64 * 1024, windowsHide: true });
  for (let attempt = 0; attempt < 2; attempt += 1) {
    const sealed = run("seal-candidate");
    assert.ifError(sealed.error);
    assert.equal(sealed.signal, null);
    assert.equal(sealed.status, 0, sealed.stderr);
    assert.match(sealed.stdout, /^discord_checkpoint_candidate=sealed sha256=[0-9a-f]{64}\n$/u);
    assert.equal(sealed.stderr, "");
    const bytes = await readFile(output, "utf8");
    const candidate = JSON.parse(bytes);
    validateDiscordProductionCheckpointCandidate(candidate, identity());
    assert.equal(bytes, `${canonicalJson(candidate)}\n`);
    const verified = run("verify-candidate", { report: output });
    assert.ifError(verified.error);
    assert.equal(verified.status, 0, verified.stderr);
    assert.equal(verified.stdout, `discord_checkpoint_candidate=verified sha256=${candidate.report_sha256}\n`);
  }
  const rejectedOutput = join(root, "foreign-candidate.json");
  const rejected = run("seal-candidate", { "accepted-run-id": "99", output: rejectedOutput });
  assert.ifError(rejected.error);
  assert.equal(rejected.status, 2);
  assert.match(rejected.stderr, /canonical acceptance evidence differs from the bound run attempt/u);
  await assert.rejects(readFile(rejectedOutput), { code: "ENOENT" });
});

test("evidence I/O retry requires a live revalidation and never retries invalid or uncertain state", async () => {
  const calls = [];
  const transient = () => Object.assign(new Error("temporary I/O"), { code: "EIO" });
  const result = await retryEvidenceWrite(async () => {
    calls.push("write");
    if (calls.length === 1) throw transient();
    return "sealed";
  }, async () => { calls.push("verify-live"); }, { waitForRetry: async () => {} });
  assert.equal(result, "sealed");
  assert.deepEqual(calls, ["write", "verify-live", "write"]);
  let writes = 0;
  await assert.rejects(retryEvidenceWrite(async () => { writes += 1; throw transient(); },
    async () => { throw new Error("live identity changed"); }, { waitForRetry: async () => {} }), /live identity changed/u);
  assert.equal(writes, 1);
  await assert.rejects(retryEvidenceWrite(async () => { throw new Error("invalid schema"); },
    async () => { assert.fail("validation failures cannot request a retry"); }), /invalid schema/u);
  writes = 0;
  await assert.rejects(retryEvidenceWrite(async () => { writes += 1; throw transient(); },
    async () => {}, { waitForRetry: async () => {} }), /temporary I\/O/u);
  assert.equal(writes, 3);
});

test("upload snapshots bind bytes and reject foreign attempts or unexpected leaves", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "clearra-evidence-upload-"));
  assert.ok(root.startsWith(join(tmpdir(), "clearra-evidence-upload-")));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(join(root, "sync"));
  const file = join(root, "sync", "discord-sync-report.json");
  await writeFile(file, '{"example":1}\n');
  const env = { SOURCE_COMMIT: SOURCE, GITHUB_RUN_ID: RUN_ID, GITHUB_RUN_ATTEMPT: RUN_ATTEMPT };
  const name = `discord-sync-inputs-${SOURCE}-run-${RUN_ID}-attempt-${RUN_ATTEMPT}`;
  const before = await snapshotEvidenceUpload(root, "sync", name, env);
  await writeFile(file, '{"example":2}\n');
  const after = await snapshotEvidenceUpload(root, "sync", name, env);
  assert.notEqual(before.report_sha256, after.report_sha256);
  await assert.rejects(snapshotEvidenceUpload(root, "sync", `${name}-foreign`, env), /active attempt/u);
  await writeFile(join(root, "sync", "unexpected.json"), '{}\n');
  await assert.rejects(snapshotEvidenceUpload(root, "sync", name, env), /unexpected leaves/u);
});

test("checkpoint prerequisite proof is exact, ordered, and stops before its own upload", () => {
  const jobs = checkpointJobList();
  const proof = validateDiscordCheckpointCandidatePrerequisites(jobs, identity());
  assert.equal(
    validateDiscordCheckpointPrerequisiteProof(proof, identity()),
    proof,
  );
  assert.equal(proof.jobs.length, 4);
  const sync = proof.jobs.find(({ job_name: name }) => name === "sync-observe");
  assert.equal(sync.steps.at(-1).name, DISCORD_CHECKPOINT_JOB_CAPTURE_STEP);
  assert.equal(sync.steps.at(-1).status, "in_progress");
  assert.equal(
    sync.steps.some(({ name }) => name === "Upload canonical Discord production checkpoint candidate"),
    false,
  );

  const shortWindow = checkpointJobList();
  const observation = shortWindow.jobs.find(({ name }) => name === "sync-observe").steps
    .find(({ name }) => name ===
      "Authority-bound global sync and sole canonical four-surface observation");
  observation.completed_at = new Date(Date.parse(observation.started_at) + 1_199_000).toISOString();
  assert.throws(
    () => validateDiscordCheckpointCandidatePrerequisites(shortWindow, identity()),
    /1200|durable observation/u,
  );

  const failedCatalogUpload = checkpointJobList();
  failedCatalogUpload.jobs.find(({ name }) => name === "sync-observe").steps
    .find(({ name }) => name ===
      "Upload Discord catalog recovery authority before global mutation").conclusion = "failure";
  assert.throws(
    () => validateDiscordCheckpointCandidatePrerequisites(failedCatalogUpload, identity()),
    /conclusion differs/u,
  );
});

test("canonical checkpoint candidate embeds semantic prerequisites but cannot claim future upload", async () => {
  const proof = validateDiscordCheckpointCandidatePrerequisites(checkpointJobList(), identity());
  const candidate = await candidateFixture(proof);
  assert.equal(
    validateDiscordProductionCheckpointCandidate(candidate, identity()),
    candidate,
  );
  assert.equal(candidate.schema_id, DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_SCHEMA_ID);
  assert.equal(candidate.expected_artifact_leaf, DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_FILE);
  assert.equal(
    candidate.expected_artifact_name,
    checkpointCandidateArtifactName(SOURCE, RUN_ID, RUN_ATTEMPT),
  );
  assert.equal(candidate.release_artifacts.length, 3);

  const artifactTamper = structuredClone(candidate);
  artifactTamper.release_artifacts[2].size_bytes = 0;
  assert.throws(
    () => validateDiscordProductionCheckpointCandidate(reseal(artifactTamper), identity()),
    /release artifacts/u,
  );

  const futureClaim = structuredClone(candidate);
  futureClaim.discord_completed_at = "2026-08-31T00:30:00.000Z";
  assert.throws(
    () => validateDiscordProductionCheckpointCandidate(reseal(futureClaim), identity()),
    /closed schema/u,
  );

  const topologyTamper = structuredClone(candidate);
  topologyTamper.deployment_topology_contract.jobs[0].steps[0].name = "Foreign mutator";
  topologyTamper.deployment_topology_contract = reseal(
    topologyTamper.deployment_topology_contract,
  );
  topologyTamper.deployment_topology_contract_sha256 =
    topologyTamper.deployment_topology_contract.report_sha256;
  assert.throws(
    () => validateDiscordProductionCheckpointCandidate(reseal(topologyTamper), identity()),
    /topology contract differs/u,
  );
});
}

export function identity() {
  return {
    repository: REPOSITORY,
    sourceCommit: SOURCE,
    workflowRunId: RUN_ID,
    workflowRunAttempt: RUN_ATTEMPT,
    acceptedRunId: ACCEPTED_RUN_ID,
    acceptedRunAttempt: ACCEPTED_RUN_ATTEMPT,
  };
}

export function checkpointJobList({ completed = false } = {}) {
  const jobs = DISCORD_SUCCESSFUL_DEPLOYMENT_JOB_NAMES.map((name, jobIndex) => {
    const allNames = DISCORD_SUCCESSFUL_DEPLOYMENT_JOB_STEPS[name];
    const names = name === "sync-observe" && !completed
      ? allNames.slice(0, allNames.indexOf(DISCORD_CHECKPOINT_JOB_CAPTURE_STEP) + 1)
      : allNames;
    let cursor = Date.parse("2026-08-31T00:00:00.000Z") + jobIndex * 3_000_000;
    const steps = names.map((stepName, index) => {
      if (stepName === DISCORD_CHECKPOINT_JOB_CAPTURE_STEP && !completed) {
        return {
          name: stepName,
          number: index + 1,
          status: "in_progress",
          conclusion: null,
          started_at: new Date(cursor).toISOString(),
          completed_at: null,
        };
      }
      const duration = stepName ===
        "Authority-bound global sync and sole canonical four-surface observation"
        ? 1_200_000
        : 1_000;
      const startedAt = new Date(cursor).toISOString();
      const completedAt = new Date(cursor + duration).toISOString();
      cursor += duration + 1_000;
      const skipped = [
        "Record explicit no-op for changes outside Discord",
        "Compensate any protected-path failure after Oracle transition began",
        "Remove only this failed deployment candidate tag",
        "Compensate catalog mutation if any later sync job step failed",
        "Upload durable catalog compensation evidence",
      ].includes(stepName);
      return {
        name: stepName,
        number: index + 1,
        status: "completed",
        conclusion: skipped ? "skipped" : "success",
        started_at: startedAt,
        completed_at: completedAt,
      };
    });
    return {
      id: 9000 + jobIndex,
      run_id: Number(RUN_ID),
      run_attempt: Number(RUN_ATTEMPT),
      head_sha: SOURCE,
      head_branch: "main",
      status: name === "sync-observe" && !completed ? "in_progress" : "completed",
      conclusion: name === "sync-observe" && !completed ? null : "success",
      name,
      html_url: `https://github.com/${REPOSITORY}/actions/runs/${RUN_ID}/job/${9000 + jobIndex}`,
      steps,
    };
  });
  return { total_count: jobs.length, jobs };
}

export function completedCheckpointJobList() {
  return checkpointJobList({ completed: true });
}

export async function candidateFixture(prerequisiteProof) {
  const observationStartedAt = prerequisiteProof.jobs
    .find(({ job_name: name }) => name === "sync-observe").steps
    .find(({ name }) => name ===
      "Authority-bound global sync and sole canonical four-surface observation").started_at;
  const clearance = sealCanonicalReport({
    schema_id: "clearra.discord-recovery-debt-clearance.v1",
    repository: REPOSITORY,
    current_workflow_run_id: RUN_ID,
    current_workflow_run_attempt: RUN_ATTEMPT,
    current_source_commit: SOURCE,
    checkpoint_sha256: "b".repeat(64),
    plan_sha256: "c".repeat(64),
    cleared_debts: [],
  });
  const syncAuthority = sealCanonicalReport({ authority: "sync" });
  const priorCatalogSha = "d".repeat(64);
  const desiredCatalogSha = "e".repeat(64);
  const syncReport = sealCanonicalReport({
    command_sync_authority_sha256: syncAuthority.report_sha256,
    current_before_sha256: priorCatalogSha,
    expected_catalog_sha256: desiredCatalogSha,
    current_after_sha256: "b".repeat(64),
  });
  const observation = await observeProductionSurfaces({
    sourceCommit: SOURCE,
    durationSeconds: 1200,
    intervalSeconds: 1200,
    clock: fakeClock(observationStartedAt),
    probes: probeSet(observationStartedAt, syncReport),
    probeSpec: probeSpec(),
  });
  const catalogAuthority = sealCanonicalReport({
    source_commit: SOURCE,
    workflow_run_id: RUN_ID,
    workflow_run_attempt: RUN_ATTEMPT,
  });
  const catalogDisposition = sealCanonicalReport({
    schema_id: "clearra.discord-production-catalog-disposition.v1",
    application_id: APPLICATION_ID,
    catalog_artifact_id: "9900",
    catalog_artifact_digest: `sha256:${"f".repeat(64)}`,
    catalog_recovery_authority: catalogAuthority,
    catalog_recovery_authority_file_sha256: fileSha(catalogAuthority),
    prior_snapshot_sha256: "0".repeat(64),
    prior_catalog_sha256: priorCatalogSha,
    prior_snapshot_file_sha256: "1".repeat(64),
    desired_catalog_sha256: desiredCatalogSha,
    desired_catalog_file_sha256: "2".repeat(64),
    discord_sync_authority: syncAuthority,
    discord_sync_authority_file_sha256: fileSha(syncAuthority),
    discord_sync_report: syncReport,
    discord_sync_report_file_sha256: fileSha(syncReport),
  });
  const topology = createDiscordSuccessfulDeploymentTopologyContract();
  const releaseArtifacts = ["linux-cli", "windows-cli", "windows-gui"].map(
    (role, index) => ({
      role,
      name: `artifact-${index}.zip`,
      sha256: String(index + 3).repeat(64),
      size_bytes: index + 1,
      source_commit: SOURCE,
    }),
  );
  return sealCanonicalReport({
    schema_id: DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_SCHEMA_ID,
    repository: REPOSITORY,
    repository_id: 1309293231,
    source_commit: SOURCE,
    accepted_workflow_run_id: ACCEPTED_RUN_ID,
    accepted_workflow_run_attempt: ACCEPTED_RUN_ATTEMPT,
    discord_workflow_run_id: RUN_ID,
    discord_workflow_run_attempt: RUN_ATTEMPT,
    canonical_acceptance_evidence_sha256: "3".repeat(64),
    canonical_acceptance_evidence_file_sha256: "4".repeat(64),
    release_artifacts: releaseArtifacts,
    deployment_topology_contract: topology,
    deployment_topology_contract_sha256: topology.report_sha256,
    deployment_prerequisite_job_proof: prerequisiteProof,
    deployment_prerequisite_job_proof_sha256: prerequisiteProof.report_sha256,
    recovery_debt_clearance: clearance,
    recovery_debt_clearance_sha256: clearance.report_sha256,
    recovery_debt_clearance_file_sha256: fileSha(clearance),
    catalog_disposition: catalogDisposition,
    catalog_disposition_sha256: catalogDisposition.report_sha256,
    production_observation: observation,
    production_observation_sha256: observation.report_sha256,
    production_observation_file_sha256: fileSha(observation),
    expected_artifact_name: checkpointCandidateArtifactName(SOURCE, RUN_ID, RUN_ATTEMPT),
    expected_artifact_leaf: DISCORD_PRODUCTION_CHECKPOINT_CANDIDATE_FILE,
  });
}

async function checkpointCliFixture(root) {
  const write = async (name, value) => {
    const path = join(root, `${name}.json`);
    await writeFile(path, `${canonicalJson(value)}\n`);
    return path;
  };
  const jobs = checkpointJobList();
  const observationStartedAt = jobs.jobs.find(({ name }) => name === "sync-observe").steps
    .find(({ name }) => name === "Authority-bound global sync and sole canonical four-surface observation").started_at;
  const acceptance = sealCanonicalReport({
    schema_id: "clearra.canonical-acceptance-evidence.v1",
    repository: REPOSITORY, release_version: CURRENT_PRODUCT_VERSION, pages_base_path: "/Clearra",
    source_commit: SOURCE, run_id: ACCEPTED_RUN_ID, run_attempt: ACCEPTED_RUN_ATTEMPT,
    workflow_path: ".github/workflows/release-cli.yml", status: "passed",
    jobs: CANONICAL_ACCEPTANCE_REQUIRED_JOB_NAMES.map((name, index) => ({
      name, job_id: String(9000 + index), status: "passed",
    })),
    accepted_inputs: {
      ctk3_manifest_sha256: HASH, pages_identity_sha256: HASH,
      wasm_build_receipt_sha256: HASH, gate_index_sha256: HASH,
    },
    final_source_fragments: {
      toolchains: { source_commit: SOURCE, manifest_sha256: HASH, rust: "fixture", node: "fixture", wasm_bindgen: "fixture" },
      canonical_gate: {
        id: `release-acceptance-run-${ACCEPTED_RUN_ID}-attempt-${ACCEPTED_RUN_ATTEMPT}`,
        sha256: HASH, source_commit: SOURCE, status: "passed", readiness_open_count: 0,
      },
      surface_reports: ["desktop", "discord", "native", "wasm"].map(surface => ({
        id: `${surface}-run-${ACCEPTED_RUN_ID}-attempt-${ACCEPTED_RUN_ATTEMPT}`,
        sha256: HASH, source_commit: SOURCE, surface, status: "passed",
      })),
      release_artifacts: ["linux-cli", "windows-cli", "windows-gui"].map((role, index) => ({
        role, name: expectedProductArtifactName(role), sha256: HASH, size_bytes: index + 1, source_commit: SOURCE,
      })),
    },
  });
  const catalog = createCanonicalDiscordCatalog({ sourceCommit: SOURCE, commands: [
    { name: "help", description: "Help" }, { type: 3, name: "Get original GIF" },
  ] });
  const prior = await captureDiscordCatalogSnapshot({
    rest: { async getGlobalCommands() { return catalog.commands; } },
    applicationId: APPLICATION_ID, sourceCommit: SOURCE,
    observedAt: new Date(Date.parse(observationStartedAt) - 10_000).toISOString(),
  });
  const syncAuthority = sealCanonicalReport({
    schema_id: "clearra.discord.command-sync-authority.v1", source_commit: SOURCE,
    repository: REPOSITORY, release_version: CURRENT_PRODUCT_VERSION, pages_base_path: "/Clearra",
    accepted_run_id: ACCEPTED_RUN_ID, accepted_run_attempt: ACCEPTED_RUN_ATTEMPT,
    accepted_ctk3_manifest_sha256: HASH,
    canonical_acceptance_evidence_sha256: acceptance.report_sha256,
    canonical_acceptance_evidence_file_sha256: fileSha(acceptance),
    command_catalog_sha256: catalog.catalog_sha256, command_catalog_file_sha256: fileSha(catalog),
  });
  const syncReport = sealCanonicalReport({
    schema_id: "clearra.discord.command-catalog-sync.v1", source_commit: SOURCE, application_id: APPLICATION_ID,
    started_at: new Date(Date.parse(observationStartedAt) - 9_000).toISOString(),
    ended_at: new Date(Date.parse(observationStartedAt) - 8_000).toISOString(),
    status: "synchronized", changed: false, command_count: catalog.command_count,
    expected_catalog_sha256: catalog.catalog_sha256, accepted_run_id: ACCEPTED_RUN_ID,
    accepted_run_attempt: ACCEPTED_RUN_ATTEMPT, accepted_ctk3_manifest_sha256: HASH,
    canonical_acceptance_evidence_sha256: acceptance.report_sha256,
    canonical_acceptance_evidence_file_sha256: fileSha(acceptance),
    command_catalog_file_sha256: fileSha(catalog), command_sync_authority_sha256: syncAuthority.report_sha256,
    command_sync_authority_file_sha256: fileSha(syncAuthority), prior_snapshot_sha256: prior.snapshot_sha256,
    prior_catalog_sha256: prior.catalog_sha256, current_before_sha256: prior.catalog_sha256,
    current_after_sha256: catalog.catalog_sha256,
  });
  const priorPath = await write("prior", prior);
  const catalogPath = await write("catalog", catalog);
  const syncAuthorityPath = await write("sync-authority", syncAuthority);
  const catalogAuthority = await sealDiscordCatalogRecoveryAuthority({
    ...identity(), applicationId: APPLICATION_ID, priorSnapshot: priorPath,
    desiredCatalog: catalogPath, syncAuthority: syncAuthorityPath,
  });
  const clearance = sealCanonicalReport({
    schema_id: "clearra.discord-recovery-debt-clearance.v1", repository: REPOSITORY,
    current_workflow_run_id: RUN_ID, current_workflow_run_attempt: RUN_ATTEMPT, current_source_commit: SOURCE,
    checkpoint_sha256: HASH, plan_sha256: HASH, cleared_debts: [],
  });
  const probes = probeSet(observationStartedAt, syncReport);
  const discordProbe = probes.discord;
  probes.discord = async context => {
    const result = await discordProbe(context);
    Object.assign(result.identity, {
      command_catalog_prior_snapshot_sha256: prior.snapshot_sha256,
      command_catalog_readback_sha256: catalog.catalog_sha256,
      accepted_ctk3_manifest_sha256: HASH,
      canonical_acceptance_evidence_sha256: acceptance.report_sha256,
      canonical_acceptance_evidence_file_sha256: fileSha(acceptance),
      command_catalog_file_sha256: fileSha(catalog), command_sync_authority_sha256: syncAuthority.report_sha256,
      command_sync_authority_file_sha256: fileSha(syncAuthority),
    });
    result.freshness.readback_sha256 = catalog.catalog_sha256;
    return result;
  };
  const observation = await observeProductionSurfaces({
    sourceCommit: SOURCE, durationSeconds: 1200, intervalSeconds: 1200,
    clock: fakeClock(observationStartedAt), probes, probeSpec: probeSpec(),
  });
  return {
    repository: REPOSITORY, "source-commit": SOURCE, "workflow-run-id": RUN_ID,
    "workflow-run-attempt": RUN_ATTEMPT, "accepted-run-id": ACCEPTED_RUN_ID,
    "accepted-run-attempt": ACCEPTED_RUN_ATTEMPT, version: CURRENT_PRODUCT_VERSION,
    "base-path": "/Clearra", "application-id": APPLICATION_ID,
    "catalog-artifact-id": "9900", "catalog-artifact-digest": `sha256:${HASH}`,
    "canonical-acceptance-evidence": await write("acceptance", acceptance),
    "recovery-debt-clearance": await write("clearance", clearance),
    "catalog-recovery-authority": await write("catalog-authority", catalogAuthority),
    "prior-catalog-snapshot": priorPath, "desired-catalog": catalogPath, "sync-authority": syncAuthorityPath,
    "sync-report": await write("sync-report", syncReport),
    "production-observation": await write("observation", observation), "job-list": await write("jobs", jobs),
  };
}

function probeSpec() {
  const probes = ["cloud", "discord", "oracle", "pages"].map((surface) => ({
    surface,
    runtime: surface === "oracle" ? "powershell" : "node",
    path: process.platform === "win32" ? `C:\\probes\\${surface}.mjs` : `/probes/${surface}.mjs`,
    sha256: HASH,
    arguments: ["--source-commit", SOURCE],
    timeout_seconds: 15,
  }));
  probes[2].arguments = [
    "-Operation", "observe-candidate", "-ScriptReleaseId", `${CURRENT_PRODUCT_TAG}-1111111`,
    "-ScriptReleaseSha256", "d".repeat(64), "-SourceCommit", SOURCE,
    "-CandidateUrl", "https://v080---clearra-current-job.example.run.app/",
    "-CandidateRevision", "clearra-current-job-v080-1111111",
    "-OracleReleaseId", `${CURRENT_PRODUCT_TAG}-1111111`, "-OracleReleaseSha256", "d".repeat(64),
    "-OracleSettingsSha256", "e".repeat(64), "-DeploymentNonce", "9".repeat(64),
    "-VerifiedAfter", "2026-08-30T23:59:59.000Z",
  ];
  return {
    schema_id: "clearra.production-observation-probe-spec.v1",
    source_commit: SOURCE,
    interval_seconds: 1200,
    probes,
  };
}

function probeSet(observationStartedAt, syncReport) {
  const identities = {
    discord: {
      source_commit: SOURCE, application_id: APPLICATION_ID,
      command_catalog_sha256: syncReport.expected_catalog_sha256,
      command_catalog_prior_snapshot_sha256: "a".repeat(64),
      command_catalog_readback_sha256: "b".repeat(64),
      command_catalog_sync_report_sha256: syncReport.report_sha256,
      accepted_run_id: ACCEPTED_RUN_ID, accepted_run_attempt: ACCEPTED_RUN_ATTEMPT,
      accepted_ctk3_manifest_sha256: "1".repeat(64),
      canonical_acceptance_evidence_sha256: "2".repeat(64),
      canonical_acceptance_evidence_file_sha256: "3".repeat(64),
      command_catalog_file_sha256: "4".repeat(64),
      command_sync_authority_sha256: "5".repeat(64),
      command_sync_authority_file_sha256: "6".repeat(64),
      command_count: 2, command_names: ["1:help", "3:Get original GIF"], status: "active",
    },
    oracle: {
      source_commit: SOURCE, release_id: `${CURRENT_PRODUCT_TAG}-1111111`,
      release_tree_sha256: "d".repeat(64), settings_sha256: "e".repeat(64),
      candidate_revision: "clearra-current-job-v080-1111111",
      candidate_url: "https://v080---clearra-current-job.example.run.app/",
      job_url: "https://v080---clearra-current-job.example.run.app/jobs",
      deployment_nonce: "9".repeat(64), gateway_pid: 1234,
      gateway_start_monotonic_usec: 123456789,
      boot_id: "12345678-1234-1234-1234-123456789abc",
      ready_record_observed: true, verified_after: "2026-08-30T23:59:59.000Z",
      status: "active",
    },
    cloud: {
      source_commit: SOURCE, engine_build_id: SOURCE,
      revision: "clearra-current-job-v080-1111111", image_digest: `sha256:${"f".repeat(64)}`,
      traffic_percent: 100, cpu: "8", memory: "16Gi", concurrency: 1,
      min_instances: 0, max_instances: 4, startup_cpu_boost: true,
      contract_schema_version: "clearra.search.contract.v2",
      supply_semantics_id: "clearra.supply.projected-terminal-lookahead.v1",
      artifact_schema_version: "clearra.solution-data.v1",
      job_smoke_report_sha256: "7".repeat(64),
      stable_url: "https://clearra-current-job.example.run.app/",
      tagged_url: "https://v080---clearra-current-job.example.run.app/", status: "active",
    },
    pages: {
      source_commit: SOURCE, engine_build_id: SOURCE, version: CURRENT_PRODUCT_VERSION,
      deployment_id: "pages-123", artifact_sha256: "9".repeat(64),
      base_path: "/Clearra", url: "https://daejunnom.github.io/Clearra/", status: "active",
    },
  };
  return Object.fromEntries(Object.entries(identities).map(([surface, identityValue]) => [
    surface,
    async ({ sequence }) => ({
      schema_id: PRODUCTION_SURFACE_PROBE_SCHEMA_ID,
      surface,
      source_commit: SOURCE,
      identity: structuredClone(identityValue),
      freshness: surface === "oracle"
        ? oracleFreshness(identityValue, sequence, observationStartedAt)
        : surface === "discord"
          ? { probe_id: probeId(sequence), readback_sha256: "b".repeat(64) }
          : surface === "cloud"
            ? {
              probe_id: probeId(sequence), service_readback_sha256: "1".repeat(64),
              revision_readback_sha256: "2".repeat(64), stable_health_sha256: "3".repeat(64),
              tagged_health_sha256: "4".repeat(64),
            }
            : {
              probe_id: probeId(sequence), deployment_readback_sha256: "6".repeat(64),
              identity_readback_sha256: "5".repeat(64),
            },
    }),
  ]));
}

function oracleFreshness(identityValue, sequence, observationStartedAt) {
  const observedAt = new Date(
    Date.parse(observationStartedAt) + sequence * 1_200_000,
  ).toISOString();
  return {
    operation_marker: canonicalSha256({
      contract: "clearra.oracle.candidate-observation.v1",
      source_commit: identityValue.source_commit,
      candidate_revision: identityValue.candidate_revision,
      verified_after: identityValue.verified_after,
      fresh_operation_at: observedAt,
      observed_at: observedAt,
    }),
    verified_after: identityValue.verified_after,
    fresh_operation_at: observedAt,
    observed_at: observedAt,
  };
}

function probeId(sequence) {
  return (sequence + 1).toString(16).padStart(64, "0");
}

function fakeClock(start) {
  let milliseconds = Date.parse(start);
  return {
    now: () => milliseconds,
    async wait(delay) { milliseconds += delay; },
  };
}

function fileSha(value) {
  return createHash("sha256").update(`${canonicalJson(value)}\n`, "utf8").digest("hex");
}

function reseal(value) {
  const { report_sha256: ignored, ...unsigned } = value;
  void ignored;
  return sealCanonicalReport(unsigned);
}
