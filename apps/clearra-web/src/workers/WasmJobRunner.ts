import type { ClearraWasmWorkerEvent } from '@clearra/ui/wasm';

import type { ClearraWasmModule } from './clearraWasmRuntime';

const SEARCH_WORK_BUDGET = 32768;
const EVENT_DRAIN_INTERVAL = 8;
const HOST_YIELD_INTERVAL = 1;
const yieldToWorkerHost = createWorkerHostYield();

export class WasmJobRunner {
  private active = false;
  private jobId: number | null = null;
  private cancellationRequested = false;

  constructor(private readonly wasm: ClearraWasmModule) {}

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
    let advancesSinceYield = 0;
    let searchProfile: unknown = null;
    try {
      this.jobId = this.wasm.start_job(commandText);
      this.active = true;
      this.cancellationRequested = false;
      this.drain(onEvent, (event) => {
        terminal = event;
      });
      while (this.active && terminal === null) {
        if (!this.active || this.jobId === null) break;
        let status: ReturnType<ClearraWasmModule['advance_job']> = 'pending';
        if (!this.cancellationRequested) {
          status = this.wasm.advance_job(this.jobId, SEARCH_WORK_BUDGET);
          advancesSinceDrain += 1;
          advancesSinceYield += 1;
        }
        if (
          this.cancellationRequested ||
          status !== 'pending' ||
          advancesSinceDrain >= EVENT_DRAIN_INTERVAL
        ) {
          if (status !== 'pending' && profilingActive && this.wasm.profile_finish) {
            searchProfile = this.wasm.profile_finish();
            profilingActive = false;
          }
          this.drain(onEvent, (event) => {
            terminal = event;
          }, searchProfile);
          advancesSinceDrain = 0;
        }
        if (
          terminal === null &&
          status === 'pending' &&
          advancesSinceYield >= HOST_YIELD_INTERVAL
        ) {
          await yieldToWorkerHost();
          advancesSinceYield = 0;
        }
      }
      if (terminal === null) {
        throw new Error('WASM job stopped without a terminal event');
      }
      return terminal;
    } finally {
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

  cancel() {
    if (!this.active || this.jobId === null) return;
    this.cancellationRequested = true;
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
        event.event === 'cancelled';
      const emittedEvent =
        terminal && searchProfile !== null
          ? ({ ...event, search_profile: searchProfile } as ClearraWasmWorkerEvent)
          : event;
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

function createWorkerHostYield(): () => Promise<void> {
  const channel = new MessageChannel();
  const pending: Array<() => void> = [];
  channel.port1.onmessage = () => pending.shift()?.();
  return () =>
    new Promise<void>((resolve) => {
      pending.push(resolve);
      channel.port2.postMessage(undefined);
    });
}
