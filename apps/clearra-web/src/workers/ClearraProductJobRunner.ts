import type {
  ClearraSearchProgressTelemetryFlags,
  ClearraWasmWorkerEvent
} from '@clearra/ui/wasm';

import { DistributedWasmJobRunner } from './DistributedWasmJobRunner';
import { withHostExecutionTiming } from './HostExecutionProfile';
import { SerialSearchProgress } from './SerialSearchProgress';
import type { SharedExecutionResourceAuthority } from './SharedExecutionResourceAuthority';
import { WasmJobRunner } from './WasmJobRunner';
import type {
  ClearraWasmHostCapabilities,
  ClearraWasmModule
} from './clearraWasmRuntime';

export class ClearraProductJobRunner {
  private activeRunner: WasmJobRunner | DistributedWasmJobRunner | null = null;

  constructor(
    private readonly wasm: ClearraWasmModule,
    private readonly jobId: number,
    private readonly lifecycleOwnerId: string,
    private readonly hostCapabilities: ClearraWasmHostCapabilities,
    private readonly resourceAuthority?: SharedExecutionResourceAuthority,
    private readonly resourceWaitTimeoutMs?: number
  ) {}

  async run(
    commandText: string,
    onEvent: (event: ClearraWasmWorkerEvent) => void,
    options: { transportProfile?: boolean } = {}
  ): Promise<ClearraWasmWorkerEvent> {
    const preparationStarted = options.transportProfile ? performance.now() : null;
    let preparationMs = 0;
    const emit = (event: ClearraWasmWorkerEvent) => onEvent(withHostExecutionTiming(event,
      preparationStarted === null ? null : { product_prepare_ms: preparationMs }));
    const distributed = new DistributedWasmJobRunner(
      this.wasm,
      this.jobId,
      this.lifecycleOwnerId,
      this.hostCapabilities,
      undefined,
      this.resourceAuthority,
      this.resourceWaitTimeoutMs
    );
    this.activeRunner = distributed;
    try {
      onEvent(preparationProgressEvent(this.jobId));
      await distributed.acquire();
      const plan = distributed.prepare(commandText);
      if (preparationStarted !== null) preparationMs = performance.now() - preparationStarted;
      if (plan.mode === 'ready') {
        return distributed.finishPreparedResult(emit);
      }
      if (plan.mode !== 'serial') {
        return await distributed.run(commandText, plan, emit, options);
      }
      distributed.resetPreparedCoordinatorForSerial();
      const serialProgress = new SerialSearchProgress(serialExecutionProgressEvent(this.jobId));
      onEvent(serialProgress.initialEvent);
      const serial = new WasmJobRunner(this.wasm);
      this.activeRunner = serial;
      try {
        return await serial.run(commandText, (event) => emit(serialProgress.project(event)));
      } finally {
        // The preparation owner retains the shared lease across the complete
        // serial execution and releases it only after a terminal/error path.
        distributed.dispose();
      }
    } catch (error) {
      this.dispose();
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
  }
}

function serialExecutionProgressEvent(jobId: number): Extract<ClearraWasmWorkerEvent, { event: 'progress' }> {
  return {
    schema_version: 1,
    runtime: 'clearra-wasm',
    event: 'progress',
    job_id: jobId,
    progress: {
      done: 1,
      total: 5,
      label: 'Serial exact search running',
      budget_status: { state: 'within-budget', used: 0, limit: null },
      backend_status: {
        backend_requested: 'cpu',
        backend_selected: 'wasm-cpu',
        fallback_used: false,
        fallback_reason: null
      },
      memory_status: {
        state: 'wasm-computation-scope-active',
        raw_pointer_exposed: false
      },
      telemetry: {
        execution_mode: 'serial',
        phase: 'searching',
        producer_complete: false,
        geometry_nodes: 0,
        candidates_emitted: 0,
        geometry_family_count: null,
        candidates_verified: 0,
        producer_build_nodes: 0,
        producer_coverage_checks: 0,
        build_nodes: 0,
        coverage_checks: 0,
        ready_workers: 1,
        active_workers: 1,
        worker_count: 1,
        oldest_batch_ms: 0,
        pass_index: 0,
        pass_count: 1,
        layer_index: 0,
        layer_count: 0,
        layer_done: 0,
        layer_total: 0,
        availability: serialExecutionTelemetryFlags(),
        exactness: serialExecutionTelemetryFlags()
      }
    }
  };
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
        layer_total: 0,
        availability: preparationTelemetryFlags(),
        exactness: preparationTelemetryFlags()
      }
    }
  };
}

function preparationTelemetryFlags(): ClearraSearchProgressTelemetryFlags {
  return {
    geometry_nodes: false,
    candidates_emitted: false,
    geometry_family_count: false,
    candidates_verified: false,
    producer_build_nodes: false,
    producer_coverage_checks: false,
    build_nodes: false,
    coverage_checks: false,
    ready_workers: true,
    active_workers: true,
    worker_count: true,
    oldest_batch_ms: true,
    pass_index: false,
    pass_count: false,
    layer_index: false,
    layer_count: false,
    layer_done: false,
    layer_total: false
  };
}

function serialExecutionTelemetryFlags(): ClearraSearchProgressTelemetryFlags {
  return {
    geometry_nodes: false,
    candidates_emitted: false,
    geometry_family_count: false,
    candidates_verified: false,
    producer_build_nodes: false,
    producer_coverage_checks: false,
    build_nodes: false,
    coverage_checks: false,
    ready_workers: true,
    active_workers: true,
    worker_count: true,
    oldest_batch_ms: true,
    pass_index: false,
    pass_count: false,
    layer_index: false,
    layer_count: false,
    layer_done: false,
    layer_total: false
  };
}
