import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import test from 'node:test';

import {
  CLEARRA_ARTIFACT_SCHEMA_VERSION,
  CLEARRA_CONTRACT_SCHEMA_VERSION,
  CLEARRA_SUPPLY_SEMANTICS_ID,
  CLEARRA_UNVERIFIED_BUILD_ID,
  CLEARRA_WASM_MANIFEST_BYTES,
  CLEARRA_WASM_REQUIRED_CAPABILITIES,
  clearraWasmCapabilitiesSha256,
  clearraWasmBuildContractsEqual,
  createClearraWasmBuildContract,
  isClearraWasmBuildContract,
  productBuildIdentityFromEnvironment,
  serializeClearraWasmManifest,
} from './clearra-wasm-build-contract.mjs';

test('WASM build contract follows compile inputs and ignores unrelated documents', async () => {
  const root = await mkdtemp(resolve(tmpdir(), 'clearra-wasm-contract-'));
  try {
    await mkdir(resolve(root, 'crates/example/src'), { recursive: true });
    await mkdir(resolve(root, 'node_modules/ignored'), { recursive: true });
    await mkdir(resolve(root, 'scripts/tools'), { recursive: true });
    await writeFile(resolve(root, 'Cargo.toml'), '[workspace]\nmembers=["crates/example"]\n');
    await writeFile(resolve(root, 'Cargo.lock'), '# fixture lock\n');
    await writeFile(resolve(root, 'crates/example/Cargo.toml'), '[package]\nname="example"\n');
    await writeFile(resolve(root, 'crates/example/src/lib.rs'), 'pub const VALUE: u8 = 1;\n');
    await writeFile(
      resolve(root, 'scripts/tools/clearra-wasm-generation-retention.mjs'),
      'export const RETAIN = 5;\n',
    );
    await writeFile(resolve(root, 'private-data.json'), '{"ignored":1}\n');
    await writeFile(resolve(root, 'README.md'), 'ignored one\n');
    await writeFile(resolve(root, 'node_modules/ignored/lib.rs'), 'ignored\n');

    const initial = await createClearraWasmBuildContract(root, {});
    assert.equal(isClearraWasmBuildContract(initial), true);
    assert.ok(CLEARRA_WASM_REQUIRED_CAPABILITIES.includes('finesse/search/v1'));
    assert.ok(CLEARRA_WASM_REQUIRED_CAPABILITIES.includes('finesse/score/v1'));
    assert.ok(
      CLEARRA_WASM_REQUIRED_CAPABILITIES.includes(
        'wasm-abi/availability-exactness/v1'
      )
    );
    assert.ok(CLEARRA_WASM_REQUIRED_CAPABILITIES.includes('product-build-identity/v1'));
    assert.deepEqual(initial.runtime_identity, {
      source_commit: CLEARRA_UNVERIFIED_BUILD_ID,
      engine_build_id: CLEARRA_UNVERIFIED_BUILD_ID,
      contract_schema_version: CLEARRA_CONTRACT_SCHEMA_VERSION,
      supply_semantics_id: CLEARRA_SUPPLY_SEMANTICS_ID,
      artifact_schema_version: CLEARRA_ARTIFACT_SCHEMA_VERSION,
    });

    await writeFile(resolve(root, 'private-data.json'), '{"ignored":2}\n');
    await writeFile(resolve(root, 'README.md'), 'ignored two\n');
    const afterIgnoredChanges = await createClearraWasmBuildContract(root, {});
    assert.equal(clearraWasmBuildContractsEqual(initial, afterIgnoredChanges), true);

    await writeFile(
      resolve(root, 'scripts/tools/clearra-wasm-generation-retention.mjs'),
      'export const RETAIN = 6;\n',
    );
    const afterRetentionProducerChange = await createClearraWasmBuildContract(root, {});
    assert.equal(clearraWasmBuildContractsEqual(initial, afterRetentionProducerChange), false);
    await writeFile(
      resolve(root, 'scripts/tools/clearra-wasm-generation-retention.mjs'),
      'export const RETAIN = 5;\n',
    );
    const afterRetentionProducerRestore = await createClearraWasmBuildContract(root, {});
    assert.equal(clearraWasmBuildContractsEqual(initial, afterRetentionProducerRestore), true);

    await writeFile(resolve(root, 'crates/example/src/lib.rs'), 'pub const VALUE: u8 = 2;\n');
    const afterSourceChange = await createClearraWasmBuildContract(root, {});
    assert.equal(clearraWasmBuildContractsEqual(initial, afterSourceChange), false);
    assert.notEqual(initial.source_sha256, afterSourceChange.source_sha256);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('WASM manifest has fixed room for an exact five-field release identity', () => {
  const commit = 'a'.repeat(40);
  const manifest = {
    schema_version: 1,
    build: {
      contract_version: 2,
      source_sha256: 'b'.repeat(64),
      source_file_count: 2_999,
      capabilities_sha256: clearraWasmCapabilitiesSha256(),
      runtime_identity: productBuildIdentityFromEnvironment({
        CLEARRA_SOURCE_COMMIT: commit,
        CLEARRA_ENGINE_BUILD_ID: commit,
      }),
    },
    bindings: {
      path: `clearra_wasm.${'c'.repeat(24)}.js`,
      bytes: 99_999_999,
      sha256: 'c'.repeat(64),
    },
    wasm: {
      path: `clearra_wasm_bg.${'d'.repeat(24)}.wasm`,
      bytes: 999_999_999,
      sha256: 'd'.repeat(64),
    },
  };

  const serialized = serializeClearraWasmManifest(manifest);
  assert.equal(Buffer.byteLength(serialized, 'utf8'), CLEARRA_WASM_MANIFEST_BYTES);
  assert.deepEqual(JSON.parse(serialized), manifest);
});

test('WASM release identity rejects partial or malformed commit pinning', () => {
  assert.throws(
    () =>
      productBuildIdentityFromEnvironment({
        CLEARRA_SOURCE_COMMIT: 'a'.repeat(40),
      }),
    /requires both full lowercase source and engine commit IDs/u
  );
  assert.throws(
    () =>
      productBuildIdentityFromEnvironment({
        CLEARRA_SOURCE_COMMIT: 'not-a-commit',
        CLEARRA_ENGINE_BUILD_ID: 'not-a-commit',
      }),
    /requires both full lowercase source and engine commit IDs/u
  );
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
  assert.equal(
    isClearraWasmBuildContract({
      contract_version: 1,
      source_sha256: '0'.repeat(64),
      source_file_count: 1,
      capabilities_sha256:
        '6e6e2c1e973f62c6d6fa28f571b326104aec625e6879c4aca67df3364029d98b',
    }),
    false
  );
});

test('browser worker requires the current WASM ABI capability contract', async () => {
  const runtimeSource = await readFile(
    resolve(import.meta.dirname, '..', '..', 'apps', 'clearra-web', 'src', 'workers', 'clearraWasmRuntime.ts'),
    'utf8'
  );
  assert.match(runtimeSource, new RegExp(clearraWasmCapabilitiesSha256()));
  assert.match(runtimeSource, /isBuildContract\(candidate\.build\)/);
  assert.match(
    runtimeSource,
    /assertClearraWasmAvailabilityExactnessExports\(raw\)/
  );
});
