import { get, writable } from 'svelte/store';

import {
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
import type { ClearraWasmSearchReport } from '../wasm';
import { DesktopJobStartGeneration } from './desktopJobStartGeneration';

export type DesktopJobState = {
  request: ClearraDesktopRequest;
  jobId: number | null;
  status: 'idle' | 'validating' | 'running' | 'cancelling' | 'completed' | 'failed' | 'cancelled';
  progressLabel: string;
  progressDone: number;
  progressTotal: number;
  result: ClearraDesktopAppResponse | null;
  searchReport: ClearraWasmSearchReport | null;
  validation: unknown | null;
  diagnostics: Array<{ code: string; severity: string }>;
  backendStatus: ClearraDesktopBackendStatus | null;
  memoryStatus: ClearraDesktopMemoryStatus | null;
  resourceStatus: ClearraDesktopResourceStatus | null;
  error: string | null;
};

const desktopJobInitialState: DesktopJobState = {
  request: {
    app_request_model: 'clearra-cli/CommandRequest',
    command: 'cli',
    language: 'en',
    arguments: ['clearra', 'pc', 'tiling', '--lines', '2']
  },
  jobId: null,
  status: 'idle',
  progressLabel: '',
  progressDone: 0,
  progressTotal: 0,
  result: null,
  searchReport: null,
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
const desktopJobStartGeneration = new DesktopJobStartGeneration();
let gpuWarmupPromise: Promise<void> | null = null;
let gpuWarmupAttempted = false;

export function clearDesktopTerminalResult() {
  desktopJobState.update((state) => {
    if (
      state.jobId !== null ||
      state.status === 'running' ||
      state.status === 'cancelling'
    ) {
      return state;
    }
    return {
      ...state,
      status: 'idle',
      progressLabel: '',
      progressDone: 0,
      progressTotal: 0,
      result: null,
      searchReport: null,
      validation: null,
      diagnostics: [],
      backendStatus: null,
      memoryStatus: null,
      resourceStatus: null,
      error: null
    };
  });
}

export function updateDesktopRequest(nextRequest: ClearraDesktopRequest) {
  const request = requireCompleteDesktopCliRequest(nextRequest);
  desktopJobState.update((state) => {
    const requestChanged = !desktopCliRequestsEqual(request, state.request);
    if (!requestChanged || state.status === 'running' || state.status === 'cancelling') {
      return { ...state, request };
    }
    return {
      ...state,
      request,
      status: 'idle',
      progressLabel: '',
      progressDone: 0,
      progressTotal: 0,
      result: null,
      searchReport: null,
      diagnostics: [],
      backendStatus: null,
      memoryStatus: null,
      resourceStatus: null,
      error: null
    };
  });
}

function requireCompleteDesktopCliRequest(request: ClearraDesktopRequest): ClearraDesktopRequest {
  const value = request as unknown;
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new TypeError('Desktop requests must be complete clearra-cli/CommandRequest objects');
  }
  const record = value as Record<string, unknown>;
  const expectedFields = ['app_request_model', 'command', 'language', 'arguments'];
  const unexpectedField = Object.keys(record).find((field) => !expectedFields.includes(field));
  if (unexpectedField !== undefined) {
    throw new TypeError(`Desktop CLI request does not accept field '${unexpectedField}'`);
  }
  if (
    record.app_request_model !== 'clearra-cli/CommandRequest' ||
    record.command !== 'cli' ||
    (record.language !== 'en' && record.language !== 'ko') ||
    !Array.isArray(record.arguments) ||
    record.arguments.length < 2 ||
    record.arguments.some((argument) => typeof argument !== 'string') ||
    record.arguments[0] !== 'clearra'
  ) {
    throw new TypeError('Desktop requests require a complete canonical CLI argv envelope');
  }
  return {
    app_request_model: 'clearra-cli/CommandRequest',
    command: 'cli',
    language: record.language,
    arguments: [...record.arguments]
  };
}

function desktopCliRequestsEqual(
  left: ClearraDesktopRequest,
  right: ClearraDesktopRequest
): boolean {
  return left.language === right.language &&
    left.arguments.length === right.arguments.length &&
    left.arguments.every((argument, index) => argument === right.arguments[index]);
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
  if (
    desktopJobStartGeneration.hasPending() ||
    current.jobId !== null ||
    current.status === 'running' ||
    current.status === 'cancelling'
  ) {
    return;
  }
  stopDesktopJobPolling();
  const startGeneration = desktopJobStartGeneration.begin();
  let startAccepted = false;
  const request = get(desktopJobState).request;
  desktopJobState.update((state) => ({
    ...state,
    status: 'running',
    jobId: null,
    progressLabel: '',
    progressDone: 0,
    progressTotal: 0,
    result: null,
    searchReport: null,
    diagnostics: [],
    backendStatus: null,
    memoryStatus: null,
    resourceStatus: null,
    error: null
  }));
  try {
    const jobId = await startJob(request);
    if (!desktopJobStartGeneration.complete(startGeneration)) {
      await cancelDetachedDesktopJob(jobId);
      return;
    }
    startAccepted = true;
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
    if (!startAccepted) {
      if (!desktopJobStartGeneration.complete(startGeneration)) {
        settleDisposedDesktopStart('cancelled', null);
        return;
      }
    }
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
  if (desktopJobStartGeneration.invalidatePending()) {
    desktopJobState.update((state) =>
      state.jobId === null && (state.status === 'running' || state.status === 'cancelling')
        ? { ...state, status: 'cancelling', error: null }
        : state
    );
  }
  stopDesktopJobPolling();
}

export function resumeDesktopJobPolling() {
  const jobId = get(desktopJobState).jobId;
  if (jobId === null) return;
  stopDesktopJobPolling();
  scheduleDesktopJobPoll(jobId, pollEpoch, 0);
}

export async function prewarmDesktopSearchBackend() {
  const request = get(desktopJobState).request;
  if (!requestsGpu(desktopRequestBackend(request))) {
    return;
  }
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

function desktopRequestBackend(
  request: ClearraDesktopRequest
): 'auto' | 'cpu' | 'gpu' | 'hybrid' | undefined {
  const optionIndex = request.arguments.lastIndexOf('--backend');
  const value = optionIndex >= 0 ? request.arguments[optionIndex + 1] : undefined;
  return value === 'auto' || value === 'cpu' || value === 'gpu' || value === 'hybrid'
    ? value
    : undefined;
}

function requestsGpu(backend: 'auto' | 'cpu' | 'gpu' | 'hybrid' | undefined): boolean {
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
    case 'completed': {
      const succeeded = event.response?.status === 'success';
      return {
        ...state,
        status: succeeded ? 'completed' : 'failed',
        jobId: null,
        result: event.response ?? null,
        searchReport: event.search_report ?? null,
        backendStatus: event.response?.backend_report ?? state.backendStatus,
        resourceStatus: event.response?.resource_report ?? state.resourceStatus,
        error: succeeded
          ? null
          : event.response?.diagnostics
              .map((diagnostic) => `${diagnostic.code}: ${diagnostic.message}`)
              .join('\n') || event.response?.status || 'desktop-job-failed'
      };
    }
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

async function cancelDetachedDesktopJob(jobId: number) {
  try {
    await cancelJob(jobId);
    settleDisposedDesktopStart('cancelled', null);
  } catch (error) {
    settleDisposedDesktopStart('failed', errorMessage(error));
  }
}

function settleDisposedDesktopStart(
  status: Extract<DesktopJobState['status'], 'cancelled' | 'failed'>,
  error: string | null
) {
  desktopJobState.update((state) => {
    if (state.jobId !== null || state.status !== 'cancelling') return state;
    return {
      ...state,
      status,
      jobId: null,
      error
    };
  });
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
