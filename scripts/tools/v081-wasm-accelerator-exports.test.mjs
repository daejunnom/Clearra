import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { resolve } from 'node:path';

import { createClearraWasmBuildContract } from './clearra-wasm-build-contract.mjs';

const root = resolve(import.meta.dirname, '..', '..');
const artifactRoot = resolve(root, 'apps', 'clearra-web', 'static', 'wasm');

test('exact v0.8.1 WASM exposes signed accelerator handoff exports', async () => {
  const manifest = JSON.parse(await readFile(
    resolve(artifactRoot, 'clearra_wasm.manifest.json'), 'utf8'
  ));
  const expected = await createClearraWasmBuildContract(root);
  assert.equal(manifest.build.source_sha256, expected.source_sha256);
  assert.equal(manifest.build.capabilities_sha256, expected.capabilities_sha256);
  assert.match(manifest.wasm.path, /^clearra_wasm_bg\.[0-9a-f]{24}\.wasm$/u);
  const wasm = await readFile(resolve(artifactRoot, manifest.wasm.path));
  assert.equal(wasm.byteLength, manifest.wasm.bytes);
  assert.equal(createHash('sha256').update(wasm).digest('hex'), manifest.wasm.sha256);
  const exports = new Set(WebAssembly.Module.exports(await WebAssembly.compile(wasm))
    .map(item => item.name));
  for (const name of [
    'clearra_wasm_accelerator_catalog',
    'clearra_wasm_accelerator_admit',
    'clearra_wasm_accelerator_remove',
    'clearra_wasm_accelerator_export_negative_synopsis',
    'clearra_wasm_accelerator_admit_negative_synopsis'
  ]) {
    assert.ok(exports.has(name), `missing accelerator export: ${name}`);
  }
});
