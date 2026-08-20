export type HostCapabilitySnapshotSource =
  | 'browser-main'
  | 'host-provided'
  | 'conservative-fallback';

export type HostCapabilitySnapshot = Readonly<{
  schemaVersion: 1;
  snapshotId: string;
  source: HostCapabilitySnapshotSource;
  reportedLogicalProcessors: number;
  automaticWorkerCap: number;
  reportedDeviceMemoryGiB: number | null;
  wasmTransferByteCap: number;
  webGpuAvailable: boolean;
  crossOriginIsolated: boolean;
}>;

export type WorkerAuthorityReason =
  | 'reserved-main-thread'
  | 'all-logical-processors'
  | 'explicit-request'
  | 'host-cap'
  | 'invalid-request';

export type WorkerAuthorityReport = Readonly<{
  snapshotId: string;
  reportedLogicalProcessors: number;
  workersRequested: number;
  workersEffective: number;
  reason: WorkerAuthorityReason;
}>;

export type RuntimeWarmupPolicy = Readonly<{
  backend: 'auto' | 'cpu' | 'gpu' | 'hybrid';
  cpuWarmup: boolean;
  gpuWarmup: boolean;
}>;

export const HOST_CAPABILITY_SNAPSHOT_CONTEXT = Symbol.for(
  '@clearra/ui/host-capability-snapshot'
);

export const DEFAULT_RUNTIME_WARMUP_POLICY: RuntimeWarmupPolicy = Object.freeze({
  backend: 'auto',
  cpuWarmup: true,
  gpuWarmup: true
});

export const CPU_ONLY_RUNTIME_WARMUP_POLICY: RuntimeWarmupPolicy = Object.freeze({
  backend: 'cpu',
  cpuWarmup: true,
  gpuWarmup: false
});

type CapabilityInput = {
  snapshotId?: string;
  source?: HostCapabilitySnapshotSource;
  reportedLogicalProcessors?: unknown;
  reportedDeviceMemoryGiB?: unknown;
  webGpuAvailable?: boolean;
  crossOriginIsolated?: boolean;
};

type BrowserCapabilityHost = {
  navigator?: {
    hardwareConcurrency?: number;
    deviceMemory?: number;
    gpu?: unknown;
  };
  crossOriginIsolated?: boolean;
  crypto?: {
    randomUUID?: () => string;
  };
};

let sharedBrowserSnapshot: HostCapabilitySnapshot | null = null;
let fallbackSnapshotSequence = 0;

export const FALLBACK_WASM_TRANSFER_BYTE_CAP = 32 * 1024 * 1024;
export const MIN_WASM_TRANSFER_BYTE_CAP = 16 * 1024 * 1024;
export const MAX_WASM_TRANSFER_BYTE_CAP = 128 * 1024 * 1024;

export function createHostCapabilitySnapshot(
  input: CapabilityInput = {}
): HostCapabilitySnapshot {
  const reportedLogicalProcessors = logicalProcessorCount(
    input.reportedLogicalProcessors
  );
  const reportedDeviceMemoryGiB = deviceMemoryGiB(
    input.reportedDeviceMemoryGiB
  );
  return Object.freeze({
    schemaVersion: 1,
    snapshotId: validSnapshotId(input.snapshotId) ?? nextFallbackSnapshotId(),
    source: input.source ?? 'host-provided',
    reportedLogicalProcessors,
    automaticWorkerCap: Math.max(1, reportedLogicalProcessors - 1),
    reportedDeviceMemoryGiB,
    wasmTransferByteCap: wasmTransferByteCap(reportedDeviceMemoryGiB),
    webGpuAvailable: input.webGpuAvailable === true,
    crossOriginIsolated: input.crossOriginIsolated === true
  });
}

/** Capture browser capabilities once on the main thread and reuse the frozen value. */
export function sharedBrowserHostCapabilitySnapshot(
  host: BrowserCapabilityHost = globalThis as BrowserCapabilityHost
): HostCapabilitySnapshot {
  if (sharedBrowserSnapshot) return sharedBrowserSnapshot;
  const randomId = host.crypto?.randomUUID?.();
  sharedBrowserSnapshot = createHostCapabilitySnapshot({
    snapshotId: validSnapshotId(randomId) ?? nextFallbackSnapshotId(),
    source: host.navigator ? 'browser-main' : 'conservative-fallback',
    reportedLogicalProcessors: host.navigator?.hardwareConcurrency,
    reportedDeviceMemoryGiB: host.navigator?.deviceMemory,
    webGpuAvailable: Boolean(host.navigator && 'gpu' in host.navigator),
    crossOriginIsolated: host.crossOriginIsolated === true
  });
  return sharedBrowserSnapshot;
}

export function automaticWorkerAuthority(
  snapshot: HostCapabilitySnapshot,
  useAllLogicalProcessors = false
): WorkerAuthorityReport {
  const workersRequested = useAllLogicalProcessors
    ? snapshot.reportedLogicalProcessors
    : snapshot.automaticWorkerCap;
  return resolveWorkerAuthority(
    snapshot,
    workersRequested,
    useAllLogicalProcessors ? 'all-logical-processors' : 'reserved-main-thread'
  );
}

export function resolveWorkerAuthority(
  snapshot: HostCapabilitySnapshot,
  requestedWorkers: unknown,
  requestedReason: Exclude<
    WorkerAuthorityReason,
    'host-cap' | 'invalid-request'
  > = 'explicit-request'
): WorkerAuthorityReport {
  const validRequest =
    typeof requestedWorkers === 'number' &&
    Number.isFinite(requestedWorkers) &&
    requestedWorkers >= 1;
  const requested = validRequest ? Math.max(1, Math.floor(requestedWorkers)) : 1;
  const effective = Math.min(requested, snapshot.reportedLogicalProcessors);
  return Object.freeze({
    snapshotId: snapshot.snapshotId,
    reportedLogicalProcessors: snapshot.reportedLogicalProcessors,
    workersRequested: requested,
    workersEffective: effective,
    reason: !validRequest
      ? 'invalid-request'
      : effective < requested
        ? 'host-cap'
        : requestedReason
  });
}

export function normalizeRuntimeWarmupPolicy(
  policy: RuntimeWarmupPolicy = DEFAULT_RUNTIME_WARMUP_POLICY
): RuntimeWarmupPolicy {
  return Object.freeze({
    backend: policy.backend,
    cpuWarmup: policy.cpuWarmup === true,
    gpuWarmup: policy.backend !== 'cpu' && policy.gpuWarmup === true
  });
}

export function isHostCapabilitySnapshot(
  value: unknown
): value is HostCapabilitySnapshot {
  if (!value || typeof value !== 'object') return false;
  const candidate = value as Partial<HostCapabilitySnapshot>;
  return (
    candidate.schemaVersion === 1 &&
    validSnapshotId(candidate.snapshotId) !== null &&
    (candidate.source === 'browser-main' ||
      candidate.source === 'host-provided' ||
      candidate.source === 'conservative-fallback') &&
    Number.isSafeInteger(candidate.reportedLogicalProcessors) &&
    Number(candidate.reportedLogicalProcessors) >= 1 &&
    Number.isSafeInteger(candidate.automaticWorkerCap) &&
    candidate.automaticWorkerCap ===
      Math.max(1, Number(candidate.reportedLogicalProcessors) - 1) &&
    (candidate.reportedDeviceMemoryGiB === null ||
      (typeof candidate.reportedDeviceMemoryGiB === 'number' &&
        Number.isFinite(candidate.reportedDeviceMemoryGiB) &&
        candidate.reportedDeviceMemoryGiB > 0)) &&
    Number.isSafeInteger(candidate.wasmTransferByteCap) &&
    candidate.wasmTransferByteCap === wasmTransferByteCap(candidate.reportedDeviceMemoryGiB) &&
    typeof candidate.webGpuAvailable === 'boolean' &&
    typeof candidate.crossOriginIsolated === 'boolean'
  );
}

function deviceMemoryGiB(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) && value > 0
    ? value
    : null;
}

function wasmTransferByteCap(reportedDeviceMemoryGiB: number | null): number {
  if (reportedDeviceMemoryGiB === null) return FALLBACK_WASM_TRANSFER_BYTE_CAP;
  const proportional = Math.floor(
    reportedDeviceMemoryGiB * FALLBACK_WASM_TRANSFER_BYTE_CAP
  );
  return Math.max(
    MIN_WASM_TRANSFER_BYTE_CAP,
    Math.min(MAX_WASM_TRANSFER_BYTE_CAP, proportional)
  );
}

function logicalProcessorCount(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return 1;
  const reported = Math.floor(value);
  return Number.isSafeInteger(reported) ? Math.max(1, reported) : 1;
}

function validSnapshotId(value: unknown): string | null {
  return typeof value === 'string' && value.length >= 1 && value.length <= 128
    ? value
    : null;
}

function nextFallbackSnapshotId(): string {
  fallbackSnapshotSequence += 1;
  return `clearra-host-${fallbackSnapshotSequence}`;
}
