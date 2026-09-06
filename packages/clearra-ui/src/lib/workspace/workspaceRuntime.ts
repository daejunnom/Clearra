import type {
  ClearraHostAppResponse,
  ClearraSearchProgressTelemetry,
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
import {
  projectWorkspaceSearchReport,
  type WorkspaceSearchReport
} from './workspaceSearchReport';
import {
  projectWorkspacePublicFailure,
  type WorkspaceDeveloperDiagnostic,
  type WorkspacePublicFailure
} from './workspacePublicFailure';

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

export type WorkspaceRuntimeDiagnostic = WorkspaceDeveloperDiagnostic;

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
  publicFailures: WorkspacePublicFailure[];
  developerDiagnostics: WorkspaceRuntimeDiagnostic[];
  response: ClearraHostAppResponse | ClearraDesktopAppResponse | null;
  searchReport: WorkspaceSearchReport | null;
  webgpuReport: ClearraWebGpuBackendReport | null;
  backendReport: ClearraHostAppResponse['backend_report'] | ClearraDesktopBackendStatus | null;
  resourceReport: ClearraHostAppResponse['resource_report'] | ClearraDesktopResourceStatus | null;
  renderCapability: RenderCapabilityReport | null;
  developerError: string | null;
};

export function workspaceViewFromWasm(state: WasmWorkerState): WorkspaceRuntimeView {
  const failure = projectWorkspacePublicFailure({
    status: state.status,
    responseStatus: state.response?.status,
    terminationReason: state.terminationReason,
    error: state.error,
    diagnostics: state.diagnostics
  });
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
    publicFailures: failure.publicFailures,
    developerDiagnostics: failure.developerEvidence.diagnostics,
    response: state.response,
    searchReport: projectWorkspaceSearchReport(
      state.searchReport,
      state.executionAvailability,
      state.resultCompleteness
    ),
    webgpuReport: state.webgpuBackend,
    backendReport: state.response?.backend_report ?? null,
    resourceReport: state.resourceReport,
    renderCapability: state.response?.capability_report.render_capability ?? null,
    developerError: failure.developerEvidence.error
  };
}

export function workspaceViewFromDesktop(state: DesktopJobState): WorkspaceRuntimeView {
  const resourceReport = state.resourceStatus ?? state.result?.resource_report ?? null;
  const failure = projectWorkspacePublicFailure({
    status: state.status,
    responseStatus: state.result?.status,
    error: state.error,
    diagnostics: state.diagnostics
  });
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
    publicFailures: failure.publicFailures,
    developerDiagnostics: failure.developerEvidence.diagnostics,
    response: state.result,
    searchReport: projectWorkspaceSearchReport(
      state.searchReport,
      resourceReport?.execution_availability,
      resourceReport?.result_completeness
    ),
    webgpuReport: null,
    backendReport: state.backendStatus ?? state.result?.backend_report ?? null,
    resourceReport,
    renderCapability: state.result?.capability_report.render_capability ?? null,
    developerError: failure.developerEvidence.error
  };
}
