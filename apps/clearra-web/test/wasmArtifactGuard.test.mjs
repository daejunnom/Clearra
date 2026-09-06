import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import test from 'node:test';

import { build } from 'esbuild';

import { createClearraWasmBuildContract } from '../../../scripts/tools/clearra-wasm-build-contract.mjs';

test('Vite accepts and broadcasts only one fully verified WASM artifact generation', async () => {
  const temporaryRoot = await mkdtemp(join(tmpdir(), 'clearra-wasm-guard-'));
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

    const initialBuild = await createClearraWasmBuildContract(repositoryRoot);
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
    await plugin.configResolved({ root: applicationRoot });
    plugin.configureServer(server);
    assert.deepEqual(watcher.added, [resolve(manifestPath)]);

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
    const nextBuild = await createClearraWasmBuildContract(repositoryRoot);
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
    watcher.emit('change', manifestPath);
    await waitForWatcher();
    assert.equal(webSocket.broadcasts.length, 1, 'server cleanup removes manifest observers');
    assert.equal(restartCount, 0);
  } finally {
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

  add(path) {
    this.added.push(path);
    return this;
  }
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
  await writeFile(path, `${JSON.stringify(manifest)}\n`);
}

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

async function waitForWatcher() {
  await new Promise((resolvePromise) => setTimeout(resolvePromise, 90));
}
