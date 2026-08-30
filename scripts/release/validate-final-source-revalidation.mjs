import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  validateDiscordCatalogSyncReport,
} from "../../apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs";
import {
  canonicalJson,
} from "./canonical-release-evidence.mjs";
import {
  validateProductionObservationReport,
} from "./observe-production-surfaces.mjs";
import {
  validateFinalSourceStageEvidence,
} from "./final-source-stage-evidence.mjs";
import {
  FINAL_SOURCE_STAGE_ORDER,
} from "./final-source-event-contract.mjs";

export const FINAL_SOURCE_SCHEMA_ID = "clearra.final-source-revalidation.v1";

const SHA1 = /^[0-9a-f]{40}$/u;
const GIT_OBJECT_ID = /^(?:[0-9a-f]{40}|[0-9a-f]{64})$/u;
const SHA256 = /^[0-9a-f]{64}$/u;
const IMAGE_DIGEST = /^sha256:[0-9a-f]{64}$/u;
const DECIMAL_ID = /^[1-9]\d*$/u;
const SECRET_KEY = /(?:^|_)(?:secret|token|password|credential|api_key|private_key)(?:_|$)/iu;
const FORBIDDEN_PRIOR_AUTHORITY = /(?:^|[^0-9])v?0\.7\.5(?:[^0-9]|$)/iu;
const REQUIRED_SURFACES = Object.freeze(["desktop", "discord", "native", "wasm"]);
const REQUIRED_ARTIFACT_ROLES = Object.freeze(["linux-cli", "windows-cli", "windows-gui"]);

export function validateFinalSourceRevalidation(
  manifest,
  {
    expectedSourceCommit,
    expectedRelease = "v0.8.0",
    discordCatalogSyncReport,
    productionObservationReport,
  } = {},
) {
  requirePlainObject(manifest, "manifest");
  rejectSecretMaterial(manifest);
  rejectPriorReleaseAuthority(manifest);
  requireExactKeys(manifest, [
    "schema_id",
    "release",
    "source",
    "contracts",
    "toolchains",
    "drift_audits",
    "canonical_gate",
    "surface_reports",
    "release_artifacts",
    "deployment",
    "observation",
    "tag",
    "immutable_release",
  ], "manifest");
  if (manifest.schema_id !== FINAL_SOURCE_SCHEMA_ID) {
    throw new Error(`unexpected final-source schema: ${String(manifest.schema_id)}`);
  }
  if (manifest.release !== expectedRelease) {
    throw new Error(`final-source release must be ${expectedRelease}`);
  }

  const source = validateSource(manifest.source, expectedSourceCommit);
  validateContracts(manifest.contracts, source.commit);
  validateToolchains(manifest.toolchains, source.commit);
  validateDriftAudits(manifest.drift_audits, source.commit);
  validateEvidenceRef(manifest.canonical_gate, "canonical_gate", source.commit, {
    exactKeys: ["id", "sha256", "source_commit", "status", "readiness_open_count"],
  });
  if (manifest.canonical_gate.status !== "passed") {
    throw new Error("canonical release gate did not pass");
  }
  if (manifest.canonical_gate.readiness_open_count !== 0) {
    throw new Error("canonical release gate must record readiness_open_count=0");
  }
  validateSurfaceReports(manifest.surface_reports, source.commit);
  validateReleaseArtifacts(manifest.release_artifacts, source.commit, expectedRelease);
  validateDeployment(manifest.deployment, source.commit);
  validateObservation(manifest.observation, source.commit);
  validateProducerEvidence(
    manifest,
    discordCatalogSyncReport,
    productionObservationReport,
    source.commit,
  );
  validateTag(manifest.tag, source.commit, expectedRelease);
  validateImmutableRelease(manifest.immutable_release, source.commit, expectedRelease);
  return true;
}

export function validateFinalSourceRevalidationFromStages(
  manifest,
  {
    expectedSourceCommit,
    expectedRelease = "v0.8.0",
    acceptanceStageEvidence,
    acceptanceStageEvidenceFileSha256,
    deploymentStageEvidence,
    deploymentStageEvidenceFileSha256,
    publicationStageEvidence,
    publicationStageEvidenceFileSha256,
    discordCatalogSyncReport,
    discordCatalogSyncReportFileSha256,
    productionObservationReport,
    productionObservationReportFileSha256,
  } = {},
) {
  const stages = [
    ["acceptance", acceptanceStageEvidence, acceptanceStageEvidenceFileSha256],
    ["deployment", deploymentStageEvidence, deploymentStageEvidenceFileSha256],
    ["publication", publicationStageEvidence, publicationStageEvidenceFileSha256],
  ];
  for (const [index, [stage, value, fileSha256]] of stages.entries()) {
    if (stage !== FINAL_SOURCE_STAGE_ORDER[index]) {
      throw new Error("final-source library stage order differs from the contract");
    }
    validateFinalSourceStageEvidence(value, {
      expectedStage: stage,
      expectedSourceCommit,
    });
    assertMatch(fileSha256, SHA256, `${stage} stage raw file SHA-256`);
  }
  assertMatch(
    discordCatalogSyncReportFileSha256,
    SHA256,
    "Discord sync raw file SHA-256",
  );
  assertMatch(
    productionObservationReportFileSha256,
    SHA256,
    "production observation raw file SHA-256",
  );
  requireStageProducerBinding(
    deploymentStageEvidence,
    "discord-catalog-sync",
    discordCatalogSyncReport?.report_sha256,
    discordCatalogSyncReportFileSha256,
  );
  requireStageProducerBinding(
    deploymentStageEvidence,
    "production-observation",
    productionObservationReport?.report_sha256,
    productionObservationReportFileSha256,
  );
  const projected = projectManifestFromStages(
    stages.map(([, value]) => value),
    expectedRelease,
  );
  if (canonicalJson(projected) !== canonicalJson(manifest)) {
    throw new Error("final-source manifest differs from the exact three producer stages");
  }
  return validateFinalSourceRevalidation(manifest, {
    expectedSourceCommit,
    expectedRelease,
    discordCatalogSyncReport,
    productionObservationReport,
  });
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  process.stderr.write(
    "final-source revalidation is library-only; use final-source-attempt-journal.mjs materialize with every original producer authority\n",
  );
  process.exitCode = 2;
}

function requireStageProducerBinding(stage, role, evidenceSha256, fileSha256) {
  const matches = stage.producer_inputs.filter((input) => input.role === role);
  if (
    matches.length !== 1 ||
    matches[0].evidence_sha256 !== evidenceSha256 ||
    matches[0].file_sha256 !== fileSha256
  ) {
    throw new Error(`${role} producer bytes differ from the deployment stage authority`);
  }
}

function projectManifestFromStages(stages, release) {
  const grouped = new Map();
  for (const stage of stages) {
    for (const event of stage.events) {
      const entries = grouped.get(event.kind) ?? [];
      entries.push(event.payload);
      grouped.set(event.kind, entries);
    }
  }
  const only = (kind) => {
    const values = grouped.get(kind) ?? [];
    if (values.length !== 1) {
      throw new Error(`${kind} stage projection must resolve exactly once`);
    }
    return values[0];
  };
  const sorted = (kind, key) => [...(grouped.get(kind) ?? [])]
    .sort((left, right) => left[key].localeCompare(right[key], "en"));
  return {
    schema_id: FINAL_SOURCE_SCHEMA_ID,
    release,
    source: only("source"),
    contracts: only("contracts"),
    toolchains: only("toolchains"),
    drift_audits: sorted("drift-audit", "phase"),
    canonical_gate: only("canonical-gate"),
    surface_reports: sorted("surface-report", "surface"),
    release_artifacts: sorted("release-artifact", "role"),
    deployment: {
      pages: only("deployment-pages"),
      discord: only("deployment-discord"),
      rollback_snapshot: only("rollback-snapshot"),
    },
    observation: only("observation"),
    tag: only("tag"),
    immutable_release: only("immutable-release"),
  };
}

function validateSource(source, expectedSourceCommit) {
  requirePlainObject(source, "source");
  requireExactKeys(source, [
    "commit",
    "tree",
    "branch",
    "worktree_clean",
    "engine_build_id",
  ], "source");
  assertMatch(source.commit, SHA1, "source.commit");
  assertMatch(source.tree, GIT_OBJECT_ID, "source.tree");
  if (expectedSourceCommit !== undefined && source.commit !== expectedSourceCommit) {
    throw new Error("final-source commit differs from the expected candidate commit");
  }
  if (source.branch !== "main") throw new Error("final source must be accepted on main");
  if (source.worktree_clean !== true) throw new Error("final source worktree must be clean");
  if (source.engine_build_id !== source.commit) {
    throw new Error("engine build identity differs from the final source commit");
  }
  return source;
}

function validateContracts(contracts, sourceCommit) {
  requirePlainObject(contracts, "contracts");
  requireExactKeys(contracts, [
    "source_commit",
    "product_registry_schema_id",
    "product_registry_sha256",
    "search_option_contract_sha256",
    "legacy_alias_contract_sha256",
    "ctk3_contract_sha256",
    "readiness_open_count",
  ], "contracts");
  assertSameCommit(contracts.source_commit, sourceCommit, "contracts");
  requireNonEmptyString(contracts.product_registry_schema_id, "contracts.product_registry_schema_id");
  for (const key of [
    "product_registry_sha256",
    "search_option_contract_sha256",
    "legacy_alias_contract_sha256",
    "ctk3_contract_sha256",
  ]) assertMatch(contracts[key], SHA256, `contracts.${key}`);
  if (contracts.readiness_open_count !== 0) {
    throw new Error("final contract registry must have zero readiness entries");
  }
}

function validateToolchains(toolchains, sourceCommit) {
  requirePlainObject(toolchains, "toolchains");
  requireExactKeys(toolchains, [
    "source_commit",
    "manifest_sha256",
    "rust",
    "node",
    "wasm_bindgen",
  ], "toolchains");
  assertSameCommit(toolchains.source_commit, sourceCommit, "toolchains");
  assertMatch(toolchains.manifest_sha256, SHA256, "toolchains.manifest_sha256");
  for (const key of ["rust", "node", "wasm_bindgen"]) {
    requireNonEmptyString(toolchains[key], `toolchains.${key}`);
  }
}

function validateDriftAudits(audits, sourceCommit) {
  if (!Array.isArray(audits) || audits.length !== 2) {
    throw new Error("drift_audits must contain implementation-start and release-freeze");
  }
  const phases = [];
  for (const audit of audits) {
    validateEvidenceRef(audit, "drift audit", sourceCommit, {
      exactKeys: ["id", "sha256", "source_commit", "phase", "status"],
    });
    if (!new Set(["implementation-start", "release-freeze"]).has(audit.phase)) {
      throw new Error(`unexpected drift audit phase: ${String(audit.phase)}`);
    }
    if (audit.status !== "no-drift") {
      throw new Error(`${audit.phase} drift audit did not close with no-drift`);
    }
    phases.push(audit.phase);
  }
  assertSortedIdentity(phases, ["implementation-start", "release-freeze"], "drift audit phases");
}

function validateSurfaceReports(reports, sourceCommit) {
  if (!Array.isArray(reports) || reports.length !== REQUIRED_SURFACES.length) {
    throw new Error("surface_reports must contain native, wasm, desktop, and discord exactly once");
  }
  const surfaces = [];
  for (const report of reports) {
    validateEvidenceRef(report, "surface report", sourceCommit, {
      exactKeys: ["id", "sha256", "source_commit", "surface", "status"],
    });
    if (!REQUIRED_SURFACES.includes(report.surface)) {
      throw new Error(`unexpected surface report: ${String(report.surface)}`);
    }
    if (report.status !== "passed") {
      throw new Error(`${report.surface} surface report did not pass`);
    }
    surfaces.push(report.surface);
  }
  assertSortedIdentity(surfaces, REQUIRED_SURFACES, "surface reports");
}

function validateReleaseArtifacts(artifacts, sourceCommit, release) {
  if (!Array.isArray(artifacts) || artifacts.length !== REQUIRED_ARTIFACT_ROLES.length) {
    throw new Error("release_artifacts must contain exactly three product artifacts");
  }
  const version = release.slice(1);
  const expectedNames = new Map([
    ["linux-cli", `Clearra-CLI-v${version}-linux-x86_64`],
    ["windows-cli", `Clearra-CLI-v${version}-windows-x86_64.exe`],
    ["windows-gui", `Clearra-GUI-v${version}-windows-x86_64.exe`],
  ]);
  const roles = [];
  const names = [];
  for (const artifact of artifacts) {
    requirePlainObject(artifact, "release artifact");
    requireExactKeys(artifact, [
      "role",
      "name",
      "sha256",
      "size_bytes",
      "source_commit",
    ], "release artifact");
    if (!REQUIRED_ARTIFACT_ROLES.includes(artifact.role)) {
      throw new Error(`unexpected release artifact role: ${String(artifact.role)}`);
    }
    requireNonEmptyString(artifact.name, "release artifact name");
    if (artifact.name !== expectedNames.get(artifact.role)) {
      throw new Error(`${artifact.role} artifact name differs from the canonical release asset`);
    }
    assertMatch(artifact.sha256, SHA256, `${artifact.role} sha256`);
    assertPositiveSafeInteger(artifact.size_bytes, `${artifact.role} size_bytes`);
    assertSameCommit(artifact.source_commit, sourceCommit, `${artifact.role} artifact`);
    roles.push(artifact.role);
    names.push(artifact.name);
  }
  assertSortedIdentity(roles, REQUIRED_ARTIFACT_ROLES, "release artifact roles");
  if (new Set(names).size !== names.length) {
    throw new Error("release artifact names must be unique");
  }
}

function validateDeployment(deployment, sourceCommit) {
  requirePlainObject(deployment, "deployment");
  requireExactKeys(deployment, ["pages", "discord", "rollback_snapshot"], "deployment");

  const pages = deployment.pages;
  requirePlainObject(pages, "deployment.pages");
  requireExactKeys(pages, [
    "source_commit",
    "deployment_id",
    "artifact_sha256",
    "status",
  ], "deployment.pages");
  assertSameCommit(pages.source_commit, sourceCommit, "Pages deployment");
  requireNonEmptyString(pages.deployment_id, "deployment.pages.deployment_id");
  assertMatch(pages.artifact_sha256, SHA256, "deployment.pages.artifact_sha256");
  if (pages.status !== "active") throw new Error("Pages deployment is not active");

  const discord = deployment.discord;
  requirePlainObject(discord, "deployment.discord");
  requireExactKeys(discord, [
    "source_commit",
    "application_id",
    "image_digest",
    "job_revision",
    "oracle_revision",
    "oracle_release_sha256",
    "oracle_settings_sha256",
    "traffic_percent",
    "command_catalog_sha256",
    "command_catalog_prior_snapshot_sha256",
    "command_catalog_readback_sha256",
    "command_catalog_sync_report_sha256",
    "catalog_synced",
    "status",
  ], "deployment.discord");
  assertSameCommit(discord.source_commit, sourceCommit, "Discord deployment");
  assertMatch(discord.application_id, /^\d{17,20}$/u, "deployment.discord.application_id");
  assertMatch(discord.image_digest, IMAGE_DIGEST, "deployment.discord.image_digest");
  requireNonEmptyString(discord.job_revision, "deployment.discord.job_revision");
  requireNonEmptyString(discord.oracle_revision, "deployment.discord.oracle_revision");
  assertMatch(
    discord.oracle_release_sha256,
    SHA256,
    "deployment.discord.oracle_release_sha256",
  );
  assertMatch(
    discord.oracle_settings_sha256,
    SHA256,
    "deployment.discord.oracle_settings_sha256",
  );
  if (discord.traffic_percent !== 100) throw new Error("Discord traffic is not at 100 percent");
  assertMatch(discord.command_catalog_sha256, SHA256, "Discord command catalog sha256");
  assertMatch(
    discord.command_catalog_prior_snapshot_sha256,
    SHA256,
    "Discord command catalog prior snapshot sha256",
  );
  assertMatch(
    discord.command_catalog_readback_sha256,
    SHA256,
    "Discord command catalog readback sha256",
  );
  assertMatch(
    discord.command_catalog_sync_report_sha256,
    SHA256,
    "Discord command catalog sync report sha256",
  );
  if (discord.catalog_synced !== true) throw new Error("Discord command catalog was not synced");
  if (discord.status !== "active") throw new Error("Discord deployment is not active");

  validateEvidenceRef(deployment.rollback_snapshot, "rollback snapshot", sourceCommit, {
    exactKeys: ["id", "sha256", "source_commit", "status"],
  });
  if (deployment.rollback_snapshot.status !== "captured") {
    throw new Error("rollback snapshot was not captured before deployment");
  }
}

function validateObservation(observation, sourceCommit) {
  requirePlainObject(observation, "observation");
  requireExactKeys(observation, [
    "report_schema_id",
    "source_commit",
    "started_at",
    "ended_at",
    "duration_seconds",
    "probe_spec_sha256",
    "status",
    "report_sha256",
  ], "observation");
  if (observation.report_schema_id !== "clearra.production-observation.v1") {
    throw new Error("production observation schema is invalid");
  }
  assertSameCommit(observation.source_commit, sourceCommit, "observation");
  const started = isoTime(observation.started_at, "observation.started_at");
  const ended = isoTime(observation.ended_at, "observation.ended_at");
  if (!Number.isSafeInteger(observation.duration_seconds) || observation.duration_seconds < 1200) {
    throw new Error("production observation must last at least 1200 seconds");
  }
  const elapsedSeconds = Math.floor((ended - started) / 1000);
  if (elapsedSeconds !== observation.duration_seconds) {
    throw new Error("production observation duration is inconsistent with its timestamps");
  }
  if (observation.status !== "passed") throw new Error("production observation did not pass");
  assertMatch(observation.probe_spec_sha256, SHA256, "observation.probe_spec_sha256");
  assertMatch(observation.report_sha256, SHA256, "observation.report_sha256");
}

function validateProducerEvidence(
  manifest,
  discordCatalogSyncReport,
  productionObservationReport,
  sourceCommit,
) {
  if (discordCatalogSyncReport === undefined) {
    throw new Error("actual Discord command catalog sync producer report is required");
  }
  if (productionObservationReport === undefined) {
    throw new Error("actual four-surface production observation producer report is required");
  }
  validateDiscordCatalogSyncReport(discordCatalogSyncReport, {
    expectedSourceCommit: sourceCommit,
    expectedApplicationId: manifest.deployment.discord.application_id,
  });
  validateProductionObservationReport(productionObservationReport, {
    expectedSourceCommit: sourceCommit,
  });

  const discord = manifest.deployment.discord;
  if (
    discord.command_catalog_sha256 !==
      discordCatalogSyncReport.expected_catalog_sha256 ||
    discord.command_catalog_prior_snapshot_sha256 !==
      discordCatalogSyncReport.prior_snapshot_sha256 ||
    discord.command_catalog_readback_sha256 !==
      discordCatalogSyncReport.current_after_sha256 ||
    discord.command_catalog_sync_report_sha256 !==
      discordCatalogSyncReport.report_sha256
  ) {
    throw new Error("Discord deployment differs from the actual catalog sync producer report");
  }

  const observation = manifest.observation;
  for (const key of ["source_commit", "started_at", "ended_at", "duration_seconds", "probe_spec_sha256", "status", "report_sha256"]) {
    if (observation[key] !== productionObservationReport[key]) {
      throw new Error(`final observation ${key} differs from the actual producer report`);
    }
  }
  if (observation.report_schema_id !== productionObservationReport.schema_id) {
    throw new Error("final observation schema differs from the actual producer report");
  }

  const identities = new Map(
    productionObservationReport.surfaces.map((surface) => [
      surface.surface,
      surface.identity,
    ]),
  );
  const discordIdentity = identities.get("discord");
  if (
    discordIdentity.application_id !== discord.application_id ||
    discordIdentity.command_catalog_sha256 !== discord.command_catalog_sha256 ||
    discordIdentity.command_catalog_prior_snapshot_sha256 !==
      discord.command_catalog_prior_snapshot_sha256 ||
    discordIdentity.command_catalog_readback_sha256 !==
      discord.command_catalog_readback_sha256 ||
    discordIdentity.command_catalog_sync_report_sha256 !==
      discord.command_catalog_sync_report_sha256
  ) {
    throw new Error("observed Discord identity differs from the synced deployment");
  }
  const oracleIdentity = identities.get("oracle");
  if (
    oracleIdentity.release_id !== discord.oracle_revision ||
    oracleIdentity.release_tree_sha256 !== discord.oracle_release_sha256 ||
    oracleIdentity.settings_sha256 !== discord.oracle_settings_sha256
  ) {
    throw new Error("observed Oracle identity differs from the Discord deployment");
  }
  const cloudIdentity = identities.get("cloud");
  if (
    cloudIdentity.revision !== discord.job_revision ||
    cloudIdentity.image_digest !== discord.image_digest ||
    cloudIdentity.traffic_percent !== discord.traffic_percent
  ) {
    throw new Error("observed Cloud identity differs from the Discord deployment");
  }
  const pagesIdentity = identities.get("pages");
  if (
    pagesIdentity.deployment_id !== manifest.deployment.pages.deployment_id ||
    pagesIdentity.artifact_sha256 !== manifest.deployment.pages.artifact_sha256
  ) {
    throw new Error("observed Pages identity differs from the Pages deployment");
  }
}

function validateTag(tag, sourceCommit, release) {
  requirePlainObject(tag, "tag");
  requireExactKeys(tag, ["name", "target_commit", "annotated", "remote_verified"], "tag");
  if (tag.name !== release) throw new Error("release tag name differs from the release identity");
  assertSameCommit(tag.target_commit, sourceCommit, "release tag");
  if (tag.annotated !== true || tag.remote_verified !== true) {
    throw new Error("release tag must be annotated and remotely verified");
  }
}

function validateImmutableRelease(releaseRecord, sourceCommit, release) {
  requirePlainObject(releaseRecord, "immutable_release");
  requireExactKeys(releaseRecord, [
    "tag",
    "source_commit",
    "workflow_run_id",
    "immutable",
    "asset_count",
    "status",
  ], "immutable_release");
  if (releaseRecord.tag !== release) throw new Error("immutable release tag identity differs");
  assertSameCommit(releaseRecord.source_commit, sourceCommit, "immutable release");
  assertMatch(releaseRecord.workflow_run_id, DECIMAL_ID, "immutable_release.workflow_run_id");
  if (releaseRecord.immutable !== true || releaseRecord.status !== "published") {
    throw new Error("release is not recorded as immutable and published");
  }
  if (releaseRecord.asset_count !== 3) throw new Error("immutable release must contain exactly three assets");
}

function validateEvidenceRef(value, label, sourceCommit, { exactKeys }) {
  requirePlainObject(value, label);
  requireExactKeys(value, exactKeys, label);
  requireNonEmptyString(value.id, `${label}.id`);
  assertMatch(value.sha256, SHA256, `${label}.sha256`);
  assertSameCommit(value.source_commit, sourceCommit, label);
}

function rejectSecretMaterial(value, path = "manifest") {
  if (Array.isArray(value)) {
    value.forEach((entry, index) => rejectSecretMaterial(entry, `${path}[${index}]`));
    return;
  }
  if (!isPlainObject(value)) return;
  for (const [key, nested] of Object.entries(value)) {
    if (SECRET_KEY.test(key)) throw new Error(`${path}.${key} is forbidden secret material`);
    rejectSecretMaterial(nested, `${path}.${key}`);
  }
}

function rejectPriorReleaseAuthority(value, path = "manifest") {
  if (typeof value === "string" && FORBIDDEN_PRIOR_AUTHORITY.test(value)) {
    throw new Error(`${path} reuses a v0.7.5 authority identity`);
  }
  if (Array.isArray(value)) {
    value.forEach((entry, index) => rejectPriorReleaseAuthority(entry, `${path}[${index}]`));
    return;
  }
  if (!isPlainObject(value)) return;
  for (const [key, nested] of Object.entries(value)) {
    rejectPriorReleaseAuthority(nested, `${path}.${key}`);
  }
}

function requireExactKeys(value, expected, label) {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (actual.length !== wanted.length || actual.some((key, index) => key !== wanted[index])) {
    throw new Error(`${label} fields differ from the closed schema`);
  }
}

function requirePlainObject(value, label) {
  if (!isPlainObject(value)) throw new Error(`${label} must be an object`);
}

function isPlainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function requireNonEmptyString(value, label) {
  if (typeof value !== "string" || value.length === 0) throw new Error(`${label} must be non-empty`);
}

function assertMatch(value, expression, label) {
  if (typeof value !== "string" || !expression.test(value)) {
    throw new Error(`${label} is invalid`);
  }
}

function assertSameCommit(actual, expected, label) {
  if (actual !== expected) throw new Error(`${label} source commit differs from final source`);
}

function assertPositiveSafeInteger(value, label) {
  if (!Number.isSafeInteger(value) || value <= 0) throw new Error(`${label} must be a positive safe integer`);
}

function assertSortedIdentity(actual, expected, label) {
  const sorted = [...actual].sort();
  if (new Set(sorted).size !== sorted.length ||
      sorted.length !== expected.length ||
      sorted.some((entry, index) => entry !== [...expected].sort()[index])) {
    throw new Error(`${label} differ from the required identity set`);
  }
}

function isoTime(value, label) {
  if (typeof value !== "string" || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z$/u.test(value)) {
    throw new Error(`${label} must be an ISO-8601 UTC timestamp`);
  }
  const time = Date.parse(value);
  if (!Number.isFinite(time)) throw new Error(`${label} is invalid`);
  return time;
}
