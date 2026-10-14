import assert from 'node:assert/strict';

// Shared test bookkeeping, not search/qualification authority. Compare named
// semantic cases rather than a magic total that changes when a case is added.
// The fast fixture contract independently defines the required product axes;
// real consumers record a row only after their actual execution assertions.
export function assertFixtureCoverage(observed, expected, label) {
  assert.ok(expected.length > 0, `${label}: an empty required matrix is not coverage`);
  const keys = rows => rows.map(row => {
    assert.ok(Array.isArray(row) && row.length > 0, `${label}: a named case is required`);
    return JSON.stringify(row);
  });
  const actualKeys = keys(observed);
  const expectedKeys = keys(expected);
  assert.equal(new Set(expectedKeys).size, expectedKeys.length,
    `${label}: the required matrix contains duplicate cases`);
  assert.equal(new Set(actualKeys).size, actualKeys.length,
    `${label}: a case was repeated instead of executing a missing case`);
  assert.deepEqual(actualKeys.sort(), expectedKeys.sort(),
    `${label}: missing, extra or mismatched semantic cases`);
}
