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
