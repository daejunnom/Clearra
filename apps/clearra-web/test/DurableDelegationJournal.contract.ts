import assert from 'node:assert/strict';

// SRP rationale: this contract's single change reason is crash-recoverable
// delegation authority. Its bounded IDB fake injects read/write/commit failures
// to verify the same journal's fences, recovery states and durable ACK boundary.

import {
  ACTIVE_LEASE_EXPIRY_MS,
  createBrowserDelegationAuthority,
  DurableDelegationAuthority,
  IndexedDbDelegationJournal,
  MemoryDelegationJournal,
  OFFER_ACCEPT_TIMEOUT_MS,
  PERSISTED_RENEWAL_INTERVAL_MS,
  TERMINAL_TOMBSTONE_RETENTION_MS,
  WORKER_HEARTBEAT_INTERVAL_MS,
  sha256Hex,
  type DelegationEvent,
  type DelegationJournal
} from '../src/workers/DurableDelegationJournal.ts';
import { ClearraVerifierPool } from '../src/workers/ClearraVerifierPool.ts';

async function runContract() {
assert.equal(OFFER_ACCEPT_TIMEOUT_MS, 10_000);
assert.equal(WORKER_HEARTBEAT_INTERVAL_MS, 5_000);
assert.equal(PERSISTED_RENEWAL_INTERVAL_MS, 30_000);
assert.equal(ACTIVE_LEASE_EXPIRY_MS, 120_000);
assert.equal(TERMINAL_TOMBSTONE_RETENTION_MS, 86_400_000);

let localNow = 100;
const localAuthority = await createBrowserDelegationAuthority(() => localNow, 'local-long-task');
const localToken = await localAuthority.prepare(identity('local-long-task'), budget());
await localAuthority.offered(localToken);
await localAuthority.accepted(localToken, {
  taskId: localToken.taskId,
  fencingTokenDecimal: localToken.fencingTokenDecimal,
  workerId: '1',
  reservationSha256: '33'.repeat(32)
});
const localPermit = await localAuthority.publish(localToken);
await localAuthority.running(localToken);
localNow += ACTIVE_LEASE_EXPIRY_MS * 100;
assert.ok(Number(localPermit.expiresAtUnixMsDecimal) > localNow);
assert.equal(await localAuthority.expireStale(), 0, 'local atomic work has no hidden runtime cap');
await localAuthority.resultSealed(localToken, '44'.repeat(32), '55'.repeat(32));
await localAuthority.resultAppliedAndCompleted(localToken);
assert.equal(localAuthority.phase(localToken), 'completed');
await assert.rejects(
  localAuthority.running({ ...localToken, fencingTokenDecimal: '999' }),
  /stale/
);
localAuthority.close();

const katJournal = new MemoryDelegationJournal();
const katAuthority = await DurableDelegationAuthority.recover(katJournal, () => 100);
await katAuthority.prepare(identity('kat'), budget());
assert.equal(
  (await katJournal.load())[0].eventSha256,
  '2ebdba8445ca1d4d33fd71a0e4fed883555333c4d83201b89ea51f3c6963025a'
);

const concurrentJournal = new MemoryDelegationJournal();
const concurrentAuthority = await DurableDelegationAuthority.recover(
  concurrentJournal,
  () => 100
);
await Promise.all([
  concurrentAuthority.prepare(identity('concurrent-1'), budget()),
  concurrentAuthority.prepare(identity('concurrent-2'), budget())
]);
assert.deepEqual(
  (await concurrentJournal.load()).map((event) => event.sequenceDecimal),
  ['1', '2']
);

const duplicateJournal = new MemoryDelegationJournal();
const duplicateAuthority = await DurableDelegationAuthority.recover(duplicateJournal, () => 100);
const duplicateResults = await Promise.allSettled([
  duplicateAuthority.prepare(identity('concurrent-duplicate'), budget()),
  duplicateAuthority.prepare(identity('concurrent-duplicate'), budget())
]);
assert.deepEqual(
  duplicateResults.map(({ status }) => status).sort(),
  ['fulfilled', 'rejected'],
  'concurrent duplicate identity preparation has exactly one durable winner'
);
assert.equal((await duplicateJournal.load()).length, 1);

const failedPrepareJournal = new MemoryDelegationJournal();
const failedPrepareAuthority = await DurableDelegationAuthority.recover(
  failedPrepareJournal,
  () => 100
);
failedPrepareJournal.failNextAppend();
await assert.rejects(
  failedPrepareAuthority.prepare(identity('prepare-ack-failure'), budget()),
  /durable append failed/
);
const recoveredFence = await failedPrepareAuthority.prepare(
  identity('prepare-ack-failure'),
  budget()
);
assert.equal(recoveredFence.fencingTokenDecimal, '1', 'failed prepare ACK cannot burn a fence');

let ackNow = 100;
const allAckJournal = new MemoryDelegationJournal();
const allAckAuthority = await DurableDelegationAuthority.recover(allAckJournal, () => ackNow);
const allAckToken = await allAckAuthority.prepare(identity('all-ack-points'), budget());
allAckJournal.failNextAppend();
await assert.rejects(allAckAuthority.offered(allAckToken), /durable append failed/);
assert.equal(allAckAuthority.phase(allAckToken), 'prepared');
const allAckOffer = await allAckAuthority.offered(allAckToken);
const allAckAcceptance = {
  taskId: allAckToken.taskId,
  fencingTokenDecimal: allAckToken.fencingTokenDecimal,
  workerId: '3',
  reservationSha256: '77'.repeat(32)
};
ackNow += 1;
allAckJournal.failNextAppend();
await assert.rejects(
  allAckAuthority.accepted(allAckToken, allAckAcceptance),
  /durable append failed/
);
assert.equal(allAckAuthority.phase(allAckToken), 'offered');
assert.equal(Number(allAckOffer.acceptByUnixMsDecimal) >= ackNow, true);
await allAckAuthority.accepted(allAckToken, allAckAcceptance);
allAckJournal.failNextAppend();
await assert.rejects(allAckAuthority.publish(allAckToken), /durable append failed/);
assert.equal(allAckAuthority.phase(allAckToken), 'accepted');
await allAckAuthority.publish(allAckToken);
allAckJournal.failNextAppend();
await assert.rejects(allAckAuthority.running(allAckToken), /durable append failed/);
assert.equal(allAckAuthority.phase(allAckToken), 'published');
await allAckAuthority.running(allAckToken);
ackNow += PERSISTED_RENEWAL_INTERVAL_MS;
allAckJournal.failNextAppend();
await assert.rejects(allAckAuthority.heartbeat(allAckToken), /durable append failed/);
assert.equal(allAckAuthority.phase(allAckToken), 'running');
assert.equal(await allAckAuthority.heartbeat(allAckToken), true);
allAckJournal.failNextAppend();
await assert.rejects(
  allAckAuthority.resultSealed(allAckToken, '44'.repeat(32), '55'.repeat(32)),
  /durable append failed/
);
assert.equal(allAckAuthority.phase(allAckToken), 'renewed');
await allAckAuthority.resultSealed(allAckToken, '44'.repeat(32), '55'.repeat(32));
allAckJournal.failNextAppend();
await assert.rejects(allAckAuthority.resultApplied(allAckToken), /durable append failed/);
assert.equal(allAckAuthority.phase(allAckToken), 'result-sealed');
await allAckAuthority.resultApplied(allAckToken);
allAckJournal.failNextAppend();
await assert.rejects(allAckAuthority.completed(allAckToken), /durable append failed/);
assert.equal(allAckAuthority.phase(allAckToken), 'result-applied');
await allAckAuthority.completed(allAckToken);
assert.equal(allAckAuthority.phase(allAckToken), 'completed');

const revokeJournal = new MemoryDelegationJournal();
const revokeAuthority = await DurableDelegationAuthority.recover(revokeJournal, () => 100);
const revokeToken = await revokeAuthority.prepare(identity('revoked'), budget());
revokeJournal.failNextAppend();
await assert.rejects(revokeAuthority.revoked(revokeToken, 'coordinator shutdown'));
assert.equal(revokeAuthority.phase(revokeToken), 'prepared');
await revokeAuthority.revoked(revokeToken, 'coordinator shutdown');
const recoveredRevoked = await DurableDelegationAuthority.recover(
  new MemoryDelegationJournal(await revokeJournal.load()),
  () => 100
);
assert.equal(recoveredRevoked.phase(revokeToken), 'revoked');

let now = 100;
const journal = new MemoryDelegationJournal();
const authority = await DurableDelegationAuthority.recover(journal, () => now);
const token = await authority.prepare(identity('publish-ack'), budget());
await authority.offered(token);
now += OFFER_ACCEPT_TIMEOUT_MS;
await authority.accepted(token, {
  taskId: token.taskId,
  fencingTokenDecimal: token.fencingTokenDecimal,
  workerId: '1',
  reservationSha256: '33'.repeat(32)
});

journal.failNextAppend();
await assert.rejects(authority.publish(token), /durable append failed/);
assert.equal(authority.phase(token), 'accepted');

now += 1;
const permit = await authority.publish(token);
assert.equal(permit.publicationSequenceDecimal, '4');
assert.equal(permit.fencingTokenDecimal, token.fencingTokenDecimal);
assert.equal(authority.phase(token), 'published');
await authority.running(token);

now += PERSISTED_RENEWAL_INTERVAL_MS - 1;
assert.equal(await authority.heartbeat(token), false);
now += 1;
assert.equal(await authority.heartbeat(token), true);
assert.equal(authority.phase(token), 'renewed');
await authority.resultSealed(token, '44'.repeat(32), '55'.repeat(32));
await authority.resultAppliedAndCompleted(token);
assert.equal(authority.phase(token), 'completed');
assert.equal(authority.retainedTerminalCount(now), 1);
assert.equal(authority.retainedTerminalCount(now + TERMINAL_TOMBSTONE_RETENTION_MS), 1);
assert.equal(authority.retainedTerminalCount(now + TERMINAL_TOMBSTONE_RETENTION_MS + 1), 0);
assert.equal((await journal.load()).length, 9, 'expired tombstones remain in the append-only journal');

const recovered = await DurableDelegationAuthority.recover(
  new MemoryDelegationJournal(await journal.load()),
  () => now
);
assert.equal(recovered.phase(token), 'completed');

let compactionNow = 1_000;
const compactionJournal = new MemoryDelegationJournal();
const compactionAuthority = await DurableDelegationAuthority.recover(
  compactionJournal,
  () => compactionNow
);
const compactionToken = await compactionAuthority.prepare(identity('compaction'), budget());
await compactionAuthority.cancelled(compactionToken);
compactionNow += TERMINAL_TOMBSTONE_RETENTION_MS;
assert.equal(
  await compactionAuthority.compactExpiredTerminalTombstones(),
  false,
  'the exact 24-hour boundary remains retained'
);
compactionNow += 1;
compactionJournal.failNextReset();
await assert.rejects(
  compactionAuthority.compactExpiredTerminalTombstones(),
  /durable reset failed/
);
assert.equal(compactionAuthority.phase(compactionToken), 'cancelled');
assert.equal((await compactionJournal.load()).length, 2);
assert.equal(await compactionAuthority.compactExpiredTerminalTombstones(), true);
assert.equal((await compactionJournal.load()).length, 0);
assert.throws(() => compactionAuthority.phase(compactionToken), /identity is unknown/);
const reusedToken = await compactionAuthority.prepare(identity('compaction'), budget());
assert.equal(reusedToken.fencingTokenDecimal, '1');

const mixedJournal = new MemoryDelegationJournal();
const mixedAuthority = await DurableDelegationAuthority.recover(mixedJournal, () => 1_000);
const terminalToken = await mixedAuthority.prepare(identity('mixed-terminal'), budget());
await mixedAuthority.cancelled(terminalToken);
await mixedAuthority.prepare(identity('mixed-live'), budget());
assert.equal(
  await mixedAuthority.compactExpiredTerminalTombstones(Number.MAX_SAFE_INTEGER),
  false,
  'a live or unresolved state prevents every tombstone reset'
);
await assert.rejects(mixedJournal.resetIfHead('ff'.repeat(32)), /head changed/);
assert.equal((await mixedJournal.load()).length, 3);

const tampered = (await journal.load()).map((event) => ({ ...event }));
tampered[1] = { ...tampered[1], timestampUnixMsDecimal: '999999' };
const corruptJournal = new MemoryDelegationJournal(tampered as DelegationEvent[]);
await assert.rejects(
  DurableDelegationAuthority.recover(
    corruptJournal,
    () => now
  ),
  /digest mismatch/
);
await assert.rejects(corruptJournal.load(), /is quarantined/);

const semanticSource = new MemoryDelegationJournal();
const semanticAuthority = await DurableDelegationAuthority.recover(semanticSource, () => 100);
await semanticAuthority.prepare(identity('semantic-corruption'), budget());
const semanticEvent = { ...(await semanticSource.load())[0] };
semanticEvent.workerId = '9';
semanticEvent.reservationSha256 = '66'.repeat(32);
semanticEvent.eventSha256 = await sha256Hex(canonicalMaterial(semanticEvent));
const semanticJournal = new MemoryDelegationJournal([semanticEvent]);
await assert.rejects(
  DurableDelegationAuthority.recover(semanticJournal, () => 100),
  /worker reservation is forbidden before acceptance/
);
await assert.rejects(semanticJournal.load(), /is quarantined/);

const expiryJournal = new MemoryDelegationJournal();
const expiryAuthority = await DurableDelegationAuthority.recover(expiryJournal, () => now);
const expiryToken = await expiryAuthority.prepare(identity('expiry'), budget());
await expiryAuthority.offered(expiryToken);
await expiryAuthority.accepted(expiryToken, {
  taskId: expiryToken.taskId,
  fencingTokenDecimal: expiryToken.fencingTokenDecimal,
  workerId: '2',
  reservationSha256: '44'.repeat(32)
});
await expiryAuthority.publish(expiryToken);
await expiryAuthority.running(expiryToken);
now += ACTIVE_LEASE_EXPIRY_MS;
assert.equal(await expiryAuthority.expireStale(), 0);
now += 1;
assert.equal(await expiryAuthority.expireStale(), 1);
assert.equal(expiryAuthority.phase(expiryToken), 'expired');

const fakeFactory = new FakeIdbFactory();
const indexedJournal = await IndexedDbDelegationJournal.open(
  fakeFactory as unknown as IDBFactory,
  'clearra-runtime-v1-contract'
);
const indexedAuthority = await DurableDelegationAuthority.recover(indexedJournal, () => 1_000);
const transactionsBeforeAppend = fakeFactory.database.transactionModes.length;
fakeFactory.database.holdWriteCompletion = true;
let durableAcked = false;
const pendingPrepare = indexedAuthority
  .prepare(identity('indexeddb-ack'), budget())
  .then((value) => {
    durableAcked = true;
    return value;
  });
await fakeFactory.database.waitForHeldWrite();
assert.equal(durableAcked, false, 'append must not ACK before transaction.oncomplete');
fakeFactory.database.releaseHeldWrite();
await pendingPrepare;
assert.equal(durableAcked, true);
assert.deepEqual(
  fakeFactory.database.transactionModes.slice(transactionsBeforeAppend),
  ['readwrite'],
  'one transition must fence quarantine and head without an extra readonly storage round trip'
);
indexedJournal.close();

async function prepareSealed(journal: DelegationJournal, task: string) {
  const authority = await DurableDelegationAuthority.recover(journal, () => 1_000);
  const token = await authority.prepare(identity(task), budget());
  await authority.offered(token);
  await authority.accepted(token, {
    taskId: token.taskId, fencingTokenDecimal: token.fencingTokenDecimal,
    workerId: '1', reservationSha256: '33'.repeat(32)
  });
  await authority.publish(token);
  await authority.running(token);
  await authority.resultSealed(token, '44'.repeat(32), '55'.repeat(32));
  return { authority, token };
}

{
  const pairFactory = new FakeIdbFactory();
  const pairJournal = await IndexedDbDelegationJournal.open(pairFactory as unknown as IDBFactory, 'atomic-terminal-pair');
  const { authority, token } = await prepareSealed(pairJournal, 'atomic-terminal-pair');
  const before = await pairJournal.load();
  const transactionStart = pairFactory.database.transactionModes.length;
  pairFactory.database.holdWriteCompletion = true;
  let acknowledged = false;
  const terminal = authority.resultAppliedAndCompleted(token).then(() => { acknowledged = true; });
  await pairFactory.database.waitForHeldWrite();
  assert.equal(acknowledged, false);
  assert.equal(authority.phase(token), 'result-sealed', 'the pair has no early in-memory terminal ACK');
  assert.equal(authority.resultApplicationDecision(token, '44'.repeat(32)), 'apply-once');
  assert.equal(pairFactory.database.stores.get('delegation-events-v1')!.size, before.length);
  pairFactory.database.releaseHeldWrite();
  await terminal;
  assert.equal(acknowledged, true);
  assert.equal(authority.phase(token), 'completed');
  assert.equal(authority.resultApplicationDecision(token, '44'.repeat(32)), 'already-applied');
  assert.deepEqual(pairFactory.database.transactionModes.slice(transactionStart), ['readwrite']);
  const pairEvents = await pairJournal.load();
  assert.deepEqual(pairEvents.slice(-2).map((event) => event.phase), ['result-applied', 'completed']);
  assert.equal(pairEvents.at(-1)!.previousEventSha256, pairEvents.at(-2)!.eventSha256);
  const legacyMemory = new MemoryDelegationJournal();
  const legacy: DelegationJournal = {
    load: () => legacyMemory.load(), append: (draft) => legacyMemory.append(draft),
    resetIfHead: (head) => legacyMemory.resetIfHead(head),
    quarantine: (reason) => legacyMemory.quarantine(reason), close: () => legacyMemory.close()
  };
  const reference = await prepareSealed(legacy, 'atomic-terminal-pair');
  await reference.authority.resultAppliedAndCompleted(reference.token);
  assert.deepEqual(pairEvents, await legacy.load(), 'atomic commit preserves every legacy event byte/hash/sequence');
  const afterTerminal = pairFactory.database.transactionModes.length;
  await Promise.all([authority.resultAppliedAndCompleted(token), authority.resultAppliedAndCompleted(token)]);
  assert.equal(pairFactory.database.transactionModes.length, afterTerminal, 'completed receipt retry writes nothing');
  await assert.rejects(authority.resultAppliedAndCompleted({ ...token, fencingTokenDecimal: '999' }), /stale/);
  await assert.rejects(authority.resultSealed(token, 'aa'.repeat(32), '55'.repeat(32)), /does not match/);
  const recovered = await DurableDelegationAuthority.recover(pairJournal, () => 1_000);
  assert.equal(recovered.phase(token), 'completed');
  assert.equal(recovered.resultApplicationDecision(token, '44'.repeat(32)), 'already-applied');
  pairJournal.close();
}

for (const failureWrite of [1, 2, 3]) {
  const failureFactory = new FakeIdbFactory();
  const failureJournal = await IndexedDbDelegationJournal.open(failureFactory as unknown as IDBFactory, `terminal-write-${failureWrite}`);
  const { authority, token } = await prepareSealed(failureJournal, `terminal-write-${failureWrite}`);
  const before = await failureJournal.load();
  const head = structuredClone(failureFactory.database.stores.get('delegation-meta-v1')!.get('head'));
  failureFactory.database.failWriteAt = failureWrite;
  await assert.rejects(authority.resultAppliedAndCompleted(token), (error: unknown) => {
    assert.ok(error instanceof Error && error.cause instanceof Error);
    assert.match(error.cause.message, /synthetic atomic write failure/);
    return true;
  });
  assert.equal(authority.phase(token), 'result-sealed', 'failed pair never creates an applied/completed state');
  assert.deepEqual(await failureJournal.load(), before, 'first/second event or head failure commits neither event');
  assert.deepEqual(failureFactory.database.stores.get('delegation-meta-v1')!.get('head'), head);
  await assert.rejects(DurableDelegationAuthority.recover(new MemoryDelegationJournal(before), () => 1_000),
    /unresolved.*result-sealed/, 'restart still fails closed for an unresolved sealed result');
  await authority.resultAppliedAndCompleted(token);
  assert.equal((await failureJournal.load()).length, before.length + 2, 'retry adds one pair without phantom rows');
  failureJournal.close();
}

for (const failureMode of ['quarantine', 'head', 'cancelled'] as const) {
  const factory = new FakeIdbFactory();
  const journal = await IndexedDbDelegationJournal.open(factory as unknown as IDBFactory, `terminal-fence-${failureMode}`);
  const { authority, token } = await prepareSealed(journal, `terminal-fence-${failureMode}`);
  if (failureMode === 'cancelled') await authority.cancelled(token);
  const eventCount = factory.database.stores.get('delegation-events-v1')!.size;
  const meta = factory.database.stores.get('delegation-meta-v1')!;
  if (failureMode === 'quarantine') meta.set('quarantine', { key: 'quarantine', reason: 'external fence' });
  if (failureMode === 'head') meta.set('head', { ...(meta.get('head') as object), eventSha256: 'aa'.repeat(32) });
  await assert.rejects(authority.resultAppliedAndCompleted(token), /quarantined|head changed|cannot complete/);
  assert.equal(factory.database.stores.get('delegation-events-v1')!.size, eventCount);
  assert.equal(authority.phase(token), failureMode === 'cancelled' ? 'cancelled' : 'result-sealed');
  journal.close();
}

for (const failure of ['append-head-read', 'append-quarantine-read', 'append-early-abort', 'reset-head-read', 'reset-early-abort']) {
  const factory = new FakeIdbFactory();
  const journal = await IndexedDbDelegationJournal.open(factory as unknown as IDBFactory, failure);
  const { authority, token } = await prepareSealed(journal, failure);
  const resetting = failure.startsWith('reset-');
  if (resetting) await authority.resultAppliedAndCompleted(token);
  const before = await journal.load();
  const head = structuredClone(factory.database.stores.get('delegation-meta-v1')!.get('head'));
  factory.database.failReadKey = failure.endsWith('head-read') ? 'head'
    : failure.endsWith('quarantine-read') ? 'quarantine' : null;
  factory.database.abortWriteBeforeReads = failure.endsWith('early-abort');
  const unhandled: unknown[] = [];
  const onUnhandled = (error: unknown) => { unhandled.push(error); };
  const processEvents = process as typeof process & {
    on(event: 'unhandledRejection', listener: (error: unknown) => void): void;
    off(event: 'unhandledRejection', listener: (error: unknown) => void): void;
  };
  processEvents.on('unhandledRejection', onUnhandled);
  try {
    await assert.rejects(resetting ? journal.resetIfHead(before.at(-1)!.eventSha256)
      : authority.resultAppliedAndCompleted(token), /read delegation|transaction aborted/);
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    assert.deepEqual(unhandled, [], 'metadata request failure and early abort share one handled transaction rejection');
  } finally { processEvents.off('unhandledRejection', onUnhandled); }
  assert.deepEqual(await journal.load(), before, 'read/early-abort failure cannot mutate committed history or cache');
  assert.deepEqual(factory.database.stores.get('delegation-meta-v1')!.get('head'), head);
  assert.equal(authority.phase(token), resetting ? 'completed' : 'result-sealed');
  journal.close();
}

for (const failure of ['quarantine-read', 'quarantine-early-abort', 'load-head-read', 'load-events-read',
  'load-early-abort', 'header-read', 'header-early-abort', 'header-write', 'quarantine-write']) {
  const factory = new FakeIdbFactory();
  const journal = await IndexedDbDelegationJournal.open(factory as unknown as IDBFactory, `owner-${failure}`);
  const before = structuredClone(factory.database.stores);
  const header = failure.startsWith('header-');
  const loadingEvents = failure.startsWith('load-');
  if (failure.endsWith('-read')) {
    factory.database.failReadMode = header ? 'readwrite' : 'readonly';
    factory.database.failReadStore = loadingEvents ? 'delegation-events-v1' : null;
    factory.database.failReadKey = header ? 'journal-header'
      : failure === 'load-events-read' ? '*getAll' : loadingEvents ? 'head' : 'quarantine';
  } else if (failure.endsWith('early-abort')) {
    factory.database.abortWriteBeforeReads = header;
    factory.database.abortReadBeforeReads = !header;
    factory.database.abortReadStore = loadingEvents ? 'delegation-events-v1' : null;
  } else factory.database.failWriteAt = 1;
  const unhandled: unknown[] = [];
  const listener = (error: unknown) => { unhandled.push(error); };
  const processEvents = process as typeof process & {
    on(event: 'unhandledRejection', listener: (error: unknown) => void): void;
    off(event: 'unhandledRejection', listener: (error: unknown) => void): void;
  };
  processEvents.on('unhandledRejection', listener);
  try {
    const operation = header ? IndexedDbDelegationJournal.openForJob(factory as unknown as IDBFactory,
      `owner-${failure}`, '018f0f25-6f8a-7c1d-9b20-8b85c4d9e001')
      : failure === 'quarantine-write' ? journal.quarantine('must not become durable') : journal.load();
    await assert.rejects(operation, /read delegation|load delegation|transaction aborted|synthetic atomic write failure/);
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    assert.deepEqual(unhandled, [], `${failure} must have exactly one handled failure owner`);
  } finally { processEvents.off('unhandledRejection', listener); }
  assert.deepEqual(factory.database.stores, before, 'failed reads/header/quarantine cannot commit partial metadata');
  assert.equal(factory.database.closeCount, header ? 1 : 0, 'a failed header does not leak its unopened journal handle');
  assert.deepEqual(await journal.load(), [], 'a failed read or quarantine write does not cache false authority');
  journal.close();
}

{
  const factory = new FakeIdbFactory();
  const journal = await IndexedDbDelegationJournal.open(factory as unknown as IDBFactory, 'legacy-result-applied');
  const { authority, token } = await prepareSealed(journal, 'legacy-result-applied');
  await authority.resultApplied(token);
  const before = await journal.load();
  await assert.rejects(DurableDelegationAuthority.recover(new MemoryDelegationJournal(before), () => 1_000),
    /unresolved.*result-applied/, 'legacy partial terminal history does not gain restart authority');
  const start = factory.database.transactionModes.length;
  await authority.resultAppliedAndCompleted(token);
  assert.deepEqual(factory.database.transactionModes.slice(start), ['readwrite']);
  assert.equal((await journal.load()).length, before.length + 1, 'legacy applied state appends only completed');
  journal.close();
}

const quarantinedWriterFactory = new FakeIdbFactory();
const quarantinedWriterJournal = await IndexedDbDelegationJournal.open(
  quarantinedWriterFactory as unknown as IDBFactory,
  'clearra-runtime-v1-concurrent-quarantine'
);
const quarantinedWriterAuthority = await DurableDelegationAuthority.recover(
  quarantinedWriterJournal, () => 1_000
);
await quarantinedWriterAuthority.prepare(identity('before-quarantine'), budget());
const eventCountBeforeQuarantine = quarantinedWriterFactory.database.stores
  .get('delegation-events-v1')!.size;
quarantinedWriterFactory.database.stores.get('delegation-meta-v1')!.set('quarantine', {
  key: 'quarantine', reason: 'other owner quarantined storage', timestampUnixMsDecimal: '1000'
});
await assert.rejects(
  quarantinedWriterAuthority.prepare(identity('after-quarantine'), budget()),
  /is quarantined/
);
assert.equal(
  quarantinedWriterFactory.database.stores.get('delegation-events-v1')!.size,
  eventCountBeforeQuarantine,
  'a cached journal head must not bypass an externally committed quarantine marker'
);
quarantinedWriterJournal.close();

const headerFactory = new FakeIdbFactory();
const durableJobId = '018f0f25-6f8a-7c1d-9b20-8b85c4d9e001';
const headerJournal = await IndexedDbDelegationJournal.openForJob(
  headerFactory as unknown as IDBFactory,
  'clearra-runtime-v1-header-contract',
  durableJobId
);
assert.deepEqual(headerFactory.database.stores.get('delegation-meta-v1')!.get('journal-header'), {
  key: 'journal-header',
  schema: 'clearra.delegation-journal-header.v1',
  jobId: durableJobId
});
const headerAuthority = await DurableDelegationAuthority.recover(headerJournal, () => 1_250);
await headerAuthority.prepare(
  { ...identity('indexeddb-header'), jobId: durableJobId },
  budget()
);
headerJournal.close();

const unknownCommitFactory = new FakeIdbFactory();
const unknownCommitWriter = await IndexedDbDelegationJournal.open(
  unknownCommitFactory as unknown as IDBFactory,
  'clearra-runtime-v1-unknown-commit-contract'
);
const unknownCommitAuthority = await DurableDelegationAuthority.recover(
  unknownCommitWriter,
  () => 1_500
);
await unknownCommitAuthority.prepare(identity('indexeddb-unknown-commit'), budget());
unknownCommitWriter.close();
const metaStore = unknownCommitFactory.database.stores.get('delegation-meta-v1')!;
const committedHead = metaStore.get('head') as Record<string, unknown>;
metaStore.set('head', { ...committedHead, eventSha256: 'aa'.repeat(32) });
const unknownCommitReader = await IndexedDbDelegationJournal.open(
  unknownCommitFactory as unknown as IDBFactory,
  'clearra-runtime-v1-unknown-commit-contract'
);
await assert.rejects(
  DurableDelegationAuthority.recover(unknownCommitReader, () => 1_500),
  /committed head does not match/
);
await assert.rejects(unknownCommitReader.load(), /is quarantined/);
unknownCommitReader.close();

const staleFactory = new FakeIdbFactory();
const staleJournalOne = await IndexedDbDelegationJournal.open(
  staleFactory as unknown as IDBFactory,
  'clearra-runtime-v1-stale-head-contract'
);
const staleJournalTwo = await IndexedDbDelegationJournal.open(
  staleFactory as unknown as IDBFactory,
  'clearra-runtime-v1-stale-head-contract'
);
const staleAuthorityOne = await DurableDelegationAuthority.recover(staleJournalOne, () => 2_000);
const staleAuthorityTwo = await DurableDelegationAuthority.recover(staleJournalTwo, () => 2_000);
await staleAuthorityOne.prepare(identity('indexeddb-first-writer'), budget());
await assert.rejects(
  staleAuthorityTwo.prepare(identity('indexeddb-stale-writer'), budget()),
  /head changed/
);
staleJournalOne.close();
staleJournalTwo.close();

let indexedCompactionNow = 3_000;
const compactionFactory = new FakeIdbFactory();
const indexedCompactionJournal = await IndexedDbDelegationJournal.open(
  compactionFactory as unknown as IDBFactory,
  'clearra-runtime-v1-compaction-contract'
);
const indexedCompactionAuthority = await DurableDelegationAuthority.recover(
  indexedCompactionJournal,
  () => indexedCompactionNow
);
const indexedCompactionToken = await indexedCompactionAuthority.prepare(
  identity('indexeddb-compaction'),
  budget()
);
await indexedCompactionAuthority.cancelled(indexedCompactionToken);
indexedCompactionNow += TERMINAL_TOMBSTONE_RETENTION_MS + 1;
compactionFactory.database.holdWriteCompletion = true;
let resetAcked = false;
const pendingReset = indexedCompactionAuthority
  .compactExpiredTerminalTombstones()
  .then((value) => {
    resetAcked = value;
    return value;
  });
await compactionFactory.database.waitForHeldWrite();
assert.equal(resetAcked, false, 'reset must not ACK before the IndexedDB transaction commits');
compactionFactory.database.releaseHeldWrite();
assert.equal(await pendingReset, true);
assert.equal((await indexedCompactionJournal.load()).length, 0);
indexedCompactionJournal.close();

const transportJournal = new MemoryDelegationJournal();
const transportAuthority = await DurableDelegationAuthority.recover(transportJournal);
const verifier = new DelegationContractWorker();
const pool = new ClearraVerifierPool(() => verifier as unknown as Worker, {
  delegationAuthority: transportAuthority
});
await pool.initialize(
  'clearra pc --lines 4',
  1,
  undefined,
  'durable-transport-contract',
  'atomic-task'
);
assert.deepEqual(verifier.messageTypes.slice(0, 3), [
  'prewarm',
  'delegation-offer',
  'initialize'
]);
assert.equal('initialization' in verifier.messages[1], false);
assert.equal('batch' in verifier.messages[1], false);
assert.equal(typeof verifier.messages[2].delegation, 'object');
assert.deepEqual(
  (await transportJournal.load()).map((event) => event.phase),
  [
    'prepared',
    'offered',
    'accepted',
    'published',
    'running',
    'result-sealed',
    'result-applied',
    'completed'
  ]
);
pool.cancel();
}

function identity(taskId: string) {
  return {
    jobId: 'job-1',
    taskId,
    coordinatorId: 'coordinator-1',
    payloadSha256: '11'.repeat(32),
    requestSha256: '22'.repeat(32)
  } as const;
}

function budget() {
  return { computeUnitsDecimal: '2', memoryBytesDecimal: '4096' } as const;
}

function canonicalMaterial(event: DelegationEvent): string {
  return JSON.stringify({
    schema: event.schema,
    sequence: event.sequenceDecimal,
    job_id: event.jobId,
    task_id: event.taskId,
    coordinator_id: event.coordinatorId,
    payload_sha256: event.payloadSha256,
    request_sha256: event.requestSha256,
    compute_units: event.computeUnitsDecimal,
    memory_bytes: event.memoryBytesDecimal,
    phase: event.phase,
    fencing_token: event.fencingTokenDecimal,
    worker_id: event.workerId,
    reservation_sha256: event.reservationSha256,
    result_sha256: event.resultSha256,
    worker_reply_sha256: event.workerReplySha256,
    timestamp_unix_ms: event.timestampUnixMsDecimal,
    reason: event.reason,
    previous_event_sha256: event.previousEventSha256
  });
}

type ContractWorkerMessage = Record<string, unknown> & {
  type: string;
  offer?: { taskId: string; fencingTokenDecimal: string };
};

class DelegationContractWorker {
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  onmessageerror: (() => void) | null = null;
  private readonly listeners = new Map<string, Set<(event: MessageEvent) => void>>();
  readonly messages: ContractWorkerMessage[] = [];
  private readonly staged = new Map<string, ContractWorkerMessage>();
  terminated = false;

  get messageTypes(): string[] {
    return this.messages.map((message) => message.type);
  }

  addEventListener(type: string, listener: (event: MessageEvent) => void): void {
    const listeners = this.listeners.get(type) ?? new Set();
    listeners.add(listener);
    this.listeners.set(type, listeners);
  }

  removeEventListener(type: string, listener: (event: MessageEvent) => void): void {
    this.listeners.get(type)?.delete(listener);
  }

  postMessage(message: ContractWorkerMessage): void {
    this.messages.push(message);
    queueMicrotask(() => {
      if (this.terminated) return;
      if (message.type === 'prewarm') this.emit({ type: 'prewarmed' });
      if (message.type === 'delegation-offer') {
        this.emit({
          type: 'delegation-accepted',
          acceptance: {
            taskId: message.offer!.taskId,
            fencingTokenDecimal: message.offer!.fencingTokenDecimal,
            workerId: '1',
            reservationSha256: '55'.repeat(32)
          }
        });
      }
      if (message.type === 'initialize') {
        const delegation = message.delegation as {
          taskId: string;
          fencingTokenDecimal: string;
        };
        this.staged.set(delegation.taskId, message);
        this.emit({
          type: 'delegation-started',
          taskId: delegation.taskId,
          fencingTokenDecimal: delegation.fencingTokenDecimal
        });
      }
      if (message.type === 'delegation-run') {
        const taskId = String(message.taskId);
        if (!this.staged.delete(taskId)) throw new Error('missing staged executable');
        this.emit({ type: 'ready' });
      }
    });
  }

  terminate(): void {
    this.terminated = true;
  }

  private emit(data: unknown): void {
    const event = { data } as MessageEvent;
    for (const listener of this.listeners.get('message') ?? []) listener(event);
    this.onmessage?.(event);
  }
}

class FakeIdbFactory {
  readonly database = new FakeIdbDatabase();

  open(): IDBOpenDBRequest {
    const request = new FakeIdbRequest<FakeIdbDatabase>();
    request.result = this.database;
    queueMicrotask(() => {
      request.onupgradeneeded?.({} as IDBVersionChangeEvent);
      queueMicrotask(() => request.onsuccess?.({} as Event));
    });
    return request as unknown as IDBOpenDBRequest;
  }
}

class FakeIdbDatabase {
  readonly stores = new Map<string, Map<string, unknown>>();
  readonly transactionModes: IDBTransactionMode[] = [];
  holdWriteCompletion = false;
  failWriteAt: number | null = null;
  failReadKey: string | null = null;
  failReadMode: IDBTransactionMode = 'readwrite';
  failReadStore: string | null = null;
  abortWriteBeforeReads = false;
  abortReadBeforeReads = false;
  abortReadStore: string | null = null;
  closeCount = 0;
  private heldWrite: FakeIdbTransaction | null = null;
  private heldWriteReady: (() => void) | null = null;

  readonly objectStoreNames = {
    contains: (name: string) => this.stores.has(name)
  };

  createObjectStore(name: string): IDBObjectStore {
    this.stores.set(name, new Map());
    return {} as IDBObjectStore;
  }

  transaction(names: string[], mode: IDBTransactionMode): IDBTransaction {
    this.transactionModes.push(mode);
    const transaction = new FakeIdbTransaction(this, names, mode);
    if (mode === 'readwrite' && this.holdWriteCompletion) {
      this.heldWrite = transaction;
      this.heldWriteReady?.();
      this.heldWriteReady = null;
    }
    return transaction as unknown as IDBTransaction;
  }

  waitForHeldWrite(): Promise<void> {
    if (this.heldWrite) return Promise.resolve();
    return new Promise((resolve) => {
      this.heldWriteReady = resolve;
    });
  }

  releaseHeldWrite(): void {
    this.holdWriteCompletion = false;
    const transaction = this.heldWrite;
    this.heldWrite = null;
    transaction?.complete();
  }

  close(): void { this.closeCount += 1; }
}

class FakeIdbTransaction {
  oncomplete: ((event: Event) => void) | null = null;
  onabort: ((event: Event) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  error: DOMException | null = null;
  private completionScheduled = false;
  private finished = false;
  private writeCount = 0;
  private readonly failWriteAt: number | null;
  private readonly failReadKey: string | null;
  private readonly stagedStores = new Map<string, Map<string, unknown>>();

  constructor(
    private readonly database: FakeIdbDatabase,
    private readonly names: string[],
    private readonly mode: IDBTransactionMode
  ) {
    this.failWriteAt = mode === 'readwrite' ? database.failWriteAt : null;
    const matchesReadFailure = mode === database.failReadMode &&
      (database.failReadStore === null || names.includes(database.failReadStore));
    this.failReadKey = matchesReadFailure ? database.failReadKey : null;
    if (matchesReadFailure) database.failReadKey = null;
    if (mode === 'readwrite') {
      database.failWriteAt = null;
      if (database.abortWriteBeforeReads) {
        database.abortWriteBeforeReads = false;
        queueMicrotask(() => {
          this.error = new DOMException('synthetic abort before metadata responses', 'AbortError');
          this.abort();
        });
      }
    }
    if (mode === 'readonly' && database.abortReadBeforeReads &&
        (database.abortReadStore === null || names.includes(database.abortReadStore))) {
      database.abortReadBeforeReads = false;
      queueMicrotask(() => {
        this.error = new DOMException('synthetic readonly abort before metadata responses', 'AbortError');
        this.abort();
      });
    }
    for (const name of names) this.stagedStores.set(name, new Map(database.stores.get(name)!));
  }

  objectStore(name: string): IDBObjectStore {
    assert.equal(this.names.includes(name), true);
    const store = this.mode === 'readwrite' ? this.stagedStores.get(name)! : this.database.stores.get(name)!;
    return {
      getAll: () => {
        const request = new FakeIdbRequest<unknown[]>();
        queueMicrotask(() => {
          if (this.finished) return;
          if (this.failReadKey === '*getAll') {
            this.error = request.error = new DOMException('synthetic event read failure', 'AbortError');
            request.onerror?.({} as Event);
            this.abort();
            return;
          }
          request.result = [...store.values()];
          request.onsuccess?.({} as Event);
          this.scheduleCompletion();
        });
        return request;
      },
      get: (key: string) => {
        const request = new FakeIdbRequest<unknown>();
        queueMicrotask(() => {
          if (this.finished) return;
          if (String(key) === this.failReadKey) {
            this.error = request.error = new DOMException('synthetic metadata read failure', 'AbortError');
            request.onerror?.({} as Event);
            this.abort();
            return;
          }
          request.result = store.get(String(key));
          request.onsuccess?.({} as Event);
          this.scheduleCompletion();
        });
        return request;
      },
      add: (value: Record<string, unknown>) => {
        this.beforeWrite();
        store.set(String(value.sequenceDecimal), structuredClone(value));
        this.scheduleCompletion();
        return new FakeIdbRequest();
      },
      put: (value: Record<string, unknown>) => {
        this.beforeWrite();
        store.set(String(value.key), structuredClone(value));
        this.scheduleCompletion();
        return new FakeIdbRequest();
      },
      clear: () => {
        this.beforeWrite();
        store.clear();
        this.scheduleCompletion();
        return new FakeIdbRequest();
      },
      delete: (key: string) => {
        this.beforeWrite();
        store.delete(String(key));
        this.scheduleCompletion();
        return new FakeIdbRequest();
      }
    } as unknown as IDBObjectStore;
  }

  complete(): void {
    // IDB completes after request callbacks and their microtasks, never while
    // the caller is still adding writes from a resolved head request.
    setTimeout(() => {
      if (this.finished) return;
      this.finished = true;
      if (this.mode === 'readwrite') {
        for (const [name, staged] of this.stagedStores) this.database.stores.set(name, staged);
      }
      this.oncomplete?.({} as Event);
    }, 0);
  }

  abort(): void {
    if (this.finished) return;
    this.finished = true;
    queueMicrotask(() => this.onabort?.({} as Event));
  }

  private beforeWrite(): void {
    if (this.finished) throw new DOMException('transaction is inactive', 'TransactionInactiveError');
    if (++this.writeCount === this.failWriteAt) {
      this.error = new DOMException('synthetic atomic write failure', 'AbortError');
      this.abort();
      throw this.error;
    }
  }

  private scheduleCompletion(): void {
    if (this.completionScheduled) return;
    this.completionScheduled = true;
    queueMicrotask(() => {
      if (this.mode === 'readwrite' && this.database.holdWriteCompletion) return;
      this.complete();
    });
  }
}

class FakeIdbRequest<T = unknown> {
  result!: T;
  error: DOMException | null = null;
  onsuccess: ((event: Event) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  onupgradeneeded: ((event: IDBVersionChangeEvent) => void) | null = null;
}

await runContract();
