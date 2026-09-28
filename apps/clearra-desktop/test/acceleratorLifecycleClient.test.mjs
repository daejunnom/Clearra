import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { stripTypeScriptTypes } from 'node:module';

const url = new URL('../src/lib/acceleratorLifecycleClient.ts', import.meta.url);
const source = stripTypeScriptTypes(await readFile(url, 'utf8'), { mode: 'transform', sourceUrl: url.href });
const { inspectAcceleratorAsset } = await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`);

test('Desktop inspection completes check before starting status and keeps one captured selection', async () => {
  const calls = [];
  let finish;
  const selection = { product: 'exact-legal-board', profile: 'srs-plus' };
  const inspection = inspectAcceleratorAsset(async (command, args) => {
    calls.push({ command, ...args });
    if (args.action === 'check') return await new Promise(resolve => { finish = resolve; });
    return '{"installed":true}';
  }, selection);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].action, 'check');
  selection.product = 'board-conditioned-reachability';
  selection.profile = 'jstris-180';
  finish('{"qualified":true}');
  assert.deepEqual(await inspection, { catalog: '{"qualified":true}', local: '{"installed":true}' });
  assert.deepEqual(calls.map(call => call.action), ['check', 'status']);
  assert.ok(calls.every(call => call.product === 'exact-legal-board' && call.profile === 'srs-plus'));
});

test('Desktop destroyed inspection does not start a second lifecycle action', async () => {
  const calls = [];
  await assert.rejects(inspectAcceleratorAsset(async (_command, args) => {
    calls.push(args.action);
    return '{}';
  }, { product: 'exact-legal-board', profile: 'srs' }, () => false), /accelerator_inspection_cancelled/u);
  assert.deepEqual(calls, ['check']);
});

test('Desktop native lifecycle refusal preserves its cause and is not retried or bypassed', async () => {
  let calls = 0;
  await assert.rejects(inspectAcceleratorAsset(async () => {
    calls += 1;
    throw new Error('accelerator: another lifecycle operation is active');
  }, { product: 'board-conditioned-reachability', profile: 'srs-x' }), /another lifecycle operation is active/u);
  assert.equal(calls, 1);
});
