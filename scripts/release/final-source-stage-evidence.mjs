// SRP rationale: final-source stage projection stays cohesive here because the behavior-level change reason is to reopen every canonical producer and fieldwise derive the three ordered release stages.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { lstat, open, readFile, realpath } from "node:fs/promises";
import { dirname, isAbsolute, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  canonicalJson,
  canonicalSha256,
  canonicalTimestamp,
  rejectSecretMaterial,
  requireExactKeys,
  requireNonEmptyString,
  requireSha256,
  requireSourceCommit,
  sealCanonicalReport,
  verifyCanonicalReportHash,
} from "./canonical-release-evidence.mjs";
import {
  validateCanonicalAcceptanceEvidence,
} from "./canonical-acceptance-evidence.mjs";
import {
  validateAuditSnapshot,
} from "../tools/audit-upstream-drift.mjs";
import {
  FINAL_SOURCE_STAGE_CARDINALITY,
  FINAL_SOURCE_STAGE_ORDER,
  validateFinalSourceEventPayload,
} from "./final-source-event-contract.mjs";
import {
  validateCanonicalDiscordCatalog,
  validateDiscordCatalogSnapshot,
  validateDiscordCatalogSyncReport,
} from "../../apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs";
import {
  validateDiscordCommandSyncAuthority,
} from "./discord-command-sync-authority.mjs";
import {
  validateCloudCandidateSmokeReport,
} from "./cloud-candidate-smoke-report.mjs";
import {
  PRODUCTION_OBSERVATION_SECONDS,
  validateProductionObservationReport,
  validateProductionProbeSpec,
} from "./observe-production-surfaces.mjs";
import {
  validateReleasePublicationEvidence,
  validateReleasePublicationFinalAuthority,
} from "./release-publication-evidence.mjs";
import {
  validatePagesDeploymentAuthorityReport,
} from "./pages-deployment-authority.mjs";
import {
  validateRollbackCaptureReport,
} from "./pages-rollback-authority.mjs";

export const FINAL_SOURCE_STAGE_EVIDENCE_SCHEMA_ID =
  "clearra.final-source-stage-evidence.v1";
export const ORACLE_ROLLBACK_CAPTURE_SCHEMA_ID =
  "clearra.oracle.rollback-authority-capture.v1";
export const ORACLE_OBSERVATION_SCHEMA_ID =
  "clearra.oracle.candidate-observation.v1";

const PRODUCT_REGISTRY_PATH =
  "tests/fixtures/contracts/product_capability_registry.v1.json";
const SEARCH_OPTION_CONTRACT_PATH =
  "tests/fixtures/contracts/search_option_contract.tsv";
const LEGACY_ALIAS_CONTRACT_PATH =
  "tests/fixtures/contracts/legacy_alias_equivalence.v1.json";
const IMPLEMENTATION_START_PATTERN =
  /^tests\/fixtures\/contracts\/upstream_drift_implementation_start\.v1\.json$/u;
const RELEASE_FREEZE_PATTERN =
  /^tests\/fixtures\/contracts\/upstream_drift_release_freeze(?:_retry[1-9][0-9]*)?\.v1\.json$/u;
const SOURCE_COMMIT = /^[0-9a-f]{40}$/u;
const GIT_OBJECT_ID = /^(?:[0-9a-f]{40}|[0-9a-f]{64})$/u;
const ORACLE_RELEASE_ID = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/u;
const ORACLE_BOOT_ID =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u;
const ORACLE_RUNTIME_AUTHORITY_KINDS = new Set([
  "clearra.rollback.runtime-identity.v1",
  "clearra.rollback.legacy-health-no-runtime.v1",
]);

const STAGE_PRODUCER_ROLES = Object.freeze(new Map([
  ["acceptance", Object.freeze([
    "canonical-acceptance",
    "implementation-start-audit",
    "legacy-alias-contract",
    "product-registry",
    "release-freeze-audit",
    "search-option-contract",
  ])],
  ["deployment", Object.freeze([
    "cloud-candidate-smoke",
    "discord-canonical-catalog",
    "discord-catalog-sync",
    "discord-command-sync-authority",
    "discord-prior-snapshot",
    "oracle-candidate-observation",
    "oracle-rollback-capture",
    "pages-deployment",
    "production-observation",
    "production-probe-spec",
    "rollback-snapshot",
  ])],
  ["publication", Object.freeze([
    "release-publication",
    "release-publication-authority",
    "release-publication-receipt",
  ])],
]));

export async function createAcceptanceStageEvidence(options, dependencies = {}) {
  const commit = requireSourceCommit(
    options.expectedSourceCommit,
    "acceptance stage source commit",
  );
  const acceptance = options.acceptanceEvidence;
  validateCanonicalAcceptanceEvidence(acceptance, {
    repository: acceptance?.repository,
    version: acceptance?.release_version,
    basePath: acceptance?.pages_base_path,
    sourceCommit: commit,
    runId: acceptance?.run_id,
    runAttempt: acceptance?.run_attempt,
  });
  const acceptanceFileSha256 = requireSha256(
    options.acceptanceEvidenceFileSha256,
    "canonical acceptance evidence file SHA-256",
  );
  const sourceIdentity = dependencies.inspectSourceIdentity
    ? await dependencies.inspectSourceIdentity(options.sourceRoot, commit)
    : await inspectExactSourceIdentity(options.sourceRoot, commit);
  const readTracked = dependencies.readTrackedEvidence ?? readTrackedEvidence;
  const registryFile = await readTracked(options.sourceRoot, commit, PRODUCT_REGISTRY_PATH);
  const searchFile = await readTracked(
    options.sourceRoot,
    commit,
    SEARCH_OPTION_CONTRACT_PATH,
  );
  const legacyFile = await readTracked(
    options.sourceRoot,
    commit,
    LEGACY_ALIAS_CONTRACT_PATH,
  );
  const registry = parseJsonBytes(registryFile.bytes, "product capability registry");
  const { implementationStartPath, releaseFreezePath } =
    selectCanonicalDriftEvidencePaths(registry);
  const [implementationStartFile, releaseFreezeFile] = await Promise.all([
    readTracked(options.sourceRoot, commit, implementationStartPath),
    readTracked(options.sourceRoot, commit, releaseFreezePath),
  ]);
  const implementationStart = parseJsonBytes(
    implementationStartFile.bytes,
    "implementation-start drift audit",
  );
  const releaseFreeze = parseJsonBytes(
    releaseFreezeFile.bytes,
    "release-freeze drift audit",
  );
  const validateAudit = dependencies.validateAuditSnapshot ?? validateAuditSnapshot;
  validateAudit(implementationStart, registry, {
    expectedPhase: "implementation-start",
  });
  validateAudit(releaseFreeze, registry, {
    expectedPhase: "release-freeze",
  });
  if (implementationStart.status !== "no-drift" || releaseFreeze.status !== "no-drift") {
    throw new Error("acceptance stage drift evidence did not close with no-drift");
  }
  const readinessOpenCount = countReadinessBlockers(registry);
  if (readinessOpenCount !== 0 || registry.release_readiness?.status !== "ready") {
    throw new Error("acceptance stage registry has open release readiness entries");
  }

  const fragments = acceptance.final_source_fragments;
  const events = [
    event("source", {
      commit,
      tree: sourceIdentity.tree,
      branch: "main",
      worktree_clean: true,
      engine_build_id: commit,
    }),
    event("contracts", {
      source_commit: commit,
      product_registry_schema_id: registry.schema_id,
      product_registry_sha256: registryFile.fileSha256,
      search_option_contract_sha256: searchFile.fileSha256,
      legacy_alias_contract_sha256: legacyFile.fileSha256,
      ctk3_contract_sha256: acceptance.accepted_inputs.ctk3_manifest_sha256,
      readiness_open_count: readinessOpenCount,
    }),
    event("toolchains", fragments.toolchains),
    event("drift-audit", driftPayload(
      implementationStartPath,
      implementationStartFile.fileSha256,
      commit,
      implementationStart,
    )),
    event("drift-audit", driftPayload(
      releaseFreezePath,
      releaseFreezeFile.fileSha256,
      commit,
      releaseFreeze,
    )),
    event("canonical-gate", fragments.canonical_gate),
    ...[...fragments.surface_reports]
      .sort((left, right) => left.surface.localeCompare(right.surface, "en"))
      .map((payload) => event("surface-report", payload)),
    ...[...fragments.release_artifacts]
      .sort((left, right) => left.role.localeCompare(right.role, "en"))
      .map((payload) => event("release-artifact", payload)),
  ];
  const producerInputs = [
    producerInput(
      "canonical-acceptance",
      acceptance.schema_id,
      acceptance.report_sha256,
      acceptanceFileSha256,
    ),
    producerInput(
      "implementation-start-audit",
      implementationStart.schema_id,
      implementationStartFile.fileSha256,
      implementationStartFile.fileSha256,
    ),
    producerInput(
      "legacy-alias-contract",
      "clearra.legacy-alias-contract.source.v1",
      legacyFile.fileSha256,
      legacyFile.fileSha256,
    ),
    producerInput(
      "product-registry",
      registry.schema_id,
      registryFile.fileSha256,
      registryFile.fileSha256,
    ),
    producerInput(
      "release-freeze-audit",
      releaseFreeze.schema_id,
      releaseFreezeFile.fileSha256,
      releaseFreezeFile.fileSha256,
    ),
    producerInput(
      "search-option-contract",
      "clearra.search-option-contract.source.v2",
      searchFile.fileSha256,
      searchFile.fileSha256,
    ),
  ];
  return createStageReport("acceptance", commit, producerInputs, events);
}

export function createDeploymentStageEvidence(options) {
  const commit = requireSourceCommit(
    options.expectedSourceCommit,
    "deployment stage source commit",
  );
  const pagesAuthority = validatePagesDeploymentAuthorityReport(
    options.pagesDeploymentAuthority,
    { expectedSourceCommit: commit },
  );
  if (pagesAuthority.mode !== "forward") {
    throw new Error("deployment stage requires a forward Pages authority report");
  }
  const pagesRollback = validateRollbackCaptureReport(
    options.pagesRollbackCapture,
    { expectedAuthoritySha: commit },
  );
  if (pagesRollback.repository !== pagesAuthority.repository) {
    throw new Error("Pages deployment and rollback capture repositories differ");
  }
  const catalog = validateCanonicalDiscordCatalog(
    options.discordCatalog,
    commit,
  );
  const priorSnapshot = validateDiscordCatalogSnapshot(
    options.discordPriorSnapshot,
    {
      expectedSourceCommit: commit,
      expectedApplicationId: options.discordCatalogSyncReport?.application_id,
    },
  );
  const syncAuthority = validateDiscordCommandSyncAuthority(
    options.discordCommandSyncAuthority,
    {
      sourceCommit: commit,
      catalog,
      catalogFileSha256: options.discordCatalogFileSha256,
    },
  );
  const sync = validateDiscordCatalogSyncReport(
    options.discordCatalogSyncReport,
    {
      expectedSourceCommit: commit,
      expectedApplicationId: options.discordCatalogSyncReport?.application_id,
      expectedCatalog: catalog,
      expectedCatalogFileSha256: options.discordCatalogFileSha256,
      expectedSyncAuthority: syncAuthority,
      expectedSyncAuthorityFileSha256:
        options.discordCommandSyncAuthorityFileSha256,
    },
  );
  if (
    sync.prior_snapshot_sha256 !== priorSnapshot.snapshot_sha256 ||
    sync.prior_catalog_sha256 !== priorSnapshot.catalog_sha256 ||
    sync.current_before_sha256 !== priorSnapshot.catalog_sha256
  ) {
    throw new Error("Discord sync report differs from its exact prior snapshot");
  }
  const smoke = validateCloudCandidateSmokeReport(
    options.cloudCandidateSmokeReport,
    { expectedSourceCommit: commit },
  );
  const oracleCapture = validateOracleRollbackCapture(
    options.oracleRollbackCapture,
  );
  const oracleObservation = validateOracleObservation(
    options.oracleObservation,
    { expectedSourceCommit: commit, cloudCandidateSmokeReport: smoke },
  );
  if (oracleCapture.deploymentNonce !== oracleObservation.deploymentNonce) {
    throw new Error("Oracle capture and observation deployment nonces differ");
  }
  const probeSpec = validateProductionProbeSpec(options.productionProbeSpec, commit);
  if (probeSpec.interval_seconds !== PRODUCTION_OBSERVATION_SECONDS) {
    throw new Error("production probe spec interval is not the exact release window");
  }
  const observation = validateProductionObservationReport(
    options.productionObservationReport,
    { expectedSourceCommit: commit },
  );
  if (observation.probe_spec_sha256 !== canonicalSha256(probeSpec)) {
    throw new Error("production observation differs from the exact canonical probe spec");
  }
  if (observation.interval_seconds !== probeSpec.interval_seconds) {
    throw new Error("production observation interval differs from its exact probe spec");
  }
  const expectedProbeAdapters = probeSpec.probes
    .map(({ surface, sha256 }) => ({ surface, sha256 }))
    .sort((left, right) => left.surface.localeCompare(right.surface, "en"));
  const observedProbeAdapters = [...observation.probe_adapters]
    .sort((left, right) => left.surface.localeCompare(right.surface, "en"));
  if (canonicalJson(observedProbeAdapters) !== canonicalJson(expectedProbeAdapters)) {
    throw new Error("production observation adapters differ from its exact probe spec");
  }
  const identities = new Map(observation.surfaces.map((surface) => [
    surface.surface,
    surface.identity,
  ]));
  const pages = identities.get("pages");
  const discord = identities.get("discord");
  const cloud = identities.get("cloud");
  const oracle = identities.get("oracle");
  if (
    pages.source_commit !== pagesAuthority.source_commit ||
    pages.deployment_id !== pagesAuthority.deployment_id ||
    pages.artifact_sha256 !== pagesAuthority.artifact_sha256 ||
    pages.base_path !== pagesAuthority.base_path ||
    pages.url !== pagesAuthority.page_url ||
    pages.status !== "active"
  ) {
    throw new Error("Pages deployment authority differs from the observed live Pages identity");
  }
  if (
    discord.application_id !== sync.application_id ||
    discord.command_catalog_sha256 !== catalog.catalog_sha256 ||
    discord.command_catalog_prior_snapshot_sha256 !== sync.prior_snapshot_sha256 ||
    discord.command_catalog_readback_sha256 !== sync.current_after_sha256 ||
    discord.command_catalog_sync_report_sha256 !== sync.report_sha256 ||
    discord.accepted_run_id !== sync.accepted_run_id ||
    discord.accepted_run_attempt !== sync.accepted_run_attempt ||
    discord.accepted_ctk3_manifest_sha256 !== sync.accepted_ctk3_manifest_sha256 ||
    discord.canonical_acceptance_evidence_sha256 !==
      sync.canonical_acceptance_evidence_sha256 ||
    discord.canonical_acceptance_evidence_file_sha256 !==
      sync.canonical_acceptance_evidence_file_sha256 ||
    discord.command_catalog_file_sha256 !== sync.command_catalog_file_sha256 ||
    discord.command_sync_authority_sha256 !== sync.command_sync_authority_sha256 ||
    discord.command_sync_authority_file_sha256 !==
      sync.command_sync_authority_file_sha256
  ) {
    throw new Error("observed Discord identity differs from its catalog producers");
  }
  if (
    cloud.revision !== smoke.candidate_revision ||
    cloud.image_digest !== smoke.image_digest ||
    cloud.job_smoke_report_sha256 !== smoke.report_sha256 ||
    new URL(cloud.tagged_url).origin !== smoke.candidate_url ||
    cloud.traffic_percent !== 100
  ) {
    throw new Error("observed Cloud identity differs from zero-traffic smoke authority");
  }
  if (!oracleObservationMatchesIdentity(oracleObservation, oracle)) {
    throw new Error("observed Oracle identity differs from its direct read-only authority");
  }
  const pagesPayload = {
    source_commit: commit,
    deployment_id: pagesAuthority.deployment_id,
    artifact_sha256: pagesAuthority.artifact_sha256,
    status: "active",
  };
  const rollbackPayload = {
    id: "clearra.rollback.snapshot-set.v1",
    sha256: canonicalSha256({
      oracle_capture_sha256: canonicalSha256(oracleCapture),
      pages_capture_report_sha256: pagesRollback.report_sha256,
    }),
    source_commit: commit,
    status: "captured",
  };
  const discordPayload = {
    source_commit: commit,
    application_id: discord.application_id,
    image_digest: cloud.image_digest,
    job_revision: cloud.revision,
    oracle_revision: oracle.release_id,
    oracle_release_sha256: oracle.release_tree_sha256,
    oracle_settings_sha256: oracle.settings_sha256,
    traffic_percent: cloud.traffic_percent,
    command_catalog_sha256: discord.command_catalog_sha256,
    command_catalog_prior_snapshot_sha256:
      discord.command_catalog_prior_snapshot_sha256,
    command_catalog_readback_sha256: discord.command_catalog_readback_sha256,
    command_catalog_sync_report_sha256:
      discord.command_catalog_sync_report_sha256,
    catalog_synced: true,
    status: "active",
  };
  const observationPayload = {
    report_schema_id: observation.schema_id,
    source_commit: observation.source_commit,
    started_at: observation.started_at,
    ended_at: observation.ended_at,
    duration_seconds: observation.duration_seconds,
    probe_spec_sha256: observation.probe_spec_sha256,
    status: observation.status,
    report_sha256: observation.report_sha256,
  };
  const events = [
    event("deployment-pages", pagesPayload),
    event("deployment-discord", discordPayload),
    event("rollback-snapshot", rollbackPayload),
    event("observation", observationPayload),
  ];
  const producerInputs = [
    inputFromDescriptor("cloud-candidate-smoke", smoke, options.cloudCandidateSmokeFileSha256),
    inputFromDescriptor("discord-canonical-catalog", catalog, options.discordCatalogFileSha256),
    inputFromDescriptor(
      "discord-command-sync-authority",
      syncAuthority,
      options.discordCommandSyncAuthorityFileSha256,
    ),
    inputFromDescriptor("discord-catalog-sync", sync, options.discordCatalogSyncFileSha256),
    producerInput(
      "discord-prior-snapshot",
      priorSnapshot.schema_id,
      priorSnapshot.snapshot_sha256,
      options.discordPriorSnapshotFileSha256,
    ),
    producerInput(
      "oracle-candidate-observation",
      ORACLE_OBSERVATION_SCHEMA_ID,
      canonicalSha256(oracleObservation),
      options.oracleObservationFileSha256,
    ),
    producerInput(
      "oracle-rollback-capture",
      ORACLE_ROLLBACK_CAPTURE_SCHEMA_ID,
      canonicalSha256(oracleCapture),
      options.oracleRollbackCaptureFileSha256,
    ),
    inputFromDescriptor(
      "pages-deployment",
      pagesAuthority,
      options.pagesDeploymentAuthorityFileSha256,
    ),
    producerInput(
      "production-observation",
      observation.schema_id,
      observation.report_sha256,
      options.productionObservationFileSha256,
    ),
    producerInput(
      "production-probe-spec",
      probeSpec.schema_id,
      canonicalSha256(probeSpec),
      options.productionProbeSpecFileSha256,
    ),
    inputFromDescriptor(
      "rollback-snapshot",
      pagesRollback,
      options.pagesRollbackCaptureFileSha256,
    ),
  ];
  return createStageReport("deployment", commit, producerInputs, events);
}

export function createPublicationStageEvidence(options) {
  const commit = requireSourceCommit(
    options.expectedSourceCommit,
    "publication stage source commit",
  );
  const publication = validateReleasePublicationEvidence(
    options.releasePublicationEvidence,
    {
      expectedRepository: options.releasePublicationEvidence?.repository,
      expectedSourceCommit: commit,
      expectedWorkflowRunId: options.releasePublicationEvidence?.workflow_run_id,
      expectedWorkflowRunAttempt:
        options.releasePublicationEvidence?.workflow_run_attempt,
      acceptanceEvidence: options.acceptanceEvidence,
      receipt: options.releasePublicationReceipt,
      receiptFileSha256: options.releasePublicationReceiptFileSha256,
    },
  );
  const publicationAuthority = validateReleasePublicationFinalAuthority(
    options.releasePublicationFinalAuthority,
    {
      expectedRepository: publication.repository,
      expectedSourceCommit: commit,
      expectedWorkflowRunId: publication.workflow_run_id,
      expectedWorkflowRunAttempt: publication.workflow_run_attempt,
      publicationEvidence: publication,
      publicationEvidenceFileSha256: options.releasePublicationFileSha256,
      publicationReceipt: options.releasePublicationReceipt,
      publicationReceiptFileSha256:
        options.releasePublicationReceiptFileSha256,
    },
  );
  const events = [
    event("tag", publication.final_source_fragments.tag),
    event(
      "immutable-release",
      publication.final_source_fragments.immutable_release,
    ),
  ];
  return createStageReport("publication", commit, [
    producerInput(
      "release-publication",
      publication.schema_id,
      publication.report_sha256,
      options.releasePublicationFileSha256,
    ),
    producerInput(
      "release-publication-authority",
      publicationAuthority.schema_id,
      publicationAuthority.report_sha256,
      options.releasePublicationFinalAuthorityFileSha256,
    ),
    producerInput(
      "release-publication-receipt",
      options.releasePublicationReceipt.schema_id,
      options.releasePublicationReceipt.report_sha256,
      options.releasePublicationReceiptFileSha256,
    ),
  ], events);
}

export function validateFinalSourceStageEvidence(
  value,
  { expectedStage, expectedSourceCommit } = {},
) {
  requireExactKeys(value, [
    "schema_id",
    "stage",
    "source_commit",
    "producer_inputs",
    "events",
    "status",
    "report_sha256",
  ], "final-source stage evidence");
  if (value.schema_id !== FINAL_SOURCE_STAGE_EVIDENCE_SCHEMA_ID) {
    throw new Error("final-source stage evidence schema is invalid");
  }
  verifyCanonicalReportHash(value, "final-source stage evidence");
  requireSourceCommit(value.source_commit, "stage evidence source commit");
  if (!FINAL_SOURCE_STAGE_ORDER.includes(value.stage)) {
    throw new Error("final-source stage evidence stage is invalid");
  }
  if (expectedStage !== undefined && value.stage !== expectedStage) {
    throw new Error("final-source stage evidence is out of order");
  }
  if (
    expectedSourceCommit !== undefined &&
    value.source_commit !== expectedSourceCommit
  ) {
    throw new Error("final-source stage evidence source differs from the attempt");
  }
  if (value.status !== "passed") {
    throw new Error("final-source stage evidence did not pass");
  }
  validateProducerInputs(value.stage, value.producer_inputs);
  validateStageEvents(value.stage, value.events, value.source_commit);
  rejectSecretMaterial(value, "final-source stage evidence");
  return value;
}

export function selectCanonicalDriftEvidencePaths(registry) {
  const requirement = Array.isArray(registry?.requirements)
    ? registry.requirements.find((entry) => entry?.id === "REQ-V080-020")
    : undefined;
  const evidence = requirement?.implementation_evidence;
  if (!Array.isArray(evidence)) {
    throw new Error("REQ-V080-020 implementation evidence is missing");
  }
  const implementationStarts = evidence.filter((path) =>
    typeof path === "string" && IMPLEMENTATION_START_PATTERN.test(path));
  const releaseFreezes = evidence.filter((path) =>
    typeof path === "string" && RELEASE_FREEZE_PATTERN.test(path));
  if (implementationStarts.length !== 1 || releaseFreezes.length < 1) {
    throw new Error("REQ-V080-020 drift evidence paths are incomplete or ambiguous");
  }
  return Object.freeze({
    implementationStartPath: implementationStarts[0],
    releaseFreezePath: releaseFreezes.at(-1),
  });
}

export function validateOracleRollbackCapture(value) {
  requireExactKeys(value, [
    "priorRevision",
    "priorOracleRelease",
    "priorOracleReleaseId",
    "priorOracleReleaseSha256",
    "priorOracleSettingsBackup",
    "priorOracleSettingsSha256",
    "priorRuntimeAuthorityKind",
    "priorRuntimeAuthoritySha256",
    "priorJobUrl",
    "deploymentNonce",
  ], "Oracle rollback authority capture");
  requirePattern(
    value.priorRevision,
    ORACLE_RELEASE_ID,
    "Oracle prior Cloud revision",
  );
  requirePattern(
    value.priorOracleReleaseId,
    ORACLE_RELEASE_ID,
    "Oracle prior release ID",
  );
  if (value.priorOracleRelease !== `/opt/clearra/releases/${value.priorOracleReleaseId}`) {
    throw new Error("Oracle prior release path differs from its release ID");
  }
  for (const [field, label] of [
    ["priorOracleReleaseSha256", "Oracle prior release SHA-256"],
    ["priorOracleSettingsSha256", "Oracle prior settings SHA-256"],
    ["priorRuntimeAuthoritySha256", "Oracle prior runtime authority SHA-256"],
    ["deploymentNonce", "Oracle deployment nonce"],
  ]) requireSha256(value[field], label);
  if (
    value.priorOracleSettingsBackup !==
      `/etc/clearra-gateway/settings.pre-v0.8.0-${value.deploymentNonce}`
  ) {
    throw new Error("Oracle prior settings backup differs from the deployment nonce");
  }
  if (!ORACLE_RUNTIME_AUTHORITY_KINDS.has(value.priorRuntimeAuthorityKind)) {
    throw new Error("Oracle prior runtime authority kind is invalid");
  }
  requireCanonicalHttpsJobUrl(value.priorJobUrl, "Oracle prior job URL");
  rejectSecretMaterial(value, "Oracle rollback authority capture");
  return value;
}

export function validateOracleObservation(
  value,
  { expectedSourceCommit, cloudCandidateSmokeReport } = {},
) {
  requireExactKeys(value, [
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
    "gatewayPid",
    "gatewayStartMonotonicUsec",
    "bootId",
    "readyRecordObserved",
    "verifiedAfter",
    "freshOperationAt",
    "observedAt",
    "runtimeIdentity",
  ], "Oracle candidate observation");
  if (value.contract !== ORACLE_OBSERVATION_SCHEMA_ID) {
    throw new Error("Oracle candidate observation contract is invalid");
  }
  const commit = requireSourceCommit(
    value.sourceCommit,
    "Oracle observation source commit",
  );
  if (expectedSourceCommit !== undefined && commit !== expectedSourceCommit) {
    throw new Error("Oracle observation source differs from the final source");
  }
  const candidateUrl = requireCanonicalHttpsOrigin(
    value.candidateUrl,
    "Oracle candidate URL",
  );
  if (value.jobUrl !== `${candidateUrl}/jobs`) {
    throw new Error("Oracle observation job URL differs from its candidate origin");
  }
  requireCanonicalHttpsJobUrl(value.jobUrl, "Oracle observation job URL");
  requirePattern(value.candidateRevision, ORACLE_RELEASE_ID, "Oracle candidate revision");
  requirePattern(value.oracleReleaseId, ORACLE_RELEASE_ID, "Oracle release ID");
  if (
    value.oracleReleaseId !== `v0.8.0-${commit.slice(0, 7)}` ||
    value.activeReleasePath !== `/opt/clearra/releases/${value.oracleReleaseId}`
  ) {
    throw new Error("Oracle active release identity differs from the source commit");
  }
  for (const [field, label] of [
    ["oracleReleaseSha256", "Oracle release SHA-256"],
    ["oracleSettingsSha256", "Oracle settings SHA-256"],
    ["deploymentNonce", "Oracle deployment nonce"],
  ]) requireSha256(value[field], label);
  if (
    !Number.isSafeInteger(value.gatewayPid) || value.gatewayPid < 1 ||
    !Number.isSafeInteger(value.gatewayStartMonotonicUsec) ||
    value.gatewayStartMonotonicUsec < 1 ||
    !ORACLE_BOOT_ID.test(value.bootId) ||
    value.readyRecordObserved !== true
  ) {
    throw new Error("Oracle observation PID/start/READY authority is invalid");
  }
  const verifiedAfter = canonicalTimestamp(
    value.verifiedAfter,
    "Oracle verified-after time",
  );
  const freshOperationAt = canonicalTimestamp(
    value.freshOperationAt,
    "Oracle fresh operation time",
  );
  const observedAt = canonicalTimestamp(value.observedAt, "Oracle observation time");
  if (
    Date.parse(verifiedAfter) > Date.parse(freshOperationAt) ||
    Date.parse(freshOperationAt) > Date.parse(observedAt)
  ) {
    throw new Error("Oracle verified-after/fresh operation authority is out of order");
  }
  requireExactKeys(value.runtimeIdentity, [
    "schema",
    "sourceCommit",
    "engineBuildId",
    "contractSchemaVersion",
    "supplySemanticsId",
    "artifactSchemaVersion",
  ], "Oracle runtime identity");
  if (
    value.runtimeIdentity.schema !== "clearra.runtime.identity.v2" ||
    value.runtimeIdentity.sourceCommit !== commit ||
    value.runtimeIdentity.engineBuildId !== commit ||
    value.runtimeIdentity.contractSchemaVersion !== "clearra.search.contract.v2" ||
    value.runtimeIdentity.supplySemanticsId !==
      "clearra.supply.projected-terminal-lookahead.v1" ||
    value.runtimeIdentity.artifactSchemaVersion !== "clearra.solution-data.v1"
  ) {
    throw new Error("Oracle runtime identity differs from the final source contract");
  }
  if (cloudCandidateSmokeReport !== undefined && (
    value.candidateRevision !== cloudCandidateSmokeReport.candidate_revision ||
    candidateUrl !== cloudCandidateSmokeReport.candidate_url
  )) {
    throw new Error("Oracle observation differs from the exact Cloud candidate");
  }
  rejectSecretMaterial(value, "Oracle candidate observation");
  return value;
}

function oracleObservationMatchesIdentity(observation, identity) {
  return identity.source_commit === observation.sourceCommit &&
    identity.release_id === observation.oracleReleaseId &&
    identity.release_tree_sha256 === observation.oracleReleaseSha256 &&
    identity.settings_sha256 === observation.oracleSettingsSha256 &&
    identity.candidate_revision === observation.candidateRevision &&
    new URL(identity.candidate_url).origin === observation.candidateUrl &&
    identity.job_url === observation.jobUrl &&
    identity.deployment_nonce === observation.deploymentNonce &&
    identity.gateway_pid === observation.gatewayPid &&
    identity.gateway_start_monotonic_usec ===
      observation.gatewayStartMonotonicUsec &&
    identity.boot_id === observation.bootId &&
    identity.ready_record_observed === observation.readyRecordObserved &&
    identity.verified_after === observation.verifiedAfter &&
    identity.status === "active";
}

function requireCanonicalHttpsOrigin(value, label) {
  let url;
  try {
    url = new URL(String(value ?? ""));
  } catch {
    throw new Error(`${label} is invalid`);
  }
  if (
    url.protocol !== "https:" || url.username || url.password ||
    url.search || url.hash || url.pathname !== "/" || url.origin !== value
  ) {
    throw new Error(`${label} must be a canonical credential-free HTTPS origin`);
  }
  return url.origin;
}

function requireCanonicalHttpsJobUrl(value, label) {
  let url;
  try {
    url = new URL(String(value ?? ""));
  } catch {
    throw new Error(`${label} is invalid`);
  }
  if (
    url.protocol !== "https:" || url.username || url.password ||
    url.search || url.hash || url.pathname !== "/jobs" ||
    url.toString() !== value
  ) {
    throw new Error(`${label} must be a canonical credential-free HTTPS /jobs URL`);
  }
  return value;
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new Error(`${label} is invalid`);
  }
  return value;
}

function createStageReport(stage, sourceCommit, producerInputs, events) {
  const report = sealCanonicalReport({
    schema_id: FINAL_SOURCE_STAGE_EVIDENCE_SCHEMA_ID,
    stage,
    source_commit: sourceCommit,
    producer_inputs: [...producerInputs]
      .sort((left, right) => left.role.localeCompare(right.role, "en")),
    events,
    status: "passed",
  });
  validateFinalSourceStageEvidence(report, {
    expectedStage: stage,
    expectedSourceCommit: sourceCommit,
  });
  return report;
}

function validateProducerInputs(stage, inputs) {
  const roles = STAGE_PRODUCER_ROLES.get(stage);
  if (!Array.isArray(inputs) || inputs.length !== roles.length) {
    throw new Error(`${stage} stage producer input count is invalid`);
  }
  const actualRoles = [];
  for (const input of inputs) {
    requireExactKeys(input, [
      "role",
      "schema_id",
      "evidence_sha256",
      "file_sha256",
    ], `${stage} stage producer input`);
    requireNonEmptyString(input.role, "stage producer input role");
    requireNonEmptyString(input.schema_id, "stage producer input schema");
    requireSha256(input.evidence_sha256, "stage producer evidence SHA-256");
    requireSha256(input.file_sha256, "stage producer file SHA-256");
    actualRoles.push(input.role);
  }
  if (canonicalJson(actualRoles) !== canonicalJson(roles)) {
    throw new Error(`${stage} stage producer roles differ from the closed contract`);
  }
}

function validateStageEvents(stage, events, sourceCommit) {
  const cardinality = FINAL_SOURCE_STAGE_CARDINALITY.get(stage);
  const expectedKinds = [...cardinality.entries()]
    .flatMap(([kind, count]) => Array.from({ length: count }, () => kind));
  if (!Array.isArray(events) || events.length !== expectedKinds.length) {
    throw new Error(`${stage} stage event count is invalid`);
  }
  const actualKinds = [];
  for (const [index, entry] of events.entries()) {
    requireExactKeys(entry, ["kind", "payload"], `${stage} stage event ${index}`);
    validateFinalSourceEventPayload(entry.kind, entry.payload, sourceCommit);
    actualKinds.push(entry.kind);
  }
  if (canonicalJson(actualKinds) !== canonicalJson(expectedKinds)) {
    throw new Error(`${stage} stage event order or cardinality is invalid`);
  }
  if (stage === "acceptance") {
    requireIdentitySet(
      events.filter(({ kind }) => kind === "drift-audit")
        .map(({ payload }) => payload.phase),
      ["implementation-start", "release-freeze"],
      "acceptance drift phases",
    );
    requireIdentitySet(
      events.filter(({ kind }) => kind === "surface-report")
        .map(({ payload }) => payload.surface),
      ["desktop", "discord", "native", "wasm"],
      "acceptance surfaces",
    );
    requireIdentitySet(
      events.filter(({ kind }) => kind === "release-artifact")
        .map(({ payload }) => payload.role),
      ["linux-cli", "windows-cli", "windows-gui"],
      "acceptance artifact roles",
    );
  }
}

function requireIdentitySet(actual, expected, label) {
  const sorted = [...actual].sort();
  const wanted = [...expected].sort();
  if (
    new Set(sorted).size !== sorted.length ||
    canonicalJson(sorted) !== canonicalJson(wanted)
  ) {
    throw new Error(`${label} differ from the closed identity set`);
  }
}

function producerInput(role, schemaId, evidenceSha256, fileSha256) {
  return Object.freeze({
    role,
    schema_id: requireNonEmptyString(schemaId, `${role} schema ID`),
    evidence_sha256: requireSha256(evidenceSha256, `${role} evidence SHA-256`),
    file_sha256: requireSha256(fileSha256, `${role} file SHA-256`),
  });
}

function inputFromDescriptor(role, report, fileSha256) {
  const evidenceSha256 = report.report_sha256 ??
    report.snapshot_sha256 ??
    report.catalog_sha256 ??
    canonicalSha256(report);
  return producerInput(role, report.schema_id, evidenceSha256, fileSha256);
}

function event(kind, payload) {
  return Object.freeze({ kind, payload });
}

function driftPayload(path, fileSha256, sourceCommit, audit) {
  return Object.freeze({
    id: path,
    sha256: fileSha256,
    source_commit: sourceCommit,
    phase: audit.phase,
    status: audit.status,
  });
}

function countReadinessBlockers(registry) {
  const terminal = new Set(
    registry.release_readiness?.allowed_terminal_implementation_statuses ?? [],
  );
  return [
    ...(registry.capability_implementation ?? []),
    ...(registry.result_affecting_option_exposure ?? []),
    ...(registry.requirements ?? []),
  ].filter((entry) => !terminal.has(entry?.implementation_status)).length;
}

async function inspectExactSourceIdentity(sourceRoot, sourceCommit) {
  const root = await realpath(resolve(requireNonEmptyString(sourceRoot, "source root")));
  const run = (arguments_, encoding = "utf8") => {
    const result = spawnSync("git", ["--no-replace-objects", ...arguments_], {
      cwd: root,
      encoding,
      maxBuffer: 8 * 1024 * 1024,
      shell: false,
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    });
    if (result.error || result.signal || result.status !== 0) {
      throw new Error("acceptance stage could not inspect the exact Git source");
    }
    return result.stdout;
  };
  const repositoryRoot = String(run(["rev-parse", "--show-toplevel"])).trim();
  if (await realpath(resolve(repositoryRoot)) !== root) {
    throw new Error("acceptance stage source root is not the Git worktree root");
  }
  if (String(run(["rev-parse", "HEAD^{commit}"])).trim() !== sourceCommit) {
    throw new Error("acceptance stage worktree HEAD differs from the accepted source");
  }
  const status = String(run(["status", "--porcelain=v1", "--untracked-files=all"]));
  if (status.length !== 0) {
    throw new Error("acceptance stage source worktree is not clean");
  }
  const tree = String(run(["rev-parse", `${sourceCommit}^{tree}`])).trim();
  if (!GIT_OBJECT_ID.test(tree)) {
    throw new Error("acceptance stage Git tree identity is invalid");
  }
  return Object.freeze({ root, tree });
}

async function readTrackedEvidence(sourceRoot, sourceCommit, repositoryPath) {
  const root = await realpath(resolve(sourceRoot));
  if (
    typeof repositoryPath !== "string" ||
    repositoryPath.length === 0 ||
    repositoryPath.includes("\\") ||
    repositoryPath.startsWith("/") ||
    repositoryPath.split("/").some((part) => !part || part === "." || part === "..")
  ) {
    throw new Error("tracked evidence path is not canonical repository-relative");
  }
  const path = resolve(root, ...repositoryPath.split("/"));
  const relativePath = relative(root, path).split(sep).join("/");
  if (relativePath !== repositoryPath) {
    throw new Error("tracked evidence path escapes the exact source root");
  }
  const metadata = await lstat(path);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error(`tracked evidence must be a regular non-link file: ${repositoryPath}`);
  }
  const blob = runGit(root, ["rev-parse", `${sourceCommit}:${repositoryPath}`]).trim();
  if (!GIT_OBJECT_ID.test(blob)) {
    throw new Error(`tracked evidence blob is invalid: ${repositoryPath}`);
  }
  const bytes = runGitBytes(root, ["cat-file", "blob", blob]);
  return Object.freeze({
    path: repositoryPath,
    bytes,
    fileSha256: createHash("sha256").update(bytes).digest("hex"),
  });
}

function runGitBytes(cwd, arguments_) {
  const result = spawnSync("git", ["--no-replace-objects", ...arguments_], {
    cwd,
    encoding: null,
    maxBuffer: 8 * 1024 * 1024,
    shell: false,
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  if (result.error || result.signal || result.status !== 0) {
    throw new Error("tracked evidence bytes could not be read from the accepted Git object");
  }
  return Buffer.from(result.stdout);
}

function runGit(cwd, arguments_) {
  const result = spawnSync("git", ["--no-replace-objects", ...arguments_], {
    cwd,
    encoding: "utf8",
    maxBuffer: 8 * 1024 * 1024,
    shell: false,
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  if (result.error || result.signal || result.status !== 0) {
    throw new Error("tracked evidence could not be resolved from the accepted Git source");
  }
  return result.stdout;
}

function parseJsonBytes(bytes, label) {
  try {
    return JSON.parse(Buffer.from(bytes).toString("utf8"));
  } catch {
    throw new Error(`${label} is not valid JSON`);
  }
}

async function readCanonicalEvidenceFile(path, label, expectedFileSha256) {
  const target = resolve(requireNonEmptyString(path, `${label} path`));
  await assertSafeDirectoryChain(dirname(target));
  const metadata = await lstat(target);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error(`${label} must be a regular non-link file`);
  }
  const bytes = await readFile(target);
  let value;
  try {
    value = JSON.parse(bytes.toString("utf8"));
  } catch {
    throw new Error(`${label} is not valid JSON`);
  }
  if (bytes.toString("utf8") !== `${canonicalJson(value)}\n`) {
    throw new Error(`${label} bytes are not canonical JSON`);
  }
  const fileSha256 = createHash("sha256").update(bytes).digest("hex");
  if (
    expectedFileSha256 !== undefined &&
    fileSha256 !== requireSha256(expectedFileSha256, `${label} expected file SHA-256`)
  ) {
    throw new Error(`${label} raw file SHA-256 differs from its authority`);
  }
  return Object.freeze({
    value,
    fileSha256,
  });
}

async function writeCanonicalJsonNew(path, value) {
  const target = resolve(requireNonEmptyString(path, "stage evidence output path"));
  await assertSafeDirectoryChain(dirname(target));
  const handle = await open(target, "wx", 0o600);
  try {
    await handle.writeFile(`${canonicalJson(value)}\n`, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }
}

async function assertSafeDirectoryChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const metadata = await lstat(current);
    if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
      throw new Error("stage evidence path uses a non-directory or link");
    }
    const parent = dirname(current);
    if (parent === current) return;
    current = parent;
  }
}

function requireAbsoluteDirectory(value, label) {
  if (typeof value !== "string" || !isAbsolute(value)) {
    throw new Error(`${label} must be an absolute path`);
  }
  return value;
}

function requireOnlyStageOptions(values, stage, expected) {
  const actual = Object.keys(values).sort();
  const wanted = [...expected].sort();
  if (
    actual.length !== wanted.length ||
    actual.some((option, index) => option !== wanted[index])
  ) {
    throw new Error(`${stage} stage options differ from the closed CLI contract`);
  }
  for (const option of wanted) {
    if (typeof values[option] !== "string" || values[option].length === 0) {
      throw new Error(`${stage} stage option --${option} is required`);
    }
  }
}

async function main() {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    options: {
      "expected-source-commit": { type: "string" },
      "source-root": { type: "string" },
      "canonical-acceptance-evidence": { type: "string" },
      "pages-deployment-authority": { type: "string" },
      "pages-deployment-authority-file-sha256": { type: "string" },
      "pages-rollback-capture": { type: "string" },
      "pages-rollback-capture-file-sha256": { type: "string" },
      "discord-catalog": { type: "string" },
      "discord-catalog-file-sha256": { type: "string" },
      "discord-prior-snapshot": { type: "string" },
      "discord-prior-snapshot-file-sha256": { type: "string" },
      "discord-command-sync-authority": { type: "string" },
      "discord-command-sync-authority-file-sha256": { type: "string" },
      "discord-catalog-sync-report": { type: "string" },
      "discord-catalog-sync-report-file-sha256": { type: "string" },
      "cloud-candidate-smoke-report": { type: "string" },
      "cloud-candidate-smoke-report-file-sha256": { type: "string" },
      "oracle-rollback-capture": { type: "string" },
      "oracle-rollback-capture-file-sha256": { type: "string" },
      "oracle-observation": { type: "string" },
      "oracle-observation-file-sha256": { type: "string" },
      "production-probe-spec": { type: "string" },
      "production-probe-spec-file-sha256": { type: "string" },
      "production-observation-report": { type: "string" },
      "production-observation-report-file-sha256": { type: "string" },
      "release-publication-evidence": { type: "string" },
      "release-publication-evidence-file-sha256": { type: "string" },
      "release-publication-final-authority": { type: "string" },
      "release-publication-final-authority-file-sha256": { type: "string" },
      "release-publication-receipt": { type: "string" },
      "release-publication-receipt-file-sha256": { type: "string" },
      report: { type: "string" },
      output: { type: "string" },
    },
    strict: true,
  });
  const stage = positionals[0];
  if (positionals.length !== 1 || !new Set([...FINAL_SOURCE_STAGE_ORDER, "verify"]).has(stage)) {
    throw new Error("usage: final-source-stage-evidence.mjs (acceptance|deployment|publication|verify) [closed options]");
  }
  const expectedSourceCommit = values["expected-source-commit"];
  requireSourceCommit(expectedSourceCommit, "expected final-source commit");
  if (stage === "verify") {
    requireOnlyStageOptions(values, "verify", [
      "expected-source-commit", "report",
    ]);
    const input = await readCanonicalEvidenceFile(values.report, "stage evidence report");
    validateFinalSourceStageEvidence(input.value, { expectedSourceCommit });
    process.stdout.write(
      `${FINAL_SOURCE_STAGE_EVIDENCE_SCHEMA_ID} ${input.value.report_sha256}\n`,
    );
    return;
  }
  if (!values.output || values.report) {
    throw new Error(`${stage} stage requires --output and forbids --report`);
  }
  let report;
  if (stage === "acceptance") {
    requireOnlyStageOptions(values, "acceptance", [
      "expected-source-commit",
      "source-root",
      "canonical-acceptance-evidence",
      "output",
    ]);
    requireAbsoluteDirectory(values["source-root"], "acceptance source root");
    const acceptance = await readCanonicalEvidenceFile(
      values["canonical-acceptance-evidence"],
      "canonical acceptance evidence",
    );
    report = await createAcceptanceStageEvidence({
      expectedSourceCommit,
      sourceRoot: values["source-root"],
      acceptanceEvidence: acceptance.value,
      acceptanceEvidenceFileSha256: acceptance.fileSha256,
    });
  } else if (stage === "deployment") {
    requireOnlyStageOptions(values, "deployment", [
      "expected-source-commit",
      "pages-deployment-authority",
      "pages-deployment-authority-file-sha256",
      "pages-rollback-capture",
      "pages-rollback-capture-file-sha256",
      "discord-catalog",
      "discord-catalog-file-sha256",
      "discord-prior-snapshot",
      "discord-prior-snapshot-file-sha256",
      "discord-command-sync-authority",
      "discord-command-sync-authority-file-sha256",
      "discord-catalog-sync-report",
      "discord-catalog-sync-report-file-sha256",
      "cloud-candidate-smoke-report",
      "cloud-candidate-smoke-report-file-sha256",
      "oracle-rollback-capture",
      "oracle-rollback-capture-file-sha256",
      "oracle-observation",
      "oracle-observation-file-sha256",
      "production-probe-spec",
      "production-probe-spec-file-sha256",
      "production-observation-report",
      "production-observation-report-file-sha256",
      "output",
    ]);
    const [pages, rollback, catalog, prior, authority, sync, smoke, oracleCapture,
      oracleObservation, spec, observation] =
      await Promise.all([
        readCanonicalEvidenceFile(values["pages-deployment-authority"], "Pages deployment authority", values["pages-deployment-authority-file-sha256"]),
        readCanonicalEvidenceFile(values["pages-rollback-capture"], "Pages rollback capture", values["pages-rollback-capture-file-sha256"]),
        readCanonicalEvidenceFile(values["discord-catalog"], "Discord canonical catalog", values["discord-catalog-file-sha256"]),
        readCanonicalEvidenceFile(values["discord-prior-snapshot"], "Discord prior snapshot", values["discord-prior-snapshot-file-sha256"]),
        readCanonicalEvidenceFile(values["discord-command-sync-authority"], "Discord command sync authority", values["discord-command-sync-authority-file-sha256"]),
        readCanonicalEvidenceFile(values["discord-catalog-sync-report"], "Discord catalog sync report", values["discord-catalog-sync-report-file-sha256"]),
        readCanonicalEvidenceFile(values["cloud-candidate-smoke-report"], "Cloud candidate smoke report", values["cloud-candidate-smoke-report-file-sha256"]),
        readCanonicalEvidenceFile(values["oracle-rollback-capture"], "Oracle rollback capture", values["oracle-rollback-capture-file-sha256"]),
        readCanonicalEvidenceFile(values["oracle-observation"], "Oracle observation", values["oracle-observation-file-sha256"]),
        readCanonicalEvidenceFile(values["production-probe-spec"], "production probe spec", values["production-probe-spec-file-sha256"]),
        readCanonicalEvidenceFile(values["production-observation-report"], "production observation report", values["production-observation-report-file-sha256"]),
      ]);
    report = createDeploymentStageEvidence({
      expectedSourceCommit,
      pagesDeploymentAuthority: pages.value,
      pagesDeploymentAuthorityFileSha256: pages.fileSha256,
      pagesRollbackCapture: rollback.value,
      pagesRollbackCaptureFileSha256: rollback.fileSha256,
      discordCatalog: catalog.value,
      discordCatalogFileSha256: catalog.fileSha256,
      discordPriorSnapshot: prior.value,
      discordPriorSnapshotFileSha256: prior.fileSha256,
      discordCommandSyncAuthority: authority.value,
      discordCommandSyncAuthorityFileSha256: authority.fileSha256,
      discordCatalogSyncReport: sync.value,
      discordCatalogSyncFileSha256: sync.fileSha256,
      cloudCandidateSmokeReport: smoke.value,
      cloudCandidateSmokeFileSha256: smoke.fileSha256,
      oracleRollbackCapture: oracleCapture.value,
      oracleRollbackCaptureFileSha256: oracleCapture.fileSha256,
      oracleObservation: oracleObservation.value,
      oracleObservationFileSha256: oracleObservation.fileSha256,
      productionProbeSpec: spec.value,
      productionProbeSpecFileSha256: spec.fileSha256,
      productionObservationReport: observation.value,
      productionObservationFileSha256: observation.fileSha256,
    });
  } else {
    requireOnlyStageOptions(values, "publication", [
      "expected-source-commit",
      "release-publication-evidence",
      "release-publication-evidence-file-sha256",
      "release-publication-final-authority",
      "release-publication-final-authority-file-sha256",
      "release-publication-receipt",
      "release-publication-receipt-file-sha256",
      "canonical-acceptance-evidence",
      "output",
    ]);
    const [publication, authority, receipt, acceptance] = await Promise.all([
      readCanonicalEvidenceFile(
        values["release-publication-evidence"],
        "release publication evidence",
        values["release-publication-evidence-file-sha256"],
      ),
      readCanonicalEvidenceFile(
        values["release-publication-final-authority"],
        "release publication final authority",
        values["release-publication-final-authority-file-sha256"],
      ),
      readCanonicalEvidenceFile(
        values["release-publication-receipt"],
        "release publication receipt",
        values["release-publication-receipt-file-sha256"],
      ),
      readCanonicalEvidenceFile(values["canonical-acceptance-evidence"], "canonical acceptance evidence"),
    ]);
    report = createPublicationStageEvidence({
      expectedSourceCommit,
      releasePublicationEvidence: publication.value,
      releasePublicationFileSha256: publication.fileSha256,
      releasePublicationFinalAuthority: authority.value,
      releasePublicationFinalAuthorityFileSha256: authority.fileSha256,
      releasePublicationReceipt: receipt.value,
      releasePublicationReceiptFileSha256: receipt.fileSha256,
      acceptanceEvidence: acceptance.value,
    });
  }
  await writeCanonicalJsonNew(values.output, report);
  process.stdout.write(`${FINAL_SOURCE_STAGE_EVIDENCE_SCHEMA_ID} ${report.report_sha256}\n`);
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    await main();
  } catch (error) {
    process.stderr.write(
      `final_source_stage_evidence=failed reason=${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}
