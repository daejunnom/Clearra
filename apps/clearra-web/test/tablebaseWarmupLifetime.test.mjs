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
