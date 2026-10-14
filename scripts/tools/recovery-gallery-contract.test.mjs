import assert from 'node:assert/strict';
import test from 'node:test';
import { assertRecoveryGallery } from './recovery-gallery-contract.mjs';

const expected = [0xfn, 0x3c0n].map(middleMask => ({ middleMask, resultMask: 0xc030n }));
function record(middle, ordinal = 0, result = 0xc030n) {
  return { status: 'recovery',
    key: `recovery-tiling.v1:${ordinal}|m0:${middle.toString(16).padStart(64, '0')}|r3:${result.toString(16).padStart(64, '0')}` };
}

test('complete mirrored Recovery gallery keeps both physical middle fields', () => {
  assertRecoveryGallery([record(0xfn), record(0x3c0n, 1)], 'recovery', expected);
  // Reordering rows or changing their ordinals does not change their geometry.
  assertRecoveryGallery([record(0x3c0n, 8), record(0xfn, 7)], 'recovery', expected);
});

test('missing, duplicate and foreign Recovery geometries are not accepted as a complete gallery', () => {
  for (const records of [
    [record(0xfn)],
    [record(0xfn), record(0xfn)],
    [record(0xfn), record(0xfn, 1)],
    [record(0xfn), record(0x78n, 1)],
    [record(0xfn), record(0x3c0n, 1, 0xc031n)],
  ]) assert.throws(() => assertRecoveryGallery(records, 'recovery', expected));
});

test('Recovery path kind and complete field encoding remain mandatory', () => {
  const first = record(0xfn);
  for (const invalid of [
    { ...first, status: 'normal' },
    { ...first, key: undefined },
    { ...first, key: first.key.replace('recovery-tiling.v1:', 'other:') },
    { ...first, key: first.key.replace('m0:0000000000000000', 'm0:') },
  ]) assert.throws(() => assertRecoveryGallery([invalid, record(0x3c0n, 1)], 'recovery', expected));
});

test('empty or duplicate expected geometry cannot weaken the observation contract', () => {
  assert.throws(() => assertRecoveryGallery([], 'recovery', []));
  assert.throws(() => assertRecoveryGallery([record(0xfn), record(0xfn, 1)], 'recovery', [expected[0], expected[0]]));
});
