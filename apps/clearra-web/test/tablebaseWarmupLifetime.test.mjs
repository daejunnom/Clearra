import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const worker = await readFile(
  new URL('../src/workers/clearraWorker.ts', import.meta.url),
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
  assert.match(toggle, /if \(requested\) return;/u);
  assert.doesNotMatch(toggle, /tablebaseWarmupGeneration\s*\+=/u);
  assert.doesNotMatch(toggle, /releasePc4TablebaseAssets\s*\(/u);
  assert.match(toggle, /loadedWasm\?\.release_tablebase\(\)/u);

  const warmup = functionBody('startTablebaseWarmupAfterWasm');
  assert.match(
    warmup,
    /generation !== tablebaseWarmupGeneration \|\| !tablebaseRequested/u
  );
});

test('only terminal worker lifecycle owners discard online preparation', () => {
  for (const owner of ['disposeRuntime', 'closeFailClosedWorker']) {
    const body = functionBody(owner);
    assert.match(body, /tablebaseWarmupGeneration\s*\+=\s*1/u);
    assert.match(body, /releasePc4TablebaseAssets\s*\(\)/u);
  }
});
