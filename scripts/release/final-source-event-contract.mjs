import {
  canonicalTimestamp,
  rejectSecretMaterial,
  requireExactKeys,
  requireNonEmptyString,
  requireSha256,
  requireSourceCommit,
  sealCanonicalReport,
  verifyCanonicalReportHash,
} from "./canonical-release-evidence.mjs";

export const FINAL_SOURCE_EVENT_EVIDENCE_SCHEMA_ID =
  "clearra.final-source-event-evidence.v1";

export const FINAL_SOURCE_STAGE_ORDER = Object.freeze([
  "acceptance",
  "deployment",
  "publication",
]);

export const FINAL_SOURCE_STAGE_CARDINALITY = Object.freeze(new Map([
  ["acceptance", Object.freeze(new Map([
    ["source", 1],
    ["contracts", 1],
    ["toolchains", 1],
    ["drift-audit", 2],
    ["canonical-gate", 1],
    ["surface-report", 4],
    ["release-artifact", 3],
  ]))],
  ["deployment", Object.freeze(new Map([
    ["deployment-pages", 1],
    ["deployment-discord", 1],
    ["rollback-snapshot", 1],
    ["observation", 1],
  ]))],
  ["publication", Object.freeze(new Map([
    ["tag", 1],
    ["immutable-release", 1],
  ]))],
]));

export const FINAL_SOURCE_EVENT_KINDS = Object.freeze(
  [...FINAL_SOURCE_STAGE_CARDINALITY.values()]
    .flatMap((cardinality) => [...cardinality.keys()]),
);

const SHA256 = /^[0-9a-f]{64}$/u;
const GIT_OBJECT_ID = /^(?:[0-9a-f]{40}|[0-9a-f]{64})$/u;
const IMAGE_DIGEST = /^sha256:[0-9a-f]{64}$/u;
const DECIMAL_ID = /^[1-9][0-9]*$/u;
const DISCORD_SNOWFLAKE = /^\d{17,20}$/u;
const FORBIDDEN_PRIOR_AUTHORITY = /(?:^|[^0-9])v?0\.7\.5(?:[^0-9]|$)/iu;
const REQUIRED_SURFACES = Object.freeze(["desktop", "discord", "native", "wasm"]);
const REQUIRED_ARTIFACT_NAMES = Object.freeze(new Map([
  ["linux-cli", "Clearra-CLI-v0.8.0-linux-x86_64"],
  ["windows-cli", "Clearra-CLI-v0.8.0-windows-x86_64.exe"],
  ["windows-gui", "Clearra-GUI-v0.8.0-windows-x86_64.exe"],
]));

export function stageForFinalSourceEventKind(kind) {
  for (const [stage, cardinality] of FINAL_SOURCE_STAGE_CARDINALITY) {
    if (cardinality.has(kind)) return stage;
  }
  throw new Error(`unsupported final-source event kind: ${String(kind)}`);
}

export function createFinalSourceEventEvidence({
  sourceCommit,
  kind,
  payload,
  producerSchemaId,
  producerReportSha256,
}) {
  const commit = requireSourceCommit(sourceCommit, "event evidence source commit");
  const stage = stageForFinalSourceEventKind(kind);
  validateFinalSourceEventPayload(kind, payload, commit);
  requireNonEmptyString(producerSchemaId, "event evidence producer schema");
  requireSha256(producerReportSha256, "event evidence producer report SHA-256");
  const report = sealCanonicalReport({
    schema_id: FINAL_SOURCE_EVENT_EVIDENCE_SCHEMA_ID,
    stage,
    source_commit: commit,
    kind,
    producer: {
      schema_id: producerSchemaId,
      report_sha256: producerReportSha256,
    },
    payload,
    status: "passed",
  });
  validateFinalSourceEventEvidence(report, {
    expectedSourceCommit: commit,
    expectedKind: kind,
  });
  return report;
}

export function validateFinalSourceEventEvidence(
  value,
  { expectedSourceCommit, expectedKind, expectedProducerSchemaId } = {},
) {
  requireExactKeys(value, [
    "schema_id",
    "stage",
    "source_commit",
    "kind",
    "producer",
    "payload",
    "status",
    "report_sha256",
  ], "final-source event evidence");
  if (value.schema_id !== FINAL_SOURCE_EVENT_EVIDENCE_SCHEMA_ID) {
    throw new Error("final-source event evidence schema is invalid");
  }
  verifyCanonicalReportHash(value, "final-source event evidence");
  requireSourceCommit(value.source_commit, "event evidence source commit");
  if (
    expectedSourceCommit !== undefined &&
    value.source_commit !== expectedSourceCommit
  ) {
    throw new Error("final-source event evidence source differs from the attempt");
  }
  if (expectedKind !== undefined && value.kind !== expectedKind) {
    throw new Error("final-source event evidence kind differs from its adapter role");
  }
  if (value.stage !== stageForFinalSourceEventKind(value.kind)) {
    throw new Error("final-source event evidence stage differs from its kind");
  }
  requireExactKeys(value.producer, [
    "schema_id",
    "report_sha256",
  ], "final-source event evidence producer");
  requireNonEmptyString(value.producer.schema_id, "event evidence producer schema");
  requireSha256(
    value.producer.report_sha256,
    "event evidence producer report SHA-256",
  );
  if (
    expectedProducerSchemaId !== undefined &&
    value.producer.schema_id !== expectedProducerSchemaId
  ) {
    throw new Error("final-source event evidence producer schema is not approved");
  }
  if (value.status !== "passed") {
    throw new Error("final-source event evidence did not pass");
  }
  validateFinalSourceEventPayload(value.kind, value.payload, value.source_commit);
  rejectPriorAuthority(value);
  rejectSecretMaterial(value, "final-source event evidence");
  return value;
}

export function validateFinalSourceEventPayload(kind, payload, sourceCommit) {
  const commit = requireSourceCommit(sourceCommit, "final-source event source commit");
  if (payload === null || typeof payload !== "object" || Array.isArray(payload)) {
    throw new Error(`${String(kind)} final-source payload must be an object`);
  }
  rejectPriorAuthority(payload);
  rejectSecretMaterial(payload, `${String(kind)} final-source payload`);
  switch (kind) {
    case "source":
      validateSource(payload, commit);
      break;
    case "contracts":
      validateContracts(payload, commit);
      break;
    case "toolchains":
      validateToolchains(payload, commit);
      break;
    case "drift-audit":
      validateDriftAudit(payload, commit);
      break;
    case "canonical-gate":
      validateCanonicalGate(payload, commit);
      break;
    case "surface-report":
      validateSurfaceReport(payload, commit);
      break;
    case "release-artifact":
      validateReleaseArtifact(payload, commit);
      break;
    case "deployment-pages":
      validatePagesDeployment(payload, commit);
      break;
    case "deployment-discord":
      validateDiscordDeployment(payload, commit);
      break;
    case "rollback-snapshot":
      validateRollbackSnapshot(payload, commit);
      break;
    case "observation":
      validateObservation(payload, commit);
      break;
    case "tag":
      validateTag(payload, commit);
      break;
    case "immutable-release":
      validateImmutableRelease(payload, commit);
      break;
    default:
      throw new Error(`unsupported final-source event kind: ${String(kind)}`);
  }
  return payload;
}

function validateSource(value, commit) {
  requireExactKeys(value, [
    "commit",
    "tree",
    "branch",
    "worktree_clean",
    "engine_build_id",
  ], "source event payload");
  if (
    value.commit !== commit ||
    value.engine_build_id !== commit ||
    !GIT_OBJECT_ID.test(value.tree) ||
    value.branch !== "main" ||
    value.worktree_clean !== true
  ) {
    throw new Error("source event payload differs from the exact clean main source");
  }
}

function validateContracts(value, commit) {
  requireExactKeys(value, [
    "source_commit",
    "product_registry_schema_id",
    "product_registry_sha256",
    "search_option_contract_sha256",
    "legacy_alias_contract_sha256",
    "ctk3_contract_sha256",
    "readiness_open_count",
  ], "contracts event payload");
  requireSameCommit(value.source_commit, commit, "contracts event");
  requireNonEmptyString(
    value.product_registry_schema_id,
    "product registry schema ID",
  );
  for (const key of [
    "product_registry_sha256",
    "search_option_contract_sha256",
    "legacy_alias_contract_sha256",
    "ctk3_contract_sha256",
  ]) requireSha256(value[key], `contracts ${key}`);
  if (value.readiness_open_count !== 0) {
    throw new Error("contracts event must record zero readiness blockers");
  }
}

function validateToolchains(value, commit) {
  requireExactKeys(value, [
    "source_commit",
    "manifest_sha256",
    "rust",
    "node",
    "wasm_bindgen",
  ], "toolchains event payload");
  requireSameCommit(value.source_commit, commit, "toolchains event");
  requireSha256(value.manifest_sha256, "toolchain manifest SHA-256");
  for (const key of ["rust", "node", "wasm_bindgen"]) {
    requireNonEmptyString(value[key], `toolchain ${key}`);
  }
}

function validateDriftAudit(value, commit) {
  requireExactKeys(value, [
    "id",
    "sha256",
    "source_commit",
    "phase",
    "status",
  ], "drift-audit event payload");
  validateEvidenceReference(value, commit, "drift-audit event");
  if (
    !new Set(["implementation-start", "release-freeze"]).has(value.phase) ||
    value.status !== "no-drift"
  ) {
    throw new Error("drift-audit event is not a closed no-drift phase");
  }
}

function validateCanonicalGate(value, commit) {
  requireExactKeys(value, [
    "id",
    "sha256",
    "source_commit",
    "status",
    "readiness_open_count",
  ], "canonical-gate event payload");
  validateEvidenceReference(value, commit, "canonical-gate event");
  if (value.status !== "passed" || value.readiness_open_count !== 0) {
    throw new Error("canonical-gate event did not close with readiness zero");
  }
}

function validateSurfaceReport(value, commit) {
  requireExactKeys(value, [
    "id",
    "sha256",
    "source_commit",
    "surface",
    "status",
  ], "surface-report event payload");
  validateEvidenceReference(value, commit, "surface-report event");
  if (!REQUIRED_SURFACES.includes(value.surface) || value.status !== "passed") {
    throw new Error("surface-report event is not an approved passed surface");
  }
}

function validateReleaseArtifact(value, commit) {
  requireExactKeys(value, [
    "role",
    "name",
    "sha256",
    "size_bytes",
    "source_commit",
  ], "release-artifact event payload");
  const expectedName = REQUIRED_ARTIFACT_NAMES.get(value.role);
  if (expectedName === undefined || value.name !== expectedName) {
    throw new Error("release-artifact event name or role is not canonical");
  }
  requireSha256(value.sha256, "release artifact SHA-256");
  requirePositiveSafeInteger(value.size_bytes, "release artifact size");
  requireSameCommit(value.source_commit, commit, "release-artifact event");
}

function validatePagesDeployment(value, commit) {
  requireExactKeys(value, [
    "source_commit",
    "deployment_id",
    "artifact_sha256",
    "status",
  ], "deployment-pages event payload");
  requireSameCommit(value.source_commit, commit, "Pages deployment event");
  requireNonEmptyString(value.deployment_id, "Pages deployment ID");
  requireSha256(value.artifact_sha256, "Pages artifact SHA-256");
  if (value.status !== "active") {
    throw new Error("Pages deployment event is not active");
  }
}

function validateDiscordDeployment(value, commit) {
  requireExactKeys(value, [
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
  ], "deployment-discord event payload");
  requireSameCommit(value.source_commit, commit, "Discord deployment event");
  requirePattern(value.application_id, DISCORD_SNOWFLAKE, "Discord application ID");
  requirePattern(value.image_digest, IMAGE_DIGEST, "Cloud image digest");
  requireNonEmptyString(value.job_revision, "Cloud job revision");
  requireNonEmptyString(value.oracle_revision, "Oracle release ID");
  for (const key of [
    "oracle_release_sha256",
    "oracle_settings_sha256",
    "command_catalog_sha256",
    "command_catalog_prior_snapshot_sha256",
    "command_catalog_readback_sha256",
    "command_catalog_sync_report_sha256",
  ]) requireSha256(value[key], `Discord deployment ${key}`);
  if (
    value.traffic_percent !== 100 ||
    value.catalog_synced !== true ||
    value.status !== "active"
  ) {
    throw new Error("Discord deployment event is not fully active and synchronized");
  }
}

function validateRollbackSnapshot(value, commit) {
  requireExactKeys(value, [
    "id",
    "sha256",
    "source_commit",
    "status",
  ], "rollback-snapshot event payload");
  validateEvidenceReference(value, commit, "rollback-snapshot event");
  if (value.status !== "captured") {
    throw new Error("rollback-snapshot event was not captured");
  }
}

function validateObservation(value, commit) {
  requireExactKeys(value, [
    "report_schema_id",
    "source_commit",
    "started_at",
    "ended_at",
    "duration_seconds",
    "probe_spec_sha256",
    "status",
    "report_sha256",
  ], "observation event payload");
  requireSameCommit(value.source_commit, commit, "observation event");
  if (value.report_schema_id !== "clearra.production-observation.v1") {
    throw new Error("observation event producer schema is invalid");
  }
  const startedAt = canonicalTimestamp(value.started_at, "observation start time");
  const endedAt = canonicalTimestamp(value.ended_at, "observation end time");
  if (
    !Number.isSafeInteger(value.duration_seconds) ||
    value.duration_seconds < 1200 ||
    Math.floor((Date.parse(endedAt) - Date.parse(startedAt)) / 1000) !==
      value.duration_seconds ||
    value.status !== "passed"
  ) {
    throw new Error("observation event did not prove the full production window");
  }
  requireSha256(value.probe_spec_sha256, "observation probe-spec SHA-256");
  requireSha256(value.report_sha256, "observation report SHA-256");
}

function validateTag(value, commit) {
  requireExactKeys(value, [
    "name",
    "target_commit",
    "annotated",
    "remote_verified",
  ], "tag event payload");
  if (
    value.name !== "v0.8.0" ||
    value.target_commit !== commit ||
    value.annotated !== true ||
    value.remote_verified !== true
  ) {
    throw new Error("tag event is not the remotely verified annotated v0.8.0 tag");
  }
}

function validateImmutableRelease(value, commit) {
  requireExactKeys(value, [
    "tag",
    "source_commit",
    "workflow_run_id",
    "immutable",
    "asset_count",
    "status",
  ], "immutable-release event payload");
  requireSameCommit(value.source_commit, commit, "immutable-release event");
  if (
    value.tag !== "v0.8.0" ||
    !DECIMAL_ID.test(value.workflow_run_id) ||
    value.immutable !== true ||
    value.asset_count !== 3 ||
    value.status !== "published"
  ) {
    throw new Error("immutable-release event is not the exact published release");
  }
}

function validateEvidenceReference(value, commit, label) {
  requireNonEmptyString(value.id, `${label} ID`);
  requireSha256(value.sha256, `${label} SHA-256`);
  requireSameCommit(value.source_commit, commit, label);
}

function requireSameCommit(value, commit, label) {
  if (value !== commit) {
    throw new Error(`${label} source commit differs from the final source`);
  }
}

function requirePositiveSafeInteger(value, label) {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${label} must be a positive safe integer`);
  }
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new Error(`${label} is invalid`);
  }
}

function rejectPriorAuthority(value, path = "final-source evidence") {
  if (typeof value === "string") {
    if (FORBIDDEN_PRIOR_AUTHORITY.test(value)) {
      throw new Error(`${path} reuses a v0.7.5 authority identity`);
    }
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((entry, index) =>
      rejectPriorAuthority(entry, `${path}[${index}]`));
    return;
  }
  if (value === null || typeof value !== "object") return;
  for (const [key, nested] of Object.entries(value)) {
    rejectPriorAuthority(nested, `${path}.${key}`);
  }
}
