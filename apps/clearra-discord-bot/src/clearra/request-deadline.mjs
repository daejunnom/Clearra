export function assertRequestActive(options = {}) {
  if (options.signal?.aborted) {
    const error = new Error("Clearra request was cancelled.");
    error.name = "AbortError";
    throw error;
  }
  if (options.deadlineUnixMs !== undefined) {
    if (!Number.isSafeInteger(options.deadlineUnixMs) || options.deadlineUnixMs <= Date.now()) {
      throw requestDeadlineError();
    }
  }
}

export function requestDeadlineError() {
  const error = new Error("Clearra request exceeded its absolute deadline.");
  error.name = "DeadlineExceededError";
  return error;
}

export function isRequestInterruption(error) {
  return error?.name === "DeadlineExceededError" || error?.name === "AbortError";
}

export async function withRequestPhase(options, phase, run) {
  const startedAt = Date.now();
  let failure;
  try {
    assertRequestActive(options);
    const result = await run();
    assertRequestActive(options);
    return result;
  } catch (error) {
    failure = error;
    throw error;
  } finally {
    const timings = options?.stageDurationsMs;
    if (timings) timings[phase] = (timings[phase] ?? 0) + Math.max(0, Date.now() - startedAt);
    if (failure && typeof failure === "object") {
      failure.failureStage ??= phase;
      if (timings) failure.stageDurationsMs = { ...timings };
    }
  }
}
