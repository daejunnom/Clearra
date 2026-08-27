import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

import {
  SharedExecutionAvailabilityError,
  SharedExecutionResourceAuthority,
  SharedExecutionResourceReleaseError,
  type SharedExecutionResourceRequest
} from '../src/workers/SharedExecutionResourceAuthority.ts';

const fixture = JSON.parse(await readFile(
  resolve(process.cwd(), 'tests/fixtures/contracts/execution_resource_authority.v1.json'),
  'utf8'
));
assert.equal(fixture.schema_version, 'clearra.execution-resource-authority.v1');
assert.deepEqual(
  fixture.availability_cases.map((entry: { state: string }) => entry.state),
  ['available', 'unavailable', 'deferred', 'exhausted', 'cancelled', 'incomplete']
);

const leaseCases = fixture.lease_cases;
const capacity = resource(leaseCases.capacity);
const authority = new SharedExecutionResourceAuthority(capacity, {
  authorityId: 71n,
  initialEpoch: 1n
});
const primary = authority.tryAcquire(
  leaseCases.primary.owner,
  resource(leaseCases.primary.request)
);
assert.equal(primary.token.authorityId, 71n);
assert.equal(primary.token.epoch, BigInt(leaseCases.primary.epoch));
assert.deepEqual(
  authority.snapshot().available,
  resource(leaseCases.primary.available_after_acquire)
);

const contender = captureAvailability(() => authority.tryAcquire(
  leaseCases.contender.owner,
  resource(leaseCases.contender.request)
));
assert.equal(contender.availability.state, leaseCases.contender.state);
assert.equal(contender.availability.reason, leaseCases.contender.reason);
assert.deepEqual(contender.available, authority.snapshot().available);

for (const key of ['compute_oversized', 'memory_oversized'] as const) {
  const fixtureCase = leaseCases[key];
  const error = captureAvailability(() => authority.tryAcquire(
    fixtureCase.owner,
    resource(fixtureCase.request)
  ));
  assert.equal(error.availability.state, fixtureCase.state);
  assert.equal(error.availability.reason, fixtureCase.reason);
}

assert.throws(
  () => primary.releaseAs('wrong-owner'),
  releaseError('owner-mismatch')
);
assert.deepEqual(
  authority.snapshot().available,
  resource(leaseCases.primary.available_after_acquire),
  'owner mismatch must not alter accounting'
);
primary.release();
assert.deepEqual(authority.snapshot().available, capacity);
assert.throws(() => primary.release(), releaseError('already-released'));
assert.throws(
  () => authority.release(primary.token, leaseCases.primary.owner),
  releaseError('stale-epoch')
);

const foreignAuthority = new SharedExecutionResourceAuthority(capacity, {
  authorityId: 72n
});
const foreignLease = foreignAuthority.tryAcquire('foreign-owner', request(1, 1n));
assert.throws(
  () => authority.release(foreignLease.token, 'foreign-owner'),
  releaseError('authority-mismatch')
);
foreignLease.release();

const childAuthority = new SharedExecutionResourceAuthority(capacity);
const childFixture = leaseCases.parent_child;
const parent = childAuthority.tryAcquire(
  childFixture.parent.owner,
  resource(childFixture.parent.request)
);
const children = childFixture.children.map((entry: {
  owner: string;
  request: { compute_units: number; memory_bytes: string };
}) => parent.tryChild(entry.owner, resource(entry.request)));
assert.deepEqual(childAuthority.snapshot().used, capacity);
assert.equal(childAuthority.snapshot().activeLeaseCount, 3);
assert.throws(() => parent.release(), releaseError('children-active'));
const noParentRemainder = captureAvailability(() => parent.tryChild(
  'third-child',
  request(1, 1n)
));
assert.equal(noParentRemainder.availability.state, 'deferred');
for (const child of children) child.release();
parent.release();
assert.deepEqual(childAuthority.snapshot().available, capacity);

const waitAuthority = new SharedExecutionResourceAuthority(capacity);
const firstOwner = waitAuthority.tryAcquire('first-owner', capacity);
const waiting = waitAuthority.acquireBounded('waiting-owner', capacity, { timeoutMs: 100 });
setTimeout(() => firstOwner.release(), 0);
const secondOwner = await waiting;
assert.equal(secondOwner.token.ownerId, 'waiting-owner');
secondOwner.release();

const cancelledOwner = waitAuthority.tryAcquire('cancel-blocker', capacity);
const cancellation = new AbortController();
const cancelled = waitAuthority.acquireBounded('cancelled-waiter', capacity, {
  timeoutMs: 100,
  signal: cancellation.signal
});
cancellation.abort();
await assert.rejects(cancelled, availabilityState('cancelled', 'cancelled-by-caller'));
assert.deepEqual(waitAuthority.snapshot().used, capacity);
cancelledOwner.release();

const timeoutOwner = waitAuthority.tryAcquire('timeout-blocker', capacity);
await assert.rejects(
  waitAuthority.acquireBounded('timed-out-waiter', capacity, { timeoutMs: 1 }),
  availabilityState('deferred', 'shared-resource-contention')
);
assert.deepEqual(waitAuthority.snapshot().used, capacity);
timeoutOwner.release();

await assert.rejects(
  waitAuthority.withLease(
    'scoped-owner',
    capacity,
    { timeoutMs: 100 },
    async () => {
      throw new Error('synthetic scoped failure');
    }
  ),
  /synthetic scoped failure/u
);
assert.deepEqual(
  waitAuthority.snapshot().available,
  capacity,
  'scoped failure must release the lease exactly once'
);

const overflowAuthority = new SharedExecutionResourceAuthority(capacity, {
  initialEpoch: 1n << 64n
});
const overflow = captureAvailability(() => overflowAuthority.tryAcquire(
  'overflow-owner',
  capacity
));
assert.equal(overflow.availability.state, 'unavailable');
assert.equal(overflow.availability.reason, 'capability-unavailable');
assert.deepEqual(overflowAuthority.snapshot().available, capacity);
assert.throws(
  () => new SharedExecutionResourceAuthority({
    computeUnits: 0x1_0000_0000,
    memoryBytes: 1n
  }),
  /positive u32/u
);
assert.throws(
  () => new SharedExecutionResourceAuthority({
    computeUnits: 1,
    memoryBytes: 1n << 64n
  }),
  /nonnegative u64/u
);
await assert.rejects(
  waitAuthority.acquireBounded('timer-overflow', capacity, {
    timeoutMs: 0x8000_0000
  }),
  /bounded host timer/u
);

function resource(value: {
  compute_units: number;
  memory_bytes: string;
}): SharedExecutionResourceRequest {
  return request(value.compute_units, BigInt(value.memory_bytes));
}

function request(
  computeUnits: number,
  memoryBytes: bigint
): SharedExecutionResourceRequest {
  return { computeUnits, memoryBytes };
}

function captureAvailability(operation: () => unknown): SharedExecutionAvailabilityError {
  try {
    operation();
  } catch (error) {
    assert.ok(error instanceof SharedExecutionAvailabilityError);
    return error;
  }
  throw new Error('expected shared execution availability error');
}

function availabilityState(state: string, reason: string) {
  return (error: unknown) => error instanceof SharedExecutionAvailabilityError &&
    error.availability.state === state &&
    error.availability.reason === reason;
}

function releaseError(code: string) {
  return (error: unknown) => error instanceof SharedExecutionResourceReleaseError &&
    error.code === code;
}
