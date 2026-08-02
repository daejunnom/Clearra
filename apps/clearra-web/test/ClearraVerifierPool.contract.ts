import assert from 'node:assert/strict';

import { ClearraVerifierPool } from '../src/workers/ClearraVerifierPool.ts';

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
};

class FakeVerifierWorker {
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  onmessageerror: (() => void) | null = null;
  private listeners = new Map<string, Set<(event: MessageEvent) => void>>();
  private consumed: number[] = [];
  private consumeCount = 0;
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
    if (message.type === 'prewarm') {
      this.emit({ type: 'prewarmed' });
      return;
    }
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
        partial: null,
        progress: { candidateCount: this.consumed.length, buildNodes: 0, coverageChecks: 0 }
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
await bounded('replay retry idle', pool.waitForIdle());
const partials: number[] = [];
await bounded(
  'replay finish',
  pool.finish((partial) => partials.push(new Uint8Array(partial)[0]))
);

assert.equal(workers.length, 2);
assert.equal(workers[0].terminated, true);
assert.deepEqual(partials, [3]);

class StreamingVerifierWorker {
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  onmessageerror: (() => void) | null = null;
  private listeners = new Map<string, Set<(event: MessageEvent) => void>>();
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
    if (message.type === 'prewarm') {
      this.emit({ type: 'prewarmed' });
      return;
    }
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
        partial: null,
        progress: { candidateCount: 1, buildNodes: 0, coverageChecks: 0 }
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
await bounded('streaming idle', streamingPool.waitForIdle());
await bounded('streaming finish', streamingPool.finish(() => undefined));

assert.equal(streamingWorkers.length, 2);
assert.equal(streamingWorkers[0].terminated, true);
assert.deepEqual(streamed, [7, 7]);

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
        progress: { candidateCount: 0, buildNodes: 1, coverageChecks: 2 }
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
await bounded('stalled consume recovery', stalledConsumePool.waitForIdle());
const stalledConsumePartials: number[] = [];
await bounded(
  'stalled consume finish',
  stalledConsumePool.finish((partial) =>
    stalledConsumePartials.push(new Uint8Array(partial)[0])
  )
);
assert.equal(stalledConsumeWorkers.length, 2);
assert.equal(stalledConsumeWorkers[0].terminated, true);
assert.deepEqual(stalledConsumePartials, [5]);

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
await bounded(
  'stalled finish recovery',
  stalledFinishPool.finish((partial) =>
    stalledFinishPartials.push(new Uint8Array(partial)[0])
  )
);
assert.equal(stalledFinishWorkers.length, 2);
assert.equal(stalledFinishWorkers[0].terminated, true);
assert.deepEqual(stalledFinishPartials, [6]);
