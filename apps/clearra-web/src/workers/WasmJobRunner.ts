import type { ClearraWasmWorkerEvent } from '@clearra/ui/wasm';

import type { ClearraWasmModule, Pc4RangeRequest } from './clearraWasmRuntime';
import { Pc4AsyncRangePump } from './Pc4AsyncRangePump';
import { onlinePc4Progress } from './OnlinePc4Progress';
import { openLocalPc4Reader } from './pc4LocalStore';
import { pc4SearchRangePolicy } from '../../../../scripts/release/pc4/pc4-search-range-policy.mjs';
import { prefetchPc4LookupFrontier, PC4_FRONTIER_MAX_GAP_BYTES } from '../../../../scripts/release/pc4/pc4-frontier-reader.mjs';
import { createPc4RangeReader, type Pc4HostGeneration } from '../../../../scripts/release/pc4/qualify-upstream-generation.mjs';

// Keep one synchronous WASM entry comfortably below the browser host turn.
// Canonical empty-board 4L minimals profiling measured a 2,048-step serial
// slice below 1 ms on native debug builds versus about 8 ms at 32,768 steps.
const SEARCH_WORK_BUDGET = 2_048;
const EVENT_DRAIN_INTERVAL = 8;
const HOST_YIELD_BUDGET_MS = 8;
const yieldToWorkerHost = createWorkerHostYield();

export class WasmJobRunner {
  private active = false;
  private jobId: number | null = null;
  private cancellationRequested = false;
  private onlineAbort: AbortController | null = null;

  constructor(private readonly wasm: ClearraWasmModule, private readonly onlineGeneration?: Pc4HostGeneration) {}

  async run(
    commandText: string,
    onEvent: (event: ClearraWasmWorkerEvent) => void
  ): Promise<ClearraWasmWorkerEvent> {
    let profilingActive = false;
    if (this.wasm.profile_start) {
      this.wasm.profile_start();
      profilingActive = true;
    }
    let terminal: ClearraWasmWorkerEvent | null = null;
    let advancesSinceDrain = 0;
    let searchProfile: unknown = null;
    let lastOnlineProgress = 0;
    const onlineStarted = performance.now();
    this.onlineAbort = this.onlineGeneration ? new AbortController() : null;
    let reader: ReturnType<typeof createPc4RangeReader> | Awaited<ReturnType<typeof openLocalPc4Reader>> = null;
    let rangePump: Pc4AsyncRangePump | null = null;
    const admit = (range: Pc4RangeRequest, bytes: Uint8Array) => {
      if (this.cancellationRequested || this.jobId === null || !reader) return;
      // Local data has its own admission kind, never a fabricated HTTP
      // status/header. The HTTP reader already checks the real 206 envelope.
      this.wasm.online_pc4_admit!(this.jobId, { lookup_session: range.lookup_session, request_id: range.request_id,
        ...('provider' in reader ? { source: 'verified-local-file' } : {
          status: 206, content_range: `bytes ${range.offset}-${range.offset + range.length - 1}/${range.artifact.byte_length}` }),
        bytes: Array.from(bytes) });
    };
    const emit = (event: ClearraWasmWorkerEvent) => onEvent(reader ? ({ ...event,
      pc4_online: { provider: 'provider' in reader ? reader.provider : 'hf-graph', profile: 'jstris-180', revision: this.onlineGeneration!.revision,
        requests: reader.requests, transferred_bytes: reader.bytes, logical_reads: reader.reads,
        local_bytes: 'localBytes' in reader ? reader.localBytes : 0,
        local_file_reads: 'fileReads' in reader ? reader.fileReads : 0,
        ...('fileAccess' in reader ? { local_file_access: reader.fileAccess } : {}),
        cache_hits: reader.cacheHits, joined_requests: reader.joinedRequests, cache_bytes: reader.retainedBytes,
        elapsed_ms: performance.now() - onlineStarted } } as ClearraWasmWorkerEvent) : event);
    try {
      if (this.onlineGeneration) {
        reader = await openLocalPc4Reader(this.onlineGeneration, this.onlineAbort!.signal)
          ?? createPc4RangeReader(this.onlineGeneration, {
            signal: this.onlineAbort!.signal, ...pc4SearchRangePolicy(this.onlineGeneration, 'jstris-180')
          });
        const openedReader = reader;
        rangePump = new Pc4AsyncRangePump(range => openedReader.read(range.artifact, range.offset, range.length),
          this.onlineAbort!.signal, 'provider' in openedReader ? undefined : PC4_FRONTIER_MAX_GAP_BYTES);
      }
      this.jobId = this.wasm.start_job(commandText);
      this.active = true;
      this.cancellationRequested = false;
      this.drain(emit, (event) => {
        terminal = event;
      });
      let lastHostYield = performance.now();
      while (this.active && terminal === null) {
        if (!this.active || this.jobId === null) break;
        let status: ReturnType<ClearraWasmModule['advance_job']> = 'pending';
        if (reader && performance.now() - lastOnlineProgress >= 200) {
          emit(onlinePc4Progress(this.jobId, reader.requests));
          lastOnlineProgress = performance.now();
        }
        if (!this.cancellationRequested) {
          rangePump?.drain(admit);
          status = this.wasm.advance_job(this.jobId, SEARCH_WORK_BUDGET);
          advancesSinceDrain += 1;
          if (reader && (status === 'pending' || status === 'progress')) {
            const range = this.wasm.online_pc4_pending?.(this.jobId);
            if (range?.batch && !range.lookup_frontier?.length) {
              rangePump!.submit(range.batch);
              // Advance ready CPU work while I/O is outstanding. When only
              // responses can unblock it, wait for the FIRST response, not
              // for all requests or a fixed polling interval.
              if (!range.can_advance) await rangePump!.waitForAny();
            } else if (range) {
              let bytes: Uint8Array;
              try {
                // Local storage keeps its measured index-page/exact-record
                // policy. Read-ahead groups known demands only for HTTP RTTs.
                if (!('provider' in reader)) await prefetchPc4LookupFrontier(reader, this.onlineGeneration!, range);
                bytes = await reader.read(range.artifact, range.offset, range.length);
              }
              catch (error) {
                // Cancellation already has a terminal event waiting in Rust.
                // Do not turn an aborted HTTP request into a generic failure.
                if (this.cancellationRequested) continue;
                throw error;
              }
              if (this.cancellationRequested) continue;
              admit(range, bytes);
            }
          }
        }
        const terminalStatus = status !== 'pending' && status !== 'progress';
        if (
          this.cancellationRequested ||
          status === 'progress' ||
          terminalStatus ||
          advancesSinceDrain >= EVENT_DRAIN_INTERVAL
        ) {
          if (terminalStatus && profilingActive && this.wasm.profile_finish) {
            searchProfile = this.wasm.profile_finish();
            profilingActive = false;
          }
          this.drain(emit, (event) => {
            terminal = event;
          }, searchProfile);
          advancesSinceDrain = 0;
        }
        const mustPublishProgress = status === 'progress';
        const mustYieldForCancellation = this.cancellationRequested;
        const hostTimeBudgetExpired =
          performance.now() - lastHostYield >= HOST_YIELD_BUDGET_MS;
        if (
          terminal === null &&
          (status === 'pending' || mustPublishProgress) &&
          (mustYieldForCancellation || mustPublishProgress || hostTimeBudgetExpired)
        ) {
          await yieldToWorkerHost();
          lastHostYield = performance.now();
        }
      }
      if (terminal === null) {
        throw new Error('WASM job stopped without a terminal event');
      }
      return terminal;
    } finally {
      // Abort before waiting, then drain in-flight reads before releasing a
      // local generation/file lease. No callback may admit after job release.
      this.onlineAbort?.abort();
      try { await rangePump?.dispose(); }
      finally {
        try { await reader?.dispose(); }
        finally {
          this.onlineAbort = null;
          if (profilingActive && this.wasm.profile_finish) {
            try {
              this.wasm.profile_finish();
            } catch {
              // The worker owner will terminate a failed runtime; cleanup must not mask the failure.
            }
          }
          if (terminal === null) this.releaseActiveJob();
          else {
            this.active = false;
            this.jobId = null;
          }
        }
      }
    }
  }

  cancel() {
    if (!this.active || this.jobId === null) return;
    this.cancellationRequested = true;
    this.onlineAbort?.abort();
    try {
      this.wasm.cancel_job(this.jobId);
    } catch {
      this.active = false;
    }
  }

  dispose() {
    this.releaseActiveJob();
  }

  private drain(
    onEvent: (event: ClearraWasmWorkerEvent) => void,
    onTerminal: (event: ClearraWasmWorkerEvent) => void,
    searchProfile: unknown = null
  ) {
    if (this.jobId === null) return;
    const events = JSON.parse(this.wasm.drain_job_events_json(this.jobId)) as unknown;
    if (!Array.isArray(events)) {
      throw new Error('clearra-wasm returned a non-array event payload');
    }
    for (const event of events as ClearraWasmWorkerEvent[]) {
      const terminal =
        event.event === 'final_response' ||
        event.event === 'failed' ||
        event.event === 'cancelled' ||
        event.event === 'terminated';
      const emittedEvent = terminal ? withSearchProfile(event, searchProfile) : event;
      onEvent(emittedEvent);
      if (terminal) {
        onTerminal(emittedEvent);
      }
    }
  }

  private releaseActiveJob() {
    const jobId = this.jobId;
    this.active = false;
    this.cancellationRequested = true;
    this.onlineAbort?.abort();
    this.jobId = null;
    if (jobId === null) return;
    try {
      this.wasm.cancel_job(jobId);
    } catch {
      // A terminal Rust job has already released its scope.
    }
    try {
      this.wasm.drain_job_events_json(jobId);
    } catch {
      // Worker termination remains the final ownership boundary after a trap.
    }
  }
}

function withSearchProfile(
  event: ClearraWasmWorkerEvent,
  searchProfile: unknown
): ClearraWasmWorkerEvent {
  if (
    searchProfile === null ||
    (event.event !== 'final_response' && event.event !== 'failed')
  ) {
    return event;
  }
  return { ...event, search_profile: searchProfile } as unknown as ClearraWasmWorkerEvent;
}

function createWorkerHostYield(): () => Promise<void> {
  const channel = new MessageChannel();
  const nodePort1 = channel.port1 as MessagePort & { ref?: () => void; unref?: () => void };
  const nodePort2 = channel.port2 as MessagePort & { unref?: () => void };
  const pending: Array<() => void> = [];
  channel.port1.onmessage = () => {
    const resolve = pending.shift();
    if (pending.length === 0) nodePort1.unref?.();
    resolve?.();
  };
  nodePort1.unref?.();
  nodePort2.unref?.();
  return () =>
    new Promise<void>((resolve) => {
      // Node contract runners have no browser host to keep the event loop
      // alive. Only an outstanding yield owns a ref; idle modules still exit.
      if (pending.length === 0) nodePort1.ref?.();
      pending.push(resolve);
      channel.port2.postMessage(undefined);
    });
}
