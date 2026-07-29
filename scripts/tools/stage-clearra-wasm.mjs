import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, rename, rm, stat, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = fileURLToPath(new URL('.', import.meta.url));
const root = resolve(scriptDir, '..', '..');
const targetRoot = process.env.CARGO_TARGET_DIR
  ? resolve(process.env.CARGO_TARGET_DIR)
  : resolve(root, 'target');
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
const stagingDir = await mkdtemp(resolve(dirname(destinationDir), '.clearra-wasm-stage-'));
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
  const artifactManifest = {
    schema_version: 1,
    bindings: {
      path: 'clearra_wasm.js',
      bytes: bindingsBytes.byteLength,
      sha256: sha256(bindingsBytes)
    },
    wasm: {
      path: 'clearra_wasm_bg.wasm',
      bytes: wasmBytes.byteLength,
      sha256: sha256(wasmBytes)
    }
  };
  await writeFile(manifest, `${JSON.stringify(artifactManifest, null, 2)}\n`, 'utf8');
  for (const name of ['clearra_wasm.js', 'clearra_wasm_bg.wasm']) {
    await replaceFileAtomically(resolve(stagingDir, name), resolve(destinationDir, name));
  }
  await replaceFileAtomically(
    manifest,
    resolve(destinationDir, 'clearra_wasm.manifest.json')
  );
  console.log(
    `staged_wasm=${resolve(destinationDir, artifactManifest.wasm.path)} bytes=${destinationStat.size} wasm_sha256=${artifactManifest.wasm.sha256} bindings=${resolve(destinationDir, artifactManifest.bindings.path)} bindings_bytes=${bindingsStat.size} manifest=${resolve(destinationDir, 'clearra_wasm.manifest.json')}`
  );
} finally {
  await rm(stagingDir, { recursive: true, force: true });
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
