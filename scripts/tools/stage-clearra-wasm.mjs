import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, readFile, rename, rm, stat, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  createClearraWasmBuildContract,
  serializeClearraWasmManifest,
} from './clearra-wasm-build-contract.mjs';
import {
  CLEARRA_WASM_GENERATION_HISTORY_FILE,
  captureClearraWasmGenerationRetention,
  retainPublishedClearraWasmGenerations,
} from './clearra-wasm-generation-retention.mjs';
import { acquireManagedTransientDirectory } from './managed-transient-directory.mjs';

const scriptDir = fileURLToPath(new URL('.', import.meta.url));
const root = resolve(scriptDir, '..', '..');
const GENERATION_HEX_LENGTH = 24;
const cacheBase = process.platform === 'win32'
  ? process.env.LOCALAPPDATA || process.env.TEMP || resolve(process.env.USERPROFILE || '.', 'AppData', 'Local')
  : process.env.XDG_CACHE_HOME || resolve(process.env.HOME || '.', '.cache');
const targetRoot = process.env.CARGO_TARGET_DIR
  ? resolve(process.env.CARGO_TARGET_DIR)
  : resolve(cacheBase, 'Clearra', 'build', 'cargo-target-wasm');
const source = resolve(targetRoot, 'wasm32-unknown-unknown', 'release', 'clearra_wasm.wasm');
const destinationDir = process.argv[2]
  ? resolve(process.argv[2])
  : resolve(root, 'apps', 'clearra-web', 'static', 'wasm');

const sourceStat = await stat(source);
if (!sourceStat.isFile() || sourceStat.size === 0) {
  throw new Error(`Clearra WASM artifact is missing or empty: ${source}`);
}
await mkdir(dirname(destinationDir), { recursive: true });
await mkdir(destinationDir, { recursive: true });
await rm(resolve(destinationDir, 'clearra_wasm.wasm'), { force: true });
const stagingLease = await acquireManagedTransientDirectory(
  resolve(dirname(destinationDir), '.clearra-wasm-stage')
);
const stagingDir = stagingLease.path;
const bindings = resolve(stagingDir, 'clearra_wasm.js');
const destination = resolve(stagingDir, 'clearra_wasm_bg.wasm');
const manifest = resolve(stagingDir, 'clearra_wasm.manifest.json');

try {
  const wasmBindgen = process.env.WASM_BINDGEN || 'wasm-bindgen';
  await run(wasmBindgen, [
    source,
    '--target',
    'web',
    '--out-dir',
    stagingDir,
    '--out-name',
    'clearra_wasm',
    '--no-typescript'
  ]);

  const bindingsStat = await stat(bindings);
  const destinationStat = await stat(destination);
  if (!bindingsStat.isFile() || bindingsStat.size === 0) {
    throw new Error(`Clearra WASM bindings are missing or empty: ${bindings}`);
  }
  if (!destinationStat.isFile() || destinationStat.size === 0) {
    throw new Error(`Clearra bound WASM artifact is missing or empty: ${destination}`);
  }
  const [bindingsBytes, wasmBytes] = await Promise.all([
    readFile(bindings),
    readFile(destination)
  ]);
  const bindingsSha256 = sha256(bindingsBytes);
  const wasmSha256 = sha256(wasmBytes);
  const artifactManifest = {
    schema_version: 1,
    build: await createClearraWasmBuildContract(root),
    bindings: {
      path: `clearra_wasm.${bindingsSha256.slice(0, GENERATION_HEX_LENGTH)}.js`,
      bytes: bindingsBytes.byteLength,
      sha256: bindingsSha256
    },
    wasm: {
      path: `clearra_wasm_bg.${wasmSha256.slice(0, GENERATION_HEX_LENGTH)}.wasm`,
      bytes: wasmBytes.byteLength,
      sha256: wasmSha256
    }
  };
  await Promise.all([
    writeFile(resolve(stagingDir, artifactManifest.bindings.path), bindingsBytes),
    writeFile(resolve(stagingDir, artifactManifest.wasm.path), wasmBytes)
  ]);
  await writeFile(manifest, serializeClearraWasmManifest(artifactManifest), 'utf8');
  const retentionSnapshot = await captureClearraWasmGenerationRetention(destinationDir);
  for (const name of [
    artifactManifest.bindings.path,
    artifactManifest.wasm.path,
    'clearra_wasm.js',
    'clearra_wasm_bg.wasm'
  ]) {
    await replaceFileAtomically(resolve(stagingDir, name), resolve(destinationDir, name));
  }
  await replaceFileAtomically(
    manifest,
    resolve(destinationDir, 'clearra_wasm.manifest.json')
  );
  const retention = await retainPublishedClearraWasmGenerations({
    destinationDir,
    currentManifest: artifactManifest,
    snapshot: retentionSnapshot,
    publishHistory: async (serializedHistory) => {
      const stagedHistory = resolve(stagingDir, CLEARRA_WASM_GENERATION_HISTORY_FILE);
      await writeFile(stagedHistory, serializedHistory, 'utf8');
      await replaceFileAtomically(
        stagedHistory,
        resolve(destinationDir, CLEARRA_WASM_GENERATION_HISTORY_FILE)
      );
    },
  });
  if (retention.status === 'skipped') {
    console.warn(`wasm_generation_cleanup=skipped reason=${retention.reason}`);
  } else {
    console.log(
      `wasm_generation_cleanup=retained generations=${retention.retainedGenerationCount} deleted_files=${retention.deleted.length}`
    );
  }
  console.log(
    `staged_wasm=${resolve(destinationDir, artifactManifest.wasm.path)} bytes=${destinationStat.size} wasm_sha256=${artifactManifest.wasm.sha256} bindings=${resolve(destinationDir, artifactManifest.bindings.path)} bindings_bytes=${bindingsStat.size} manifest=${resolve(destinationDir, 'clearra_wasm.manifest.json')}`
  );
} finally {
  await stagingLease.release();
}

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

async function replaceFileAtomically(sourcePath, destinationPath) {
  for (let attempt = 0; ; attempt += 1) {
    try {
      await rename(sourcePath, destinationPath);
      return;
    } catch (error) {
      if (
        attempt >= 9 ||
        !['EACCES', 'EBUSY', 'EEXIST', 'EPERM'].includes(error?.code)
      ) {
        throw error;
      }
      await new Promise((resolveRetry) => setTimeout(resolveRetry, 20 * (attempt + 1)));
    }
  }
}

function run(command, args) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(command, args, { stdio: 'inherit', shell: false });
    child.once('error', (error) => {
      rejectRun(
        new Error(
          `failed to start ${command}; install the source-built wasm-bindgen CLI matching Cargo.lock: ${error.message}`
        )
      );
    });
    child.once('exit', (code, signal) => {
      if (code === 0) {
        resolveRun();
        return;
      }
      rejectRun(new Error(`${command} failed with code=${code} signal=${signal ?? 'none'}`));
    });
  });
}
