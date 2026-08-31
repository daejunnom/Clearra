#!/usr/bin/env node

import { createHash } from "node:crypto";
import { lstat, open, readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  canonicalJson,
  canonicalTimestamp,
  requireExactKeys,
  requireSha256,
  requireSourceCommit,
  verifyCanonicalReportHash,
} from "./canonical-release-evidence.mjs";
import {
  validateCanonicalDiscordCatalog,
  validateDiscordCatalogSyncReport,
} from "../../apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs";
import { validateCloudCandidateSmokeReport } from "./cloud-candidate-smoke-report.mjs";
import { readDiscordCommandSyncAuthority } from "./discord-command-sync-authority.mjs";
import {
  PRODUCTION_PROBE_AUTHORITY_SCHEMA_ID,
  validateProductionProbeAuthority,
} from "./materialize-production-probe-spec.mjs";
import { PRODUCTION_OBSERVATION_SECONDS } from "./observe-production-surfaces.mjs";
import { validatePagesDeploymentAuthorityReport } from "./pages-deployment-authority.mjs";
import { candidateSettingsAuthorityV080 } from "./oracle/candidate-settings-v080.mjs";
import { parseCanonicalManifest } from "./oracle/create-inactive-stage-v080.mjs";

const APPLICATION_ID = /^\d{17,20}$/u;
const PROJECT_ID = /^[a-z][a-z0-9-]{4,61}[a-z0-9]$/u;
const FULL_IMAGE_DIGEST =
  /^(asia-northeast1-docker\.pkg\.dev\/[a-z][a-z0-9-]{4,61}[a-z0-9]\/clearra\/clearra-current-job)@sha256:([0-9a-f]{64})$/u;
const STATE_SCHEMA = "clearra.discord-deployment-state.v1";
const CANDIDATE_CONTRACT = "clearra.cloud.zero-traffic-candidate.v1";
const ORACLE_ATTESTATION = Buffer.from("oracle_candidate=verified\n", "utf8");

export async function createProductionProbeAuthorityV080(options) {
  const sourceCommit = requireSourceCommit(
    options?.sourceCommit,
    "production probe source commit",
  );
  const applicationId = requirePattern(
    options?.applicationId,
    APPLICATION_ID,
    "Discord application ID",
  );
  const projectId = requirePattern(options?.projectId, PROJECT_ID, "GCP project ID");

  const pages = await readCanonicalEvidence(
    options?.pagesDeploymentReport,
    "Pages deployment authority",
  );
  validatePagesDeploymentAuthorityReport(pages.value, {
    expectedSourceCommit: sourceCommit,
  });
  if (pages.value.mode !== "forward") {
    throw new Error("production probe requires a forward Pages deployment authority");
  }

  const catalog = await readCanonicalEvidence(
    options?.discordCatalog,
    "Discord canonical catalog",
  );
  validateCanonicalDiscordCatalog(catalog.value, sourceCommit);
  const syncAuthorityPath = resolveRequiredPath(
    options?.discordSyncAuthority,
    "Discord sync authority",
  );
  const syncAuthorityFile = await readRegularFile(
    syncAuthorityPath,
    "Discord sync authority",
  );
  const syncAuthority = await readDiscordCommandSyncAuthority(
    syncAuthorityPath,
    syncAuthorityFile.sha256,
    {
      sourceCommit,
      catalog: catalog.value,
      catalogFileSha256: catalog.sha256,
    },
  );
  const syncReport = await readCanonicalEvidence(
    options?.discordSyncReport,
    "Discord sync report",
  );
  validateDiscordCatalogSyncReport(syncReport.value, {
    expectedSourceCommit: sourceCommit,
    expectedApplicationId: applicationId,
    expectedCatalog: catalog.value,
    expectedCatalogFileSha256: catalog.sha256,
    expectedSyncAuthority: syncAuthority.authority,
    expectedSyncAuthorityFileSha256: syncAuthority.fileSha256,
  });

  const cloudCandidate = await readJsonEvidence(
    options?.cloudCandidateAuthority,
    "Cloud candidate authority",
  );
  validateCloudCandidateAuthority(cloudCandidate.value, { sourceCommit, projectId });
  const cloudSmoke = await readCanonicalEvidence(
    options?.cloudSmokeReport,
    "Cloud candidate smoke report",
  );
  validateCloudCandidateSmokeReport(cloudSmoke.value, {
    expectedSourceCommit: sourceCommit,
  });
  validateCloudCrossBinding(cloudCandidate.value, cloudSmoke.value);

  const oracleManifestFile = await readRegularFile(
    options?.oracleManifest,
    "Oracle inactive-stage manifest",
  );
  const oracleManifest = parseCanonicalManifest(oracleManifestFile.bytes);
  if (oracleManifest.sourceCommit !== sourceCommit) {
    throw new Error("Oracle inactive-stage manifest source differs");
  }
  const oracleAttestation = await readRegularFile(
    options?.oracleAttestation,
    "Oracle candidate attestation",
  );
  if (!oracleAttestation.bytes.equals(ORACLE_ATTESTATION)) {
    throw new Error("Oracle candidate attestation is not the exact success marker");
  }
  const oracleAdapter = await readRegularFile(
    options?.oracleAdapter,
    "tracked Oracle probe adapter",
  );

  const candidateState = await readCanonicalEvidence(
    options?.candidateState,
    "Discord candidate deployment state",
  );
  const promotedState = await readCanonicalEvidence(
    options?.promotedState,
    "Discord promoted deployment state",
  );
  validateState(candidateState.value, "candidate", sourceCommit);
  validateState(promotedState.value, "promoted", sourceCommit);
  if (
    promotedState.value.deployment_nonce !== candidateState.value.deployment_nonce ||
    promotedState.value.parent_report_sha256 !== candidateState.sha256 ||
    promotedState.value.accepted_run_id !== candidateState.value.accepted_run_id ||
    promotedState.value.accepted_run_attempt !== candidateState.value.accepted_run_attempt
  ) {
    throw new Error("promoted deployment state differs from its candidate state authority");
  }
  requireBoundFile(candidateState.value, "cloud_candidate_authority", cloudCandidate);
  requireBoundFile(candidateState.value, "cloud_candidate_smoke", cloudSmoke);
  requireBoundFile(candidateState.value, "oracle_stage_manifest", oracleManifestFile);
  requireBoundFile(promotedState.value, "candidate_state", candidateState);
  requireBoundFile(promotedState.value, "oracle_candidate_attestation", oracleAttestation);

  const verifiedAfter = canonicalTimestamp(
    promotedState.value.verified_after,
    "promoted Oracle verified-after time",
  );
  const settings = candidateSettingsAuthorityV080({
    sourceCommit,
    candidateUrl: cloudCandidate.value.candidateUrl,
  });
  const authority = {
    schema_id: PRODUCTION_PROBE_AUTHORITY_SCHEMA_ID,
    source_commit: sourceCommit,
    interval_seconds: PRODUCTION_OBSERVATION_SECONDS,
    discord: {
      application_id: applicationId,
      catalog_path: catalog.path,
      catalog_file_sha256: catalog.sha256,
      sync_authority_path: syncAuthorityPath,
      sync_authority_file_sha256: syncAuthority.fileSha256,
      sync_report_path: syncReport.path,
      sync_report_file_sha256: syncReport.sha256,
      timeout_seconds: 60,
    },
    cloud: {
      project_id: projectId,
      region: cloudSmoke.value.region,
      service_name: cloudSmoke.value.service_name,
      revision: cloudSmoke.value.candidate_revision,
      tag: cloudSmoke.value.candidate_tag,
      image_digest: cloudSmoke.value.image_digest,
      smoke_report_path: cloudSmoke.path,
      smoke_report_file_sha256: cloudSmoke.sha256,
      timeout_seconds: 60,
    },
    oracle: {
      adapter_path: oracleAdapter.path,
      adapter_sha256: oracleAdapter.sha256,
      script_release_id: oracleManifest.releaseId,
      script_release_sha256: oracleManifest.candidate.treeSha256,
      candidate_url: cloudCandidate.value.candidateUrl,
      candidate_revision: cloudCandidate.value.candidateRevision,
      oracle_release_id: oracleManifest.releaseId,
      oracle_release_sha256: oracleManifest.candidate.treeSha256,
      oracle_settings_sha256: settings.sha256,
      deployment_nonce: promotedState.value.deployment_nonce,
      verified_after: verifiedAfter,
      timeout_seconds: 60,
    },
    pages: {
      deployment_report_path: pages.path,
      deployment_report_file_sha256: pages.sha256,
      timeout_seconds: 60,
    },
  };
  validateProductionProbeAuthority(authority);
  return Object.freeze(authority);
}

export async function writeProductionProbeAuthorityV080(path, authority) {
  validateProductionProbeAuthority(authority);
  const target = resolveRequiredPath(path, "production probe authority output");
  await assertSafeDirectoryChain(dirname(target));
  const handle = await open(target, "wx", 0o600);
  try {
    await handle.writeFile(`${canonicalJson(authority)}\n`, "utf8");
  } finally {
    await handle.close();
  }
}

function validateCloudCandidateAuthority(value, { sourceCommit, projectId }) {
  requireExactKeys(value, [
    "contract",
    "sourceCommit",
    "projectId",
    "region",
    "service",
    "priorRevision",
    "candidateRevision",
    "candidateTag",
    "candidateUrl",
    "imageDigest",
    "jobBearerSecret",
    "jobBearerSecretVersion",
  ], "Cloud zero-traffic candidate authority");
  if (
    value.contract !== CANDIDATE_CONTRACT ||
    value.sourceCommit !== sourceCommit ||
    value.projectId !== projectId ||
    value.region !== "asia-northeast1" ||
    value.service !== "clearra-current-job" ||
    value.candidateRevision !== `clearra-current-job-v080-${sourceCommit.slice(0, 7)}` ||
    value.candidateTag !== `candidate-${sourceCommit.slice(0, 7)}` ||
    value.jobBearerSecret !== "clearra-job-token" ||
    !/^[1-9][0-9]{0,18}$/u.test(value.jobBearerSecretVersion ?? "")
  ) {
    throw new Error("Cloud candidate authority differs from the exact v0.8 source");
  }
  const digest = FULL_IMAGE_DIGEST.exec(value.imageDigest ?? "");
  if (!digest || !digest[1].includes(`/${projectId}/`)) {
    throw new Error("Cloud candidate authority image is not an immutable project digest");
  }
  canonicalCredentialFreeOrigin(value.candidateUrl, "Cloud candidate URL");
  requirePattern(
    value.priorRevision,
    /^clearra-current-job-v(?:075|080)-[0-9a-f]{7}$/u,
    "Cloud prior revision",
  );
}

function validateCloudCrossBinding(candidate, smoke) {
  const fullDigest = FULL_IMAGE_DIGEST.exec(candidate.imageDigest);
  if (
    smoke.project_id !== candidate.projectId ||
    smoke.region !== candidate.region ||
    smoke.service_name !== candidate.service ||
    smoke.candidate_revision !== candidate.candidateRevision ||
    smoke.candidate_tag !== candidate.candidateTag ||
    smoke.candidate_url !== candidate.candidateUrl ||
    smoke.image_digest !== `sha256:${fullDigest[2]}`
  ) {
    throw new Error("Cloud smoke report differs from its zero-traffic candidate authority");
  }
}

function validateState(value, stage, sourceCommit) {
  requireExactKeys(value, [
    "schema_id",
    "stage",
    "source_commit",
    "workflow_run_id",
    "workflow_run_attempt",
    "accepted_run_id",
    "accepted_run_attempt",
    "deployment_nonce",
    "verified_after",
    "parent_report_sha256",
    "bindings",
    "report_sha256",
  ], `${stage} Discord deployment state`);
  verifyCanonicalReportHash(value, `${stage} Discord deployment state`);
  if (
    value.schema_id !== STATE_SCHEMA ||
    value.stage !== stage ||
    value.source_commit !== sourceCommit ||
    value.workflow_run_attempt !== "1" ||
    !/^[1-9][0-9]*$/u.test(value.workflow_run_id ?? "") ||
    !/^[1-9][0-9]*$/u.test(value.accepted_run_id ?? "") ||
    !/^[1-9][0-9]*$/u.test(value.accepted_run_attempt ?? "") ||
    !/^[0-9a-f]{64}$/u.test(value.deployment_nonce ?? "") ||
    !Array.isArray(value.bindings)
  ) {
    throw new Error(`${stage} Discord deployment state identity is invalid`);
  }
}

function requireBoundFile(state, name, evidence) {
  const matches = state.bindings.filter((binding) => binding?.name === name);
  if (
    matches.length !== 1 ||
    matches[0].file_sha256 !== evidence.sha256 ||
    matches[0].size !== evidence.size
  ) {
    throw new Error(`Discord deployment state binding differs for ${name}`);
  }
}

async function readCanonicalEvidence(path, label) {
  const input = await readRegularFile(path, label);
  let value;
  try {
    value = JSON.parse(input.bytes.toString("utf8"));
  } catch {
    throw new Error(`${label} is not valid JSON`);
  }
  if (!input.bytes.equals(Buffer.from(`${canonicalJson(value)}\n`, "utf8"))) {
    throw new Error(`${label} bytes are not canonical JSON`);
  }
  return Object.freeze({ ...input, value });
}

async function readJsonEvidence(path, label) {
  const input = await readRegularFile(path, label);
  let value;
  try {
    value = JSON.parse(input.bytes.toString("utf8"));
  } catch {
    throw new Error(`${label} is not valid JSON`);
  }
  return Object.freeze({ ...input, value });
}

async function readRegularFile(path, label) {
  const target = resolveRequiredPath(path, `${label} path`);
  await assertSafeDirectoryChain(dirname(target));
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size < 1) {
    throw new Error(`${label} must be a nonempty regular non-link file`);
  }
  const bytes = await readFile(target);
  return Object.freeze({
    path: target,
    bytes,
    size: metadata.size,
    sha256: createHash("sha256").update(bytes).digest("hex"),
  });
}

async function assertSafeDirectoryChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const metadata = await lstat(current);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error("production probe evidence path uses a link or non-directory");
    }
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
}

function canonicalCredentialFreeOrigin(value, label) {
  let parsed;
  try {
    parsed = new URL(String(value ?? ""));
  } catch {
    throw new Error(`${label} is invalid`);
  }
  if (
    parsed.protocol !== "https:" ||
    parsed.username ||
    parsed.password ||
    parsed.pathname !== "/" ||
    parsed.search ||
    parsed.hash ||
    parsed.origin !== value
  ) {
    throw new Error(`${label} must be a canonical credential-free HTTPS origin`);
  }
  return value;
}

function resolveRequiredPath(value, label) {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} is required`);
  }
  return resolve(value);
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new Error(`${label} is invalid`);
  }
  return value;
}

function parseCli(arguments_) {
  const names = [
    "source-commit",
    "application-id",
    "project-id",
    "pages-deployment-report",
    "discord-catalog",
    "discord-sync-authority",
    "discord-sync-report",
    "cloud-candidate-authority",
    "cloud-smoke-report",
    "oracle-manifest",
    "oracle-attestation",
    "oracle-adapter",
    "candidate-state",
    "promoted-state",
    "output",
  ];
  const options = Object.fromEntries(names.map((name) => [name, { type: "string" }]));
  const { values } = parseArgs({ args: arguments_, options, strict: true });
  for (const name of names) {
    if (typeof values[name] !== "string" || values[name].length === 0) {
      throw new Error(`--${name} is required`);
    }
  }
  return values;
}

async function main() {
  try {
    const values = parseCli(process.argv.slice(2));
    const authority = await createProductionProbeAuthorityV080({
      sourceCommit: values["source-commit"],
      applicationId: values["application-id"],
      projectId: values["project-id"],
      pagesDeploymentReport: values["pages-deployment-report"],
      discordCatalog: values["discord-catalog"],
      discordSyncAuthority: values["discord-sync-authority"],
      discordSyncReport: values["discord-sync-report"],
      cloudCandidateAuthority: values["cloud-candidate-authority"],
      cloudSmokeReport: values["cloud-smoke-report"],
      oracleManifest: values["oracle-manifest"],
      oracleAttestation: values["oracle-attestation"],
      oracleAdapter: values["oracle-adapter"],
      candidateState: values["candidate-state"],
      promotedState: values["promoted-state"],
    });
    await writeProductionProbeAuthorityV080(values.output, authority);
    process.stdout.write(
      `production_probe_authority=sealed source=${authority.source_commit}\n`,
    );
  } catch (error) {
    process.stderr.write(
      `production_probe_authority=failed reason=${
        error instanceof Error ? error.message : String(error)
      }\n`,
    );
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}
