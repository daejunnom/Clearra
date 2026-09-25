// Read-only diagnosis against public artifacts; never changes the deployed application.
import { mkdir, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import path from 'node:path';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.BROWSER_ROOT + '/node_modules/playwright');
const base = 'https://daejunnom.github.io/Clearra/';
const root = process.env.AUDIT_ROOT;
await mkdir(root, { recursive: true });
const browser = await chromium.launch({ headless: true });
const report = { schema: 'clearra.live-worker-diagnosis.v1', startedAt: new Date().toISOString(), cases: [] };
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function inspectWorkers() {
  const cdp = await browser.newBrowserCDPSession();
  const { targetInfos } = await cdp.send('Target.getTargets');
  const results = [];
  for (const target of targetInfos.filter(value => value.type === 'worker' && value.url.startsWith(base)).slice(0, 5)) {
    const item = { target, events: [] }; results.push(item);
    let sessionId;
    try {
      ({ sessionId } = await cdp.send('Target.attachToTarget', { targetId: target.targetId, flatten: false }));
      let next = 1; const pending = new Map();
      const receive = event => {
        if (event.sessionId !== sessionId) return;
        const value = JSON.parse(event.message);
        if (value.id && pending.has(value.id)) { pending.get(value.id)(value); pending.delete(value.id); }
        else if (['Debugger.paused', 'Runtime.exceptionThrown'].includes(value.method)) item.events.push(value);
      };
      cdp.on('Target.receivedMessageFromTarget', receive);
      async function command(method, params = {}) {
        const id = next++;
        let timer;
        const result = new Promise(resolve => {
          pending.set(id, value => { clearTimeout(timer); resolve(value); });
          timer = setTimeout(() => { pending.delete(id); resolve({ error: { message: method + ' did not answer' } }); }, 7000);
        });
        await cdp.send('Target.sendMessageToTarget', { sessionId, message: JSON.stringify({ id, method, params }) });
        return result;
      }
      item.debugger = await command('Debugger.enable', { maxScriptsCacheSize: 4 * 1024 * 1024 });
      item.pause = await command('Debugger.pause');
      await delay(500);
      const paused = item.events.find(value => value.method === 'Debugger.paused');
      if (paused) {
        item.stack = paused.params.callFrames.slice(0, 12).map(frame => ({ functionName: frame.functionName, location: frame.location, url: frame.url }));
        for (const frame of paused.params.callFrames.filter(frame => !frame.url.startsWith('wasm:')).slice(0, 2)) {
          const source = await command('Debugger.getScriptSource', { scriptId: frame.location.scriptId });
          const lines = source.result?.scriptSource?.split('\n');
          const text = lines?.[frame.location.lineNumber];
          if (text) {
            const column = frame.location.columnNumber ?? 0;
            (item.sourceExcerpts ??= []).push({ url: frame.url, location: frame.location, text: text.slice(Math.max(0, column - 450), column + 850) });
          }
        }
      }
      item.resume = await command('Debugger.resume');
      cdp.off('Target.receivedMessageFromTarget', receive);
      await cdp.send('Target.detachFromTarget', { sessionId });
    } catch (error) { item.error = String(error); }
  }
  await cdp.detach();
  return results;
}
try {
  const context = await browser.newContext({ locale: 'ko-KR', viewport: { width: 1440, height: 1000 }, serviceWorkers: 'block' });
  const observed = { name: 'unmodified-default-public-ui', network: [], console: [], workerLifetime: [], pageErrors: [] };
  context.on('response', response => {
    const url = response.url();
    if (/wasm|Worker|worker/.test(url)) observed.network.push({ url, status: response.status() });
  });
  context.on('requestfailed', request => observed.network.push({ url: request.url(), failure: request.failure() }));
  const page = await context.newPage();
  page.on('console', message => { if (observed.console.length < 80) observed.console.push({ type: message.type(), text: message.text().slice(0, 3000), location: message.location() }); });
  page.on('pageerror', error => observed.pageErrors.push(String(error)));
  page.on('worker', worker => { observed.workerLifetime.push({ event: 'created', url: worker.url() }); worker.on('close', () => observed.workerLifetime.push({ event: 'closed', url: worker.url() })); });
  await page.goto(base + '?tool=pc', { waitUntil: 'domcontentloaded', timeout: 60000 });
  await page.locator('.workspace-queue-input').waitFor({ timeout: 30000 });
  await page.locator('.fumen-import input:not([type="file"])').first().fill('ctk3_w0kCQBhwwAEHHABh4Q');
  await page.locator('.fumen-import button').filter({ hasText: '필드 불러오기' }).last().click();
  await page.locator('.workspace-queue-input').fill('STOILJZ');
  await page.getByRole('button', { name: '탐색 실행', exact: true }).first().click();
  await delay(25000);
  observed.text = await page.locator('body').innerText();
  observed.workers = await inspectWorkers();
  await page.screenshot({ path: path.join(root, 'unmodified-ui.png'), fullPage: true });
  report.cases.push(observed);
  console.log('UNMODIFIED_UI ' + JSON.stringify(observed));
  await context.close();

  const cold = await browser.newContext({ locale: 'ko-KR', serviceWorkers: 'block' });
  const coldPage = await cold.newPage();
  // The static identity URL establishes same-origin without mounting the application.
  await coldPage.goto(base + 'clearra-build-identity.json', { waitUntil: 'domcontentloaded' });
  const result = await coldPage.evaluate(async base => {
    const identity = await (await fetch(base + 'clearra-build-identity.json', { cache: 'no-store' })).json();
    const script = identity.files.find(file => /\/workers\/clearraWorker-[^/]+\.js$/.test(file.path));
    if (!script) throw new Error('Published root worker was not listed in identity');
    const worker = new Worker(base + script.path, { type: 'module' });
    const events = []; const errors = [];
    worker.addEventListener('message', message => events.push(message.data));
    worker.addEventListener('error', event => errors.push(event.message));
    worker.postMessage({
      type: 'run_command_text',
      commandText: 'clearra --format json rules list',
      prewarmWorkerCount: 1,
      tablebaseRequested: false,
      lifecycleOwnerId: 'read-only-cold-worker-audit',
      hostCapabilitySnapshot: { schemaVersion: 1, snapshotId: 'read-only-cold-worker-audit', source: 'browser-main', reportedLogicalProcessors: 1, automaticWorkerCap: 1, reportedDeviceMemoryGiB: 8, wasmTransferByteCap: 134217728, webGpuAvailable: false, crossOriginIsolated: false },
      workerAuthority: { snapshotId: 'read-only-cold-worker-audit', reportedLogicalProcessors: 1, workersRequested: 1, workersEffective: 1, reason: 'explicit-request' },
      warmupPolicy: { backend: 'cpu', cpuWarmup: false, gpuWarmup: false }
    });
    await new Promise(resolve => setTimeout(resolve, 25000));
    window.__coldAuditWorker = worker;
    return { name: 'cold-worker-no-warmup-rules', events, errors };
  }, base);
  result.workers = await inspectWorkers();
  report.cases.push(result);
  console.log('COLD_WORKER ' + JSON.stringify(result));
  await cold.close();
} catch (error) { report.error = String(error); console.error(error); }
finally { await browser.close(); }
await writeFile(path.join(root, 'worker-diagnosis.json'), JSON.stringify(report, null, 2) + '\n');
if (report.error) process.exitCode = 1;
