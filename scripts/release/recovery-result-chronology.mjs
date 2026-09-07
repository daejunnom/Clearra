// SRP: compare a verified recovery report with its bound Actions chronology.
// This module grants no run, artifact, IAM, or deployment authority.

const UTC_TIMESTAMP = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z$/u;
const ERROR_PREFIX = "Discord recovery result chronology differs from its exact run attempt";

function parseTimestamp(value, field) {
  if (typeof value !== "string" || !UTC_TIMESTAMP.test(value)) {
    throw new Error(`${ERROR_PREFIX} (check=invalid-${field})`);
  }
  const milliseconds = Date.parse(value);
  const canonical = value.includes(".") ? value : value.replace(/Z$/u, ".000Z");
  if (!Number.isSafeInteger(milliseconds) || new Date(milliseconds).toISOString() !== canonical) {
    throw new Error(`${ERROR_PREFIX} (check=invalid-${field})`);
  }
  return milliseconds;
}

export function validateRecoveryResultChronology({
  resolutionCreatedAt,
  resultCreatedAt,
  recoveredAt,
  recoveryStartedAt,
  recoveryUpdatedAt,
}) {
  const resolution = parseTimestamp(resolutionCreatedAt, "resolution-created-at");
  const artifact = parseTimestamp(resultCreatedAt, "result-created-at");
  const recovered = parseTimestamp(recoveredAt, "recovered-at");
  const started = parseTimestamp(recoveryStartedAt, "recovery-started-at");
  const updated = parseTimestamp(recoveryUpdatedAt, "recovery-updated-at");

  // Actions returned created_at=15:18:02Z for the digest-verified result whose
  // recovered_at is 15:18:02.034Z (recovery 34137249254/1). A whole-second
  // creation projection does not assert .000. Compare within that reported
  // second only; an explicitly millisecond-precise upper bound stays exact.
  // No clock-skew allowance, report restamping, or rounding of the run bounds.
  const artifactUpperInclusive = artifact + (resultCreatedAt.includes(".") ? 0 : 999);
  const failedCheck = artifact < resolution ? "artifact-before-resolution"
    : recovered < resolution ? "recovered-before-resolution"
    : recovered < started ? "recovered-before-run"
    : recovered > artifactUpperInclusive ? "recovered-after-artifact-precision"
    : recovered > updated ? "recovered-after-run"
    : null;
  if (failedCheck !== null) throw new Error(`${ERROR_PREFIX} (check=${failedCheck})`);
}
