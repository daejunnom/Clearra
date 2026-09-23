import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const root = resolve(import.meta.dirname, '..', '..');
const artifactRoot = resolve(root, 'apps', 'clearra-web', 'static', 'wasm');
const encoder = new TextDecoder();

test('exact WASM validates and installs a real qualified conditioned pack', async () => {
  const manifest = JSON.parse(await readFile(
    resolve(artifactRoot, 'clearra_wasm.manifest.json'), 'utf8'
  ));
  const wasm = await readFile(resolve(artifactRoot, manifest.wasm.path));
  const bindings = await import(pathToFileURL(resolve(artifactRoot, manifest.bindings.path)).href);
  const raw = await bindings.default(wasm);
  const output = () => {
    const pointer = raw.clearra_wasm_output_ptr() >>> 0;
    const length = raw.clearra_wasm_output_len() >>> 0;
    const text = encoder.decode(new Uint8Array(raw.memory.buffer, pointer, length));
    assert.equal(raw.clearra_wasm_output_release(), 0);
    return JSON.parse(text);
  };

  // One real profile is sufficient for this ABI smoke. All five profile
  // payloads have already passed the independent source-bound qualification
  // and explicit native CLI admission; this test does not regenerate them.
  assert.equal(raw.clearra_wasm_accelerator_catalog(1, 4), 0);
  const plan = output();
  assert.equal(plan.product, 'board-conditioned-reachability');
  assert.equal(plan.profile, 'no-kick');
  assert.equal(plan.state, 'qualified');
  assert.ok(plan.payload_bytes > 0 && plan.payload_bytes <= 16 * 1024 * 1024);
  assert.match(plan.url, /^https:\/\/github\.com\/daejunnom\/Clearra\/releases\/download\//u);

  const response = await fetch(plan.url, { signal: AbortSignal.timeout(30_000) });
  assert.equal(response.status, 200, `immutable pack GET failed: ${response.status}`);
  const bytes = new Uint8Array(await response.arrayBuffer());
  assert.equal(bytes.byteLength, plan.payload_bytes);
  assert.equal(createHash('sha256').update(bytes).digest('hex'), plan.payload_identity);

  assert.equal(raw.clearra_wasm_transfer_resize(bytes.byteLength), 0);
  const pointer = raw.clearra_wasm_transfer_ptr() >>> 0;
  new Uint8Array(raw.memory.buffer, pointer, bytes.byteLength).set(bytes);
  assert.equal(raw.clearra_wasm_accelerator_admit(1, 4, 1), 0);
  assert.deepEqual(output(), {
    state: 'ready',
    product: 'board-conditioned-reachability',
    profile: 'no-kick',
    generation: plan.generation
  });

  assert.equal(raw.clearra_wasm_accelerator_remove(1, 4), 0);
  assert.deepEqual(output(), {
    state: 'not_loaded',
    product: 'board-conditioned-reachability',
    profile: 'no-kick',
    removed: true
  });
});
