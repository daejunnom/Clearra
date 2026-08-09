import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import test from 'node:test';

import {
  CLEARRA_WASM_REQUIRED_CAPABILITIES,
  clearraWasmCapabilitiesSha256,
  clearraWasmBuildContractsEqual,
  createClearraWasmBuildContract,
  isClearraWasmBuildContract,
} from './clearra-wasm-build-contract.mjs';

test('WASM build contract follows compile inputs and ignores unrelated documents', async () => {
  const root = await mkdtemp(resolve(tmpdir(), 'clearra-wasm-contract-'));
  try {
    await mkdir(resolve(root, 'crates/example/src'), { recursive: true });
    await mkdir(resolve(root, 'node_modules/ignored'), { recursive: true });
    await writeFile(resolve(root, 'Cargo.toml'), '[workspace]\nmembers=["crates/example"]\n');
    await writeFile(resolve(root, 'Cargo.lock'), '# fixture lock\n');
    await writeFile(resolve(root, 'crates/example/Cargo.toml'), '[package]\nname="example"\n');
    await writeFile(resolve(root, 'crates/example/src/lib.rs'), 'pub const VALUE: u8 = 1;\n');
    await writeFile(resolve(root, 'private-data.json'), '{"ignored":1}\n');
    await writeFile(resolve(root, 'README.md'), 'ignored one\n');
    await writeFile(resolve(root, 'node_modules/ignored/lib.rs'), 'ignored\n');

    const initial = await createClearraWasmBuildContract(root);
    assert.equal(isClearraWasmBuildContract(initial), true);
    assert.ok(CLEARRA_WASM_REQUIRED_CAPABILITIES.includes('finesse/search/v1'));
    assert.ok(CLEARRA_WASM_REQUIRED_CAPABILITIES.includes('finesse/score/v1'));

    await writeFile(resolve(root, 'private-data.json'), '{"ignored":2}\n');
    await writeFile(resolve(root, 'README.md'), 'ignored two\n');
    const afterIgnoredChanges = await createClearraWasmBuildContract(root);
    assert.equal(clearraWasmBuildContractsEqual(initial, afterIgnoredChanges), true);

    await writeFile(resolve(root, 'crates/example/src/lib.rs'), 'pub const VALUE: u8 = 2;\n');
    const afterSourceChange = await createClearraWasmBuildContract(root);
    assert.equal(clearraWasmBuildContractsEqual(initial, afterSourceChange), false);
    assert.notEqual(initial.source_sha256, afterSourceChange.source_sha256);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('WASM build contract rejects legacy or partial manifest build metadata', () => {
  assert.equal(isClearraWasmBuildContract(undefined), false);
  assert.equal(
    isClearraWasmBuildContract({
      contract_version: 1,
      source_sha256: '0'.repeat(64),
      source_file_count: 1,
      capabilities_sha256: '0'.repeat(64),
    }),
    false
  );
});

test('browser worker requires the same finesse capability contract', async () => {
  const runtimeSource = await readFile(
    resolve(import.meta.dirname, '..', '..', 'apps', 'clearra-web', 'src', 'workers', 'clearraWasmRuntime.ts'),
    'utf8'
  );
  assert.match(runtimeSource, new RegExp(clearraWasmCapabilitiesSha256()));
  assert.match(runtimeSource, /isBuildContract\(candidate\.build\)/);
});
