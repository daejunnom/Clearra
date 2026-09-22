import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { generateIndex, generationKey } from './kick-index.mjs';

const contract = { engine: 'fixture-exhaustive-movement-v1', width: 10, height: 4,
  spawn: 'spawn-v1', lock: 'lock-v1', lineClear: 'clear-v1', initialDomain: 'explicit-enumerated-states',
  pieceDomain: ['I', 'O', 'T', 'S', 'Z', 'J', 'L'], kicks: { 'T:0>2': [[0, 0], [1, 0]] } };
const states = ['a', 'b', 'dead', 'goal'];
const goals = ['goal'];
const transitions = async (state, rules) => ({ complete: true, next:
  state === 'a' ? ['b'] : state === 'b' && rules.kicks['T:0>2'].length > 1 ? ['goal'] : [] });
const create = (extra = {}) => generateIndex({ contract, states, goals, transitions, ...extra });

test('kick-specific regeneration changes an unreachable state without cross-rule cache reuse', async () => {
  const noExtraKick = { ...contract, kicks: { 'T:0>2': [[0, 0]] } };
  const before = await create({ contract: noExtraKick });
  const after = await create();
  assert.equal(before.classify('a', noExtraKick), 'impossible');
  assert.equal(before.classify('a', contract), 'unknown');
  assert.equal(after.classify('a', contract), 'unknown');
  assert.equal(after.classify('dead', contract), 'impossible');
});

test('outside-domain arbitrary garbage and reachable membership never become negative proof', async () => {
  const index = await create();
  assert.equal(index.classify('arbitrary-user-garbage', contract), 'unknown');
  assert.equal(index.classify('goal', contract), 'unknown');
  for (const key of ['engine', 'spawn', 'lock', 'lineClear', 'initialDomain', 'height']) {
    assert.equal(index.classify('dead', { ...contract, [key]: `${contract[key]}-changed` }), 'unknown');
  }
});

test('partial generation, cancellation, budgets, closure errors and exceptions fail open to the exact engine', async () => {
  const controller = new AbortController(); controller.abort();
  for (const patch of [
    { maxStates: 1 }, { maxEdges: 0 }, { signal: controller.signal },
    { transitions: async () => ({ complete: false, next: [] }) },
    { transitions: async () => ({ complete: true, next: ['outside'] }) },
    { transitions: async () => { throw new Error('fixture'); } },
    { goals: [] }, { states: ['a', 'a'] },
  ]) {
    const index = await create(patch);
    assert.equal(index.complete, false);
    assert.equal(index.classify('dead', contract), 'unknown');
  }
});

test('ordered real SRS-X kick fixture invalidates on order, offset and O-policy changes', async () => {
  const fixture = JSON.parse(await readFile(new URL('../../tests/fixtures/rules/tetrio_srs_x_standard_tetromino_kicks.json',
    import.meta.url), 'utf8'));
  const actual = { ...contract, kicks: fixture };
  const baseline = generationKey(actual);
  for (const change of [
    (f) => f.families.jlstz['01'].reverse(),
    (f) => { f.families.jlstz['01'][0][0] += 1; },
    (f) => { f.standard_o.disallow_kick = false; },
    (f) => { f.implicit_origin_attempt = false; },
  ]) {
    const altered = structuredClone(fixture); change(altered);
    assert.notEqual(generationKey({ ...contract, kicks: altered }), baseline);
  }
});

test('complete reverse graph agrees with exhaustive forward reachability on every small graph', async () => {
  const small = ['0', '1', '2'];
  for (let bits = 0; bits < 512; bits += 1) {
    const next = (state) => small.filter((_, to) => bits & (1 << (Number(state) * 3 + to)));
    const index = await create({ states: small, goals: ['2'], transitions: async (state) => ({ complete: true, next: next(state) }) });
    for (const state of small) {
      const visited = new Set([state]); const queue = [state];
      for (let head = 0; head < queue.length; head += 1) for (const to of next(queue[head]))
        if (!visited.has(to)) { visited.add(to); queue.push(to); }
      assert.equal(index.classify(state, contract), visited.has('2') ? 'unknown' : 'impossible');
    }
  }
});
