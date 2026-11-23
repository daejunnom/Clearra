export const CLOUD_RUN_REQUEST_TIMEOUT_MS = 900_000;
export const PLATFORM_RESPONSE_RESERVE_MS = 15_000;
export const PLATFORM_DEADLINE_CODE = "E_CLEARRA_PLATFORM_REQUEST_DEADLINE";

export function platformResponseDeadline(startedAt, timeoutMs, reserveMs) {
  if (timeoutMs === 0) return 0;
  if (!Number.isSafeInteger(startedAt) || startedAt < 0 ||
      !Number.isSafeInteger(timeoutMs) || timeoutMs < 1 ||
      !Number.isSafeInteger(reserveMs) || reserveMs < 1 || reserveMs >= timeoutMs) {
    throw new Error("Clearra platform request timing is invalid.");
  }
  const deadline = startedAt + timeoutMs - reserveMs;
  if (!Number.isSafeInteger(deadline)) {
    throw new Error("Clearra platform response deadline is out of range.");
  }
  return deadline;
}

export function platformDeadlineError() {
  const error = new Error(
    "The Cloud Run request deadline is approaching. " +
    "This attempt has failed and cancellation was requested; any later result will be discarded.",
  );
  error.code = PLATFORM_DEADLINE_CODE;
  return error;
}
