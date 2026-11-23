const MAX_TIMER_DELAY_MS = 2 ** 31 - 1;

export function nonnegativeInteger(value, name) {
  const parsed = Number(value);
  if (!/^(?:0|[1-9]\d*)$/u.test(String(value)) || !Number.isSafeInteger(parsed)) {
    throw new Error(`${name} must be a nonnegative safe integer (0 means unlimited)`);
  }
  return parsed;
}

// A zero search budget creates no timer. Finite, explicitly requested budgets
// are split into representable timer delays so Node/browser integer overflow
// cannot turn a long search budget into an immediate cancellation.
export function scheduleSearchDeadline(timeoutMs, onDeadline, {
  now = () => performance.now(),
  setTimer = setTimeout,
  clearTimer = clearTimeout,
} = {}) {
  const budgetMs = nonnegativeInteger(timeoutMs, 'timeout');
  if (budgetMs === 0) return () => {};
  const deadline = now() + budgetMs;
  let timer = null;
  let cancelled = false;
  const arm = () => {
    if (cancelled) return;
    const remaining = deadline - now();
    if (remaining <= 0) {
      cancelled = true;
      timer = null;
      onDeadline();
      return;
    }
    timer = setTimer(arm, Math.min(MAX_TIMER_DELAY_MS, Math.ceil(remaining)));
  };
  arm();
  return () => {
    cancelled = true;
    if (timer !== null) clearTimer(timer);
    timer = null;
  };
}
