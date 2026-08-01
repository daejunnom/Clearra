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

  private emit(data: unknown) {
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
