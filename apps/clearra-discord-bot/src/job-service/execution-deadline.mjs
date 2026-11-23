// Zero is an explicit absence of a computation deadline. Delivery and a
// platform's HTTP request lifetime are separate owners of finite deadlines.
export function earliestDeadline(...deadlines) {
  let earliest = 0;
  for (const deadline of deadlines) {
    if (deadline > 0 && (earliest === 0 || deadline < earliest)) earliest = deadline;
  }
  return earliest;
}

export function executionDeadline(now, timeoutMs, callerDeadline = 0) {
  return earliestDeadline(timeoutMs === 0 ? 0 : now + timeoutMs, callerDeadline);
}

export function armDeadline(deadline, callback, clock = {}) {
  if (deadline === 0) return () => {};
  const now = clock.now ?? Date.now;
  const schedule = clock.setTimeout ?? setTimeout;
  const cancel = clock.clearTimeout ?? clearTimeout;
  let timer = null;
  let stopped = false;
  const tick = () => {
    if (stopped) return;
    const remaining = deadline - now();
    if (remaining <= 0) {
      stopped = true;
      callback();
    } else {
      timer = schedule(tick, Math.min(remaining, 2_147_483_647));
    }
  };
  tick();
  return () => {
    stopped = true;
    if (timer !== null) cancel(timer);
  };
}
