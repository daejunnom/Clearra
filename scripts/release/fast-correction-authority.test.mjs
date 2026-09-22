import assert from "node:assert/strict";
import test from "node:test";

import { validateFastCorrectionControlAuthority } from "./fast-correction-authority.mjs";
import { createFastCorrectionEvidence } from "./fast-correction-evidence.mjs";
import { classifyFastCorrectionEntries, loadOwnerManifest } from "./fast-correction-plan.mjs";

const BASE = "1".repeat(40);
const CANDIDATE = "2".repeat(40);
const loaded = loadOwnerManifest();

function correction(path) {
  const plan = classifyFastCorrectionEntries({
    baseCommit: BASE,
    baseTag: "v0.8.0",
    candidateCommit: CANDIDATE,
    entries: [{
      status: "M",
      path,
      oldPath: null,
      oldMode: "100644",
      newMode: "100644",
      oldObject: "3".repeat(40),
      newObject: "4".repeat(40),
    }],
    manifest: loaded.manifest,
    manifestSha256: loaded.sha256,
  });
  const evidence = createFastCorrectionEvidence({
    plan,
    baseAcceptedRunId: "100",
    baseAcceptedRunAttempt: "1",
    workflowRunId: "200",
    workflowRunAttempt: "1",
    outcome: plan.decision === "full-required"
      ? "full-required"
      : plan.deploy_pages
        ? "pages-deployed"
        : "passed-no-deploy",
    pagesDeployment: plan.deploy_pages ? {
      build_artifact_id: "301",
      build_artifact_digest: `sha256:${"5".repeat(64)}`,
      pages_artifact_id: "302",
      pages_artifact_digest: `sha256:${"6".repeat(64)}`,
      deployment_id: CANDIDATE,
      page_url: "https://daejunnom.github.io/Clearra/",
      live_identity_sha256: "7".repeat(64),
    } : null,
  });
  return { plan, evidence };
}

function validate(value, control) {
  return validateFastCorrectionControlAuthority({
    ...value,
    expectedBaseCommit: BASE,
    expectedCandidateCommit: CANDIDATE,
    expectedEvidenceRunId: "200",
    expectedEvidenceRunAttempt: "1",
    requiredControl: control,
  });
}

test("Discord workflow-only evidence authorizes only a base-product Discord follow-up", () => {
  const value = correction(".github/workflows/discord-deploy.yml");
  const authority = validate(value, "discord");
  assert.equal(authority.productSourceCommit, BASE);
  assert.equal(authority.workflowSourceCommit, CANDIDATE);
  assert.equal(authority.acceptedRunId, "100");
  assert.throws(() => validate(value, "pages"), /does not qualify/u);
});

test("Pages workflow-only evidence authorizes only a base-product Pages follow-up", () => {
  const value = correction(".github/workflows/pages.yml");
  assert.equal(validate(value, "pages").requiredControl, "pages");
  assert.throws(() => validate(value, "discord"), /does not qualify/u);
});

test("docs, runtime source, full-required, and mismatched evidence never become dual authority", () => {
  assert.throws(() => validate(correction("docs/test-policy.md"), "discord"), /release-workflow-only/u);
  const runtime = correction("apps/clearra-web/src/routes/+page.svelte");
  assert.throws(() => validate(runtime, "pages"), /runtime source change/u);
  assert.throws(() => validate(correction("crates/clearra-app/src/lib.rs"), "discord"), /fast-eligible/u);
  const mismatched = correction(".github/workflows/discord-deploy.yml");
  assert.throws(() => validateFastCorrectionControlAuthority({
    ...mismatched,
    expectedBaseCommit: BASE,
    expectedCandidateCommit: "9".repeat(40),
    expectedEvidenceRunId: "200",
    expectedEvidenceRunAttempt: "1",
    requiredControl: "discord",
  }), /candidate mismatch/u);
});
