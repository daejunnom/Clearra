import { get, writable } from 'svelte/store';

import {
  buildDesktopAppRequest,
  cancelJob,
  getJobEvents,
  prewarmSearchBackend,
  startJob,
  validateRequest,
  type ClearraDesktopAppResponse,
  type ClearraDesktopBackendStatus,
  type ClearraDesktopJobEvent,
  type ClearraDesktopMemoryStatus,
  type ClearraDesktopRequest,
  type ClearraDesktopResourceStatus
} from '../host';

export type DesktopJobState = {
  request: ClearraDesktopRequest;
  jobId: number | null;
  status: 'idle' | 'validating' | 'running' | 'cancelling' | 'completed' | 'failed' | 'cancelled';
  progressLabel: string;
  progressDone: number;
  progressTotal: number;
  result: ClearraDesktopAppResponse | null;
  validation: unknown | null;
  diagnostics: Array<{ code: string; severity: string }>;
  backendStatus: ClearraDesktopBackendStatus | null;
  memoryStatus: ClearraDesktopMemoryStatus | null;
  resourceStatus: ClearraDesktopResourceStatus | null;
  error: string | null;
};

const desktopJobInitialState: DesktopJobState = {
  request: buildDesktopAppRequest({}),
  jobId: null,
  status: 'idle',
  progressLabel: '',
  progressDone: 0,
  progressTotal: 0,
  result: null,
  validation: null,
  diagnostics: [],
  backendStatus: null,
  memoryStatus: null,
  resourceStatus: null,
  error: null
};

export const desktopJobState = writable(desktopJobInitialState);
const DESKTOP_JOB_POLL_INTERVAL_MS = 100;
let pollTimer: ReturnType<typeof setTimeout> | null = null;
let pollEpoch = 0;
let gpuWarmupPromise: Promise<void> | null = null;
let gpuWarmupAttempted = false;

export function updateDesktopRequest(patch: Partial<ClearraDesktopRequest>) {
  desktopJobState.update((state) => {
    const request = buildDesktopAppRequest({ ...state.request, ...patch });
    return { ...state, request };
  });
}

export async function validateDesktopRequest() {
  const request = get(desktopJobState).request;
  desktopJobState.update((state) => ({ ...state, status: 'validating', error: null }));
  try {
    const validation = await validateRequest(request);
    desktopJobState.update((state) => ({ ...state, status: 'idle', validation }));
  } catch (error) {
    desktopJobState.update((state) => ({
      ...state,
      status: 'failed',
      error: error instanceof Error ? error.message : String(error)
    }));
  }
}

export async function startDesktopJob() {
  const current = get(desktopJobState);
  if (current.jobId !== null || current.status === 'running' || current.status === 'cancelling') {
    return;
  }
  stopDesktopJobPolling();
  const request = get(desktopJobState).request;
  desktopJobState.update((state) => ({
    ...state,
    status: 'running',
    jobId: null,
    progressLabel: '',
    progressDone: 0,
    progressTotal: 0,
    result: null,
    diagnostics: [],
    backendStatus: null,
    memoryStatus: null,
    resourceStatus: null,
    error: null
  }));
  try {
    const jobId = await startJob(request);
    const cancellationRequested = get(desktopJobState).status === 'cancelling';
    desktopJobState.update((state) => ({
      ...state,
      jobId,
      status: cancellationRequested ? 'cancelling' : 'running'
    }));
    if (cancellationRequested) {
      await cancelJob(jobId);
    }
    scheduleDesktopJobPoll(jobId, pollEpoch, 0);
  } catch (error) {
    desktopJobState.update((state) => ({
      ...state,
      status: 'failed',
      jobId: null,
      error: errorMessage(error)
    }));
  }
}

export async function cancelDesktopJob() {
  const current = get(desktopJobState);
  if (current.status !== 'running' && current.status !== 'cancelling') return;
  desktopJobState.update((state) => ({ ...state, status: 'cancelling', error: null }));
  if (current.jobId !== null) {
    try {
      await cancelJob(current.jobId);
    } catch (error) {
      desktopJobState.update((state) => ({
        ...state,
        status: 'failed',
        error: errorMessage(error)
      }));
    }
  }
}

export function disposeDesktopJobPolling() {
  stopDesktopJobPolling();
}

export function resumeDesktopJobPolling() {
  const jobId = get(desktopJobState).jobId;
  if (jobId === null) return;
  stopDesktopJobPolling();
  scheduleDesktopJobPoll(jobId, pollEpoch, 0);
}

export async function prewarmDesktopSearchBackend() {
  if (!requestsGpu(get(desktopJobState).request.backend)) return;
  await beginDesktopSearchBackendPrewarm();
}

function beginDesktopSearchBackendPrewarm(): Promise<void> {
  if (gpuWarmupAttempted) return Promise.resolve();
  if (gpuWarmupPromise !== null) return gpuWarmupPromise;

  gpuWarmupPromise = prewarmSearchBackend()
    .then(() => {
      gpuWarmupAttempted = true;
    })
    .catch(() => {
      // The real request remains the authority for capability and fallback evidence.
      gpuWarmupAttempted = true;
    })
    .finally(() => {
      gpuWarmupPromise = null;
    });
  return gpuWarmupPromise;
}

function requestsGpu(backend: ClearraDesktopRequest['backend']): boolean {
  return backend === 'auto' || backend === 'gpu' || backend === 'hybrid';
}

function scheduleDesktopJobPoll(jobId: number, epoch: number, delay: number) {
  pollTimer = setTimeout(() => void pollDesktopJob(jobId, epoch), delay);
}

async function pollDesktopJob(jobId: number, epoch: number) {
  if (epoch !== pollEpoch || get(desktopJobState).jobId !== jobId) return;
  try {
    const events = await getJobEvents(jobId);
    if (epoch !== pollEpoch) return;
    desktopJobState.update((state) =>
      events.reduce(
        (nextState, event) => applyDesktopJobEvent(nextState, event, jobId),
        state
      )
    );
    if (events.some(isTerminalEvent)) {
      stopDesktopJobPolling();
      return;
    }
    scheduleDesktopJobPoll(jobId, epoch, DESKTOP_JOB_POLL_INTERVAL_MS);
  } catch (error) {
    if (epoch !== pollEpoch) return;
    stopDesktopJobPolling();
    desktopJobState.update((state) => ({
      ...state,
      status: 'failed',
      jobId: null,
      error: errorMessage(error)
    }));
  }
}

function applyDesktopJobEvent(
  state: DesktopJobState,
  event: ClearraDesktopJobEvent,
  expectedJobId: number
): DesktopJobState {
  if (event.job_id !== expectedJobId) return state;
  switch (event.event) {
    case 'started':
      return state.status === 'cancelling' ? state : { ...state, status: 'running' };
    case 'progress':
      return {
        ...state,
        progressLabel: event.label ?? state.progressLabel,
        progressDone: event.done ?? state.progressDone,
        progressTotal: event.total ?? state.progressTotal,
        backendStatus: event.backend_status ?? state.backendStatus,
        memoryStatus: event.memory_status ?? state.memoryStatus,
        resourceStatus: event.resource_status ?? state.resourceStatus
      };
    case 'diagnostic':
      return {
        ...state,
        diagnostics: [
          ...state.diagnostics,
          { code: event.code ?? 'desktop-diagnostic', severity: event.severity ?? 'error' }
        ]
      };
    case 'completed':
      return {
        ...state,
        status: 'completed',
        jobId: null,
        result: event.response ?? null,
        backendStatus: event.response?.backend_report ?? state.backendStatus,
        resourceStatus: event.response?.resource_report ?? state.resourceStatus
      };
    case 'failed':
      return { ...state, status: 'failed', jobId: null, error: event.code ?? 'desktop-job-failed' };
    case 'cancelled':
      return { ...state, status: 'cancelled', jobId: null };
  }
}

function isTerminalEvent(event: ClearraDesktopJobEvent) {
  return event.event === 'completed' || event.event === 'failed' || event.event === 'cancelled';
}

function stopDesktopJobPolling() {
  if (pollTimer !== null) {
    clearTimeout(pollTimer);
    pollTimer = null;
  }
  pollEpoch += 1;
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
