import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const primary = await readFile(
  new URL("../../.github/workflows/discord-deploy.yml", import.meta.url),
  "utf8",
);
const recovery = await readFile(
  new URL("../../.github/workflows/discord-deploy-recovery.yml", import.meta.url),
  "utf8",
);
const release = await readFile(
  new URL("../../.github/workflows/release-cli.yml", import.meta.url),
  "utf8",
);
const releaseRegressions = await readFile(
  new URL("../tools/run-release-regression-tests.mjs", import.meta.url),
  "utf8",
);

test("primary has exact automatic/manual authority and explicit impact no-op", () => {
  for (const marker of [
    'workflows: ["Publish Product Release"]',
    "workflow_dispatch:",
    "accepted_sha:",
    "github.event.workflow_run.event == 'workflow_dispatch'",
    "github.event.workflow_run.head_branch == 'main'",
    "github.event.workflow_run.head_repository.full_name == github.repository",
    "--require one",
    "--expected-run-id",
    "--expected-run-attempt",
    "Discord source must be exact current main",
    "deployment-impact.mjs",
    "deploy_discord != 'true'",
  ]) assert.ok(primary.includes(marker), marker);
  for (const component of [
    "deploy_discord_gateway",
    "deploy_heavy_cloud_runtime",
    "deploy_pc4_lookup_service",
    "deploy_pc4_activation_manifest",
    "release_infrastructure_changed",
  ]) {
    assert.match(primary, new RegExp(`needs\\.authority\\.outputs\\.${component} == 'true'`, "u"));
  }
  const authorityCheckout = primary.slice(
    primary.indexOf("Check out main for exact authority resolution"),
    primary.indexOf("Resolve exact current main and one canonical acceptance"),
  );
  assert.match(authorityCheckout, /ref: main[\s\S]*fetch-depth: 0/u);
  assert.match(primary, /git fetch --force --tags origin main/u);
  assert.doesNotMatch(primary, /Discord deployment rerun attempts are forbidden/u);
});

test("primary and recovery share one non-cancelling production serialization group", () => {
  for (const workflow of [primary, recovery]) {
    assert.match(workflow, /group: discord-production/u);
    assert.match(workflow, /cancel-in-progress: false/u);
    assert.match(workflow, /queue: max/u);
    assert.doesNotMatch(workflow, /queue: single/u);
  }
});

test("the three protected subjects remain distinct across the two workflows", () => {
  assert.match(primary, /environment: discord-path-confirmation/u);
  assert.match(primary, /environment: discord-global-command-sync/u);
  assert.doesNotMatch(primary, /environment: discord-runtime-rollback/u);
  assert.doesNotMatch(recovery, /environment: discord-path-confirmation/u);
  assert.doesNotMatch(recovery, /environment: discord-global-command-sync/u);
  assert.match(recovery, /environment: discord-runtime-rollback/u);
});

test("approval-free preparation uses only builder WIF and never SSH/runtime mutation", () => {
  const candidate = primary.slice(primary.indexOf("  candidate:"), primary.indexOf("  promote:"));
  assert.match(candidate, /GCP_BUILD_SERVICE_ACCOUNT/u);
  assert.doesNotMatch(candidate, /GCP_DEPLOY_SERVICE_ACCOUNT|GCP_ROLLBACK_SERVICE_ACCOUNT/u);
  assert.doesNotMatch(candidate, /ORACLE_SSH_PRIVATE_KEY_B64|ORACLE_PRIVATE_OVERLAY/u);
  assert.doesNotMatch(candidate, /invoke-(?:freeze|inactive-stage)-v080/u);
  assert.doesNotMatch(candidate, /candidate-release-v080\.mjs deploy|run services update-traffic/u);
});

test("Cloud Build stages the accepted archive in the exact bucket without the default project-wide ownership probe", () => {
  const candidate = primary.slice(primary.indexOf("  candidate:"), primary.indexOf("  promote:"));
  const exactStagingArgument =
    '--gcs-source-staging-dir="gs://clearra-cloud_cloudbuild/source"';
  assert.equal(candidate.split(exactStagingArgument).length - 1, 1);
  assert.ok(
    candidate.indexOf(exactStagingArgument) >
      candidate.indexOf("gcloud builds submit evidence/exact-source.tar.gz"),
  );
});

test("prestage and live recovery artifacts bracket every protected runtime transition", () => {
  const prestageUpload = primary.indexOf("Upload prestage authority before Oracle freeze or Cloud zero traffic");
  const freeze = primary.indexOf("invoke-freeze-v080.ps1", prestageUpload);
  const zeroTraffic = primary.indexOf("candidate-release-v080.mjs deploy", prestageUpload);
  const liveUpload = primary.indexOf("Upload live-transition authority before Oracle activation or Cloud traffic", prestageUpload);
  const activation = primary.indexOf("-Operation verify-candidate", liveUpload);
  const cutover = primary.indexOf("run services update-traffic", liveUpload);
  assert.ok(prestageUpload >= 0 && freeze > prestageUpload && zeroTraffic > prestageUpload);
  assert.ok(liveUpload > zeroTraffic && activation > liveUpload && cutover > liveUpload);
  assert.match(primary, /discord-prestage-recovery-authority-\$\{\{ needs\.authority\.outputs\.source_commit \}\}-run-/u);
  assert.match(primary, /discord-live-recovery-authority-\$\{\{ needs\.authority\.outputs\.source_commit \}\}-run-/u);
});

test("ordinary cancellation preserves in-job path and catalog compensation", () => {
  assert.match(primary, /promote:\n(?:\s+#.*\n)*\s+if: always\(\) && needs\.candidate\.result == 'success'/u);
  assert.match(primary, /sync-observe:\n(?:\s+#.*\n)*\s+if: always\(\) && needs\.promote\.result == 'success'/u);
  assert.match(primary, /Compensate any protected-path failure[^\n]*\n\s+if: failure\(\) \|\| cancelled\(\)/u);
  assert.match(primary, /Compensate catalog mutation[^\n]*\n\s+if: failure\(\) \|\| cancelled\(\)/u);
  assert.doesNotMatch(primary, /^  rollback-after-sync:/mu);
});

test("recovery executes only trusted default-branch code and exact current run-attempt authority", () => {
  assert.match(recovery, /workflow_run:\n\s+workflows: \["Deploy Discord Production"\]/u);
  assert.equal((recovery.match(/ref: \$\{\{ github\.sha \}\}/gu) ?? []).length, 2);
  assert.doesNotMatch(recovery, /ref: \$\{\{ github\.event\.workflow_run\.head_sha \}\}/u);
  for (const marker of [
    "attempts/$ORIGINAL_RUN_ATTEMPT",
    "primary-run-catalog.json",
    "original-jobs.json",
    "--job-list",
    "discord-deployment-recovery.mjs resolve",
    "GCP_ROLLBACK_WORKLOAD_IDENTITY_PROVIDER",
    "GCP_ROLLBACK_SERVICE_ACCOUNT",
  ]) assert.ok(recovery.includes(marker), marker);
  assert.match(recovery, /if:\s*>-\n\s+always\(\) &&\n\s+needs\.authority\.result == 'success'/u);
});

test("recovery hard-fails artifact bytes before extraction and binds the resolution", () => {
  assert.match(recovery, /actions\/artifacts\/\$ARTIFACT_ID\/zip/u);
  assert.match(recovery, /sha256:\$actual" == "\$ARTIFACT_DIGEST/u);
  assert.match(recovery, /artifact-ids: \$\{\{ needs\.authority\.outputs\.artifact_id \}\}/u);
  assert.match(recovery, /path: \$\{\{ runner\.temp \}\}\/discord-recovery-input/u);
  assert.match(recovery, /-RecoveryAuthorityPath/u);
  assert.match(recovery, /Re-resolve exact recovery authority immediately before protected mutation/u);
});

test("rollback SSH and WIF authority exist only in reviewer-protected recovery", () => {
  assert.match(recovery, /environment: discord-runtime-rollback/u);
  assert.match(recovery, /ORACLE_SSH_PRIVATE_KEY_B64: \$\{\{ secrets\.ORACLE_SSH_PRIVATE_KEY_B64 \}\}/u);
  assert.match(recovery, /base64 --decode/u);
  assert.match(recovery, /chmod 0600/u);
  assert.match(recovery, /if: always\(\)[\s\S]*rm -f -- "\$key_path"/u);
  assert.doesNotMatch(recovery, /GCP_DEPLOY_SERVICE_ACCOUNT/u);
  assert.match(recovery, /GCP_ROLLBACK_SERVICE_ACCOUNT/u);
  assert.match(recovery, /GCP_COMMAND_SYNC_SERVICE_ACCOUNT/u);
  assert.equal((recovery.match(/environment: discord-runtime-rollback/gu) ?? []).length, 1);
  assert.match(recovery, /always\(\) && steps\.restore\.outcome != 'success'/u);
  assert.match(
    recovery,
    /Preserve and verify the exact terminal recovery evidence as the sole success authority/u,
  );
  assert.match(recovery, /if-no-files-found: error/u);
  assert.match(recovery, /force-cancel path/u);
});

test("Oracle key materialization uses the Ubuntu runner's supported base64 decoder", () => {
  const combined = `${primary}\n${recovery}`;
  assert.equal((primary.match(/base64 --decode > "\$key_path"/gu) ?? []).length, 2);
  assert.equal((recovery.match(/base64 --decode > "\$key_path"/gu) ?? []).length, 1);
  assert.doesNotMatch(combined, /base64 --decode --strict/u);
});

test("build/test/tag/publication work is absent from Discord deploy workflows", () => {
  const combined = `${primary}\n${recovery}`;
  for (const forbidden of [
    /npm\s+(?:run\s+)?test/iu,
    /node\s+--test/iu,
    /cargo\s+(?:test|build)/iu,
    /ReleaseAcceptance/u,
    /npm\s+run\s+build/iu,
    /git\s+tag\s+(?:-[asufdD]|--(?:annotate|sign|force|delete))/iu,
    /gh\s+release/iu,
  ]) assert.doesNotMatch(combined, forbidden);
});

test("canonical dispatch owns every Discord authority regression in one bounded pool", () => {
  const name = "Validate independent release regressions with bounded workers";
  const start = release.indexOf(`- name: ${name}`);
  const end = release.indexOf("\n      - name:", start + 1);
  assert.ok(start >= 0 && end > start, "bounded release regression step");
  const step = release.slice(start, end);
  assert.match(step, /if: github\.event_name == 'workflow_dispatch'/u);
  assert.match(step, /node scripts\/tools\/run-release-regression-tests\.mjs/u);
  assert.match(releaseRegressions, /ACTIONS_TEST_WORKER_CAP = 4/u);
  assert.match(releaseRegressions, /--test-concurrency=\$\{workers\}/u);
  for (const file of [
    "scripts/release/deployment-impact.test.mjs",
    "scripts/release/discord-catalog-recovery-authority.test.mjs",
    "scripts/release/discord-deploy-workflow.test.mjs",
    "scripts/release/discord-deployment-recovery.test.mjs",
    "scripts/release/discord-deployment-state.test.mjs",
    "scripts/release/discord-production-checkpoint-receipt.test.mjs",
    "scripts/release/discord-recovery-debt.test.mjs",
  ]) {
    assert.equal(
      releaseRegressions.split(`\"${file}\"`).length - 1,
      1,
      `${file} exact owner`,
    );
  }
  assert.equal((release.match(new RegExp(name, "gu")) ?? []).length, 1);
});

test("catalog preimage and checkpoint candidate are durable before their dependent mutations", () => {
  const capture = primary.indexOf(
    "Capture and seal Discord catalog recovery authority before mutation",
  );
  const preimageUpload = primary.indexOf(
    "Upload Discord catalog recovery authority before global mutation",
  );
  const mutation = primary.indexOf(
    "Authority-bound global sync and sole canonical four-surface observation",
  );
  const observationUpload = primary.indexOf(
    "Upload durable sync and sole canonical observation evidence",
  );
  const candidateCapture = primary.indexOf(
    "Capture exact completed deployment prerequisites for the checkpoint candidate",
  );
  const candidateSeal = primary.indexOf(
    "Seal canonical Discord production checkpoint candidate",
  );
  const candidateUpload = primary.indexOf(
    "Upload canonical Discord production checkpoint candidate",
  );
  assert.ok(
    capture >= 0 && preimageUpload > capture && mutation > preimageUpload &&
    observationUpload > mutation && candidateCapture > observationUpload &&
    candidateSeal > candidateCapture && candidateUpload > candidateSeal,
  );
  assert.match(primary, /if-no-files-found: error[\s\S]*discord-production-checkpoint-candidate/u);
  assert.match(primary, /verify-prerequisites/u);
  assert.match(primary, /verify-candidate/u);
  assert.doesNotMatch(primary, /discord_completed_at|candidate_upload_completed_at/u);
});

test("primary and recovery enumerate every exact workflow rerun attempt", () => {
  for (const workflow of [primary, recovery]) {
    assert.match(workflow, /for \(\(attempt = 1; attempt <= max_attempt; attempt \+= 1\)\)/u);
    assert.match(workflow, /actions\/runs\/\$run_id\/attempts\/\$attempt/u);
  }
  assert.match(recovery, /--run-attempt-catalog/u);
  assert.match(recovery, /--run-job-catalog/u);
});

test("global sync stays command-only and owns the sole four-surface observer", () => {
  const sync = primary.slice(primary.indexOf("  sync-observe:"));
  assert.match(sync, /environment: discord-global-command-sync/u);
  assert.match(sync, /GCP_COMMAND_SYNC_SERVICE_ACCOUNT/u);
  assert.doesNotMatch(sync, /GCP_ROLLBACK_SERVICE_ACCOUNT/u);
  assert.equal((primary.match(/observe-production-surfaces\.mjs/gu) ?? []).length, 1);
  assert.doesNotMatch(primary, /sleep 1200|discord-deployment-observation/u);
  assert.match(primary, /pages-deployment-run\.mjs resolve/u);
});

test("the repository/organization SSH secret scope is explicitly forbidden", () => {
  assert.match(primary, /ORACLE_SSH_PRIVATE_KEY_B64 is forbidden as a repository or organization secret/u);
  assert.doesNotMatch(`${primary}\n${recovery}`, /ORACLE_SSH_PRIVATE_KEY(?!_B64)/u);
  assert.doesNotMatch(`${primary}\n${recovery}`, /ssh-keyscan/u);
});
