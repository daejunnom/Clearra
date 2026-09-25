// Explicit diagnostic modes, not changes to users' requested worker counts or fallback policy.
import { mkdir, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import path from 'node:path';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.BROWSER_ROOT + '/node_modules/playwright');
const root = process.env.AUDIT_ROOT;
await mkdir(root, { recursive: true });
const browser = await chromium.launch({ headless: true });
const base = 'https://daejunnom.github.io/Clearra/';
const report = { schema: 'clearra.public-execution-matrix.v1', source: '952280950aeb24528279cbff685fe8acec892223', observedAt: new Date().toISOString(), cases: [] };
try {
  for (const mode of [
    { name: 'explicit-cpu-single', backend: 'cpu', workers: 1, webGpuAvailable: false },
    { name: 'explicit-cpu-three', backend: 'cpu', workers: 3, webGpuAvailable: false },
    { name: 'auto-three-adapter-unavailable', backend: 'auto', workers: 3, webGpuAvailable: true }
  ]) {
    const context = await browser.newContext({ serviceWorkers: 'block' });
    const page = await context.newPage();
    const consoleMessages = [];
    page.on('console', message => { if (consoleMessages.length < 20) consoleMessages.push({ type: message.type(), text: message.text().slice(0, 4000) }); });
    await page.goto(base + 'clearra-build-identity.json', { waitUntil: 'domcontentloaded', timeout: 45000 });
    const result = await page.evaluate(async ({ base, mode }) => {
      const identity = await (await fetch(base + 'clearra-build-identity.json', { cache: 'no-store' })).json();
      const script = identity.files.find(file => /\/workers\/clearraWorker-[^/]+\.js$/.test(file.path));
      if (!script) throw new Error('Published root worker is missing');
      let adapterStatus;
      try { adapterStatus = navigator.gpu ? (await navigator.gpu.requestAdapter() ? 'adapter-returned' : 'null-adapter') : 'api-absent'; }
      catch (error) { adapterStatus = String(error); }
      const worker = new Worker(base + script.path, { type: 'module' });
      const startedAt = performance.now();
      const events = []; const errors = [];
      let finish;
      const done = new Promise(resolve => { finish = resolve; });
      worker.addEventListener('message', message => {
        events.push(message.data);
        if (events.length > 30) events.splice(1, 1);
        if (['final_response', 'failed', 'cancelled', 'terminated'].includes(message.data.event)) finish('terminal');
      });
      worker.addEventListener('error', event => { errors.push(event.message); finish('worker-error'); });
      const snapshotId = 'live-audit-' + mode.name;
      const command = `clearra pc --lines 4 --board-mask 0x00000001c0701c07 --height 4 --pieces 7 --hold empty --queue STOILJZ --count unique --rule srs-plus --no-tablebase --no-build-dependency-dag --queue-knowledge oracle --backend ${mode.backend} --allow-backend-fallback --workers ${mode.workers}`;
      worker.postMessage({
        type: 'run_command_text', commandText: command,
        prewarmWorkerCount: mode.workers, tablebaseRequested: false, lifecycleOwnerId: snapshotId,
        hostCapabilitySnapshot: { schemaVersion: 1, snapshotId, source: 'browser-main', reportedLogicalProcessors: 4, automaticWorkerCap: 3, reportedDeviceMemoryGiB: 8, wasmTransferByteCap: 134217728, webGpuAvailable: mode.webGpuAvailable, crossOriginIsolated: false },
        workerAuthority: { snapshotId, reportedLogicalProcessors: 4, workersRequested: mode.workers, workersEffective: mode.workers, reason: 'explicit-request' },
        warmupPolicy: { backend: mode.backend, cpuWarmup: false, gpuWarmup: false }
      });
      let timer;
      const state = await Promise.race([done, new Promise(resolve => { timer = setTimeout(() => resolve('timed-out'), 45000); })]);
      clearTimeout(timer); worker.terminate();
      return { ...mode, adapterStatus, command, elapsedMs: performance.now() - startedAt, state, events, errors };
    }, { base, mode });
    result.console = consoleMessages;
    report.cases.push(result);
    console.log('EXECUTION_CASE ' + JSON.stringify(result));
    await context.close();
  }
} catch (error) { report.error = String(error); }
finally { await browser.close(); }
await writeFile(path.join(root, 'execution-matrix.json'), JSON.stringify(report, null, 2) + '\n');
// This is diagnostic collection, not product acceptance. Conclusions remain in each case.
if (report.error) { console.error(report.error); process.exitCode = 1; }
