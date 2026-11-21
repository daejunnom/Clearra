// Functional Chromium execution of the product Worker. The opt-in 4195
// paired timing uses instrumented WASM, not Pages/UI or release readback.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, realpath } from 'node:fs/promises';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { availableParallelism } from 'node:os';
import { basename, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
import { assertNoBuildLinks, enterManagedBuildOrRelaunch } from './clearra-build-policy.mjs';
import { createClearraWasmBuildContract, clearraWasmBuildContractsEqual }
  from './clearra-wasm-build-contract.mjs';
import { acquireManagedTransientDirectory } from './managed-transient-directory.mjs';

const SELF = fileURLToPath(import.meta.url);
const repository = resolve(fileURLToPath(new URL('../..', import.meta.url)));
const HOST = '127.0.0.1';
const options = process.argv.slice(2);
const pairedProbe = pairedProbeOptions(options);
const PORT = pairedProbe ? 4195 : 4194;
const ORIGIN = 'http://' + HOST + ':' + PORT;
const ROOT_WORKER = '/workers/clearraWorker.ts';
const VERIFIER_WORKER = '/workers/clearraVerifierWorker.ts';
const PLAYWRIGHT_VERSION = '1.56.1';
const CASE_TIMEOUT_MS = 40_000;
const PAIRED_TIMEOUT_MS = 65_000;
const ACCELERATORS_OFF = '--no-legal-board --no-conditioned-reachability';
const PC_CPU = '--objective unique --count unique --queue-knowledge oracle --backend cpu ' +
  '--rule srs-plus --no-hold --no-tablebase ' + ACCELERATORS_OFF;

const runtimeProfile = pairedProbe ? 'benchmark-search' : 'local-service';
assert.ok(process.env.CLEARRA_RUNTIME_SUPERVISED === '1' &&
  process.env.CLEARRA_RUNTIME_PROFILE === runtimeProfile,
  'run the reserved ' + PORT + ' listener under clearra-manage runtime run --profile ' + runtimeProfile);
for (const name of ['CLEARRA_BROWSER_TOOLS_ROOT', 'CLEARRA_REAL_ACCELERATOR_WASM_DIR',
  'CLEARRA_SOURCE_COMMIT', 'CLEARRA_ENGINE_BUILD_ID']) {
  assert.ok(process.env[name], name + ' is required');
}
const source = process.env.CLEARRA_SOURCE_COMMIT;
assert.match(source, /^[0-9a-f]{40}$/u);
assert.equal(process.env.CLEARRA_ENGINE_BUILD_ID, source);

function pairedProbeOptions(args) {
  if (!args.length) return null;
  assert.ok(args.length === 5 && args[0] === '--paired-probe' &&
    args[1] === '--workers' && args[3] === '--command',
  'expected --paired-probe --workers N --command "clearra pc ..."');
  assert.match(args[2], /^[1-8]$/u);
  const workers = Number(args[2]);
  const command = args[4];
  // The paired scheduler owns the exact input. Reject accidental large/other
  // products rather than turning this bounded diagnostic into a generic runner.
  assert.match(command, /^clearra pc(?: [a-zA-Z0-9-]+)+$/u);
  const expected = new Map([
    ['--board-mask', /^0x[0-9a-f]{1,10}$/u], ['--height', '4'], ['--lines', '4'],
    ['--pieces', '6'], ['--patterns', 'P7'], ['--hold', 'empty'],
    ['--queue-knowledge', 'oracle'], ['--count', 'unique'], ['--backend', 'cpu'],
    ['--rule', 'srs-plus'], ['--workers', String(workers)], ['--no-build-dependency-dag', null],
    ['--no-tablebase', null], ['--no-legal-board', null], ['--no-conditioned-reachability', null]
  ]);
  const tokens = command.split(' ').slice(2);
  for (let index = 0; index < tokens.length; index++) {
    const option = tokens[index];
    assert.ok(expected.has(option), 'unexpected or duplicate paired option: ' + option);
    const constraint = expected.get(option);
    expected.delete(option);
    if (constraint instanceof RegExp) assert.match(tokens[++index] ?? '', constraint, option);
    else if (constraint !== null) assert.equal(tokens[++index], constraint, option);
  }
  assert.equal(expected.size, 0, 'missing required paired options');
  return { command, workers };
}

const wasmRoot = join(await realpath(repository), '_local/artifacts/v081-browser-peer-smoke/wasm');
assertNoBuildLinks(wasmRoot);
assertNoBuildLinks(resolve(process.env.CLEARRA_REAL_ACCELERATOR_WASM_DIR));
assert.equal(await realpath(process.env.CLEARRA_REAL_ACCELERATOR_WASM_DIR), wasmRoot);
const manifestFile = 'clearra_wasm.manifest.json';
const manifestPath = join(wasmRoot, manifestFile);
assertNoBuildLinks(manifestPath);
const manifestBytes = await readFile(manifestPath);
const manifest = JSON.parse(manifestBytes);
assert.equal(manifest.schema_version, 1);
assert.ok(clearraWasmBuildContractsEqual(manifest.build, await createClearraWasmBuildContract(repository)),
  'the prebuilt WASM must match the complete current-source build contract');
const files = new Map([
  ['/', { type: 'text/html; charset=utf-8',
    bytes: '<!doctype html><meta charset="utf-8"><link rel="icon" href="data:,"><title>Clearra CPU Worker smoke</title>' }],
  ['/wasm/' + manifestFile, { bytes: manifestBytes, type: 'application/json' }]
]);
for (const [entry, namePattern, type] of [
  [manifest.bindings, /^clearra_wasm\.[a-f0-9]{24}\.js$/u, 'text/javascript'],
  [manifest.wasm, /^clearra_wasm_bg\.[a-f0-9]{24}\.wasm$/u, 'application/wasm']
]) {
  assert.match(entry.path, namePattern);
  const path = join(wasmRoot, entry.path);
  assertNoBuildLinks(path);
  const bytes = await readFile(path);
  assert.equal(bytes.length, entry.bytes);
  assert.equal(createHash('sha256').update(bytes).digest('hex'), entry.sha256);
  // The browser sees exactly the bytes that were checked, never a later file.
  files.set('/wasm/' + entry.path, { bytes, type });
}

const fixtureFile = join(repository, 'tests/fixtures/contracts/extended_pc_surface_input.v1.tsv');
assertNoBuildLinks(fixtureFile);
const fixture = (await readFile(fixtureFile, 'utf8')).split(/\r?\n/u)
  .filter(row => !row.startsWith('#') && row.length > 0).map(row => row.split('\t'));
function forcedFixture(rows, targetLines, exactPieces) {
  const matches = rows.filter(row => row[0] === 'forced-' + targetLines + 'L');
  assert.equal(matches.length, 1);
  const [, height, mask, pieces, queue] = matches[0];
  assert.equal(height, String(targetLines));
  assert.match(mask, /^[a-f0-9]{64}$/u);
  assert.equal(pieces, String(exactPieces));
  assert.equal(queue, 'I'.repeat(exactPieces));
  return { height, mask, pieces, queue };
}
const forced24 = forcedFixture(fixture, 24, 6);
const forced7 = forcedFixture(fixture, 7, 2);
const { height, mask, pieces, queue } = forced24;
function pcProductCommand(fixture, product, preserveB2b = false) {
  return 'clearra pc' + (product ? ' ' + product : '') +
    ' --lines ' + fixture.height + ' --height ' + fixture.height +
    ' --board-mask 0x' + fixture.mask + ' --pieces ' + fixture.pieces + ' --queue ' + fixture.queue +
    ' --no-hold --rule srs-plus --backend cpu --no-tablebase ' + ACCELERATORS_OFF +
    (preserveB2b ? ' --preserve-b2b --spin-profile t-spins' : '');
}
const tests = [
  { label: 'pc-one-i', workers: 2, kind: 'pc-scenario', keyPrefix: 'ctk1|',
    command: 'clearra pc --lines 1 --height 1 --board-mask 0x3f0 --pieces 1 --queue I ' + PC_CPU,
    keys: ['ctk1|initial=00000000000003f0|placements=I:000000000000000f'] },
  { label: 'pc-forced-24l', workers: 3, kind: 'pc-scenario',
    keyPrefix: 'ctk2|height=24|initial=' + mask + '|placements=',
    command: 'clearra pc --lines ' + height + ' --height ' + height + ' --board-mask 0x' + mask +
      ' --pieces ' + pieces + ' --queue ' + queue + ' ' + PC_CPU, count: 1 },
  { label: 'build-mirrored-i', workers: 2, kind: 'build-probability', keyPrefix: 'ctk1|',
    command: 'clearra build-probability --base-mask 0 --target-mask 0xf --height 4 --queue I --no-hold ' +
      '--include-mirror --aggregate buildability --result-mode all-solutions --solution-probabilities ' +
      '--backend cpu --rule srs-plus ' + ACCELERATORS_OFF,
    keys: ['ctk1|initial=0000000000000000|placements=I:000000000000000f',
      'ctk1|initial=0000000000000000|placements=I:00000000000003c0'] },
  { label: 'pc-forced-24l-minimum', workers: 3, kind: 'pc-minimum-cover.v2',
    page: 'minimum', initialMask: mask, targetLines: Number(height),
    command: pcProductCommand(forced24, 'minimals') },
  { label: 'pc-forced-24l-replay', workers: 3, kind: 'pc-path-family.v2',
    page: 'replay', initialMask: mask, targetLines: Number(height), pieces: Number(pieces),
    command: pcProductCommand(forced24, 'path') },
  // Empty physical evidence is not a verifier-activity witness. Use serial
  // readback for the two valid-but-empty products instead of requiring peers.
  { label: 'pc-forced-7l-b2b-empty-minimum', workers: 1, kind: 'pc-minimum-cover.v2',
    page: 'minimum', empty: true, command: pcProductCommand(forced7, 'minimals', true) },
  { label: 'pc-forced-7l-b2b-empty-replay', workers: 1, kind: 'pc-path-family.v2',
    page: 'replay', empty: true, command: pcProductCommand(forced7, 'path', true) }
];

const toolsRoot = await realpath(process.env.CLEARRA_BROWSER_TOOLS_ROOT);
const require = createRequire(join(toolsRoot, 'package.json'));
assert.equal(require('playwright/package.json').version, PLAYWRIGHT_VERSION);
const { chromium } = require('playwright');
const owner = enterManagedBuildOrRelaunch(repository, [SELF, ...options], 'experiment');
assert.equal(owner.purpose, 'experiment', 'the browser smoke must not write into a product transaction');
const scripts = await build({
  absWorkingDir: repository, bundle: true, write: false, format: 'esm', platform: 'browser',
  target: 'es2022', conditions: ['browser'], mainFields: ['browser', 'module', 'main'],
  define: { 'import.meta.env.MODE': JSON.stringify(pairedProbe ? 'local-audit' : 'production') },
  outdir: join(owner.transaction, 'chromium-cpu-worker-in-memory'),
  entryPoints: {
    clearraWorker: 'apps/clearra-web/src/workers/clearraWorker.ts',
    clearraVerifierWorker: 'apps/clearra-web/src/workers/clearraVerifierWorker.ts',
    capabilities: 'packages/clearra-ui/src/lib/wasm/hostCapabilitySnapshot.ts',
    workerClient: 'packages/clearra-ui/src/lib/wasm/wasmCommandClient.ts',
    wasmRuntime: 'apps/clearra-web/src/workers/clearraWasmRuntime.ts',
    productPager: 'packages/clearra-ui/src/lib/workspace/productResultPager.ts',
    replayPager: 'packages/clearra-ui/src/lib/workspace/pcReplayPager.ts',
    replayPresentation: 'packages/clearra-ui/src/lib/workspace/pcPathReplayPresentation.ts',
    ...(pairedProbe ? { localSearchProfile: 'apps/clearra-web/src/lib/localSearchProfile.ts' } : {})
  }
});
const compiled = new Map(scripts.outputFiles.map(file => [basename(file.path), file.contents]));
assert.equal(compiled.size, pairedProbe ? 9 : 8);
for (const [url, entry] of [[ROOT_WORKER, 'clearraWorker.js'],
  [VERIFIER_WORKER, 'clearraVerifierWorker.js'], ['/capabilities.js', 'capabilities.js'],
  ['/worker-client.js', 'workerClient.js'], ['/wasm-runtime.js', 'wasmRuntime.js'],
  ['/product-pager.js', 'productPager.js'],
  ['/replay-pager.js', 'replayPager.js'], ['/replay-presentation.js', 'replayPresentation.js'],
  ...(pairedProbe ? [['/local-search-profile.js', 'localSearchProfile.js']] : [])]) {
  const bytes = compiled.get(entry);
  assert.ok(bytes, 'missing compiled entry ' + entry);
  files.set(url, { bytes, type: 'text/javascript; charset=utf-8' });
}

const unexpected = new Set();
let verifierLoads = 0;
const server = createServer((request, response) => {
  const url = new URL(request.url, ORIGIN);
  const file = request.method === 'GET' && url.origin === ORIGIN && files.get(url.pathname);
  if (!file) {
    unexpected.add(request.method + ' ' + url.origin + url.pathname);
    response.writeHead(404).end();
    return;
  }
  if (url.pathname === VERIFIER_WORKER) verifierLoads += 1;
  response.writeHead(200, { 'Content-Type': file.type, 'Cache-Control': 'no-store' }).end(file.bytes);
});
server.on('connect', (request, socket) => {
  // The browser's proxy can only reject non-loopback requests, never relay them.
  unexpected.add('CONNECT ' + request.url);
  socket.end('HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n');
});

const scratch = await acquireManagedTransientDirectory(join(owner.transaction, 'transient', 'chromium-cpu-worker'));
let context;
let launchStarted = false;
try {
  // Let the OS reject an occupied port; do not adopt/stop its existing owner.
  await new Promise((ready, reject) => {
    server.once('error', reject);
    server.listen({ host: HOST, port: PORT, exclusive: true }, ready);
  });
  launchStarted = true;
  context = await chromium.launchPersistentContext(join(scratch.path, 'profile'), {
    headless: true, timeout: 30_000, serviceWorkers: 'block',
    downloadsPath: join(scratch.path, 'downloads'), tracesDir: join(scratch.path, 'traces'),
    proxy: { server: ORIGIN, bypass: HOST },
    args: ['--disable-background-networking']
  });
  const page = context.pages()[0] ?? await context.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const response = await page.goto(ORIGIN, { timeout: 15_000 });
  assert.equal(response?.status(), 200);
  if (pairedProbe) await readbackPairedProbe(page, pairedProbe);
  else await readbackChromiumSmoke(page);
  assert.deepEqual(errors, []);
  assert.deepEqual([...unexpected], [], 'the browser requested files outside the exact asset-free allowlist');
  if (!pairedProbe) {
    console.log('chromium_cpu_worker_smoke=passed source=' + source + ' wasm_sha256=' + manifest.wasm.sha256 +
      ' verifier_loads=' + verifierLoads + ' release_authority=false');
  }
} finally {
  let browserDrained = !launchStarted;
  try {
    if (context) {
      await context.close();
      browserDrained = true;
    }
  } finally {
    try {
      if (server.listening) {
        server.closeAllConnections();
        await new Promise((done, reject) => server.close(error => error ? reject(error) : done()));
      }
    } finally {
      // Incomplete launch/close leaves an unresolved lease for reviewed
      // recovery. The outer supervisor still owns the entire process tree.
      if (browserDrained) await scratch.release();
    }
  }
}

async function readbackChromiumSmoke(page) {
  const available = await page.evaluate(() => navigator.hardwareConcurrency);
  assert.ok(Number.isSafeInteger(available) && available >= 3, 'the Chromium smoke needs three real logical processors');
  await readbackLocalSr3(page);
  let forced24GeometryKey;
  for (const test of tests) {
    const runs = [];
    let workersUsed = 1;
    for (const workers of test.workers > 1 ? [1, test.workers] : [1]) {
      const before = verifierLoads;
      const execution = await page.evaluate(runWorker, {
        rootWorker: ROOT_WORKER, command: test.command + ' --workers ' + workers, workers,
        timeoutMs: CASE_TIMEOUT_MS, label: test.label, pageKind: test.page ?? null,
        expectedKind: test.kind, empty: test.empty ?? false, targetLines: test.targetLines ?? null,
      });
      assert.equal(execution.terminal.event, 'final_response', test.label);
      assert.equal(execution.terminal.response.status, 'success', JSON.stringify(execution.terminal.response.diagnostics));
      assert.equal(execution.terminal.response.result?.kind, test.kind, test.label);
      assert.deepEqual(execution.terminal.response.runtime_identity, manifest.build.runtime_identity);
      const report = execution.terminal.search_report;
      assert.equal(report.cpu_parallel_execution, workers > 1, test.label);
      if (workers > 1) {
        // A one-candidate family may retire a peer that did not become ready
        // before the producer drained. Require real peer work, not idle slots.
        workersUsed = report.workers_used;
        assert.ok(Number.isSafeInteger(workersUsed) && workersUsed >= 2 &&
          workersUsed <= workers, test.label + ' did not complete on a valid worker subset');
        assert.ok(verifierLoads > before, test.label + ' did not load a real Chromium verifier');
        assert.ok(execution.verified > 0, test.label + ' did not verify a candidate in a peer');
      } else {
        assert.equal(report.workers_used, 1, test.label);
        assert.equal(verifierLoads, before, test.label + ' unexpectedly created a verifier');
      }
      runs.push(test.page
        ? typedProjection(test, execution, forced24GeometryKey)
        : exactProjection(test, report));
    }
    if (runs.length > 1) {
      assert.deepEqual(runs[1], runs[0], test.label + ' serial/distributed result mismatch');
    }
    if (test.label === 'pc-forced-24l') {
      forced24GeometryKey = runs[0].keys[0];
      const placements = forced24GeometryKey.split('|placements=')[1].split(',').sort();
      assert.deepEqual(placements, forced24OriginalPlacements(),
        'the ordinary family must cover the six independently known I wells');
    }
    console.log('chromium_cpu_worker_case=passed case=' + test.label + ' workers_requested=' + test.workers +
      ' workers_used=' + workersUsed + (test.page
        ? ' product_page=' + test.page + ' page_read=' + (runs[0].page !== null) +
          ' page_unavailable_after_release=true'
        : ' solutions=' + runs[0].keys.length + ' covered=' + runs[0].covered));
  }
}

async function readbackPairedProbe(page, { workers, command }) {
  const browser = await page.evaluate(() => ({
    logical_processors: navigator.hardwareConcurrency,
    cross_origin_isolated: self.crossOriginIsolated,
  }));
  const runnerProcessors = availableParallelism();
  const availableWorkers = Math.max(1, Math.min(runnerProcessors, browser.logical_processors) - 1);
  assert.ok(workers <= availableWorkers, 'paired worker count exceeds this browser host capacity');
  const label = 'existing-field-4l-p7';
  console.log('chromium_cpu_paired_config=' + JSON.stringify({
    source, wasm_sha256: manifest.wasm.sha256, runner_logical_processors: runnerProcessors,
    browser, workers_requested: workers, command, timeout_ms: PAIRED_TIMEOUT_MS,
    core_stage_scope: workers === 1 ? 'single-root-worker' : 'root-coordinator-only-no-peer-rust-stages',
    timing_scope: 'fresh Worker post-to-terminal; excludes Chromium launch and JS bundle',
    memory_scope: 'solver-reported peak and terminal-root WASM linear capacity; not idle-process RSS',
    idle_rss_measured: false, autonomous_peer_drain_measured: false,
    pages_readback: false, release_authority: false,
  }));
  const before = verifierLoads;
  const execution = await page.evaluate(runWorker, {
    rootWorker: ROOT_WORKER, command, workers, timeoutMs: PAIRED_TIMEOUT_MS, label, benchmarkProbe: true,
  });
  console.log('chromium_cpu_paired_sample=' + JSON.stringify({
    label, browser, wasm_sha256: manifest.wasm.sha256, verifier_loads: verifierLoads - before, ...execution,
  }));
  assert.equal(execution.event, 'final_response', label);
  assert.equal(execution.status, 'success', label);
  assert.deepEqual(execution.runtime_identity, manifest.build.runtime_identity);
  assert.equal(execution.search?.backend_selected, 'wasm-cpu', label);
  if (workers === 1) {
    assert.equal(execution.search_profile?.schema_version, 1, 'missing instrumented WASM search profile');
    assert.ok(execution.search_profile.stages.some(stage => stage.invocation_count > 0),
      'no observed one-worker WASM search stages');
  }
}

async function readbackLocalSr3(page) {
  const label = 'pc-forced-7l-visible-seven-sr3';
  const before = verifierLoads;
  const run = command => page.evaluate(runWorker, {
    rootWorker: ROOT_WORKER, command, workers: 1, timeoutMs: CASE_TIMEOUT_MS, label
  });
  const { terminal: seed } = await run(pcProductCommand(forced7, '') +
    ' --objective all --count all --queue-knowledge visible-7 --workers 1');
  assert.equal(seed.event, 'final_response', label);
  assert.equal(seed.response.status, 'success', label);
  assert.equal(seed.response.result?.kind, 'pc-scenario', label);
  assert.deepEqual(seed.response.runtime_identity, manifest.build.runtime_identity);
  assert.equal(seed.search_report.workers_used, 1, label);
  exactProjection({
    label, count: 1, keyPrefix: 'ctk2|height=7|initial=' + forced7.mask + '|placements='
  }, seed.search_report);
  const fields = new Map(seed.search_report.summary_fields);
  assert.equal(fields.get('queue_knowledge'), 'visible-7', label);
  assert.equal(fields.get('observation_policy_evaluated'), 'true', label);
  assert.equal(fields.get('scenario_replay_token_version'), 'sr3', label);
  const token = fields.get('scenario_replay_token');
  assert.match(token, /^sr3:w10:v7:[^\s]+$/u, label);
  assert.notEqual(fields.get('continuation_token'), token, 'sr3 is an input replay, not a next-state token');

  // Continue's public parser only accepts the token. Worker authority is
  // carried by the host message, not by appending PC options to Continue.
  const command = 'clearra continue --token ' + token;
  const { terminal: replay } = await run(command);
  assertContinueTerminal(replay, label);
  assert.equal(verifierLoads, before, 'local sr3 must not start a distributed verifier');

  // The browser product worker proves real ingress/completion, not how many
  // times the ABI advanced. A second view over the same WASM exercises unit
  // work budgets and interleaved cancellation without racing browser timers.
  const stepped = await page.evaluate(stepLocalSr3, { command, timeoutMs: CASE_TIMEOUT_MS });
  assertContinueTerminal(stepped.completed, label);
  assert.equal(stepped.cancelled.event, 'cancelled', label);
  assert.equal(stepped.cancelled.scope_released, true, label);
  assert.equal(stepped.cancelled.execution_availability.reason, 'cancelled-by-caller', label);
  assert.equal(stepped.cancelled.result_completeness, 'incomplete', label);
  assert.notEqual(stepped.completed.job_id, stepped.cancelled.job_id, label);
  console.log('chromium_local_sr3=passed height=7 visible_seven=true worker_terminal=continue ' +
    'cooperative_slices=' + stepped.slices + ' cancelled_scope_released=true');
}

function assertContinueTerminal(terminal, label) {
  assert.equal(terminal.event, 'final_response', label);
  assert.equal(terminal.response.status, 'success', label);
  assert.equal(terminal.response.command, 'continue', label);
  assert.equal(terminal.response.result?.kind, 'continue', label);
  assert.deepEqual(terminal.response.runtime_identity, manifest.build.runtime_identity);
  // Continue is an AppMessage, not a Core-backed search result or product
  // page. The native App readback owns its summary fields and result parity.
  assert.equal(terminal.search_report ?? null, null, label);
}

// In Chromium, step the existing serial job ABI exactly as the product worker
// does, but with budget one so an Immediate Continue cannot pass this check.
async function stepLocalSr3({ command, timeoutMs }) {
  const deadline = performance.now() + timeoutMs;
  const { loadClearraWasmModule } = await import('/wasm-runtime.js');
  const wasm = await loadClearraWasmModule();
  let owned = null;
  const live = status => status === 'pending' || status === 'progress';
  const terminalEvent = event => ['final_response', 'failed', 'cancelled', 'terminated'].includes(event.event);
  function drain(job) {
    const events = JSON.parse(wasm.drain_job_events_json(job));
    if (events.some(event => event.job_id !== job)) throw new Error('sr3 event escaped its job scope');
    return events;
  }
  function advance(job) {
    if (performance.now() >= deadline) throw new Error('sr3 unit-work stepping timed out');
    const status = wasm.advance_job(job, 1);
    if (!live(status)) owned = null;
    return { status, events: drain(job) };
  }
  function begin() {
    const job = wasm.start_job(command);
    owned = job;
    // Common parsing preparation is Pending even for the old Immediate path.
    const preparation = advance(job);
    if (preparation.status !== 'pending' || preparation.events.some(terminalEvent)) {
      throw new Error('sr3 did not prepare a live job: ' + preparation.status);
    }
    const firstSearch = advance(job);
    // The event kind is stable; the Core's stage labels are not a public
    // discriminator and are deliberately not asserted here.
    if (!live(firstSearch.status) || firstSearch.events.some(terminalEvent) ||
        !firstSearch.events.some(event => event.event === 'progress')) {
      throw new Error('sr3 did not yield a live search slice after preparation');
    }
    return job;
  }
  try {
    const cancelledJob = begin();
    wasm.cancel_job(cancelledJob);
    owned = null;
    const cancellation = drain(cancelledJob).filter(terminalEvent);
    if (cancellation.length !== 1 || cancellation[0].event !== 'cancelled') {
      throw new Error('sr3 cancellation did not publish one cancelled terminal');
    }
    const completedJob = begin();
    for (let slices = 2; slices <= 4096; slices++) {
      const { status, events } = advance(completedJob);
      if (live(status)) {
        if (events.some(terminalEvent)) throw new Error('live sr3 job published a terminal event');
        continue;
      }
      const completions = events.filter(terminalEvent);
      if (status !== 'completed' || completions.length !== 1 || completions[0].event !== 'final_response') {
        throw new Error('replacement sr3 job did not complete: ' + status);
      }
      return { completed: completions[0], cancelled: cancellation[0], slices };
    }
    throw new Error('sr3 did not complete after 4096 unit-work advances');
  } finally {
    if (owned !== null) {
      // Preserve the original error; exiting the browser also ends the owner.
      try { wasm.cancel_job(owned); } catch { /* already released */ }
    }
  }
}

function exactProjection(test, report) {
  for (const flag of ['solution_count_calculated', 'solution_set_materialized', 'solution_keys_complete',
    'coverage_calculated', 'probability_calculated', 'count_complete', 'probability_complete']) {
    assert.equal(report[flag], true, test.label + '/' + flag);
  }
  assert.equal(report.resource_truncated, false, test.label);
  assert.equal(report.total_possible_pattern_count, '1', test.label);
  assert.equal(report.covered_pattern_count, 1, test.label);
  assert.equal(report.coverage_probability, '1', test.label);
  const keys = report.normalized_solution_keys;
  assert.equal(report.unique_solution_count, test.keys?.length ?? test.count, test.label);
  assert.equal(keys.length, report.unique_solution_count, test.label);
  assert.ok(keys.every(key => key.startsWith(test.keyPrefix)), test.label);
  assert.equal(new Set(keys).size, keys.length, test.label);
  if (test.keys) assert.deepEqual([...keys].sort(), [...test.keys].sort(), test.label);
  return { keys, hash: report.normalized_solution_set_hash, covered: report.covered_pattern_count,
    total: report.total_possible_pattern_count, probability: report.coverage_probability };
}

function placedI(mask) {
  return 'I:' + mask.toString(16).padStart(64, '0');
}

function forced24OriginalPlacements() {
  // In the tracked fixture each group of four rows has one hole per row
  // in its own column. Expected masks do not depend on any solver result.
  return Array.from({ length: 6 }, (_, column) =>
    placedI([0, 1, 2, 3].reduce((mask, dy) => mask |
      (1n << BigInt((4 * column + dy) * 10 + column)), 0n))).sort();
}

function replayOriginalPlacements(steps, height) {
  // Replay masks follow compacted physical coordinates. Track survivors to
  // compare each lock to the independent original-row geometry oracle.
  let liveRows = Array.from({ length: height }, (_, i) => i);
  const originalMasks = [];
  for (const step of steps) {
    const placed = BigInt(step.placement_mask);
    let original = 0n;
    for (let cell = 0; cell < height * 10; cell++) {
      if ((placed & (1n << BigInt(cell))) === 0n) continue;
      const row = liveRows[Math.floor(cell / 10)];
      assert.notEqual(row, undefined, 'the fixture locks only within the original frame');
      original |= 1n << BigInt(row * 10 + cell % 10);
    }
    originalMasks.push(placedI(original));
    const cleared = BigInt(step.cleared_row_mask);
    liveRows = liveRows.filter((_, physicalRow) => (cleared & (1n << BigInt(physicalRow))) === 0n);
  }
  assert.deepEqual(liveRows, [], 'the full-height fixture eliminates all original rows');
  return originalMasks.sort();
}

function typedProjection(test, execution, forced24GeometryKey) {
  const { terminal, productPage: page, frames, pageUnavailableAfterRelease } = execution;
  const report = terminal.search_report;
  assert.equal(report.count_complete, true, test.label);
  assert.equal(report.resource_truncated, false, test.label);
  assert.equal(pageUnavailableAfterRelease, true, test.label + ' retained a released product page');
  const product = terminal.response.product_result_payload;
  assert.ok(product, test.label + ' must present actual product-owned evidence');
  assert.equal(product.result_kind, test.kind);
  const payload = product.content.payload;
  if (test.page === 'minimum') {
    assert.equal(product.contract, 'pc.minimals');
    assert.equal(product.content.payload_kind, 'coverage-portfolio');
    assert.equal(payload.optimal_cardinality, test.empty ? '0' : '1');
    assert.equal(payload.canonical_selection, 'smallest-canonical-candidate-id');
    assert.equal(payload.alternative_index, '1');
    assert.equal(payload.member_page_number, '1');
    assert.equal(payload.total_member_pages, '1');
    assert.match(payload.set_identity_sha256, /^[a-f0-9]{64}$/u);
    assert.match(payload.candidate_map_sha256, /^[a-f0-9]{64}$/u);
    // Cardinality zero is still a valid mathematical portfolio with one
    // (empty) alternative and member page, unlike an empty replay family.
    assert.equal(payload.page_handle_available, true);
    if (test.empty) {
      assert.deepEqual(payload.members, []);
      assert.equal(payload.canonical_witness ?? null, null);
    } else {
      // The solver owns candidate IDs, not the worker's JSON envelope. Use the
      // ordinary physical family as an additional witness to the chosen key.
      assert.equal(typeof forced24GeometryKey, 'string');
      assert.equal(payload.members.length, 1);
      assert.equal(payload.members[0].candidate_id, '1');
      assert.equal(payload.members[0].normalized_solution_key, forced24GeometryKey);
    }
    assert.ok(page, test.label + ' did not read the live minimum portfolio');
    for (const field of ['set_identity_sha256', 'candidate_map_sha256', 'alternative_index',
      'optimal_cardinality', 'member_page_number', 'total_member_pages', 'members']) {
      assert.deepEqual(page[field], payload[field], test.label + '/page/' + field);
    }
  } else {
    assert.equal(test.page, 'replay');
    assert.equal(product.contract, 'pc.path');
    assert.equal(product.content.payload_kind, 'pc-path-family');
    assert.equal(payload.complete, true);
    if (test.empty) {
      assert.equal(payload.witness_count, '0');
      assert.deepEqual(payload.witnesses, []);
      assert.equal(payload.canonical_witness, null);
      assert.notEqual(payload.page_source_available, true);
      assert.equal(page, null);
      assert.equal(frames, null);
    } else {
      assert.equal(payload.materialized_pattern_count, '1');
      assert.equal(payload.geometry_count, '1');
      assert.equal(payload.geometry_page_number, '1');
      assert.equal(payload.member_page_number, '1');
      assert.equal(payload.page_source_available, true);
      assert.ok(page, test.label + ' did not read the live replay source');
      assert.deepEqual(page.witnesses, payload.witnesses);
      assert.deepEqual(payload.canonical_witness, page.witnesses[0]);
      assert.ok(payload.witnesses.length > 0, test.label);
      const witness = payload.canonical_witness;
      assert.match(witness.normalized_trace_key, /^trk2:/u);
      assert.equal(witness.candidate_id, '1');
      assert.equal(witness.pattern_id, '0');
      assert.equal(witness.consumed_piece_count, String(test.pieces));
      assert.equal(witness.terminal_hold_piece, null);
      assert.equal(witness.steps.length, test.pieces);
      assert.equal(witness.steps[0].board_before_mask, '0x' + test.initialMask);
      assert.equal(witness.steps.at(-1).board_after_line_clear_mask, '0x' + '0'.repeat(64));
      for (const [index, step] of witness.steps.entries()) {
        assert.equal(step.active_piece, 'I');
        assert.equal(step.input_cursor, String(index));
        assert.equal(step.output_cursor, String(index + 1));
        assert.equal(step.input_hold_piece, null);
        assert.equal(step.output_hold_piece, null);
        assert.equal(step.cleared_lines, '4');
      }
      assert.deepEqual(replayOriginalPlacements(witness.steps, test.targetLines),
        forced24OriginalPlacements());
      // Frame construction consumes every original mask in the actual
      // production presenter. On this fixture each I clears four rows.
      assert.equal(frames.length, 1 + 2 * test.pieces);
      assert.ok(frames.every(frame => frame.height === test.targetLines && frame.width === 10));
      assert.equal(frames[0].phase, 'initial');
      assert.equal(frames.at(-1).phase, 'after-clear');
      assert.ok(frames.at(-1).cells.every(cell => cell === null));
    }
  }
  // Worker/transport telemetry can differ. The authoritative payload, the
  // separately requested page and the rendered physical frames cannot.
  return { product, page, frames };
}

// Executed only in Chromium; do not shim the Worker implementation, the
// executable-authority protocol, or the terminal result.
async function runWorker({ rootWorker, command, workers, timeoutMs, label, pageKind,
  expectedKind, empty, targetLines, benchmarkProbe = false }) {
  const { createHostCapabilitySnapshot, resolveWorkerAuthority } = await import('/capabilities.js');
  const snapshot = createHostCapabilitySnapshot({
    snapshotId: 'exact-chromium-cpu-smoke', source: 'browser-main',
    reportedLogicalProcessors: navigator.hardwareConcurrency, reportedDeviceMemoryGiB: navigator.deviceMemory,
    webGpuAvailable: false, crossOriginIsolated: self.crossOriginIsolated
  });
  const worker = new Worker(rootWorker, { type: 'module' });
  let verified = 0;
  let lastPhase = 'preparing';
  let requestId = 0;
  let postedAt = null;
  const observations = Object.create(null);
  const progressFields = [
    'worker_count', 'ready_workers', 'active_workers', 'geometry_nodes',
    'candidates_emitted', 'candidates_verified', 'producer_build_nodes', 'build_nodes',
    'producer_coverage_checks', 'coverage_checks', 'oldest_batch_ms'
  ];
  let lastProgress = null;
  let timedOut = false;
  const lifetime = new AbortController();
  const signal = lifetime.signal;
  const timeout = setTimeout(() => {
    timedOut = true;
    lifetime.abort(new Error(label + ' timed out in ' + lastPhase));
  }, timeoutMs);
  worker.addEventListener('error', error => lifetime.abort(new Error(label + ': ' + error.message)));
  worker.addEventListener('messageerror', () => lifetime.abort(new Error(label + ': invalid worker message')));
  worker.addEventListener('message', ({ data }) => {
    if (data.event === 'progress' && data.progress?.telemetry) {
      const telemetry = data.progress.telemetry;
      lastPhase = telemetry.phase;
      if (telemetry.execution_mode === 'distributed' && telemetry.availability?.candidates_verified) {
        verified = Math.max(verified, telemetry.candidates_verified);
      }
      if (benchmarkProbe && postedAt !== null) {
        const elapsed = Math.round(performance.now() - postedAt);
        const phase = observations[lastPhase] ??= {
          first_ms: elapsed, last_ms: elapsed, updates: 0, peak_active_workers: null
        };
        phase.last_ms = elapsed;
        phase.updates += 1;
        if (telemetry.availability?.active_workers) {
          phase.peak_active_workers = Math.max(phase.peak_active_workers ?? 0, telemetry.active_workers);
        }
        lastProgress = {
          elapsed_ms: elapsed, phase: lastPhase, execution_mode: telemetry.execution_mode ?? null,
          producer_complete: telemetry.producer_complete,
          ...Object.fromEntries(progressFields.map(field => [
            field, telemetry.availability?.[field] ? telemetry[field] : null
          ]))
        };
      }
    }
  });

  function waitForMessage(matches, post) {
    return new Promise((resolve, reject) => {
      signal.throwIfAborted();
      const cleanup = () => {
        worker.removeEventListener('message', receive);
        signal.removeEventListener('abort', abort);
      };
      const abort = () => { cleanup(); reject(signal.reason); };
      const receive = ({ data }) => {
        if (!matches(data)) return;
        cleanup();
        resolve(data);
      };
      worker.addEventListener('message', receive);
      signal.addEventListener('abort', abort, { once: true });
      try { post(); } catch (error) { cleanup(); reject(error); }
    });
  }

  function probeProgress() {
    return {
      first_observed_phase_ms: Object.fromEntries(
        Object.entries(observations).map(([name, phase]) => [name, phase.first_ms])
      ),
      phases: observations, last_progress: lastProgress, peer_verified: verified
    };
  }

  try {
    const terminal = await waitForMessage(
      data => ['final_response', 'failed', 'cancelled', 'terminated'].includes(data.event),
      () => {
        if (benchmarkProbe) postedAt = performance.now();
        worker.postMessage({ type: 'run_command_text', commandText: command, prewarmWorkerCount: workers,
          tablebaseRequested: false, hostCapabilitySnapshot: snapshot,
          workerAuthority: resolveWorkerAuthority(snapshot, workers),
          warmupPolicy: { backend: 'cpu', cpuWarmup: false, gpuWarmup: false, verifierWarmup: false } });
      });
    if (benchmarkProbe) {
      // Time ends when the browser receives the Worker terminal, so it
      // includes Worker startup and result transport, but not Playwright's
      // serialization of this compact observation back to Node.
      const elapsed = Math.round(performance.now() - postedAt);
      const report = terminal.search_report;
      const { localSearchProfileText } = await import('/local-search-profile.js');
      const profile = terminal.search_profile;
      const host = localSearchProfileText({
        event: terminal.event,
        search_profile: {
          host_execution: profile?.host_execution,
          verifier_transport: profile?.verifier_transport
        }
      });
      const counters = ['duration_ns', 'invocation_count', 'work_item_count'];
      const stages = Array.isArray(profile?.stages) && profile.stages.length <= 256
        ? profile.stages.filter(row => typeof row?.name === 'string' &&
          /^[a-z][a-z0-9_]*(?:\.[a-z0-9_]+)+$/u.test(row.name) && row.name.length <= 128 &&
          counters.every(key => Number.isSafeInteger(row[key]) && row[key] >= 0))
          .map(({ name, duration_ns, invocation_count, work_item_count }) =>
            ({ name, duration_ns, invocation_count, work_item_count }))
        : [];
      const search = report && {
        backend_selected: report.backend_selected,
        workers_used: report.workers_used,
        cpu_parallel_execution: report.cpu_parallel_execution,
        cpu_parallel_decision_reason: report.cpu_parallel_decision_reason,
        packing_candidate_count: report.packing_candidate_count,
        packing_candidate_set_digest: report.packing_candidate_set_digest,
        unique_solution_count: report.unique_solution_count,
        normalized_solution_set_hash: report.normalized_solution_set_hash,
        covered_pattern_count: report.covered_pattern_count,
        total_possible_pattern_count: report.total_possible_pattern_count,
        count_complete: report.count_complete,
        searched_nodes: report.searched_nodes,
        solver_reported_peak_cpu_bytes: report.peak_cpu_bytes,
      };
      return {
        event: terminal.event, status: terminal.response?.status ?? null,
        runtime_identity: terminal.response?.runtime_identity ?? null,
        diagnostics: terminal.response?.diagnostics ?? null,
        post_to_terminal_ms: elapsed,
        terminal_root_linear_memory_bytes: terminal.runtime_memory_bytes ?? null,
        terminal_retained_result_pages: terminal.runtime_retained_pages ?? null,
        // Rust stage rows belong to this root Worker. Per-peer transport
        // intervals overlap and are not equivalent to peer Rust stage times.
        search_profile: {
          schema_version: profile?.schema_version === 1 ? 1 : null,
          stages, ...(host === null ? {} : JSON.parse(host))
        },
        ...probeProgress(), search,
      };
    }
    if (!pageKind || terminal.event !== 'final_response' || terminal.response.status !== 'success') {
      return { terminal, verified };
    }
    lastPhase = 'product-page';
    const client = await import('/worker-client.js');
    const pager = await import('/product-pager.js');
    const product = terminal.response.product_result_payload;
    if (product?.result_kind !== expectedKind || pager.validateProductResultPayload(product) !== null) {
      throw new Error(label + ' did not present the exact typed result');
    }
    const payload = product.content.payload;
    const current = () => !signal.aborted;
    const get = async (outer, member, maximumWorkSteps) => {
      const request = ++requestId;
      return waitForMessage(
        response => client.isProductPageWorkerEvent(response) && response.request_id === request,
        () => client.postLoadProductMemberPage(worker, request, outer, member, maximumWorkSteps));
    };
    const loadMemberPage = async (outer, member, _signal, maximumWorkSteps = 64) => {
      const message = await get(outer, member, maximumWorkSteps);
      if (message.type !== 'product_page' || message.payload?.runtime !== 'clearra-wasm') {
        throw new Error(label + ' product-page request failed: ' + (message.message ?? message.type));
      }
      return message.payload;
    };
    let productPage = null;
    let frames = null;
    if (pageKind === 'minimum' && payload.page_handle_available) {
      const alternativeIndex = payload.alternative_index;
      const memberPageNumber = payload.member_page_number;
      productPage = await pager.loadCoveragePortfolioExactPage({
        alternativeIndex, memberPageNumber, signal, isCurrent: current, loadMemberPage,
        expectation: {
          setIdentitySha256: payload.set_identity_sha256,
          candidateMapSha256: payload.candidate_map_sha256,
          alternativeIndex, memberPageNumber,
          referencePage: pager.coveragePortfolioPageReference(payload),
          requireSameAlternativeMetadata: true,
        }
      });
    } else if (pageKind === 'replay' && payload.page_source_available) {
      const { loadPcReplayPage } = await import('/replay-pager.js');
      const { buildPcPathReplayFrames } = await import('/replay-presentation.js');
      productPage = await loadPcReplayPage({
        loadMemberPage, reference: payload, geometryPageNumber: payload.geometry_page_number,
        memberPageNumber: payload.member_page_number, signal, isCurrent: current
      });
      frames = buildPcPathReplayFrames(productPage.witnesses[0], targetLines);
    }
    if ((!empty || pageKind === 'minimum') && productPage === null) {
      throw new Error(label + ' lost its exact product page');
    }
    if (productPage === null && (await get('1', '1', 64)).type !== 'product_page_failed') {
      throw new Error(label + ' returned a live page for a result that owns none');
    }
    lastPhase = 'product-page-release';
    // Both messages target the same live Worker and are FIFO. A new Worker
    // could trivially deny this lookup without releasing the actual handle.
    client.postReleaseProductPages(worker);
    const afterRelease = await get('1', '1', 64);
    if (afterRelease.type !== 'product_page_failed') {
      throw new Error(label + ' still owns a product page after release');
    }
    return { terminal, verified, productPage, frames, pageUnavailableAfterRelease: true };
  } catch (error) {
    if (!benchmarkProbe || !signal.aborted) throw error;
    // A censored run has no terminal timing/result. Return bounded progress
    // so Node can log it before the exact-result assertion fails the probe.
    return {
      event: timedOut ? 'probe-timeout' : 'probe-worker-error',
      status: 'incomplete', error: String(error),
      elapsed_before_abort_ms: postedAt === null ? null : Math.round(performance.now() - postedAt),
      ...probeProgress(), search: null
    };
  } finally {
    clearTimeout(timeout);
    worker.terminate();
  }
}
