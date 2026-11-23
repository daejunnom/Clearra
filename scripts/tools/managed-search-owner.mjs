import { spawn } from 'node:child_process';

// The wrapper owns the manager's stdin pipe. Closing that pipe asks the
// manager to drain its process tree and publish its receipt before exiting.
// Keep the manager referenced: killing or unref'ing it would lose that proof.
export function createManagedSearchOwner({
  signalSource = process,
  spawnProcess = spawn,
  onCancel = () => {},
} = {}) {
  const active = new Map();
  let cancelledBy = null;
  let disposed = false;

  const endLease = (lease) => {
    if (lease.ended) return;
    lease.ended = true;
    if (!lease.stdin.destroyed && !lease.stdin.writableEnded) lease.stdin.end();
  };
  const cancel = (signal) => {
    if (cancelledBy !== null) return;
    cancelledBy = signal;
    for (const lease of active.values()) endLease(lease);
    onCancel(signal);
  };
  const listeners = new Map(['SIGINT', 'SIGTERM'].map((signal) => [signal, () => cancel(signal)]));
  for (const [signal, listener] of listeners) signalSource.on(signal, listener);

  const assertActive = () => {
    if (cancelledBy !== null || disposed) {
      const error = new Error(`Managed search owner ended${cancelledBy ? `: ${cancelledBy}` : ''}`);
      error.code = 'E_CLEARRA_OWNER_CANCELLED';
      throw error;
    }
  };
  return {
    get cancelledBy() { return cancelledBy; },
    assertActive,
    spawn(command, args, options = {}) {
      // This synchronous check also covers a signal received during an
      // awaited binary hash or any other admission preflight.
      assertActive();
      const child = spawnProcess(command, args, {
        // On POSIX, terminal signals must reach the wrapper and close its
        // lease instead of killing the manager before it can drain workers.
        ...options, detached: process.platform !== 'win32', stdio: ['pipe', 'pipe', 'pipe'],
      });
      const lease = { stdin: child.stdin, ended: false };
      active.set(child, lease);
      // A manager may exit before the owner closes its lease. EPIPE is a
      // normal consequence; process completion is reported by close/error.
      const ignorePipeError = () => {};
      child.stdin.on('error', ignorePipeError);
      child.once('close', () => {
        active.delete(child);
        child.stdin.removeListener('error', ignorePipeError);
      });
      return child;
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      for (const [signal, listener] of listeners) signalSource.removeListener(signal, listener);
      for (const lease of active.values()) endLease(lease);
    },
  };
}
