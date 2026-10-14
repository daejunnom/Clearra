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

test("qualification workflow has no direct production publication authority", () => {
  assert.doesNotMatch(workflow, /^\s*push:/mu);
  assert.doesNotMatch(workflow, /^\s*workflow_run:/mu);
  assert.doesNotMatch(workflow, /actions\/deploy-pages|gcloud\s+run\s+services\s+update-traffic|gh\s+release\s+(?:create|upload)|discord\.com\/api/u);
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
