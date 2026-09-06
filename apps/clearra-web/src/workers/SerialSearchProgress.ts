import type {
  ClearraSearchProgressTelemetry,
  ClearraWasmWorkerEvent
} from '@clearra/ui/wasm';

type ProgressEvent = Extract<ClearraWasmWorkerEvent, { event: 'progress' }>;
type SerialCounter = 'geometry_nodes' | 'candidates_emitted' | 'candidates_verified' | 'build_nodes';

// Adapt the existing stage/count protocol without leaking browser telemetry
// types into the core execution contract. One instance belongs to one job.
export class SerialSearchProgress {
  private telemetry: ClearraSearchProgressTelemetry;

  constructor(readonly initialEvent: ProgressEvent) {
    if (!initialEvent.progress.telemetry) {
      throw new Error('serial progress requires an initial execution snapshot');
    }
    this.telemetry = initialEvent.progress.telemetry;
  }

  project(event: ClearraWasmWorkerEvent): ClearraWasmWorkerEvent {
    if (event.event !== 'progress' || event.progress.telemetry) return event;
    const { label, done } = event.progress;
    const keys: SerialCounter[] = label === 'build-geometry'
      ? ['geometry_nodes']
      : label === 'build-candidates'
        ? ['candidates_emitted', 'candidates_verified']
        : label === 'build-verification'
          ? ['build_nodes']
          : [];
    const finalizing = label === 'postprocess' || label === 'pc-minimals-finalize'
      || label?.startsWith('complete-replay-') === true;
    if (keys.length === 0 && !finalizing) return event;

    const next: ClearraSearchProgressTelemetry = {
      ...this.telemetry,
      availability: { ...this.telemetry.availability },
      exactness: { ...this.telemetry.exactness }
    };
    for (const key of keys) {
      if (!Number.isSafeInteger(done) || done < 0) continue;
      // Rust's compatibility JobProgress saturates at u32::MAX. A saturated
      // metric is still useful, but must not be advertised as an exact count.
      next[key] = done;
      next.availability[key] = true;
      next.exactness[key] = done < 0xffff_ffff;
    }
    if (finalizing) {
      next.phase = 'postprocessing';
      next.producer_complete = true;
    }
    this.telemetry = next;
    return { ...event, progress: { ...event.progress, telemetry: next } };
  }
}
