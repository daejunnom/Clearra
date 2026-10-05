// SRP rationale: Discord supplies runtime policy but does not reinterpret or
// deduplicate the CLI-owned legal-board/conditioned-reachability selectors.
import assert from 'node:assert/strict';
import test from 'node:test';
import { prepareClearraArguments } from '../src/clearra/command.mjs';

const commands = [
  ['pc', '--lines', '4'],
  ['pc', 'minimals', '--lines', '2', '--queue', 'IIOOO', '--no-hold'],
  ['pc', 'score-minimals', '--lines', '2', '--queue', 'IIOOO', '--no-hold'],
  ['setup', 'pc', '--remaining', 'IOTSZJL'],
  ['build-probability', '--base-mask', '0', '--target-mask', '15', '--height', '4', '--queue', 'I', '--no-hold'],
];
const selectors = new Set([
  '--legal-board', '--no-legal-board',
  '--conditioned-reachability', '--no-conditioned-reachability',
]);
const runtime = { workers: 3, logicalProcessors: 4, outputFormat: 'json' };

test('Discord forwards each independent exact accelerator selection to CLI unchanged', () => {
  for (const command of commands) {
    for (const legal of ['--legal-board', '--no-legal-board']) {
      for (const conditioned of ['--conditioned-reachability', '--no-conditioned-reachability']) {
        const flags = [legal, conditioned];
        const prepared = prepareClearraArguments([...command, ...flags], runtime);
        assert.deepEqual(prepared.filter(token => selectors.has(token)), flags);
        assert.deepEqual(prepareClearraArguments(prepared, runtime), prepared);
      }
    }
  }
});

test('Discord does not hide ambiguous selections from the authoritative CLI parser', () => {
  for (const command of commands) {
    for (const flags of [
      ['--legal-board', '--no-legal-board'],
      ['--conditioned-reachability', '--no-conditioned-reachability'],
      ['--legal-board', '--legal-board'],
      ['--no-conditioned-reachability', '--no-conditioned-reachability'],
    ]) {
      const prepared = prepareClearraArguments([...command, ...flags], runtime);
      assert.deepEqual(prepared.filter(token => selectors.has(token)), flags);
    }
  }
});

test('the curated Discord Build v2 registry remains closed to explicit accelerator overrides', () => {
  const command = [
    'build', 'cover', '--base-mask', '0', '--target-mask', '15', '--height', '4',
    '--queue', 'I', '--no-hold', '--queue-knowledge', 'oracle',
    '--objective', 'min-cover', '--rule', 'srs-plus', '--backend', 'cpu',
    '--no-backend-fallback',
  ];
  assert.doesNotThrow(() => prepareClearraArguments(command, runtime));
  for (const flag of selectors) {
    assert.throws(() => prepareClearraArguments([...command, flag], runtime),
      /does not expose the Build v2 option/u);
  }
});
