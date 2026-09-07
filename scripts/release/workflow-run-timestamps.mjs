// SRP rationale: this module has one change reason: the permitted ordering of
// normalized GitHub workflow-run timestamps, not deployment/recovery authority.

// The attempt endpoint for recovery 34089672723/1 returned created_at
// 2026-09-07T06:10:41Z and run_started_at 2026-09-07T06:10:40Z. Accommodate
// only this one-second metadata skew. Do not rewrite timestamps or extend the
// upper bound used by artifact, checkpoint, or recovery-result verification.
const MAX_CREATION_START_SKEW_MS = 1_000;

export function workflowRunTimestampOrderIsValid(createdAt, startedAt, updatedAt) {
  if (!Number.isSafeInteger(createdAt) || !Number.isSafeInteger(updatedAt)) return false;
  if (createdAt > updatedAt) return false;
  if (startedAt === null) return true;
  if (!Number.isSafeInteger(startedAt) || startedAt > updatedAt) return false;
  return createdAt - startedAt <= MAX_CREATION_START_SKEW_MS;
}
