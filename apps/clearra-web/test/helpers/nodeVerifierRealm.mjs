// Functional transport adapter only. Search, asset admission, delegation and
// cancellation remain in the production Web worker and its actual WASM.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parentPort, workerData } from 'node:worker_threads';

export function installFileArtifactSurface(wasmRoot, artifacts) {
  const allowed = new Set(['clearra_wasm.manifest.json', ...artifacts]);
  const deployment = pathToFileURL(`${dirname(wasmRoot)}/`).pathname.replace(/\/$/u, '');
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: {} });
  Object.assign(globalThis, {
    self: globalThis,
    location: { origin: 'file://', pathname: `${deployment}/_app/immutable/workers/functional.js` },
    fetch: async (input, options) => {
      if (options?.signal?.aborted) throw new DOMException('cancelled', 'AbortError');
      const url = new URL(input instanceof Request ? input.url : String(input));
      assert.equal(url.protocol, 'file:', 'the functional adapter must not make network requests');
      url.search = '';
      const path = fileURLToPath(url);
      assert.equal(dirname(path), resolve(wasmRoot));
      assert.ok(allowed.has(path.slice(dirname(path).length + 1)));
      const bytes = await readFile(path);
      if (options?.signal?.aborted) throw new DOMException('cancelled', 'AbortError');
      return new Response(bytes, { headers: { 'Content-Type': path.endsWith('.wasm')
        ? 'application/wasm' : path.endsWith('.json') ? 'application/json' : 'text/javascript' } });
    }
  });
}

if (parentPort) {
  installFileArtifactSurface(workerData.wasmRoot, workerData.artifacts);
  globalThis.postMessage = (message, transfer = []) => parentPort.postMessage(message, transfer);
  globalThis.close = () => parentPort.close();
  await import('../../src/workers/clearraVerifierWorker.ts');
  parentPort.on('message', data => globalThis.onmessage({ data }));
}
