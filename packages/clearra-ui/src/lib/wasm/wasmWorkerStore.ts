import { get, writable } from 'svelte/store';

import {
  buildWasmCommandRequest,
  postCancelJob,
  postRunCommand,
  type ClearraDiagnostic,
  type ClearraHostAppResponse,
  type ClearraSearchProgressTelemetry,
  type ClearraWebGpuBackendReport,
  type ClearraWasmCommandRequest,
  type ClearraWasmRuntimeAuthority,
  type ClearraWasmSearchReport,
  type ClearraWasmWorkerEvent
} from './wasmCommandClient';
import type { ClearraWasmForcedTerminationReason } from './wasmWorkerLifecycle';

export type WasmWorkerState = {
  request: ClearraWasmCommandRequest;
  jobId: number | null;
  status:
    | 'idle'
    | 'running'
    | 'cancelling'
    | 'completed'
    | 'cancelled'
    | 'terminated'
    | 'failed';
  terminationReason: ClearraWasmForcedTerminationReason | null;
  progressLabel: string;
  progressDone: number;
  progressTotal: number;
  forwardPatternDone: number;
  forwardPatternTotal: number;
  progressTelemetry: ClearraSearchProgressTelemetry | null;
  terminalLines: string[];
  diagnostics: ClearraDiagnostic[];
  response: ClearraHostAppResponse | null;
  searchReport: ClearraWasmSearchReport | null;
  webgpuBackend: ClearraWebGpuBackendReport | null;
  tablebaseWarmup: WasmTablebaseWarmupState;
  error: string | null;
};

export type WasmTablebaseWarmupState = {
  status: 'disabled' | 'loading' | 'ready' | 'unavailable';
  artifactSha256: string;
  byteLength: number;
  message: string | null;
};

export type TablebaseWarmupWorkerEvent = {
  type: 'tablebase_warmup';
  phase: WasmTablebaseWarmupState['status'];
  artifactSha256: string;
  byteLength: number;
  message?: string;
};

const wasmWorkerInitialState: WasmWorkerState = {
  request: buildWasmCommandRequest({}),
  jobId: null,
  status: 'idle',
  terminationReason: null,
  progressLabel: '',
  progressDone: 0,
  progressTotal: 0,
  forwardPatternDone: 0,
  forwardPatternTotal: 0,
  progressTelemetry: null,
  terminalLines: ['clearra web runtime ready'],
  diagnostics: [],
  response: null,
  searchReport: null,
  webgpuBackend: null,
  tablebaseWarmup: {
    status: 'disabled',
    artifactSha256: '',
    byteLength: 0,
    message: null
  },
  error: null
};

export const wasmWorkerState = writable(wasmWorkerInitialState);

export function clearWasmTerminalResult() {
  wasmWorkerState.update((state) => {
    if (state.status === 'running' || state.status === 'cancelling') return state;
    return {
      ...state,
      jobId: null,
      status: 'idle',
      terminationReason: null,
      progressLabel: '',
      progressDone: 0,
      progressTotal: 0,
      forwardPatternDone: 0,
      forwardPatternTotal: 0,
      progressTelemetry: null,
      terminalLines: ['clearra web runtime ready'],
      diagnostics: [],
      response: null,
      searchReport: null,
      webgpuBackend: null,
      error: null
    };
  });
}

export function updateWasmCommandText(commandText: string) {
  wasmWorkerState.update((state) => ({
    ...state,
    request: buildWasmCommandRequest({ ...state.request, commandText })
  }));
}

export function runWasmCommand(
  worker: Worker,
  prewarmWorkerCount = 1,
  tablebaseRequested = false,
  lifecycleOwnerId?: string,
  runtimeAuthority?: ClearraWasmRuntimeAuthority
) {
  const request = get(wasmWorkerState).request;
  wasmWorkerState.update((state) => ({
    ...state,
    jobId: null,
    status: 'running',
    terminationReason: null,
    progressLabel: '',
    progressDone: 0,
    progressTotal: 0,
    forwardPatternDone: 0,
    forwardPatternTotal: 0,
    progressTelemetry: null,
    response: null,
    searchReport: null,
    webgpuBackend: null,
    diagnostics: [],
    error: null,
    terminalLines: [...state.terminalLines, `$ ${displayCommandText(request.commandText)}`]
  }));
  postRunCommand(
    worker,
    request,
    prewarmWorkerCount,
    tablebaseRequested,
    lifecycleOwnerId,
    runtimeAuthority
  );
}

function displayCommandText(commandText: string): string {
  const maxDisplayedCharacters = 240;
  if (commandText.length <= maxDisplayedCharacters) return commandText;
  return `${commandText.slice(0, maxDisplayedCharacters)}... (${commandText.length} characters)`;
}

export function cancelWasmCommand(worker: Worker): number | null | undefined {
  const state = get(wasmWorkerState);
  if (state.status !== 'running' && state.status !== 'cancelling') return undefined;
  const jobId = state.jobId;
  wasmWorkerState.update((current) => ({ ...current, status: 'cancelling' }));
  postCancelJob(worker, jobId ?? undefined);
  return jobId;
}

export function applyWasmWorkerEvent(event: ClearraWasmWorkerEvent) {
  wasmWorkerState.update((state) => reduceWasmWorkerEvent(state, event));
}

export function applyTablebaseWarmupEvent(event: TablebaseWarmupWorkerEvent) {
  wasmWorkerState.update((state) => ({
    ...state,
    tablebaseWarmup: {
      status: event.phase,
      artifactSha256: event.artifactSha256,
      byteLength: event.byteLength,
      message: event.message ?? null
    }
  }));
}

function reduceWasmWorkerEvent(
  state: WasmWorkerState,
  event: ClearraWasmWorkerEvent
): WasmWorkerState {
  switch (event.event) {
    case 'started':
      return {
        ...state,
        jobId: event.job_id,
        status: state.status === 'cancelling' ? 'cancelling' : 'running',
        terminationReason: null,
        terminalLines: [...state.terminalLines, `job ${event.job_id} started`]
      };
    case 'progress':
      return {
        ...state,
        progressLabel: event.progress.label,
        progressDone: event.progress.done,
        progressTotal: event.progress.total,
        forwardPatternDone:
          event.progress.label === 'forward-search-patterns'
            ? event.progress.done
            : state.forwardPatternDone,
        forwardPatternTotal:
          event.progress.label === 'forward-search-patterns'
            ? event.progress.total
            : state.forwardPatternTotal,
        progressTelemetry: event.progress.telemetry ?? null,
        terminalLines: appendDistinctProgress(state.terminalLines, event.progress.label)
      };
    case 'diagnostic':
      return {
        ...state,
        diagnostics: [...state.diagnostics, event.diagnostic],
        terminalLines: [
          ...state.terminalLines,
          `${event.diagnostic.code}: ${event.diagnostic.message}`
        ]
      };
    case 'partial_result':
      return {
        ...state,
        terminalLines: [
          ...state.terminalLines,
          event.partial ? `partial: ${event.label}` : event.label
        ]
      };
    case 'final_response': {
      const succeeded = event.response.status === 'success';
      const error = succeeded
        ? null
        : event.response.diagnostics
            .map((diagnostic) => `${diagnostic.code}: ${diagnostic.message}`)
            .join('\n') || event.response.status;
      return {
        ...state,
        jobId: null,
        status: succeeded ? 'completed' : 'failed',
        terminationReason: null,
        response: event.response,
        searchReport: event.search_report,
        webgpuBackend: event.webgpu_backend,
        progressTelemetry: null,
        diagnostics: event.response.diagnostics,
        error,
        terminalLines: [...state.terminalLines, JSON.stringify(event.response, null, 2)]
      };
    }
    case 'cancelled':
      return {
        ...state,
        jobId: null,
        status: 'cancelled',
        terminationReason: null,
        progressTelemetry: null,
        terminalLines: [
          ...state.terminalLines,
          event.scope_released ? 'job cancelled; computation scope released' : 'job cancelled'
        ]
      };
    case 'terminated': {
      const error =
        event.diagnostics.diagnostics
          .map((diagnostic) => `${diagnostic.code}: ${diagnostic.message}`)
          .join('\n') || 'WASM execution was force-terminated';
      return {
        ...state,
        jobId: null,
        status: 'terminated',
        terminationReason: event.reason,
        progressTelemetry: null,
        error,
        diagnostics: event.diagnostics.diagnostics,
        terminalLines: [...state.terminalLines, error]
      };
    }
    case 'failed': {
      const error =
        event.diagnostics.diagnostics
          .map((diagnostic) => `${diagnostic.code}: ${diagnostic.message}`)
          .join('\n') || 'WASM execution failed';
      return {
        ...state,
        jobId: null,
        status: 'failed',
        terminationReason: null,
        progressTelemetry: null,
        error,
        diagnostics: event.diagnostics.diagnostics,
        terminalLines: [...state.terminalLines, error]
      };
    }
    default:
      return state;
  }
}

function appendDistinctProgress(lines: string[], label: string): string[] {
  if (lines.at(-1) === label) return lines;
  return [...lines, label];
}
