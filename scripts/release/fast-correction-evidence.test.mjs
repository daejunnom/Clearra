import assert from "node:assert/strict";
import test from "node:test";

import {
  classifyFastCorrectionEntries,
  loadOwnerManifest,
} from "./fast-correction-plan.mjs";
import {
  createFastCorrectionEvidence,
  validateFastCorrectionEvidence,
} from "./fast-correction-evidence.mjs";

const BASE = "1".repeat(40);
const CANDIDATE = "2".repeat(40);
const loaded = loadOwnerManifest();

function plan(path) {
  const entries = path === null ? [] : [{
    status: "M",
    path,
    oldPath: null,
    oldMode: "100644",
    newMode: "100644",
    oldObject: "3".repeat(40),
    newObject: "4".repeat(40),
  }];
  return classifyFastCorrectionEntries({
    baseCommit: BASE,
    baseTag: "v0.8.0",
    candidateCommit: CANDIDATE,
    entries,
    manifest: loaded.manifest,
    manifestSha256: loaded.sha256,
  });
}

function create(planValue, outcome, pagesDeployment = null) {
  return createFastCorrectionEvidence({
    plan: planValue,
    baseAcceptedRunId: "100",
    baseAcceptedRunAttempt: "1",
    workflowRunId: "200",
    workflowRunAttempt: "1",
    outcome,
    pagesDeployment,
  });
}

function pages() {
  return {
    build_artifact_id: "301",
    build_artifact_digest: `sha256:${"5".repeat(64)}`,
    pages_artifact_id: "302",
    pages_artifact_digest: `sha256:${"6".repeat(64)}`,
    deployment_id: CANDIDATE,
    page_url: "https://daejunnom.github.io/Clearra/",
    live_identity_sha256: "7".repeat(64),
  };
}

test("Pages evidence binds canonical base, candidate, diff, selection, artifacts, and public identity", () => {
  const evidence = create(plan("apps/clearra-web/src/routes/+page.svelte"), "pages-deployed", pages());
  assert.equal(evidence.accepted_base.source_commit, BASE);
  assert.equal(evidence.accepted_base.tag, "v0.8.0");
  assert.equal(evidence.candidate_commit, CANDIDATE);
  assert.equal(evidence.outcome, "pages-deployed");
  assert.deepEqual(evidence.affected_products, ["pages"]);
  assert.deepEqual(evidence.skipped_products, ["cli", "discord", "gui"]);
  assert.equal(evidence.pages_deployment.live_identity_sha256, "7".repeat(64));
  validateFastCorrectionEvidence(evidence, { expectedCandidateCommit: CANDIDATE });
});

test("documentation and workflow correction evidence cannot invent a deployment", () => {
  for (const path of ["docs/test-policy.md", ".github/workflows/finalize-release-publication.yml"]) {
    const evidence = create(plan(path), "passed-no-deploy");
    assert.equal(evidence.pages_deployment, null);
    assert.equal(evidence.status, "passed");
    validateFastCorrectionEvidence(evidence);
    assert.throws(() => create(plan(path), "pages-deployed", pages()), /closed plan/u);
  }
});

test("a full-required classification produces blocked evidence only", () => {
  const fullPlan = plan("crates/clearra-app/src/lib.rs");
  const evidence = create(fullPlan, "full-required");
  assert.equal(evidence.status, "blocked");
  assert.equal(evidence.outcome, "full-required");
  assert.throws(() => create(fullPlan, "passed-no-deploy"), /cannot produce/u);
});

test("no-op evidence never deploys", () => {
  const noOp = plan(null);
  validateFastCorrectionEvidence(create(noOp, "passed-no-deploy"));
  assert.throws(() => create(noOp, "pages-deployed", pages()), /cannot deploy/u);
});

test("artifact, deployment, URL, and evidence tampering fail closed", () => {
  const pagesPlan = plan("apps/clearra-web/src/routes/+page.svelte");
  for (const mutate of [
    (value) => { value.pages_deployment.build_artifact_digest = "bad"; },
    (value) => { value.pages_deployment.pages_artifact_id = value.pages_deployment.build_artifact_id; },
    (value) => { value.pages_deployment.deployment_id = BASE; },
    (value) => { value.pages_deployment.page_url = "http://example.com/"; },
    (value) => { value.selected.tests = []; },
  ]) {
    const value = structuredClone(create(pagesPlan, "pages-deployed", pages()));
    mutate(value);
    assert.throws(() => validateFastCorrectionEvidence(value), /hash mismatch|invalid|differs|distinct|canonical/u);
  }
});
