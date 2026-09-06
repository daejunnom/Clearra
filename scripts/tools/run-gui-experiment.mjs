#!/usr/bin/env node
// A leased audit server, not the persistent 4194 watchdog and not a search timeout.
import { fork } from 'node:child_process';
import { createRequire } from 'node:module';
import { createServer as createPortProbe } from 'node:net';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

export const EXPERIMENT_PORT = 4195;
export const DEFAULT_LEASE_MINUTES = 30;
const SELF = fileURLToPath(import.meta.url);

export function experimentOptions(args, cwd = process.cwd()) {
  const { values } = parseArgs({ args, strict: true, options: {
    'source-root': { type: 'string', default: cwd },
    'lease-minutes': { type: 'string', default: String(DEFAULT_LEASE_MINUTES) },
  } });
  const minutes = Number(values['lease-minutes']);
  if (!Number.isSafeInteger(minutes) || minutes < 1 || minutes > 120)
    throw new Error('lease-minutes must be an integer from 1 to 120');
  return { sourceRoot: resolve(values['source-root']), leaseMs: minutes * 60_000 };
}

export function assertPortUnused() {
  return new Promise((resolveProbe, reject) => {
    const probe = createPortProbe();
    probe.once('error', () => reject(new Error('4195 is occupied or unavailable; preserve the listener and clean it up manually')));
    probe.listen({ host: '127.0.0.1', port: EXPERIMENT_PORT, exclusive: true }, () => probe.close(resolveProbe));
  });
}

/** Controls only the child object this invocation created, never a port/PID lookup. */
export function watchOwnedChild(child, { leaseMs, processEvents = process,
  setTimer = setTimeout, clearTimer = clearTimeout } = {}) {
  return new Promise((resolveDone, reject) => {
    let finished = false;
    let stopping = false;
    let forceTimer;
    const stop = () => {
      if (finished || stopping) return;
      stopping = true;
      // Disconnect is also observed after parent crash/exit. The child owns its
      // own lease, so a wedged parent cannot leave an indefinite audit server.
      if (child.connected) child.disconnect();
      forceTimer = setTimer(() => { if (!finished) child.kill(); }, 5_000);
    };
    const leaseTimer = setTimer(stop, leaseMs);
    const clean = () => {
      finished = true;
      clearTimer(leaseTimer);
      if (forceTimer) clearTimer(forceTimer);
      processEvents.removeListener('SIGINT', stop);
      processEvents.removeListener('SIGTERM', stop);
      processEvents.removeListener('exit', stop);
    };
    processEvents.once('SIGINT', stop);
    processEvents.once('SIGTERM', stop);
    processEvents.once('exit', stop);
    child.once('error', error => { clean(); reject(error); });
    child.once('exit', (code, signal) => {
      clean();
      if (code === 0 || stopping) resolveDone();
      else reject(new Error(`owned experiment exited: ${signal ?? code}`));
    });
  });
}

export async function startExperiment(options, { checkPort = assertPortUnused,
  forkProcess = fork, watch = watchOwnedChild } = {}) {
  await checkPort();
  const child = forkProcess(SELF, ['--owned-server', '--source-root', options.sourceRoot,
    '--lease-minutes', String(options.leaseMs / 60_000)], {
    cwd: resolve(options.sourceRoot, 'apps/clearra-web'),
    execArgv: [], windowsHide: true, detached: false,
    stdio: ['ignore', 'inherit', 'inherit', 'ipc'],
  });
  return watch(child, options);
}

export async function serveOwnedExperiment(options, { events = process, loadVite,
  setTimer = setTimeout, clearTimer = clearTimeout, exit = code => process.exit(code) } = {}) {
  if (!events.connected) throw new Error('owned server requires its creating parent IPC connection');
  let server;
  let stopped = false;
  const stop = async (code = 0) => {
    if (stopped) return;
    stopped = true;
    const forceTimer = setTimer(() => exit(1), 4_000);
    try { await server?.close(); exit(code); }
    catch { exit(1); }
    finally { clearTimer(forceTimer); }
  };
  events.once('disconnect', stop);
  events.once('SIGINT', stop);
  events.once('SIGTERM', stop);
  const lease = setTimer(stop, options.leaseMs);
  try {
    const api = loadVite ? await loadVite() : await import(pathToFileURL(
      createRequire(resolve(options.sourceRoot, 'package.json')).resolve('vite')).href);
    if (stopped) return;
    server = await api.createServer({ root: resolve(options.sourceRoot, 'apps/clearra-web'),
      configFile: resolve(options.sourceRoot, 'apps/clearra-web/vite.config.ts'), mode: 'local-audit',
      server: { host: '127.0.0.1', port: EXPERIMENT_PORT, strictPort: true, hmr: false } });
    if (stopped) { await server.close(); return; }
    await server.listen(); // strictPort protects the race after the preflight probe.
    if (!stopped) process.stdout.write(`experiment=http://127.0.0.1:4195 lease_minutes=${options.leaseMs / 60_000} no_auto_restart=true\n`);
  } catch (error) {
    process.stderr.write(`gui_experiment_server_failed: ${error.message}\n`);
    await stop(1);
    throw error;
  }
  // The timer deliberately stays referenced for this one finite audit session.
  // It is not an HTTP-idle timer: browser-local WASM can compute without requests.
  return { stop, clearLease: () => clearTimer(lease) };
}

if (process.argv[1] && resolve(process.argv[1]) === SELF) {
  try {
    const child = process.argv[2] === '--owned-server';
    const options = experimentOptions(process.argv.slice(child ? 3 : 2));
    if (child) await serveOwnedExperiment(options);
    else await startExperiment(options);
  } catch (error) { process.stderr.write(`gui_experiment_failed: ${error.message}\n`); process.exitCode = 1; }
}
