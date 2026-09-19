// SRP: bounded host I/O continuations. No graph semantics, WASM calls, fallback
// or HTTP validation here; the existing reader and Rust admission own those.
import type { Pc4RangeRequest } from './clearraWasmRuntime';
import { checkedPc4Read, planPc4ReadBatch } from '../../../../scripts/release/pc4/pc4-range-plan.mjs';

const MAX_PENDING = 16;
const MAX_RANGE_BYTES = 65_536;
type Entry = { range: Pc4RangeRequest; identity: string; task: Promise<void> };
type Settled = { key: string; range: Pc4RangeRequest } & (
  { bytes: Uint8Array; error?: never } | { error: unknown; bytes?: never }
);
const keyOf = (range: Pc4RangeRequest) => `${range.lookup_session}:${range.request_id}`;
function fail(code: string): never { throw Object.assign(new Error(code), { code }); }

export class Pc4AsyncRangePump {
  private entries = new Map<string, Entry>();
  private settled: Settled[] = [];
  private wake: (() => void) | null = null;
  private closed = false;
  private readonly onAbort = () => this.notify();

  constructor(
    private readonly read: (range: Pc4RangeRequest) => Promise<Uint8Array>,
    private readonly signal: AbortSignal,
    private readonly maxGapBytes?: number
  ) { signal.addEventListener('abort', this.onAbort, { once: true }); }

  submit(ranges: readonly Pc4RangeRequest[]) {
    if (this.closed || this.signal.aborted) return;
    if (ranges.length === 0 || ranges.length > MAX_PENDING) fail('pc4_online_pending_limit');
    // Validate the entire snapshot before starting any of its new I/O. Retained
    // responses occupy a slot until admission, bounding both tasks and bytes.
    const checked = new Map<string, { range: Pc4RangeRequest; identity: string }>();
    for (const range of ranges) {
      if (!Number.isSafeInteger(range.lookup_session) || range.lookup_session < 1 ||
          !Number.isSafeInteger(range.request_id) || range.request_id < 1 ||
          !Number.isSafeInteger(range.length) || range.length < 1 || range.length > MAX_RANGE_BYTES) {
        fail('pc4_online_pending_invalid');
      }
      checkedPc4Read(range.artifact, range.offset, range.length);
      const key = keyOf(range);
      const identity = JSON.stringify([range.profile, range.offset, range.length,
        range.artifact.path, range.artifact.byte_length, range.artifact.content_identity]);
      const prior = checked.get(key) ?? this.entries.get(key);
      if (prior && prior.identity !== identity) fail('pc4_online_pending_identity_changed');
      checked.set(key, { range: { ...range, artifact: { ...range.artifact } }, identity });
    }
    const fresh = [...checked].filter(([key]) => !this.entries.has(key));
    if (this.entries.size + fresh.length > MAX_PENDING) fail('pc4_online_pending_limit');
    // Group only demands already available NOW. Never wait for another batch
    // to fill; never cross a profile/immutable artifact boundary. Local files
    // keep their existing page/exact policy without network-oriented gaps.
    const profiles = new Map<string, number[]>();
    for (let index = 0; index < fresh.length; index++) {
      const profile = fresh[index][1].range.profile;
      if (!profiles.has(profile)) profiles.set(profile, []);
      profiles.get(profile)!.push(index);
    }
    const plan = [...profiles.values()].flatMap(indices => {
      if (this.maxGapBytes === undefined) return indices.map(index => ({
        range: fresh[index][1].range, indices: [index]
      }));
      return planPc4ReadBatch(indices.map(index => fresh[index][1].range), { maxGapBytes: this.maxGapBytes })
        .map(span => ({ range: { ...fresh[indices[span.demands[0].index]][1].range,
          artifact: span.artifact, offset: span.offset, length: span.length },
        indices: span.demands.map(demand => indices[demand.index]) }));
    });
    for (const { range: request, indices } of plan) {
      const task = (async () => {
        if (this.closed || this.signal.aborted) return;
        return this.read(request);
      })().then(bytes => {
        if (!this.closed && !this.signal.aborted && bytes) {
          if (bytes.byteLength !== request.length) fail('pc4_online_range_length_mismatch');
          for (const index of indices) {
            const [key, { range }] = fresh[index];
            const start = range.offset - request.offset;
            this.settled.push({ key, range, bytes: bytes.slice(start, start + range.length) });
          }
        }
      }).catch(error => {
        if (!this.closed && !this.signal.aborted) {
          const [key, { range }] = fresh[indices[0]];
          this.settled.push({ key, range, error });
        }
      }).then(() => this.notify());
      for (const index of indices) {
        const [key, { range, identity }] = fresh[index];
        this.entries.set(key, { range, identity, task });
      }
    }
  }

  drain(admit: (range: Pc4RangeRequest, bytes: Uint8Array) => void) {
    if (this.closed || this.signal.aborted) return;
    while (this.settled.length) {
      const result = this.settled.shift()!;
      // No partial success is published on a transport failure. Other tasks
      // remain owned until the caller aborts and drains them in finally.
      if ('error' in result) throw result.error;
      admit(result.range, result.bytes!);
      this.entries.delete(result.key);
    }
  }

  async waitForAny() {
    if (this.closed || this.signal.aborted || this.settled.length) return;
    if (this.entries.size === 0) fail('pc4_online_pending_missing');
    // Exactly one host wait, no accumulating Promise.race handlers on a slow
    // first request. Completion order never waits for an entire batch.
    await new Promise<void>(resolve => { this.wake = resolve; });
  }

  async dispose() {
    this.closed = true;
    this.notify();
    this.signal.removeEventListener('abort', this.onAbort);
    await Promise.all([...this.entries.values()].map(entry => entry.task));
    this.entries.clear();
    this.settled = [];
  }

  private notify() { const wake = this.wake; this.wake = null; wake?.(); }
}
