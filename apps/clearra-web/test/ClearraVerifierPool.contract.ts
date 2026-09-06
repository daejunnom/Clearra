import assert from 'node:assert/strict';

// SRP rationale: this executable contract's single change reason is the verifier
// pool's exact delegated-work lifecycle across readiness, failure, cancellation
// and drain. Fake workers and timing controls exercise that pool contract;
// they do not implement search or replace the durable journal's authority.

import { ClearraVerifierPool } from '../src/workers/ClearraVerifierPool.ts';
import { VerifierTransportProfile } from '../src/workers/VerifierTransportProfile.ts';
import {
  DurableDelegationAuthority,
  MemoryDelegationJournal
} from '../src/workers/DurableDelegationJournal.ts';

async function bounded<T>(label: string, operation: Promise<T>): Promise<T> {
  let timeout: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      operation,
      new Promise<never>((_, reject) => {
        timeout = setTimeout(
          () => reject(new Error(`${label} did not settle`)),
          2_000
        );
      })
    ]);
  } finally {
    if (timeout !== undefined) clearTimeout(timeout);
  }
}

type WorkerMessage = {
  type: string;
  requestId?: number;
  batch?: ArrayBuffer;
  offer?: {
    taskId: string;
    fencingTokenDecimal: string;
  };
  delegation?: {
    taskId: string;
    fencingTokenDecimal: string;
  };
  taskId?: string;
  fencingTokenDecimal?: string;
};

class FakeVerifierWorker {
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  onmessageerror: (() => void) | null = null;
  private listeners = new Map<string, Set<(event: MessageEvent) => void>>();
  private consumed: number[] = [];
  private consumeCount = 0;
  private staged = new Map<string, WorkerMessage>();
  terminated = false;

  constructor(private readonly failOnSecondConsume: boolean) {}

  addEventListener(type: string, listener: (event: MessageEvent) => void) {
    const listeners = this.listeners.get(type) ?? new Set();
    listeners.add(listener);
    this.listeners.set(type, listeners);
  }

  removeEventListener(type: string, listener: (event: MessageEvent) => void) {
    this.listeners.get(type)?.delete(listener);
  }

  postMessage(message: WorkerMessage) {
    queueMicrotask(() => this.handle(message));
  }

  terminate() {
    this.terminated = true;
  }

  private handle(message: WorkerMessage) {
    if (this.terminated) return;
    if (message.type === 'delegation-offer') {
      this.emit(delegationAccepted(message));
      return;
    }
    if (message.type === 'prewarm') {
      this.emit({ type: 'prewarmed' });
      return;
    }
    if (message.type === 'delegation-run') {
      const staged = this.staged.get(message.taskId!);
      if (!staged || staged.delegation?.fencingTokenDecimal !== message.fencingTokenDecimal) {
        throw new Error('missing staged verifier executable');
      }
      this.staged.delete(message.taskId!);
      this.handleExecutable(staged);
      return;
    }
    if (
      message.type === 'initialize' ||
      message.type === 'consume' ||
      message.type === 'finish'
    ) {
      const delegation = message.delegation!;
      this.staged.set(delegation.taskId, message);
      this.emit({
        type: 'delegation-started',
        taskId: delegation.taskId,
        fencingTokenDecimal: delegation.fencingTokenDecimal
      });
      return;
    }
  }

  protected handleExecutable(message: WorkerMessage) {
    if (message.type === 'initialize') {
      this.consumed = [];
      this.consumeCount = 0;
      this.emit({ type: 'ready' });
      return;
    }
    if (message.type === 'consume') {
      this.consumeCount += 1;
      if (this.failOnSecondConsume && this.consumeCount === 2) {
        this.onerror?.({ message: 'synthetic worker transport failure' } as ErrorEvent);
        return;
      }
      this.consumed.push(new Uint8Array(message.batch!)[0]);
      this.emit({
        type: 'consumed',
        requestId: message.requestId,
        candidateCount: 1,
        candidateCountAvailable: true,
        candidateCountExact: true,
        partial: null,
        progress: exactVerifierProgress(this.consumed.length)
      });
      return;
    }
    const total = this.consumed.reduce((sum, value) => sum + value, 0);
    this.emit({
      type: 'finished',
      requestId: message.requestId,
      partial: Uint8Array.of(total).buffer
    });
  }

  protected emit(data: unknown) {
    const event = { data } as MessageEvent;
    for (const listener of this.listeners.get('message') ?? []) listener(event);
    this.onmessage?.(event);
  }
}

const workers: FakeVerifierWorker[] = [];
const pool = new ClearraVerifierPool(() => {
  const worker = new FakeVerifierWorker(workers.length === 0);
  workers.push(worker);
  return worker as unknown as Worker;
});

await bounded(
  'replay initialize',
  pool.initialize('clearra pc --lines 4', 1, undefined, 'contract-owner', 'replay-state')
);
await bounded('replay first enqueue', pool.enqueue(Uint8Array.of(1).buffer, () => undefined));
await bounded('replay first idle', pool.waitForIdle());
await bounded('replay retry enqueue', pool.enqueue(Uint8Array.of(2).buffer, () => undefined));
await assert.rejects(
  bounded('replay retry idle', pool.waitForIdle()),
  /synthetic worker transport failure/
);

assert.equal(workers.length, 1);
assert.equal(workers[0].terminated, true);

class StreamingVerifierWorker {
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  onmessageerror: (() => void) | null = null;
  private listeners = new Map<string, Set<(event: MessageEvent) => void>>();
  private staged = new Map<string, WorkerMessage>();
  terminated = false;

  constructor(private readonly failAfterPartial: boolean) {}

  addEventListener(type: string, listener: (event: MessageEvent) => void) {
    const listeners = this.listeners.get(type) ?? new Set();
    listeners.add(listener);
    this.listeners.set(type, listeners);
  }

  removeEventListener(type: string, listener: (event: MessageEvent) => void) {
    this.listeners.get(type)?.delete(listener);
  }

  postMessage(message: WorkerMessage) {
    queueMicrotask(() => this.handle(message));
  }

  terminate() {
    this.terminated = true;
  }

  private handle(message: WorkerMessage) {
    if (this.terminated) return;
    if (message.type === 'delegation-offer') {
      this.emit(delegationAccepted(message));
      return;
    }
    if (message.type === 'prewarm') {
      this.emit({ type: 'prewarmed' });
      return;
    }
    if (message.type === 'delegation-run') {
      const staged = this.staged.get(message.taskId!);
      if (!staged || staged.delegation?.fencingTokenDecimal !== message.fencingTokenDecimal) {
        throw new Error('missing staged verifier executable');
      }
      this.staged.delete(message.taskId!);
      this.execute(staged);
      return;
    }
    if (
      message.type === 'initialize' ||
      message.type === 'consume' ||
      message.type === 'finish'
    ) {
      const delegation = message.delegation!;
      this.staged.set(delegation.taskId, message);
      this.emit({
        type: 'delegation-started',
        taskId: delegation.taskId,
        fencingTokenDecimal: delegation.fencingTokenDecimal
      });
      return;
    }
  }

  private execute(message: WorkerMessage) {
    if (message.type === 'initialize') {
      this.emit({ type: 'ready' });
      return;
    }
    if (message.type === 'consume') {
      this.emit({ type: 'partial', requestId: message.requestId, partial: Uint8Array.of(7).buffer });
      if (this.failAfterPartial) {
        this.onerror?.({ message: 'synthetic post-commit transport failure' } as ErrorEvent);
        return;
      }
      this.emit({
        type: 'consumed',
        requestId: message.requestId,
        candidateCount: 1,
        candidateCountAvailable: true,
        candidateCountExact: true,
        partial: null,
        progress: exactVerifierProgress(1)
      });
      return;
    }
    this.emit({ type: 'finished', requestId: message.requestId, partial: new ArrayBuffer(0) });
  }

  private emit(data: unknown) {
    const event = { data } as MessageEvent;
    for (const listener of this.listeners.get('message') ?? []) listener(event);
    this.onmessage?.(event);
  }
}

function exactVerifierProgress(candidateCount: number) {
  return {
    candidateCount,
    buildNodes: 0,
    coverageChecks: 0,
    availability: {
      candidateCount: true,
      buildNodes: true,
      coverageChecks: true
    },
    exactness: {
      candidateCount: true,
      buildNodes: true,
      coverageChecks: true
    }
  };
}

function delegationAccepted(message: WorkerMessage) {
  if (!message.offer) throw new Error('delegation offer is absent');
  return {
    type: 'delegation-accepted',
    acceptance: {
      taskId: message.offer.taskId,
      fencingTokenDecimal: message.offer.fencingTokenDecimal,
      workerId: '1',
      reservationSha256: '33'.repeat(32)
    }
  };
}

const streamingWorkers: StreamingVerifierWorker[] = [];
const streamingPool = new ClearraVerifierPool(() => {
  const worker = new StreamingVerifierWorker(streamingWorkers.length === 0);
  streamingWorkers.push(worker);
  return worker as unknown as Worker;
});
const streamed: number[] = [];
await bounded(
  'streaming initialize',
  streamingPool.initialize(
    new ArrayBuffer(0),
    1,
    undefined,
    'streaming-contract-owner',
    'streaming'
  )
);
await bounded(
  'streaming enqueue',
  streamingPool.enqueue(Uint8Array.of(9).buffer, (partial) =>
    streamed.push(new Uint8Array(partial)[0])
  )
);
await assert.rejects(
  bounded('streaming idle', streamingPool.waitForIdle()),
  /synthetic post-commit transport failure/
);

assert.equal(streamingWorkers.length, 1);
assert.equal(streamingWorkers[0].terminated, true);
assert.deepEqual(streamed, [], 'streamed partial must not escape before immutable result sealing');

class FullInitializationVerifierWorker extends FakeVerifierWorker {
  constructor(
    failOnSecondConsume: boolean,
    private readonly initializeImmediately: boolean
  ) {
    super(failOnSecondConsume);
  }

  override postMessage(message: WorkerMessage) {
    if (message.type === 'initialize' && !this.initializeImmediately) return;
    super.postMessage(message);
  }
}

const fullInitializationWorkers: FullInitializationVerifierWorker[] = [];
const fullInitializationPool = new ClearraVerifierPool(() => {
  const worker = new FullInitializationVerifierWorker(
    false,
    fullInitializationWorkers.length === 0
  );
  fullInitializationWorkers.push(worker);
  return worker as unknown as Worker;
}, {
  initializationTimeoutMs: 25
});

await assert.rejects(
  bounded(
    'full verifier initialization',
    fullInitializationPool.initialize(
      new ArrayBuffer(0),
      2,
      undefined,
      'full-initialization-contract-owner',
      'atomic-task'
    )
  ),
  /initialization timed out/
);
assert.equal(fullInitializationWorkers.length, 2);
assert.equal(
  fullInitializationWorkers.filter((worker) => worker.terminated).length,
  2
);

class ProgressiveInitializationVerifierWorker extends FakeVerifierWorker {
  private heldInitialization: WorkerMessage | null = null;
  consumePostCount = 0;

  constructor(private readonly initializeImmediately: boolean) {
    super(false);
  }

  override postMessage(message: WorkerMessage) {
    if (message.type === 'initialize' && !this.initializeImmediately) {
      this.heldInitialization = message;
      return;
    }
    if (message.type === 'consume') this.consumePostCount += 1;
    super.postMessage(message);
  }

  releaseInitialization() {
    const message = this.heldInitialization;
    this.heldInitialization = null;
    if (message) super.postMessage(message);
  }
}

const progressiveInitializationWorkers: ProgressiveInitializationVerifierWorker[] = [];
const progressiveInitializationPool = new ClearraVerifierPool(() => {
  const worker = new ProgressiveInitializationVerifierWorker(
    progressiveInitializationWorkers.length === 0
  );
  progressiveInitializationWorkers.push(worker);
  return worker as unknown as Worker;
});
const progressiveInitialization = progressiveInitializationPool.initialize(
  new ArrayBuffer(0),
  2,
  undefined,
  'progressive-initialization-contract-owner',
  'atomic-task'
);
let completeInitializationSettled = false;
void progressiveInitialization.then(() => {
  completeInitializationSettled = true;
});
await bounded(
  'first ready verifier consumes before full initialization',
  progressiveInitializationPool.enqueue(Uint8Array.of(5).buffer, () => undefined)
);
await bounded(
  'progressive verifier first batch',
  progressiveInitializationPool.waitForIdle()
);
assert.equal(completeInitializationSettled, false);
assert.equal(progressiveInitializationWorkers[0].consumePostCount, 1);
assert.equal(progressiveInitializationWorkers[1].consumePostCount, 0);
assert.equal(progressiveInitializationPool.progressSnapshot().readyWorkers, 1);
progressiveInitializationWorkers[1].releaseInitialization();
await bounded('progressive verifier full initialization', progressiveInitialization);
assert.equal(progressiveInitializationPool.progressSnapshot().readyWorkers, 2);
await bounded(
  'progressive verifier finish',
  progressiveInitializationPool.finish(() => undefined)
);

const terminalSubsetWorkers: ProgressiveInitializationVerifierWorker[] = [];
const terminalSubsetPool = new ClearraVerifierPool(() => {
  const worker = new ProgressiveInitializationVerifierWorker(
    terminalSubsetWorkers.length === 0
  );
  terminalSubsetWorkers.push(worker);
  return worker as unknown as Worker;
});
const terminalSubsetInitialization = terminalSubsetPool.initialize(
  new ArrayBuffer(0),
  2,
  undefined,
  'terminal-subset-contract-owner',
  'atomic-task'
);
await bounded(
  'terminal subset first ready verifier consumes',
  terminalSubsetPool.enqueue(Uint8Array.of(7).buffer, () => undefined)
);
await bounded('terminal subset first batch', terminalSubsetPool.waitForIdle());
assert.equal(terminalSubsetPool.progressSnapshot().readyWorkers, 1);
assert.equal(
  await bounded(
    'terminal subset finish does not wait for a never-ready initializer',
    terminalSubsetPool.finish(() => undefined, { readySubset: true })
  ),
  1
);
await bounded('retired terminal initializer settles', terminalSubsetInitialization);
assert.equal(terminalSubsetWorkers[0].terminated, false);
assert.equal(
  terminalSubsetWorkers[1].terminated,
  true,
  'producer completion retires only the verifier that never became ready'
);

class PrewarmGateVerifierWorker extends FakeVerifierWorker {
  constructor() {
    super(false);
  }

  override postMessage(message: WorkerMessage) {
    if (message.type === 'prewarm') return;
    super.postMessage(message);
  }
}

const prewarmGateWorkers: PrewarmGateVerifierWorker[] = [];
const prewarmGatePool = new ClearraVerifierPool(() => {
  const worker = new PrewarmGateVerifierWorker();
  prewarmGateWorkers.push(worker);
  return worker as unknown as Worker;
});
const pendingPrewarm = prewarmGatePool.prewarm(2, undefined, 'prewarm-gate-owner');
await new Promise<void>((resolve) => setTimeout(resolve, 0));
prewarmGatePool.cancel();
await bounded('cancelled prewarm', pendingPrewarm);
assert.equal(prewarmGateWorkers.filter((worker) => worker.terminated).length, 2);

{
  class CachedPrewarmWorker extends FakeVerifierWorker {
    prewarmPosts = 0;
    initializePosts = 0;
    lastInitialization: WorkerMessage | null = null;
    constructor() { super(false); }
    override postMessage(message: WorkerMessage) {
      if (message.type === 'prewarm') this.prewarmPosts += 1;
      if (message.type === 'initialize') {
        this.initializePosts += 1;
        this.lastInitialization = message;
      }
      super.postMessage(message);
    }
  }
  const cachedWorkers: CachedPrewarmWorker[] = [];
  const cachedPool = new ClearraVerifierPool(() => {
    const worker = new CachedPrewarmWorker();
    cachedWorkers.push(worker);
    return worker as unknown as Worker;
  });
  const hostCapabilities = {
    logicalProcessorCount: 12, webGpuAvailable: false,
    crossOriginIsolated: true, transferByteCap: 32 * 1024 * 1024
  };
  const liveWorkers = () => cachedWorkers.filter((worker) => !worker.terminated);
  await cachedPool.prewarm(10, undefined, 'cached-owner', hostCapabilities);
  await cachedPool.prewarm(8, undefined, 'cached-owner', hostCapabilities);
  assert.equal(liveWorkers().length, 10, 'post-job eager warmup is a floor, not a shrink request');
  assert.equal(cachedWorkers.length, 10, 'smaller warmup does not recreate warm clients');
  assert.ok(liveWorkers().every((worker) => worker.prewarmPosts === 1));
  assert.equal(cachedPool.progressSnapshot().activeWorkers, 0);
  assert.equal(cachedPool.progressSnapshot().readyWorkers, 0);
  assert.equal(cachedPool.progressSnapshot().workerCount, 0);
  await assert.rejects(cachedPool.enqueue(Uint8Array.of(1).buffer, () => undefined), /cancelled/,
    'cached modules alone never grant task execution');

  // Remote pools 10/11 correspond to default 11 / full 12 total workers.
  // Explicit job initialization still admits exactly its requested count.
  for (const remoteCount of [10, 11, 10, 3]) {
    await bounded(`cached pool initialize ${remoteCount}`, cachedPool.initialize(
      new ArrayBuffer(0), remoteCount, undefined, 'cached-owner', 'atomic-task', hostCapabilities
    ));
    assert.equal(liveWorkers().length, remoteCount);
    assert.equal(cachedPool.progressSnapshot().workerCount, remoteCount);
    assert.equal(cachedPool.progressSnapshot().readyWorkers, remoteCount);
    assert.ok(liveWorkers().every((worker) => worker.initializePosts > 0));
    assert.ok(liveWorkers().every((worker) =>
      (worker.lastInitialization as WorkerMessage & { hostCapabilities?: unknown }).hostCapabilities === hostCapabilities),
    'every initialization still forwards current host capabilities through the authorized path');
    await bounded(`cached pool finish ${remoteCount}`, cachedPool.finish(() => undefined));
    await cachedPool.prewarm(8, undefined, 'cached-owner', hostCapabilities);
    assert.equal(liveWorkers().length, Math.max(remoteCount, 8));
    assert.equal(cachedPool.progressSnapshot().activeWorkers, 0);
    assert.equal(cachedPool.progressSnapshot().readyWorkers, 0);
    assert.equal(cachedPool.progressSnapshot().workerCount, 0);
  }
  const previousOwnerWorkers = liveWorkers();
  await cachedPool.prewarm(8, undefined, 'replacement-owner', hostCapabilities);
  assert.ok(previousOwnerWorkers.every((worker) => worker.terminated),
    'a new module lifecycle still retires every cached worker from the old owner');
  assert.equal(liveWorkers().length, 8);
  cachedPool.cancel();
  assert.ok(cachedWorkers.every((worker) => worker.terminated),
    'cancellation releases all retained warm clients, not just the last job size');
}

{
  class TerminalFailureWorker extends FakeVerifierWorker {
    consumePosts = 0;
    constructor() { super(false); }
    override postMessage(message: WorkerMessage) {
      if (message.type === 'consume') this.consumePosts += 1;
      super.postMessage(message);
    }
    protected override handleExecutable(message: WorkerMessage) {
      if (message.type !== 'consume') { super.handleExecutable(message); return; }
      this.emit({ type: 'consumed', requestId: message.requestId,
        candidateCount: 1, candidateCountAvailable: true, candidateCountExact: true,
        partial: Uint8Array.of(7).buffer, progress: exactVerifierProgress(1) });
    }
  }
  const journal = new MemoryDelegationJournal();
  const authority = await DurableDelegationAuthority.recover(journal);
  const worker = new TerminalFailureWorker();
  const guardedPool = new ClearraVerifierPool(() => worker as unknown as Worker, { delegationAuthority: authority });
  await guardedPool.initialize(new ArrayBuffer(0), 1, undefined, 'terminal-pair-failure');
  const entered = signal();
  const release = signal();
  const originalTerminal = authority.resultAppliedAndCompleted.bind(authority);
  authority.resultAppliedAndCompleted = async (token) => {
    entered.resolve();
    await release.promise;
    journal.failNextAppend(new Error('terminal pair commit failed'));
    await originalTerminal(token);
  };
  let applied = 0;
  await guardedPool.enqueue(Uint8Array.of(1).buffer, () => { applied += 1; });
  const idle = guardedPool.waitForIdle().then(() => null, (error: unknown) => error);
  const second = guardedPool.enqueue(Uint8Array.of(2).buffer, () => { applied += 1; })
    .then(() => null, (error: unknown) => error);
  await bounded('terminal commit blocks lease reuse', entered.promise);
  assert.equal(worker.consumePosts, 1);
  assert.equal(applied, 1);
  release.resolve();
  const results = await bounded('terminal failure poisons pending pool work', Promise.all([idle, second]));
  assert.ok(results.every((error) => error instanceof Error));
  assert.equal(worker.terminated, true);
  assert.equal(worker.consumePosts, 1, 'failed terminal ACK never frees an executable lease for the next task');
  assert.equal(applied, 1, 'failed ACK does not replay an already-applied immutable result');
  const consumeEvents = (await journal.load()).filter((event) => event.taskId.endsWith(':consume'));
  assert.equal(consumeEvents.at(-1)!.phase, 'failed-closed');
  assert.ok(consumeEvents.every((event) => !['result-applied', 'completed'].includes(event.phase)));
}

// A rejected old initializer can finish unwinding after a new query starts.
// Its generation must not fail or terminate the replacement pool.
let supersededFactoryCalls = 0;
const supersededPool = new ClearraVerifierPool(() =>
  (++supersededFactoryCalls === 1
    ? new PrewarmGateVerifierWorker()
    : new FakeVerifierWorker(false)) as unknown as Worker
);
const supersededInitialization = supersededPool.initialize(new ArrayBuffer(0), 1);
await new Promise<void>((resolve) => setTimeout(resolve, 0));
supersededPool.cancel();
const replacementInitialization = supersededPool.initialize(new ArrayBuffer(0), 1);
await bounded('superseded initializer settles without poisoning next query', supersededInitialization);
await bounded('replacement initializer stays active', replacementInitialization);
await bounded('replacement query consumes', supersededPool.enqueue(Uint8Array.of(3).buffer, () => undefined));
await bounded('replacement query receipts drain', supersededPool.completeAtomicTasks());

class ExactCancellationLatchWorker extends FakeVerifierWorker {
  cancellationIds: number[] = [];
  consumeIds: number[] = [];

  constructor() { super(false); }

  override postMessage(message: WorkerMessage) {
    if (message.type === 'cancel-exact-task') {
      this.cancellationIds.push(message.requestId!);
    }
    if (message.type === 'consume') this.consumeIds.push(message.requestId!);
    super.postMessage(message);
  }
}

const latchWorker = new ExactCancellationLatchWorker();
const latchPool = new ClearraVerifierPool(() => latchWorker as unknown as Worker);
await bounded('exact latch initialize', latchPool.initialize(new ArrayBuffer(0), 1, undefined, 'latch-owner', 'atomic-task', undefined, 'exact-at-most'));
// Both the active delegation gap and an already-issued task waiting for a
// client are represented by cancellation before the consume becomes pending.
latchPool.cancelExactTasks();
await bounded('late issued exact task still receives executable identity', latchPool.enqueue(Uint8Array.of(9).buffer, () => undefined));
await bounded('late cancelled exact task drains', latchPool.completeAtomicTasks());
assert.deepEqual(latchWorker.cancellationIds, latchWorker.consumeIds);
assert.equal(latchWorker.cancellationIds.length, 1);
await bounded('next exact query clears cancellation latch', latchPool.initialize(new ArrayBuffer(0), 1, undefined, 'latch-owner', 'atomic-task', undefined, 'exact-at-most'));
await bounded('next exact task executes normally', latchPool.enqueue(Uint8Array.of(10).buffer, () => undefined));
await bounded('next exact query drains', latchPool.completeAtomicTasks());
assert.equal(latchWorker.consumeIds.length, 2);
assert.equal(latchWorker.cancellationIds.length, 1, 'previous query cancellation cannot leak into next generation');

{
  const selectiveWorkers: ExactCancellationLatchWorker[] = [];
  const selective = new ClearraVerifierPool(() => {
    const worker = new ExactCancellationLatchWorker();
    selectiveWorkers.push(worker);
    return worker as unknown as Worker;
  });
  const key = (partition: number) => {
    const bytes = new Uint8Array(56); bytes[55] = partition; return bytes.buffer;
  };
  await selective.initialize(new ArrayBuffer(0), 2, undefined, 'selective-exact-cancel', 'atomic-task', undefined, 'exact-at-most');
  await selective.enqueueFromSource(() => Uint8Array.of(1).buffer, () => undefined, () => key(1));
  await selective.enqueueFromSource(() => Uint8Array.of(2).buffer, () => undefined, () => key(2));
  // Cancellation can arrive while the selected task is still publishing its
  // durable executable; it must latch for that task, but not the client's next.
  selective.cancelRedundantExactTasks((value) => new Uint8Array(value)[55] === 1);
  selective.cancelRedundantExactTasks((value) => new Uint8Array(value)[55] === 1);
  await selective.waitForIdle();
  assert.equal(selectiveWorkers.reduce((sum, worker) => sum + worker.cancellationIds.length, 0), 1);
  await selective.enqueueFromSource(() => Uint8Array.of(3).buffer, () => undefined, () => key(3));
  await selective.waitForIdle();
  assert.equal(selectiveWorkers.reduce((sum, worker) => sum + worker.cancellationIds.length, 0), 1,
    'one redundant parent does not cancel unrelated subsequent work');
  await selective.completeAtomicTasks();
  await selective.initialize(new ArrayBuffer(0), 1, undefined, 'selective-next-query');
  let staleKeys = 0;
  selective.cancelRedundantExactTasks(() => { staleKeys += 1; return true; });
  assert.equal(staleKeys, 0, 'previous query routing keys are not retained');
  await assert.rejects(selective.enqueueFromSource(() => Uint8Array.of(4).buffer, () => undefined,
    () => new ArrayBuffer(16)), /full core identity/);
  selective.cancel();
}

class HeartbeatVerifierWorker extends FakeVerifierWorker {
  constructor() {
    super(false);
  }

  override postMessage(message: WorkerMessage) {
    if (message.type !== 'consume') {
      super.postMessage(message);
      return;
    }
    const heartbeat = setInterval(() => {
      if (this.terminated) {
        clearInterval(heartbeat);
        return;
      }
      this.emit({
        type: 'heartbeat',
        requestId: message.requestId,
        progress: {
          ...exactVerifierProgress(0),
          buildNodes: 1,
          coverageChecks: 2
        }
      });
    }, 10);
    setTimeout(() => {
      clearInterval(heartbeat);
      if (!this.terminated) super.postMessage(message);
    }, 80);
  }
}

const heartbeatWorkers: HeartbeatVerifierWorker[] = [];
const heartbeatPool = new ClearraVerifierPool(() => {
  const worker = new HeartbeatVerifierWorker();
  heartbeatWorkers.push(worker);
  return worker as unknown as Worker;
}, {
  requestStallTimeoutMs: 25
});
await bounded(
  'heartbeat initialize',
  heartbeatPool.initialize(
    new ArrayBuffer(0),
    1,
    undefined,
    'heartbeat-contract-owner',
    'atomic-task'
  )
);
await bounded(
  'heartbeat enqueue',
  heartbeatPool.enqueue(Uint8Array.of(4).buffer, () => undefined)
);
await bounded('heartbeat idle', heartbeatPool.waitForIdle());
assert.equal(heartbeatWorkers.length, 1);
assert.deepEqual(heartbeatPool.progressSnapshot(), {
  candidatesVerified: 1,
  buildNodes: 0,
  coverageChecks: 0,
  availability: {
    candidatesVerified: true,
    buildNodes: true,
    coverageChecks: true
  },
  exactness: {
    candidatesVerified: true,
    buildNodes: true,
    coverageChecks: true
  },
  readyWorkers: 1,
  activeWorkers: 0,
  workerCount: 1,
  oldestBatchMs: 0
});
await bounded('heartbeat finish', heartbeatPool.finish(() => undefined));

class FinishGateVerifierWorker extends FakeVerifierWorker {
  private pendingFinish: WorkerMessage | null = null;

  constructor() {
    super(false);
  }

  override postMessage(message: WorkerMessage) {
    if (message.type === 'finish') {
      this.pendingFinish = message;
      return;
    }
    super.postMessage(message);
  }

  releaseFinish() {
    const message = this.pendingFinish;
    this.pendingFinish = null;
    if (message) super.postMessage(message);
  }
}

const finishGateWorker = new FinishGateVerifierWorker();
const finishGatePool = new ClearraVerifierPool(
  () => finishGateWorker as unknown as Worker
);
await bounded(
  'finish gate initialize',
  finishGatePool.initialize(
    new ArrayBuffer(0),
    1,
    undefined,
    'finish-gate-owner',
    'atomic-task'
  )
);
const gatedFinish = finishGatePool.finish(() => undefined);
await bounded(
  'finish gate becomes active',
  (async () => {
    while (finishGatePool.progressSnapshot().activeWorkers !== 1) {
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
    }
  })()
);
assert.equal(finishGatePool.progressSnapshot().activeWorkers, 1);
finishGateWorker.releaseFinish();
await bounded('finish gate release', gatedFinish);

function signal() {
  let resolve!: () => void;
  const promise = new Promise<void>((complete) => { resolve = complete; });
  return { promise, resolve };
}

class ControlledFinishVerifierWorker extends FakeVerifierWorker {
  readonly finishStarted = signal();
  private pendingFinish: WorkerMessage | null = null;

  constructor(private readonly result: number) {
    super(false);
  }

  protected override handleExecutable(message: WorkerMessage) {
    if (message.type !== 'finish') {
      super.handleExecutable(message);
      return;
    }
    this.pendingFinish = message;
    this.finishStarted.resolve();
  }

  releaseFinish() {
    assert.ok(this.pendingFinish, 'only an authorized, running finalizer can complete');
    this.emit({
      type: 'finished',
      requestId: this.pendingFinish.requestId,
      partial: Uint8Array.of(this.result).buffer
    });
    this.pendingFinish = null;
  }
}

async function controlledFinishPool(owner: string) {
  const authority = await DurableDelegationAuthority.recover(new MemoryDelegationJournal());
  const workers: ControlledFinishVerifierWorker[] = [];
  const pool = new ClearraVerifierPool(() => {
    const worker = new ControlledFinishVerifierWorker(workers.length + 1);
    workers.push(worker);
    return worker as unknown as Worker;
  }, { delegationAuthority: authority });
  await bounded(`${owner} initialize`, pool.initialize(new ArrayBuffer(0), 2, undefined, owner));
  return { pool, workers, authority };
}

{
  const { pool, workers } = await controlledFinishPool('streaming-finish');
  const applied: number[] = [];
  const firstApplied = signal();
  let finished = false;
  const finish = pool.finish((partial) => {
    applied.push(new Uint8Array(partial)[0]);
    firstApplied.resolve();
  }).then((count) => { finished = true; return count; });
  await bounded('both finalizers running', Promise.all(workers.map((worker) => worker.finishStarted.promise)));
  assert.equal(pool.progressSnapshot().activeWorkers, 2);
  workers[0].releaseFinish();
  await bounded('fast finalizer committed before slow sibling', firstApplied.promise);
  assert.deepEqual(applied, [1]);
  assert.equal(finished, false);
  assert.equal(pool.progressSnapshot().activeWorkers, 1);
  assert.equal(pool.progressSnapshot().readyWorkers, 2);
  assert.equal(pool.progressSnapshot().workerCount, 2);
  workers[1].releaseFinish();
  assert.equal(await bounded('streaming finalizers complete', finish), 2);
  assert.deepEqual(applied, [1, 2], 'each sealed final result is applied exactly once');
}

for (const cancelPending of [false, true]) {
  const { pool, workers, authority } = await controlledFinishPool(`serialized-finish-${cancelPending}`);
  const applied: number[] = [];
  const commitStarted = signal();
  const releaseCommit = signal();
  const bothSealed = signal();
  const originalResultApplied = authority.resultAppliedAndCompleted.bind(authority);
  const originalResultSealed = authority.resultSealed.bind(authority);
  let sealedCount = 0;
  let commitCount = 0;
  authority.resultSealed = async (...args) => {
    await originalResultSealed(...args);
    if (++sealedCount === 2) bothSealed.resolve();
  };
  authority.resultAppliedAndCompleted = async (token) => {
    if (++commitCount === 1) {
      commitStarted.resolve();
      await releaseCommit.promise;
    }
    await originalResultApplied(token);
  };
  const finish = pool.finish((partial) => { applied.push(new Uint8Array(partial)[0]); });
  // Attach the rejection observer before cancelling the generation below.
  const result = finish.then((count) => ({ count }), (error: unknown) => ({ error }));
  await bounded('serialized finalizers running', Promise.all(workers.map((worker) => worker.finishStarted.promise)));
  workers[0].releaseFinish();
  await bounded('first durable final commit begins', commitStarted.promise);
  workers[1].releaseFinish();
  await bounded('second result sealed during first commit', bothSealed.promise);
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(applied, [1], 'the second immutable result waits for the first durable commit');
  assert.equal(commitCount, 1);
  if (cancelPending) pool.cancel();
  releaseCommit.resolve();
  const outcome = await bounded('serialized final commit settles', result);
  if (cancelPending) {
    assert.ok('error' in outcome && outcome.error instanceof Error);
    assert.match(outcome.error.message, /cancelled/);
    assert.deepEqual(applied, [1], 'a stale generation never applies queued final results');
  } else {
    assert.deepEqual(outcome, { count: 2 });
    assert.deepEqual(applied, [1, 2]);
    assert.equal(commitCount, 2);
  }
}

{
  const { pool, workers } = await controlledFinishPool('failed-final-commit');
  let applications = 0;
  const finish = pool.finish(() => {
    applications += 1;
    throw new Error('synthetic final merge failure');
  });
  const rejected = assert.rejects(finish, (error: unknown) => {
    assert.ok(error instanceof Error && error.cause instanceof Error);
    assert.match(error.message, /distributed verifier result commit failed/);
    assert.equal(error.cause.message, 'synthetic final merge failure');
    return true;
  });
  await bounded('failing finalizers running', Promise.all(workers.map((worker) => worker.finishStarted.promise)));
  workers[0].releaseFinish();
  workers[1].releaseFinish();
  await bounded('final commit fails closed', rejected);
  assert.equal(applications, 1, 'a failed commit poisons later queued applications');
  assert.ok(workers.every((worker) => worker.terminated));
}

class ControlledConsumeVerifierWorker extends FakeVerifierWorker {
  readonly consumeStarted = signal();
  private pendingConsume: WorkerMessage | null = null;
  constructor() { super(false); }
  protected override handleExecutable(message: WorkerMessage) {
    if (message.type !== 'consume') { super.handleExecutable(message); return; }
    this.pendingConsume = message;
    this.consumeStarted.resolve();
  }
  releaseConsume() {
    assert.ok(this.pendingConsume);
    const message = this.pendingConsume;
    this.pendingConsume = null;
    super.handleExecutable(message);
  }
}

for (const size of [1, 3, 11]) {
  const controlled: ControlledConsumeVerifierWorker[] = [];
  const sourcePool = new ClearraVerifierPool(() => {
    const worker = new ControlledConsumeVerifierWorker();
    controlled.push(worker);
    return worker as unknown as Worker;
  });
  await sourcePool.initialize(new ArrayBuffer(0), size, undefined, `lease-before-source-${size}`);
  await Promise.all(Array.from({ length: size }, (_, i) =>
    sourcePool.enqueueFromSource(() => Uint8Array.of(i).buffer, () => undefined)));
  await bounded('all reserved executors begin', Promise.all(controlled.map((worker) => worker.consumeStarted.promise)));
  const frontier = [99];
  let issued = 0;
  const waiting = sourcePool.enqueueFromSource(() => {
    issued += 1;
    const task = frontier.shift();
    return task === undefined ? null : Uint8Array.of(task).buffer;
  }, () => undefined);
  await Promise.resolve();
  assert.equal(issued, 0, 'an unavailable remote executor cannot reserve a core task');
  assert.equal(frontier.shift(), 99, 'the coordinator can still consume the last unissued task');
  controlled[0].releaseConsume();
  assert.equal(await bounded('empty source releases reserved executor', waiting), false);
  for (const worker of controlled.slice(1)) worker.releaseConsume();
  await sourcePool.waitForIdle();
  await assert.rejects(sourcePool.enqueueFromSource(() => { throw new Error('source rejected'); }, () => undefined), /source rejected/);
  assert.equal(await sourcePool.enqueueFromSource(() => null, () => undefined), false,
    'throwing and empty factories leave the same client reusable');
  assert.equal(await sourcePool.enqueueFromSource(() => Uint8Array.of(7).buffer, () => undefined), true);
  // Cancellation while all executors are reserved rejects the waiter without
  // invoking its factory or leaking an issued task into another generation.
  const cancelWait = sourcePool.enqueueFromSource(() => { issued += 1; return null; }, () => undefined);
  if (size === 1) {
    const rejected = assert.rejects(cancelWait, /cancelled/);
    sourcePool.cancel();
    await rejected;
    assert.equal(issued, 1);
  } else {
    assert.equal(await cancelWait, false);
    sourcePool.cancel();
  }
}

{
  const sourcePool = new ClearraVerifierPool(() => new FakeVerifierWorker(false) as unknown as Worker);
  await sourcePool.initialize(new ArrayBuffer(0), 1, undefined, 'reentrant-source-cancel');
  let applied = 0;
  await assert.rejects(sourcePool.enqueueFromSource(() => {
    sourcePool.cancel();
    return Uint8Array.of(1).buffer;
  }, () => { applied += 1; }), /cancelled/);
  assert.equal(applied, 0, 'a cancelled generation cannot publish the just-issued task');
  await sourcePool.initialize(new ArrayBuffer(0), 1, undefined, 'replacement-source-generation');
  assert.equal(await sourcePool.enqueueFromSource(() => null, () => undefined), false);
  let replacement: Promise<void> | undefined;
  await assert.rejects(sourcePool.enqueueFromSource(() => {
    replacement = sourcePool.initialize(new ArrayBuffer(0), 1, undefined, 'factory-replaced-generation');
    return Uint8Array.of(2).buffer;
  }, () => { applied += 1; }), /cancelled/);
  await replacement;
  assert.equal(applied, 0, 'a stale factory does not publish into the new generation');
  await sourcePool.enqueueFromSource(() => Uint8Array.of(3).buffer, () => { applied += 1; });
  await sourcePool.waitForIdle();
  await sourcePool.finish((partial) => {
    assert.equal(new Uint8Array(partial)[0], 3, 'the stale task was never executed');
    applied += 1;
  });
  assert.equal(applied, 1, 'stale cleanup does not release or poison the replacement lease');
  sourcePool.cancel();
}

{
  const profile = new VerifierTransportProfile();
  const before = profile.start('prepare', 'consume'); before();
  assert.deepEqual(profile.finish().timings, {}, 'profiling is disabled unless requested');
  profile.begin();
  const stale = profile.start('run_grant_to_reply', 'consume');
  profile.begin(); stale();
  assert.deepEqual(profile.finish().timings, {}, 'a previous run cannot append timings to its replacement');
  profile.begin();
  await assert.rejects(profile.measure('prepare', 'consume', async () => { throw new Error('measured failure'); }), /measured failure/);
  const failure = profile.finish().timings['consume.prepare'];
  assert.equal(failure.count, 1); assert.equal(failure.failed, 1);
}

for (const size of [1, 3, 11]) {
  const profiled = new ClearraVerifierPool(() => new FakeVerifierWorker(false) as unknown as Worker);
  profiled.beginTransportProfile();
  await profiled.initialize(new ArrayBuffer(0), size, undefined, `profiled-${size}`);
  await Promise.all(Array.from({ length: size }, (_, index) =>
    profiled.enqueue(Uint8Array.of(index).buffer, () => undefined)));
  await profiled.finish(() => undefined);
  const report = profiled.finishTransportProfile();
  for (const operation of ['initialize', 'consume', 'finish']) {
    for (const stage of ['prepare', 'offered', 'offer_round_trip', 'accepted', 'published',
      'posted_to_start_notice', 'running_commit', 'run_grant_to_reply', 'result_hash', 'result_sealed',
      'result_applied', 'completed']) {
      const summary = report.timings[`${operation}.${stage}`];
      assert.equal(summary.count, size, `one ${operation}.${stage} per admitted executor`);
      assert.equal(summary.failed, 0);
      assert.ok(summary.total_ms >= 0 && summary.max_ms >= 0);
    }
  }
  assert.equal(report.timings['initialize.prewarm_new'].count, size);
  profiled.beginTransportProfile();
  await profiled.initialize(new ArrayBuffer(0), size, undefined, `profiled-${size}`);
  assert.equal(profiled.finishTransportProfile().timings['initialize.prewarm_reuse'].count, size);
  profiled.cancel();
}

class StalledConsumeVerifierWorker extends FakeVerifierWorker {
  constructor(private readonly stallConsume: boolean) {
    super(false);
  }

  override postMessage(message: WorkerMessage) {
    if (message.type === 'consume' && this.stallConsume) return;
    super.postMessage(message);
  }
}

const stalledConsumeWorkers: StalledConsumeVerifierWorker[] = [];
const stalledConsumePool = new ClearraVerifierPool(() => {
  const worker = new StalledConsumeVerifierWorker(stalledConsumeWorkers.length === 0);
  stalledConsumeWorkers.push(worker);
  return worker as unknown as Worker;
}, {
  requestStallTimeoutMs: 25
});
await bounded(
  'stalled consume initialize',
  stalledConsumePool.initialize(
    new ArrayBuffer(0),
    1,
    undefined,
    'stalled-consume-owner',
    'replay-state'
  )
);
await bounded(
  'stalled consume enqueue',
  stalledConsumePool.enqueue(Uint8Array.of(5).buffer, () => undefined)
);
await assert.rejects(
  bounded('stalled consume fail closed', stalledConsumePool.waitForIdle()),
  /stalled/
);
assert.equal(stalledConsumeWorkers.length, 1);
assert.equal(stalledConsumeWorkers[0].terminated, true);

class StalledFinishVerifierWorker extends FakeVerifierWorker {
  constructor(private readonly stallFinish: boolean) {
    super(false);
  }

  override postMessage(message: WorkerMessage) {
    if (message.type === 'finish' && this.stallFinish) return;
    super.postMessage(message);
  }
}

const stalledFinishWorkers: StalledFinishVerifierWorker[] = [];
const stalledFinishPool = new ClearraVerifierPool(() => {
  const worker = new StalledFinishVerifierWorker(stalledFinishWorkers.length === 0);
  stalledFinishWorkers.push(worker);
  return worker as unknown as Worker;
}, {
  requestStallTimeoutMs: 25,
  finishStallTimeoutMs: 25
});
await bounded(
  'stalled finish initialize',
  stalledFinishPool.initialize(
    new ArrayBuffer(0),
    1,
    undefined,
    'stalled-finish-owner',
    'replay-state'
  )
);
await bounded(
  'stalled finish enqueue',
  stalledFinishPool.enqueue(Uint8Array.of(6).buffer, () => undefined)
);
await bounded('stalled finish idle', stalledFinishPool.waitForIdle());
const stalledFinishPartials: number[] = [];
await assert.rejects(
  bounded(
    'stalled finish fail closed',
    stalledFinishPool.finish((partial) =>
      stalledFinishPartials.push(new Uint8Array(partial)[0])
    )
  ),
  /stalled/
);
assert.equal(stalledFinishWorkers.length, 1);
assert.equal(stalledFinishWorkers[0].terminated, true);
assert.deepEqual(stalledFinishPartials, []);

class SaturatedTelemetryVerifierWorker extends FakeVerifierWorker {
  constructor(private readonly saturated: boolean) {
    super(false);
  }

  protected override handleExecutable(message: WorkerMessage) {
    if (message.type !== 'consume' || !this.saturated) {
      super.handleExecutable(message);
      return;
    }
    this.emit({
      type: 'consumed',
      requestId: message.requestId,
      candidateCount: 0xffff_ffff,
      candidateCountAvailable: false,
      candidateCountExact: false,
      partial: null,
      progress: {
        candidateCount: 0xffff_ffff,
        buildNodes: 0xffff_ffff,
        coverageChecks: 0xffff_ffff,
        availability: {
          candidateCount: false,
          buildNodes: false,
          coverageChecks: false
        },
        exactness: {
          candidateCount: false,
          buildNodes: false,
          coverageChecks: false
        }
      }
    });
  }
}

const saturatedTelemetryWorkers: SaturatedTelemetryVerifierWorker[] = [];
const saturatedTelemetryPool = new ClearraVerifierPool(() => {
  const worker = new SaturatedTelemetryVerifierWorker(
    saturatedTelemetryWorkers.length === 0
  );
  saturatedTelemetryWorkers.push(worker);
  return worker as unknown as Worker;
});
await bounded(
  'saturated telemetry initialize',
  saturatedTelemetryPool.initialize(
    new ArrayBuffer(0),
    2,
    undefined,
    'saturated-telemetry-owner',
    'atomic-task'
  )
);
await Promise.all([
  bounded(
    'saturated telemetry first enqueue',
    saturatedTelemetryPool.enqueue(Uint8Array.of(1).buffer, () => undefined)
  ),
  bounded(
    'saturated telemetry second enqueue',
    saturatedTelemetryPool.enqueue(Uint8Array.of(2).buffer, () => undefined)
  )
]);
await bounded('saturated telemetry idle', saturatedTelemetryPool.waitForIdle());
const saturatedTelemetrySnapshot = saturatedTelemetryPool.progressSnapshot();
assert.equal(saturatedTelemetrySnapshot.candidatesVerified, 0x1_0000_0000);
assert.equal(saturatedTelemetrySnapshot.buildNodes, 0xffff_ffff);
assert.equal(saturatedTelemetrySnapshot.coverageChecks, 0xffff_ffff);
assert.deepEqual(saturatedTelemetrySnapshot.availability, {
  candidatesVerified: false,
  buildNodes: false,
  coverageChecks: false
});
assert.deepEqual(saturatedTelemetrySnapshot.exactness, {
  candidatesVerified: false,
  buildNodes: false,
  coverageChecks: false
});
await bounded(
  'saturated telemetry finish',
  saturatedTelemetryPool.finish(() => undefined)
);
