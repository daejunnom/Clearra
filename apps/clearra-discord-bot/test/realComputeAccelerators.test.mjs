// SRP rationale: verify the production compute-data provisioning adapter with
// an actual CLI and signed assets. This is not an accepted image, Cloud Run,
// performance measurement, or a second implementation of the solver.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { realpath } from 'node:fs/promises';
import { isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { prepareComputeAccelerators } from '../scripts/provision-v081-accelerators.mjs';

const executable = process.env.CLEARRA_REAL_COMPUTE_CLI;
const assetRoot = process.env.CLEARRA_REAL_COMPUTE_ASSET_ROOT;
const sourceCommit = process.env.CLEARRA_REAL_COMPUTE_SOURCE_COMMIT;
const mode = process.env.CLEARRA_REAL_COMPUTE_MODE ?? 'verify';
const profiles = ['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick'];
const products = [
  { command: 'legal-board', directory: 'legal-board' },
  { command: 'reachability-pack', directory: 'conditioned-reachability' },
];
const policyPairs = [[false, false], [true, false], [false, true], [true, true]];
const inputs = [
  { name: 'eligible-empty-4L', expected: () => 159,
    arguments: ['--lines', '4', '--height', '4', '--board-mask', '0', '--pieces', '10',
      '--queue', 'IIOOOIIOOO', '--no-hold'] },
  { name: 'outside-initial-4L',
    expected: profile => ({ srs: 245, 'srs-plus': 246, 'srs-x': 289, 'jstris-180': 246, 'no-kick': 175 })[profile],
    arguments: ['--lines', '4', '--height', '4', '--board-mask', '0x3c0f03c0f', '--pieces', '6',
      '--patterns', 'P7', '--hold', 'empty'] },
];

function invoke(arguments_, options = {}) {
  return JSON.parse(execFileSync(executable, arguments_, {
    encoding: 'utf8', timeout: 120_000, maxBuffer: 1024 * 1024,
    windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'], ...options,
  }));
}

function exactResult(input, profile, legalBoard, conditionedReachability) {
  const value = invoke(['--format', 'json', '--include-solution-data', 'pc', ...input.arguments,
    '--objective', 'unique', '--count', 'unique', '--solution-probabilities',
    '--backend', 'cpu', '--workers', '1', '--rule', profile, '--no-tablebase',
    legalBoard ? '--legal-board' : '--no-legal-board',
    conditionedReachability ? '--conditioned-reachability' : '--no-conditioned-reachability'], {
    env: { ...process.env,
      CLEARRA_LEGAL_BOARD_DIRECTORY: join(assetRoot, 'legal-board'),
      CLEARRA_CONDITIONED_REACHABILITY_DIRECTORY: join(assetRoot, 'conditioned-reachability') },
  });
  const summary = value.summary;
  assert.equal(value.schema_version, 2);
  assert.equal(value.kind, 'pc-scenario');
  assert.equal(value.runtime_identity?.source_commit, sourceCommit);
  assert.equal(value.runtime_identity?.engine_build_id, sourceCommit);
  assert.equal(summary?.rule_profile, profile);
  assert.equal(summary.tablebase_status, 'disabled');
  assert.equal(summary.backend_fallback_used, false);
  assert.equal(summary.workers_used, 1);
  assert.equal(value.resource_report?.truncated, false);
  assert.equal(value.contract?.solution_data?.status, 'complete');
  for (const field of ['solution_found', 'solution_count_calculated', 'solution_set_materialized',
    'solution_keys_complete', 'coverage_calculated', 'probability_calculated', 'probability_complete',
    'count_complete', 'objective_complete']) assert.equal(summary[field], true, field);
  assert.equal(summary.conditioned_reachability_policy_enabled, String(conditionedReachability));
  assert.equal(summary.conditioned_reachability_snapshot_active, String(conditionedReachability));
  const artifacts = value.contract.artifacts;
  const keys = artifacts?.solution_keys;
  assert.ok(Array.isArray(keys));
  assert.equal(keys.length, input.expected(profile));
  assert.equal(summary.unique_solution_count, keys.length);
  assert.equal(summary.solution_keys_materialized_count, keys.length);
  assert.equal(new Set(keys).size, keys.length);
  assert.deepEqual([...keys].sort(), keys);
  assert.equal(artifacts.solution_probabilities.length, keys.length);
  if (input.name === 'outside-initial-4L') {
    assert.equal(summary.legal_board_verified_negative_prunes, '0');
  }
  if (input.name === 'eligible-empty-4L' && profile === 'srs-plus' && legalBoard && !conditionedReachability) {
    assert.ok(Number(summary.legal_board_verified_negative_prunes) > 0,
      'the qualified native legal-board must prune, not only pass admission');
  }
  return { keys, setHash: summary.normalized_solution_set_hash,
    covered: summary.covered_pattern_count, total: summary.total_possible_pattern_count,
    probability: summary.coverage_probability, solutionProbabilities: artifacts.solution_probabilities };
}

function installedSnapshot() {
  return products.flatMap(product => profiles.map(profile => {
    const status = invoke([product.command, 'status', '--profile', profile,
      '--directory', join(assetRoot, product.directory), '--format', 'json']);
    assert.equal(status.installed, true);
    assert.equal(status.qualified, true);
    assert.equal(status.validation, 'ready');
    return { command: product.command, profile, catalog: status.catalog_identity,
      generation: status.installed_generation_identity, bytes: status.installed_payload_bytes };
  }));
}

test('real CLI provisions the ten signed compute slots and consumes them without changing results', {
  skip: !executable && !assetRoot && !sourceCommit,
  timeout: 1_200_000,
}, async t => {
  assert.ok(['provision', 'verify'].includes(mode));
  assert.match(sourceCommit, /^[0-9a-f]{40}$/u);
  assert.ok(isAbsolute(executable) && isAbsolute(assetRoot));
  const repository = await realpath(fileURLToPath(new URL('../../..', import.meta.url)));
  const expectedRoot = join(repository, '_local', 'artifacts', 'v081-compute-data-smoke');
  assert.equal(resolve(assetRoot), expectedRoot);
  // Only the explicitly supplied ordinary product CLI is eligible. The
  // supervisor/storage verifier owns output-path admission before this test.
  const actualExecutable = await realpath(executable);
  const filename = process.platform === 'win32' ? 'clearra.exe' : 'clearra';
  const binaryPaths = ['debug', 'release'].map(profile =>
    join(repository, 'build', 'cargo', 'default', profile, filename));
  assert.ok(binaryPaths.includes(actualExecutable));

  // The default adapter invokes the real CLI; deliberately no fake invoke.
  // Provision is an explicit opt-in. A rerun defaults to status-only verify.
  assert.deepEqual(prepareComputeAccelerators({ mode, version: '0.8.1', executable, root: assetRoot }),
    { required: true, profiles: 5 });
  assert.equal(await realpath(assetRoot), expectedRoot);
  const before = installedSnapshot();
  assert.deepEqual(prepareComputeAccelerators({ mode: 'verify', version: '0.8.1', executable, root: assetRoot }),
    { required: true, profiles: 5 });
  for (const profile of profiles) {
    for (const input of inputs) {
      const baseline = exactResult(input, profile, false, false);
      for (const [legalBoard, conditionedReachability] of policyPairs.slice(1)) {
        assert.deepEqual(exactResult(input, profile, legalBoard, conditionedReachability), baseline,
          `${profile}/${input.name}/${legalBoard}/${conditionedReachability}`);
      }
      t.diagnostic(`${profile}/${input.name}: ${baseline.keys.length} complete identities, four policies agree`);
    }
  }
  assert.deepEqual(installedSnapshot(), before, 'execution must not replace the installed generations');
  t.diagnostic('ten signed slots verified; native adapter proof only, not a read-only container or Cloud Run proof');
});
