import {
  ClearraWasmRuntimeError,
  loadClearraWasmModule,
  type ClearraDistributedVerifierConsume,
  type ClearraDistributedVerifierProgress,
  type ClearraWasmModule
} from './clearraWasmRuntime';

type VerifierRequest =
  | { type: 'prewarm' }
  | { type: 'initialize'; initialization: string | ArrayBuffer }
  | { type: 'consume'; requestId: number; batch: ArrayBuffer }
  | { type: 'finish'; requestId: number };

type VerifierResponse =
  | { type: 'prewarmed' }
  | { type: 'ready' }
  | {
      type: 'consumed';
      requestId: number;
      candidateCount: number;
      partial: ArrayBuffer | null;
      progress: ClearraDistributedVerifierProgress;
    }
  | { type: 'partial'; requestId: number; partial: ArrayBuffer }
  | { type: 'failed'; requestId?: number; code: string; message: string };

let wasm: ClearraWasmModule | null = null;
let initialized = false;

self.onmessage = (event: MessageEvent<VerifierRequest>) => {
  void handleRequest(event.data);
};

async function handleRequest(request: VerifierRequest) {
  try {
    if (request.type === 'prewarm') {
      wasm ??= await loadClearraWasmModule();
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
      const consumed: ClearraDistributedVerifierConsume =
        wasm.distributed_verifier_consume(request.batch);
      const response: VerifierResponse = {
        type: 'consumed',
        requestId: request.requestId,
        candidateCount: consumed.candidateCount,
        partial: consumed.partial,
        progress: wasm.distributed_verifier_progress()
      };
      post(response, consumed.partial ? [consumed.partial] : []);
      return;
    }
    const partial = wasm.distributed_verifier_finish();
    initialized = false;
    post({ type: 'partial', requestId: request.requestId, partial }, [partial]);
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
  self.postMessage(response, transfer);
}
