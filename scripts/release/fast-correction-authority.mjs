#!/usr/bin/env node

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { validateFastCorrectionEvidence } from "./fast-correction-evidence.mjs";
import { validateFastCorrectionPlan } from "./fast-correction-plan.mjs";

const COMMIT = /^[0-9a-f]{40}$/u;
const DECIMAL_ID = /^[1-9][0-9]*$/u;
const CONTROLS = new Set(["discord", "pages"]);

export function validateFastCorrectionControlAuthority({
  plan,
  evidence,
  expectedBaseCommit,
  expectedCandidateCommit,
  expectedEvidenceRunId,
  expectedEvidenceRunAttempt,
  requiredControl,
}) {
  validateFastCorrectionPlan(plan);
  validateFastCorrectionEvidence(evidence, { expectedCandidateCommit });
  requireCommit(expectedBaseCommit, "expected accepted base");
  requireCommit(expectedCandidateCommit, "expected workflow candidate");
  const evidenceRunId = requireId(expectedEvidenceRunId, "expected evidence run ID");
  const evidenceRunAttempt = requireId(expectedEvidenceRunAttempt, "expected evidence run attempt");
  if (!CONTROLS.has(requiredControl)) throw new Error("required fast correction control is invalid");
  if (plan.decision !== "fast-eligible" || evidence.decision !== "fast-eligible") {
    throw new Error("dual authority requires a fast-eligible plan");
  }
  if (plan.deploy_pages || plan.affected_products.length !== 0) {
    throw new Error("dual authority cannot reuse accepted products across a runtime source change");
  }
  if (
    plan.changed_owners.some((owner) => !["documentation", "release-workflow"].includes(owner)) ||
    !plan.changed_owners.includes("release-workflow")
  ) throw new Error("dual authority requires a release-workflow-only correction");
  if (!plan.control_deployments.includes(requiredControl)) {
    throw new Error(`fast correction plan does not qualify ${requiredControl} control deployment`);
  }
  if (
    evidence.outcome !== "passed-no-deploy" || evidence.status !== "passed" ||
    evidence.pages_deployment !== null
  ) throw new Error("dual authority requires passing no-deploy qualification evidence");
  if (
    plan.accepted_base_commit !== expectedBaseCommit ||
    evidence.accepted_base.source_commit !== expectedBaseCommit
  ) throw new Error("fast correction accepted base mismatch");
  if (plan.candidate_commit !== expectedCandidateCommit) {
    throw new Error("fast correction workflow candidate mismatch");
  }
  if (
    evidence.fast_workflow.run_id !== evidenceRunId ||
    evidence.fast_workflow.run_attempt !== evidenceRunAttempt
  ) throw new Error("fast correction evidence run binding mismatch");
  for (const [left, right, label] of [
    [evidence.plan_sha256, plan.plan_sha256, "plan hash"],
    [evidence.diff_sha256, plan.diff_sha256, "diff hash"],
    [evidence.owner_manifest_sha256, plan.owner_manifest_sha256, "owner manifest hash"],
    [evidence.lane, plan.lane, "lane"],
  ]) if (left !== right) throw new Error(`fast correction evidence ${label} mismatch`);
  if (JSON.stringify(evidence.selected) !== JSON.stringify(plan.selected)) {
    throw new Error("fast correction evidence selected execution mismatch");
  }
  const deploymentId = requiredControl === "discord"
    ? "discord-accepted-product-redeploy"
    : "pages-accepted-product-redeploy";
  if (!plan.selected.deployments.some(({ id }) => id === deploymentId)) {
    throw new Error("fast correction selected control deployment is missing");
  }
  return Object.freeze({
    productSourceCommit: expectedBaseCommit,
    workflowSourceCommit: expectedCandidateCommit,
    acceptedBaseTag: plan.accepted_base_tag,
    acceptedRunId: requireId(evidence.accepted_base.run_id, "accepted product run ID"),
    acceptedRunAttempt: requireId(evidence.accepted_base.run_attempt, "accepted product run attempt"),
    fastEvidenceRunId: evidenceRunId,
    fastEvidenceRunAttempt: evidenceRunAttempt,
    planSha256: plan.plan_sha256,
    evidenceSha256: evidence.evidence_sha256,
    requiredControl,
  });
}

function requireCommit(value, label) {
  if (!COMMIT.test(value ?? "")) throw new Error(`${label} commit is invalid`);
  return value;
}

function requireId(value, label) {
  const normalized = String(value ?? "");
  if (!DECIMAL_ID.test(normalized)) throw new Error(`${label} is invalid`);
  return normalized;
}

function parseArguments(args) {
  const values = {};
  const allowed = new Set([
    "--plan", "--evidence", "--expected-base", "--expected-candidate",
    "--expected-run-id", "--expected-run-attempt", "--required-control", "--format"
  ]);
  for (let index = 0; index < args.length; index += 2) {
    const option = args[index];
    const value = args[index + 1];
    if (!allowed.has(option) || typeof value !== "string" || value.length === 0 || Object.hasOwn(values, option)) {
      throw new Error("fast correction authority arguments are invalid");
    }
    values[option] = value;
  }
  for (const required of [
    "--plan", "--evidence", "--expected-base", "--expected-candidate",
    "--expected-run-id", "--expected-run-attempt", "--required-control"
  ]) if (!Object.hasOwn(values, required)) throw new Error(`missing option: ${required}`);
  if (!Object.hasOwn(values, "--format")) values["--format"] = "json";
  if (!["json", "github-output"].includes(values["--format"])) throw new Error("fast authority format is invalid");
  return values;
}

function main() {
  const values = parseArguments(process.argv.slice(2));
  const authority = validateFastCorrectionControlAuthority({
    plan: JSON.parse(readFileSync(resolve(values["--plan"]), "utf8")),
    evidence: JSON.parse(readFileSync(resolve(values["--evidence"]), "utf8")),
    expectedBaseCommit: values["--expected-base"],
    expectedCandidateCommit: values["--expected-candidate"],
    expectedEvidenceRunId: values["--expected-run-id"],
    expectedEvidenceRunAttempt: values["--expected-run-attempt"],
    requiredControl: values["--required-control"],
  });
  if (values["--format"] === "github-output") {
    for (const [key, value] of Object.entries({
      source_commit: authority.productSourceCommit,
      workflow_source_commit: authority.workflowSourceCommit,
      accepted_base_tag: authority.acceptedBaseTag,
      accepted_run_id: authority.acceptedRunId,
      accepted_run_attempt: authority.acceptedRunAttempt,
      fast_evidence_run_id: authority.fastEvidenceRunId,
      fast_evidence_run_attempt: authority.fastEvidenceRunAttempt,
      fast_plan_sha256: authority.planSha256,
      fast_evidence_sha256: authority.evidenceSha256,
    })) process.stdout.write(`${key}=${value}\n`);
  } else {
    process.stdout.write(`${JSON.stringify(authority)}\n`);
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`fast_correction_authority=failed reason=${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 2;
  }
}
