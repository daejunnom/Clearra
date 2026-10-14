// Test-only observation contract for complete Recovery geometry galleries.
// Keep fixture geometry independent of the production key builder and solver.
import assert from 'node:assert/strict';

function maskText(mask) {
  assert.ok(typeof mask === 'bigint' && mask >= 0n && mask < (1n << 256n),
    'expected Recovery geometry must fit the four-word field');
  return mask.toString(16).padStart(64, '0');
}

export function assertRecoveryGallery(records, status, expectedGeometry) {
  assert.ok(Array.isArray(expectedGeometry) && expectedGeometry.length > 0,
    'the required complete gallery fixture cannot be empty');
  const expected = expectedGeometry.map(({ middleMask, resultMask }) =>
    `${maskText(middleMask)}|${maskText(resultMask)}`);
  assert.equal(new Set(expected).size, expected.length, 'fixture geometries must be distinct');
  assert.equal(records.length, expected.length, 'the complete gallery must preserve every geometry');
  assert.equal(new Set(records.map(record => record.key)).size, records.length,
    'the complete gallery cannot repeat a solution key');
  const actual = records.map(record => {
    assert.equal(record.status, status, 'unexpected Recovery path kind');
    assert.equal(typeof record.key, 'string', 'Recovery gallery keys must be explicit');
    const key = /^recovery-tiling\.v1:[0-9]+\|m0:([0-9a-f]{64})\|r3:([0-9a-f]{64})$/u.exec(record.key);
    assert.ok(key, 'Recovery gallery key must retain both complete four-word fields');
    // The ordinal labels a catalog row; compare geometry, not an incidental
    // ordinal or the location of the first rendered card.
    return `${key[1]}|${key[2]}`;
  });
  assert.equal(new Set(actual).size, actual.length, 'different ordinals cannot duplicate geometry');
  assert.deepEqual(actual.sort(), expected.sort(), 'the complete Recovery geometry set differs');
}
