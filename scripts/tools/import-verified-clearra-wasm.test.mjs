import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, readFile, readdir, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import test from 'node:test';
import {
  createClearraWasmBuildContract,
  serializeClearraWasmManifest,
} from './clearra-wasm-build-contract.mjs';
import {
  importVerifiedClearraWasmDirectory,
  inspectVerifiedClearraWasmDirectory,
} from './import-verified-clearra-wasm.mjs';

const MANIFEST = 'clearra_wasm.manifest.json';
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');

async function fixture(t) {
  const root = await mkdtemp(resolve(tmpdir(), 'clearra-verified-wasm-import-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const repositoryRoot = resolve(root, 'source');
  const sourceDirectory = resolve(root, 'producer');
  const destinationDirectory = resolve(root, 'public', 'wasm');
  await mkdir(repositoryRoot);
  await mkdir(sourceDirectory);
  await writeFile(resolve(repositoryRoot, 'Cargo.toml'), '[workspace]\n');
  await writeFile(resolve(repositoryRoot, 'Cargo.lock'), 'version = 4\n');
  return { root, repositoryRoot, sourceDirectory, destinationDirectory };
}

async function generation(options, index = 1) {
  const bindings = Buffer.from(`export const fixtureGeneration = ${index};\n`);
  const wasm = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0, index]);
  const descriptor = (bytes, prefix, extension) => ({
    path: `${prefix}.${hash(bytes).slice(0, 24)}.${extension}`,
    bytes: bytes.length, sha256: hash(bytes),
  });
  const manifest = {
    schema_version: 1,
    build: await createClearraWasmBuildContract(options.repositoryRoot),
    bindings: descriptor(bindings, 'clearra_wasm', 'js'),
    wasm: descriptor(wasm, 'clearra_wasm_bg', 'wasm'),
  };
  for (const [name, bytes] of [
    [manifest.bindings.path, bindings], ['clearra_wasm.js', bindings],
    [manifest.wasm.path, wasm], ['clearra_wasm_bg.wasm', wasm],
    [MANIFEST, serializeClearraWasmManifest(manifest)],
  ]) await writeFile(resolve(options.sourceDirectory, name), bytes);
  return manifest;
}

test('imports exactly the five original authoritative files without restamping identity', async (t) => {
  const options = await fixture(t);
  const manifest = await generation(options);
  await writeFile(resolve(options.sourceDirectory, 'unrelated-note.txt'), 'not an import input');
  const result = await importVerifiedClearraWasmDirectory(options);
  assert.equal(result.copiedFiles, 5);
  assert.equal(result.generation, manifest.wasm.sha256);
  for (const name of [MANIFEST, manifest.bindings.path, manifest.wasm.path, 'clearra_wasm.js', 'clearra_wasm_bg.wasm']) {
    assert.deepEqual(await readFile(resolve(options.destinationDirectory, name)), await readFile(resolve(options.sourceDirectory, name)));
  }
  const published = await readdir(options.destinationDirectory);
  assert.equal(published.length, 6); // Five artifact files plus local retention metadata.
  assert.ok(!published.includes('unrelated-note.txt'));
  assert.ok(!published.some((path) => /accepted|receipt|benchmark/u.test(path)));
});

test('a stale source contract cannot overwrite an existing verified generation', async (t) => {
  const options = await fixture(t);
  await generation(options);
  await importVerifiedClearraWasmDirectory(options);
  const prior = await readFile(resolve(options.destinationDirectory, MANIFEST));
  await writeFile(resolve(options.repositoryRoot, 'Cargo.toml'), '[workspace]\nresolver = "3"\n');
  await assert.rejects(importVerifiedClearraWasmDirectory(options), /differs from the current source/u);
  assert.deepEqual(await readFile(resolve(options.destinationDirectory, MANIFEST)), prior);
});

for (const changed of ['versioned', 'alias', 'missing', 'traversal', 'oversize', 'identity', 'manifest-size', 'extra-field']) {
  test(`rejects ${changed} input before changing the old manifest`, async (t) => {
    const options = await fixture(t);
    await generation(options);
    await importVerifiedClearraWasmDirectory(options);
    const prior = await readFile(resolve(options.destinationDirectory, MANIFEST));
    const manifest = await generation(options, 2);
    if (changed === 'versioned' || changed === 'alias') {
      const path = changed === 'versioned' ? manifest.wasm.path : 'clearra_wasm_bg.wasm';
      await writeFile(resolve(options.sourceDirectory, path), Buffer.alloc(manifest.wasm.bytes));
    } else if (changed === 'missing') {
      await rm(resolve(options.sourceDirectory, 'clearra_wasm.js'));
    } else if (changed === 'manifest-size') {
      await writeFile(resolve(options.sourceDirectory, MANIFEST), JSON.stringify(manifest));
    } else {
      if (changed === 'traversal') manifest.wasm.path = '../outside.wasm';
      if (changed === 'oversize') manifest.wasm.bytes = Number.MAX_SAFE_INTEGER;
      if (changed === 'identity') manifest.build = {
        ...manifest.build,
        runtime_identity: { ...manifest.build.runtime_identity, source_commit: 'a'.repeat(40) },
      };
      if (changed === 'extra-field') manifest.accepted = true;
      await writeFile(resolve(options.sourceDirectory, MANIFEST), serializeClearraWasmManifest(manifest));
    }
    await assert.rejects(importVerifiedClearraWasmDirectory(options));
    assert.deepEqual(await readFile(resolve(options.destinationDirectory, MANIFEST)), prior);
  });
}

test('source and publication directory overlap is rejected without deleting input', async (t) => {
  const options = await fixture(t);
  const manifest = await generation(options);
  for (const destinationDirectory of [options.sourceDirectory, resolve(options.sourceDirectory, 'nested'), options.root]) {
    await assert.rejects(importVerifiedClearraWasmDirectory({ ...options, destinationDirectory }), /separate|overlap/u);
  }
  assert.equal((await inspectVerifiedClearraWasmDirectory(options)).manifest.wasm.sha256, manifest.wasm.sha256);
});

test('managed producer lock prevents concurrent import without altering current manifest', async (t) => {
  const options = await fixture(t);
  await generation(options);
  await importVerifiedClearraWasmDirectory(options);
  const prior = await readFile(resolve(options.destinationDirectory, MANIFEST));
  await writeFile(resolve(options.root, 'public', '.clearra-wasm-stage.lock'), JSON.stringify({ pid: process.pid }));
  await assert.rejects(importVerifiedClearraWasmDirectory(options), /already active/u);
  assert.deepEqual(await readFile(resolve(options.destinationDirectory, MANIFEST)), prior);
});

test('destination cannot be the managed staging path and its existing contents survive', async (t) => {
  const options = await fixture(t);
  await generation(options);
  const destinationDirectory = resolve(options.root, '.clearra-wasm-stage');
  await mkdir(destinationDirectory);
  const sentinel = resolve(destinationDirectory, 'existing-user-data.txt');
  await writeFile(sentinel, 'must remain untouched');
  await assert.rejects(
    importVerifiedClearraWasmDirectory({ ...options, destinationDirectory }),
    /cannot overlap the managed publication staging/u,
  );
  assert.equal(await readFile(sentinel, 'utf8'), 'must remain untouched');
});

test('retention removes only the sixth proven old generation and preserves current files', async (t) => {
  const options = await fixture(t);
  const generations = [];
  for (let index = 1; index <= 6; index += 1) {
    generations.push(await generation(options, index));
    const result = await importVerifiedClearraWasmDirectory(options);
    assert.equal(result.retention.status, 'retained');
  }
  const names = await readdir(options.destinationDirectory);
  assert.ok(!names.includes(generations[0].wasm.path));
  assert.ok(!names.includes(generations[0].bindings.path));
  for (const manifest of generations.slice(1)) {
    assert.ok(names.includes(manifest.wasm.path));
    assert.ok(names.includes(manifest.bindings.path));
  }
});

test('linked input artifacts are rejected', async (t) => {
  const options = await fixture(t);
  const manifest = await generation(options);
  const alias = resolve(options.sourceDirectory, 'clearra_wasm_bg.wasm');
  await rm(alias);
  try { await symlink(resolve(options.sourceDirectory, manifest.wasm.path), alias, 'file'); }
  catch (error) {
    if (['EPERM', 'EACCES'].includes(error.code)) { t.skip('OS does not grant test symlink creation'); return; }
    throw error;
  }
  await assert.rejects(importVerifiedClearraWasmDirectory(options), /linked/u);
});
