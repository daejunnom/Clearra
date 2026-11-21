import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, readFile, readdir, realpath, rm, symlink, writeFile } from 'node:fs/promises';
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
import { acquireManagedTransientDirectory } from './managed-transient-directory.mjs';
import { readChromiumGateWasmInput } from './chromium-gate-wasm-input.mjs';
import { sealAcceptedWasmBuild } from '../release/accepted-wasm-build.mjs';

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

async function generation(options, index = 1, environment = process.env) {
  const bindings = Buffer.from(`export const fixtureGeneration = ${index};\n`);
  const wasm = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0, index]);
  const descriptor = (bytes, prefix, extension) => ({
    path: `${prefix}.${hash(bytes).slice(0, 24)}.${extension}`,
    bytes: bytes.length, sha256: hash(bytes),
  });
  const manifest = {
    schema_version: 1,
    build: await createClearraWasmBuildContract(options.repositoryRoot, environment),
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

test('Chromium gate executes the existing current-run bytes and rejects a prior attempt or substituted file', async (t) => {
  const options = await fixture(t);
  const owner = { purpose: 'product', cargoTarget: await realpath(options.root) };
  options.sourceDirectory = resolve(owner.cargoTarget, 'clearra-web-public', 'wasm');
  await mkdir(options.sourceDirectory, { recursive: true });
  const source = 'a'.repeat(40);
  const environment = {
    GITHUB_ACTIONS: 'true', GITHUB_SHA: source, GITHUB_REF: 'refs/heads/main',
    GITHUB_REPOSITORY: 'owner/project', GITHUB_EVENT_NAME: 'workflow_dispatch',
    GITHUB_WORKFLOW_REF: 'owner/project/.github/workflows/release-cli.yml@refs/heads/main',
    GITHUB_RUN_ID: '123', GITHUB_RUN_ATTEMPT: '2',
    CLEARRA_SOURCE_COMMIT: source, CLEARRA_ENGINE_BUILD_ID: source,
    CLEARRA_ACCEPTED_WASM_DIR: resolve(options.root, 'download'),
    CLEARRA_ACCEPTED_RUN_ID: '123', CLEARRA_ACCEPTED_RUN_ATTEMPT: '2',
  };
  const manifest = await generation(options, 1, environment);
  const receipt = await sealAcceptedWasmBuild(options.sourceDirectory, source, '123', '2', {
    cargo: 'cargo 1.91.0', cmake: 'cmake version 3.31.0', node: 'v22.18.0',
    npm: '10.9.3', powershell: '7.5.1', rust: 'rustc 1.91.0',
    wasm_bindgen: 'wasm-bindgen 0.2.126',
  });
  const read = (changes = {}, purpose = 'product') => readChromiumGateWasmInput({
    owner: { ...owner, purpose }, repositoryRoot: options.repositoryRoot,
    environment: { ...environment, ...changes },
  });
  const artifact = await read();
  assert.equal(artifact.artifactInput, 'canonical-wasm-producer');
  assert.equal(artifact.acceptedPayloadSha256, receipt.payload_sha256);
  assert.equal(hash(artifact.files.get(manifest.wasm.path)), manifest.wasm.sha256);
  await assert.rejects(read({ GITHUB_RUN_ATTEMPT: '3' }), /'2' !== '3'/u);
  await assert.rejects(read({ GITHUB_RUN_ATTEMPT: '3', CLEARRA_ACCEPTED_RUN_ATTEMPT: '3' }), /run attempt mismatch/u);
  await assert.rejects(read({ GITHUB_SHA: 'b'.repeat(40) }), /Wrong Actions checkout source/u);
  await assert.rejects(read({
    GITHUB_WORKFLOW_REF: 'owner/project/.github/workflows/candidate-preflight.yml@refs/heads/main',
  }), /release-cli\.yml/u);
  await assert.rejects(read({
    GITHUB_WORKFLOW_REF: 'owner/project/.github/workflows/candidate-preflight.yml@refs/heads/main',
    CLEARRA_ACCEPTED_WASM_DIR: '', CLEARRA_ACCEPTED_RUN_ID: '', CLEARRA_ACCEPTED_RUN_ATTEMPT: '',
  }, 'experiment'), /found a canonical receipt/u);
  await writeFile(resolve(options.sourceDirectory, manifest.wasm.path), Buffer.from([0, 1, 2]));
  await assert.rejects(read(), /closed regular-file set/u);
  assert.equal(hash(artifact.files.get(manifest.wasm.path)), manifest.wasm.sha256,
    'the browser receives captured bytes rather than re-reading the mutated file');
});

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
  const incoming = await generation(options, 2);
  const staging = await acquireManagedTransientDirectory(resolve(await realpath(resolve(options.root, 'public')), '.clearra-wasm-stage'));
  const lockPath = `${staging.path}.lock`;
  const recordPath = `${staging.path}.owner.json`;
  try {
    const lock = await readFile(lockPath);
    const record = await readFile(recordPath);
    await assert.rejects(importVerifiedClearraWasmDirectory(options), /already active/u);
    assert.deepEqual(await readFile(resolve(options.destinationDirectory, MANIFEST)), prior);
    assert.deepEqual(await readFile(lockPath), lock);
    assert.deepEqual(await readFile(recordPath), record);
  } finally { await staging.release(); }
  const imported = await importVerifiedClearraWasmDirectory(options);
  assert.equal(imported.generation, incoming.wasm.sha256);
});

for (const [kind, lease] of [['PID-only', JSON.stringify({ pid: process.pid })], ['malformed', 'not-json']]) {
  test(`an unresolved ${kind} producer lease preserves the current manifest and lock`, async (t) => {
    const options = await fixture(t);
    await generation(options);
    await importVerifiedClearraWasmDirectory(options);
    const prior = await readFile(resolve(options.destinationDirectory, MANIFEST));
    await generation(options, 2);
    const lockPath = resolve(options.root, 'public', '.clearra-wasm-stage.lock');
    await writeFile(lockPath, lease);
    await assert.rejects(importVerifiedClearraWasmDirectory(options), /unresolved or invalid lease/u);
    assert.deepEqual(await readFile(resolve(options.destinationDirectory, MANIFEST)), prior);
    assert.equal(await readFile(lockPath, 'utf8'), lease);
  });
}

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
