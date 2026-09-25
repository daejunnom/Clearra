import assert from 'node:assert/strict';
import test from 'node:test';
import { BrowserGpuAvailability } from '../src/workers/BrowserGpuAvailability.ts';

test('missing and null adapters preserve CPU admission without reading adapter info', async () => {
  const owner = new BrowserGpuAvailability();
  assert.equal((await owner.qualify(false, undefined)).available, false);
  const result = await owner.qualify(true, { requestAdapter: async () => null });
  assert.deepEqual(result, { available: false, reason: 'adapter-unavailable' });
  assert.equal(owner.available, false);
});
test('rejected and synchronous-throwing GPU discovery are unavailable, not execution failures', async () => {
  for (const requestAdapter of [() => Promise.reject(new Error('driver')), () => { throw new Error('driver'); }]) {
    const owner = new BrowserGpuAvailability();
    assert.equal((await owner.qualify(true, { requestAdapter })).reason, 'adapter-rejected');
    assert.equal(owner.available, false);
  }
});
test('a stalled adapter has a finite deadline and late completion cannot re-enable it', async () => {
  const owner = new BrowserGpuAvailability();
  let resolve;
  const result = await owner.qualify(true, { requestAdapter: () => new Promise(done => { resolve = done; }) }, 10);
  assert.equal(result.reason, 'adapter-timeout');
  resolve({});
  await Promise.resolve(); await Promise.resolve();
  assert.equal(owner.available, false);
});
test('concurrent preparation shares one probe and a usable adapter stays eligible', async () => {
  const owner = new BrowserGpuAvailability();
  let calls = 0;
  const gpu = { requestAdapter: async () => { calls++; return { get info() { throw new Error('must not access info'); } }; } };
  const [a, b] = await Promise.all([owner.qualify(true, gpu), owner.qualify(true, gpu)]);
  assert.equal(a, b); assert.equal(calls, 1); assert.equal(owner.available, true);
});
