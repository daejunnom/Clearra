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
import {
  isExecutionAvailabilityReport,
  type ExecutionAvailabilityReport,
  type ExecutionCompletenessState
} from './executionAvailability';
import type { ClearraWasmForcedTerminationReason } from './wasmWorkerLifecycle';
import { deferWasmTerminalResponse, type WasmTerminalLine } from './wasmTerminalTranscript';

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
  terminalLines: WasmTerminalLine[];
  diagnostics: ClearraDiagnostic[];
  response: ClearraHostAppResponse | null;
  resourceReport: ClearraHostAppResponse['resource_report'] | null;
  executionAvailability: ExecutionAvailabilityReport | null;
  resultCompleteness: ExecutionCompletenessState | null;
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
  resourceReport: null,
  executionAvailability: null,
  resultCompleteness: null,
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
      resourceReport: null,
      executionAvailability: null,
      resultCompleteness: null,
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
    resourceReport: null,
    executionAvailability: null,
    resultCompleteness: null,
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
      const resourceReport = isResourceReport(event.response.resource_report)
        ? event.response.resource_report
        : null;
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
        resourceReport,
        executionAvailability: resourceReport?.execution_availability ?? null,
        resultCompleteness: resourceReport?.result_completeness ?? null,
        searchReport: event.search_report,
        webgpuBackend: event.webgpu_backend,
        progressTelemetry: null,
        diagnostics: event.response.diagnostics,
        error,
        terminalLines: [...state.terminalLines, deferWasmTerminalResponse(event.response)]
      };
    }
    case 'cancelled':
      return {
        ...state,
        jobId: null,
        status: 'cancelled',
        terminationReason: null,
        progressTelemetry: null,
        resourceReport: null,
        executionAvailability: event.execution_availability ?? null,
        resultCompleteness: event.result_completeness ?? null,
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
        resourceReport: null,
        executionAvailability: null,
        resultCompleteness: null,
        error,
        diagnostics: event.diagnostics.diagnostics,
        terminalLines: [...state.terminalLines, error]
      };
    }
    case 'failed': {
      const suppliedResponse = event.response ?? null;
      const projection = reconcileFailedResourceReport(event, suppliedResponse);
      const response = projection.valid ? suppliedResponse : null;
      const executionAvailability = projection.valid
        ? projection.report?.execution_availability ?? event.execution_availability ?? null
        : null;
      const resultCompleteness = projection.valid
        ? projection.report?.result_completeness ?? event.result_completeness ?? 'incomplete'
        : 'incomplete';
      const diagnostics = [
        ...(response?.diagnostics ?? event.diagnostics.diagnostics),
        ...(projection.valid
          ? []
          : [
              {
                code: 'E_WASM_RESOURCE_REPORT_MISMATCH',
                severity: 'error',
                message: 'WASM failed-event resource evidence was inconsistent'
              }
            ])
      ];
      const error =
        diagnostics
          .map((diagnostic) => `${diagnostic.code}: ${diagnostic.message}`)
          .join('\n') || 'WASM execution failed';
      return {
        ...state,
        jobId: null,
        status: 'failed',
        terminationReason: null,
        progressTelemetry: null,
        response,
        resourceReport: projection.valid ? projection.report : null,
        executionAvailability,
        resultCompleteness,
        searchReport: null,
        webgpuBackend: null,
        error,
        diagnostics,
        terminalLines: [
          ...state.terminalLines,
          response ? deferWasmTerminalResponse(response) : error
        ]
      };
    }
    default:
      return state;
  }
}

type FailedEvent = Extract<ClearraWasmWorkerEvent, { event: 'failed' }>;
type ResourceReport = ClearraHostAppResponse['resource_report'];

function reconcileFailedResourceReport(
  event: FailedEvent,
  response: ClearraHostAppResponse | null
): { valid: boolean; report: ResourceReport | null } {
  const eventReport = event.resource_report ?? null;
  const responseReport = response?.resource_report ?? null;
  if (eventReport && !isResourceReport(eventReport)) return { valid: false, report: null };
  if (responseReport && !isResourceReport(responseReport)) return { valid: false, report: null };
  if (eventReport && responseReport && !resourceReportsEqual(eventReport, responseReport)) {
    return { valid: false, report: null };
  }
  const report = eventReport ?? responseReport;
  if (!report) return { valid: true, report: null };
  if (
    !event.execution_availability ||
    !event.result_completeness ||
    !availabilityReportsEqual(
      report.execution_availability,
      event.execution_availability
    ) ||
    report.result_completeness !== event.result_completeness
  ) {
    return { valid: false, report: null };
  }
  return { valid: true, report };
}

function isResourceReport(value: unknown): value is ResourceReport {
  if (!value || typeof value !== 'object') return false;
  const report = value as Partial<ResourceReport>;
  const counts = [
    report.peak_frontier_states,
    report.peak_candidate_rows,
    report.peak_hash_buckets,
    report.peak_gpu_bytes,
    report.peak_cpu_bytes,
    report.build_worker_backlog_peak,
    report.coverage_rows_emitted
  ];
  return (
    typeof report.solver_executed === 'boolean' &&
    typeof report.memory_status === 'string' &&
    typeof report.truncated === 'boolean' &&
    (report.truncation_reason === null || typeof report.truncation_reason === 'string') &&
    typeof report.probability_complete === 'boolean' &&
    isExecutionAvailabilityReport(report.execution_availability) &&
    (report.result_completeness === 'not-executed' ||
      report.result_completeness === 'complete' ||
      report.result_completeness === 'incomplete') &&
    counts.every((count) => Number.isSafeInteger(count) && (count as number) >= 0)
  );
}

function resourceReportsEqual(left: ResourceReport, right: ResourceReport): boolean {
  return (
    left.solver_executed === right.solver_executed &&
    left.memory_status === right.memory_status &&
    left.truncated === right.truncated &&
    left.truncation_reason === right.truncation_reason &&
    left.peak_frontier_states === right.peak_frontier_states &&
    left.peak_candidate_rows === right.peak_candidate_rows &&
    left.peak_hash_buckets === right.peak_hash_buckets &&
    left.peak_gpu_bytes === right.peak_gpu_bytes &&
    left.peak_cpu_bytes === right.peak_cpu_bytes &&
    left.build_worker_backlog_peak === right.build_worker_backlog_peak &&
    left.coverage_rows_emitted === right.coverage_rows_emitted &&
    left.probability_complete === right.probability_complete &&
    availabilityReportsEqual(left.execution_availability, right.execution_availability) &&
    left.result_completeness === right.result_completeness
  );
}

function availabilityReportsEqual(
  left: ExecutionAvailabilityReport,
  right: ExecutionAvailabilityReport
): boolean {
  return (
    left.state === right.state &&
    left.reason === right.reason &&
    left.surface === right.surface &&
    left.descriptor_pattern_count === right.descriptor_pattern_count &&
    left.dense_pattern_count === right.dense_pattern_count &&
    left.required_dense_bytes === right.required_dense_bytes &&
    left.required_memory_bytes === right.required_memory_bytes
  );
}

function appendDistinctProgress(lines: WasmTerminalLine[], label: string): WasmTerminalLine[] {
  if (lines.at(-1) === label) return lines;
  return [...lines, label];
}
