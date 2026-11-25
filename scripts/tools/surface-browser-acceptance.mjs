// Real compiled Svelte + real WASM. GPU discovery and the dedicated tiling
// cases' advertised logical capacity are fixtures; computation is real.
// This is an acceptance test: every assertion and timeout is a failing exit.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir, stat, writeFile } from 'node:fs/promises';
import { resolve, extname, sep } from 'node:path';
import { createRequire } from 'node:module';
import { assertRecoveryGallery } from './recovery-gallery-contract.mjs';
import { expectedWasmProbeIdentity, validateWasmProbeTerminal } from './wasm-product-terminal-contract.mjs';
const require = createRequire(resolve(process.env.CLEARRA_BROWSER_TOOLS_ROOT, 'package.json'));
const { chromium } = require('playwright');
const root = resolve(process.argv[2] || 'apps/clearra-web/build');
const reportRoot = resolve(process.env.RUNNER_TEMP || 'build', 'clearra-surface-acceptance');
await mkdir(reportRoot, { recursive: true });
const base = process.env.CLEARRA_WEB_BASE_PATH || '';
assert.match(base, /^(?:\/[A-Za-z0-9._-]+)?$/u);
const mime = { '.js': 'text/javascript', '.json': 'application/json', '.css': 'text/css', '.wasm': 'application/wasm', '.html': 'text/html', '.svg': 'image/svg+xml' };
const server = createServer(async (req, res) => {
  try {
    const path = decodeURIComponent(new URL(req.url, 'http://localhost').pathname);
    if (!path.startsWith(base + '/') && path !== base) { res.writeHead(404).end(); return; }
    let file = resolve(root, '.' + path.slice(base.length));
    if (file !== root && !file.startsWith(root + sep)) { res.writeHead(403).end(); return; }
    if (!(await stat(file).catch(() => null))?.isFile()) file = resolve(root, 'index.html');
    res.writeHead(200, { 'Content-Type': mime[extname(file)] || 'application/octet-stream', 'Cache-Control': 'no-store' });
    res.end(await readFile(file));
  } catch { res.writeHead(500).end(); }
});
await new Promise((ok, fail) => { server.once('error', fail); server.listen(4194, '127.0.0.1', ok); });
const browser = await chromium.launch({ headless: true });
const results = [];
const failures = [];
function recordDiagnostic(entries, entry) {
  if (entries.length < 16) entries.push(entry);
}
function diagnosticUrl(value) {
  try {
    const url = new URL(value);
    return `${url.origin}${url.pathname}`.slice(0, 2048);
  } catch { return 'unavailable'; }
}
function gpuShim(mode) {
  const body = mode === 'null' ? 'Promise.resolve(null)' : mode === 'reject' ? "Promise.reject(new Error('simulated adapter refusal'))" : 'new Promise(() => {})';
  return `Object.defineProperty(navigator, 'gpu', { configurable: true, value: { requestAdapter: () => ${body} } });`;
}
async function context(mode, logicalProcessors) {
  const ctx = await browser.newContext({ locale: 'en-US', viewport: { width: 1440, height: 1100 } });
  const capacity = logicalProcessors === undefined ? '' :
    `globalThis.__clearraAcceptanceActualLogical = navigator.hardwareConcurrency;
     Object.defineProperty(navigator, 'hardwareConcurrency', { configurable: true, get: () => ${logicalProcessors} });`;
  const shim = gpuShim(mode) + '\n' + capacity;
  await ctx.addInitScript(shim);
  // Preserve real worker construction, URL/base-path, lifecycle and all messages.
  await ctx.route(/\/_app\/immutable\/workers\/[^/]+\.js(?:\?.*)?$/, async route => {
    const response = await route.fetch();
    await route.fulfill({ response, body: shim + '\n' + await response.text() });
  });
  return ctx;
}
// A completed click is not a completed SvelteKit view transition. Both the
// URL and the destination component's own active tab must agree before any
// shared control is queried. Keep strict selection; never pick an old input.
async function navigateWorkspace(page, tool) {
  assert.match(tool, /^[a-z][a-z-]*$/u);
  const destination = new URL(`?tool=${tool}`, page.url());
  const tab = `.product-tabs a[href="?tool=${tool}"]`;
  await page.locator(tab).click();
  await page.waitForURL(destination.href, { waitUntil: 'commit', timeout: 30000 });
  const workspace = page.locator('main.app-shell').filter({
    has: page.locator(`${tab}[aria-current="page"]`)
  });
  await workspace.waitFor({ state: 'visible', timeout: 30000 });
  return workspace;
}
async function keys(page) {
  return page.locator('.solution-gallery > li[data-solution-key]').evaluateAll(nodes => nodes.map(n => n.dataset.solutionKey));
}
// Arm before clicking: an old Completed result must not satisfy a new run.
// Observe the real header lifecycle, including short jobs between polling ticks.
async function completeRun(page, action, label) {
  await page.evaluate(() => {
    window.__clearraAcceptanceRun?.observer.disconnect();
    const status = document.querySelector('.header-status > span');
    if (!status) throw new Error('workspace status is missing');
    const state = { started: false, status: null, observer: null };
    const observer = new MutationObserver(records => {
      const active = status.classList.contains('running');
      state.started ||= active || records.some(record => record.type === 'attributes' &&
        record.attributeName === 'class' && (record.oldValue || '').split(/\s+/u).includes('running'));
      const text = status.textContent.trim();
      if (state.started && !active && ['Completed', 'Failed', 'Cancelled', 'Terminated'].includes(text)) {
        state.status = text;
        observer.disconnect();
      }
    });
    state.observer = observer;
    window.__clearraAcceptanceRun = state;
    observer.observe(status, { attributes: true, attributeOldValue: true,
      childList: true, subtree: true, characterData: true });
  });
  try {
    await action();
    await page.waitForFunction(() => window.__clearraAcceptanceRun.status !== null, null, { timeout: 60000 });
    const status = await page.evaluate(() => window.__clearraAcceptanceRun.status);
    assert.equal(status, 'Completed', `${label}: execution did not complete successfully`);
  } finally {
    await page.evaluate(() => window.__clearraAcceptanceRun?.observer.disconnect());
  }
}
async function completedKeys(page, count, label) {
  // Computation and lazy result-page loading have separate completion points.
  // Wait for the real page loader, never for a particular expected key count.
  await page.locator('.pager-loading').waitFor({ state: 'hidden', timeout: 60000 });
  const actual = await keys(page);
  assert.equal(actual.length, count, `${label}: unexpected complete solution set: ${JSON.stringify(actual)}`);
  assert.equal(new Set(actual).size, count, `${label}: duplicate solution keys`);
  return actual;
}
const LEFT_I = 'ctk1|initial=0000000000000000|placements=I:000000000000000f';
const RIGHT_I = 'ctk1|initial=0000000000000000|placements=I:00000000000003c0';
const CENTER_I = 'ctk1|initial=0000000000000000|placements=I:0000000000000078';

async function paint(page, index, mask, height) {
  const board = page.locator('.board-tool .board').nth(index);
  assert.equal(await board.locator('button').count(), height * 10,
    'paint coordinates must match the actual committed field height');
  for (let y = 0; y < height; y++) for (let x = 0; x < 10; x++) {
    if ((mask & (1n << BigInt(y * 10 + x))) !== 0n) await board.locator('button').nth((height - 1 - y) * 10 + x).click();
  }
}
// Read only the selected draft, not the gray reference layers. The button
// order is top-down, whereas field masks use bottom-up bit indices.
async function readPaintedMask(page, index, height) {
  const cells = page.locator('.board-tool .board').nth(index).locator('button');
  assert.equal(await cells.count(), height * 10);
  const selected = await cells.evaluateAll(nodes => nodes.map(node => node.getAttribute('aria-pressed') === 'true'));
  let mask = 0n;
  for (let i = 0; i < selected.length; i++) {
    if (selected[i]) mask |= 1n << BigInt((height - 1 - Math.floor(i / 10)) * 10 + i % 10);
  }
  return mask;
}

function emptyOTilingKey(height) {
  const placements = [];
  for (let y = 0; y < height; y += 2) for (let x = 0; x < 10; x += 2) {
    const mask = 0xc03n << BigInt(y * 10 + x);
    placements.push(`O:${mask.toString(16).padStart(16, '0')}`);
  }
  return `ctk1|initial=0000000000000000|placements=${placements.join(',')}`;
}

function assertTilingFinal(final, height, expectedIdentity) {
  assert.equal(final?.event, 'final_response');
  assert.equal(final.response.status, 'success', JSON.stringify(final.response.diagnostics));
  assert.equal(final.response.result?.kind, 'pc-tiling-family.v1');
  assert.deepEqual(final.response.runtime_identity, expectedIdentity);
  const report = final.search_report;
  assert.ok(report, 'typed tiling must publish its real search report');
  assert.equal(report.count_complete, true);
  assert.equal(report.solution_keys_complete, true);
  assert.equal(report.resource_truncated, false);
  assert.equal(report.unique_solution_count, 1);
  assert.deepEqual(report.normalized_solution_keys, [emptyOTilingKey(height)]);
  if (height === 2) assert.equal(report.normalized_solution_set_hash, 'cts1:21fffe814548e2c1');
  return report;
}

async function tilingUiCase({ height, hold, workers }, expectedIdentity) {
  const label = `tiling-${height}L-hold-${hold ? 'on' : 'off'}-workers-${workers}`;
  const ctx = await context('null', workers);
  await ctx.addInitScript(() => {
    window.__clearraTilingObserved = { commands: [], terminals: [], workerErrors: [] };
    const Original = window.Worker;
    window.Worker = class extends Original {
      constructor(...args) {
        super(...args);
        this.acceptanceOwnsRun = false;
        this.addEventListener('message', event => {
          if (this.acceptanceOwnsRun &&
              ['final_response', 'failed', 'cancelled', 'terminated'].includes(event.data?.event)) {
            window.__clearraTilingObserved.terminals.push(event.data);
          }
        });
        this.addEventListener('error', event => {
          window.__clearraTilingObserved.workerErrors.push(String(event.message));
        });
      }
      postMessage(...args) {
        if (args[0]?.type === 'run_command_text') {
          this.acceptanceOwnsRun = true;
          window.__clearraTilingObserved.commands.push(args[0].commandText);
        }
        return super.postMessage(...args);
      }
    };
  });
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  try {
    await page.goto(`http://127.0.0.1:4194${base}/?tool=pc`);
    await page.locator('.product-tabs').waitFor();
    await page.locator('.dimension-field input').fill(String(height));
    await page.waitForFunction(expected =>
      document.querySelector('.board-tool .board')?.querySelectorAll('button').length === expected, height * 10);
    assert.equal(await readPaintedMask(page, 0, height), 0n);
    await page.locator('.workspace-queue-input').fill('O'.repeat(height * 5 / 2));
    await page.locator('[data-pc-control="result-mode"]').selectOption('tiling');
    await page.getByRole('checkbox', { name: 'Hold', exact: true }).setChecked(hold);
    await page.locator('[data-pc-control="all-logical-processors"]').check();
    await page.waitForFunction(expected =>
      document.querySelector('.worker-authority')?.getAttribute('data-workers-effective') === String(expected), workers);
    assert.equal(await page.locator('.worker-authority').getAttribute('data-workers-requested'), String(workers));
    assert.equal(await page.locator('.workspace-validation li').count(), 0);
    await completeRun(page, () => page.getByRole('button', { name: 'Run search', exact: true }).click(), label);
    const observed = await page.evaluate(() => window.__clearraTilingObserved);
    assert.deepEqual(errors, []);
    assert.deepEqual(observed.workerErrors, []);
    assert.equal(observed.commands.length, 1, 'one UI action must issue exactly one product command');
    assert.match(observed.commands[0], /^clearra pc tiling\b/u);
    assert.match(observed.commands[0], new RegExp(`(?:^| )--workers ${workers}(?: |$)`, 'u'));
    assert.match(observed.commands[0], /(?:^| )--use-all-cpu-threads(?: |$)/u);
    assert.equal(observed.terminals.length, 1, 'typed tiling must emit one successful terminal');
    const report = assertTilingFinal(observed.terminals[0], height, expectedIdentity);
    assert.equal(report.workers_used, workers, 'typed tiling must retain the requested compute pool width');
    assert.deepEqual(await completedKeys(page, 1, label), [emptyOTilingKey(height)]);
    await writeFile(resolve(reportRoot, `${label}.json`), JSON.stringify({
      command: observed.commands[0], terminal: observed.terminals[0]
    }, null, 2) + '\n');
    results.push({ case: label, result: 'passed', height, hold, advertisedLogicalProcessors: workers,
      actualLogicalProcessors: await page.evaluate(() => window.__clearraAcceptanceActualLogical),
      requestedWorkers: workers, workersUsed: report.workers_used,
      solutionCount: 1, normalizedSolutionSetHash: report.normalized_solution_set_hash });
  } catch (error) {
    const failure = error instanceof Error ? error : new Error(String(error));
    const observed = await page.evaluate(() => window.__clearraTilingObserved).catch(() => null);
    await page.screenshot({ path: resolve(reportRoot, `failed-${label}.png`), fullPage: true }).catch(() => {});
    await writeFile(resolve(reportRoot, `failed-${label}.json`), JSON.stringify({
      source: process.env.CLEARRA_SOURCE_COMMIT, failure: failure.stack, errors, observed,
      body: (await page.locator('body').innerText().catch(() => 'unavailable')).slice(0, 16384)
    }, null, 2) + '\n');
    failures.push(failure);
    results.push({ case: label, result: 'failed', message: failure.message });
    console.error(`surface_case=${label} status=failed: ${failure.stack}`);
  } finally { await ctx.close(); }
}

// The original product WASM runs in a real browser worker. The page only
// observes it and owns the same 60-second acceptance deadline as UI cases.
// Long inputs stop after preparation; only the finite all-O tiling is searched.
function stagedTilingProbeWorker() {
  self.onmessage = async ({ data }) => {
    try {
      const bindings = await import(data.bindingsUrl);
      const api = await bindings.default({ module_or_path: data.wasmUrl });
      if (api.clearra_wasm_abi_version() !== 1) throw new Error('unsupported WASM ABI');
      const output = () => {
        try {
          return new TextDecoder().decode(new Uint8Array(api.memory.buffer,
            api.clearra_wasm_output_ptr(), api.clearra_wasm_output_len()));
        } finally { api.clearra_wasm_output_release(); }
      };
      const requireOk = (status, operation) => {
        if (status < 0) throw new Error(`${operation}: ${output()}`);
        return status;
      };
      const command = text => {
        const bytes = new TextEncoder().encode(text);
        requireOk(api.clearra_wasm_input_resize(bytes.length), 'input resize');
        new Uint8Array(api.memory.buffer, api.clearra_wasm_input_ptr(), bytes.length).set(bytes);
      };
      requireOk(api.clearra_wasm_configure_host(8, 0), 'host configuration');
      const preparations = [];
      for (const text of data.prepareCommands) {
        command(text);
        const mode = requireOk(api.clearra_wasm_distributed_prepare(), `distributed preparation: ${text}`);
        preparations.push({ command: text, mode,
          workers: api.clearra_wasm_distributed_worker_count(),
          completionWorkers: api.clearra_wasm_distributed_completion_worker_count(),
          rootTaskParallel: api.clearra_wasm_distributed_root_task_parallel() !== 0 });
        requireOk(api.clearra_wasm_distributed_reset(), 'preparation reset');
      }
      command(data.runCommand);
      const job = api.clearra_wasm_start_job();
      if (job === 0) throw new Error(`start job: ${output()}`);
      const events = [];
      let status = 0;
      while (status === 0 || status === 4) {
        status = requireOk(api.clearra_wasm_advance_job(job, 8192), 'advance job');
        requireOk(api.clearra_wasm_drain_job_events(job), 'drain events');
        events.push(...JSON.parse(output()).filter(event =>
          ['final_response', 'failed', 'cancelled', 'terminated'].includes(event.event)));
      }
      self.postMessage({ preparations, terminalStatus: status, events });
    } catch (error) {
      self.postMessage({ error: error instanceof Error ? error.stack : String(error) });
    }
  };
}

async function tilingArtifactCase(expectedIdentity, manifest) {
  const label = 'typed-tiling-artifact-preparation-and-initial-hold';
  const ctx = await context('null');
  const page = await ctx.newPage();
  const held = draws => `clearra pc tiling --lines 6 --board-mask 0 --height 6 --pieces 15 --queue ${'O'.repeat(draws)} --hold O --backend cpu`;
  const prepareCommands = [
    'clearra pc tiling --lines 2 --queue OOOOO --backend cpu',
    'clearra pc tiling --lines 2 --board-mask 0 --height 2 --pieces 5 --queue OOOOO --no-hold --backend cpu',
    held(14), held(16)
  ].map(command => `${command} --workers 8 --use-all-cpu-threads`);
  const runCommand = `${held(14)} --workers 1 --use-all-cpu-threads`;
  try {
    await page.goto(`http://127.0.0.1:4194${base}/?tool=pc`);
    await page.evaluate(({ source, prepareCommands, runCommand, base, bindingsPath, wasmPath }) => {
      const url = URL.createObjectURL(new Blob([`(${source})()`], { type: 'text/javascript' }));
      const worker = new Worker(url, { type: 'module' });
      const state = { worker, url, outcome: null };
      window.__clearraTilingArtifact = state;
      worker.onmessage = event => { state.outcome = event.data; };
      worker.onerror = event => { state.outcome = { error: String(event.message) }; };
      worker.postMessage({ prepareCommands, runCommand,
        bindingsUrl: new URL(`${base}/wasm/${bindingsPath}`, location.origin).href,
        wasmUrl: new URL(`${base}/wasm/${wasmPath}`, location.origin).href });
    }, { source: stagedTilingProbeWorker.toString(), prepareCommands, runCommand, base,
      bindingsPath: manifest.bindings.path, wasmPath: manifest.wasm.path });
    await page.waitForFunction(() => window.__clearraTilingArtifact.outcome !== null, null, { timeout: 60000 });
    const outcome = await page.evaluate(() => window.__clearraTilingArtifact.outcome);
    await writeFile(resolve(reportRoot, `${label}.json`), JSON.stringify({
      source: process.env.CLEARRA_SOURCE_COMMIT, runCommand, ...outcome
    }, null, 2) + '\n');
    assert.equal(outcome.error, undefined, outcome.error);
    assert.equal(outcome.preparations.length, prepareCommands.length);
    for (const [index, preparation] of outcome.preparations.entries()) {
      assert.equal(preparation.command, prepareCommands[index]);
      assert.equal(preparation.mode, 1, 'one natural root must retain the typed distributed coordinator');
      assert.equal(preparation.rootTaskParallel, true);
      assert.equal(preparation.workers, 8, 'request width is independent of currently active roots');
      assert.equal(preparation.completionWorkers, 8);
    }
    const final = validateWasmProbeTerminal({ ...outcome, expectedRuntimeIdentity: expectedIdentity });
    const report = assertTilingFinal(final, 6, expectedIdentity);
    results.push({ case: label, result: 'passed', preparedCases: prepareCommands.length,
      geometryLocks: 15, initialHold: 'O', sourceDraws: 14, solutionCount: 1,
      normalizedSolutionSetHash: report.normalized_solution_set_hash });
  } catch (error) {
    const failure = error instanceof Error ? error : new Error(String(error));
    await writeFile(resolve(reportRoot, `failed-${label}.json`), JSON.stringify({
      source: process.env.CLEARRA_SOURCE_COMMIT, failure: failure.stack, prepareCommands, runCommand,
      outcome: await page.evaluate(() => window.__clearraTilingArtifact?.outcome).catch(() => null)
    }, null, 2) + '\n');
    failures.push(failure);
    results.push({ case: label, result: 'failed', message: failure.message });
    console.error(`surface_case=${label} status=failed: ${failure.stack}`);
  } finally {
    await page.evaluate(() => {
      const state = window.__clearraTilingArtifact;
      state?.worker.terminate();
      if (state) URL.revokeObjectURL(state.url);
    }).catch(() => {});
    await ctx.close();
  }
}
try {
  const manifest = JSON.parse(await readFile(resolve(root, 'wasm/clearra_wasm.manifest.json'), 'utf8'));
  assert.equal(manifest.build.runtime_identity.source_commit, process.env.CLEARRA_SOURCE_COMMIT);
  const expectedIdentity = expectedWasmProbeIdentity(manifest, process.env.CLEARRA_SOURCE_COMMIT);
  let reference;
  let completeBagReference;
  for (const mode of ['null', 'reject', 'timeout']) {
    const ctx = await context(mode);
    const page = await ctx.newPage();
    const errors = [];
    const browserConsole = [];
    const failedRequests = [];
    const httpErrors = [];
    page.on('pageerror', error => errors.push(error.stack || String(error)));
    page.on('console', message => {
      if (message.type() === 'error' || message.type() === 'warning') {
        recordDiagnostic(browserConsole, { type: message.type(), text: message.text().slice(0, 2048) });
      }
    });
    page.on('requestfailed', request => recordDiagnostic(failedRequests, {
      url: diagnosticUrl(request.url()),
      error: (request.failure()?.errorText || 'unknown').slice(0, 2048)
    }));
    page.on('response', response => {
      if (response.status() >= 400) recordDiagnostic(httpErrors, {
        url: diagnosticUrl(response.url()), status: response.status()
      });
    });
    try {
      await page.goto(`http://127.0.0.1:4194${base}/?tool=pc`);
      await page.locator('.product-tabs').waitFor();
      const nav = await page.locator('.product-tabs a').evaluateAll(nodes => nodes.map(n => n.getAttribute('href')));
      const publishedTools = ['pc', 'setup', 'build-probability', 'recovery', 'damage', 'spin-finder', 'ctk', 'player'];
      assert.deepEqual(nav, publishedTools.map(tool => `?tool=${tool}`), 'Pages must publish only the approved eight tools');
      const index = nav.indexOf('?tool=recovery');
      assert.equal(nav[index - 1], '?tool=build-probability');
      assert.equal(nav[index + 1], '?tool=damage');
      assert.ok(!nav.includes('?tool=build'), 'internal Build tools remain reachable only by explicit routes');

      // This field consumes all seven P7 pieces and has only one natural
      // multiset root. Test the real compiled GUI, not just a worker-only
      // command. GPU discovery may be absent, rejected, or never resolve;
      // none of those states may hold up the CPU-default product request.
      const logicalProcessors = await page.evaluate(() => navigator.hardwareConcurrency);
      assert.ok(Number.isSafeInteger(logicalProcessors) && logicalProcessors >= 2,
        'the compiled GUI parallel regression requires at least two logical processors');
      await page.locator('.dimension-field input').fill('4');
      await page.locator('.fumen-import input:not([type="file"])').fill('ctk3_w0kCMBhw9oAAAzRb');
      await page.locator('.fumen-import button').last().click();
      assert.equal(await readPaintedMask(page, 0, 4), 0x30080f67n,
        'the user CTK field must reach the real GUI without changing its physical cells');
      await page.locator('.workspace-queue-input').fill('P7');
      const everyCpu = page.locator('[data-pc-control="all-logical-processors"]');
      const resultMode = page.locator('[data-pc-control="result-mode"]');
      const solutionProbabilities = page.locator('[data-pc-control="solution-probabilities"]');
      const queueVisibility = page.locator('[data-pc-control="queue-visibility"]');
      const workerAuthority = page.locator('.worker-authority');
      await solutionProbabilities.check();
      await queueVisibility.selectOption('visible-7');
      for (const [scoreMode, useEveryCpu] of [
        ['summary', true], ['score-finder', false], ['score-minimals', true]
      ]) {
        await resultMode.selectOption(scoreMode);
        if (scoreMode === 'score-minimals') {
          assert.equal(await queueVisibility.isEnabled(), true,
            'minimum-score validation must not trap an uneditable visibility draft');
          assert.equal(await queueVisibility.inputValue(), 'visible-7');
          assert.equal(await page.locator('.workspace-validation li').count(), 1);
          await queueVisibility.selectOption('oracle');
          await page.locator('.workspace-validation li').waitFor({ state: 'hidden' });
        } else {
          assert.equal(await queueVisibility.isDisabled(), true);
          assert.equal(await queueVisibility.inputValue(), 'oracle',
            `${scoreMode} must display its actual queue policy`);
        }
        assert.equal(await solutionProbabilities.isDisabled(), true,
          `${scoreMode} must not offer a probability option its command rejects`);
        assert.equal(await solutionProbabilities.isChecked(), false,
          `${scoreMode} must not display the retained draft as active`);
        assert.equal(await everyCpu.isEnabled(), true,
          `${scoreMode} must permit the supported worker policy`);
        if (useEveryCpu) await everyCpu.check(); else await everyCpu.uncheck();
        const expectedWorkers = useEveryCpu ? logicalProcessors : logicalProcessors - 1;
        await page.waitForFunction(expected =>
          document.querySelector('.worker-authority')?.getAttribute('data-workers-effective') === String(expected),
        expectedWorkers);
        assert.equal(await workerAuthority.getAttribute('data-workers-requested'), String(expectedWorkers));
      }
      await resultMode.selectOption('off');
      assert.equal(await solutionProbabilities.isEnabled(), true);
      assert.equal(await solutionProbabilities.isChecked(), true,
        'returning to all solutions must restore the retained supported choice');
      await solutionProbabilities.uncheck();
      await everyCpu.uncheck();
      await everyCpu.check();
      for (let repeat = 0; repeat < 2; repeat++) {
        await completeRun(page, () => page.getByRole('button', { name: 'Run search', exact: true }).click(),
          `${mode}: complete P7 bag repeat ${repeat}`);
        const completeBag = await completedKeys(page, 22, `${mode}: complete P7 bag repeat ${repeat}`);
        if (completeBagReference) assert.deepEqual(completeBag, completeBagReference,
          'CPU worker reuse and GPU discovery states must preserve canonical solution order');
        else completeBagReference = completeBag;
      }
      await everyCpu.uncheck();
      await completeRun(page, () => page.getByRole('button', { name: 'Run search', exact: true }).click(),
        `${mode}: complete P7 bag with the default CPU policy`);
      assert.deepEqual(await completedKeys(page, 22, `${mode}: complete P7 bag default policy`),
        completeBagReference, 'the reserved-processor policy must preserve the same exact family');
      results.push({ case: 'complete P7 bag CPU reuse', mode, result: 'passed',
        solutionCount: 22, logicalProcessors, repetitions: 3 });

      await page.locator('.fumen-import input:not([type="file"])').fill('ctk3_w0kCQBhwwAEHHABh4Q');
      await page.locator('.fumen-import button').last().click();
      await page.locator('.workspace-queue-input').fill('STOILJZ');
      await completeRun(page, () => page.getByRole('button', { name: 'Run search', exact: true }).click(), `${mode}: PC`);
      const actual = (await completedKeys(page, 18, `${mode}: PC`)).sort();
      assert.equal(new Set(actual).size, 18);
      if (reference) assert.deepEqual(actual, reference); else reference = actual;
      assert.equal(await page.locator('.mandatory-choice input').count(), 18);
      // A new run through the same controller must also finish (no leaked lease).
      await completeRun(page, () => page.getByRole('button', { name: 'Run search', exact: true }).click(), `${mode}: repeated PC`);
      assert.deepEqual((await completedKeys(page, 18, `${mode}: repeated PC`)).sort(), reference);
      if (mode === 'null') {
        const selected = [(await keys(page))[0], (await keys(page))[17]];
        await page.locator('.mandatory-choice input').first().check();
        await page.locator('.mandatory-choice input').last().check();
        assert.equal(await page.locator('.solution-toolbar .mandatory-summary').count(), 1);
        assert.equal(await page.locator('.mandatory-choice').first().innerText(), 'Solution 1');
        await completeRun(page, () => page.locator('.mandatory-summary button').first().click(), 'pinned PC');
        assert.deepEqual((await completedKeys(page, 2, 'pinned PC')).sort(), selected.sort(), 'mandatory solutions were not retained');
        await page.locator('.workspace-queue-input').fill('I');
        await page.waitForFunction(() => !document.querySelector('.mandatory-summary'));
        // Navigate using the visible menu, then execute a known two-stage case.
        const recovery = await navigateWorkspace(page, 'recovery');
        await recovery.locator('.recovery-field-editor').waitFor();
        await page.locator('.dimension-field input').fill('4');
        await page.evaluate(() => new Promise(requestAnimationFrame));
        assert.deepEqual(errors, [], 'recovery height editing must not throw before search');
        assert.equal(await recovery.locator('.recovery-field-editor .board button').count(), 40);
        const supplies = recovery.locator('.recovery-supplies .workspace-queue-input');
        assert.equal(await supplies.count(), 2);
        await supplies.nth(0).fill('I'); await supplies.nth(1).fill('O');
        assert.equal(await recovery.locator('.placement-constraints,.recovery-required-pieces,.board-stats').count(), 0);
        assert.equal(await recovery.locator('input[type="number"]').count(), 1);
        const early = recovery.getByRole('combobox', { name: 'Maximum early placements', exact: true });
        assert.equal(await early.inputValue(), 'auto');
        await recovery.getByRole('checkbox', { name: 'Hold', exact: true }).uncheck();
        const fields = recovery.locator('.field-palette button');
        await fields.filter({ hasText: /^Start$/ }).click();
        await paint(page, 0, 0x3f0n, 4);
        await fields.filter({ hasText: /^Middle$/ }).click();
        await paint(page, 0, 0xfn, 4);
        await fields.filter({ hasText: /^Result$/ }).click();
        assert.equal(await recovery.locator('.recovery-result-frame').count(), 0, 'editor coordinates are fixed');
        // Start + Middle completes row 0. The same physical O must therefore
        // be drawn one row higher in the shared frame, not over that full row.
        // These are independent fixture coordinates, not output from the
        // production projection helper under test.
        const sharedResult = 0x300c000n;
        const afterMiddleResult = 0xc030n;
        await paint(page, 0, sharedResult, 4);
        assert.equal(await readPaintedMask(page, 0, 4), sharedResult);
        assert.equal(await recovery.getByRole('button', { name: 'Run search', exact: true }).isEnabled(), true,
          'the shared-frame normal fixture must be accepted before execution');
        const runRecovery = label => completeRun(page,
          () => page.getByRole('button', { name: 'Run search', exact: true }).click(), label);
        const paths = recovery.locator('.recovery-path-gallery');
        await runRecovery('paired Build normal, with an actual line clear');
        await paths.locator('[data-recovery-path="normal"]').waitFor();
        assert.equal(await paths.locator('.pc-path-replay-gif img').count(), 1);
        assert.equal(await paths.locator('details,code,.frame-count').count(), 0);
        assert.equal(await recovery.locator('.invalid-evidence,.invalid-replay').count(), 0);
        assert.equal(await recovery.locator('.solution-toolbar .copy-format').count(), 1);
        assert.deepEqual(await recovery.locator('.recovery-metrics strong').allTextContents(), ['100%', '0%', '0%']);
        // Empty base, middle I and result O: supplies O then I need different-
        // kind repayment. This exercises the actual new parser, solver and wire.
        await fields.filter({ hasText: /^Start$/ }).click();
        await paint(page, 0, 0x3f0n, 4);
        await fields.filter({ hasText: /^Result$/ }).click();
        await paint(page, 0, sharedResult, 4);
        await paint(page, 0, afterMiddleResult, 4);
        await supplies.nth(0).fill('O'); await supplies.nth(1).fill('I');
        const exchange = recovery.getByRole('checkbox', { name: 'Allow different-piece repayment', exact: true });
        assert.equal(await exchange.isChecked(), false);
        await runRecovery('different-piece repayment disabled');
        assert.equal(await recovery.locator('.recovery-path-gallery>li').count(), 0);
        assert.equal(await recovery.locator('.invalid-evidence').count(), 0);
        await exchange.check();
        await runRecovery('different-piece repayment enabled');
        await paths.waitFor({ state: 'visible' });
        const recoveryCards = paths.locator(':scope > li');
        const recoveryRecords = await recoveryCards.evaluateAll(nodes => nodes.map(node => ({
          key: node.dataset.solutionKey, status: node.dataset.recoveryPath
        })));
        // Empty Start admits both the middle I and its mirror. They cover the
        // same queue; neither the complete geometry set nor the probability
        // may be replaced by an arbitrary representative card.
        assertRecoveryGallery(recoveryRecords, 'recovery', [
          { middleMask: 0xfn, resultMask: afterMiddleResult },
          { middleMask: 0x3c0n, resultMask: afterMiddleResult }
        ]);
        for (const card of await recoveryCards.all()) {
          const image = card.locator('.pc-path-replay-gif img');
          await image.waitFor({ state: 'visible' });
          await image.evaluate(node => node.decode());
        }
        assert.equal(await paths.locator('.pc-path-replay-gif img').count(), recoveryRecords.length);
        assert.equal(await paths.locator('details,code,.frame-count').count(), 0);
        assert.equal(await recovery.locator('.invalid-evidence,.invalid-replay').count(), 0);
        assert.deepEqual(await recovery.locator('.recovery-metrics strong').allTextContents(), ['0%', '100%', '0%']);
        await early.selectOption('0');
        await runRecovery('zero early placements disables the same exchange route');
        assert.equal(await recovery.locator('.recovery-path-gallery>li').count(), 0);
        assert.equal(await recovery.locator('.invalid-evidence').count(), 0);
        console.log('surface_recovery_build=passed normal=true shared_frame=true after_middle_frame=true exchange=true quota=true');

        const build = await navigateWorkspace(page, 'build-probability');
        await page.locator('.recovery-field-editor').waitFor({ state: 'detached' });
        const buildQueue = build.locator('.workspace-controls .workspace-queue-input');
        await buildQueue.waitFor();
        assert.equal(await buildQueue.count(), 1, 'the mounted Build workspace owns one queue input');
        await build.locator('.dimension-field input').fill('4');
        await buildQueue.fill('I');
        const hold = build.getByRole('checkbox', { name: 'Hold', exact: true });
        await hold.uncheck();
        assert.equal(await hold.isChecked(), false, 'build request must disable hold through the real control');
        await paint(page, 0, 0xfn, 4);
        await completeRun(page, () => page.getByRole('button', { name: 'Run search', exact: true }).click(), 'mirrored Build source');
        // A symmetric empty base includes both the drawn target and its mirror.
        // These are two tilings covering ONE queue, not two probability events.
        const buildKeys = (await completedKeys(page, 2, 'mirrored Build source')).sort();
        assert.deepEqual(buildKeys, [LEFT_I, RIGHT_I].sort());
        // Select the mirrored drawing, not just the first canonical solution.
        await page.locator(`.solution-gallery > li[data-solution-key="${RIGHT_I}"] .mandatory-choice input`).check();
        await completeRun(page, () => page.locator('.mandatory-summary button').first().click(), 'mirrored mandatory Build');
        assert.deepEqual(await completedKeys(page, 1, 'mirrored mandatory Build'), [RIGHT_I]);
        // Request both pins from the complete source: ordinary minimum size is
        // one, but mandatory inclusion must retain both selected drawings.
        await page.getByRole('combobox', { name: 'Result aggregation', exact: true }).selectOption('all-solutions');
        await completeRun(page, () => page.getByRole('button', { name: 'Run search', exact: true }).click(), 'Build source replay');
        assert.deepEqual((await completedKeys(page, 2, 'Build source replay')).sort(), buildKeys);
        await page.locator(`.solution-gallery > li[data-solution-key="${LEFT_I}"] .mandatory-choice input`).check();
        await completeRun(page, () => page.locator('.mandatory-summary button').first().click(), 'two mandatory Build drawings');
        assert.deepEqual((await completedKeys(page, 2, 'two mandatory Build drawings')).sort(), buildKeys);
        // Changing the target invalidates the old pins. A centered I equals its
        // mirror and must be deduplicated to exactly one source solution.
        await page.getByRole('combobox', { name: 'Result aggregation', exact: true }).selectOption('all-solutions');
        await paint(page, 0, 0xfn ^ 0x78n, 4);
        await page.waitForFunction(() => !document.querySelector('.mandatory-summary'));
        await completeRun(page, () => page.getByRole('button', { name: 'Run search', exact: true }).click(), 'symmetric Build source');
        assert.deepEqual(await completedKeys(page, 1, 'symmetric Build source'), [CENTER_I]);
        results.push({ case: 'Build mirror and mandatory source', original: LEFT_I, mirrored: RIGHT_I,
          sourceCount: 2, mirroredPinCount: 1, twoPinsCount: 2, symmetricCount: 1 });

      }
      assert.deepEqual(errors, []);
      results.push({ mode, result: 'passed', solutionCount: 18 });
    } catch (error) {
      const failure = error instanceof Error ? error : new Error(String(error));
      await page.screenshot({ path: resolve(reportRoot, `failed-${mode}.png`), fullPage: true })
        .catch(captureError => errors.push(`screenshot capture failed: ${captureError.message}`));
      const text = await page.locator('body').innerText().catch(() => 'unavailable');
      await writeFile(resolve(reportRoot, `failed-${mode}.txt`), `${failure.stack}\n${errors.join('\n')}\n${text}`).catch(() => {});
      const diagnostic = {
        source: process.env.CLEARRA_SOURCE_COMMIT,
        mode, url: diagnosticUrl(page.url()), failure: failure.message,
        pageErrors: errors.slice(0, 16).map(error => error.slice(0, 2048)),
        browserConsole, failedRequests, httpErrors, body: text.slice(0, 16384)
      };
      await writeFile(resolve(reportRoot, `failed-${mode}.json`), JSON.stringify(diagnostic, null, 2) + '\n')
        .catch(captureError => console.error(`surface_diagnostic_write=failed mode=${mode} reason=${captureError.message}`));
      failures.push(failure);
      results.push({ mode, result: 'failed', message: failure.message });
      console.error(`surface_case=${mode} status=failed: ${failure.stack}`);
      console.error(`surface_diagnostic=${JSON.stringify({ ...diagnostic,
        pageErrors: diagnostic.pageErrors.slice(0, 4), browserConsole: browserConsole.slice(0, 4),
        failedRequests: failedRequests.slice(0, 4), httpErrors: httpErrors.slice(0, 4), body: text.slice(0, 4096)
      })}`);
    } finally { await ctx.close(); }
  }
  for (const workers of [1, 8]) for (const hold of [false, true]) {
    await tilingUiCase({ height: 2, hold, workers }, expectedIdentity);
  }
  await tilingUiCase({ height: 6, hold: true, workers: 8 }, expectedIdentity);
  await tilingArtifactCase(expectedIdentity, manifest);
  const report = { source: process.env.CLEARRA_SOURCE_COMMIT, results, failedCases: failures.length };
  await writeFile(resolve(reportRoot, 'results.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify(report));
  // Each GPU mode owns a fresh context. A failed mode must remain a failure,
  // but it need not hide the independent modes that can still execute.
  if (failures.length) throw new AggregateError(failures, 'compiled Pages operational acceptance failed');
} finally {
  await browser.close();
  await new Promise(resolve => server.close(resolve));
}
