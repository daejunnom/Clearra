import assert from "node:assert/strict";
import test from "node:test";

import {
  ACCEPTED_COMPONENT_LEDGER_SCHEMA,
  COMPONENT_QUALIFICATION_COMMANDS,
  createAcceptedComponentLedger,
  createCanonicalPromotionEvidence,
  createComponentQualification,
  createFastFixQualificationLedger,
  FAST_FIX_LEDGER_SCHEMA,
  FAST_FIX_PROMOTION_SCHEMA,
  validateAcceptedComponentLedger,
  validateFastFixQualificationLedger,
} from "./fast-fix-qualification-evidence.mjs";
import {
  ACCEPTED_COMPONENT_LEDGER_BASELINE_KIND,
  classifyDeploymentImpact,
  DEPLOYMENT_COMPONENTS,
  PRODUCTION_TAG_BASELINE_KIND,
} from "./deployment-impact.mjs";
import {
  canonicalSha256,
  sealCanonicalReport,
} from "./canonical-release-evidence.mjs";

const baselineAuthority = Object.freeze({
  repository: "daejunnom/Clearra",
  sourceCommit: "1".repeat(40),
  workflowPath: ".github/workflows/component-ledger-bootstrap.yml",
  runId: "101",
  runAttempt: "1",
});
const candidateAuthority = Object.freeze({
  repository: "daejunnom/Clearra",
  sourceCommit: "2".repeat(40),
  runId: "202",
  runAttempt: "1",
});

function acceptedComponents() {
  const hex = "123456789abcdef";
  return DEPLOYMENT_COMPONENTS.map((component, index) => ({
    component,
    accepted_digest: hex[index].repeat(64),
    accepted_receipt_sha256: hex[index + 1].repeat(64),
    deployment_receipt_sha256: hex[index + 2].repeat(64),
  }));
}

function acceptedLedger() {
  return createAcceptedComponentLedger(baselineAuthority, acceptedComponents());
}

function impactPlan(paths, { baseline = acceptedLedger() } = {}) {
  const impact = classifyDeploymentImpact(paths);
  const plan = {
    source_commit: candidateAuthority.sourceCommit,
    baseline_kind: ACCEPTED_COMPONENT_LEDGER_BASELINE_KIND,
    baseline_tag: null,
    baseline_commit: baseline.source_commit,
    baseline_source_commit: baseline.source_commit,
    baseline_workflow_path: baseline.workflow_path,
    baseline_run_id: baseline.run_id,
    baseline_run_attempt: baseline.run_attempt,
    baseline_ledger_report_sha256: baseline.report_sha256,
    component_scope: impact.componentScope,
    requires_full_gate: impact.requiresFullGate,
    gate_mode: impact.gateMode,
    changed_components: impact.changedComponents,
    carry_forward_components: impact.carryForwardComponents,
    changed_paths_sha256: impact.changedPathsSha256,
  };
  plan.impact_plan_sha256 = canonicalSha256({
    source_commit: plan.source_commit,
    baseline_kind: plan.baseline_kind,
    baseline_tag: plan.baseline_tag,
    baseline_commit: plan.baseline_commit,
    baseline_source_commit: plan.baseline_source_commit,
    baseline_workflow_path: plan.baseline_workflow_path,
    baseline_run_id: plan.baseline_run_id,
    baseline_run_attempt: plan.baseline_run_attempt,
    baseline_ledger_report_sha256: plan.baseline_ledger_report_sha256,
    gate_mode: plan.gate_mode,
    requires_full_gate: plan.requires_full_gate,
    changed_components: plan.changed_components,
    carry_forward_components: plan.carry_forward_components,
    changed_paths_sha256: plan.changed_paths_sha256,
  });
  return plan;
}

function componentReport(component) {
  return createComponentQualification(
    candidateAuthority,
    component,
    COMPONENT_QUALIFICATION_COMMANDS.get(component),
  );
}

test("focused qualification replaces only the changed component and carries every deployed receipt", () => {
  const baseline = acceptedLedger();
  const ledger = createFastFixQualificationLedger({
    authority: candidateAuthority,
    impact: impactPlan(["apps/clearra-web/src/routes/+page.svelte"]),
    baseline,
    componentReports: [componentReport("pages")],
  });

  assert.equal(ledger.schema_id, FAST_FIX_LEDGER_SCHEMA);
  assert.equal(ledger.status, "qualified-not-deployed");
  assert.equal(ledger.production_mutation, false);
  assert.deepEqual(ledger.changed_components, ["pages"]);
  assert.equal(ledger.baseline.kind, ACCEPTED_COMPONENT_LEDGER_BASELINE_KIND);
  assert.equal(
    ledger.baseline.ledger_report_sha256,
    baseline.report_sha256,
  );
  assert.equal(ledger.components[0].disposition, "qualified-not-deployed");
  assert.equal(ledger.components[0].accepted_digest, null);
  assert.equal(ledger.components[0].accepted_receipt_sha256, null);
  assert.equal(ledger.components[0].deployment_receipt_sha256, null);
  assert.equal(
    ledger.components[0].qualification_receipt_sha256,
    componentReport("pages").report_sha256,
  );
  for (const entry of ledger.components.slice(1)) {
    assert.equal(entry.disposition, "carry-forward");
    assert.equal(
      entry.deployment_receipt_sha256,
      entry.prior_deployment_receipt_sha256,
    );
  }
  assert.equal(validateFastFixQualificationLedger(ledger), ledger);
});

test("documentation-only qualification carries all components without a component report", () => {
  const ledger = createFastFixQualificationLedger({
    authority: candidateAuthority,
    impact: impactPlan(["docs/test-policy.md"]),
    baseline: acceptedLedger(),
    componentReports: [],
  });
  assert.equal(ledger.gate_mode, "none");
  assert.deepEqual(ledger.changed_components, []);
  assert.equal(
    ledger.components.every((entry) => entry.disposition === "carry-forward"),
    true,
  );
});

test("full-gate impact and incomplete component evidence fail closed", () => {
  assert.throws(
    () => createFastFixQualificationLedger({
      authority: candidateAuthority,
      impact: impactPlan(["crates/clearra-app/src/lib.rs"]),
      baseline: acceptedLedger(),
      componentReports: [],
    }),
    /full-gate/u,
  );
  assert.throws(
    () => createFastFixQualificationLedger({
      authority: candidateAuthority,
      impact: impactPlan(["apps/clearra-web/src/routes/+page.svelte"]),
      baseline: acceptedLedger(),
      componentReports: [],
    }),
    /evidence set differs/u,
  );
});

test("a re-sealed non-Pages focused plan is rejected by the evidence boundary", () => {
  const impact = impactPlan(["apps/clearra-web/src/routes/+page.svelte"]);
  impact.changed_components = ["desktop_gui"];
  impact.carry_forward_components = DEPLOYMENT_COMPONENTS.filter(
    (component) => component !== "desktop_gui",
  );
  impact.component_scope = "desktop_gui";
  impact.impact_plan_sha256 = canonicalSha256({
    source_commit: impact.source_commit,
    baseline_kind: impact.baseline_kind,
    baseline_tag: impact.baseline_tag,
    baseline_commit: impact.baseline_commit,
    baseline_source_commit: impact.baseline_source_commit,
    baseline_workflow_path: impact.baseline_workflow_path,
    baseline_run_id: impact.baseline_run_id,
    baseline_run_attempt: impact.baseline_run_attempt,
    baseline_ledger_report_sha256: impact.baseline_ledger_report_sha256,
    gate_mode: impact.gate_mode,
    requires_full_gate: impact.requires_full_gate,
    changed_components: impact.changed_components,
    carry_forward_components: impact.carry_forward_components,
    changed_paths_sha256: impact.changed_paths_sha256,
  });

  assert.throws(
    () => createFastFixQualificationLedger({
      authority: candidateAuthority,
      impact,
      baseline: acceptedLedger(),
      componentReports: [componentReport("desktop_gui")],
    }),
    /restricted to Pages-only/u,
  );

  const ledger = structuredClone(createFastFixQualificationLedger({
    authority: candidateAuthority,
    impact: impactPlan(["apps/clearra-web/src/routes/+page.svelte"]),
    baseline: acceptedLedger(),
    componentReports: [componentReport("pages")],
  }));
  ledger.changed_components = ["desktop_gui"];
  ledger.carry_forward_components = DEPLOYMENT_COMPONENTS.filter(
    (component) => component !== "desktop_gui",
  );
  const { report_sha256: oldDigest, ...unsignedLedger } = ledger;
  void oldDigest;
  assert.throws(
    () => validateFastFixQualificationLedger(sealCanonicalReport(unsignedLedger)),
    /restricted to Pages-only/u,
  );
});

test("common changes produce a non-mutating canonical dispatch request receipt", () => {
  const report = createCanonicalPromotionEvidence(
    candidateAuthority,
    impactPlan(["crates/clearra-app/src/lib.rs"]),
  );
  assert.equal(report.schema_id, FAST_FIX_PROMOTION_SCHEMA);
  assert.equal(report.status, "canonical-full-gate-dispatch-requested");
  assert.equal(report.production_mutation, false);
  assert.equal(report.target_workflow_path, ".github/workflows/release-cli.yml");
});

test("rerun attempts and pre-v0.9 PC4 fast qualification are rejected", () => {
  assert.throws(
    () => createComponentQualification(
      { ...candidateAuthority, runAttempt: "2" },
      "pages",
      COMPONENT_QUALIFICATION_COMMANDS.get("pages"),
    ),
    /forbids rerun/u,
  );
  assert.throws(
    () => componentReport("pc4_lookup_service"),
    /not implemented before v0\.9/u,
  );
});

test("baseline hashes and changed-component nondeployment are closed evidence", () => {
  const baseline = structuredClone(acceptedLedger());
  baseline.components[0].deployment_receipt_sha256 = "f".repeat(64);
  assert.throws(
    () => validateAcceptedComponentLedger(baseline),
    /SHA-256 differs/u,
  );

  const ledger = structuredClone(createFastFixQualificationLedger({
    authority: candidateAuthority,
    impact: impactPlan(["apps/clearra-web/src/routes/+page.svelte"]),
    baseline: acceptedLedger(),
    componentReports: [componentReport("pages")],
  }));
  ledger.components[0].deployment_receipt_sha256 = "a".repeat(64);
  assert.throws(
    () => validateFastFixQualificationLedger(ledger),
    /SHA-256 differs/u,
  );

  const { report_sha256: ignored, ...unsigned } = ledger;
  void ignored;
  const resealed = sealCanonicalReport(unsigned);
  assert.throws(
    () => validateFastFixQualificationLedger(resealed),
    /must not claim accepted or deployed state/u,
  );

  const tamperedImpact = impactPlan([
    "apps/clearra-web/src/routes/+page.svelte",
  ]);
  tamperedImpact.changed_components = ["desktop_gui"];
  assert.throws(
    () => createFastFixQualificationLedger({
      authority: candidateAuthority,
      impact: tamperedImpact,
      baseline: acceptedLedger(),
      componentReports: [componentReport("desktop_gui")],
    }),
    /impact plan digest differs/u,
  );

  const wrongLedger = structuredClone(acceptedLedger());
  wrongLedger.source_commit = "9".repeat(40);
  const { report_sha256: oldHash, ...wrongUnsigned } = wrongLedger;
  void oldHash;
  const resealedWrongLedger = sealCanonicalReport(wrongUnsigned);
  assert.throws(
    () => createFastFixQualificationLedger({
      authority: candidateAuthority,
      impact: impactPlan(["apps/clearra-web/src/routes/+page.svelte"]),
      baseline: resealedWrongLedger,
      componentReports: [componentReport("pages")],
    }),
    /impact baseline differs/u,
  );

  const invalidBaselineAuthority = structuredClone(createFastFixQualificationLedger({
    authority: candidateAuthority,
    impact: impactPlan(["docs/test-policy.md"]),
    baseline: acceptedLedger(),
    componentReports: [],
  }));
  invalidBaselineAuthority.baseline.workflow_path = ".github/workflows/release-cli.yml";
  const { report_sha256: baselineHash, ...invalidBaselineUnsigned } = invalidBaselineAuthority;
  void baselineHash;
  assert.throws(
    () => validateFastFixQualificationLedger(sealCanonicalReport(invalidBaselineUnsigned)),
    /baseline workflow is not authoritative/u,
  );
});

test("accepted baseline schema is explicit and covers the complete component vector", () => {
  const ledger = acceptedLedger();
  assert.equal(ledger.schema_id, ACCEPTED_COMPONENT_LEDGER_SCHEMA);
  assert.deepEqual(
    ledger.components.map((entry) => entry.component),
    DEPLOYMENT_COMPONENTS,
  );
  assert.throws(
    () => createAcceptedComponentLedger(
      { ...baselineAuthority, workflowPath: ".github/workflows/release-cli.yml" },
      acceptedComponents(),
    ),
    /workflow is not authoritative/u,
  );
});

test("canonical promotion rejects duplicate component partitions even when re-sealed", () => {
  const impact = impactPlan(["crates/clearra-app/src/lib.rs"]);
  impact.changed_components = [...impact.changed_components, "pages"];
  impact.impact_plan_sha256 = canonicalSha256({
    source_commit: impact.source_commit,
    baseline_kind: impact.baseline_kind,
    baseline_tag: impact.baseline_tag,
    baseline_commit: impact.baseline_commit,
    baseline_source_commit: impact.baseline_source_commit,
    baseline_workflow_path: impact.baseline_workflow_path,
    baseline_run_id: impact.baseline_run_id,
    baseline_run_attempt: impact.baseline_run_attempt,
    baseline_ledger_report_sha256: impact.baseline_ledger_report_sha256,
    gate_mode: impact.gate_mode,
    requires_full_gate: impact.requires_full_gate,
    changed_components: impact.changed_components,
    carry_forward_components: impact.carry_forward_components,
    changed_paths_sha256: impact.changed_paths_sha256,
  });
  assert.throws(
    () => createCanonicalPromotionEvidence(candidateAuthority, impact),
    /must partition every component/u,
  );
});

test("production-tag baseline can request only a full canonical promotion", () => {
  const full = impactPlan(["crates/clearra-app/src/lib.rs"]);
  full.baseline_kind = PRODUCTION_TAG_BASELINE_KIND;
  full.baseline_tag = "v0.7.4";
  full.baseline_commit = "0".repeat(40);
  full.baseline_source_commit = full.baseline_commit;
  full.baseline_workflow_path = null;
  full.baseline_run_id = null;
  full.baseline_run_attempt = null;
  full.baseline_ledger_report_sha256 = null;
  full.impact_plan_sha256 = canonicalSha256({
    source_commit: full.source_commit,
    baseline_kind: full.baseline_kind,
    baseline_tag: full.baseline_tag,
    baseline_commit: full.baseline_commit,
    baseline_source_commit: full.baseline_source_commit,
    baseline_workflow_path: full.baseline_workflow_path,
    baseline_run_id: full.baseline_run_id,
    baseline_run_attempt: full.baseline_run_attempt,
    baseline_ledger_report_sha256: full.baseline_ledger_report_sha256,
    gate_mode: full.gate_mode,
    requires_full_gate: full.requires_full_gate,
    changed_components: full.changed_components,
    carry_forward_components: full.carry_forward_components,
    changed_paths_sha256: full.changed_paths_sha256,
  });
  assert.equal(
    createCanonicalPromotionEvidence(candidateAuthority, full).status,
    "canonical-full-gate-dispatch-requested",
  );

  const focused = impactPlan([
    "apps/clearra-web/src/routes/+page.svelte",
  ]);
  focused.baseline_kind = PRODUCTION_TAG_BASELINE_KIND;
  focused.baseline_tag = "v0.7.4";
  focused.baseline_commit = "0".repeat(40);
  focused.baseline_source_commit = focused.baseline_commit;
  focused.baseline_workflow_path = null;
  focused.baseline_run_id = null;
  focused.baseline_run_attempt = null;
  focused.baseline_ledger_report_sha256 = null;
  focused.impact_plan_sha256 = canonicalSha256({
    source_commit: focused.source_commit,
    baseline_kind: focused.baseline_kind,
    baseline_tag: focused.baseline_tag,
    baseline_commit: focused.baseline_commit,
    baseline_source_commit: focused.baseline_source_commit,
    baseline_workflow_path: focused.baseline_workflow_path,
    baseline_run_id: focused.baseline_run_id,
    baseline_run_attempt: focused.baseline_run_attempt,
    baseline_ledger_report_sha256: focused.baseline_ledger_report_sha256,
    gate_mode: focused.gate_mode,
    requires_full_gate: focused.requires_full_gate,
    changed_components: focused.changed_components,
    carry_forward_components: focused.carry_forward_components,
    changed_paths_sha256: focused.changed_paths_sha256,
  });
  assert.throws(
    () => createFastFixQualificationLedger({
      authority: candidateAuthority,
      impact: focused,
      baseline: acceptedLedger(),
      componentReports: [componentReport("pages")],
    }),
    /accepted-ledger baseline/u,
  );
});
