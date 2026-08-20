import type { ClearraProductBuildIdentity } from '@clearra/ui/wasm';

// SRP rationale: this module has one behavior-level change reason: adapting the validated
// WASM ABI exports into the browser runtime contract.
export type ClearraWasmModule = {
  compiled_module: () => WebAssembly.Module;
  configure_host: (capabilities: ClearraWasmHostCapabilities) => void;
  install_tablebase: (artifact: ArrayBuffer) => ClearraTablebaseInstallReport;
  release_tablebase: () => boolean;
  start_job: (commandText: string) => number;
  advance_job: (
    jobId: number,
    workBudget: number
  ) => 'pending' | 'progress' | 'completed' | 'cancelled' | 'failed';
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
  tiling_solution_count: () => number;
  tiling_solution_page: (offset: number, limit: number) => string[];
  tiling_solution_release: () => void;
  distributed_cancel: () => void;
  distributed_reset: () => void;
  distributed_verifier_start: (initialization: string | ArrayBuffer) => void;
  distributed_verifier_consume: (batch: ArrayBuffer) => ClearraDistributedVerifierConsume;
  distributed_verifier_continue: () => ClearraDistributedVerifierConsume;
  distributed_verifier_progress: () => ClearraDistributedVerifierProgress;
  distributed_verifier_finish: () => ArrayBuffer;
  prewarm_gpu: (deviceIndex: number | null) => Promise<'connected' | 'unavailable'>;
  cancel_gpu_warmup: () => void;
  profile_start?: () => void;
  profile_finish?: () => unknown;
  failure_diagnostics: () => ClearraWasmFailureDiagnostics;
};

export type ClearraTablebaseInstallReport = {
  schema_version: 12;
  tier: 'compact-exact';
  artifact_bytes: number;
  certified_states: number;
  certified_targets: number;
  payload_sha256: string;
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
  deferredInitialization: boolean;
  verificationRequired: boolean;
  tilingGeometryParallel: boolean;
};

export type ClearraDistributedVerifierConsume = {
  candidateCount: number;
  candidateCountAvailable: boolean;
  candidateCountExact: boolean;
  partial: ArrayBuffer | null;
  hasPendingWork: boolean;
};

export type ClearraDistributedProducerResult =
  | { status: 'pending' | 'completed' | 'cancelled' }
  | { status: 'initialization'; initialization: ArrayBuffer }
  | { status: 'batch'; batch: ArrayBuffer };

export type ClearraDistributedCoreProgress = {
  geometryNodes: number;
  candidateCount: number;
  candidateFamilyCount: string | null;
  buildNodes: number;
  coverageChecks: number;
  passIndex: number;
  passCount: number;
  layerIndex: number;
  layerCount: number;
  layerDone: number;
  layerTotal: number;
  availability: ClearraDistributedCoreProgressFlags;
  exactness: ClearraDistributedCoreProgressFlags;
};

export type ClearraDistributedCoreProgressFlags = {
  geometryNodes: boolean;
  candidateCount: boolean;
  candidateFamilyCount: boolean;
  buildNodes: boolean;
  coverageChecks: boolean;
  passIndex: boolean;
  passCount: boolean;
  layerIndex: boolean;
  layerCount: boolean;
  layerDone: boolean;
  layerTotal: boolean;
};

export type ClearraDistributedVerifierProgress = {
  candidateCount: number;
  buildNodes: number;
  coverageChecks: number;
  availability: ClearraDistributedVerifierProgressFlags;
  exactness: ClearraDistributedVerifierProgressFlags;
};

export type ClearraDistributedVerifierProgressFlags = {
  candidateCount: boolean;
  buildNodes: boolean;
  coverageChecks: boolean;
};

export type ClearraWasmHostCapabilities = {
  logicalProcessorCount: number;
  webGpuAvailable: boolean;
  crossOriginIsolated: boolean;
  transferByteCap: number;
};

let wasmModulePromise: Promise<ClearraWasmModule> | null = null;
const CONSERVATIVE_HOST_CAPABILITIES: ClearraWasmHostCapabilities = Object.freeze({
  logicalProcessorCount: 1,
  webGpuAvailable: false,
  crossOriginIsolated: false,
  transferByteCap: 32 * 1024 * 1024
});
const ARTIFACT_NETWORK_TIMEOUT_MS = 30_000;
const ARTIFACT_MODULE_TIMEOUT_MS = 60_000;

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
  clearra_wasm_tablebase_install: () => number;
  clearra_wasm_tablebase_release: () => number;
  clearra_wasm_distributed_prepare: () => number;
  clearra_wasm_distributed_worker_initialization: () => number;
  clearra_wasm_distributed_worker_initialization_deferred: () => number;
  clearra_wasm_distributed_worker_count: () => number;
  clearra_wasm_distributed_worker_count_available: () => number;
  clearra_wasm_distributed_worker_count_exact: () => number;
  clearra_wasm_distributed_verification_required: () => number;
  clearra_wasm_distributed_tiling_geometry_parallel: () => number;
  clearra_wasm_distributed_requested_backend: () => number;
  clearra_wasm_distributed_preparation_fallback_reason: () => number;
  clearra_wasm_distributed_produce: (
    workBudget: number,
    batchCapacity: number
  ) => number;
  clearra_wasm_distributed_progress_geometry_nodes: () => number;
  clearra_wasm_distributed_progress_available: () => number;
  clearra_wasm_distributed_progress_geometry_nodes_exact: () => number;
  clearra_wasm_distributed_progress_candidate_count: () => number;
  clearra_wasm_distributed_progress_candidate_count_exact: () => number;
  clearra_wasm_distributed_progress_build_nodes: () => number;
  clearra_wasm_distributed_progress_build_nodes_exact: () => number;
  clearra_wasm_distributed_progress_coverage_checks: () => number;
  clearra_wasm_distributed_progress_coverage_checks_exact: () => number;
  clearra_wasm_distributed_progress_candidate_family_count_available: () => number;
  clearra_wasm_distributed_progress_candidate_family_count_exact: () => number;
  clearra_wasm_distributed_progress_candidate_family_count_word: (wordIndex: number) => number;
  clearra_wasm_distributed_progress_pass_index: () => number;
  clearra_wasm_distributed_progress_pass_index_exact: () => number;
  clearra_wasm_distributed_progress_pass_count: () => number;
  clearra_wasm_distributed_progress_pass_count_exact: () => number;
  clearra_wasm_distributed_progress_layer_index: () => number;
  clearra_wasm_distributed_progress_layer_index_exact: () => number;
  clearra_wasm_distributed_progress_layer_count: () => number;
  clearra_wasm_distributed_progress_layer_count_exact: () => number;
  clearra_wasm_distributed_progress_layer_done: () => number;
  clearra_wasm_distributed_progress_layer_done_exact: () => number;
  clearra_wasm_distributed_progress_layer_total: () => number;
  clearra_wasm_distributed_progress_layer_total_exact: () => number;
  clearra_wasm_distributed_merge_partial: () => number;
  clearra_wasm_distributed_finish: (jobId: number, workersUsed: number) => number;
  clearra_wasm_tiling_solution_count: () => number;
  clearra_wasm_tiling_solution_count_available: () => number;
  clearra_wasm_tiling_solution_count_exact: () => number;
  clearra_wasm_tiling_solution_page: (offset: number, limit: number) => number;
  clearra_wasm_tiling_solution_release: () => number;
  clearra_wasm_distributed_cancel: () => number;
  clearra_wasm_distributed_reset: () => number;
  clearra_wasm_distributed_verifier_start: () => number;
  clearra_wasm_distributed_forward_verifier_start: () => number;
  clearra_wasm_distributed_verifier_consume: () => number;
  clearra_wasm_distributed_verifier_partial_available: () => number;
  clearra_wasm_distributed_verifier_pending_work: () => number;
  clearra_wasm_distributed_verifier_last_candidate_count_available: () => number;
  clearra_wasm_distributed_verifier_last_candidate_count_exact: () => number;
  clearra_wasm_distributed_verifier_continue: () => number;
  clearra_wasm_distributed_verifier_progress_candidate_count: () => number;
  clearra_wasm_distributed_verifier_progress_available: () => number;
  clearra_wasm_distributed_verifier_progress_candidate_count_exact: () => number;
  clearra_wasm_distributed_verifier_progress_build_nodes: () => number;
  clearra_wasm_distributed_verifier_progress_build_nodes_exact: () => number;
  clearra_wasm_distributed_verifier_progress_coverage_checks: () => number;
  clearra_wasm_distributed_verifier_progress_coverage_checks_exact: () => number;
  clearra_wasm_distributed_verifier_finish: () => number;
  clearra_wasm_gpu_warmup_start: (deviceIndex: number) => number;
  clearra_wasm_gpu_warmup_advance: () => number;
  clearra_wasm_gpu_warmup_cancel: () => number;
  clearra_wasm_start_job: () => number;
  clearra_wasm_advance_job: (jobId: number, workBudget: number) => number;
  clearra_wasm_cancel_job: (jobId: number) => number;
  clearra_wasm_drain_job_events: (jobId: number) => number;
  clearra_wasm_output_ptr: () => number;
  clearra_wasm_output_len: () => number;
  clearra_wasm_output_len_exact: () => number;
  clearra_wasm_last_panic_ptr: () => number;
  clearra_wasm_last_panic_len: () => number;
  clearra_wasm_last_panic_len_exact: () => number;
  clearra_wasm_profile_start?: () => number;
  clearra_wasm_profile_finish?: () => number;
};

export const CLEARRA_WASM_AVAILABILITY_EXACTNESS_EXPORTS = Object.freeze([
  'clearra_wasm_distributed_worker_count_available',
  'clearra_wasm_distributed_worker_count_exact',
  'clearra_wasm_distributed_progress_available',
  'clearra_wasm_distributed_progress_geometry_nodes_exact',
  'clearra_wasm_distributed_progress_candidate_count_exact',
  'clearra_wasm_distributed_progress_candidate_family_count_available',
  'clearra_wasm_distributed_progress_candidate_family_count_exact',
  'clearra_wasm_distributed_progress_build_nodes_exact',
  'clearra_wasm_distributed_progress_coverage_checks_exact',
  'clearra_wasm_distributed_progress_pass_index_exact',
  'clearra_wasm_distributed_progress_pass_count_exact',
  'clearra_wasm_distributed_progress_layer_index_exact',
  'clearra_wasm_distributed_progress_layer_count_exact',
  'clearra_wasm_distributed_progress_layer_done_exact',
  'clearra_wasm_distributed_progress_layer_total_exact',
  'clearra_wasm_tiling_solution_count_available',
  'clearra_wasm_tiling_solution_count_exact',
  'clearra_wasm_distributed_verifier_last_candidate_count_available',
  'clearra_wasm_distributed_verifier_last_candidate_count_exact',
  'clearra_wasm_distributed_verifier_progress_available',
  'clearra_wasm_distributed_verifier_progress_candidate_count_exact',
  'clearra_wasm_distributed_verifier_progress_build_nodes_exact',
  'clearra_wasm_distributed_verifier_progress_coverage_checks_exact',
  'clearra_wasm_output_len_exact',
  'clearra_wasm_last_panic_len_exact'
] as const);

type ClearraWasmBindings = {
  default: (input?: {
    module_or_path: string | URL | WebAssembly.Module;
  }) => Promise<ClearraRawWasmExports>;
};

type ClearraWasmArtifactManifest = {
  schema_version: 1;
  // The worker also enforces the capability contract. This closes the gap
  // where a long-running Vite process can serve a freshly HMR-updated GUI with
  // an older, otherwise valid WASM generation.
  build: {
    contract_version: number;
    source_sha256: string;
    source_file_count: number;
    capabilities_sha256: string;
    runtime_identity: ClearraProductBuildIdentity;
  };
  bindings: ClearraWasmArtifact;
  wasm: ClearraWasmArtifact;
};

type ClearraWasmArtifact = {
  path: string;
  bytes: number;
  sha256: string;
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

export async function loadClearraWasmModule(
  sharedCompiledModule?: WebAssembly.Module,
  hostCapabilities: ClearraWasmHostCapabilities = CONSERVATIVE_HOST_CAPABILITIES
): Promise<ClearraWasmModule> {
  if (!wasmModulePromise) {
    wasmModulePromise = loadClearraWasmArtifactGeneration(sharedCompiledModule);
  }
  const attempt = wasmModulePromise;
  try {
    const module = await attempt;
    module.configure_host(hostCapabilities);
    return module;
  } catch (error) {
    if (wasmModulePromise === attempt) wasmModulePromise = null;
    throw error;
  }
}

async function loadClearraWasmArtifactGeneration(
  sharedCompiledModule?: WebAssembly.Module
): Promise<ClearraWasmModule> {
  let firstFailure: unknown = null;
  for (let attempt = 0; attempt < 2; attempt += 1) {
    try {
      const wasmRoot = `${deploymentBaseFromWorkerLocation(self.location.pathname)}/wasm`;
      const manifestUrl = new URL(`${wasmRoot}/clearra_wasm.manifest.json`, self.location.origin);
      const manifest = await withArtifactDeadline(
        'Clearra WASM manifest fetch',
        ARTIFACT_NETWORK_TIMEOUT_MS,
        async (signal) => {
          const response = await fetch(manifestUrl, { cache: 'no-store', signal });
          if (!response.ok) {
            throw new Error(`Clearra WASM manifest unavailable: ${response.status}`);
          }
          return (await response.json()) as ClearraWasmArtifactManifest;
        }
      );
      if (!isArtifactManifest(manifest)) {
        throw new Error('Clearra WASM manifest is invalid');
      }
      const bindingsUrl = new URL(`${wasmRoot}/${manifest.bindings.path}`, self.location.origin);
      const wasmUrl = new URL(`${wasmRoot}/${manifest.wasm.path}`, self.location.origin);
      bindingsUrl.searchParams.set('v', manifest.bindings.sha256);
      wasmUrl.searchParams.set('v', manifest.wasm.sha256);
      if (attempt !== 0) {
        bindingsUrl.searchParams.set('retry', String(attempt));
        wasmUrl.searchParams.set('retry', String(attempt));
      }
      const bindings = await importClearraWasmBindings(bindingsUrl, manifest.bindings);
      const compiledModule =
        sharedCompiledModule ?? (await compileClearraWasmModule(wasmUrl, manifest.wasm));
      const raw = await withArtifactDeadline(
        'Clearra WASM instantiation',
        ARTIFACT_MODULE_TIMEOUT_MS,
        () => bindings.default({ module_or_path: compiledModule })
      );
      return wrapRawModule(raw, compiledModule, manifest.build.runtime_identity);
    } catch (error) {
      if (isArtifactTimeout(error)) throw error;
      if (attempt !== 0) {
        throw new Error('Clearra WASM artifact generation could not be loaded after a fresh retry', {
          cause: error
        });
      }
      firstFailure = error;
    }
  }
  throw firstFailure;
}

async function importClearraWasmBindings(
  bindingsUrl: URL,
  artifact: ClearraWasmArtifact
): Promise<ClearraWasmBindings> {
  try {
    return await withArtifactDeadline(
      'Clearra WASM bindings import',
      ARTIFACT_MODULE_TIMEOUT_MS,
      async () =>
        (await import(
          /* @vite-ignore */ bindingsUrl.href
        )) as ClearraWasmBindings
    );
  } catch (error) {
    if (isArtifactTimeout(error)) throw error;
    const bytes = await fetchVerifiedArtifactBytes(bindingsUrl, artifact);
    const blobUrl = URL.createObjectURL(new Blob([bytes], { type: 'text/javascript' }));
    try {
      return await withArtifactDeadline(
        'verified Clearra WASM bindings import',
        ARTIFACT_MODULE_TIMEOUT_MS,
        async () =>
          (await import(
            /* @vite-ignore */ blobUrl
          )) as ClearraWasmBindings
      );
    } finally {
      URL.revokeObjectURL(blobUrl);
    }
  }
}

async function compileClearraWasmModule(
  wasmUrl: URL,
  artifact: ClearraWasmArtifact
): Promise<WebAssembly.Module> {
  if (typeof WebAssembly.compileStreaming === 'function') {
    try {
      return await withArtifactDeadline(
        'Clearra WASM streaming compile',
        ARTIFACT_MODULE_TIMEOUT_MS,
        async (signal) => {
          const response = await fetch(wasmUrl, { cache: 'no-store', signal });
          if (!response.ok) {
            throw new Error(`Clearra WASM artifact unavailable: ${response.status}`);
          }
          return WebAssembly.compileStreaming(response);
        }
      );
    } catch (error) {
      if (isArtifactTimeout(error)) throw error;
    }
  }
  const bytes = await fetchVerifiedArtifactBytes(wasmUrl, artifact);
  return withArtifactDeadline(
    'verified Clearra WASM compile',
    ARTIFACT_MODULE_TIMEOUT_MS,
    () => WebAssembly.compile(bytes)
  );
}

export class ClearraWasmTransferLimitError extends ClearraWasmRuntimeError {
  constructor(
    public readonly requestedBytes: number,
    public readonly limitBytes: number
  ) {
    super(
      'E_WASM_TRANSFER_HOST_LIMIT',
      `WASM transfer requires ${requestedBytes} bytes, above the host snapshot limit of ${limitBytes} bytes`
    );
    this.name = 'ClearraWasmTransferLimitError';
  }
}

export function assertWasmTransferWithinHostCap(
  byteLength: number,
  transferByteCap: number
): void {
  const requested = Number.isSafeInteger(byteLength) && byteLength >= 0
    ? byteLength
    : Number.MAX_SAFE_INTEGER;
  const limit = Number.isSafeInteger(transferByteCap) && transferByteCap >= 1
    ? transferByteCap
    : 1;
  if (requested > limit) throw new ClearraWasmTransferLimitError(requested, limit);
}

/** WebAssembly exposes `u32` results to JavaScript as signed i32 values. */
export function normalizeWasmU32(value: number): number {
  return value >>> 0;
}

export function assertClearraWasmAvailabilityExactnessExports(
  raw: unknown
): void {
  const candidate =
    raw !== null && typeof raw === 'object'
      ? (raw as Record<string, unknown>)
      : {};
  const missing = CLEARRA_WASM_AVAILABILITY_EXACTNESS_EXPORTS.filter(
    (name) => typeof candidate[name] !== 'function'
  );
  if (missing.length === 0) return;
  throw new ClearraWasmRuntimeError(
    'E_WASM_CAPABILITY_MISSING',
    `Clearra WASM availability/exactness ABI v1 is incomplete: ${missing.join(', ')}`
  );
}

async function fetchVerifiedArtifactBytes(
  artifactUrl: URL,
  artifact: ClearraWasmArtifact
): Promise<ArrayBuffer> {
  return withArtifactDeadline(
    'Clearra WASM artifact verification',
    ARTIFACT_NETWORK_TIMEOUT_MS,
    async (signal) => {
      const response = await fetch(artifactUrl, { cache: 'no-store', signal });
      if (!response.ok) {
        throw new Error(`Clearra WASM artifact unavailable: ${response.status}`);
      }
      const bytes = await response.arrayBuffer();
      if (bytes.byteLength !== artifact.bytes) {
        throw new Error(
          `Clearra WASM artifact length mismatch: expected ${artifact.bytes}, received ${bytes.byteLength}`
        );
      }
      const digest = await crypto.subtle.digest('SHA-256', bytes);
      const actualSha256 = [...new Uint8Array(digest)]
        .map((byte) => byte.toString(16).padStart(2, '0'))
        .join('');
      if (actualSha256 !== artifact.sha256) {
        throw new Error('Clearra WASM artifact SHA-256 mismatch');
      }
      return bytes;
    }
  );
}

export async function withArtifactDeadline<T>(
  label: string,
  timeoutMs: number,
  operation: (signal: AbortSignal) => Promise<T>
): Promise<T> {
  const controller = new AbortController();
  const timeoutError = new ClearraWasmRuntimeError(
    'E_WASM_MODULE_LOAD_TIMEOUT',
    `${label} timed out after ${timeoutMs} ms`
  );
  let timeout: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      Promise.resolve().then(() => operation(controller.signal)),
      new Promise<never>((_, reject) => {
        timeout = setTimeout(() => {
          controller.abort(timeoutError);
          reject(timeoutError);
        }, timeoutMs);
      })
    ]);
  } catch (error) {
    if (controller.signal.aborted) throw timeoutError;
    throw error;
  } finally {
    if (timeout !== undefined) clearTimeout(timeout);
  }
}

function isArtifactTimeout(error: unknown): boolean {
  return (
    error instanceof ClearraWasmRuntimeError &&
    error.diagnosticCode === 'E_WASM_MODULE_LOAD_TIMEOUT'
  );
}

function deploymentBaseFromWorkerLocation(pathname: string): string {
  const appMarker = '/_app/';
  const appIndex = pathname.lastIndexOf(appMarker);
  return appIndex < 0 ? '' : pathname.slice(0, appIndex);
}

function isSha256(value: string): boolean {
  return /^[0-9a-f]{64}$/.test(value);
}

function isArtifactManifest(manifest: unknown): manifest is ClearraWasmArtifactManifest {
  if (!manifest || typeof manifest !== 'object') return false;
  const candidate = manifest as Partial<ClearraWasmArtifactManifest>;
  return (
    candidate.schema_version === 1 &&
    isBuildContract(candidate.build) &&
    isArtifact(candidate.bindings, 'clearra_wasm.js', 'clearra_wasm', '.js') &&
    isArtifact(candidate.wasm, 'clearra_wasm_bg.wasm', 'clearra_wasm_bg', '.wasm')
  );
}

const REQUIRED_WASM_CAPABILITIES_SHA256 =
  '30a6cc08ce00320997ccf86982a1b6770d67ff7e1f7aeabb8bb22dea77dbaa0d';

const PRODUCT_BUILD_IDENTITY = {
  contract_schema_version: 'clearra.search.contract.v2',
  supply_semantics_id: 'clearra.supply.projected-terminal-lookahead.v1',
  artifact_schema_version: 'clearra.solution-data.v1',
  unverified_build_id: 'unverified-local-build'
} as const;

function isBuildContract(value: unknown): value is ClearraWasmArtifactManifest['build'] {
  if (!value || typeof value !== 'object') return false;
  const candidate = value as Partial<ClearraWasmArtifactManifest['build']>;
  return (
    candidate.contract_version === 2 &&
    typeof candidate.source_sha256 === 'string' &&
    isSha256(candidate.source_sha256) &&
    Number.isSafeInteger(candidate.source_file_count) &&
    Number(candidate.source_file_count) > 0 &&
    candidate.capabilities_sha256 === REQUIRED_WASM_CAPABILITIES_SHA256 &&
    isProductBuildIdentity(candidate.runtime_identity)
  );
}

function isProductBuildIdentity(value: unknown): value is ClearraProductBuildIdentity {
  if (!value || typeof value !== 'object') return false;
  const candidate = value as Partial<ClearraProductBuildIdentity>;
  const pinned =
    isCommit(candidate.source_commit) && isCommit(candidate.engine_build_id);
  const local =
    candidate.source_commit === PRODUCT_BUILD_IDENTITY.unverified_build_id &&
    candidate.engine_build_id === PRODUCT_BUILD_IDENTITY.unverified_build_id;
  return (
    (pinned || local) &&
    candidate.contract_schema_version === PRODUCT_BUILD_IDENTITY.contract_schema_version &&
    candidate.supply_semantics_id === PRODUCT_BUILD_IDENTITY.supply_semantics_id &&
    candidate.artifact_schema_version === PRODUCT_BUILD_IDENTITY.artifact_schema_version
  );
}

function isCommit(value: unknown): value is string {
  return typeof value === 'string' && /^[0-9a-f]{40}$/u.test(value);
}

export function assertClearraWasmTerminalResponseIdentities(
  output: string,
  expected: ClearraProductBuildIdentity
): string {
  let events: unknown;
  try {
    events = JSON.parse(output);
  } catch (error) {
    throw new ClearraWasmRuntimeError(
      'E_WASM_RUNTIME_IDENTITY_INVALID',
      `Clearra WASM emitted an invalid event envelope: ${String(error)}`
    );
  }
  if (!Array.isArray(events)) {
    throw new ClearraWasmRuntimeError(
      'E_WASM_RUNTIME_IDENTITY_INVALID',
      'Clearra WASM emitted a non-array event envelope'
    );
  }
  for (const event of events) {
    if (!event || typeof event !== 'object' || Reflect.get(event, 'event') !== 'final_response') {
      continue;
    }
    const response = Reflect.get(event, 'response');
    const actual =
      response && typeof response === 'object'
        ? Reflect.get(response, 'runtime_identity')
        : null;
    if (!productBuildIdentitiesEqual(actual, expected)) {
      throw new ClearraWasmRuntimeError(
        'E_WASM_RUNTIME_IDENTITY_MISMATCH',
        'Clearra WASM response identity does not match the loaded artifact manifest'
      );
    }
  }
  return output;
}

function productBuildIdentitiesEqual(
  actual: unknown,
  expected: ClearraProductBuildIdentity
): boolean {
  if (!isProductBuildIdentity(actual) || !isProductBuildIdentity(expected)) return false;
  return (
    actual.source_commit === expected.source_commit &&
    actual.engine_build_id === expected.engine_build_id &&
    actual.contract_schema_version === expected.contract_schema_version &&
    actual.supply_semantics_id === expected.supply_semantics_id &&
    actual.artifact_schema_version === expected.artifact_schema_version
  );
}

function isArtifact(
  value: unknown,
  legacyPath: string,
  versionedPrefix: string,
  versionedSuffix: string
): value is ClearraWasmArtifact {
  if (!value || typeof value !== 'object') return false;
  const candidate = value as Partial<ClearraWasmArtifact>;
  const versionedPaths =
    typeof candidate.sha256 === 'string'
      ? [20, 24, 64].map(
          (length) =>
            `${versionedPrefix}.${candidate.sha256!.slice(0, length)}${versionedSuffix}`
        )
      : [];
  return (
    (candidate.path === legacyPath || versionedPaths.includes(candidate.path ?? '')) &&
    Number.isSafeInteger(candidate.bytes) &&
    Number(candidate.bytes) > 0 &&
    typeof candidate.sha256 === 'string' &&
    isSha256(candidate.sha256)
  );
}

function wrapRawModule(
  raw: ClearraRawWasmExports,
  compiledModule: WebAssembly.Module,
  expectedRuntimeIdentity: ClearraProductBuildIdentity
): ClearraWasmModule {
  assertClearraWasmAvailabilityExactnessExports(raw);
  if (raw.clearra_wasm_abi_version() !== 1) {
    throw new Error('unsupported Clearra WASM ABI version');
  }

  let hostTransferByteCap = CONSERVATIVE_HOST_CAPABILITIES.transferByteCap;
  const outputText = () => {
    requireExactLength(
      raw.clearra_wasm_output_len_exact(),
      'E_WASM_OUTPUT_TOO_LARGE',
      'WASM output length exceeds the exact host ABI range'
    );
    const ptr = raw.clearra_wasm_output_ptr() >>> 0;
    const len = raw.clearra_wasm_output_len() >>> 0;
    return decoder.decode(new Uint8Array(raw.memory.buffer, ptr, len));
  };
  const outputBytes = () => {
    requireExactLength(
      raw.clearra_wasm_output_len_exact(),
      'E_WASM_OUTPUT_TOO_LARGE',
      'WASM output length exceeds the exact host ABI range'
    );
    const ptr = raw.clearra_wasm_output_ptr() >>> 0;
    const len = raw.clearra_wasm_output_len() >>> 0;
    return new Uint8Array(raw.memory.buffer, ptr, len).slice().buffer;
  };
  const lastPanic = () => {
    requireExactLength(
      raw.clearra_wasm_last_panic_len_exact(),
      'E_WASM_PANIC_TOO_LARGE',
      'WASM panic length exceeds the exact host ABI range'
    );
    const ptr = raw.clearra_wasm_last_panic_ptr() >>> 0;
    const len = raw.clearra_wasm_last_panic_len() >>> 0;
    return len === 0 ? null : decoder.decode(new Uint8Array(raw.memory.buffer, ptr, len));
  };
  const requireOk = (status: number) => {
    if (status < 0) throw ClearraWasmRuntimeError.fromRuntimeOutput(outputText());
  };
  const setCommand = (commandText: string) => {
    const bytes = encoder.encode(commandText);
    requireOk(raw.clearra_wasm_input_resize(bytes.byteLength));
    const ptr = raw.clearra_wasm_input_ptr() >>> 0;
    new Uint8Array(raw.memory.buffer, ptr, bytes.byteLength).set(bytes);
  };
  const setTransfer = (input: ArrayBuffer) => {
    assertWasmTransferWithinHostCap(input.byteLength, hostTransferByteCap);
    requireOk(raw.clearra_wasm_transfer_resize(input.byteLength));
    const ptr = raw.clearra_wasm_transfer_ptr() >>> 0;
    new Uint8Array(raw.memory.buffer, ptr, input.byteLength).set(new Uint8Array(input));
  };
  let gpuWarmupGeneration = 0;

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
      hostTransferByteCap = capabilities.transferByteCap;
    },
    install_tablebase(artifact) {
      setTransfer(artifact);
      requireOk(raw.clearra_wasm_tablebase_install());
      const report = JSON.parse(outputText()) as ClearraTablebaseInstallReport;
      if (
        report.schema_version !== 12 ||
        report.tier !== 'compact-exact' ||
        report.artifact_bytes !== artifact.byteLength ||
        report.certified_targets !== 4_795 ||
        !isSha256(report.payload_sha256)
      ) {
        throw new ClearraWasmRuntimeError(
          'E_WASM_TABLEBASE_INSTALL',
          'installed tablebase metadata is invalid'
        );
      }
      return report;
    },
    release_tablebase() {
      const status = raw.clearra_wasm_tablebase_release();
      requireOk(status);
      return status === 1;
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
      const labels = {
        0: 'pending',
        1: 'completed',
        2: 'cancelled',
        3: 'failed',
        4: 'progress'
      } as const;
      const label = labels[status as keyof typeof labels];
      if (!label) throw new Error(`invalid Clearra WASM advance status: ${status}`);
      return label;
    },
    cancel_job(jobId) {
      requireOk(raw.clearra_wasm_cancel_job(jobId));
    },
    drain_job_events_json(jobId) {
      requireOk(raw.clearra_wasm_drain_job_events(jobId));
      return assertClearraWasmTerminalResponseIdentities(
        outputText(),
        expectedRuntimeIdentity
      );
    },
    distributed_prepare(commandText) {
      setCommand(commandText);
      const mode = raw.clearra_wasm_distributed_prepare();
      requireOk(mode);
      const labels = ['serial', 'cpu-multi', 'gpu-multi'] as const;
      const requestedLabels = ['auto', 'cpu', 'gpu', 'hybrid'] as const;
      const fallbackReasonCode = raw.clearra_wasm_distributed_preparation_fallback_reason();
      const fallbackReasonLabels = {
        1: 'gpu_kernel_unavailable',
        2: 'gpu_device_not_found'
      } as const;
      const selectedMode = labels[mode] ?? 'serial';
      requireExactCount(
        raw.clearra_wasm_distributed_worker_count_available(),
        raw.clearra_wasm_distributed_worker_count_exact(),
        'distributed worker count'
      );
      requireOk(raw.clearra_wasm_distributed_worker_initialization());
      const initialization = outputBytes();
      return {
        mode: selectedMode,
        workerCount: Math.max(1, raw.clearra_wasm_distributed_worker_count()),
        requestedBackend:
          requestedLabels[raw.clearra_wasm_distributed_requested_backend()] ?? 'auto',
        selectedBackend: selectedMode === 'gpu-multi' ? 'webgpu' : 'wasm-cpu',
        fallbackUsed: fallbackReasonCode !== 0,
        fallbackReason:
          fallbackReasonLabels[fallbackReasonCode as keyof typeof fallbackReasonLabels] ?? null,
        workerInitialization: initialization.byteLength === 0 ? null : initialization,
        deferredInitialization:
          raw.clearra_wasm_distributed_worker_initialization_deferred() !== 0,
        verificationRequired:
          raw.clearra_wasm_distributed_verification_required() !== 0,
        tilingGeometryParallel:
          raw.clearra_wasm_distributed_tiling_geometry_parallel() !== 0
      };
    },
    distributed_produce(workBudget, batchCapacity) {
      const status = raw.clearra_wasm_distributed_produce(workBudget, batchCapacity);
      requireOk(status);
      if (status === 1) return { status: 'batch', batch: outputBytes() };
      if (status === 2) return { status: 'completed' };
      if (status === 3) return { status: 'cancelled' };
      if (status === 4) {
        return { status: 'initialization', initialization: outputBytes() };
      }
      if (status !== 0) throw new Error(`invalid distributed producer status: ${status}`);
      return { status: 'pending' };
    },
    distributed_progress() {
      const available = raw.clearra_wasm_distributed_progress_available() !== 0;
      const candidateFamilyAvailable =
        available &&
        raw.clearra_wasm_distributed_progress_candidate_family_count_available() !== 0;
      return {
        geometryNodes: normalizeWasmU32(
          raw.clearra_wasm_distributed_progress_geometry_nodes()
        ),
        candidateCount: normalizeWasmU32(
          raw.clearra_wasm_distributed_progress_candidate_count()
        ),
        candidateFamilyCount: candidateFamilyAvailable
          ? readCandidateFamilyCount(raw)
          : null,
        buildNodes: normalizeWasmU32(
          raw.clearra_wasm_distributed_progress_build_nodes()
        ),
        coverageChecks: normalizeWasmU32(
          raw.clearra_wasm_distributed_progress_coverage_checks()
        ),
        passIndex: normalizeWasmU32(
          raw.clearra_wasm_distributed_progress_pass_index()
        ),
        passCount: Math.max(
          1,
          normalizeWasmU32(raw.clearra_wasm_distributed_progress_pass_count())
        ),
        layerIndex: normalizeWasmU32(
          raw.clearra_wasm_distributed_progress_layer_index()
        ),
        layerCount: normalizeWasmU32(
          raw.clearra_wasm_distributed_progress_layer_count()
        ),
        layerDone: normalizeWasmU32(
          raw.clearra_wasm_distributed_progress_layer_done()
        ),
        layerTotal: normalizeWasmU32(
          raw.clearra_wasm_distributed_progress_layer_total()
        ),
        availability: {
          geometryNodes: available,
          candidateCount: available,
          candidateFamilyCount: candidateFamilyAvailable,
          buildNodes: available,
          coverageChecks: available,
          passIndex: available,
          passCount: available,
          layerIndex: available,
          layerCount: available,
          layerDone: available,
          layerTotal: available
        },
        exactness: {
          geometryNodes:
            available && raw.clearra_wasm_distributed_progress_geometry_nodes_exact() !== 0,
          candidateCount:
            available && raw.clearra_wasm_distributed_progress_candidate_count_exact() !== 0,
          candidateFamilyCount:
            candidateFamilyAvailable &&
            raw.clearra_wasm_distributed_progress_candidate_family_count_exact() !== 0,
          buildNodes:
            available && raw.clearra_wasm_distributed_progress_build_nodes_exact() !== 0,
          coverageChecks:
            available && raw.clearra_wasm_distributed_progress_coverage_checks_exact() !== 0,
          passIndex:
            available && raw.clearra_wasm_distributed_progress_pass_index_exact() !== 0,
          passCount:
            available && raw.clearra_wasm_distributed_progress_pass_count_exact() !== 0,
          layerIndex:
            available && raw.clearra_wasm_distributed_progress_layer_index_exact() !== 0,
          layerCount:
            available && raw.clearra_wasm_distributed_progress_layer_count_exact() !== 0,
          layerDone:
            available && raw.clearra_wasm_distributed_progress_layer_done_exact() !== 0,
          layerTotal:
            available && raw.clearra_wasm_distributed_progress_layer_total_exact() !== 0
        }
      };
    },
    distributed_merge_partial(partial) {
      setTransfer(partial);
      requireOk(raw.clearra_wasm_distributed_merge_partial());
    },
    distributed_finish(jobId, workersUsed) {
      requireOk(raw.clearra_wasm_distributed_finish(jobId, workersUsed));
      return assertClearraWasmTerminalResponseIdentities(
        outputText(),
        expectedRuntimeIdentity
      );
    },
    tiling_solution_count() {
      requireExactCount(
        raw.clearra_wasm_tiling_solution_count_available(),
        raw.clearra_wasm_tiling_solution_count_exact(),
        'tiling solution count'
      );
      return raw.clearra_wasm_tiling_solution_count() >>> 0;
    },
    tiling_solution_page(offset, limit) {
      requireOk(raw.clearra_wasm_tiling_solution_page(offset, limit));
      return JSON.parse(outputText()) as string[];
    },
    tiling_solution_release() {
      requireOk(raw.clearra_wasm_tiling_solution_release());
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
      const candidateCountAvailable =
        raw.clearra_wasm_distributed_verifier_last_candidate_count_available() !== 0;
      const candidateCountExact =
        candidateCountAvailable &&
        raw.clearra_wasm_distributed_verifier_last_candidate_count_exact() !== 0;
      return {
        candidateCount: consumed,
        candidateCountAvailable,
        candidateCountExact,
        partial:
          raw.clearra_wasm_distributed_verifier_partial_available() === 0
            ? null
            : outputBytes(),
        hasPendingWork: raw.clearra_wasm_distributed_verifier_pending_work() !== 0
      };
    },
    distributed_verifier_continue() {
      const consumed = raw.clearra_wasm_distributed_verifier_continue();
      requireOk(consumed);
      const candidateCountAvailable =
        raw.clearra_wasm_distributed_verifier_last_candidate_count_available() !== 0;
      const candidateCountExact =
        candidateCountAvailable &&
        raw.clearra_wasm_distributed_verifier_last_candidate_count_exact() !== 0;
      return {
        candidateCount: consumed,
        candidateCountAvailable,
        candidateCountExact,
        partial:
          raw.clearra_wasm_distributed_verifier_partial_available() === 0
            ? null
            : outputBytes(),
        hasPendingWork: raw.clearra_wasm_distributed_verifier_pending_work() !== 0
      };
    },
    distributed_verifier_progress() {
      const available =
        raw.clearra_wasm_distributed_verifier_progress_available() !== 0;
      return {
        candidateCount: normalizeWasmU32(
          raw.clearra_wasm_distributed_verifier_progress_candidate_count()
        ),
        buildNodes: normalizeWasmU32(
          raw.clearra_wasm_distributed_verifier_progress_build_nodes()
        ),
        coverageChecks: normalizeWasmU32(
          raw.clearra_wasm_distributed_verifier_progress_coverage_checks()
        ),
        availability: {
          candidateCount: available,
          buildNodes: available,
          coverageChecks: available
        },
        exactness: {
          candidateCount:
            available &&
            raw.clearra_wasm_distributed_verifier_progress_candidate_count_exact() !== 0,
          buildNodes:
            available &&
            raw.clearra_wasm_distributed_verifier_progress_build_nodes_exact() !== 0,
          coverageChecks:
            available &&
            raw.clearra_wasm_distributed_verifier_progress_coverage_checks_exact() !== 0
        }
      };
    },
    distributed_verifier_finish() {
      requireOk(raw.clearra_wasm_distributed_verifier_finish());
      return outputBytes();
    },
    async prewarm_gpu(deviceIndex) {
      const generation = ++gpuWarmupGeneration;
      requireOk(raw.clearra_wasm_gpu_warmup_start(deviceIndex ?? -1));
      for (;;) {
        if (generation !== gpuWarmupGeneration) {
          throw new ClearraWasmRuntimeError(
            'E_WASM_GPU_WARMUP_CANCELLED',
            'GPU warmup ownership was transferred to a foreground command'
          );
        }
        const status = raw.clearra_wasm_gpu_warmup_advance();
        requireOk(status);
        if (status === 1) return 'connected';
        if (status === 2) return 'unavailable';
        if (status !== 0) throw new Error(`invalid GPU warmup status: ${status}`);
        await yieldToRuntimeHost();
      }
    },
    cancel_gpu_warmup() {
      gpuWarmupGeneration += 1;
      requireOk(raw.clearra_wasm_gpu_warmup_cancel());
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
  const nodePort1 = channel.port1 as MessagePort & { unref?: () => void };
  const nodePort2 = channel.port2 as MessagePort & { unref?: () => void };
  const pending: Array<() => void> = [];
  channel.port1.onmessage = () => pending.shift()?.();
  nodePort1.unref?.();
  nodePort2.unref?.();
  return () =>
    new Promise<void>((resolve) => {
      pending.push(resolve);
      channel.port2.postMessage(undefined);
    });
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

function requireExactLength(
  exact: number,
  code: string,
  message: string
): void {
  if (exact !== 0) return;
  throw new ClearraWasmRuntimeError(code, message);
}

function requireExactCount(
  available: number,
  exact: number,
  label: string
): void {
  if (available === 0) {
    throw new ClearraWasmRuntimeError(
      'E_WASM_COUNT_UNAVAILABLE',
      `${label} is unavailable in the current WASM ownership state`
    );
  }
  if (exact === 0) {
    throw new ClearraWasmRuntimeError(
      'E_WASM_COUNT_INEXACT',
      `${label} exceeds the exact WASM ABI scalar range`
    );
  }
}
