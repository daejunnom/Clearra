import type {
  ClearraSearchProgressTelemetry,
  ClearraWasmWorkerEvent
} from '@clearra/ui/wasm';
import { mergeNativeSearchProgress } from '@clearra/ui/search-progress';

type ProgressEvent = Extract<ClearraWasmWorkerEvent, { event: 'progress' }>;

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
    const { label, done, total } = event.progress;
    let next = mergeNativeSearchProgress(this.telemetry, label, done, total);
    if (!next) return event;
    if (label === 'build-candidates' && next !== this.telemetry) {
      // The serial producer verifies each candidate before returning. A
      // native parallel producer instead reports its own dispatched counter.
      next = {
        ...next,
        candidates_verified: next.candidates_emitted,
        availability: { ...next.availability, candidates_verified: true },
        exactness: { ...next.exactness, candidates_verified: next.exactness.candidates_emitted }
      };
    }
    this.telemetry = next;
    return { ...event, progress: { ...event.progress, telemetry: next } };
  }
}
