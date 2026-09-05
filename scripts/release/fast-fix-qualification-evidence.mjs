#!/usr/bin/env node

import { lstat, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  canonicalJson,
  canonicalSha256,
  requireExactKeys,
  requireNonEmptyString,
  requirePlainObject,
  requireSha256,
  requireSourceCommit,
  sealCanonicalReport,
  verifyCanonicalReportHash,
} from "./canonical-release-evidence.mjs";
import {
  ACCEPTED_COMPONENT_LEDGER_BASELINE_KIND,
  DEPLOYMENT_COMPONENTS,
  PRODUCTION_TAG_BASELINE_KIND,
} from "./deployment-impact.mjs";

export const FAST_FIX_WORKFLOW_PATH =
  ".github/workflows/fast-fix-qualification.yml";
export const FAST_FIX_COMPONENT_SCHEMA =
  "clearra.fast-fix-component-qualification.v1";
export const FAST_FIX_LEDGER_SCHEMA =
  "clearra.fast-fix-qualification-ledger.v1";
export const FAST_FIX_PROMOTION_SCHEMA =
  "clearra.fast-fix-canonical-promotion.v1";
export const ACCEPTED_COMPONENT_LEDGER_SCHEMA =
  "clearra.accepted-component-ledger.v1";
export const FAST_FIX_LEDGER_FILE =
  "clearra-fast-fix-qualification-ledger.v1.json";
export const ACCEPTED_COMPONENT_LEDGER_FILE =
  "clearra-accepted-component-ledger.v1.json";
export const FAST_FIX_PROMOTION_FILE =
  "clearra-fast-fix-canonical-promotion.v1.json";

const DECIMAL_ID = /^[1-9][0-9]*$/u;
const REPOSITORY = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u;
const WORKFLOW_PATH = /^\.github\/workflows\/[A-Za-z0-9_.-]+\.ya?ml$/u;
const NULL_SHA = null;
export const ACCEPTED_COMPONENT_LEDGER_WORKFLOW_PATHS = Object.freeze([
  ".github/workflows/component-ledger-bootstrap.yml",
  ".github/workflows/fast-fix-production-finalizer.yml",
]);

export const COMPONENT_QUALIFICATION_COMMANDS = Object.freeze(new Map([
  ["pages", "node scripts/tools/run-focused-js-tests.mjs apps/clearra-web/test/ClearraWasmRuntime.contract.ts packages/clearra-ui/test/pagesEssentialSurface.test.mjs"],
  ["desktop_gui", "node scripts/tools/run-focused-js-tests.mjs packages/clearra-ui/test/desktopProductPageCancellation.test.mjs packages/clearra-ui/test/uiRuntimeContracts.test.mjs"],
  ["cli", "cargo test -p clearra-cli --test product_cli_surface_contract -- --test-threads=1"],
  ["discord_gateway", "node scripts/tools/run-focused-js-tests.mjs apps/clearra-discord-bot/test/capability-registry.test.mjs apps/clearra-discord-bot/test/cli-authority-result-paths.test.mjs"],
  ["heavy_cloud_runtime", "node scripts/tools/run-focused-js-tests.mjs apps/clearra-discord-bot/test/cloud-candidate-smoke-job.test.mjs apps/clearra-discord-bot/test/current-job-container-closure.test.mjs"],
  ["pc4_lookup_service", "unsupported-before-v0.9-fail-closed"],
  ["pc4_activation_manifest", "unsupported-before-v0.9-fail-closed"],
]));

export function createAcceptedComponentLedger(authority, components) {
  const identity = validateAuthority(authority, { acceptedLedgerWorkflow: true });
  const entries = validateAcceptedComponents(components);
  return sealCanonicalReport({
    schema_id: ACCEPTED_COMPONENT_LEDGER_SCHEMA,
    repository: identity.repository,
    source_commit: identity.sourceCommit,
    workflow_path: identity.workflowPath,
    run_id: identity.runId,
    run_attempt: identity.runAttempt,
    status: "accepted-and-deployed",
    components: entries,
  });
}

export function validateAcceptedComponentLedger(value, expected = {}) {
  requireExactKeys(value, [
    "schema_id", "repository", "source_commit", "workflow_path", "run_id",
    "run_attempt", "status", "components", "report_sha256",
  ], "accepted component ledger");
  if (value.schema_id !== ACCEPTED_COMPONENT_LEDGER_SCHEMA) {
    throw new Error("accepted component ledger schema is invalid");
  }
  verifyCanonicalReportHash(value, "accepted component ledger");
  const identity = validateAuthority({
    repository: value.repository,
    sourceCommit: value.source_commit,
    workflowPath: value.workflow_path,
    runId: value.run_id,
    runAttempt: value.run_attempt,
  }, { acceptedLedgerWorkflow: true });
  if (value.status !== "accepted-and-deployed") {
    throw new Error("accepted component ledger does not prove deployed state");
  }
  if (
    (expected.repository !== undefined && identity.repository !== expected.repository) ||
    (expected.sourceCommit !== undefined && identity.sourceCommit !== expected.sourceCommit) ||
    (expected.runId !== undefined && identity.runId !== String(expected.runId)) ||
    (expected.runAttempt !== undefined && identity.runAttempt !== String(expected.runAttempt)) ||
    (expected.workflowPath !== undefined && identity.workflowPath !== expected.workflowPath)
  ) {
    throw new Error("accepted component ledger differs from artifact run authority");
  }
  validateAcceptedComponents(value.components);
  return value;
}

export function createComponentQualification(authority, component, command) {
  const identity = validateAuthority(authority);
  const expectedCommand = COMPONENT_QUALIFICATION_COMMANDS.get(component);
  if (expectedCommand === undefined || component === "release_infrastructure") {
    throw new Error(`component is not eligible for fast qualification: ${component}`);
  }
  if (expectedCommand === "unsupported-before-v0.9-fail-closed") {
    throw new Error(`${component} fast qualification is not implemented before v0.9`);
  }
  if (command !== expectedCommand) {
    throw new Error(`${component} qualification command differs from its closed contract`);
  }
  return sealCanonicalReport({
    schema_id: FAST_FIX_COMPONENT_SCHEMA,
    repository: identity.repository,
    source_commit: identity.sourceCommit,
    workflow_path: FAST_FIX_WORKFLOW_PATH,
    run_id: identity.runId,
    run_attempt: identity.runAttempt,
    component,
    command,
    status: "passed",
    production_mutation: false,
  });
}

export function createCanonicalPromotionEvidence(authority, impact) {
  const identity = validateAuthority(authority);
  const plan = validateFullGateImpactPlan(impact, identity.sourceCommit);
  return sealCanonicalReport({
    schema_id: FAST_FIX_PROMOTION_SCHEMA,
    repository: identity.repository,
    source_commit: identity.sourceCommit,
    workflow_path: FAST_FIX_WORKFLOW_PATH,
    run_id: identity.runId,
    run_attempt: identity.runAttempt,
    status: "canonical-full-gate-dispatch-requested",
    production_mutation: false,
    target_workflow_path: ".github/workflows/release-cli.yml",
    target_ref: "main",
    changed_paths_sha256: plan.changedPathsSha256,
    impact_plan_sha256: plan.impactPlanSha256,
    changed_components: plan.changedComponents,
  });
}

export function validateComponentQualification(value, authority, component) {
  requireExactKeys(value, [
    "schema_id", "repository", "source_commit", "workflow_path", "run_id",
    "run_attempt", "component", "command", "status", "production_mutation",
    "report_sha256",
  ], `${component} qualification evidence`);
  verifyCanonicalReportHash(value, `${component} qualification evidence`);
  const expected = createComponentQualification(
    authority,
    component,
    COMPONENT_QUALIFICATION_COMMANDS.get(component),
  );
  if (canonicalJson(value) !== canonicalJson(expected)) {
    throw new Error(`${component} qualification evidence differs from its closed contract`);
  }
  return value;
}

export function createFastFixQualificationLedger({
  authority,
  impact,
  baseline,
  componentReports,
}) {
  const identity = validateAuthority(authority);
  const plan = validateImpactPlan(impact, identity.sourceCommit);
  if (plan.gateMode === "full") {
    throw new Error("full-gate changes cannot create fast-fix qualification evidence");
  }
  const prior = validateAcceptedComponentLedger(baseline, {
    repository: identity.repository,
    runAttempt: "1",
  });
  if (
    plan.baseline.kind !== ACCEPTED_COMPONENT_LEDGER_BASELINE_KIND ||
    plan.baseline.sourceCommit !== prior.source_commit ||
    plan.baseline.workflowPath !== prior.workflow_path ||
    plan.baseline.runId !== prior.run_id ||
    plan.baseline.runAttempt !== prior.run_attempt ||
    plan.baseline.ledgerReportSha256 !== prior.report_sha256
  ) {
    throw new Error("deployment impact baseline differs from the accepted ledger");
  }
  const reports = new Map();
  for (const report of componentReports) {
    requirePlainObject(report, "component qualification evidence");
    const component = requireNonEmptyString(report.component, "qualified component");
    if (reports.has(component)) {
      throw new Error(`duplicate component qualification evidence: ${component}`);
    }
    validateComponentQualification(report, identity, component);
    reports.set(component, report);
  }
  const wanted = [...plan.changedComponents].sort();
  const actual = [...reports.keys()].sort();
  if (canonicalJson(actual) !== canonicalJson(wanted)) {
    throw new Error("component qualification evidence set differs from deployment impact");
  }

  const baselineByComponent = new Map(
    prior.components.map((entry) => [entry.component, entry]),
  );
  const components = DEPLOYMENT_COMPONENTS.map((component) => {
    const baselineEntry = baselineByComponent.get(component);
    const report = reports.get(component);
    if (report !== undefined) {
      return Object.freeze({
        component,
        disposition: "qualified-not-deployed",
        qualification_receipt_sha256: report.report_sha256,
        accepted_digest: NULL_SHA,
        accepted_receipt_sha256: NULL_SHA,
        deployment_receipt_sha256: NULL_SHA,
        prior_accepted_digest: baselineEntry.accepted_digest,
        prior_accepted_receipt_sha256:
          baselineEntry.accepted_receipt_sha256,
        prior_deployment_receipt_sha256:
          baselineEntry.deployment_receipt_sha256,
      });
    }
    return Object.freeze({
      component,
      disposition: "carry-forward",
      qualification_receipt_sha256: NULL_SHA,
      accepted_digest: baselineEntry.accepted_digest,
      accepted_receipt_sha256: baselineEntry.accepted_receipt_sha256,
      deployment_receipt_sha256: baselineEntry.deployment_receipt_sha256,
      prior_accepted_digest: baselineEntry.accepted_digest,
      prior_accepted_receipt_sha256:
        baselineEntry.accepted_receipt_sha256,
      prior_deployment_receipt_sha256:
        baselineEntry.deployment_receipt_sha256,
    });
  });
  return sealCanonicalReport({
    schema_id: FAST_FIX_LEDGER_SCHEMA,
    repository: identity.repository,
    source_commit: identity.sourceCommit,
    workflow_path: FAST_FIX_WORKFLOW_PATH,
    run_id: identity.runId,
    run_attempt: identity.runAttempt,
    status: "qualified-not-deployed",
    production_mutation: false,
    gate_mode: plan.gateMode,
    changed_paths_sha256: plan.changedPathsSha256,
    impact_plan_sha256: plan.impactPlanSha256,
    changed_components: plan.changedComponents,
    carry_forward_components: plan.carryForwardComponents,
    baseline: {
      kind: plan.baseline.kind,
      source_commit: prior.source_commit,
      workflow_path: prior.workflow_path,
      run_id: prior.run_id,
      run_attempt: prior.run_attempt,
      ledger_report_sha256: prior.report_sha256,
    },
    components,
  });
}

export function validateFastFixQualificationLedger(value) {
  requireExactKeys(value, [
    "schema_id", "repository", "source_commit", "workflow_path", "run_id",
    "run_attempt", "status", "production_mutation", "gate_mode",
    "changed_paths_sha256", "impact_plan_sha256", "changed_components", "carry_forward_components",
    "baseline", "components", "report_sha256",
  ], "fast-fix qualification ledger");
  if (value.schema_id !== FAST_FIX_LEDGER_SCHEMA) {
    throw new Error("fast-fix qualification ledger schema is invalid");
  }
  verifyCanonicalReportHash(value, "fast-fix qualification ledger");
  validateAuthority({
    repository: value.repository,
    sourceCommit: value.source_commit,
    workflowPath: value.workflow_path,
    runId: value.run_id,
    runAttempt: value.run_attempt,
  });
  if (
    value.status !== "qualified-not-deployed" ||
    value.production_mutation !== false ||
    !["none", "focused"].includes(value.gate_mode)
  ) {
    throw new Error("fast-fix evidence must remain qualification-only");
  }
  requireSha256(value.changed_paths_sha256, "changed path set digest");
  requireSha256(value.impact_plan_sha256, "deployment impact plan digest");
  validateClosedComponentPartition(
    value.changed_components,
    value.carry_forward_components,
  );
  if ((value.gate_mode === "none") !== (value.changed_components.length === 0)) {
    throw new Error("qualification gate mode and changed components disagree");
  }
  if (
    value.gate_mode === "focused" &&
    (value.changed_components.length !== 1 || value.changed_components[0] !== "pages")
  ) {
    throw new Error("v0.8 focused qualification ledger is restricted to Pages-only changes");
  }
  requireExactKeys(value.baseline, [
    "kind", "source_commit", "workflow_path", "run_id", "run_attempt",
    "ledger_report_sha256",
  ], "fast-fix baseline authority");
  if (value.baseline.kind !== ACCEPTED_COMPONENT_LEDGER_BASELINE_KIND) {
    throw new Error("fast-fix baseline kind is invalid");
  }
  requireSourceCommit(value.baseline.source_commit, "baseline source commit");
  requireWorkflowPath(value.baseline.workflow_path, "baseline workflow path");
  if (!ACCEPTED_COMPONENT_LEDGER_WORKFLOW_PATHS.includes(value.baseline.workflow_path)) {
    throw new Error("fast-fix baseline workflow is not authoritative");
  }
  if (value.baseline.source_commit === value.source_commit) {
    throw new Error("fast-fix baseline must precede the qualification source");
  }
  requireDecimalId(value.baseline.run_id, "baseline run ID");
  if (String(value.baseline.run_attempt) !== "1") {
    throw new Error("baseline run must be attempt 1");
  }
  requireSha256(
    value.baseline.ledger_report_sha256,
    "baseline ledger report digest",
  );
  validateQualificationComponents(value.components, value.changed_components);
  return value;
}

function validateImpactPlan(value, sourceCommit) {
  requirePlainObject(value, "deployment impact plan");
  const sealedPlan = validateImpactPlanDigest(value, sourceCommit);
  if (value.source_commit !== sourceCommit) {
    throw new Error("deployment impact source differs from qualification source");
  }
  if (sealedPlan.baseline.kind !== ACCEPTED_COMPONENT_LEDGER_BASELINE_KIND) {
    throw new Error("focused qualification requires an accepted-ledger baseline");
  }
  if (value.requires_full_gate !== false) {
    throw new Error("fast qualification rejects a full-gate deployment impact");
  }
  if (!["none", "focused"].includes(value.gate_mode)) {
    throw new Error("deployment impact gate mode is not fast-qualification eligible");
  }
  const changedComponents = value.changed_components;
  const carryForwardComponents = value.carry_forward_components;
  if (
    value.gate_mode === "focused" &&
    (changedComponents.length !== 1 || changedComponents[0] !== "pages")
  ) {
    throw new Error("v0.8 focused qualification is restricted to Pages-only changes");
  }
  validateClosedComponentPartition(changedComponents, carryForwardComponents);
  if (
    (value.gate_mode === "none") !== (changedComponents.length === 0) ||
    value.component_scope !== (changedComponents.join("+") || "none")
  ) {
    throw new Error("deployment impact gate mode or component scope is inconsistent");
  }
  requireSha256(value.changed_paths_sha256, "changed path set digest");
  return Object.freeze({
    gateMode: value.gate_mode,
    changedPathsSha256: value.changed_paths_sha256,
    impactPlanSha256: sealedPlan.impactPlanSha256,
    baseline: sealedPlan.baseline,
    changedComponents: Object.freeze([...changedComponents]),
    carryForwardComponents: Object.freeze([...carryForwardComponents]),
  });
}

function validateFullGateImpactPlan(value, sourceCommit) {
  requirePlainObject(value, "deployment impact plan");
  const sealedPlan = validateImpactPlanDigest(value, sourceCommit);
  if (
    value.source_commit !== sourceCommit ||
    value.requires_full_gate !== true ||
    value.gate_mode !== "full"
  ) {
    throw new Error("canonical promotion requires exact full-gate impact");
  }
  validateClosedComponentPartition(
    value.changed_components,
    value.carry_forward_components,
  );
  if (value.changed_components.length === 0) {
    throw new Error("canonical promotion requires at least one changed component");
  }
  requireSha256(value.changed_paths_sha256, "changed path set digest");
  return Object.freeze({
    changedPathsSha256: value.changed_paths_sha256,
    impactPlanSha256: sealedPlan.impactPlanSha256,
    baseline: sealedPlan.baseline,
    changedComponents: Object.freeze([...value.changed_components]),
  });
}

function validateImpactPlanDigest(value, sourceCommit) {
  if (value.source_commit !== sourceCommit) {
    throw new Error("deployment impact source differs from qualification source");
  }
  const baseline = validateImpactBaseline(value, sourceCommit);
  requireSha256(value.changed_paths_sha256, "changed path set digest");
  const expected = canonicalSha256({
    source_commit: value.source_commit,
    baseline_kind: value.baseline_kind,
    baseline_tag: value.baseline_tag,
    baseline_commit: value.baseline_commit,
    baseline_source_commit: value.baseline_source_commit,
    baseline_workflow_path: value.baseline_workflow_path,
    baseline_run_id: value.baseline_run_id,
    baseline_run_attempt: value.baseline_run_attempt,
    baseline_ledger_report_sha256: value.baseline_ledger_report_sha256,
    gate_mode: value.gate_mode,
    requires_full_gate: value.requires_full_gate,
    changed_components: value.changed_components,
    carry_forward_components: value.carry_forward_components,
    changed_paths_sha256: value.changed_paths_sha256,
  });
  requireSha256(value.impact_plan_sha256, "deployment impact plan digest");
  if (value.impact_plan_sha256 !== expected) {
    throw new Error("deployment impact plan digest differs from its closed fields");
  }
  return Object.freeze({ impactPlanSha256: expected, baseline });
}

function validateImpactBaseline(value, sourceCommit) {
  const baselineSourceCommit = requireSourceCommit(
    value.baseline_source_commit,
    "deployment impact baseline source commit",
  );
  if (value.baseline_commit !== baselineSourceCommit) {
    throw new Error("deployment impact baseline commit/source differ");
  }
  if (value.baseline_kind === PRODUCTION_TAG_BASELINE_KIND) {
    requireNonEmptyString(value.baseline_tag, "deployment impact baseline tag");
    if (
      value.baseline_workflow_path !== null ||
      value.baseline_run_id !== null ||
      value.baseline_run_attempt !== null ||
      value.baseline_ledger_report_sha256 !== null
    ) {
      throw new Error("production-tag baseline must not invent ledger authority");
    }
    return Object.freeze({
      kind: value.baseline_kind,
      sourceCommit: baselineSourceCommit,
      workflowPath: null,
      runId: null,
      runAttempt: null,
      ledgerReportSha256: null,
    });
  }
  if (value.baseline_kind !== ACCEPTED_COMPONENT_LEDGER_BASELINE_KIND) {
    throw new Error("deployment impact baseline kind is invalid");
  }
  if (value.baseline_tag !== null) {
    throw new Error("accepted-ledger baseline must not claim a production tag");
  }
  const workflowPath = requireWorkflowPath(
    value.baseline_workflow_path,
    "deployment impact baseline workflow path",
  );
  if (!ACCEPTED_COMPONENT_LEDGER_WORKFLOW_PATHS.includes(workflowPath)) {
    throw new Error("deployment impact baseline workflow is not authoritative");
  }
  const runId = requireDecimalId(
    value.baseline_run_id,
    "deployment impact baseline run ID",
  );
  const runAttempt = requireDecimalId(
    value.baseline_run_attempt,
    "deployment impact baseline run attempt",
  );
  if (runAttempt !== "1") {
    throw new Error("deployment impact baseline must be attempt 1");
  }
  const ledgerReportSha256 = requireSha256(
    value.baseline_ledger_report_sha256,
    "deployment impact baseline ledger report digest",
  );
  if (baselineSourceCommit === sourceCommit) {
    throw new Error("deployment impact baseline must precede the candidate");
  }
  return Object.freeze({
    kind: value.baseline_kind,
    sourceCommit: baselineSourceCommit,
    workflowPath,
    runId,
    runAttempt,
    ledgerReportSha256,
  });
}

function validateClosedComponentPartition(changed, carried) {
  if (!Array.isArray(changed) || !Array.isArray(carried)) {
    throw new Error("component vectors must be arrays");
  }
  const actual = [...changed, ...carried];
  if (
    actual.length !== DEPLOYMENT_COMPONENTS.length ||
    new Set(actual).size !== DEPLOYMENT_COMPONENTS.length ||
    actual.some((component) => !DEPLOYMENT_COMPONENTS.includes(component)) ||
    DEPLOYMENT_COMPONENTS.some((component) => !actual.includes(component))
  ) {
    throw new Error("changed and carry-forward vectors must partition every component");
  }
}

function validateAcceptedComponents(components) {
  if (!Array.isArray(components) || components.length !== DEPLOYMENT_COMPONENTS.length) {
    throw new Error("accepted component ledger must cover every component");
  }
  const entries = components.map((entry, index) => {
    requireExactKeys(entry, [
      "component", "accepted_digest", "accepted_receipt_sha256",
      "deployment_receipt_sha256",
    ], "accepted component entry");
    if (entry.component !== DEPLOYMENT_COMPONENTS[index]) {
      throw new Error("accepted component ledger order or membership is invalid");
    }
    requireSha256(entry.accepted_digest, `${entry.component} accepted digest`);
    requireSha256(
      entry.accepted_receipt_sha256,
      `${entry.component} accepted receipt digest`,
    );
    requireSha256(
      entry.deployment_receipt_sha256,
      `${entry.component} deployment receipt digest`,
    );
    return Object.freeze({ ...entry });
  });
  return Object.freeze(entries);
}

function validateQualificationComponents(components, changedComponents) {
  if (!Array.isArray(components) || components.length !== DEPLOYMENT_COMPONENTS.length) {
    throw new Error("qualification ledger must cover every component");
  }
  components.forEach((entry, index) => {
    requireExactKeys(entry, [
      "component", "disposition", "qualification_receipt_sha256",
      "accepted_digest", "accepted_receipt_sha256", "deployment_receipt_sha256",
      "prior_accepted_digest", "prior_accepted_receipt_sha256",
      "prior_deployment_receipt_sha256",
    ], "qualification component entry");
    if (entry.component !== DEPLOYMENT_COMPONENTS[index]) {
      throw new Error("qualification component order or membership is invalid");
    }
    requireSha256(
      entry.prior_accepted_digest,
      `${entry.component} prior accepted digest`,
    );
    requireSha256(
      entry.prior_accepted_receipt_sha256,
      `${entry.component} prior accepted receipt digest`,
    );
    requireSha256(
      entry.prior_deployment_receipt_sha256,
      `${entry.component} prior deployment receipt digest`,
    );
    const changed = changedComponents.includes(entry.component);
    if (changed) {
      if (
        entry.disposition !== "qualified-not-deployed" ||
        entry.accepted_digest !== null ||
        entry.accepted_receipt_sha256 !== null ||
        entry.deployment_receipt_sha256 !== null
      ) {
        throw new Error("changed component must not claim accepted or deployed state");
      }
      requireSha256(
        entry.qualification_receipt_sha256,
        `${entry.component} qualification receipt digest`,
      );
    } else if (
      entry.disposition !== "carry-forward" ||
      entry.qualification_receipt_sha256 !== null ||
      entry.accepted_digest !== entry.prior_accepted_digest ||
      entry.accepted_receipt_sha256 !== entry.prior_accepted_receipt_sha256 ||
      entry.deployment_receipt_sha256 !== entry.prior_deployment_receipt_sha256
    ) {
      throw new Error("unchanged component must preserve its accepted and deployed receipts");
    } else {
      requireSha256(entry.accepted_digest, `${entry.component} accepted digest`);
      requireSha256(
        entry.accepted_receipt_sha256,
        `${entry.component} accepted receipt digest`,
      );
      requireSha256(
        entry.deployment_receipt_sha256,
        `${entry.component} deployment receipt digest`,
      );
    }
  });
}

function validateAuthority(authority, { acceptedLedgerWorkflow = false } = {}) {
  requirePlainObject(authority, "qualification authority");
  const repository = requirePattern(
    authority.repository,
    REPOSITORY,
    "qualification repository",
  );
  const sourceCommit = requireSourceCommit(authority.sourceCommit);
  const workflowPath = requireWorkflowPath(
    authority.workflowPath ?? FAST_FIX_WORKFLOW_PATH,
    "qualification workflow path",
  );
  if (
    acceptedLedgerWorkflow &&
    !ACCEPTED_COMPONENT_LEDGER_WORKFLOW_PATHS.includes(workflowPath)
  ) {
    throw new Error("accepted component ledger workflow is not authoritative");
  }
  if (!acceptedLedgerWorkflow && workflowPath !== FAST_FIX_WORKFLOW_PATH) {
    throw new Error("qualification workflow path is not authoritative");
  }
  const runId = requireDecimalId(authority.runId, "qualification run ID");
  const runAttempt = requireDecimalId(
    authority.runAttempt,
    "qualification run attempt",
  );
  if (runAttempt !== "1") {
    throw new Error("fast-fix qualification forbids rerun attempts");
  }
  return Object.freeze({ repository, sourceCommit, workflowPath, runId, runAttempt });
}

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new Error(`${label} is invalid`);
  }
  return value;
}

function requireWorkflowPath(value, label) {
  return requirePattern(value, WORKFLOW_PATH, label);
}

function requireDecimalId(value, label) {
  return requirePattern(String(value ?? ""), DECIMAL_ID, label);
}

async function readJsonFile(path, label) {
  const raw = await readFile(resolve(path), "utf8");
  let value;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new Error(`${label} is not JSON`);
  }
  return value;
}

async function writeReport(path, report) {
  const output = resolve(path);
  await mkdir(dirname(output), { recursive: true });
  await writeFile(output, `${canonicalJson(report)}\n`, { encoding: "utf8", flag: "wx" });
}

async function readComponentReports(directory) {
  const root = resolve(directory);
  const entries = await readdir(root, { withFileTypes: true });
  const reports = [];
  for (const entry of entries) {
    if (!entry.isFile() || entry.isSymbolicLink() || !entry.name.endsWith(".json")) {
      throw new Error("component evidence directory contains an unsupported entry");
    }
    const path = resolve(root, entry.name);
    const stat = await lstat(path);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.size <= 0) {
      throw new Error("component evidence must be a non-empty regular file");
    }
    reports.push(await readJsonFile(path, "component qualification evidence"));
  }
  return reports;
}

async function main() {
  const mode = process.argv[2];
  const { values } = parseArgs({
    args: process.argv.slice(3),
    options: {
      repository: { type: "string" },
      "source-commit": { type: "string" },
      "workflow-path": { type: "string" },
      "run-id": { type: "string" },
      "run-attempt": { type: "string" },
      component: { type: "string" },
      command: { type: "string" },
      baseline: { type: "string" },
      impact: { type: "string" },
      "component-directory": { type: "string" },
      output: { type: "string" },
      "expected-repository": { type: "string" },
      "expected-source-commit": { type: "string" },
      "expected-run-id": { type: "string" },
      "expected-run-attempt": { type: "string" },
      "expected-workflow-path": { type: "string" },
    },
    strict: true,
  });
  if (mode === "component") {
    const report = createComponentQualification({
      repository: values.repository,
      sourceCommit: values["source-commit"],
      runId: values["run-id"],
      runAttempt: values["run-attempt"],
    }, values.component, values.command);
    await writeReport(values.output, report);
    return;
  }
  if (mode === "promotion") {
    const impact = await readJsonFile(values.impact, "deployment impact plan");
    const report = createCanonicalPromotionEvidence({
      repository: values.repository,
      sourceCommit: values["source-commit"],
      runId: values["run-id"],
      runAttempt: values["run-attempt"],
    }, impact);
    await writeReport(values.output, report);
    return;
  }
  if (mode === "verify-baseline") {
    const baseline = await readJsonFile(values.baseline, "accepted component ledger");
    validateAcceptedComponentLedger(baseline, {
      repository: values["expected-repository"],
      sourceCommit: values["expected-source-commit"],
      runId: values["expected-run-id"],
      runAttempt: values["expected-run-attempt"],
      workflowPath: values["expected-workflow-path"],
    });
    return;
  }
  if (mode === "ledger") {
    const [baseline, impact, componentReports] = await Promise.all([
      readJsonFile(values.baseline, "accepted component ledger"),
      readJsonFile(values.impact, "deployment impact plan"),
      readComponentReports(values["component-directory"]),
    ]);
    const report = createFastFixQualificationLedger({
      authority: {
        repository: values.repository,
        sourceCommit: values["source-commit"],
        runId: values["run-id"],
        runAttempt: values["run-attempt"],
      },
      impact,
      baseline,
      componentReports,
    });
    validateFastFixQualificationLedger(report);
    await writeReport(values.output, report);
    return;
  }
  throw new Error("usage: fast-fix-qualification-evidence.mjs (component|promotion|verify-baseline|ledger) [closed options]");
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    await main();
  } catch (error) {
    process.stderr.write(
      `fast_fix_qualification=failed reason=${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 2;
  }
}
