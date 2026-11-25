import type { ClearraWasmWorkerEvent } from '@clearra/ui/wasm';
import { listenForWasmOwnerTermination } from '@clearra/ui/wasm-lifecycle';
import {
  ClearraWasmRuntimeError,
  type AcceleratorWorkerRelation,
  type AcceleratorWorkerSynopsis,
  type ClearraDistributedPlan,
  type ClearraWasmHostCapabilities,
  type ClearraWasmModule
} from './clearraWasmRuntime';
import { ClearraVerifierPool } from './ClearraVerifierPool';
import { DistributedWasmJobRunner } from './DistributedWasmJobRunner';
import type { DistributedSourceRuntime, DistributedSourceTask } from './DistributedSourceRuntime';
import {
  SharedExecutionResourceAuthority,
  type SharedExecutionResourceCapacity
} from './SharedExecutionResourceAuthority';

// Only source-owned operations cross this boundary. Opaque handles, product
// pages, immutable accelerator owners and the live WASM instance stay together.
const SOURCE_METHODS = [
  'distributed_produce', 'distributed_progress', 'distributed_merge_partial',
  'distributed_finish', 'distributed_finish_start', 'distributed_finish_advance',
  'distributed_finish_parallel_prepare', 'distributed_finish_parallel_configure',
  'distributed_finish_parallel_admit', 'distributed_finish_parallel_guarded_query',
  'distributed_finish_parallel_task', 'distributed_finish_parallel_merge',
  'distributed_finish_parallel_found', 'distributed_finish_parallel_assist',
  'distributed_finish_parallel_last_task_key', 'distributed_finish_parallel_redundant',
  'distributed_cancel', 'distributed_reset', 'profile_start', 'profile_finish'
] as const satisfies readonly (keyof ClearraWasmModule)[];
type SourceMethod = typeof SOURCE_METHODS[number];
type RpcMethod = SourceMethod | 'take_exact_task' | 'conditioned_queries';
type Failure = { name: string; message: string; diagnosticCode?: string };
type RelationWire = Omit<AcceleratorWorkerRelation, 'answerQueries'>;

type Start = {
  type: 'run'; generation: number; jobId: number; lifecycleOwnerId: string;
  commandText: string; plan: ClearraDistributedPlan;
  host: ClearraWasmHostCapabilities; grant: SharedExecutionResourceCapacity;
  compiledModule: WebAssembly.Module; methods: SourceMethod[];
  synopsis: AcceleratorWorkerSynopsis | null; relation: RelationWire | null;
  transportProfile: boolean;
};
type SourceRequest = { type: 'source'; generation: number; requestId: number;
  method: RpcMethod; args: unknown[] };
type SourceReply = { type: 'source-reply'; generation: number; requestId: number;
  value?: unknown; error?: Failure };
type ManagerReply = SourceRequest |
  { type: 'event'; generation: number; event: ClearraWasmWorkerEvent } |
  { type: 'done'; generation: number; terminal?: ClearraWasmWorkerEvent; error?: Failure };
type ManagerMessage = Start | SourceReply | { type: 'cancel'; generation: number } | { type: 'dispose' };

type ManagerScope = {
  onmessage: ((event: MessageEvent<ManagerMessage>) => void) | null;
  postMessage(message: ManagerReply, transfer?: Transferable[]): void;
  close?(): void;
};

export type DedicatedManagerOptions = {
  wasm: ClearraWasmModule;
  jobId: number;
  lifecycleOwnerId: string;
  hostCapabilities: ClearraWasmHostCapabilities;
  legalBoardSynopsis?: AcceleratorWorkerSynopsis | null;
  conditionedPack?: AcceleratorWorkerRelation | null;
};
export type DedicatedManagerFactory = (options: DedicatedManagerOptions) => DedicatedDistributedManager;

let nextGeneration = 1;

/** The caller is the source compute owner; dispatch, durable journals, verifier
 * readiness and timers execute in a different, control-only worker. */
export class DedicatedDistributedManager {
  private worker: Worker | null = null;
  private readonly generation = nextGeneration++;
  private cancelled = false;
  private sourceOwned = false;
  private rejectRun: ((error: Error) => void) | null = null;
  private sourceTail: Promise<void> = Promise.resolve();

  constructor(
    private readonly options: DedicatedManagerOptions,
    private readonly workerFactory: () => Worker,
    private readonly releaseWorker: (worker: Worker, reusable: boolean) => void = worker => worker.terminate()
  ) {}

  async run(commandText: string, plan: ClearraDistributedPlan,
    grant: SharedExecutionResourceCapacity,
    onEvent: (event: ClearraWasmWorkerEvent) => void,
    runOptions: { transportProfile?: boolean } = {}): Promise<ClearraWasmWorkerEvent> {
    if (this.worker || this.sourceOwned) throw new Error('dedicated manager is already running');
    this.sourceOwned = true;
    this.cancelled = false;
    const { wasm } = this.options;
    let successful = false;
    try {
      const worker = this.worker = this.workerFactory();
      const generation = this.generation;
      const terminal = await new Promise<ClearraWasmWorkerEvent>((resolve, reject) => {
        this.rejectRun = reject;
        worker.onerror = event => reject(new Error(event.message || 'dedicated manager failed'));
        worker.onmessageerror = () => reject(new Error('dedicated manager message could not be decoded'));
        worker.onmessage = (event: MessageEvent<ManagerReply>) => {
          if (this.worker !== worker || event.data.generation !== generation) return;
          const message = event.data;
          if (message.type === 'source') {
            // One ordered queue owns the live WASM state. A cancellation fences
            // already queued work before it can run against a replacement job.
            this.sourceTail = this.sourceTail.then(() => this.answerSource(worker, message))
              .catch(error => reject(asError(error)));
          } else if (message.type === 'event') {
            if (!this.cancelled && !isTerminal(message.event)) onEvent(message.event);
          } else if (message.type === 'done') {
            if (message.error) reject(fromFailure(message.error));
            else if (!message.terminal || !isTerminal(message.terminal)) reject(new Error('dedicated manager omitted its terminal event'));
            else if (this.cancelled) reject(new Error('distributed search cancelled'));
            else { onEvent(message.terminal); resolve(message.terminal); }
          }
        };
        const relation = this.options.conditionedPack;
        const synopsis = this.options.legalBoardSynopsis;
        const start: Start = {
          type: 'run', generation, jobId: this.options.jobId,
          lifecycleOwnerId: this.options.lifecycleOwnerId, commandText, plan,
          host: this.options.hostCapabilities, grant,
          compiledModule: wasm.compiled_module(),
          methods: SOURCE_METHODS.filter(method => typeof wasm[method] === 'function'),
          synopsis: synopsis ? { ...synopsis, wire: synopsis.wire.slice(0) } : null,
          relation: relation ? { profile: relation.profile, seed: relation.seed.slice(0),
            identity: relation.identity, reservedBytes: relation.reservedBytes,
            maximumPeers: relation.maximumPeers } : null,
          transportProfile: runOptions.transportProfile === true
        };
        worker.postMessage(start, ownedBuffers(start));
      });
      successful = true;
      return terminal;
    } finally {
      // Terminate the manager only after it drained and explicitly closed its
      // children; cancellation/failure also invalidates every queued source RPC.
      this.rejectRun = null;
      const worker = this.worker;
      this.worker = null;
      if (worker) {
        worker.onmessage = null;
        worker.onerror = null;
        worker.onmessageerror = null;
        this.releaseWorker(worker, successful);
      }
      if (this.sourceOwned) {
        this.sourceOwned = false;
        if (!successful) { try { wasm.distributed_cancel(); } catch { /* reset owns release */ } }
        wasm.distributed_reset();
      }
    }
  }

  cancel() {
    if (this.cancelled) return;
    this.cancelled = true;
    try { this.options.wasm.distributed_cancel(); } catch { /* outer fail-close owns a trapped source */ }
    try { this.worker?.postMessage({ type: 'cancel', generation: this.generation }); }
    catch (error) { this.rejectRun?.(asError(error)); }
  }

  dispose() {
    this.cancel();
  }

  private async answerSource(worker: Worker, request: SourceRequest): Promise<void> {
    const current = () => this.worker === worker && request.generation === this.generation;
    if (!current()) return;
    try {
      if (this.cancelled && request.method !== 'distributed_reset' && request.method !== 'distributed_cancel') {
        throw new Error('distributed source generation cancelled');
      }
      const { wasm, conditionedPack } = this.options;
      let value: unknown;
      if (request.method === 'conditioned_queries') {
        if (!conditionedPack || !(request.args[0] instanceof ArrayBuffer)) throw new Error('conditioned source query is unavailable');
        value = await conditionedPack.answerQueries(request.args[0]);
      } else if (request.method === 'take_exact_task') {
        const jobId = request.args[0] as number;
        if (jobId !== this.options.jobId || !wasm.distributed_finish_parallel_task) throw new Error('exact source task owner mismatch');
        const task = wasm.distributed_finish_parallel_task(jobId);
        // Task identity is a single mutable core slot: capture the key in the
        // SAME source turn, before any merge, assistance or second issuance.
        value = { task, key: task && wasm.distributed_finish_parallel_last_task_key
          ? wasm.distributed_finish_parallel_last_task_key(jobId) : null };
      } else {
        if (!SOURCE_METHODS.includes(request.method)) throw new Error('unknown distributed source operation');
        const method = wasm[request.method];
        if (typeof method !== 'function') throw new Error('distributed source operation is unavailable');
        value = Reflect.apply(method, wasm, request.args);
        if (request.method === 'distributed_reset') this.sourceOwned = false;
      }
      if (current()) worker.postMessage({ type: 'source-reply', generation: request.generation,
        requestId: request.requestId, value }, ownedBuffers(value));
    } catch (error) {
      if (current()) worker.postMessage({ type: 'source-reply', generation: request.generation,
        requestId: request.requestId, error: failure(error) });
    }
  }
}

/** Shared by the real worker entrypoint and executable transport contracts. */
export function installDedicatedDistributedManager(
  scope: ManagerScope,
  poolFactory: () => ClearraVerifierPool = () => new ClearraVerifierPool()
) {
  let active: { generation: number; runner: DistributedWasmJobRunner; pool: ClearraVerifierPool } | null = null;
  let cachedPool: ClearraVerifierPool | null = null;
  let releaseLifecycle = () => {};
  let nextRequestId = 1;
  const pending = new Map<number, { resolve: (value: unknown) => void; reject: (error: Error) => void }>();
  const rejectPending = (error: Error) => {
    for (const entry of pending.values()) entry.reject(error);
    pending.clear();
  };
  scope.onmessage = event => {
    const message = event.data;
    if (message.type === 'dispose') {
      active?.runner.cancel();
      cachedPool?.cancel();
      cachedPool = null;
      rejectPending(new Error('dedicated manager owner disposed'));
      releaseLifecycle();
      scope.close?.();
      return;
    }
    if (message.type === 'source-reply') {
      if (message.generation !== active?.generation) return;
      const entry = pending.get(message.requestId);
      if (!entry) return;
      pending.delete(message.requestId);
      if (message.error) entry.reject(fromFailure(message.error));
      else entry.resolve(message.value);
      return;
    }
    if (message.type === 'cancel') {
      if (message.generation !== active?.generation) return;
      active.runner.cancel();
      rejectPending(new Error('distributed manager cancelled'));
      return;
    }
    if (active) {
      scope.postMessage({ type: 'done', generation: message.generation,
        error: failure(new Error('dedicated manager already has an execution owner')) });
      return;
    }
    const start = message;
    const call = (method: RpcMethod, args: unknown[]): Promise<unknown> => {
      const requestId = nextRequestId++;
      return new Promise((resolve, reject) => {
        pending.set(requestId, { resolve, reject });
        // Merges and immutable query packets are consumed by their source.
        // Exact routing keys remain borrowed from the pool and must not detach.
        const transfer = ['distributed_merge_partial', 'distributed_finish_parallel_merge',
          'conditioned_queries'].includes(method) ? ownedBuffers(args) : [];
        try { scope.postMessage({ type: 'source', generation: start.generation, requestId, method, args }, transfer); }
        catch (error) { pending.delete(requestId); reject(asError(error)); }
      });
    };
    const source: Record<string, unknown> = { compiled_module: () => start.compiledModule };
    for (const method of start.methods) source[method] = (...args: unknown[]) => call(method, args);
    if (start.methods.includes('distributed_finish_parallel_task')) {
      source.takeExactTask = (jobId: number) => call('take_exact_task', [jobId]) as Promise<DistributedSourceTask>;
    }
    const relation = start.relation ? { ...start.relation,
      answerQueries: (wire: ArrayBuffer) => call('conditioned_queries', [wire]) as Promise<ArrayBuffer> } : null;
    const pool = cachedPool ??= poolFactory();
    const runner = new DistributedWasmJobRunner(source as DistributedSourceRuntime,
      start.jobId, start.lifecycleOwnerId, start.host, pool,
      // This is a delegated view of the source owner's still-live whole-job
      // lease, never a second grant derived from navigator or free memory.
      new SharedExecutionResourceAuthority(start.grant), undefined, 'dedicated', start.synopsis, relation);
    active = { generation: start.generation, runner, pool };
    releaseLifecycle();
    releaseLifecycle = listenForWasmOwnerTermination(start.lifecycleOwnerId, () => {
      active?.runner.cancel();
      pool.cancel();
      rejectPending(new Error('dedicated manager source owner terminated'));
      releaseLifecycle();
      scope.close?.();
    });
    void runner.run(start.commandText, start.plan, event => {
      scope.postMessage({ type: 'event', generation: start.generation, event });
    }, { transportProfile: start.transportProfile }).then(
      terminal => {
        rejectPending(new Error('distributed manager completed'));
        scope.postMessage({ type: 'done', generation: start.generation, terminal });
        active = null;
      },
      error => {
        pool.cancel();
        cachedPool = null;
        rejectPending(asError(error));
        scope.postMessage({ type: 'done', generation: start.generation, error: failure(error) });
        active = null;
      }
    );
  };
}

function isTerminal(event: ClearraWasmWorkerEvent) {
  return ['final_response', 'failed', 'cancelled', 'terminated'].includes(event.event);
}
function asError(error: unknown): Error { return error instanceof Error ? error : new Error(String(error)); }
function failure(error: unknown): Failure {
  const value = asError(error);
  return { name: value.name, message: value.message,
    ...(value instanceof ClearraWasmRuntimeError ? { diagnosticCode: value.diagnosticCode } : {}) };
}
function fromFailure(error: Failure): Error {
  return error.diagnosticCode ? new ClearraWasmRuntimeError(error.diagnosticCode, error.message)
    : Object.assign(new Error(error.message), { name: error.name });
}
function ownedBuffers(value: unknown): Transferable[] {
  const buffers = new Set<ArrayBuffer>();
  const visit = (item: unknown) => {
    if (item instanceof ArrayBuffer) buffers.add(item);
    else if (Array.isArray(item)) item.forEach(visit);
    else if (item !== null && typeof item === 'object') Object.values(item).forEach(visit);
  };
  visit(value);
  return [...buffers];
}
