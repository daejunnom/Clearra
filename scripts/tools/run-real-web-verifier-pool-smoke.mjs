// Compile only the production host/worker functional consumer. This command
// never builds WASM, benchmarks, downloads packs or changes product ports.
import { build } from 'esbuild';
import assert from 'node:assert/strict';
import { mkdir, readFile, realpath } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { enterManagedBuildOrRelaunch, assertNoBuildLinks } from './clearra-build-policy.mjs';
import { createClearraWasmBuildContract, clearraWasmBuildContractsEqual }
  from './clearra-wasm-build-contract.mjs';

const self = fileURLToPath(import.meta.url);
const repository = resolve(fileURLToPath(new URL('../..', import.meta.url)));
for (const name of ['CLEARRA_REAL_ACCELERATOR_WASM_DIR', 'CLEARRA_SIGNED_CONDITIONED_SMOKE_DIR',
  'CLEARRA_SOURCE_COMMIT', 'CLEARRA_ENGINE_BUILD_ID']) {
  if (!process.env[name]) throw new Error(`Required real functional fixture: ${name}`);
}
assert.equal(await realpath(process.env.CLEARRA_REAL_ACCELERATOR_WASM_DIR),
  join(await realpath(repository), '_local', 'artifacts', 'v081-browser-peer-smoke', 'wasm'));
assert.equal(await realpath(process.env.CLEARRA_SIGNED_CONDITIONED_SMOKE_DIR),
  join(await realpath(repository), '_local', 'artifacts', 'v081-peer-signed-smoke'));
const manifest = JSON.parse(await readFile(join(process.env.CLEARRA_REAL_ACCELERATOR_WASM_DIR,
  'clearra_wasm.manifest.json'), 'utf8'));
assert.ok(clearraWasmBuildContractsEqual(manifest.build, await createClearraWasmBuildContract(repository)),
  'the current source and explicit runtime identity must match the ordinary WASM fixture');
// Reject missing/stale inputs before opening or retiring any managed build slot.
const owner = enterManagedBuildOrRelaunch(repository, [self]);
const output = join(owner.transaction, 'functional-web-verifier-pool');
assertNoBuildLinks(output);
await mkdir(output, { recursive: true });
await build({ absWorkingDir: repository, bundle: true, format: 'esm', platform: 'node',
  target: 'node22', outdir: output, entryNames: '[name]', logLevel: 'silent',
  outExtension: { '.js': '.mjs' },
  tsconfig: join(repository, 'apps/clearra-web/tsconfig.contract.json'),
  entryPoints: ['apps/clearra-web/test/realVerifierPool.smoke.mjs',
    'apps/clearra-web/test/helpers/nodeVerifierRealm.mjs'] });
process.env.CLEARRA_REAL_VERIFIER_BOOT = join(output, 'nodeVerifierRealm.mjs');
await import(pathToFileURL(join(output, 'realVerifierPool.smoke.mjs')).href);
