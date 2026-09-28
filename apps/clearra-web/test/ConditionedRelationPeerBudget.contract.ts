import assert from 'node:assert/strict';
import { relationPeerReservation } from '../src/workers/ConditionedRelationPeerBudget.ts';

const mib = 1024 * 1024;
for (const workers of [1, 2, 7, 11, 16]) {
  const reservation = relationPeerReservation(80 * mib, workers);
  assert.ok(reservation !== null);
  assert.ok(reservation >= mib && reservation <= 2 * mib);
  assert.ok(reservation * workers <= 16 * mib);
  assert.ok(80 * mib + reservation * workers + 4 * mib <= 128 * mib);
}
assert.equal(relationPeerReservation(80 * mib, 17), null, 'disable optional peer caches instead of reducing 17 requested workers');
assert.equal(relationPeerReservation(123 * mib, 2), null);
assert.equal(relationPeerReservation(123 * mib, 1), mib);
for (const workers of [0, -1, 1.5, NaN, Infinity]) assert.equal(relationPeerReservation(80 * mib, workers), null);
for (const bytes of [-1, 0.5, NaN, Infinity, 128 * mib]) assert.equal(relationPeerReservation(bytes, 11), null);
