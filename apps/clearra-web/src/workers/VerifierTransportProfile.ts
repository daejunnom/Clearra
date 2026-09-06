// SRP: bounded, opt-in timing summaries only; never an execution authority.
export type TransportOperation = 'initialize' | 'consume' | 'finish';
export type TransportStage = 'ready_client_wait' | 'prewarm_new' | 'prewarm_reuse' |
  'payload_hash' | 'prepare' | 'offered' | 'offer_round_trip' | 'accepted' | 'published' |
  'posted_to_start_notice' | 'running_commit' | 'run_grant_to_reply' | 'result_hash' |
  'result_sealed' | 'result_apply' | 'result_applied' | 'completed';
type Summary = { count: number; failed: number; total_ms: number; max_ms: number };
const noop = () => undefined;

export class VerifierTransportProfile {
  private generation = 0;
  private active = false;
  private summaries = new Map<string, Summary>();
  begin() { this.generation += 1; this.active = true; this.summaries.clear(); }
  start(stage: TransportStage, operation?: TransportOperation): (failed?: boolean) => void {
    if (!this.active) return noop;
    const generation = this.generation;
    const started = performance.now();
    const name = operation ? `${operation}.${stage}` : stage;
    let ended = false;
    return (failed = false) => {
      if (ended || !this.active || generation !== this.generation) return;
      ended = true;
      const elapsed = performance.now() - started;
      const summary = this.summaries.get(name) ?? { count: 0, failed: 0, total_ms: 0, max_ms: 0 };
      summary.count += 1; summary.failed += Number(failed); summary.total_ms += elapsed;
      summary.max_ms = Math.max(summary.max_ms, elapsed);
      this.summaries.set(name, summary);
    };
  }
  measure<T>(stage: TransportStage, operation: TransportOperation | undefined, run: () => Promise<T>): Promise<T> {
    if (!this.active) return run();
    const end = this.start(stage, operation);
    try {
      return run().then((result) => { end(); return result; }, (error) => { end(true); throw error; });
    } catch (error) { end(true); throw error; }
  }
  finish() {
    this.active = false;
    return {
      schema: 'clearra.verifier-transport-profile.v1',
      semantics: 'completed intervals; totals overlap across workers and are not job elapsed time',
      timings: Object.fromEntries(this.summaries)
    };
  }
}
