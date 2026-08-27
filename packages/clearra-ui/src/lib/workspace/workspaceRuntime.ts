import type {
  ClearraDiagnostic,
  ClearraHostAppResponse,
  ClearraSearchProgressTelemetry,
  ClearraWasmSearchReport,
  ClearraWebGpuBackendReport
} from '../wasm/wasmCommandClient';
import type { WasmWorkerState } from '../wasm/wasmWorkerStore';
import type {
  ClearraDesktopAppResponse,
  ClearraDesktopBackendStatus,
  ClearraDesktopResourceStatus
} from '../host/clearraDesktopHost';
import type { DesktopJobState } from '../stores/desktopJobStore';
import type { RenderCapabilityReport } from '../render/renderCapabilityReport';

export type WorkspaceRuntimeKind = 'web' | 'desktop';
export type WorkspaceRuntimeStatus =
  | 'idle'
  | 'validating'
  | 'running'
  | 'cancelling'
  | 'completed'
  | 'cancelled'
  | 'terminated'
  | 'failed';

export type WorkspaceRuntimeDiagnostic = {
  code: string;
  severity: string;
  message: string;
};

export type WorkspaceRuntimeView = {
  kind: WorkspaceRuntimeKind;
  status: WorkspaceRuntimeStatus;
  terminationReason: WasmWorkerState['terminationReason'];
  jobId: number | null;
  progressLabel: string;
  progressDone: number;
  progressTotal: number;
  forwardPatternDone: number;
  forwardPatternTotal: number;
  progressTelemetry: ClearraSearchProgressTelemetry | null;
  diagnostics: WorkspaceRuntimeDiagnostic[];
  response: ClearraHostAppResponse | ClearraDesktopAppResponse | null;
  searchReport: ClearraWasmSearchReport | null;
  webgpuReport: ClearraWebGpuBackendReport | null;
  backendReport: ClearraHostAppResponse['backend_report'] | ClearraDesktopBackendStatus | null;
  resourceReport: ClearraHostAppResponse['resource_report'] | ClearraDesktopResourceStatus | null;
  renderCapability: RenderCapabilityReport | null;
  error: string | null;
};

export function workspaceViewFromWasm(state: WasmWorkerState): WorkspaceRuntimeView {
  return {
    kind: 'web',
    status: state.status,
    terminationReason: state.terminationReason,
    jobId: state.jobId,
    progressLabel: state.progressLabel,
    progressDone: state.progressDone,
    progressTotal: state.progressTotal,
    forwardPatternDone: state.forwardPatternDone,
    forwardPatternTotal: state.forwardPatternTotal,
    progressTelemetry: state.progressTelemetry,
    diagnostics: state.diagnostics.map(normalizeDiagnostic),
    response: state.response,
    searchReport: state.searchReport,
    webgpuReport: state.webgpuBackend,
    backendReport: state.response?.backend_report ?? null,
    resourceReport: state.resourceReport,
    renderCapability: state.response?.capability_report.render_capability ?? null,
    error: state.error
  };
}

export function workspaceViewFromDesktop(state: DesktopJobState): WorkspaceRuntimeView {
  return {
    kind: 'desktop',
    status: state.status,
    terminationReason: null,
    jobId: state.jobId,
    progressLabel: state.progressLabel,
    progressDone: state.progressDone,
    progressTotal: state.progressTotal,
    forwardPatternDone: 0,
    forwardPatternTotal: 0,
    progressTelemetry: null,
    diagnostics: state.diagnostics.map((diagnostic) => ({
      ...diagnostic,
      message: diagnostic.code
    })),
    response: state.result,
    searchReport: state.searchReport,
    webgpuReport: null,
    backendReport: state.backendStatus ?? state.result?.backend_report ?? null,
    resourceReport: state.resourceStatus ?? state.result?.resource_report ?? null,
    renderCapability: state.result?.capability_report.render_capability ?? null,
    error: state.error
  };
}

function normalizeDiagnostic(diagnostic: ClearraDiagnostic): WorkspaceRuntimeDiagnostic {
  return {
    code: diagnostic.code,
    severity: diagnostic.severity,
    message: diagnostic.message
  };
}
