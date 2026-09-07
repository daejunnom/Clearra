import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import {
  collectReleaseFailureSummary, DIAGNOSTIC_JOB_DEPENDENCIES, renderReleaseFailureSummary,
} from './release-failure-summary.mjs';

const allSuccess = () => Object.fromEntries(
  Object.keys(DIAGNOSTIC_JOB_DEPENDENCIES).map((name) => [name, { result: 'success' }]),
);
const workflow = readFileSync(new URL('../../.github/workflows/release-cli.yml', import.meta.url), 'utf8');
function jobSource(name) {
  return workflow.split(`  ${name}:`)[1]?.split(/\r?\n  [a-z][a-z0-9-]*:\r?\n/)[0] ?? '';
}
function dependencies(source) {
  const match = /^    needs:\s*(?:\[([^\]]+)\]|([^\r\n]+))/m.exec(source);
  return match ? (match[1] ?? match[2]).split(',').map((name) => name.trim()).sort() : [];
}

test('all-success summary never creates acceptance authority', () => {
  const report = collectReleaseFailureSummary(allSuccess());
  assert.equal(report.status, 'all-jobs-succeeded');
  assert.equal(report.release_authority, false);
  assert.equal(report.jobs.length, 15);
  assert.match(renderReleaseFailureSummary(report), /cannot authorize acceptance or publication/);
});

test('independent failures and downstream blocks are all visible', () => {
  const needs = allSuccess();
  for (const name of ['release-acceptance-rust', 'release-acceptance-wasm-build', 'windows-cli']) {
    needs[name].result = 'failure';
  }
  for (const name of ['release-acceptance-pages', 'release-acceptance', 'canonical-evidence']) {
    needs[name].result = 'skipped';
  }
  const report = collectReleaseFailureSummary(needs);
  assert.equal(report.status, 'failed-or-blocked');
  assert.equal(report.jobs.filter((job) => job.result === 'failure').length, 3);
  assert.deepEqual(report.jobs.find((job) => job.name === 'release-acceptance-pages').blocked_by,
    ['release-acceptance-wasm-build']);
  assert.equal(report.jobs.find((job) => job.name === 'release-acceptance-pages').diagnostic_status, 'blocked');
  assert.equal(report.jobs.find((job) => job.name === 'windows-gui').result, 'success');
});

test('cancelled or unexplained skipped jobs cannot pass', () => {
  for (const result of ['cancelled', 'skipped']) {
    const needs = allSuccess();
    needs['canonical-evidence'].result = result;
    const report = collectReleaseFailureSummary(needs);
    assert.equal(report.status, 'failed-or-blocked');
    assert.equal(report.jobs.at(-1).diagnostic_status, result);
  }
});

test('missing, extra, nonterminal and inconsistent states fail closed', () => {
  const missing = allSuccess();
  delete missing.ctk3;
  const unexpected = { ...allSuccess(), unknown: { result: 'success' } };
  const nonterminal = allSuccess();
  nonterminal.ctk3.result = 'running';
  const inconsistent = allSuccess();
  inconsistent.ctk3.result = 'failure';
  for (const input of [null, [], missing, unexpected, nonterminal, inconsistent]) {
    assert.throws(() => collectReleaseFailureSummary(input));
  }
});

test('workflow summary awaits the exact graph without changing artifact prerequisites', () => {
  for (const [name, expected] of Object.entries(DIAGNOSTIC_JOB_DEPENDENCIES)) {
    assert.deepEqual(dependencies(jobSource(name)), [...expected].sort(), name);
  }
  const summary = jobSource('release-failure-summary');
  assert.match(summary, /if: always\(\) && github\.event_name == 'workflow_dispatch'/);
  assert.deepEqual(dependencies(summary), Object.keys(DIAGNOSTIC_JOB_DEPENDENCIES).sort());
  assert.doesNotMatch(summary, /continue-on-error|upload-artifact|download-artifact|GH_TOKEN|actions: write/);
  assert.doesNotMatch(jobSource('canonical-evidence'), /release-failure-summary/);
  assert.doesNotMatch(jobSource('publish'), /release-failure-summary/);
});

test('CLI prints every failure before returning a nonzero status', () => {
  const needs = allSuccess();
  needs['canonical-evidence'].result = 'failure';
  const result = spawnSync(process.execPath, [fileURLToPath(new URL('./release-failure-summary.mjs', import.meta.url))], {
    env: { ...process.env, GITHUB_STEP_SUMMARY: '', CLEARRA_DIAGNOSTIC_NEEDS: JSON.stringify(needs) },
    encoding: 'utf8', windowsHide: true,
  });
  assert.equal(result.status, 1);
  assert.match(result.stdout, /"release_authority": false/);
  assert.match(result.stdout, /\| canonical-evidence \| failure \|/);
  assert.match(result.stdout, /\| windows-gui \| success \|/);
});
