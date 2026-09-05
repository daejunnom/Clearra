import {
  workspaceMessage,
  type WorkspaceLanguage,
  type WorkspaceMessageKey
} from './workspaceI18n.ts';

export type WorkspacePublicFailureCode =
  | 'request-cancelled'
  | 'worker-terminated'
  | 'invalid-input'
  | 'unsupported'
  | 'resource-limit'
  | 'result-incomplete'
  | 'result-invalid'
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
  'invalid-input': 'workspaceFailureInvalidInput',
  unsupported: 'workspaceFailureUnsupported',
  'resource-limit': 'workspaceFailureResourceLimit',
  'result-incomplete': 'workspaceFailureIncomplete',
  'result-invalid': 'workspaceFailureInvalidResult',
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
  const publicFailures: WorkspacePublicFailure[] = [];
  const observed = new Set<WorkspacePublicFailureCode>();
  const add = (code: WorkspacePublicFailureCode, severity: 'error' | 'warning' = 'error') => {
    if (observed.has(code)) return;
    observed.add(code);
    publicFailures.push({ code, severity });
  };

  if (input.status === 'cancelled') add('request-cancelled');
  if (input.status === 'terminated' || input.terminationReason) add('worker-terminated');
  if (input.responseStatus === 'validation-failed') add('invalid-input');
  if (input.responseStatus === 'unsupported') add('unsupported');
  if (input.responseStatus === 'execution-failed') add('execution-failed');

  for (const diagnostic of developerEvidence.diagnostics) {
    const severity = diagnostic.severity.toLowerCase() === 'warning' ? 'warning' : 'error';
    add(classifyDiagnosticCode(diagnostic.code, severity), severity);
  }

  if (
    publicFailures.length === 0 &&
    (input.error || input.status === 'failed' || input.responseStatus === 'execution-failed')
  ) {
    add(input.fallbackCode ?? 'execution-failed');
  }

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
