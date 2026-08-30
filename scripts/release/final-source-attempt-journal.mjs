import { createHash, randomUUID } from "node:crypto";
import { lstat, mkdir, open, readFile, rename, unlink } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { canonicalJson, requireSha256 } from "./canonical-release-evidence.mjs";
import {
  FINAL_SOURCE_STAGE_CARDINALITY,
  FINAL_SOURCE_STAGE_ORDER,
  stageForFinalSourceEventKind,
  validateFinalSourceEventPayload,
} from "./final-source-event-contract.mjs";
import {
  createAcceptanceStageEvidence,
  createDeploymentStageEvidence,
  createPublicationStageEvidence,
  FINAL_SOURCE_STAGE_EVIDENCE_SCHEMA_ID,
  validateFinalSourceStageEvidence,
} from "./final-source-stage-evidence.mjs";
import {
  validateFinalSourceRevalidationFromStages,
} from "./validate-final-source-revalidation.mjs";

export const FINAL_SOURCE_ATTEMPT_SCHEMA_ID =
  "clearra.final-source-attempt-journal.v1";
export const FINAL_SOURCE_EVENT_SCHEMA_ID =
  "clearra.final-source-attempt-event.v1";

const RELEASE = "v0.8.0";
const SHA1 = /^[0-9a-f]{40}$/u;
const SHA256 = /^[0-9a-f]{64}$/u;
const ATTEMPT_ID = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/u;

export async function initializeFinalSourceAttempt({
  journalPath,
  attemptId,
  sourceCommit,
}) {
  const path = requirePath(journalPath, "journalPath");
  requireAttemptId(attemptId);
  requireSourceCommit(sourceCommit);
  await ensureSafeDirectory(dirname(path));
  const releaseLock = await acquireJournalLock(path);
  try {
    const header = sealRecord({
      schema_id: FINAL_SOURCE_ATTEMPT_SCHEMA_ID,
      sequence: 0,
      attempt_id: attemptId,
      release: RELEASE,
      source_commit: sourceCommit,
      previous_sha256: null,
    });
    await writeNewFile(path, `${canonicalJson(header)}\n`);
    return header;
  } finally {
    await releaseLock();
  }
}

export async function appendFinalSourceAttemptStage({
  journalPath,
  stageEvidencePath,
  stageEvidenceFileSha256,
}, dependencies = {}) {
  const path = requirePath(journalPath, "journalPath");
  const stageInput = await readCanonicalReport(
    stageEvidencePath,
    "final-source stage evidence",
    stageEvidenceFileSha256,
  );
  const replaceJournal = dependencies.replaceJournal ?? replaceJournalAtomically;
  const releaseLock = await acquireJournalLock(path);
  try {
    const { raw, records } = await readVerifiedJournal(path);
    const header = records[0];
    const completedStages = verifyRecordedStages(records.slice(1), header.source_commit);
    const expectedStage = FINAL_SOURCE_STAGE_ORDER[completedStages.length];
    if (expectedStage === undefined) {
      throw new Error("final-source attempt already contains every stage");
    }
    const stage = validateFinalSourceStageEvidence(stageInput.value, {
      expectedStage,
      expectedSourceCommit: header.source_commit,
    });
    const descriptor = Object.freeze({
      stage: stage.stage,
      schema_id: stage.schema_id,
      report_sha256: stage.report_sha256,
      file_sha256: stageInput.fileSha256,
    });
    const appended = [];
    let previous = records.at(-1);
    for (const entry of stage.events) {
      const record = sealRecord({
        schema_id: FINAL_SOURCE_EVENT_SCHEMA_ID,
        sequence: previous.sequence + 1,
        attempt_id: header.attempt_id,
        release: header.release,
        source_commit: header.source_commit,
        kind: entry.kind,
        payload: entry.payload,
        stage_evidence: descriptor,
        previous_sha256: previous.record_sha256,
      });
      appended.push(record);
      previous = record;
    }
    const batch = `${appended.map((record) => canonicalJson(record)).join("\n")}\n`;
    await replaceJournal(path, raw, batch);
    return Object.freeze(appended);
  } finally {
    await releaseLock();
  }
}

export async function materializeFinalSourceManifest({
  journalPath,
  outputPath,
  sourceRoot,
  acceptanceStageEvidencePath,
  acceptanceStageEvidenceFileSha256,
  deploymentStageEvidencePath,
  deploymentStageEvidenceFileSha256,
  publicationStageEvidencePath,
  publicationStageEvidenceFileSha256,
  canonicalAcceptanceEvidencePath,
  canonicalAcceptanceEvidenceFileSha256,
  pagesDeploymentAuthorityPath,
  pagesDeploymentAuthorityFileSha256,
  pagesRollbackCapturePath,
  pagesRollbackCaptureFileSha256,
  discordCatalogPath,
  discordCatalogFileSha256,
  discordPriorSnapshotPath,
  discordPriorSnapshotFileSha256,
  discordCommandSyncAuthorityPath,
  discordCommandSyncAuthorityFileSha256,
  discordCatalogSyncReportPath,
  discordCatalogSyncReportFileSha256,
  cloudCandidateSmokeReportPath,
  cloudCandidateSmokeReportFileSha256,
  oracleRollbackCapturePath,
  oracleRollbackCaptureFileSha256,
  oracleObservationPath,
  oracleObservationFileSha256,
  productionProbeSpecPath,
  productionProbeSpecFileSha256,
  productionObservationReportPath,
  productionObservationReportFileSha256,
  releasePublicationEvidencePath,
  releasePublicationEvidenceFileSha256,
  releasePublicationFinalAuthorityPath,
  releasePublicationFinalAuthorityFileSha256,
  releasePublicationReceiptPath,
  releasePublicationReceiptFileSha256,
}, dependencies = {}) {
  const path = requirePath(journalPath, "journalPath");
  const target = requirePath(outputPath, "outputPath");
  await ensureSafeDirectory(dirname(target));
  const [acceptance, deployment, publication, sync, observation] =
    await Promise.all([
      readCanonicalReport(acceptanceStageEvidencePath, "acceptance stage evidence", acceptanceStageEvidenceFileSha256),
      readCanonicalReport(deploymentStageEvidencePath, "deployment stage evidence", deploymentStageEvidenceFileSha256),
      readCanonicalReport(publicationStageEvidencePath, "publication stage evidence", publicationStageEvidenceFileSha256),
      readCanonicalReport(discordCatalogSyncReportPath, "Discord command catalog sync report", discordCatalogSyncReportFileSha256),
      readCanonicalReport(productionObservationReportPath, "production observation report", productionObservationReportFileSha256),
    ]);
  const reconstructStages = dependencies.reconstructStages ?? reconstructFinalSourceStages;
  const reconstructed = await reconstructStages({
    expectedSourceCommit: acceptance.value.source_commit,
    sourceRoot,
    submittedStages: [acceptance, deployment, publication],
    canonicalAcceptanceEvidencePath,
    canonicalAcceptanceEvidenceFileSha256,
    pagesDeploymentAuthorityPath,
    pagesDeploymentAuthorityFileSha256,
    pagesRollbackCapturePath,
    pagesRollbackCaptureFileSha256,
    discordCatalogPath,
    discordCatalogFileSha256,
    discordPriorSnapshotPath,
    discordPriorSnapshotFileSha256,
    discordCommandSyncAuthorityPath,
    discordCommandSyncAuthorityFileSha256,
    discordCatalogSyncReportPath,
    discordCatalogSyncReportFileSha256,
    discordCatalogSyncReport: sync,
    cloudCandidateSmokeReportPath,
    cloudCandidateSmokeReportFileSha256,
    oracleRollbackCapturePath,
    oracleRollbackCaptureFileSha256,
    oracleObservationPath,
    oracleObservationFileSha256,
    productionProbeSpecPath,
    productionProbeSpecFileSha256,
    productionObservationReportPath,
    productionObservationReportFileSha256,
    productionObservationReport: observation,
    releasePublicationEvidencePath,
    releasePublicationEvidenceFileSha256,
    releasePublicationFinalAuthorityPath,
    releasePublicationFinalAuthorityFileSha256,
    releasePublicationReceiptPath,
    releasePublicationReceiptFileSha256,
  });
  for (const [index, submitted] of [acceptance, deployment, publication].entries()) {
    if (canonicalJson(submitted.value) !== canonicalJson(reconstructed.stages[index])) {
      throw new Error(
        `${FINAL_SOURCE_STAGE_ORDER[index]} stage differs from its reopened original producers`,
      );
    }
  }
  const releaseLock = await acquireJournalLock(path);
  try {
    const { records } = await readVerifiedJournal(path);
    const header = records[0];
    const stageInputs = [acceptance, deployment, publication];
    for (const [index, input] of stageInputs.entries()) {
      validateFinalSourceStageEvidence(input.value, {
        expectedStage: FINAL_SOURCE_STAGE_ORDER[index],
        expectedSourceCommit: header.source_commit,
      });
    }
    const completedStages = verifyRecordedStages(records.slice(1), header.source_commit);
    if (completedStages.length !== FINAL_SOURCE_STAGE_ORDER.length) {
      throw new Error("final-source attempt is incomplete");
    }
    for (const [index, recorded] of completedStages.entries()) {
      requireExactStageMatch(recorded, stageInputs[index]);
    }
    requireDeploymentProducerMatch(deployment.value, "discord-catalog-sync", sync.value.report_sha256, sync.fileSha256);
    requireDeploymentProducerMatch(deployment.value, "production-observation", observation.value.report_sha256, observation.fileSha256);
    const grouped = groupEvents(records.slice(1));
    const manifest = {
      schema_id: "clearra.final-source-revalidation.v1",
      release: header.release,
      source: onlyPayload(grouped, "source"),
      contracts: onlyPayload(grouped, "contracts"),
      toolchains: onlyPayload(grouped, "toolchains"),
      drift_audits: sortedPayloads(grouped, "drift-audit", "phase"),
      canonical_gate: onlyPayload(grouped, "canonical-gate"),
      surface_reports: sortedPayloads(grouped, "surface-report", "surface"),
      release_artifacts: sortedPayloads(grouped, "release-artifact", "role"),
      deployment: {
        pages: onlyPayload(grouped, "deployment-pages"),
        discord: onlyPayload(grouped, "deployment-discord"),
        rollback_snapshot: onlyPayload(grouped, "rollback-snapshot"),
      },
      observation: onlyPayload(grouped, "observation"),
      tag: onlyPayload(grouped, "tag"),
      immutable_release: onlyPayload(grouped, "immutable-release"),
    };
    validateFinalSourceRevalidationFromStages(manifest, {
      expectedSourceCommit: header.source_commit,
      expectedRelease: header.release,
      acceptanceStageEvidence: acceptance.value,
      acceptanceStageEvidenceFileSha256: acceptance.fileSha256,
      deploymentStageEvidence: deployment.value,
      deploymentStageEvidenceFileSha256: deployment.fileSha256,
      publicationStageEvidence: publication.value,
      publicationStageEvidenceFileSha256: publication.fileSha256,
      discordCatalogSyncReport: sync.value,
      discordCatalogSyncReportFileSha256: sync.fileSha256,
      productionObservationReport: observation.value,
      productionObservationReportFileSha256: observation.fileSha256,
    });
    await writeNewFile(target, `${canonicalJson(manifest)}\n`);
    return manifest;
  } finally {
    await releaseLock();
  }
}

async function reconstructFinalSourceStages(options) {
  const [canonicalAcceptance, pages, pagesRollback, catalog, prior, syncAuthority,
    smoke, oracleCapture, oracleObservation, probeSpec, publication,
    publicationAuthority, receipt] =
    await Promise.all([
      readCanonicalReport(options.canonicalAcceptanceEvidencePath, "canonical acceptance evidence", options.canonicalAcceptanceEvidenceFileSha256),
      readCanonicalReport(options.pagesDeploymentAuthorityPath, "Pages deployment authority", options.pagesDeploymentAuthorityFileSha256),
      readCanonicalReport(options.pagesRollbackCapturePath, "Pages rollback capture", options.pagesRollbackCaptureFileSha256),
      readCanonicalReport(options.discordCatalogPath, "Discord canonical catalog", options.discordCatalogFileSha256),
      readCanonicalReport(options.discordPriorSnapshotPath, "Discord prior snapshot", options.discordPriorSnapshotFileSha256),
      readCanonicalReport(options.discordCommandSyncAuthorityPath, "Discord command sync authority", options.discordCommandSyncAuthorityFileSha256),
      readCanonicalReport(options.cloudCandidateSmokeReportPath, "Cloud candidate smoke report", options.cloudCandidateSmokeReportFileSha256),
      readCanonicalReport(options.oracleRollbackCapturePath, "Oracle rollback capture", options.oracleRollbackCaptureFileSha256),
      readCanonicalReport(options.oracleObservationPath, "Oracle candidate observation", options.oracleObservationFileSha256),
      readCanonicalReport(options.productionProbeSpecPath, "production probe spec", options.productionProbeSpecFileSha256),
      readCanonicalReport(options.releasePublicationEvidencePath, "release publication evidence", options.releasePublicationEvidenceFileSha256),
      readCanonicalReport(options.releasePublicationFinalAuthorityPath, "release publication final authority", options.releasePublicationFinalAuthorityFileSha256),
      readCanonicalReport(options.releasePublicationReceiptPath, "release publication receipt", options.releasePublicationReceiptFileSha256),
    ]);
  const acceptance = await createAcceptanceStageEvidence({
    expectedSourceCommit: options.expectedSourceCommit,
    sourceRoot: options.sourceRoot,
    acceptanceEvidence: canonicalAcceptance.value,
    acceptanceEvidenceFileSha256: canonicalAcceptance.fileSha256,
  });
  const deployment = createDeploymentStageEvidence({
    expectedSourceCommit: options.expectedSourceCommit,
    pagesDeploymentAuthority: pages.value,
    pagesDeploymentAuthorityFileSha256: pages.fileSha256,
    pagesRollbackCapture: pagesRollback.value,
    pagesRollbackCaptureFileSha256: pagesRollback.fileSha256,
    discordCatalog: catalog.value,
    discordCatalogFileSha256: catalog.fileSha256,
    discordPriorSnapshot: prior.value,
    discordPriorSnapshotFileSha256: prior.fileSha256,
    discordCommandSyncAuthority: syncAuthority.value,
    discordCommandSyncAuthorityFileSha256: syncAuthority.fileSha256,
    discordCatalogSyncReport: options.discordCatalogSyncReport.value,
    discordCatalogSyncFileSha256: options.discordCatalogSyncReport.fileSha256,
    cloudCandidateSmokeReport: smoke.value,
    cloudCandidateSmokeFileSha256: smoke.fileSha256,
    oracleRollbackCapture: oracleCapture.value,
    oracleRollbackCaptureFileSha256: oracleCapture.fileSha256,
    oracleObservation: oracleObservation.value,
    oracleObservationFileSha256: oracleObservation.fileSha256,
    productionProbeSpec: probeSpec.value,
    productionProbeSpecFileSha256: probeSpec.fileSha256,
    productionObservationReport: options.productionObservationReport.value,
    productionObservationFileSha256: options.productionObservationReport.fileSha256,
  });
  const publicationStage = createPublicationStageEvidence({
    expectedSourceCommit: options.expectedSourceCommit,
    releasePublicationEvidence: publication.value,
    releasePublicationFileSha256: publication.fileSha256,
    releasePublicationFinalAuthority: publicationAuthority.value,
    releasePublicationFinalAuthorityFileSha256: publicationAuthority.fileSha256,
    releasePublicationReceipt: receipt.value,
    releasePublicationReceiptFileSha256: receipt.fileSha256,
    acceptanceEvidence: canonicalAcceptance.value,
  });
  return Object.freeze({ stages: Object.freeze([acceptance, deployment, publicationStage]) });
}

export function parseFinalSourceAttemptCliArguments(args) {
  if (!Array.isArray(args) || args.length === 0) {
    throw new Error("final-source attempt command is required");
  }
  const command = args[0];
  const specifications = new Map([
    ["initialize", {
      allowed: ["--journal", "--attempt-id", "--source-commit"],
      required: ["--journal", "--attempt-id", "--source-commit"],
    }],
    ["append-stage", {
      allowed: ["--journal", "--stage-evidence", "--stage-evidence-file-sha256"],
      required: ["--journal", "--stage-evidence", "--stage-evidence-file-sha256"],
    }],
    ["materialize", {
      allowed: [
        "--journal", "--output", "--source-root",
        "--acceptance-stage-evidence", "--acceptance-stage-evidence-file-sha256",
        "--deployment-stage-evidence", "--deployment-stage-evidence-file-sha256",
        "--publication-stage-evidence", "--publication-stage-evidence-file-sha256",
        "--canonical-acceptance-evidence", "--canonical-acceptance-evidence-file-sha256",
        "--pages-deployment-authority", "--pages-deployment-authority-file-sha256",
        "--pages-rollback-capture", "--pages-rollback-capture-file-sha256",
        "--discord-catalog", "--discord-catalog-file-sha256",
        "--discord-prior-snapshot", "--discord-prior-snapshot-file-sha256",
        "--discord-command-sync-authority", "--discord-command-sync-authority-file-sha256",
        "--discord-catalog-sync-report", "--discord-catalog-sync-report-file-sha256",
        "--cloud-candidate-smoke-report", "--cloud-candidate-smoke-report-file-sha256",
        "--oracle-rollback-capture", "--oracle-rollback-capture-file-sha256",
        "--oracle-observation", "--oracle-observation-file-sha256",
        "--production-probe-spec", "--production-probe-spec-file-sha256",
        "--production-observation-report", "--production-observation-report-file-sha256",
        "--release-publication-evidence", "--release-publication-evidence-file-sha256",
        "--release-publication-final-authority", "--release-publication-final-authority-file-sha256",
        "--release-publication-receipt", "--release-publication-receipt-file-sha256",
      ],
      required: [
        "--journal", "--output", "--source-root",
        "--acceptance-stage-evidence", "--acceptance-stage-evidence-file-sha256",
        "--deployment-stage-evidence", "--deployment-stage-evidence-file-sha256",
        "--publication-stage-evidence", "--publication-stage-evidence-file-sha256",
        "--canonical-acceptance-evidence", "--canonical-acceptance-evidence-file-sha256",
        "--pages-deployment-authority", "--pages-deployment-authority-file-sha256",
        "--pages-rollback-capture", "--pages-rollback-capture-file-sha256",
        "--discord-catalog", "--discord-catalog-file-sha256",
        "--discord-prior-snapshot", "--discord-prior-snapshot-file-sha256",
        "--discord-command-sync-authority", "--discord-command-sync-authority-file-sha256",
        "--discord-catalog-sync-report", "--discord-catalog-sync-report-file-sha256",
        "--cloud-candidate-smoke-report", "--cloud-candidate-smoke-report-file-sha256",
        "--oracle-rollback-capture", "--oracle-rollback-capture-file-sha256",
        "--oracle-observation", "--oracle-observation-file-sha256",
        "--production-probe-spec", "--production-probe-spec-file-sha256",
        "--production-observation-report", "--production-observation-report-file-sha256",
        "--release-publication-evidence", "--release-publication-evidence-file-sha256",
        "--release-publication-final-authority", "--release-publication-final-authority-file-sha256",
        "--release-publication-receipt", "--release-publication-receipt-file-sha256",
      ],
    }],
  ]);
  const specification = specifications.get(command);
  if (specification === undefined) {
    throw new Error(`unsupported final-source attempt command: ${String(command)}`);
  }
  return { command, values: parseStrictNamedArguments(args.slice(1), specification) };
}

async function readVerifiedJournal(journalPath) {
  const path = resolve(journalPath);
  await assertSafePathChain(dirname(path));
  await assertRegularNonLinkFile(path, "final-source attempt journal");
  const raw = await readFile(path, "utf8");
  if (raw.length === 0 || !raw.endsWith("\n")) {
    throw new Error("final-source attempt journal is empty or torn");
  }
  const lines = raw.slice(0, -1).split("\n");
  const records = lines.map((line, index) => {
    try {
      return JSON.parse(line);
    } catch {
      throw new Error(`final-source attempt journal line ${index + 1} is invalid JSON`);
    }
  });
  verifyHeader(records[0]);
  for (let index = 1; index < records.length; index += 1) {
    verifyEvent(records[index], records[index - 1], records[0], index);
  }
  verifyRecordedStages(records.slice(1), records[0].source_commit);
  return Object.freeze({ raw, records });
}

function verifyHeader(header) {
  requirePlainObject(header, "attempt journal header");
  requireExactKeys(header, [
    "schema_id", "sequence", "attempt_id", "release", "source_commit",
    "previous_sha256", "record_sha256",
  ], "attempt journal header");
  if (header.schema_id !== FINAL_SOURCE_ATTEMPT_SCHEMA_ID || header.sequence !== 0 ||
      header.release !== RELEASE || header.previous_sha256 !== null) {
    throw new Error("attempt journal header identity is invalid");
  }
  requireAttemptId(header.attempt_id);
  requireSourceCommit(header.source_commit);
  verifyRecordHash(header, "attempt journal header");
}

function verifyEvent(event, previous, header, index) {
  requirePlainObject(event, `attempt journal event ${index}`);
  requireExactKeys(event, [
    "schema_id", "sequence", "attempt_id", "release", "source_commit",
    "kind", "payload", "stage_evidence", "previous_sha256", "record_sha256",
  ], `attempt journal event ${index}`);
  if (event.schema_id !== FINAL_SOURCE_EVENT_SCHEMA_ID || event.sequence !== index ||
      event.attempt_id !== header.attempt_id || event.release !== header.release ||
      event.source_commit !== header.source_commit ||
      event.previous_sha256 !== previous.record_sha256) {
    throw new Error(`attempt journal event ${index} identity or chain is invalid`);
  }
  validateFinalSourceEventPayload(event.kind, event.payload, header.source_commit);
  verifyStageDescriptor(event.stage_evidence, event.kind);
  verifyRecordHash(event, `attempt journal event ${index}`);
}

function verifyStageDescriptor(descriptor, kind) {
  requirePlainObject(descriptor, "attempt journal stage descriptor");
  requireExactKeys(descriptor, ["stage", "schema_id", "report_sha256", "file_sha256"], "attempt journal stage descriptor");
  if (descriptor.stage !== stageForFinalSourceEventKind(kind) ||
      descriptor.schema_id !== FINAL_SOURCE_STAGE_EVIDENCE_SCHEMA_ID) {
    throw new Error("attempt journal stage descriptor identity is invalid");
  }
  requireSha256(descriptor.report_sha256, "stage evidence report SHA-256");
  requireSha256(descriptor.file_sha256, "stage evidence file SHA-256");
}

function verifyRecordedStages(events, sourceCommit) {
  if (events.length === 0) return [];
  const completed = [];
  let offset = 0;
  for (const expectedStage of FINAL_SOURCE_STAGE_ORDER) {
    if (offset >= events.length) break;
    const descriptor = events[offset].stage_evidence;
    if (descriptor.stage !== expectedStage) {
      throw new Error("attempt journal stages are not in canonical order");
    }
    const expectedCount = [...FINAL_SOURCE_STAGE_CARDINALITY.get(expectedStage).values()]
      .reduce((sum, count) => sum + count, 0);
    const batch = events.slice(offset, offset + expectedCount);
    if (batch.length !== expectedCount) {
      throw new Error(`${expectedStage} stage journal batch is incomplete`);
    }
    if (batch.some((event) => canonicalJson(event.stage_evidence) !== canonicalJson(descriptor))) {
      throw new Error(`${expectedStage} stage journal batch mixes producer authorities`);
    }
    const expectedKinds = [...FINAL_SOURCE_STAGE_CARDINALITY.get(expectedStage).entries()]
      .flatMap(([kind, count]) => Array.from({ length: count }, () => kind));
    if (canonicalJson(batch.map(({ kind }) => kind)) !== canonicalJson(expectedKinds)) {
      throw new Error(`${expectedStage} stage journal event order is invalid`);
    }
    completed.push(Object.freeze({
      descriptor,
      events: batch.map(({ kind, payload }) => ({ kind, payload })),
    }));
    offset += expectedCount;
  }
  if (offset !== events.length) {
    throw new Error("attempt journal contains events outside the canonical stages");
  }
  return completed;
}

function requireExactStageMatch(recorded, input) {
  const report = input.value;
  if (recorded.descriptor.stage !== report.stage ||
      recorded.descriptor.schema_id !== report.schema_id ||
      recorded.descriptor.report_sha256 !== report.report_sha256 ||
      recorded.descriptor.file_sha256 !== input.fileSha256 ||
      canonicalJson(recorded.events) !== canonicalJson(report.events)) {
    throw new Error(`${report.stage} journal batch differs from its exact stage evidence file`);
  }
}

function requireDeploymentProducerMatch(stage, role, evidenceSha256, fileSha256) {
  const input = stage.producer_inputs.find((entry) => entry.role === role);
  if (input === undefined || input.evidence_sha256 !== evidenceSha256 ||
      input.file_sha256 !== fileSha256) {
    throw new Error(`deployment stage ${role} input differs from materialization authority`);
  }
}

function sealRecord(record) {
  return { ...record, record_sha256: sha256(canonicalJson(record)) };
}

function verifyRecordHash(record, label) {
  if (typeof record.record_sha256 !== "string" || !SHA256.test(record.record_sha256)) {
    throw new Error(`${label} record_sha256 is invalid`);
  }
  const { record_sha256: actual, ...unsigned } = record;
  if (sha256(canonicalJson(unsigned)) !== actual) {
    throw new Error(`${label} hash differs from its canonical content`);
  }
}

function groupEvents(events) {
  const grouped = new Map();
  for (const stage of FINAL_SOURCE_STAGE_CARDINALITY.values()) {
    for (const kind of stage.keys()) grouped.set(kind, []);
  }
  for (const event of events) grouped.get(event.kind).push(event.payload);
  return grouped;
}

function onlyPayload(grouped, kind) {
  return grouped.get(kind)[0];
}

function sortedPayloads(grouped, kind, key) {
  return [...grouped.get(kind)].sort((left, right) =>
    String(left[key]).localeCompare(String(right[key]), "en"));
}

function parseStrictNamedArguments(args, { allowed, required }) {
  const allowedSet = new Set(allowed);
  const values = {};
  for (let index = 0; index < args.length; index += 1) {
    const option = args[index];
    if (!allowedSet.has(option)) {
      throw new Error(`unsupported final-source attempt argument: ${String(option)}`);
    }
    if (Object.hasOwn(values, option)) {
      throw new Error(`duplicate final-source attempt argument: ${option}`);
    }
    const value = args[index + 1];
    if (typeof value !== "string" || value.length === 0 || value.startsWith("--")) {
      throw new Error(`${option} requires one value`);
    }
    values[option] = value;
    index += 1;
  }
  for (const option of required) {
    if (!Object.hasOwn(values, option)) throw new Error(`${option} is required`);
  }
  return values;
}

function requireAttemptId(attemptId) {
  if (typeof attemptId !== "string" || !ATTEMPT_ID.test(attemptId)) {
    throw new Error("attemptId must be a bounded portable identifier");
  }
}

function requireSourceCommit(sourceCommit) {
  if (typeof sourceCommit !== "string" || !SHA1.test(sourceCommit)) {
    throw new Error("sourceCommit must be a full lowercase SHA-1 commit");
  }
  return sourceCommit;
}

function requirePath(value, label) {
  if (typeof value !== "string" || value.length === 0 || value.includes("\0")) {
    throw new Error(`${label} must be a non-empty filesystem path`);
  }
  return resolve(value);
}

function sha256(value) {
  return createHash("sha256").update(value, "utf8").digest("hex");
}

function requireExactKeys(value, expected, label) {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (actual.length !== wanted.length || actual.some((key, index) => key !== wanted[index])) {
    throw new Error(`${label} fields differ from the closed schema`);
  }
}

function requirePlainObject(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
}

async function readCanonicalReport(path, label, expectedFileSha256) {
  const target = requirePath(path, `${label} path`);
  await assertSafePathChain(dirname(target));
  await assertRegularNonLinkFile(target, label);
  const raw = await readFile(target, "utf8");
  let value;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new Error(`${label} is not valid JSON`);
  }
  if (raw !== `${canonicalJson(value)}\n`) {
    throw new Error(`${label} bytes are not canonical producer JSON`);
  }
  const fileSha256 = sha256(raw);
  if (fileSha256 !== requireSha256(expectedFileSha256, `${label} file SHA-256`)) {
    throw new Error(`${label} raw file SHA-256 differs from the requested authority`);
  }
  return Object.freeze({ value, fileSha256 });
}

async function replaceJournalAtomically(path, original, batch) {
  const temporary = `${path}.next-${process.pid}-${randomUUID()}`;
  let handle;
  try {
    handle = await open(temporary, "wx", 0o600);
    await handle.writeFile(`${original}${batch}`, "utf8");
    await handle.sync();
    await handle.close();
    handle = undefined;
    await rename(temporary, path);
  } catch (error) {
    await handle?.close().catch(() => undefined);
    await unlink(temporary).catch(() => undefined);
    throw error;
  }
}

async function writeNewFile(path, contents) {
  const handle = await open(path, "wx", 0o600);
  try {
    await handle.writeFile(contents, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }
}

async function acquireJournalLock(journalPath) {
  const lockPath = `${journalPath}.lock`;
  let handle;
  try {
    handle = await open(lockPath, "wx", 0o600);
  } catch (error) {
    if (error?.code === "EEXIST") {
      throw new Error("final-source attempt journal has a concurrent writer");
    }
    throw error;
  }
  try {
    await handle.writeFile(`${process.pid}\n`, "utf8");
    await handle.sync();
  } catch (error) {
    await handle.close();
    await unlink(lockPath).catch(() => undefined);
    throw error;
  }
  return async () => {
    await handle.close();
    await unlink(lockPath);
  };
}

async function ensureSafeDirectory(directory) {
  const missing = [];
  let current = resolve(directory);
  for (;;) {
    try {
      await lstat(current);
      break;
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
      missing.push(current);
      const parent = dirname(current);
      if (parent === current) throw error;
      current = parent;
    }
  }
  await assertSafePathChain(current);
  for (const path of missing.reverse()) {
    try {
      await mkdir(path, { mode: 0o700 });
    } catch (error) {
      if (error?.code !== "EEXIST") throw error;
    }
    const status = await lstat(path);
    if (!status.isDirectory() || status.isSymbolicLink()) {
      throw new Error(`release evidence path uses a non-directory or link: ${path}`);
    }
  }
}

async function assertSafePathChain(directory) {
  let current = resolve(directory);
  for (;;) {
    const status = await lstat(current);
    if (!status.isDirectory() || status.isSymbolicLink()) {
      throw new Error(`release evidence path uses a non-directory or link: ${current}`);
    }
    const parent = dirname(current);
    if (parent === current) break;
    current = parent;
  }
}

async function assertRegularNonLinkFile(path, label) {
  const status = await lstat(path);
  if (!status.isFile() || status.isSymbolicLink()) {
    throw new Error(`${label} is not a regular non-link file: ${path}`);
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    const { command, values } = parseFinalSourceAttemptCliArguments(process.argv.slice(2));
    if (command === "initialize") {
      await initializeFinalSourceAttempt({
        journalPath: values["--journal"],
        attemptId: values["--attempt-id"],
        sourceCommit: values["--source-commit"],
      });
    } else if (command === "append-stage") {
      await appendFinalSourceAttemptStage({
        journalPath: values["--journal"],
        stageEvidencePath: values["--stage-evidence"],
        stageEvidenceFileSha256: values["--stage-evidence-file-sha256"],
      });
    } else {
      await materializeFinalSourceManifest({
        journalPath: values["--journal"],
        outputPath: values["--output"],
        sourceRoot: values["--source-root"],
        acceptanceStageEvidencePath: values["--acceptance-stage-evidence"],
        acceptanceStageEvidenceFileSha256: values["--acceptance-stage-evidence-file-sha256"],
        deploymentStageEvidencePath: values["--deployment-stage-evidence"],
        deploymentStageEvidenceFileSha256: values["--deployment-stage-evidence-file-sha256"],
        publicationStageEvidencePath: values["--publication-stage-evidence"],
        publicationStageEvidenceFileSha256: values["--publication-stage-evidence-file-sha256"],
        canonicalAcceptanceEvidencePath: values["--canonical-acceptance-evidence"],
        canonicalAcceptanceEvidenceFileSha256: values["--canonical-acceptance-evidence-file-sha256"],
        pagesDeploymentAuthorityPath: values["--pages-deployment-authority"],
        pagesDeploymentAuthorityFileSha256: values["--pages-deployment-authority-file-sha256"],
        pagesRollbackCapturePath: values["--pages-rollback-capture"],
        pagesRollbackCaptureFileSha256: values["--pages-rollback-capture-file-sha256"],
        discordCatalogPath: values["--discord-catalog"],
        discordCatalogFileSha256: values["--discord-catalog-file-sha256"],
        discordPriorSnapshotPath: values["--discord-prior-snapshot"],
        discordPriorSnapshotFileSha256: values["--discord-prior-snapshot-file-sha256"],
        discordCommandSyncAuthorityPath: values["--discord-command-sync-authority"],
        discordCommandSyncAuthorityFileSha256: values["--discord-command-sync-authority-file-sha256"],
        discordCatalogSyncReportPath: values["--discord-catalog-sync-report"],
        discordCatalogSyncReportFileSha256: values["--discord-catalog-sync-report-file-sha256"],
        cloudCandidateSmokeReportPath: values["--cloud-candidate-smoke-report"],
        cloudCandidateSmokeReportFileSha256: values["--cloud-candidate-smoke-report-file-sha256"],
        oracleRollbackCapturePath: values["--oracle-rollback-capture"],
        oracleRollbackCaptureFileSha256: values["--oracle-rollback-capture-file-sha256"],
        oracleObservationPath: values["--oracle-observation"],
        oracleObservationFileSha256: values["--oracle-observation-file-sha256"],
        productionProbeSpecPath: values["--production-probe-spec"],
        productionProbeSpecFileSha256: values["--production-probe-spec-file-sha256"],
        productionObservationReportPath: values["--production-observation-report"],
        productionObservationReportFileSha256: values["--production-observation-report-file-sha256"],
        releasePublicationEvidencePath: values["--release-publication-evidence"],
        releasePublicationEvidenceFileSha256: values["--release-publication-evidence-file-sha256"],
        releasePublicationFinalAuthorityPath: values["--release-publication-final-authority"],
        releasePublicationFinalAuthorityFileSha256: values["--release-publication-final-authority-file-sha256"],
        releasePublicationReceiptPath: values["--release-publication-receipt"],
        releasePublicationReceiptFileSha256: values["--release-publication-receipt-file-sha256"],
      });
    }
    process.stdout.write(`${FINAL_SOURCE_ATTEMPT_SCHEMA_ID}\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 2;
  }
}
