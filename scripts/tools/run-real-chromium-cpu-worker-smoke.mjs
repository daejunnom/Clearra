// Functional Chromium execution of the product Worker, not a full Pages/UI
// build, a signed asset qualification, or a performance measurement.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, realpath } from 'node:fs/promises';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
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
const PORT = 4194;
const ORIGIN = 'http://127.0.0.1:4194';
const ROOT_WORKER = '/workers/clearraWorker.ts';
const VERIFIER_WORKER = '/workers/clearraVerifierWorker.ts';
const PLAYWRIGHT_VERSION = '1.56.1';
const CASE_TIMEOUT_MS = 40_000;
const ACCELERATORS_OFF = '--no-legal-board --no-conditioned-reachability';
const PC_CPU = '--objective unique --count unique --queue-knowledge oracle --backend cpu ' +
  '--rule srs-plus --no-hold --no-tablebase ' + ACCELERATORS_OFF;

assert.equal(process.argv.length, 2, 'the Chromium smoke does not accept options');
assert.ok(process.env.CLEARRA_RUNTIME_SUPERVISED === '1' && process.env.CLEARRA_RUNTIME_PROFILE === 'local-service',
  'run the reserved 4194 listener under clearra-manage runtime run --profile local-service --timeout 360');
for (const name of ['CLEARRA_BROWSER_TOOLS_ROOT', 'CLEARRA_REAL_ACCELERATOR_WASM_DIR',
  'CLEARRA_SOURCE_COMMIT', 'CLEARRA_ENGINE_BUILD_ID']) {
  assert.ok(process.env[name], name + ' is required');
}
const source = process.env.CLEARRA_SOURCE_COMMIT;
assert.match(source, /^[0-9a-f]{40}$/u);
assert.equal(process.env.CLEARRA_ENGINE_BUILD_ID, source);

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
  .filter(row => row.startsWith('forced-24L\t')).map(row => row.split('\t'));
assert.equal(fixture.length, 1);
const [, height, mask, pieces, queue] = fixture[0];
assert.equal(height, '24');
assert.match(mask, /^[a-f0-9]{64}$/u);
assert.equal(pieces, '6');
assert.equal(queue, 'IIIIII');
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
      'ctk1|initial=0000000000000000|placements=I:00000000000003c0'] }
];

const toolsRoot = await realpath(process.env.CLEARRA_BROWSER_TOOLS_ROOT);
const require = createRequire(join(toolsRoot, 'package.json'));
assert.equal(require('playwright/package.json').version, PLAYWRIGHT_VERSION);
const { chromium } = require('playwright');
const owner = enterManagedBuildOrRelaunch(repository, [SELF], 'experiment');
assert.equal(owner.purpose, 'experiment', 'the browser smoke must not write into a product transaction');
const scripts = await build({
  absWorkingDir: repository, bundle: true, write: false, format: 'esm', platform: 'browser',
  target: 'es2022', conditions: ['browser'], mainFields: ['browser', 'module', 'main'],
  define: { 'import.meta.env.MODE': '"production"' },
  outdir: join(owner.transaction, 'chromium-cpu-worker-in-memory'),
  entryPoints: {
    clearraWorker: 'apps/clearra-web/src/workers/clearraWorker.ts',
    clearraVerifierWorker: 'apps/clearra-web/src/workers/clearraVerifierWorker.ts',
    capabilities: 'packages/clearra-ui/src/lib/wasm/hostCapabilitySnapshot.ts'
  }
});
const compiled = new Map(scripts.outputFiles.map(file => [basename(file.path), file.contents]));
assert.equal(compiled.size, 3);
for (const [url, entry] of [[ROOT_WORKER, 'clearraWorker.js'],
  [VERIFIER_WORKER, 'clearraVerifierWorker.js'], ['/capabilities.js', 'capabilities.js']]) {
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
  // Let the OS reject an occupied 4194; do not adopt/stop its existing owner.
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
  const available = await page.evaluate(() => navigator.hardwareConcurrency);
  assert.ok(Number.isSafeInteger(available) && available >= 3, 'the Chromium smoke needs three real logical processors');
  for (const test of tests) {
    const runs = [];
    let distributedWorkers = 0;
    for (const workers of [1, test.workers]) {
      const before = verifierLoads;
      const execution = await page.evaluate(runWorker, {
        rootWorker: ROOT_WORKER, command: test.command + ' --workers ' + workers, workers,
        timeoutMs: CASE_TIMEOUT_MS, label: test.label
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
        distributedWorkers = report.workers_used;
        assert.ok(Number.isSafeInteger(distributedWorkers) && distributedWorkers >= 2 &&
          distributedWorkers <= workers, test.label + ' did not complete on a valid worker subset');
        assert.ok(verifierLoads > before, test.label + ' did not load a real Chromium verifier');
        assert.ok(execution.verified > 0, test.label + ' did not verify a candidate in a peer');
      } else {
        assert.equal(report.workers_used, 1, test.label);
        assert.equal(verifierLoads, before, test.label + ' unexpectedly created a verifier');
      }
      runs.push(exactProjection(test, report));
    }
    assert.deepEqual(runs[1], runs[0], test.label + ' serial/distributed result mismatch');
    console.log('chromium_cpu_worker_case=passed case=' + test.label + ' workers_requested=' + test.workers +
      ' workers_used=' + distributedWorkers +
      ' solutions=' + runs[0].keys.length + ' covered=' + runs[0].covered);
  }
  assert.deepEqual(errors, []);
  assert.deepEqual([...unexpected], [], 'the browser requested files outside the exact asset-free allowlist');
  console.log('chromium_cpu_worker_smoke=passed source=' + source + ' wasm_sha256=' + manifest.wasm.sha256 +
    ' verifier_loads=' + verifierLoads + ' release_authority=false');
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

// Executed only in Chromium; do not shim the Worker implementation, the
// executable-authority protocol, or the terminal result.
async function runWorker({ rootWorker, command, workers, timeoutMs, label }) {
  const { createHostCapabilitySnapshot, resolveWorkerAuthority } = await import('/capabilities.js');
  const snapshot = createHostCapabilitySnapshot({
    snapshotId: 'exact-chromium-cpu-smoke', source: 'browser-main',
    reportedLogicalProcessors: navigator.hardwareConcurrency, reportedDeviceMemoryGiB: navigator.deviceMemory,
    webGpuAvailable: false, crossOriginIsolated: self.crossOriginIsolated
  });
  const worker = new Worker(rootWorker, { type: 'module' });
  let timeout;
  let verified = 0;
  let lastPhase = 'preparing';
  try {
    const terminal = await new Promise((resolve, reject) => {
      timeout = setTimeout(() => reject(new Error(label + ' timed out in ' + lastPhase)), timeoutMs);
      worker.addEventListener('error', error => reject(new Error(label + ': ' + error.message)));
      worker.addEventListener('messageerror', () => reject(new Error(label + ': invalid worker message')));
      worker.addEventListener('message', ({ data }) => {
        if (data.event === 'progress' && data.progress?.telemetry) {
          const telemetry = data.progress.telemetry;
          lastPhase = telemetry.phase;
          if (telemetry.execution_mode === 'distributed' && telemetry.availability?.candidates_verified) {
            verified = Math.max(verified, telemetry.candidates_verified);
          }
        }
        if (['final_response', 'failed', 'cancelled', 'terminated'].includes(data.event)) resolve(data);
      });
      worker.postMessage({ type: 'run_command_text', commandText: command, prewarmWorkerCount: workers,
        tablebaseRequested: false, hostCapabilitySnapshot: snapshot,
        workerAuthority: resolveWorkerAuthority(snapshot, workers),
        warmupPolicy: { backend: 'cpu', cpuWarmup: false, gpuWarmup: false, verifierWarmup: false } });
    });
    return { terminal, verified };
  } finally {
    clearTimeout(timeout);
    worker.terminate();
  }
}
