import { execFile, spawn } from "node:child_process";

// Each invocation owns its process group. Forward parent cancellation before
// exiting so nested adapters can finish terminating their own groups.
export function runBoundedCommand(executable, arguments_, {
  timeoutMs, maxBytes, label, signal, killGraceMs = 1_000,
}) {
  return new Promise((resolve, reject) => {
    const grouped = process.platform !== "win32";
    const child = spawn(executable, arguments_, {
      shell: false, windowsHide: true, detached: grouped,
      stdio: ["ignore", "pipe", "ignore"],
    });
    const chunks = [];
    let size = 0;
    let settled = false;
    let failure = null;
    let forceTimer;
    let drainTimer;
    const timeout = setTimeout(() => stop(new Error(`${label} timed out`)), timeoutMs);
    const cancelled = () => stop(new Error(`${label} was cancelled`));
    signal?.addEventListener("abort", cancelled, { once: true });
    process.on("SIGTERM", cancelled);
    process.on("SIGINT", cancelled);

    child.stdout.on("data", (chunk) => {
      if (failure || settled) return;
      size += chunk.length;
      if (size > maxBytes) {
        stop(new Error(`${label} exceeded its output bound`));
        return;
      }
      chunks.push(chunk);
    });
    child.once("error", () => finish(new Error(`${label} failed to start`)));
    child.once("close", (code, exitSignal) => {
      finish(failure ?? (code !== 0 || exitSignal
        ? new Error(`${label} did not exit successfully (exit=${code}, signal=${exitSignal ?? "none"})`)
        : null));
    });
    if (signal?.aborted) cancelled();

    function send(exitSignal) {
      if (!child.pid) return;
      try {
        if (grouped) process.kill(-child.pid, exitSignal);
        else {
          execFile("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"],
            { windowsHide: true, timeout: 2_000 }, () => {});
        }
      } catch (error) {
        if (error.code !== "ESRCH") failure ??= error;
      }
    }

    function stop(error) {
      if (settled || failure) return;
      failure = error;
      clearTimeout(timeout);
      send("SIGTERM");
      forceTimer = setTimeout(() => send("SIGKILL"), killGraceMs);
      // Bound even stream drainage after forced termination.
      drainTimer = setTimeout(() => {
        send("SIGKILL");
        child.stdout.destroy();
        child.unref();
        finish(failure);
      }, killGraceMs + 2_000);
    }

    function finish(error) {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      clearTimeout(forceTimer);
      clearTimeout(drainTimer);
      signal?.removeEventListener("abort", cancelled);
      process.removeListener("SIGTERM", cancelled);
      process.removeListener("SIGINT", cancelled);
      if (error) reject(error);
      else resolve(Buffer.concat(chunks));
    }
  });
}
