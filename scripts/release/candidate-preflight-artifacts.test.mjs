import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import test from 'node:test';
import { createClearraWasmBuildContract, serializeClearraWasmManifest } from '../tools/clearra-wasm-build-contract.mjs';
import { preserveCandidateWasm } from './candidate-preflight-artifacts.mjs';

const SOURCE = '7'.repeat(40);
const MANIFEST = 'clearra_wasm.manifest.json';
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');

async function fixture(t, built = true) {
  const root = await mkdtemp(resolve(tmpdir(), 'clearra-unqualified-ci-'));
  const previous = [process.env.CLEARRA_SOURCE_COMMIT, process.env.CLEARRA_ENGINE_BUILD_ID];
  process.env.CLEARRA_SOURCE_COMMIT = SOURCE;
  process.env.CLEARRA_ENGINE_BUILD_ID = SOURCE;
  t.after(async () => {
    for (const [index, name] of ['CLEARRA_SOURCE_COMMIT', 'CLEARRA_ENGINE_BUILD_ID'].entries()) {
      if (previous[index] === undefined) delete process.env[name];
      else process.env[name] = previous[index];
    }
    await rm(root, { recursive: true, force: true });
  });
  const options = {
    sourceDirectory: resolve(root, 'producer'), outputDirectory: resolve(root, 'preserved'),
    repositoryRoot: resolve(root, 'repository'), sourceCommit: SOURCE,
  };
  await mkdir(options.sourceDirectory);
  await mkdir(options.repositoryRoot);
  await writeFile(resolve(options.repositoryRoot, 'Cargo.toml'), '[workspace]\n');
  await writeFile(resolve(options.repositoryRoot, 'Cargo.lock'), 'version = 4\n');
  if (built) {
    const bindings = Buffer.from('export const candidate = true;\n');
    const wasm = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]);
    const descriptor = (bytes, prefix, extension) => ({ path: `${prefix}.${hash(bytes).slice(0, 24)}.${extension}`, bytes: bytes.length, sha256: hash(bytes) });
    const manifest = {
      schema_version: 1, build: await createClearraWasmBuildContract(options.repositoryRoot),
      bindings: descriptor(bindings, 'clearra_wasm', 'js'), wasm: descriptor(wasm, 'clearra_wasm_bg', 'wasm'),
    };
    for (const [name, bytes] of [
      [manifest.bindings.path, bindings], ['clearra_wasm.js', bindings],
      [manifest.wasm.path, wasm], ['clearra_wasm_bg.wasm', wasm],
      [MANIFEST, serializeClearraWasmManifest(manifest)],
    ]) await writeFile(resolve(options.sourceDirectory, name), bytes);
  }
  return options;
}

test('preserves only the original five verified files without release authority', async (t) => {
  const options = await fixture(t);
  await writeFile(resolve(options.sourceDirectory, 'unrelated-note.txt'), 'not an input');
  assert.deepEqual(await preserveCandidateWasm(options), { ready: true, copiedFiles: 5 });
  const files = await readdir(options.outputDirectory);
  assert.equal(files.length, 5);
  assert.ok(!files.some((name) => /accepted|receipt|unrelated/u.test(name)));
  for (const name of files) assert.deepEqual(await readFile(resolve(options.sourceDirectory, name)), await readFile(resolve(options.outputDirectory, name)));
});

test('an earlier gate failure with no WASM emits not-built, not verified', async (t) => {
  const options = await fixture(t, false);
  assert.deepEqual(await preserveCandidateWasm(options), { ready: false, reason: 'wasm-not-built' });
  await assert.rejects(readdir(options.outputDirectory), { code: 'ENOENT' });
});

for (const kind of ['alias', 'source', 'commit', 'existing', 'overlap', 'malformed']) {
  test(`rejects ${kind} without publishing a ready output`, async (t) => {
    const options = await fixture(t);
    if (kind === 'alias') await writeFile(resolve(options.sourceDirectory, 'clearra_wasm_bg.wasm'), Buffer.alloc(8));
    if (kind === 'source') await writeFile(resolve(options.repositoryRoot, 'Cargo.toml'), '[workspace]\nresolver="3"\n');
    if (kind === 'commit') options.sourceCommit = '8'.repeat(40);
    if (kind === 'existing') {
      await mkdir(options.outputDirectory);
      await writeFile(resolve(options.outputDirectory, 'sentinel'), 'preserved');
    }
    if (kind === 'overlap') options.outputDirectory = resolve(options.repositoryRoot, 'artifact');
    if (kind === 'malformed') await writeFile(resolve(options.sourceDirectory, MANIFEST), '{}');
    await assert.rejects(preserveCandidateWasm(options));
    if (kind === 'existing') assert.equal(await readFile(resolve(options.outputDirectory, 'sentinel'), 'utf8'), 'preserved');
    else await assert.rejects(readdir(options.outputDirectory), { code: 'ENOENT' });
  });
}
