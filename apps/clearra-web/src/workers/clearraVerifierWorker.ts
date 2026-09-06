import {
  ClearraWasmRuntimeError,
  loadClearraWasmModule,
  type ClearraDistributedVerifierConsume,
  type ClearraDistributedVerifierProgress,
  type ClearraWasmHostCapabilities,
  type ClearraWasmModule
} from './clearraWasmRuntime';
import { listenForWasmOwnerTermination } from '@clearra/ui/wasm-lifecycle';
import type { ClearraWorkerExecutionKind } from './ClearraVerifierPool';
import { createWorkerHostYield } from './workerHostYield';
import {
  WORKER_HEARTBEAT_INTERVAL_MS,
  sha256Hex,
  type DelegationAcceptance,
  type DelegationOffer,
  type ExecutableDelegationPermit
} from './DurableDelegationJournal';

type VerifierRequest =
  | { type: 'delegation-offer'; offer: DelegationOffer }
  | {
      type: 'delegation-run';
      taskId: string;
      fencingTokenDecimal: string;
    }
  | {
      type: 'prewarm';
      compiledModule?: WebAssembly.Module;
      lifecycleOwnerId?: string;
      hostCapabilities?: ClearraWasmHostCapabilities;
      workerId: string;
    }
  | {
      type: 'initialize';
      initialization: string | ArrayBuffer;
      executionKind?: ClearraWorkerExecutionKind;
      lifecycleOwnerId?: string;
      hostCapabilities?: ClearraWasmHostCapabilities;
      delegation: ExecutableDelegationPermit;
    }
  | {
      type: 'consume';
      requestId: number;
      batch: ArrayBuffer;
      delegation: ExecutableDelegationPermit;
    }
  | { type: 'finish'; requestId: number; delegation: ExecutableDelegationPermit }
  | { type: 'cancel-exact-task'; requestId: number }
  | { type: 'dispose' };

type VerifierResponse =
  | { type: 'prewarmed' }
  | { type: 'delegation-accepted'; acceptance: DelegationAcceptance }
  | {
      type: 'delegation-started';
      taskId: string;
      fencingTokenDecimal: string;
    }
  | {
      type: 'delegation-rejected';
      taskId: string;
      fencingTokenDecimal: string;
      code: string;
      message: string;
    }
  | { type: 'ready' }
  | {
      type: 'heartbeat';
      requestId: number;
      progress: ClearraDistributedVerifierProgress;
    }
  | {
      type: 'consumed';
      requestId: number;
      candidateCount: number;
      candidateCountAvailable: boolean;
      candidateCountExact: boolean;
      partial: ArrayBuffer | null;
      progress: ClearraDistributedVerifierProgress;
    }
  | { type: 'partial'; requestId: number; partial: ArrayBuffer }
  | { type: 'finished'; requestId: number; partial: ArrayBuffer }
  | { type: 'failed'; requestId?: number; code: string; message: string };

let wasm: ClearraWasmModule | null = null;
let initialized = false;
let executionKind: ClearraWorkerExecutionKind = 'geometry-verifier';
const cancelledExactRequests = new Set<number>();
let lifecycleOwnerId = '';
let closeLifecycleListener: (() => void) | null = null;
const acceptedOffers = new Map<string, DelegationOffer>();
type ExecutableVerifierRequest = Extract<
  VerifierRequest,
  { type: 'initialize' | 'consume' | 'finish' }
>;
const stagedExecutables = new Map<string, ExecutableVerifierRequest>();
let workerId = '';
const VERIFIER_HOST_QUANTUM_MS = 8;
const yieldToHost = createWorkerHostYield();

self.onmessage = (event: MessageEvent<VerifierRequest>) => {
  void handleRequest(event.data);
};

async function handleRequest(request: VerifierRequest) {
  try {
    if (request.type === 'cancel-exact-task') {
      if (executionKind === 'exact-at-most') cancelledExactRequests.add(request.requestId);
      return;
    }
    if (request.type === 'delegation-offer') {
      await acceptDelegationOffer(request.offer);
      return;
    }
    if (request.type === 'delegation-run') {
      const staged = stagedExecutables.get(request.taskId);
      if (
        !staged ||
        staged.delegation.fencingTokenDecimal !== request.fencingTokenDecimal
      ) {
        throw new Error('executable start ACK is absent, stale, or mismatched');
      }
      stagedExecutables.delete(request.taskId);
      await executeAuthorized(staged);
      return;
    }
    if ('lifecycleOwnerId' in request && request.lifecycleOwnerId) {
      bindLifecycleOwner(request.lifecycleOwnerId);
    }
    if (request.type === 'dispose') {
      disposeVerifierRuntime();
      return;
    }
    if (request.type === 'prewarm') {
      if (!isPositiveDecimal(request.workerId)) {
        throw new Error('durable verifier worker ID must be a job-local positive integer');
      }
      if (workerId && workerId !== request.workerId) {
        throw new Error('durable verifier worker ID changed after binding');
      }
      workerId = request.workerId;
      wasm ??= await loadClearraWasmModule(
        request.compiledModule,
        request.hostCapabilities
      );
      post({ type: 'prewarmed' });
      return;
    }
    if (
      request.type === 'initialize' ||
      request.type === 'consume' ||
      request.type === 'finish'
    ) {
      await stageExecutable(request);
      return;
    }
    const unreachable: never = request;
    throw new Error(`unsupported verifier request ${String(unreachable)}`);
  } catch (error) {
    if (request.type === 'delegation-offer') {
      post({
        type: 'delegation-rejected',
        taskId: request.offer.taskId,
        fencingTokenDecimal: request.offer.fencingTokenDecimal,
        code: 'E_WASM_DELEGATION_REJECTED',
        message: error instanceof Error ? error.message : String(error)
      });
      return;
    }
    const failure = verifierFailure(error, wasm);
    initialized = false;
    try {
      wasm?.distributed_reset();
    } catch {
      // The parent pool terminates this worker after receiving the failure.
    }
    post({
      type: 'failed',
      requestId: 'requestId' in request ? request.requestId : undefined,
      code: failure.code,
      message: failure.message
    });
  }
}

async function stageExecutable(request: ExecutableVerifierRequest): Promise<void> {
  const payload =
    request.type === 'initialize'
      ? request.initialization
      : request.type === 'consume'
        ? request.batch
        : 'clearra-verifier-finish-v1';
  await authorizeExecutable(request.delegation, payload);
  if (stagedExecutables.has(request.delegation.taskId)) {
    throw new Error('executable delegation is already staged');
  }
  stagedExecutables.set(request.delegation.taskId, request);
  post({
    type: 'delegation-started',
    taskId: request.delegation.taskId,
    fencingTokenDecimal: request.delegation.fencingTokenDecimal
  });
}

async function executeAuthorized(request: ExecutableVerifierRequest): Promise<void> {
  if (request.type === 'initialize') {
    wasm ??= await loadClearraWasmModule(undefined, request.hostCapabilities);
    executionKind = request.executionKind ?? 'geometry-verifier';
    cancelledExactRequests.clear();
    if (executionKind === 'exact-at-most') {
      if (!(request.initialization instanceof ArrayBuffer) ||
        !wasm.distributed_finish_parallel_worker_init ||
        !wasm.distributed_finish_parallel_worker_start ||
        !wasm.distributed_finish_parallel_worker_advance ||
        !wasm.distributed_finish_parallel_worker_cancel) {
        throw new Error('exact worker initialization requires the parallel proof ABI');
      }
      wasm.distributed_finish_parallel_worker_init(request.initialization);
    } else {
      wasm.distributed_verifier_start(request.initialization);
    }
    initialized = true;
    post({ type: 'ready' });
    return;
  }
  if (!wasm || !initialized) throw new Error('distributed verifier is not initialized');
  if (executionKind === 'exact-at-most') {
    if (request.type !== 'consume') throw new Error('exact task receipts have no verifier finish');
    wasm.distributed_finish_parallel_worker_start!(request.batch);
    let lastHeartbeatAt = performance.now();
    let lastHostYieldAt = lastHeartbeatAt;
    let receipt: ArrayBuffer | null = null;
    while (receipt === null) {
      receipt = cancelledExactRequests.delete(request.requestId)
        ? wasm.distributed_finish_parallel_worker_cancel!()
        : wasm.distributed_finish_parallel_worker_advance!(128);
      if (receipt !== null) break;
      const now = performance.now();
      if (now - lastHeartbeatAt >= WORKER_HEARTBEAT_INTERVAL_MS) {
        post({ type: 'heartbeat', requestId: request.requestId, progress: exactTaskProgress() });
        lastHeartbeatAt = now;
      }
      // A solver work slice is not a host scheduling quantum. Match the
      // Geometry verifier's bounded host yield while keeping cancel messages
      // observable between quanta, including during long exact proofs.
      if (now - lastHostYieldAt >= VERIFIER_HOST_QUANTUM_MS) {
        await yieldToHost();
        lastHostYieldAt = performance.now();
      }
    }
    post({
      type: 'consumed', requestId: request.requestId,
      // A proof shard is not a Geometry candidate or a coverage check.
      candidateCount: 0, candidateCountAvailable: false, candidateCountExact: false,
      partial: receipt, progress: exactTaskProgress()
    }, [receipt]);
    return;
  }
  if (request.type === 'consume') {
      let lastHeartbeatAt = performance.now();
      let lastHostYieldAt = lastHeartbeatAt;
      let consumed: ClearraDistributedVerifierConsume =
        wasm.distributed_verifier_consume(request.batch);
      let candidateCount = consumed.candidateCount;
      let candidateCountAvailable = consumed.candidateCountAvailable;
      let candidateCountExact =
        consumed.candidateCountAvailable && consumed.candidateCountExact;
      while (consumed.hasPendingWork) {
        if (consumed.partial) {
          post(
            { type: 'partial', requestId: request.requestId, partial: consumed.partial },
            [consumed.partial]
          );
        }
        const now = performance.now();
        if (now - lastHeartbeatAt >= WORKER_HEARTBEAT_INTERVAL_MS) {
          lastHeartbeatAt = postHeartbeat(request.requestId, wasm, now);
        }
        // A candidate is an atomic core transaction, not a browser scheduling
        // quantum. Yielding a nested setTimeout(0) after every tiny candidate
        // adds timer-clamping latency and leaves CPU workers mostly asleep.
        if (now - lastHostYieldAt >= VERIFIER_HOST_QUANTUM_MS) {
          await yieldToHost();
          lastHostYieldAt = performance.now();
        }
        consumed = wasm.distributed_verifier_continue();
        const accumulated = addVerifierCounts(candidateCount, consumed.candidateCount);
        candidateCount = accumulated.value;
        candidateCountAvailable &&= consumed.candidateCountAvailable;
        candidateCountExact &&=
          consumed.candidateCountAvailable &&
          consumed.candidateCountExact &&
          accumulated.exact;
      }
      const response: VerifierResponse = {
        type: 'consumed',
        requestId: request.requestId,
        candidateCount,
        candidateCountAvailable,
        candidateCountExact,
        partial: consumed.partial,
        progress: wasm.distributed_verifier_progress()
      };
      post(response, consumed.partial ? [consumed.partial] : []);
      return;
  }
  postHeartbeat(request.requestId, wasm);
  const partial = wasm.distributed_verifier_finish();
  initialized = false;
  post({ type: 'finished', requestId: request.requestId, partial }, [partial]);
}

async function acceptDelegationOffer(offer: DelegationOffer): Promise<void> {
  if (!workerId) throw new Error('durable verifier worker ID is not bound');
  if (
    offer.schemaVersion !== 1 ||
    offer.jobId.length === 0 ||
    offer.taskId.length === 0 ||
    offer.coordinatorId.length === 0 ||
    !isSha256(offer.payloadSha256) ||
    !isSha256(offer.requestSha256) ||
    !isPositiveDecimal(offer.computeUnitsDecimal) ||
    !isDecimal(offer.memoryBytesDecimal) ||
    !isPositiveDecimal(offer.fencingTokenDecimal) ||
    !isDecimal(offer.acceptByUnixMsDecimal)
  ) {
    throw new Error('invalid durable delegation offer');
  }
  if (Number(offer.acceptByUnixMsDecimal) < Date.now()) {
    throw new Error('durable delegation offer expired');
  }
  const existing = acceptedOffers.get(offer.taskId);
  if (existing && existing.fencingTokenDecimal !== offer.fencingTokenDecimal) {
    throw new Error('durable delegation task reused with a different fence');
  }
  if (existing) throw new Error('durable delegation offer was already accepted');
  acceptedOffers.set(offer.taskId, offer);
  const reservationSha256 = await sha256Hex(
    `clearra-reservation-v1\0${workerId}\0${offer.taskId}\0${offer.fencingTokenDecimal}\0${offer.payloadSha256}`
  );
  post({
    type: 'delegation-accepted',
    acceptance: {
      taskId: offer.taskId,
      fencingTokenDecimal: offer.fencingTokenDecimal,
      workerId,
      reservationSha256
    }
  });
}

async function authorizeExecutable(
  permit: ExecutableDelegationPermit,
  payload: string | ArrayBuffer
): Promise<void> {
  const offer = acceptedOffers.get(permit.taskId);
  if (
    permit.schemaVersion !== 1 ||
    !offer ||
    permit.jobId !== offer.jobId ||
    permit.workerId !== workerId ||
    permit.fencingTokenDecimal !== offer.fencingTokenDecimal ||
    permit.payloadSha256 !== offer.payloadSha256 ||
    permit.requestSha256 !== offer.requestSha256 ||
    !isPositiveDecimal(permit.publicationSequenceDecimal) ||
    !isSha256(permit.publicationSha256) ||
    !isDecimal(permit.expiresAtUnixMsDecimal) ||
    Number(permit.expiresAtUnixMsDecimal) < Date.now()
  ) {
    throw new Error('executable delegation permit is absent, stale, or mismatched');
  }
  if ((await sha256Hex(payload)) !== permit.payloadSha256) {
    throw new Error('executable delegation payload digest mismatch');
  }
  acceptedOffers.delete(permit.taskId);
}

function isDecimal(value: string): boolean {
  return /^(0|[1-9][0-9]*)$/.test(value);
}

function isPositiveDecimal(value: string): boolean {
  return /^[1-9][0-9]*$/.test(value);
}

function isSha256(value: string): boolean {
  return /^[0-9a-f]{64}$/.test(value);
}

function addVerifierCounts(
  left: number,
  right: number
): { value: number; exact: boolean } {
  if (
    !Number.isSafeInteger(left) ||
    left < 0 ||
    !Number.isSafeInteger(right) ||
    right < 0 ||
    left > Number.MAX_SAFE_INTEGER - right
  ) {
    return { value: Number.MAX_SAFE_INTEGER, exact: false };
  }
  return { value: left + right, exact: true };
}

function postHeartbeat(
  requestId: number,
  runtime: ClearraWasmModule,
  now = performance.now()
): number {
  post({
    type: 'heartbeat',
    requestId,
    progress: runtime.distributed_verifier_progress()
  });
  return now;
}

function exactTaskProgress(): ClearraDistributedVerifierProgress {
  return {
    candidateCount: 0, buildNodes: 0, coverageChecks: 0,
    availability: { candidateCount: false, buildNodes: false, coverageChecks: false },
    exactness: { candidateCount: false, buildNodes: false, coverageChecks: false }
  };
}

function bindLifecycleOwner(ownerId: string) {
  if (lifecycleOwnerId === ownerId) return;
  closeLifecycleListener?.();
  lifecycleOwnerId = ownerId;
  closeLifecycleListener = listenForWasmOwnerTermination(ownerId, () => {
    disposeVerifierRuntime();
  });
}

function disposeVerifierRuntime() {
  initialized = false;
  try {
    wasm?.distributed_reset();
  } catch {
    // Closing this worker releases a trapped verifier's complete WASM instance.
  }
  wasm = null;
  closeLifecycleListener?.();
  closeLifecycleListener = null;
  lifecycleOwnerId = '';
  acceptedOffers.clear();
  stagedExecutables.clear();
  self.close();
}

function verifierFailure(error: unknown, wasm: ClearraWasmModule | null) {
  let diagnostics: ReturnType<ClearraWasmModule['failure_diagnostics']> | undefined;
  try {
    diagnostics = wasm?.failure_diagnostics();
  } catch {
    // Reading diagnostics from a trapped instance is best effort. It must not
    // prevent the original failure from reaching the parent and ending the job.
  }
  const baseMessage = error instanceof Error ? error.message : String(error);
  const context = diagnostics
    ? `WASM linear memory: ${formatByteCount(diagnostics.linearMemoryBytes)}` +
      (diagnostics.rustPanic ? `; Rust panic: ${diagnostics.rustPanic}` : '')
    : null;
  const message = context ? `${baseMessage} (${context})` : baseMessage;
  if (error instanceof ClearraWasmRuntimeError) {
    return { code: error.diagnosticCode, message };
  }
  if (error instanceof WebAssembly.RuntimeError) {
    const memoryExhausted =
      error.message.toLowerCase().includes('unreachable') &&
      !diagnostics?.rustPanic &&
      (diagnostics?.linearMemoryBytes ?? 0) >= 3 * 1024 * 1024 * 1024;
    return {
      code: memoryExhausted ? 'E_WASM_LINEAR_MEMORY_EXHAUSTED' : 'E_WASM_VERIFIER_TRAP',
      message
    };
  }
  return { code: 'E_WASM_VERIFIER_FAILED', message };
}

function formatByteCount(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GiB`;
}

function post(response: VerifierResponse, transfer: Transferable[] = []) {
  (
    self as unknown as {
      postMessage(message: unknown, transfer: Transferable[]): void;
    }
  ).postMessage(response, transfer);
}
