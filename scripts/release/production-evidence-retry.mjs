import { createHash } from "node:crypto";
import { lstat, open, readFile, readdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { setTimeout as wait } from "node:timers/promises";
import { runBoundedCommand } from "./bounded-command.mjs";
import { canonicalJson, sealCanonicalReport, verifyCanonicalReportHash } from "./canonical-release-evidence.mjs";
import { verifyDiscordDeploymentState } from "./discord-deployment-state.mjs";
import { prepareDiscordProductionCheckpointInputs, validateDiscordProductionCheckpointCandidate } from "./discord-production-checkpoint-receipt.mjs";
import { createCommandProbes, verifyProductionObservationStillCurrent } from "./observe-production-surfaces.mjs";

const SYNC_BINDINGS = Object.freeze({
  discord_catalog: "sync/discord-catalog.json",
  discord_prior_catalog: "sync/discord-prior-catalog.json",
  discord_sync_authority: "sync/discord-sync-authority.json",
  discord_sync_report: "sync/discord-sync-report.json",
  oracle_end_observation: "sync/oracle-end-observation.json",
  pages_deployment_authority: "sync/pages-deployment-authority.json",
  production_observation: "sync/production-observation.json",
  production_probe_authority: "sync/production-probe-authority.json",
  production_probe_spec: "sync/production-probe-spec.json",
  promoted_state: "promoted/promotion/promoted-state.json",
});
const SYNC_LEAVES = new Set([
  ...Object.values(SYNC_BINDINGS).filter((path) => path.startsWith("sync/")).map((path) => path.slice(5)),
  "discord-catalog-recovery-authority.json", "synchronized-state.json",
  "discord-catalog-restore.json", "discord-catalog-compensation.json",
]);
const CHECKPOINT_LEAF = "discord-production-checkpoint-candidate.json";

export function checkpointInputsFromEnvironment(root, env = process.env) {
  const path = (value) => resolve(root, value);
  const digest = String(env.CATALOG_ARTIFACT_DIGEST ?? "").replace(/^sha256:/u, "");
  return {
    repository: env.GITHUB_REPOSITORY, sourceCommit: env.SOURCE_COMMIT,
    workflowRunId: env.GITHUB_RUN_ID, workflowRunAttempt: env.GITHUB_RUN_ATTEMPT,
    acceptedRunId: env.ACCEPTED_RUN_ID, acceptedRunAttempt: env.ACCEPTED_RUN_ATTEMPT,
    version: "0.8.0", basePath: "/Clearra", applicationId: env.DISCORD_APPLICATION_ID,
    catalogArtifactId: env.CATALOG_ARTIFACT_ID, catalogArtifactDigest: `sha256:${digest}`,
    canonicalAcceptanceEvidence: path("promoted/prepared/canonical-acceptance/clearra-canonical-acceptance-evidence.v1.json"),
    recoveryDebtClearance: path("promoted/prepared/recovery-debt/recovery-debt-clearance.json"),
    catalogRecoveryAuthority: path("sync/discord-catalog-recovery-authority.json"),
    priorCatalogSnapshot: path(SYNC_BINDINGS.discord_prior_catalog),
    desiredCatalog: path(SYNC_BINDINGS.discord_catalog),
    syncAuthority: path(SYNC_BINDINGS.discord_sync_authority),
    syncReport: path(SYNC_BINDINGS.discord_sync_report),
  };
}

export async function verifyEvidenceRetrySafety(root, kind, env = process.env) {
  if (!["sync", "checkpoint"].includes(kind)) throw new Error("invalid evidence retry kind");
  const options = checkpointInputsFromEnvironment(root, env);
  const prepared = await prepareDiscordProductionCheckpointInputs(options);
  const statePath = resolve(root, "sync/synchronized-state.json");
  const state = await readCanonical(statePath);
  await verifyDiscordDeploymentState(statePath, {
    ...options, stage: "synchronized", deploymentNonce: state.deployment_nonce,
    verifiedAfter: state.verified_after,
    bindings: Object.entries(SYNC_BINDINGS).map(([name, path]) => `${name}=${resolve(root, path)}`),
  });
  const spec = await readCanonical(resolve(root, SYNC_BINDINGS.production_probe_spec));
  const report = await readCanonical(resolve(root, SYNC_BINDINGS.production_observation));
  // A retry is a short continuation of this attempt, never permission to revive
  // a stale successful observation from an earlier deployment.
  const age = Date.now() - Date.parse(report.ended_at);
  if (!Number.isFinite(age) || age < 0 || age > 15 * 60_000) {
    throw new Error("evidence retry requires a recent completed observation");
  }
  if (kind === "checkpoint") {
    const candidate = await readCanonical(resolve(root, "checkpoint-candidate", CHECKPOINT_LEAF));
    validateDiscordProductionCheckpointCandidate(candidate, options);
    for (const [field, value] of Object.entries(prepared)) {
      if (canonicalJson(candidate[field]) !== canonicalJson(value)) {
        throw new Error("checkpoint retry inputs differ from its original preparation");
      }
    }
    if (candidate.production_observation_sha256 !== report.report_sha256) {
      throw new Error("checkpoint retry differs from the completed observation");
    }
  }
  const probes = await createCommandProbes(spec);
  const priorToken = process.env.DISCORD_TOKEN;
  try {
    if (!priorToken) {
      if (!/^[a-z][a-z0-9-]{4,61}[a-z0-9]$/u.test(env.GCP_PROJECT_ID ?? "")) {
        throw new Error("evidence retry Cloud project is invalid");
      }
      const bytes = await runBoundedCommand("gcloud", [
        "secrets", "versions", "access", "latest", "--secret=discord-bot-token",
        `--project=${env.GCP_PROJECT_ID}`,
      ], { timeoutMs: 15_000, maxBytes: 8_192, label: "Discord read credential access" });
      const token = bytes.toString("utf8").trim();
      if (!token || /[\r\n]/u.test(token)) throw new Error("Discord read credential is invalid");
      process.env.DISCORD_TOKEN = token;
    }
    await verifyProductionObservationStillCurrent({ report, spec, probes });
  } finally {
    if (priorToken === undefined) delete process.env.DISCORD_TOKEN;
    else process.env.DISCORD_TOKEN = priorToken;
  }
}

// Snapshot only the closed non-secret artifact leaves. The retry must upload
// the same bytes and name from the same source/run/attempt as the first try.
export async function snapshotEvidenceUpload(root, kind, name, env = process.env) {
  if (!["sync", "checkpoint"].includes(kind)) throw new Error("invalid evidence upload kind");
  const { sourceCommit, workflowRunId, workflowRunAttempt } = checkpointInputsFromEnvironment(root, env);
  if (!/^[0-9a-f]{40}$/u.test(sourceCommit ?? "") ||
      !/^[1-9][0-9]*$/u.test(workflowRunId ?? "") || !/^[1-9][0-9]*$/u.test(workflowRunAttempt ?? "")) {
    throw new Error("evidence upload identity is invalid");
  }
  const prefix = kind === "sync" ? "discord-sync-inputs" : "discord-production-checkpoint-candidate";
  if (name !== `${prefix}-${sourceCommit}-run-${workflowRunId}-attempt-${workflowRunAttempt}`) {
    throw new Error("evidence upload name differs from the active attempt");
  }
  const directory = resolve(root, kind === "sync" ? "sync" : "checkpoint-candidate");
  const leaves = (await readdir(directory)).sort();
  if (leaves.length === 0 || leaves.some((leaf) =>
    kind === "sync" ? !SYNC_LEAVES.has(leaf) : leaf !== CHECKPOINT_LEAF)) {
    throw new Error("evidence upload has missing or unexpected leaves");
  }
  const files = [];
  for (const leaf of leaves) {
    const bytes = await readRegular(resolve(directory, leaf));
    files.push({ leaf, size: bytes.length, sha256: createHash("sha256").update(bytes).digest("hex") });
  }
  return sealCanonicalReport({
    schema_id: "clearra.production-evidence-upload.v1", kind, name,
    source_commit: sourceCommit, workflow_run_id: workflowRunId,
    workflow_run_attempt: workflowRunAttempt, files,
  });
}

// Validation errors are never retried. Only a small set of transient local I/O
// errors may repeat sealing, and only after live state is revalidated.
export async function retryEvidenceWrite(operation, revalidate, {
  waitForRetry = wait, onRetry = () => {},
} = {}) {
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    try { return await operation(); }
    catch (error) {
      if (attempt === 3 || !new Set(["EIO", "EBUSY", "EAGAIN", "EINTR", "ETIMEDOUT"]).has(error?.code)) throw error;
      await waitForRetry(attempt * 2_000);
      await revalidate();
      onRetry(attempt);
    }
  }
}

async function readRegular(path) {
  for (let directory = dirname(path);; directory = dirname(directory)) {
    const metadata = await lstat(directory);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) throw new Error("evidence path contains a link");
    if (directory === dirname(directory)) break;
  }
  const metadata = await lstat(path);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size < 1 || metadata.size > 20 * 1024 * 1024) {
    throw new Error("evidence must be a bounded regular file");
  }
  return readFile(path);
}

async function readCanonical(path) {
  const bytes = await readRegular(path);
  const value = JSON.parse(bytes.toString("utf8"));
  if (bytes.toString("utf8") !== `${canonicalJson(value)}\n`) throw new Error("evidence bytes are not canonical");
  return value;
}

async function main() {
  const { values, positionals } = parseArgs({
    options: { kind: { type: "string" }, name: { type: "string" }, manifest: { type: "string" } },
    allowPositionals: true, strict: true,
  });
  const root = resolve(process.env.GITHUB_WORKSPACE ?? ".");
  const operation = positionals[0];
  if (positionals.length !== 1 || !["prepare", "retry", "verify-inputs"].includes(operation)) {
    throw new Error("invalid evidence operation");
  }
  if (operation === "verify-inputs") {
    await prepareDiscordProductionCheckpointInputs(checkpointInputsFromEnvironment(root));
    process.stdout.write("discord_checkpoint_inputs=verified-before-observation\n");
    return;
  }
  const snapshot = await snapshotEvidenceUpload(root, values.kind, values.name);
  if (operation === "prepare") {
    const handle = await open(resolve(values.manifest), "wx", 0o600);
    try { await handle.writeFile(`${canonicalJson(snapshot)}\n`); await handle.sync(); }
    finally { await handle.close(); }
  } else {
    const original = await readCanonical(resolve(values.manifest));
    verifyCanonicalReportHash(original, "original evidence upload");
    if (canonicalJson(original) !== canonicalJson(snapshot)) throw new Error("evidence changed before upload retry");
    await verifyEvidenceRetrySafety(root, values.kind);
    process.stdout.write("evidence_retry=authorized-after-live-readback release_status=pending\n");
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try { await main(); }
  catch (error) {
    process.stderr.write(`evidence_retry=refused reason=${error.message}\n`);
    process.exitCode = 2;
  }
}
