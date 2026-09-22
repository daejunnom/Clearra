#!/usr/bin/env node

import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  canonicalJson,
  sha256,
  validateFastCorrectionPlan,
} from "./fast-correction-plan.mjs";

export const FAST_CORRECTION_EVIDENCE_SCHEMA = "clearra.fast-correction-evidence.v1";

const DECIMAL_ID = /^[1-9][0-9]*$/u;
const SHA256 = /^[0-9a-f]{64}$/u;
const ARTIFACT_DIGEST = /^sha256:[0-9a-f]{64}$/u;
const OUTCOMES = new Set(["full-required", "passed-no-deploy", "pages-deployed"]);
const EVIDENCE_FIELDS = Object.freeze([
  "accepted_base",
  "affected_products",
  "candidate_commit",
  "decision",
  "diff_sha256",
  "evidence_sha256",
  "fast_workflow",
  "lane",
  "outcome",
  "owner_manifest_path",
  "owner_manifest_sha256",
  "pages_deployment",
  "plan_sha256",
  "reason_codes",
  "schema_id",
  "selected",
  "skipped_products",
  "status"
]);

export function createFastCorrectionEvidence({
  plan,
  baseAcceptedRunId,
  baseAcceptedRunAttempt,
  workflowRunId,
  workflowRunAttempt,
  outcome,
  pagesDeployment = null,
}) {
  validateFastCorrectionPlan(plan);
  const normalizedOutcome = requireOutcome(outcome, plan);
  const acceptedRunId = requireId(baseAcceptedRunId, "base accepted run ID");
  const acceptedRunAttempt = requireId(baseAcceptedRunAttempt, "base accepted run attempt");
  const runId = requireId(workflowRunId, "fast workflow run ID");
  const runAttempt = requireId(workflowRunAttempt, "fast workflow run attempt");
  const normalizedPages = normalizePagesDeployment(pagesDeployment, {
    required: normalizedOutcome === "pages-deployed",
    candidateCommit: plan.candidate_commit,
    workflowRunId: runId,
  });
  const status = normalizedOutcome === "full-required" ? "blocked" : "passed";
  const body = {
    schema_id: FAST_CORRECTION_EVIDENCE_SCHEMA,
    accepted_base: {
      source_commit: plan.accepted_base_commit,
      tag: plan.accepted_base_tag,
      workflow_path: ".github/workflows/release-cli.yml",
      run_id: acceptedRunId,
      run_attempt: acceptedRunAttempt,
    },
    candidate_commit: plan.candidate_commit,
    fast_workflow: {
      workflow_path: ".github/workflows/fast-correction.yml",
      run_id: runId,
      run_attempt: runAttempt,
    },
    decision: plan.decision,
    lane: plan.lane,
    outcome: normalizedOutcome,
    status,
    owner_manifest_path: plan.owner_manifest_path,
    owner_manifest_sha256: plan.owner_manifest_sha256,
    diff_sha256: plan.diff_sha256,
    plan_sha256: plan.plan_sha256,
    reason_codes: [...plan.reason_codes],
    affected_products: [...plan.affected_products],
    skipped_products: [...plan.skipped_products],
    selected: structuredClone(plan.selected),
    pages_deployment: normalizedPages,
  };
  return Object.freeze({
    ...body,
    evidence_sha256: sha256(`${canonicalJson(body)}\n`),
  });
}

export function validateFastCorrectionEvidence(value, { expectedCandidateCommit } = {}) {
  requireExactKeys(value, EVIDENCE_FIELDS, "fast correction evidence");
  if (value.schema_id !== FAST_CORRECTION_EVIDENCE_SCHEMA) {
    throw new Error("fast correction evidence schema mismatch");
  }
  const { evidence_sha256: claimed, ...body } = value;
  requireHash(claimed, "fast correction evidence");
  if (sha256(`${canonicalJson(body)}\n`) !== claimed) {
    throw new Error("fast correction evidence hash mismatch");
  }
  if (expectedCandidateCommit !== undefined && value.candidate_commit !== expectedCandidateCommit) {
    throw new Error("fast correction evidence candidate mismatch");
  }
  if (!OUTCOMES.has(value.outcome)) throw new Error("fast correction evidence outcome is invalid");
  const expectedStatus = value.outcome === "full-required" ? "blocked" : "passed";
  if (value.status !== expectedStatus) throw new Error("fast correction evidence status mismatch");
  requireHash(value.diff_sha256, "fast correction diff");
  requireHash(value.owner_manifest_sha256, "owner manifest");
  requireHash(value.plan_sha256, "fast correction plan");
  requireId(value.accepted_base?.run_id, "base accepted run ID");
  requireId(value.accepted_base?.run_attempt, "base accepted run attempt");
  requireId(value.fast_workflow?.run_id, "fast workflow run ID");
  requireId(value.fast_workflow?.run_attempt, "fast workflow run attempt");
  if (value.accepted_base?.workflow_path !== ".github/workflows/release-cli.yml") {
    throw new Error("fast correction base workflow path mismatch");
  }
  if (value.fast_workflow?.workflow_path !== ".github/workflows/fast-correction.yml") {
    throw new Error("fast correction workflow path mismatch");
  }
  if (value.outcome === "pages-deployed") {
    normalizePagesDeployment(value.pages_deployment, {
      required: true,
      candidateCommit: value.candidate_commit,
      workflowRunId: String(value.fast_workflow.run_id),
    });
  } else if (value.pages_deployment !== null) {
    throw new Error("non-Pages fast correction must not claim a Pages deployment");
  }
  rejectSecretMaterial(value);
  return value;
}

function requireOutcome(value, plan) {
  if (!OUTCOMES.has(value)) throw new Error("fast correction outcome is invalid");
  if (plan.decision === "full-required" && value !== "full-required") {
    throw new Error("full-required plan cannot produce passing evidence");
  }
  if (plan.decision === "no-op" && value !== "passed-no-deploy") {
    throw new Error("no-op plan cannot deploy");
  }
  if (plan.decision === "fast-eligible") {
    const expected = plan.deploy_pages ? "pages-deployed" : "passed-no-deploy";
    if (value !== expected) throw new Error("fast correction outcome differs from its closed plan");
  }
  return value;
}

function normalizePagesDeployment(value, { required, candidateCommit, workflowRunId }) {
  if (!required) {
    if (value !== null && value !== undefined) throw new Error("Pages deployment evidence is unexpected");
    return null;
  }
  requireExactKeys(value, [
    "build_artifact_digest",
    "build_artifact_id",
    "deployment_id",
    "live_identity_sha256",
    "page_url",
    "pages_artifact_digest",
    "pages_artifact_id"
  ], "Pages fast correction evidence");
  const deploymentId = requireCommit(value.deployment_id, "Pages deployment");
  if (deploymentId !== candidateCommit) throw new Error("Pages deployment ID differs from candidate");
  const pageUrl = requirePagesUrl(value.page_url);
  const normalized = {
    build_artifact_id: requireId(value.build_artifact_id, "Pages build artifact ID"),
    build_artifact_digest: requireDigest(value.build_artifact_digest, "Pages build artifact"),
    pages_artifact_id: requireId(value.pages_artifact_id, "Pages deployment artifact ID"),
    pages_artifact_digest: requireDigest(value.pages_artifact_digest, "Pages deployment artifact"),
    deployment_id: deploymentId,
    page_url: pageUrl,
    live_identity_sha256: requireHash(value.live_identity_sha256, "live Pages identity"),
  };
  if (normalized.build_artifact_id === normalized.pages_artifact_id) {
    throw new Error("Pages build and deployment artifacts must be distinct");
  }
  if (typeof workflowRunId !== "string" || !DECIMAL_ID.test(workflowRunId)) {
    throw new Error("Pages evidence workflow binding is invalid");
  }
  return Object.freeze(normalized);
}

function requirePagesUrl(value) {
  let parsed;
  try {
    parsed = new URL(value);
  } catch {
    throw new Error("Pages URL is invalid");
  }
  if (
    parsed.protocol !== "https:" || parsed.username !== "" || parsed.password !== "" ||
    parsed.port !== "" || !parsed.hostname.endsWith(".github.io") ||
    !/^\/[A-Za-z0-9._-]+\/$/u.test(parsed.pathname) || parsed.search !== "" || parsed.hash !== ""
  ) throw new Error("Pages URL is not canonical");
  return parsed.toString();
}

function requireExactKeys(value, fields, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) throw new Error(`${label} must be an object`);
  if (JSON.stringify(Object.keys(value).sort()) !== JSON.stringify([...fields].sort())) {
    throw new Error(`${label} fields are not closed`);
  }
}

function requireCommit(value, label) {
  if (!/^[0-9a-f]{40}$/u.test(value ?? "")) throw new Error(`${label} commit is invalid`);
  return value;
}

function requireHash(value, label) {
  if (!SHA256.test(value ?? "")) throw new Error(`${label} SHA-256 is invalid`);
  return value;
}

function requireDigest(value, label) {
  if (!ARTIFACT_DIGEST.test(value ?? "")) throw new Error(`${label} digest is invalid`);
  return value;
}

function requireId(value, label) {
  const normalized = String(value ?? "");
  if (!DECIMAL_ID.test(normalized)) throw new Error(`${label} is invalid`);
  return normalized;
}

function rejectSecretMaterial(value) {
  const text = canonicalJson(value);
  if (/(?:BEGIN [A-Z ]*PRIVATE KEY|gh[pousr]_[A-Za-z0-9]{20,}|sk-[A-Za-z0-9]{20,})/u.test(text)) {
    throw new Error("fast correction evidence contains secret-like material");
  }
}

function parseArguments(args) {
  const values = {};
  const allowed = new Set([
    "--plan", "--base-accepted-run-id", "--base-accepted-run-attempt",
    "--run-id", "--run-attempt", "--outcome", "--output",
    "--build-artifact-id", "--build-artifact-digest", "--pages-artifact-id",
    "--pages-artifact-digest", "--deployment-id", "--page-url", "--live-identity-sha256"
  ]);
  for (let index = 0; index < args.length; index += 2) {
    const option = args[index];
    const value = args[index + 1];
    if (!allowed.has(option) || typeof value !== "string" || value.length === 0 || Object.hasOwn(values, option)) {
      throw new Error("fast correction evidence arguments are invalid");
    }
    values[option] = value;
  }
  for (const required of [
    "--plan", "--base-accepted-run-id", "--base-accepted-run-attempt",
    "--run-id", "--run-attempt", "--outcome", "--output"
  ]) if (!Object.hasOwn(values, required)) throw new Error(`missing option: ${required}`);
  return values;
}

function main() {
  const values = parseArguments(process.argv.slice(2));
  const plan = JSON.parse(readFileSync(resolve(values["--plan"]), "utf8"));
  const pagesDeployment = values["--outcome"] === "pages-deployed" ? {
    build_artifact_id: values["--build-artifact-id"],
    build_artifact_digest: values["--build-artifact-digest"],
    pages_artifact_id: values["--pages-artifact-id"],
    pages_artifact_digest: values["--pages-artifact-digest"],
    deployment_id: values["--deployment-id"],
    page_url: values["--page-url"],
    live_identity_sha256: values["--live-identity-sha256"],
  } : null;
  const evidence = createFastCorrectionEvidence({
    plan,
    baseAcceptedRunId: values["--base-accepted-run-id"],
    baseAcceptedRunAttempt: values["--base-accepted-run-attempt"],
    workflowRunId: values["--run-id"],
    workflowRunAttempt: values["--run-attempt"],
    outcome: values["--outcome"],
    pagesDeployment,
  });
  validateFastCorrectionEvidence(evidence, { expectedCandidateCommit: plan.candidate_commit });
  writeFileSync(resolve(values["--output"]), `${canonicalJson(evidence)}\n`, {
    encoding: "utf8",
    flag: "wx",
  });
  process.stdout.write(`fast_correction_evidence=sealed outcome=${evidence.outcome} candidate=${evidence.candidate_commit}\n`);
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`fast_correction_evidence=failed reason=${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 2;
  }
}
