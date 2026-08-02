import {
  ClearraWasmRuntimeError,
  loadClearraWasmModule,
  type ClearraDistributedVerifierConsume,
  type ClearraDistributedVerifierProgress,
  type ClearraWasmModule
} from './clearraWasmRuntime';
import { listenForWasmOwnerTermination } from '@clearra/ui/wasm-lifecycle';

type VerifierRequest =
  | {
      type: 'prewarm';
      compiledModule?: WebAssembly.Module;
      lifecycleOwnerId?: string;
    }
  | {
      type: 'initialize';
      initialization: string | ArrayBuffer;
      lifecycleOwnerId?: string;
    }
  | { type: 'consume'; requestId: number; batch: ArrayBuffer }
  | { type: 'finish'; requestId: number }
  | { type: 'dispose' };

type VerifierResponse =
  | { type: 'prewarmed' }
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
      partial: ArrayBuffer | null;
      progress: ClearraDistributedVerifierProgress;
    }
  | { type: 'partial'; requestId: number; partial: ArrayBuffer }
  | { type: 'finished'; requestId: number; partial: ArrayBuffer }
  | { type: 'failed'; requestId?: number; code: string; message: string };

let wasm: ClearraWasmModule | null = null;
let initialized = false;
let lifecycleOwnerId = '';
let closeLifecycleListener: (() => void) | null = null;
const HEARTBEAT_INTERVAL_MS = 1_000;

self.onmessage = (event: MessageEvent<VerifierRequest>) => {
  void handleRequest(event.data);
};

async function handleRequest(request: VerifierRequest) {
  try {
    if ('lifecycleOwnerId' in request && request.lifecycleOwnerId) {
      bindLifecycleOwner(request.lifecycleOwnerId);
    }
    if (request.type === 'dispose') {
      disposeVerifierRuntime();
      return;
    }
    if (request.type === 'prewarm') {
      wasm ??= await loadClearraWasmModule(request.compiledModule);
      post({ type: 'prewarmed' });
      return;
    }
    if (request.type === 'initialize') {
      wasm ??= await loadClearraWasmModule();
      wasm.distributed_verifier_start(request.initialization);
      initialized = true;
      post({ type: 'ready' });
      return;
    }
    if (!wasm || !initialized) throw new Error('distributed verifier is not initialized');
    if (request.type === 'consume') {
      let lastHeartbeatAt = performance.now();
      let consumed: ClearraDistributedVerifierConsume =
        wasm.distributed_verifier_consume(request.batch);
      const candidateCount = consumed.candidateCount;
      while (consumed.hasPendingWork) {
        if (consumed.partial) {
          post(
            { type: 'partial', requestId: request.requestId, partial: consumed.partial },
            [consumed.partial]
          );
        }
        const now = performance.now();
        if (now - lastHeartbeatAt >= HEARTBEAT_INTERVAL_MS) {
          lastHeartbeatAt = postHeartbeat(request.requestId, wasm, now);
        }
        await yieldToHost();
        consumed = wasm.distributed_verifier_continue();
      }
      const response: VerifierResponse = {
        type: 'consumed',
        requestId: request.requestId,
        candidateCount,
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
  } catch (error) {
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

function yieldToHost(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
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
  self.close();
}

function verifierFailure(error: unknown, wasm: ClearraWasmModule | null) {
  const diagnostics = wasm?.failure_diagnostics();
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
