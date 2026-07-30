import type { ClearraWasmWorkerEvent } from '@clearra/ui/wasm';

import { DistributedWasmJobRunner } from './DistributedWasmJobRunner';
import { WasmJobRunner } from './WasmJobRunner';
import type { ClearraWasmModule } from './clearraWasmRuntime';

export class ClearraProductJobRunner {
  private activeRunner: WasmJobRunner | DistributedWasmJobRunner | null = null;

  constructor(
    private readonly wasm: ClearraWasmModule,
    private readonly jobId: number,
    private readonly lifecycleOwnerId: string
  ) {}

  async run(
    commandText: string,
    onEvent: (event: ClearraWasmWorkerEvent) => void
  ): Promise<ClearraWasmWorkerEvent> {
    const distributed = new DistributedWasmJobRunner(
      this.wasm,
      this.jobId,
      this.lifecycleOwnerId
    );
    try {
      onEvent(preparationProgressEvent(this.jobId));
      const plan = distributed.prepare(commandText);
      if (plan.mode !== 'serial') {
        this.activeRunner = distributed;
        return await distributed.run(commandText, plan, onEvent);
      }
      this.wasm.distributed_reset();
      const serial = new WasmJobRunner(this.wasm);
      this.activeRunner = serial;
      return await serial.run(commandText, onEvent);
    } catch (error) {
      this.dispose();
      distributed.dispose();
      throw error;
    } finally {
      this.activeRunner = null;
    }
  }

  cancel() {
    this.activeRunner?.cancel();
  }

  dispose() {
    this.activeRunner?.dispose();
    this.activeRunner = null;
    try {
      this.wasm.distributed_reset();
    } catch {
      // Main worker termination releases the entire runtime after an abnormal exit.
    }
  }
}

function preparationProgressEvent(jobId: number): ClearraWasmWorkerEvent {
  return {
    schema_version: 1,
    runtime: 'clearra-wasm',
    event: 'progress',
    job_id: jobId,
    progress: {
      done: 0,
      total: 5,
      label: 'Search catalog preparing',
      budget_status: { state: 'within-budget', used: 0, limit: null },
      backend_status: {
        backend_requested: 'pending',
        backend_selected: 'pending',
        fallback_used: false,
        fallback_reason: null
      },
      memory_status: {
        state: 'wasm-computation-scope-active',
        raw_pointer_exposed: false
      },
      telemetry: {
        phase: 'preparing',
        producer_complete: false,
        geometry_nodes: 0,
        candidates_emitted: 0,
        geometry_family_count: null,
        candidates_verified: 0,
        producer_build_nodes: 0,
        producer_coverage_checks: 0,
        build_nodes: 0,
        coverage_checks: 0,
        ready_workers: 0,
        active_workers: 0,
        worker_count: 0,
        oldest_batch_ms: 0,
        pass_index: 0,
        pass_count: 1,
        layer_index: 0,
        layer_count: 0,
        layer_done: 0,
        layer_total: 0
      }
    }
  };
}
