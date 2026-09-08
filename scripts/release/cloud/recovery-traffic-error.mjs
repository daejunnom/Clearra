// Owns bounded, allowlisted diagnostics for Cloud Run HTTP failures only.
// A diagnostic is not authority to retry, change identities, or clear recovery debt.
const MAX_ERROR_BYTES = 16 * 1024;
const STATUSES = new Set(["PERMISSION_DENIED", "UNAUTHENTICATED", "NOT_FOUND", "FAILED_PRECONDITION", "ABORTED", "RESOURCE_EXHAUSTED", "UNAVAILABLE", "INTERNAL", "INVALID_ARGUMENT"]);
const REASONS = new Set(["IAM_PERMISSION_DENIED", "ACCESS_TOKEN_SCOPE_INSUFFICIENT", "SERVICE_DISABLED", "CONSUMER_INVALID", "USER_PROJECT_DENIED", "BILLING_DISABLED", "SECURITY_POLICY_VIOLATED"]);
const PERMISSIONS = ["iam.serviceAccounts.actAs", "run.services.update", "run.services.get", "run.revisions.get", "serviceusage.services.use", "artifactregistry.repositories.downloadArtifacts"];

// EX_NOPERM is a failure, never recovery evidence. The PowerShell boundary uses
// it only for this helper to retain the cause instead of reporting a validator bug.
export const RUNTIME_ACTAS_DENIED_EXIT_CODE = 77;
export function recoveryTrafficExitCode(error) {
  return error?.name === "RecoveryTrafficHttpError" &&
    error.httpStatus === 403 && ["validate", "apply"].includes(error.phase) &&
    error.diagnosis === "runtime-actas-denied" ? RUNTIME_ACTAS_DENIED_EXIT_CODE : 2;
}

async function readErrorObject(response) {
  let reader;
  try {
    reader = response.body?.getReader();
    if (!reader) return { bodyState: "empty", error: null };
    const parts = [];
    let bytes = 0;
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      bytes += value.byteLength;
      if (bytes > MAX_ERROR_BYTES) return { bodyState: "oversized", error: null };
      parts.push(Buffer.from(value));
    }
    if (bytes === 0) return { bodyState: "empty", error: null };
    let parsed;
    try { parsed = JSON.parse(Buffer.concat(parts).toString("utf8")); }
    catch { return { bodyState: "invalid-json", error: null }; }
    return parsed?.error && typeof parsed.error === "object" && !Array.isArray(parsed.error)
      ? { bodyState: "parsed", error: parsed.error }
      : { bodyState: "missing-error-object", error: null };
  } catch {
    return { bodyState: "read-failed", error: null };
  } finally {
    if (reader) {
      try { await reader.cancel(); } catch { /* Preserve the original HTTP failure. */ }
      try { reader.releaseLock(); } catch { /* Do not replace the primary error. */ }
    }
  }
}

export async function recoveryTrafficHttpError(response, { method, validateOnly }) {
  const phase = method === "PATCH" ? (validateOnly ? "validate" : "apply") : "read";
  const { error, bodyState } = await readErrorObject(response);
  const status = STATUSES.has(error?.status) ? error.status : "unknown";
  const reasons = new Set();
  const permissions = new Set();
  const permissionSources = new Set();
  let hasErrorInfo = false;
  for (const detail of Array.isArray(error?.details) ? error.details : []) {
    if (detail?.["@type"] !== "type.googleapis.com/google.rpc.ErrorInfo") continue;
    hasErrorInfo = true;
    if (REASONS.has(detail.reason)) reasons.add(detail.reason);
    for (const permission of PERMISSIONS) {
      if (typeof detail.metadata?.permission === "string" &&
          detail.metadata.permission.toLowerCase() === permission.toLowerCase()) {
        permissions.add(permission);
        permissionSources.add("error-info");
      }
    }
  }
  // Message-derived permission names are evidence with an explicit source, not
  // invented ErrorInfo.reason values. Never echo the message, account, or UID.
  if (typeof error?.message === "string") {
    for (const permission of PERMISSIONS) {
      if (error.message.toLowerCase().includes(`'${permission.toLowerCase()}'`) ||
          error.message.toLowerCase().includes(`"${permission.toLowerCase()}"`)) {
        permissions.add(permission);
        permissionSources.add("message");
      }
    }
  }
  const diagnosis = response.status === 403 && phase !== "read" &&
    permissions.has("iam.serviceAccounts.actAs") ? "runtime-actas-denied" : "unclassified-http-failure";
  const message = `Cloud recovery ${method} HTTP ${response.status}; phase=${phase}; ` +
    `google_status=${status}; reason=${[...reasons].sort().join(",") || "unknown"}; ` +
    `reported_permission=${[...permissions].sort().join(",") || "unknown"}; ` +
    `body_state=${bodyState}; error_info=${hasErrorInfo ? "present" : "absent"}; ` +
    `permission_source=${[...permissionSources].sort().join(",") || "none"}; diagnosis=${diagnosis}; ` +
    "stop without widening IAM or retrying as deployer";
  const result = new Error(message);
  result.name = "RecoveryTrafficHttpError";
  result.httpStatus = response.status;
  result.phase = phase;
  result.diagnosis = diagnosis;
  return result;
}
