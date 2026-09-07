import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { collectQualificationJobTopology, QUALIFICATION_JOB_FLAGS } from './fast-fix-job-topology.mjs';

const workflow = await readFile(
  new URL("../../.github/workflows/fast-fix-qualification.yml", import.meta.url),
  "utf8",
);

test("fast-fix workflow is qualification-only with exact-main attempt-1 authority", () => {
  for (const marker of [
    "name: Fast Fix Qualification",
    "group: fast-fix-qualification-${{ inputs.source_commit }}",
    "cancel-in-progress: false",
    "GITHUB_RUN_ATTEMPT\" == '1'",
    "GITHUB_REF\" == 'refs/heads/main'",
    "SOURCE_COMMIT\" == \"$GITHUB_SHA",
    "git rev-parse origin/main",
    "exact source already has a fast-fix qualification run",
    "main moved before qualification fan-in",
    "Seal closed qualification-only evidence",
  ]) {
    assert.match(workflow, new RegExp(escapeRegExp(marker), "u"), marker);
  }
  assert.doesNotMatch(workflow, /^\s*push:/mu);
  assert.doesNotMatch(workflow, /^\s*workflow_run:/mu);
  assert.doesNotMatch(workflow, /actions\/deploy-pages|gcloud\s+run\s+services\s+update-traffic|gh\s+release\s+(?:create|upload)|discord\.com\/api/u);
});

test("every deployment-impact product and component vector owns a conditional job", () => {
  for (const condition of [
    "outputs.deploy_pages == 'true'",
    "outputs.deploy_gui == 'true'",
    "outputs.deploy_cli == 'true'",
    "outputs.deploy_discord == 'true'",
    "outputs.deploy_desktop_gui == 'true'",
    "outputs.deploy_discord_gateway == 'true'",
    "outputs.deploy_heavy_cloud_runtime == 'true'",
    "outputs.deploy_pc4_lookup_service == 'true'",
    "outputs.deploy_pc4_activation_manifest == 'true'",
  ]) {
    assert.ok(workflow.includes(condition), condition);
  }
  assert.match(
    workflow,
    /canonical-promotion:[\s\S]*if: needs\.authority\.outputs\.requires_full_gate == 'true'/u,
  );
  assert.match(
    workflow,
    /qualification-evidence:[\s\S]*requires_full_gate != 'true'/u,
  );
});

test("full scope promotes to canonical while focused scope uses explicit closed tests", () => {
  assert.match(
    workflow,
    /actions\/workflows\/release-cli\.yml\/dispatches/u,
  );
  assert.match(
    workflow,
    /canonical-acceptance-run\.mjs[\s\S]*--require zero/u,
  );
  for (const command of [
    "apps/clearra-web/test/ClearraWasmRuntime.contract.ts",
    "packages/clearra-ui/test/desktopProductPageCancellation.test.mjs",
    "cargo test -p clearra-cli --test product_cli_surface_contract -- --test-threads=1",
    "apps/clearra-discord-bot/test/capability-registry.test.mjs",
    "apps/clearra-discord-bot/test/cloud-candidate-smoke-job.test.mjs",
  ]) {
    assert.ok(workflow.includes(command), command);
  }
  assert.match(workflow, /PC4 lookup fast qualification is unavailable before v0\.9/u);
  assert.match(workflow, /PC4 activation fast qualification is unavailable before v0\.9/u);
});

test("latest accepted ledger is verified before ledger-relative classification", () => {
  for (const marker of [
    "actions/runs/$latest_accepted_run/attempts/1",
    "git merge-base --is-ancestor",
    "baseline is not the last successful accepted component ledger run",
    'latest_accepted_run="$(jq -r',
    "baseline ledger artifact name differs from its exact source/run authority",
    "baseline artifact pagination is incomplete or unstable",
    ".expired == false",
    "^sha256:[0-9a-f]{64}$",
    "verify-baseline",
    "--baseline-kind accepted-component-ledger",
    "--baseline-source-commit",
    "--baseline-ledger-report-sha256",
    "fast-fix focused/no-product qualification requires a bootstrapped accepted component ledger",
    "clearra-accepted-component-ledger.v1.json",
  ]) {
    assert.ok(workflow.includes(marker), marker);
  }
  assert.ok(
    workflow.indexOf("Verify the latest accepted component ledger before classification") <
      workflow.indexOf("Calculate closed deployment-impact qualification plan"),
    "accepted ledger must be verified before impact classification",
  );
  assert.match(
    workflow,
    /baseline_kind" != 'production-tag' \|\| "\$gate_mode" != 'full'/u,
  );
});

test("fan-in distinguishes selected success from unselected skip and seals no deployment receipt", () => {
  const topology = workflow.split('      - name: Require exact conditional job topology')[1]
    .split('      - name: Download exact impact and verified baseline evidence')[0];
  assert.match(topology, /run: node scripts\/release\/fast-fix-job-topology\.mjs/u);
  assert.doesNotMatch(topology, /continue-on-error|\|\| true|if: always/u);
  assert.match(workflow, /Seal closed qualification-only evidence/u);
  assert.match(workflow, /fast-fix-qualification-ledger-/u);
});

function matchedTopology() {
  return {
    GATE_MODE: 'focused', CARRY_RESULT: 'success',
    ...Object.fromEntries(QUALIFICATION_JOB_FLAGS.flatMap(([, prefix]) => [
      [`${prefix}_SELECTED`, 'false'], [`${prefix}_RESULT`, 'skipped'],
    ])),
    PAGES_SELECTED: 'true', PAGES_RESULT: 'success',
  };
}

test('qualification reports every selected failure and unselected surprise before refusing evidence', () => {
  const report = collectQualificationJobTopology({
    ...matchedTopology(), CARRY_RESULT: 'failure', PAGES_RESULT: 'failure', GUI_RESULT: 'success',
    CLOUD_SELECTED: 'true', CLOUD_RESULT: 'cancelled',
  });
  assert.equal(report.status, 'failed');
  assert.equal(report.release_authority, false);
  assert.equal(report.jobs.length, 7);
  assert.equal(report.failures.length, 4);
  assert.match(report.failures.join('; '), /carry-forward.*pages.*desktop_gui.*heavy_cloud_runtime/u);
});

test('qualification accepts only matched terminal jobs and rejects malformed flags without fallback', () => {
  assert.equal(collectQualificationJobTopology(matchedTopology()).status, 'matched');
  for (const changed of [
    { GATE_MODE: 'full' }, { PAGES_SELECTED: 'invalid' }, { PAGES_RESULT: 'in_progress' },
    { PAGES_RESULT: 'skipped' }, { GUI_RESULT: 'failure' }, { GATE_MODE: 'none' },
  ]) assert.equal(collectQualificationJobTopology({ ...matchedTopology(), ...changed }).status, 'failed');
  assert.equal(collectQualificationJobTopology({ ...matchedTopology(), GATE_MODE: 'none',
    PAGES_SELECTED: 'false', PAGES_RESULT: 'skipped' }).status, 'matched');
});

test('qualification CLI outputs all errors and exits nonzero before artifact consumption', () => {
  const result = spawnSync(process.execPath, [fileURLToPath(new URL('./fast-fix-job-topology.mjs', import.meta.url))], {
    env: { ...process.env, ...matchedTopology(), PAGES_RESULT: 'failure', GUI_RESULT: 'success' },
    encoding: 'utf8', windowsHide: true,
  });
  assert.equal(result.status, 2);
  assert.match(result.stderr, /pages expected success but was failure/u);
  assert.match(result.stderr, /desktop_gui expected skipped but was success/u);
  assert.match(result.stdout, /"release_authority": false/u);
});

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
}
