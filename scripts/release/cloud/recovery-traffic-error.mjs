// Owns bounded, allowlisted diagnostics for Cloud Run HTTP failures only.
// A diagnostic is not authority to retry, change identities, or clear recovery debt.
const MAX_ERROR_BYTES = 16 * 1024;
const STATUSES = new Set(["PERMISSION_DENIED", "UNAUTHENTICATED", "NOT_FOUND", "FAILED_PRECONDITION", "ABORTED", "RESOURCE_EXHAUSTED", "UNAVAILABLE", "INTERNAL", "INVALID_ARGUMENT"]);
const REASONS = new Set(["IAM_PERMISSION_DENIED", "ACCESS_TOKEN_SCOPE_INSUFFICIENT", "SERVICE_DISABLED", "CONSUMER_INVALID", "USER_PROJECT_DENIED", "BILLING_DISABLED", "SECURITY_POLICY_VIOLATED"]);
const PERMISSIONS = ["iam.serviceAccounts.actAs", "run.services.update", "run.services.get", "run.revisions.get", "serviceusage.services.use", "artifactregistry.repositories.downloadArtifacts"];

async function readErrorObject(response) {
  let reader;
  try {
    reader = response.body?.getReader();
    if (!reader) return null;
    const parts = [];
    let bytes = 0;
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      bytes += value.byteLength;
      if (bytes > MAX_ERROR_BYTES) return null;
      parts.push(Buffer.from(value));
    }
    const parsed = JSON.parse(Buffer.concat(parts).toString("utf8"));
    return parsed?.error && typeof parsed.error === "object" && !Array.isArray(parsed.error)
      ? parsed.error : null;
  } catch {
    return null;
  } finally {
    if (reader) {
      try { await reader.cancel(); } catch { /* Preserve the original HTTP failure. */ }
      reader.releaseLock();
    }
  }
}

export async function recoveryTrafficHttpError(response, { method, validateOnly }) {
  const phase = method === "PATCH" ? (validateOnly ? "validate" : "apply") : "read";
  const error = await readErrorObject(response);
  const status = STATUSES.has(error?.status) ? error.status : "unknown";
  const reasons = new Set();
  const permissions = new Set();
  for (const detail of Array.isArray(error?.details) ? error.details : []) {
    if (detail?.["@type"] !== "type.googleapis.com/google.rpc.ErrorInfo") continue;
    if (REASONS.has(detail.reason)) reasons.add(detail.reason);
    for (const permission of PERMISSIONS) {
      if (typeof detail.metadata?.permission === "string" &&
          detail.metadata.permission.toLowerCase() === permission.toLowerCase()) permissions.add(permission);
    }
  }
  // Some Cloud Run denials contain only a message. Match a quoted, known
  // permission, never echo the message, account, UID, request, or credential.
  if (typeof error?.message === "string") {
    for (const permission of PERMISSIONS) {
      if (error.message.toLowerCase().includes(`'${permission.toLowerCase()}'`) ||
          error.message.toLowerCase().includes(`"${permission.toLowerCase()}"`)) permissions.add(permission);
    }
  }
  const message = `Cloud recovery ${method} HTTP ${response.status}; phase=${phase}; ` +
    `google_status=${status}; reason=${[...reasons].sort().join(",") || "unknown"}; ` +
    `reported_permission=${[...permissions].sort().join(",") || "unknown"}; ` +
    "stop without widening IAM or retrying as deployer";
  const result = new Error(message);
  result.name = "RecoveryTrafficHttpError";
  result.httpStatus = response.status;
  result.phase = phase;
  return result;
}
