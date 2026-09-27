// SRP rationale: consume real current-source CLI products through the production
// Discord runner/direct executor and canonical projection. No fake solver,
// service, gateway message, benchmark or accepted Cloud Run image is used.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { realpath } from 'node:fs/promises';
import { isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { assertDiscordCanonicalOnlyResult, prepareClearraArguments } from '../src/clearra/command.mjs';
import { ClearraDirectExecutor } from '../src/clearra/direct-executor.mjs';
import { prepareComputeAccelerators } from '../scripts/provision-v081-accelerators.mjs';
import { realCliProductProjectionRequests, realCliProjectionProfiles as profiles }
  from './support/realCliProductProjectionRequests.mjs';

const executable = process.env.CLEARRA_REAL_COMPUTE_CLI;
const assetRoot = process.env.CLEARRA_REAL_COMPUTE_ASSET_ROOT;
const sourceCommit = process.env.CLEARRA_REAL_COMPUTE_SOURCE_COMMIT;

function invoke(arguments_) {
  return execFileSync(executable, arguments_, {
    encoding: 'utf8', timeout: 120_000, maxBuffer: 1024 * 1024,
    windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
  }).trim();
}

function installedSnapshot() {
  return ['legal-board', 'reachability-pack'].flatMap(product => profiles.map(profile => {
    const directory = product === 'legal-board' ? 'legal-board' : 'conditioned-reachability';
    const slot = JSON.parse(invoke([product, 'status', '--profile', profile,
      '--directory', join(assetRoot, directory), '--format', 'json']));
    assert.equal(slot.installed, true);
    assert.equal(slot.qualified, true);
    assert.equal(slot.validation, 'ready');
    return { product, profile, catalog: slot.catalog_identity,
      generation: slot.installed_generation_identity, bytes: slot.installed_payload_bytes };
  }));
}

test('real CLI minimum, score, replay and Build results survive production Discord hops', {
  skip: !executable && !assetRoot && !sourceCommit,
  timeout: 1_200_000,
}, async t => {
  assert.ok(isAbsolute(executable ?? '') && isAbsolute(assetRoot ?? ''), 'explicit paths required');
  assert.match(sourceCommit ?? '', /^[0-9a-f]{40}$/u);
  const repository = await realpath(fileURLToPath(new URL('../../..', import.meta.url)));
  const expectedRoot = join(repository, '_local', 'artifacts', 'v081-compute-data-smoke');
  assert.equal(resolve(assetRoot), expectedRoot);
  assert.equal(await realpath(assetRoot), expectedRoot);
  const binary = process.platform === 'win32' ? 'clearra.exe' : 'clearra';
  assert.ok(['debug', 'release'].some(profile =>
    join(repository, 'build', 'cargo', 'default', profile, binary) === resolve(executable)));
  assert.equal(await realpath(executable), resolve(executable));
  for (const [environment, directory] of [
    ['CLEARRA_LEGAL_BOARD_DIRECTORY', 'legal-board'],
    ['CLEARRA_CONDITIONED_REACHABILITY_DIRECTORY', 'conditioned-reachability'],
  ]) assert.equal(await realpath(process.env[environment] ?? ''), join(expectedRoot, directory));
  assert.deepEqual(prepareComputeAccelerators({ mode: 'verify', version: '0.8.1',
    executable, root: assetRoot }), { required: true, profiles: 5 });
  const before = installedSnapshot();
  const executor = new ClearraDirectExecutor({
    executable, processLogicalProcessors: 1, searchWorkersPerSession: 1,
    useAllLogicalProcessors: false, searchTimeoutMs: 120_000,
    interactionDeadlineMs: 150_000, maxOutputBytes: 1024 * 1024,
    maxArtifactBytes: 1024 * 1024, terminationGraceMs: 1_000,
  });
  let completed = 0;
  for (const profile of profiles) {
    for (const input of realCliProductProjectionRequests(profile)) {
      const arguments_ = input.arguments;
      const prepared = prepareClearraArguments(arguments_, { workers: 1,
        logicalProcessors: 1, outputFormat: 'json', includeSolutionData: true });
      const stdout = invoke(prepared);
      const raw = JSON.parse(stdout);
      assert.equal(raw.kind, input.kind);
      assert.equal(raw.runtime_identity?.source_commit, sourceCommit);
      assert.equal(raw.runtime_identity?.engine_build_id, sourceCommit);
      const directProjection = assertDiscordCanonicalOnlyResult({
        exitCode: 0, signal: null, stderr: '', stdout,
      });
      const actual = await executor.execute(arguments_);
      assert.equal(actual.exitCode, 0, actual.stderr);
      assert.equal(actual.signal, null);
      assert.equal(actual.terminationReason, 'exit');
      assert.deepEqual(JSON.parse(actual.stdout), JSON.parse(directProjection.stdout),
        `${profile}/${input.name}/${input.policy}: actual CLI/Discord result drift`);
      assert.equal(assertDiscordCanonicalOnlyResult(actual).stdout, actual.stdout,
        'canonical projection must remain stable across runner/direct/client hops');
      completed += 1;
    }
    t.diagnostic(`${profile}: PC three products/four policies, Build default, actual CLI/Discord parity`);
  }
  assert.equal(completed, 65);
  assert.deepEqual(installedSnapshot(), before, 'result projection must not replace asset generations');
});
