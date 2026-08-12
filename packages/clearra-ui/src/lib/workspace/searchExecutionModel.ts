export type SearchBackend = 'auto' | 'cpu' | 'gpu' | 'hybrid';

export type SearchExecutionRequest = {
  backend: SearchBackend;
  gpuDevice: string;
  workers: number;
  useAllLogicalProcessors: boolean;
  allowBackendFallback: boolean;
  cpuWarmup: boolean;
  gpuWarmup: boolean;
};

export type SearchExecutionDesktopFields = {
  backend: SearchBackend;
  gpu_device: string;
  workers: number;
  use_all_logical_processors: boolean;
  allow_backend_fallback: boolean;
};

export function normalizeSearchExecutionRequest(
  request: SearchExecutionRequest
): SearchExecutionRequest {
  const workers = Number.isFinite(request.workers)
    ? Math.max(1, Math.trunc(request.workers))
    : 1;
  return {
    ...request,
    gpuDevice: request.backend === 'cpu' ? 'auto' : request.gpuDevice,
    workers,
    gpuWarmup: request.backend !== 'cpu' && request.gpuWarmup
  };
}

export function searchExecutionCommandArguments(request: SearchExecutionRequest): string[] {
  const normalized = normalizeSearchExecutionRequest(request);
  const output = ['--backend', normalized.backend];
  if (normalized.gpuDevice !== 'auto') output.push('--gpu-device', normalized.gpuDevice);
  output.push(
    normalized.allowBackendFallback
      ? '--allow-backend-fallback'
      : '--no-backend-fallback',
    '--workers',
    String(normalized.workers)
  );
  if (normalized.cpuWarmup) output.push('--cpu-warmup');
  if (normalized.useAllLogicalProcessors) output.push('--use-all-cpu-threads');
  if (normalized.gpuWarmup) output.push('--gpu-warmup');
  return output;
}

export function searchExecutionDesktopFields(
  request: SearchExecutionRequest
): SearchExecutionDesktopFields {
  const normalized = normalizeSearchExecutionRequest(request);
  return {
    backend: normalized.backend,
    gpu_device: normalized.gpuDevice,
    workers: normalized.workers,
    use_all_logical_processors: normalized.useAllLogicalProcessors,
    allow_backend_fallback: normalized.allowBackendFallback
  };
}
