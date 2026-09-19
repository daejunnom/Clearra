import {
  workspaceMessage,
  type WorkspaceLanguage,
  type WorkspaceMessageKey
} from './workspaceI18n.ts';

export type WorkspacePublicFailureCode =
  | 'request-cancelled'
  | 'worker-terminated'
  | 'worker-lease-expired'
  | 'runtime-trap'
  | 'invalid-input'
  | 'unsupported'
  | 'resource-limit'
  | 'result-incomplete'
  | 'result-invalid'
  | 'tablebase-miss'
  | 'tablebase-offline'
  | 'tablebase-rate-limited'
  | 'tablebase-timeout'
  | 'tablebase-unavailable'
  | 'execution-failed'
  | 'execution-warning';

export type WorkspacePublicFailure = {
  code: WorkspacePublicFailureCode;
  severity: 'error' | 'warning';
};

export type WorkspaceDeveloperDiagnostic = {
  code: string;
  severity: string;
  message: string;
};

export type WorkspaceDeveloperFailureEvidence = {
  error: string | null;
  diagnostics: WorkspaceDeveloperDiagnostic[];
};

export type WorkspaceFailureProjection = {
  publicFailures: WorkspacePublicFailure[];
  developerEvidence: WorkspaceDeveloperFailureEvidence;
};

export type WorkspaceFailureProjectionInput = {
  status: string;
  responseStatus?: string | null;
  terminationReason?: string | null;
  error?: string | null;
  diagnostics?: ReadonlyArray<{
    code?: string | null;
    severity?: string | null;
    message?: string | null;
  }>;
  fallbackCode?: WorkspacePublicFailureCode;
};

const PUBLIC_FAILURE_MESSAGE_KEYS: Record<WorkspacePublicFailureCode, WorkspaceMessageKey> = {
  'request-cancelled': 'workspaceFailureCancelled',
  'worker-terminated': 'workspaceFailureTerminated',
  'worker-lease-expired': 'workspaceFailureLeaseExpired',
  'runtime-trap': 'workspaceFailureRuntimeTrap',
  'invalid-input': 'workspaceFailureInvalidInput',
  unsupported: 'workspaceFailureUnsupported',
  'resource-limit': 'workspaceFailureResourceLimit',
  'result-incomplete': 'workspaceFailureIncomplete',
  'result-invalid': 'workspaceFailureInvalidResult',
  'tablebase-miss': 'workspaceFailureTablebaseMiss',
  'tablebase-offline': 'workspaceFailureTablebaseOffline',
  'tablebase-rate-limited': 'workspaceFailureTablebaseRateLimited',
  'tablebase-timeout': 'workspaceFailureTablebaseTimeout',
  'tablebase-unavailable': 'workspaceFailureTablebaseUnavailable',
  'execution-failed': 'workspaceFailureExecution',
  'execution-warning': 'workspaceFailureWarning'
};

export function projectWorkspacePublicFailure(
  input: WorkspaceFailureProjectionInput
): WorkspaceFailureProjection {
  const developerEvidence: WorkspaceDeveloperFailureEvidence = {
    error: input.error ?? null,
    diagnostics: (input.diagnostics ?? []).map((diagnostic) => ({
      code: diagnostic.code ?? 'unknown-diagnostic',
      severity: diagnostic.severity ?? 'error',
      message: diagnostic.message ?? ''
    }))
  };
  // One failed execution can carry a generic response status, a wrapping
  // diagnostic, and the concrete cause. Present the most specific cause once;
  // retain every original diagnostic above for developer inspection.
  const candidates: Array<WorkspacePublicFailure & { priority: number }> = [];
  const add = (
    code: WorkspacePublicFailureCode,
    priority: number,
    severity: 'error' | 'warning' = 'error'
  ) => {
    candidates.push({ code, severity, priority });
  };

  if (input.status === 'cancelled') add('request-cancelled', 5);
  if (input.status === 'terminated' || input.terminationReason) add('worker-terminated', 2);
  if (input.responseStatus === 'validation-failed') add('invalid-input', 2);
  if (input.responseStatus === 'unsupported') add('unsupported', 2);
  if (input.responseStatus === 'execution-failed') add('execution-failed', 1);

  for (const diagnostic of developerEvidence.diagnostics) {
    const reportedSeverity = diagnostic.severity.toLowerCase();
    if (reportedSeverity !== 'warning' && reportedSeverity !== 'error') continue;
    const severity = reportedSeverity;
    const runtimeFailure = knownRuntimeFailure(diagnostic.message);
    const code = runtimeFailure ?? classifyDiagnosticCode(diagnostic.code, severity);
    const generic = code === 'execution-failed' || code === 'execution-warning';
    add(code, runtimeFailure ? 4 : generic ? 1 : 3, severity);
  }

  const runtimeFailure = knownRuntimeFailure(input.error ?? '');
  if (runtimeFailure) add(runtimeFailure, 4);

  if (
    candidates.length === 0 &&
    (input.error || input.status === 'failed' || input.responseStatus === 'execution-failed')
  ) {
    add(input.fallbackCode ?? 'execution-failed', 1);
  }

  const primary = candidates.reduce<(typeof candidates)[number] | null>((best, candidate) => {
    if (best === null) return candidate;
    if (best.severity !== candidate.severity) return candidate.severity === 'error' ? candidate : best;
    return candidate.priority > best.priority ? candidate : best;
  }, null);
  const publicFailures: WorkspacePublicFailure[] = primary
    ? [{ code: primary.code, severity: primary.severity }]
    : [];
  return { publicFailures, developerEvidence };
}

export function workspacePublicFailure(
  code: WorkspacePublicFailureCode,
  severity: 'error' | 'warning' = 'error'
): WorkspacePublicFailure {
  return { code, severity };
}

export function workspacePublicFailureMessage(
  language: WorkspaceLanguage,
  failure: WorkspacePublicFailure
): string {
  return workspaceMessage(language, PUBLIC_FAILURE_MESSAGE_KEYS[failure.code]);
}

function classifyDiagnosticCode(
  code: string,
  severity: 'error' | 'warning'
): WorkspacePublicFailureCode {
  const normalized = code.toLowerCase();
  const tablebase = knownTablebaseFailure(normalized);
  if (tablebase) return tablebase;
  if (/(?:wasm|runtime).*trap/u.test(normalized)) return 'runtime-trap';
  if (/(?:cancel|abort)/u.test(normalized)) return 'request-cancelled';
  if (/(?:terminat|worker.*fail|panic)/u.test(normalized)) return 'worker-terminated';
  if (/(?:unsupported|unavailable|not[_-]?supported)/u.test(normalized)) return 'unsupported';
  if (/(?:resource|memory|limit|budget|capacity|quota|overflow|oom)/u.test(normalized)) {
    return 'resource-limit';
  }
  if (/(?:validation|invalid[_-]?input|required|parse|syntax)/u.test(normalized)) {
    return 'invalid-input';
  }
  if (/(?:incomplete|truncat|partial)/u.test(normalized)) return 'result-incomplete';
  if (/(?:contract|schema|identity|malformed|mismatch|consistency)/u.test(normalized)) {
    return 'result-invalid';
  }
  return severity === 'warning' ? 'execution-warning' : 'execution-failed';
}

// Runtime wrappers often carry a generic code. Recognize only stable failure
// classes in their evidence; never echo raw messages, fields, IDs or paths.
function knownRuntimeFailure(message: string): WorkspacePublicFailureCode | null {
  const tablebase = knownTablebaseFailure(message.toLowerCase());
  if (tablebase) return tablebase;
  if (/(?:lease[-_ ]expired|heartbeat lease expired)/iu.test(message)) {
    return 'worker-lease-expired';
  }
  if (/(?:out of memory|allocation[_ -]failed|memory[^\n]{0,80}(?:limit|budget|exceeded)|whole[_-]live[^\n]{0,40}(?:limit|exceeded))/iu.test(message)) {
    return 'resource-limit';
  }
  if (/(?:memory access out of bounds|WebAssembly\.RuntimeError|RuntimeError: unreachable|wasm[^\n]{0,40}trap)/iu.test(message)) {
    return 'runtime-trap';
  }
  if (/replay does not terminate at the requested cleared field/iu.test(message)) {
    return 'result-invalid';
  }
  return null;
}

// These are stable runtime reason codes, not user-visible diagnostics. Map
// them before the generic unsupported/resource classifiers so a transport
// outage is not presented as an unsupported command and a table miss is not
// presented as a solver failure. The localized messages describe the only
// explicit offline path; this projection never starts that work itself.
function knownTablebaseFailure(value: string): WorkspacePublicFailureCode | null {
  if (/pc4_online_(?:field_)?miss/u.test(value)) return 'tablebase-miss';
  if (/pc4_online_offline/u.test(value)) return 'tablebase-offline';
  if (/pc4_online_rate_limited/u.test(value)) return 'tablebase-rate-limited';
  if (/pc4_online_timeout/u.test(value)) return 'tablebase-timeout';
  if (/pc4_online_(?:generation_|profile_or_target_)?unavailable/u.test(value)) {
    return 'tablebase-unavailable';
  }
  return null;
}
