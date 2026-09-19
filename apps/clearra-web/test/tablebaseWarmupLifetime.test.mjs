import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const worker = await readFile(
  new URL('../src/workers/clearraWorker.ts', import.meta.url),
  'utf8'
);
const assets = await readFile(
  new URL('../src/workers/pc4TablebaseAssets.ts', import.meta.url),
  'utf8'
);
const controller = await readFile(
  new URL('../../../packages/clearra-ui/src/lib/wasm/WasmTerminalWorkerController.ts', import.meta.url),
  'utf8'
);

function functionBody(name) {
  const marker = `function ${name}(`;
  const start = worker.indexOf(marker);
  assert.notEqual(start, -1, `${name} must exist`);
  const open = worker.indexOf('{', start);
  assert.notEqual(open, -1, `${name} must have a body`);
  let depth = 0;
  for (let index = open; index < worker.length; index += 1) {
    if (worker[index] === '{') depth += 1;
    if (worker[index] === '}') depth -= 1;
    if (depth === 0) return worker.slice(open + 1, index);
  }
  assert.fail(`${name} body is not balanced`);
}

test('turning TB off preserves worker-owned online preparation', () => {
  const toggle = functionBody('setTablebaseRequested');
  assert.match(toggle, /if \(requested\)[\s\S]*startTablebaseTransportWarmup/u);
  assert.doesNotMatch(toggle, /tablebaseWarmupGeneration\s*\+=/u);
  assert.doesNotMatch(toggle, /releasePc4TablebaseAssets\s*\(/u);
  assert.match(toggle, /loadedWasm\?\.release_tablebase\(\)/u);

  const warmup = functionBody('startTablebaseTransportWarmup');
  assert.match(
    warmup,
    /generation !== tablebaseWarmupGeneration \|\| !tablebaseRequested/u
  );

  const controllerPrewarmStart = controller.indexOf('  prewarm(');
  const controllerPrewarmEnd = controller.indexOf('\n  cancel()', controllerPrewarmStart);
  const controllerPrewarm = controller.slice(controllerPrewarmStart, controllerPrewarmEnd);
  assert.doesNotMatch(
    controllerPrewarm,
    /tablebaseChanged[\s\S]*disposeOwnedWorker/u
  );
});

test('TB transport handshake starts before the WASM capability join', () => {
  const toggle = functionBody('setTablebaseRequested');
  assert.match(toggle, /void startTablebaseTransportWarmup\(\)/u);

  const join = functionBody('startTablebaseWarmupAfterWasm');
  assert.match(join, /await startTablebaseTransportWarmup\(\)/u);
  assert.match(join, /!wasm\.configure_online_pc4/u);

  const transport = functionBody('startTablebaseTransportWarmup');
  assert.doesNotMatch(transport, /ClearraWasmModule|configure_online_pc4/u);
});

test('turning TB on during another job still starts transport preparation', () => {
  const prewarm = functionBody('startRuntimePrewarm');
  const activeGuard = prewarm.indexOf('if (active)');
  const activeTransportIntent = prewarm.indexOf(
    'if (requestedTablebase && !tablebaseRequested) setTablebaseRequested(true)',
    activeGuard
  );
  assert.ok(activeGuard >= 0, 'active-job guard must exist');
  assert.ok(
    activeTransportIntent > activeGuard,
    'the active-job branch must start the requested TB transport warmup'
  );
  assert.ok(
    activeTransportIntent < prewarm.indexOf('return;', activeGuard),
    'transport warmup must start before returning from the active-job branch'
  );
  assert.match(
    controller,
    /if \(this\.runInFlight\)[\s\S]*tablebaseChanged && tablebaseRequested && this\.worker[\s\S]*postPrewarmRuntime\(/u
  );
  const controllerPrewarmStart = controller.indexOf('  prewarm(');
  const controllerActiveStart = controller.indexOf('if (this.runInFlight)', controllerPrewarmStart);
  const controllerActiveEnd = controller.indexOf('\n      return;', controllerActiveStart);
  assert.ok(controllerActiveStart >= 0 && controllerActiveEnd > controllerActiveStart);
  assert.doesNotMatch(
    controller.slice(controllerActiveStart, controllerActiveEnd),
    /prewarmingWorker\s*=/u
  );
});

test('only terminal worker lifecycle owners discard online preparation', () => {
  for (const owner of ['disposeRuntime', 'closeFailClosedWorker']) {
    const body = functionBody(owner);
    assert.match(body, /tablebaseWarmupGeneration\s*\+=\s*1/u);
    assert.match(body, /releasePc4TablebaseAssets\s*\(\)/u);
  }
});

test('a cached online generation performs one bounded transport touch on re-enable', () => {
  assert.match(assets, /cachedProvider !== 'online'/u);
  assert.match(assets, /TRANSPORT_TOUCH_FLOOR_MS/u);
  assert.match(assets, /touchPc4OnlineTransport\(bundle\.generation/u);
  assert.match(assets, /maxBytes:\s*1/u);
  assert.match(assets, /maxRequests:\s*1/u);
  assert.match(assets, /reader\.read\(artifact, 0, 1\)/u);
  assert.doesNotMatch(assets, /setInterval|setTimeout/u);
});
