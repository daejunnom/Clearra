import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, rename, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import test from 'node:test';

import { build } from 'esbuild';

import { createClearraWasmBuildContract } from '../../../scripts/tools/clearra-wasm-build-contract.mjs';

test('Vite accepts and broadcasts only one fully verified WASM artifact generation', async () => {
  const temporaryRoot = await mkdtemp(join(tmpdir(), 'clearra-wasm-guard-'));
  let stopServer = () => {};
  try {
    const guard = await loadProductionGuard(temporaryRoot);
    const repositoryRoot = join(temporaryRoot, 'repository');
    const applicationRoot = join(repositoryRoot, 'apps', 'clearra-web');
    const wasmRoot = join(applicationRoot, 'static', 'wasm');
    const sourcePath = join(repositoryRoot, 'crates', 'fixture', 'src', 'lib.rs');
    const manifestPath = join(wasmRoot, 'clearra_wasm.manifest.json');
    await mkdir(join(repositoryRoot, 'crates', 'fixture', 'src'), { recursive: true });
    await mkdir(wasmRoot, { recursive: true });
    await Promise.all([
      writeFile(join(repositoryRoot, 'Cargo.toml'), '[workspace]\nmembers = []\n'),
      writeFile(join(repositoryRoot, 'Cargo.lock'), '# synthetic contract fixture\n'),
      writeFile(sourcePath, 'pub fn generation() -> u8 { 1 }\n')
    ]);

    const initialBuild = await createClearraWasmBuildContract(repositoryRoot, {});
    const initialManifest = await publishGeneration(
      wasmRoot,
      manifestPath,
      initialBuild,
      'bindings-generation-one',
      'wasm-generation-one'
    );

    const watcher = new FakeWatcher();
    const webSocket = new FakeWebSocket();
    const httpServer = new FakeHttpServer();
    stopServer = () => httpServer.emit('close');
    const infoMessages = [];
    const warningMessages = [];
    let restartCount = 0;
    let generationEndpoint;
    const server = {
      middlewares: { use(handler) { generationEndpoint = handler; } },
      watcher,
      ws: webSocket,
      httpServer,
      config: {
        logger: {
          info(message) {
            infoMessages.push(message);
          },
          warn(message) {
            warningMessages.push(message);
          }
        }
      },
      async restart() {
        restartCount += 1;
      }
    };
    const plugin = guard.wasmArtifactGuard();
    await plugin.configResolved({ root: applicationRoot, command: 'serve' });
    await plugin.configureServer(server);
    assert.deepEqual(watcher.added, [], 'no direct file watch may pin the atomic publication target');
    assert.deepEqual(watcher.unwatched, [resolve(manifestPath)], 'release any Vite inherited direct manifest handle before stat polling');

    watcher.emit('change', join(wasmRoot, 'unrelated.json'));
    await waitForWatcher();
    assert.equal(webSocket.broadcasts.length, 0, 'unrelated files never publish a generation');

    const staleSourceManifest = structuredClone(initialManifest);
    staleSourceManifest.build.source_sha256 = 'd'.repeat(64);
    await writeManifest(manifestPath, staleSourceManifest);
    watcher.emit('change', manifestPath);
    await waitForWatcher();
    assert.equal(webSocket.broadcasts.length, 0, 'a stale source fingerprint is ignored');

    const incompleteManifest = structuredClone(initialManifest);
    incompleteManifest.bindings.bytes += 1;
    await writeManifest(manifestPath, incompleteManifest);
    watcher.emit('change', manifestPath);
    await waitForWatcher();
    assert.equal(webSocket.broadcasts.length, 0, 'an incomplete artifact generation is ignored');

    const wrongHashManifest = structuredClone(initialManifest);
    wrongHashManifest.bindings.sha256 = 'f'.repeat(64);
    await writeManifest(manifestPath, wrongHashManifest);
    watcher.emit('change', manifestPath);
    await waitForWatcher();
    assert.equal(webSocket.broadcasts.length, 0, 'an artifact with a wrong SHA-256 is ignored');
    assert.equal(warningMessages.length, 3, 'each invalid manifest fails closed with one warning');

    await writeFile(sourcePath, 'pub fn generation() -> u8 { 2 }\n');
    const nextBuild = await createClearraWasmBuildContract(repositoryRoot, {});
    const nextManifest = await publishGeneration(
      wasmRoot,
      manifestPath,
      nextBuild,
      'bindings-generation-two',
      'wasm-generation-two'
    );
    watcher.emit('add', manifestPath);
    watcher.emit('change', manifestPath);
    await waitForWatcher();

    assert.deepEqual(webSocket.broadcasts, [{
      type: 'custom',
      event: guard.CLEARRA_WASM_ARTIFACT_UPDATE_EVENT,
      data: {
        sourceSha256: nextManifest.build.source_sha256,
        bindingsSha256: nextManifest.bindings.sha256,
        wasmSha256: nextManifest.wasm.sha256
      }
    }], 'a fully verified generation is debounced and broadcast exactly once');
    assert.equal(infoMessages.length, 1);
    assert.equal(restartCount, 0, 'artifact replacement never restarts the Vite server');
    const endpointHeaders = {};
    let endpointBody;
    const endpointResponse = {
      setHeader(name, value) { endpointHeaders[name] = value; },
      end(body) { endpointBody = JSON.parse(body); }
    };
    generationEndpoint({ url: guard.CLEARRA_WASM_GENERATION_ENDPOINT }, endpointResponse,
      () => assert.fail('verified endpoint should answer directly'));
    assert.equal(endpointResponse.statusCode, 200);
    assert.equal(endpointHeaders['Cache-Control'], 'no-store');
    assert.equal(endpointBody.wasmSha256, nextManifest.wasm.sha256);
    assert.equal(Object.keys(endpointBody).length, 3, 'no filesystem/runtime configuration leaks');

    watcher.emit('change', manifestPath);
    await waitForWatcher();
    assert.equal(
      webSocket.broadcasts.length,
      1,
      're-observing the accepted manifest never duplicates the hot-update broadcast'
    );

    const synchronized = [];
    webSocket.emit(
      guard.CLEARRA_WASM_ARTIFACT_SYNC_EVENT,
      undefined,
      {
        send(event, data) {
          synchronized.push([event, data]);
        }
      }
    );
    assert.deepEqual(synchronized, [[
      guard.CLEARRA_WASM_ARTIFACT_UPDATE_EVENT,
      {
        sourceSha256: nextManifest.build.source_sha256,
        bindingsSha256: nextManifest.bindings.sha256,
        wasmSha256: nextManifest.wasm.sha256
      }
    ]], 'a newly connected client receives the last verified generation');

    httpServer.emit('close');
    await writeFile(sourcePath, 'pub fn generation() -> u8 { 3 }\n');
    await publishGeneration(wasmRoot, manifestPath, await createClearraWasmBuildContract(repositoryRoot, {}), 'bindings-generation-three', 'wasm-generation-three');
    watcher.emit('change', manifestPath);
    await waitForWatcher();
    assert.equal(webSocket.broadcasts.length, 1, 'server cleanup removes manifest observers');
    assert.equal(restartCount, 0);
  } finally {
    stopServer();
    const resolvedTemporaryRoot = resolve(temporaryRoot);
    assert.ok(
      resolvedTemporaryRoot.startsWith(resolve(tmpdir())),
      'the contract removes only its own operating-system temp directory'
    );
    await rm(resolvedTemporaryRoot, { recursive: true, force: true });
  }
});

class FakeEmitter {
  #listeners = new Map();

  on(event, listener) {
    const listeners = this.#listeners.get(event) ?? new Set();
    listeners.add(listener);
    this.#listeners.set(event, listeners);
    return this;
  }

  off(event, listener) {
    this.#listeners.get(event)?.delete(listener);
    return this;
  }

  emit(event, ...arguments_) {
    for (const listener of [...(this.#listeners.get(event) ?? [])]) {
      listener(...arguments_);
    }
  }
}

class FakeWatcher extends FakeEmitter {
  added = [];
  unwatched = [];

  add(path) {
    this.added.push(path);
    return this;
  }
  async unwatch(path) { this.unwatched.push(path); }
}

class FakeWebSocket extends FakeEmitter {
  broadcasts = [];

  send(payload) {
    this.broadcasts.push(payload);
  }
}

class FakeHttpServer extends FakeEmitter {
  once(event, listener) {
    const onceListener = (...arguments_) => {
      this.off(event, onceListener);
      listener(...arguments_);
    };
    return this.on(event, onceListener);
  }
}

async function loadProductionGuard(temporaryRoot) {
  const bundle = await build({
    absWorkingDir: fileURLToPath(new URL('../../..', import.meta.url)),
    bundle: true,
    entryPoints: [fileURLToPath(new URL('../wasmArtifactGuard.ts', import.meta.url))],
    format: 'esm',
    logLevel: 'silent',
    platform: 'node',
    target: 'node22',
    write: false
  });
  assert.equal(bundle.outputFiles.length, 1);
  const bundlePath = join(temporaryRoot, 'wasm-artifact-guard.mjs');
  await writeFile(bundlePath, bundle.outputFiles[0].contents);
  return import(pathToFileURL(bundlePath).href);
}

async function publishGeneration(
  wasmRoot,
  manifestPath,
  buildContract,
  bindingsText,
  wasmText
) {
  const bindings = new TextEncoder().encode(bindingsText);
  const wasm = new TextEncoder().encode(wasmText);
  const manifest = {
    schema_version: 1,
    build: buildContract,
    bindings: {
      path: 'clearra_wasm.js',
      bytes: bindings.byteLength,
      sha256: sha256(bindings)
    },
    wasm: {
      path: 'clearra_wasm_bg.wasm',
      bytes: wasm.byteLength,
      sha256: sha256(wasm)
    }
  };
  await Promise.all([
    writeFile(join(wasmRoot, manifest.bindings.path), bindings),
    writeFile(join(wasmRoot, manifest.wasm.path), wasm)
  ]);
  await writeManifest(manifestPath, manifest);
  return manifest;
}

async function writeManifest(path, manifest) {
  // Exercise a real atomic replacement while the production stat monitor is
  // active, including Windows rename semantics (not a fake watcher event).
  const staged = `${path}.next`;
  await writeFile(staged, `${JSON.stringify(manifest)}\n`);
  await rename(staged, path);
}

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

async function waitForWatcher() {
  await new Promise((resolvePromise) => setTimeout(resolvePromise, 500));
}

test('local hot-update pins bind to current HEAD while production environment remains strict', async () => {
  const temporaryRoot = await mkdtemp(join(tmpdir(), 'clearra-wasm-guard-pins-'));
  const savedSource = process.env.CLEARRA_SOURCE_COMMIT;
  const savedEngine = process.env.CLEARRA_ENGINE_BUILD_ID;
  try {
    const guard = await loadProductionGuard(temporaryRoot);
    const root = join(temporaryRoot, 'repository');
    await mkdir(root);
    await writeFile(join(root, 'Cargo.toml'), '[workspace]\nmembers = []\n');
    await writeFile(join(root, 'Cargo.lock'), '# synthetic fixture\n');
    const previous = 'a'.repeat(40);
    const current = 'b'.repeat(40);
    process.env.CLEARRA_SOURCE_COMMIT = previous;
    process.env.CLEARRA_ENGINE_BUILD_ID = previous;
    const buildContract = await createClearraWasmBuildContract(root, { CLEARRA_SOURCE_COMMIT: current, CLEARRA_ENGINE_BUILD_ID: current });
    const manifest = { build: buildContract };
    const context = { root, repositoryRoot: root, manifestPath: join(root, 'manifest.json'), localServe: true };
    let headReads = 0;
    const expected = await guard.expectedBuildForManifest(context, manifest, async () => { headReads++; return current; });
    assert.deepEqual(expected, buildContract);
    assert.equal(headReads, 2, 'current HEAD is rechecked after reading source inputs');
    assert.equal(process.env.CLEARRA_SOURCE_COMMIT, previous, 'the server environment is not rewritten/restamped');
    await assert.rejects(guard.expectedBuildForManifest(context, manifest, async () => previous), /current Git HEAD/);
    let reads = 0;
    await assert.rejects(guard.expectedBuildForManifest(context, manifest, async () => ++reads === 1 ? current : previous), /changed during/);
    const wrongEngine = { build: { ...buildContract, runtime_identity: { ...buildContract.runtime_identity, engine_build_id: previous } } };
    await assert.rejects(guard.expectedBuildForManifest(context, wrongEngine, async () => current), /source\/engine pins/);
    const production = await guard.expectedBuildForManifest({ ...context, localServe: false }, manifest, async () => { assert.fail('production must not substitute HEAD for its environment'); });
    assert.equal(production.runtime_identity.source_commit, previous);
    assert.notDeepEqual(production, manifest.build, 'the ordinary strict contract comparison must reject stale production pins');
    const unverified = { build: await createClearraWasmBuildContract(root, {}) };
    assert.deepEqual(await guard.expectedBuildForManifest(context, unverified, async () => { assert.fail('unverified local mode does not mint a commit identity'); }), unverified.build);
    await writeFile(join(root, 'Cargo.toml'), '[workspace]\nmembers = []\n# changed source\n');
    const changed = await guard.expectedBuildForManifest(context, manifest, async () => current);
    assert.notEqual(changed.source_sha256, manifest.build.source_sha256, 'matching pins never bypass actual source fingerprint comparison');
  } finally {
    if (savedSource === undefined) delete process.env.CLEARRA_SOURCE_COMMIT; else process.env.CLEARRA_SOURCE_COMMIT = savedSource;
    if (savedEngine === undefined) delete process.env.CLEARRA_ENGINE_BUILD_ID; else process.env.CLEARRA_ENGINE_BUILD_ID = savedEngine;
    assert.ok(resolve(temporaryRoot).startsWith(resolve(tmpdir())));
    await rm(temporaryRoot, { recursive: true, force: true });
  }
});
