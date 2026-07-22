export type ClearraWasmModule = {
  compiled_module: () => WebAssembly.Module;
  configure_host: (capabilities: ClearraWasmHostCapabilities) => void;
  start_job: (commandText: string) => number;
  advance_job: (
    jobId: number,
    workBudget: number
  ) => 'pending' | 'completed' | 'cancelled' | 'failed';
  cancel_job: (jobId: number) => void;
  drain_job_events_json: (jobId: number) => string;
  distributed_prepare: (commandText: string) => ClearraDistributedPlan;
  distributed_produce: (
    workBudget: number,
    batchCapacity: number
  ) => ClearraDistributedProducerResult;
  distributed_progress: () => ClearraDistributedCoreProgress;
  distributed_merge_partial: (partial: ArrayBuffer) => void;
  distributed_finish: (jobId: number, workersUsed: number) => string;
  distributed_cancel: () => void;
  distributed_reset: () => void;
  distributed_verifier_start: (initialization: string | ArrayBuffer) => void;
  distributed_verifier_consume: (batch: ArrayBuffer) => ClearraDistributedVerifierConsume;
  distributed_verifier_progress: () => ClearraDistributedVerifierProgress;
  distributed_verifier_finish: () => ArrayBuffer;
  prewarm_gpu: (deviceIndex: number | null) => Promise<'connected' | 'unavailable'>;
  profile_start?: () => void;
  profile_finish?: () => unknown;
  failure_diagnostics: () => ClearraWasmFailureDiagnostics;
};

export type ClearraWasmFailureDiagnostics = {
  linearMemoryBytes: number;
  rustPanic: string | null;
};

export type ClearraDistributedPlan = {
  mode: 'serial' | 'cpu-multi' | 'gpu-multi';
  workerCount: number;
  requestedBackend: 'auto' | 'cpu' | 'gpu' | 'hybrid';
  selectedBackend: 'wasm-cpu' | 'webgpu';
  fallbackUsed: boolean;
  fallbackReason: string | null;
  workerInitialization: ArrayBuffer | null;
};

export type ClearraDistributedVerifierConsume = {
  candidateCount: number;
  partial: ArrayBuffer | null;
};

export type ClearraDistributedProducerResult =
  | { status: 'pending' | 'completed' | 'cancelled' }
  | { status: 'batch'; batch: ArrayBuffer };

export type ClearraDistributedCoreProgress = {
  geometryNodes: number;
  candidateCount: number;
  candidateFamilyCount: string | null;
  passIndex: number;
  passCount: number;
};

export type ClearraDistributedVerifierProgress = {
  candidateCount: number;
  buildNodes: number;
  coverageChecks: number;
};

export type ClearraWasmHostCapabilities = {
  logicalProcessorCount: number;
  webGpuAvailable: boolean;
  crossOriginIsolated: boolean;
};

let wasmModulePromise: Promise<ClearraWasmModule> | null = null;

type ClearraRawWasmExports = {
  memory: WebAssembly.Memory;
  clearra_wasm_abi_version: () => number;
  clearra_wasm_configure_host: (
    logicalProcessorCount: number,
    capabilityFlags: number
  ) => number;
  clearra_wasm_input_resize: (byteLen: number) => number;
  clearra_wasm_input_ptr: () => number;
  clearra_wasm_transfer_resize: (byteLen: number) => number;
  clearra_wasm_transfer_ptr: () => number;
  clearra_wasm_distributed_prepare: () => number;
  clearra_wasm_distributed_worker_initialization: () => number;
  clearra_wasm_distributed_worker_count: () => number;
  clearra_wasm_distributed_requested_backend: () => number;
  clearra_wasm_distributed_preparation_fallback_reason: () => number;
  clearra_wasm_distributed_produce: (
    workBudget: number,
    batchCapacity: number
  ) => number;
  clearra_wasm_distributed_progress_geometry_nodes: () => number;
  clearra_wasm_distributed_progress_candidate_count: () => number;
  clearra_wasm_distributed_progress_candidate_family_count_available: () => number;
  clearra_wasm_distributed_progress_candidate_family_count_word: (wordIndex: number) => number;
  clearra_wasm_distributed_progress_pass_index: () => number;
  clearra_wasm_distributed_progress_pass_count: () => number;
  clearra_wasm_distributed_merge_partial: () => number;
  clearra_wasm_distributed_finish: (jobId: number, workersUsed: number) => number;
  clearra_wasm_distributed_cancel: () => number;
  clearra_wasm_distributed_reset: () => number;
  clearra_wasm_distributed_verifier_start: () => number;
  clearra_wasm_distributed_forward_verifier_start: () => number;
  clearra_wasm_distributed_verifier_consume: () => number;
  clearra_wasm_distributed_verifier_partial_available: () => number;
  clearra_wasm_distributed_verifier_progress_candidate_count: () => number;
  clearra_wasm_distributed_verifier_progress_build_nodes: () => number;
  clearra_wasm_distributed_verifier_progress_coverage_checks: () => number;
  clearra_wasm_distributed_verifier_finish: () => number;
  clearra_wasm_gpu_warmup_start: (deviceIndex: number) => number;
  clearra_wasm_gpu_warmup_advance: () => number;
  clearra_wasm_start_job: () => number;
  clearra_wasm_advance_job: (jobId: number, workBudget: number) => number;
  clearra_wasm_cancel_job: (jobId: number) => number;
  clearra_wasm_drain_job_events: (jobId: number) => number;
  clearra_wasm_output_ptr: () => number;
  clearra_wasm_output_len: () => number;
  clearra_wasm_last_panic_ptr: () => number;
  clearra_wasm_last_panic_len: () => number;
  clearra_wasm_profile_start?: () => number;
  clearra_wasm_profile_finish?: () => number;
};

type ClearraWasmBindings = {
  default: (input?: {
    module_or_path: string | URL | WebAssembly.Module;
  }) => Promise<ClearraRawWasmExports>;
};

type ClearraWasmArtifactManifest = {
  schema_version: 1;
  bindings: { path: string; bytes: number; sha256: string };
  wasm: { path: string; bytes: number; sha256: string };
};

export class ClearraWasmRuntimeError extends Error {
  constructor(
    public readonly diagnosticCode: string,
    message: string
  ) {
    super(message);
    this.name = 'ClearraWasmRuntimeError';
  }

  static fromRuntimeOutput(output: string): ClearraWasmRuntimeError {
    const match = /^(E_[A-Z0-9_]+):\s*([\s\S]*)$/.exec(output.trim());
    if (!match) return new ClearraWasmRuntimeError('E_WASM_EXECUTION_FAILED', output);
    return new ClearraWasmRuntimeError(match[1], match[2]);
  }
}

const encoder = new TextEncoder();
const decoder = new TextDecoder('utf-8', { fatal: true });

export function loadClearraWasmModule(
  sharedCompiledModule?: WebAssembly.Module
): Promise<ClearraWasmModule> {
  if (!wasmModulePromise) {
    wasmModulePromise = (async () => {
      const wasmRoot = `${deploymentBaseFromWorkerLocation(self.location.pathname)}/wasm`;
      const manifestUrl = new URL(`${wasmRoot}/clearra_wasm.manifest.json`, self.location.origin);
      const manifestResponse = await fetch(manifestUrl, { cache: 'no-store' });
      if (!manifestResponse.ok) {
        throw new Error(`Clearra WASM manifest unavailable: ${manifestResponse.status}`);
      }
      const manifest = (await manifestResponse.json()) as ClearraWasmArtifactManifest;
      if (manifest.schema_version !== 1 || !isSha256(manifest.wasm.sha256)) {
        throw new Error('Clearra WASM manifest is invalid');
      }
      const bindingsUrl = new URL(`${wasmRoot}/${manifest.bindings.path}`, self.location.origin);
      const wasmUrl = new URL(`${wasmRoot}/${manifest.wasm.path}`, self.location.origin);
      bindingsUrl.searchParams.set('v', manifest.bindings.sha256);
      wasmUrl.searchParams.set('v', manifest.wasm.sha256);
      const bindings = (await import(
        /* @vite-ignore */ bindingsUrl.href
      )) as ClearraWasmBindings;
      const compiledModule =
        sharedCompiledModule ?? (await compileClearraWasmModule(wasmUrl));
      const raw = await bindings.default({ module_or_path: compiledModule });
      const module = wrapRawModule(raw, compiledModule);
      module.configure_host(detectHostCapabilities());
      return module;
    })();
  }
  return wasmModulePromise;
}

async function compileClearraWasmModule(wasmUrl: URL): Promise<WebAssembly.Module> {
  const response = await fetch(wasmUrl);
  if (!response.ok) {
    throw new Error(`Clearra WASM artifact unavailable: ${response.status}`);
  }
  if (typeof WebAssembly.compileStreaming === 'function') {
    const fallbackResponse = response.clone();
    try {
      return await WebAssembly.compileStreaming(response);
    } catch {
      return WebAssembly.compile(await fallbackResponse.arrayBuffer());
    }
  }
  return WebAssembly.compile(await response.arrayBuffer());
}

function deploymentBaseFromWorkerLocation(pathname: string): string {
  const appMarker = '/_app/';
  const appIndex = pathname.lastIndexOf(appMarker);
  return appIndex < 0 ? '' : pathname.slice(0, appIndex);
}

function isSha256(value: string): boolean {
  return /^[0-9a-f]{64}$/.test(value);
}

function wrapRawModule(
  raw: ClearraRawWasmExports,
  compiledModule: WebAssembly.Module
): ClearraWasmModule {
  if (raw.clearra_wasm_abi_version() !== 1) {
    throw new Error('unsupported Clearra WASM ABI version');
  }

  const outputText = () => {
    const ptr = raw.clearra_wasm_output_ptr();
    const len = raw.clearra_wasm_output_len();
    return decoder.decode(new Uint8Array(raw.memory.buffer, ptr, len));
  };
  const outputBytes = () => {
    const ptr = raw.clearra_wasm_output_ptr();
    const len = raw.clearra_wasm_output_len();
    return new Uint8Array(raw.memory.buffer, ptr, len).slice().buffer;
  };
  const lastPanic = () => {
    const ptr = raw.clearra_wasm_last_panic_ptr();
    const len = raw.clearra_wasm_last_panic_len();
    return len === 0 ? null : decoder.decode(new Uint8Array(raw.memory.buffer, ptr, len));
  };
  const requireOk = (status: number) => {
    if (status < 0) throw ClearraWasmRuntimeError.fromRuntimeOutput(outputText());
  };
  const setCommand = (commandText: string) => {
    const bytes = encoder.encode(commandText);
    requireOk(raw.clearra_wasm_input_resize(bytes.byteLength));
    const ptr = raw.clearra_wasm_input_ptr();
    new Uint8Array(raw.memory.buffer, ptr, bytes.byteLength).set(bytes);
  };
  const setTransfer = (input: ArrayBuffer) => {
    requireOk(raw.clearra_wasm_transfer_resize(input.byteLength));
    const ptr = raw.clearra_wasm_transfer_ptr();
    new Uint8Array(raw.memory.buffer, ptr, input.byteLength).set(new Uint8Array(input));
  };

  const module: ClearraWasmModule = {
    compiled_module() {
      return compiledModule;
    },
    failure_diagnostics() {
      return {
        linearMemoryBytes: raw.memory.buffer.byteLength,
        rustPanic: lastPanic()
      };
    },
    configure_host(capabilities) {
      const flags =
        (capabilities.webGpuAvailable ? 1 : 0) |
        (capabilities.crossOriginIsolated ? 2 : 0);
      requireOk(
        raw.clearra_wasm_configure_host(
          Math.max(1, Math.floor(capabilities.logicalProcessorCount)),
          flags
        )
      );
    },
    start_job(commandText) {
      setCommand(commandText);
      const jobId = raw.clearra_wasm_start_job();
      if (jobId === 0) throw new Error(outputText());
      return jobId;
    },
    advance_job(jobId, workBudget) {
      const status = raw.clearra_wasm_advance_job(jobId, workBudget);
      requireOk(status);
      const labels = ['pending', 'completed', 'cancelled', 'failed'] as const;
      const label = labels[status];
      if (!label) throw new Error(`invalid Clearra WASM advance status: ${status}`);
      return label;
    },
    cancel_job(jobId) {
      requireOk(raw.clearra_wasm_cancel_job(jobId));
    },
    drain_job_events_json(jobId) {
      requireOk(raw.clearra_wasm_drain_job_events(jobId));
      return outputText();
    },
    distributed_prepare(commandText) {
      setCommand(commandText);
      const mode = raw.clearra_wasm_distributed_prepare();
      requireOk(mode);
      const labels = ['serial', 'cpu-multi', 'gpu-multi'] as const;
      const requestedLabels = ['auto', 'cpu', 'gpu', 'hybrid'] as const;
      const fallbackReasonCode = raw.clearra_wasm_distributed_preparation_fallback_reason();
      const selectedMode = labels[mode] ?? 'serial';
      requireOk(raw.clearra_wasm_distributed_worker_initialization());
      const initialization = outputBytes();
      return {
        mode: selectedMode,
        workerCount: Math.max(1, raw.clearra_wasm_distributed_worker_count()),
        requestedBackend:
          requestedLabels[raw.clearra_wasm_distributed_requested_backend()] ?? 'auto',
        selectedBackend: selectedMode === 'gpu-multi' ? 'webgpu' : 'wasm-cpu',
        fallbackUsed: fallbackReasonCode !== 0,
        fallbackReason: fallbackReasonCode === 1 ? 'gpu_kernel_unavailable' : null,
        workerInitialization: initialization.byteLength === 0 ? null : initialization
      };
    },
    distributed_produce(workBudget, batchCapacity) {
      const status = raw.clearra_wasm_distributed_produce(workBudget, batchCapacity);
      requireOk(status);
      if (status === 1) return { status: 'batch', batch: outputBytes() };
      if (status === 2) return { status: 'completed' };
      if (status === 3) return { status: 'cancelled' };
      if (status !== 0) throw new Error(`invalid distributed producer status: ${status}`);
      return { status: 'pending' };
    },
    distributed_progress() {
      return {
        geometryNodes: raw.clearra_wasm_distributed_progress_geometry_nodes(),
        candidateCount: raw.clearra_wasm_distributed_progress_candidate_count(),
        candidateFamilyCount: readCandidateFamilyCount(raw),
        passIndex: raw.clearra_wasm_distributed_progress_pass_index(),
        passCount: Math.max(1, raw.clearra_wasm_distributed_progress_pass_count())
      };
    },
    distributed_merge_partial(partial) {
      setTransfer(partial);
      requireOk(raw.clearra_wasm_distributed_merge_partial());
    },
    distributed_finish(jobId, workersUsed) {
      requireOk(raw.clearra_wasm_distributed_finish(jobId, workersUsed));
      return outputText();
    },
    distributed_cancel() {
      requireOk(raw.clearra_wasm_distributed_cancel());
    },
    distributed_reset() {
      requireOk(raw.clearra_wasm_distributed_reset());
    },
    distributed_verifier_start(initialization) {
      if (typeof initialization === 'string') {
        setCommand(initialization);
        requireOk(raw.clearra_wasm_distributed_verifier_start());
      } else {
        setTransfer(initialization);
        requireOk(raw.clearra_wasm_distributed_forward_verifier_start());
      }
    },
    distributed_verifier_consume(batch) {
      setTransfer(batch);
      const consumed = raw.clearra_wasm_distributed_verifier_consume();
      requireOk(consumed);
      return {
        candidateCount: consumed,
        partial:
          raw.clearra_wasm_distributed_verifier_partial_available() === 0
            ? null
            : outputBytes()
      };
    },
    distributed_verifier_progress() {
      return {
        candidateCount: raw.clearra_wasm_distributed_verifier_progress_candidate_count(),
        buildNodes: raw.clearra_wasm_distributed_verifier_progress_build_nodes(),
        coverageChecks: raw.clearra_wasm_distributed_verifier_progress_coverage_checks()
      };
    },
    distributed_verifier_finish() {
      requireOk(raw.clearra_wasm_distributed_verifier_finish());
      return outputBytes();
    },
    async prewarm_gpu(deviceIndex) {
      requireOk(raw.clearra_wasm_gpu_warmup_start(deviceIndex ?? -1));
      for (;;) {
        const status = raw.clearra_wasm_gpu_warmup_advance();
        requireOk(status);
        if (status === 1) return 'connected';
        if (status === 2) return 'unavailable';
        if (status !== 0) throw new Error(`invalid GPU warmup status: ${status}`);
        await yieldToRuntimeHost();
      }
    }
  };
  if (raw.clearra_wasm_profile_start && raw.clearra_wasm_profile_finish) {
    module.profile_start = () => requireOk(raw.clearra_wasm_profile_start!());
    module.profile_finish = () => {
      requireOk(raw.clearra_wasm_profile_finish!());
      return JSON.parse(outputText()) as unknown;
    };
  }
  return module;
}

const yieldToRuntimeHost = createRuntimeHostYield();

function createRuntimeHostYield(): () => Promise<void> {
  const channel = new MessageChannel();
  const pending: Array<() => void> = [];
  channel.port1.onmessage = () => pending.shift()?.();
  return () =>
    new Promise<void>((resolve) => {
      pending.push(resolve);
      channel.port2.postMessage(undefined);
    });
}

function detectHostCapabilities(): ClearraWasmHostCapabilities {
  return {
    logicalProcessorCount: Math.max(1, self.navigator.hardwareConcurrency || 1),
    webGpuAvailable: 'gpu' in self.navigator,
    crossOriginIsolated: self.crossOriginIsolated === true
  };
}

function readCandidateFamilyCount(raw: ClearraRawWasmExports): string | null {
  if (raw.clearra_wasm_distributed_progress_candidate_family_count_available() === 0) return null;
  let value = 0n;
  for (let word = 3; word >= 0; word -= 1) {
    value =
      (value << 32n) |
      BigInt(raw.clearra_wasm_distributed_progress_candidate_family_count_word(word) >>> 0);
  }
  return value.toString();
}
