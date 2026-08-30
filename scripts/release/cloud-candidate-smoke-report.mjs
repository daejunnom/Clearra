import {
  canonicalTimestamp,
  rejectSecretMaterial,
  requireExactKeys,
  requireSha256,
  requireSourceCommit,
  verifyCanonicalReportHash,
} from "./canonical-release-evidence.mjs";

export const CLOUD_CANDIDATE_SMOKE_SCHEMA_ID =
  "clearra.cloud.candidate-smoke.v1";

const IDENTIFIER = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/u;
const PROJECT_ID = /^[a-z][a-z0-9-]{4,28}[a-z0-9]$/u;
const REGION = /^[a-z]+(?:-[a-z0-9]+)+[0-9]$/u;
const IMAGE_DIGEST = /^sha256:[0-9a-f]{64}$/u;
const MANAGED_JOB_ID = /^candidate-smoke-[0-9a-f]{12}-[0-9a-z]+$/u;
const SOLUTION_SET_HASH = /^cts1:[0-9a-f]{16}$/u;

export function validateCloudCandidateSmokeReport(
  value,
  { expectedSourceCommit } = {},
) {
  requireExactKeys(value, [
    "schema_id",
    "source_commit",
    "project_id",
    "region",
    "service_name",
    "candidate_revision",
    "candidate_tag",
    "candidate_url",
    "image_digest",
    "started_at",
    "ended_at",
    "smoke_job",
    "execution_name",
    "job_id",
    "zero_traffic_verified",
    "service_readback_sha256",
    "revision_readback_sha256",
    "execution_readback_sha256",
    "solution_set_hash",
    "status",
    "report_sha256",
  ], "Cloud candidate smoke report");
  if (value.schema_id !== CLOUD_CANDIDATE_SMOKE_SCHEMA_ID) {
    throw new Error("Cloud candidate smoke report schema is invalid");
  }
  verifyCanonicalReportHash(value, "Cloud candidate smoke report");
  requireSourceCommit(value.source_commit, "Cloud candidate smoke source commit");
  if (expectedSourceCommit !== undefined && value.source_commit !== expectedSourceCommit) {
    throw new Error("Cloud candidate smoke source differs from the release source");
  }
  requirePattern(value.project_id, PROJECT_ID, "candidate smoke project ID");
  requirePattern(value.region, REGION, "candidate smoke region");
  requirePattern(value.service_name, IDENTIFIER, "candidate smoke service name");
  requirePattern(value.candidate_revision, IDENTIFIER, "candidate smoke revision");
  requirePattern(value.candidate_tag, IDENTIFIER, "candidate smoke tag");
  requireCredentialFreeHttpsOrigin(value.candidate_url);
  requirePattern(value.image_digest, IMAGE_DIGEST, "candidate smoke image digest");
  const sourcePrefix = value.source_commit.slice(0, 7);
  if (
    value.service_name !== "clearra-current-job" ||
    value.region !== "asia-northeast1" ||
    value.candidate_revision !== `clearra-current-job-v080-${sourcePrefix}` ||
    value.candidate_tag !== `candidate-${sourcePrefix}` ||
    value.smoke_job !== `clearra-v080-candidate-smoke-${sourcePrefix}`
  ) {
    throw new Error("Cloud candidate smoke revision/tag differs from the source prefix");
  }
  const startedAt = canonicalTimestamp(value.started_at, "candidate smoke start time");
  const endedAt = canonicalTimestamp(value.ended_at, "candidate smoke end time");
  if (Date.parse(endedAt) < Date.parse(startedAt)) {
    throw new Error("Cloud candidate smoke timestamps are reversed");
  }
  if (
    typeof value.execution_name !== "string" ||
    !IDENTIFIER.test(value.execution_name) ||
    typeof value.job_id !== "string" ||
    !MANAGED_JOB_ID.test(value.job_id) ||
    value.zero_traffic_verified !== true ||
    typeof value.solution_set_hash !== "string" ||
    !SOLUTION_SET_HASH.test(value.solution_set_hash) ||
    value.status !== "passed"
  ) {
    throw new Error("Cloud candidate smoke report did not pass its bounded job contract");
  }
  requireSha256(value.service_readback_sha256, "candidate service readback SHA-256");
  requireSha256(value.revision_readback_sha256, "candidate revision readback SHA-256");
  requireSha256(value.execution_readback_sha256, "candidate execution readback SHA-256");
  rejectSecretMaterial(value, "Cloud candidate smoke report");
  return value;
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new Error(`${label} is invalid`);
  }
}

function requireCredentialFreeHttpsOrigin(value) {
  let url;
  try {
    url = new URL(String(value ?? ""));
  } catch {
    throw new Error("candidate smoke URL is invalid");
  }
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    url.pathname !== "/" ||
    !url.hostname.endsWith(".run.app") ||
    url.origin !== value
  ) {
    throw new Error("candidate smoke URL must be a canonical credential-free HTTPS origin");
  }
}
