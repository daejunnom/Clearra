import assert from 'node:assert/strict';

import {
  ACTIVE_LEASE_EXPIRY_MS,
  DurableDelegationAuthority,
  IndexedDbDelegationJournal,
  MemoryDelegationJournal,
  OFFER_ACCEPT_TIMEOUT_MS,
  PERSISTED_RENEWAL_INTERVAL_MS,
  TERMINAL_TOMBSTONE_RETENTION_MS,
  WORKER_HEARTBEAT_INTERVAL_MS,
  sha256Hex,
  type DelegationEvent
} from '../src/workers/DurableDelegationJournal.ts';
import { ClearraVerifierPool } from '../src/workers/ClearraVerifierPool.ts';

async function runContract() {
assert.equal(OFFER_ACCEPT_TIMEOUT_MS, 10_000);
assert.equal(WORKER_HEARTBEAT_INTERVAL_MS, 5_000);
assert.equal(PERSISTED_RENEWAL_INTERVAL_MS, 30_000);
assert.equal(ACTIVE_LEASE_EXPIRY_MS, 120_000);
assert.equal(TERMINAL_TOMBSTONE_RETENTION_MS, 86_400_000);

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
indexedJournal.close();

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
  holdWriteCompletion = false;
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

  close(): void {}
}

class FakeIdbTransaction {
  oncomplete: ((event: Event) => void) | null = null;
  onabort: ((event: Event) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  error: DOMException | null = null;
  private completionScheduled = false;

  constructor(
    private readonly database: FakeIdbDatabase,
    private readonly names: string[],
    private readonly mode: IDBTransactionMode
  ) {}

  objectStore(name: string): IDBObjectStore {
    assert.equal(this.names.includes(name), true);
    const store = this.database.stores.get(name)!;
    return {
      getAll: () => {
        const request = new FakeIdbRequest<unknown[]>();
        queueMicrotask(() => {
          request.result = [...store.values()];
          request.onsuccess?.({} as Event);
          this.scheduleCompletion();
        });
        return request;
      },
      get: (key: string) => {
        const request = new FakeIdbRequest<unknown>();
        queueMicrotask(() => {
          request.result = store.get(String(key));
          request.onsuccess?.({} as Event);
          this.scheduleCompletion();
        });
        return request;
      },
      add: (value: Record<string, unknown>) => {
        store.set(String(value.sequenceDecimal), structuredClone(value));
        this.scheduleCompletion();
        return new FakeIdbRequest();
      },
      put: (value: Record<string, unknown>) => {
        store.set(String(value.key), structuredClone(value));
        this.scheduleCompletion();
        return new FakeIdbRequest();
      },
      clear: () => {
        store.clear();
        this.scheduleCompletion();
        return new FakeIdbRequest();
      },
      delete: (key: string) => {
        store.delete(String(key));
        this.scheduleCompletion();
        return new FakeIdbRequest();
      }
    } as unknown as IDBObjectStore;
  }

  complete(): void {
    queueMicrotask(() => this.oncomplete?.({} as Event));
  }

  abort(): void {
    queueMicrotask(() => this.onabort?.({} as Event));
  }

  private scheduleCompletion(): void {
    if (this.completionScheduled) return;
    this.completionScheduled = true;
    queueMicrotask(() => {
      if (this.mode === 'readwrite' && this.database.holdWriteCompletion) return;
      this.oncomplete?.({} as Event);
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
