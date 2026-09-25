import assert from 'node:assert/strict';
import test from 'node:test';
import { emptyMandatorySelection, pcMandatorySource, buildMandatorySource, toggleMandatorySolution } from '../src/lib/workspace/mandatorySolutionSelection.ts';
const hash = 'cts1:0123456789abcdef';
const key = 'ctk1|initial=000000000000003f|placements=I:00000000000003c0';
const encode = keys => 'ctk3_' + keys.length;
test('mandatory selection toggles canonical drawings and seals their full source identity', () => {
  const selected = toggleMandatorySolution(emptyMandatorySelection(), key, hash, encode);
  assert.equal(selected.pinnedSourceSetHash, hash);
  assert.equal(selected.pinnedSolutionDocument, 'ctk3_1');
  assert.deepEqual(toggleMandatorySolution(selected, key, hash, encode), emptyMandatorySelection());
  assert.throws(() => toggleMandatorySolution(selected, key, 'cts1:aaaaaaaaaaaaaaaa', encode));
});
test('encoding failures leave the previous selection unchanged', () => {
  const initial = emptyMandatorySelection();
  assert.throws(() => toggleMandatorySolution(initial, key, hash, () => { throw new Error('overflow'); }));
  assert.deepEqual(initial, emptyMandatorySelection());
});
test('solver objective and worker changes retain pins, but input changes invalidate the source', () => {
  const pc = { lines: 4, boardMask: 0n, queue: 'STOILJZ', holdEnabled: true, rule: 'srs-plus' };
  assert.equal(pcMandatorySource(pc), pcMandatorySource({ ...pc, scoreMode: 'minimum-cover', workers: 8 }));
  for (const change of [{ queue: 'I' }, { boardMask: 1n }, { holdPiece: 'T' }, { rule: 'srs' }, { lines: 2 }]) {
    assert.notEqual(pcMandatorySource(pc), pcMandatorySource({ ...pc, ...change }));
  }
  const build = { height: 4, existingMask: 0n, targetMask: 15n, queue: 'I', sourcePieces: null };
  assert.equal(buildMandatorySource(build), buildMandatorySource({ ...build, resultMode: 'minimum-solutions', workers: 8 }));
  assert.notEqual(buildMandatorySource(build), buildMandatorySource({ ...build, targetMask: 30n }));
});
