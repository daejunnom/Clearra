import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = fileURLToPath(new URL('.', import.meta.url));
const root = resolve(scriptDir, '..', '..');
const options = parseArguments(process.argv.slice(2));
const destinationDir = options.destination
  ? resolve(options.destination)
  : resolve(root, 'apps', 'clearra-web', 'static', 'wasm');

await mkdir(dirname(destinationDir), { recursive: true });
await mkdir(destinationDir, { recursive: true });
const stagingDir = await mkdtemp(resolve(dirname(destinationDir), '.clearra-wasm-stage-'));
try {
  if (options.environment === 'wsl') {
    if (process.platform !== 'win32') {
      throw new Error('--environment wsl is available only from a Windows host');
    }
    await buildWithWsl();
  } else {
    await buildNative();
  }
  const manifest = await writeManifest(stagingDir);
  await publishArtifacts();
  console.log(
    `staged_wasm=${resolve(destinationDir, manifest.wasm.path)} bytes=${manifest.wasm.bytes} wasm_sha256=${manifest.wasm.sha256} bindings=${resolve(destinationDir, manifest.bindings.path)} bindings_bytes=${manifest.bindings.bytes} manifest=${resolve(destinationDir, 'clearra_wasm.manifest.json')}`
  );
} finally {
  await rm(stagingDir, { recursive: true, force: true });
}

async function buildWithWsl() {
  const distribution = process.env.CLEARRA_WSL_DISTRIBUTION || 'Ubuntu';
  const cargoFeatures = options.stageProfiling ? ' --features stage-profiling' : '';
  const script = `set -euo pipefail
ROOT=$(wslpath -a ${shellQuote(root)})
DESTINATION=$(wslpath -a ${shellQuote(stagingDir)})
TARGET_ROOT="\${CLEARRA_WSL_CARGO_TARGET_DIR:-\${XDG_CACHE_HOME:-$HOME/.cache}/Clearra/build/cargo-target-wasm}"
mkdir -p "$TARGET_ROOT" "$DESTINATION"
${options.verify ? `CARGO_TARGET_DIR="$TARGET_ROOT" cargo check --manifest-path "$ROOT/Cargo.toml" --package clearra-web-command --lib --tests
CARGO_TARGET_DIR="$TARGET_ROOT" cargo check --manifest-path "$ROOT/Cargo.toml" --package clearra-wasm --lib --tests
CARGO_TARGET_DIR="$TARGET_ROOT" cargo test --manifest-path "$ROOT/Cargo.toml" --package clearra-wasm --test wasm_host_contract` : ''}
CARGO_TARGET_DIR="$TARGET_ROOT" cargo build --manifest-path "$ROOT/Cargo.toml" --target wasm32-unknown-unknown --release -p clearra-wasm-abi${cargoFeatures}
wasm-bindgen "$TARGET_ROOT/wasm32-unknown-unknown/release/clearra_wasm.wasm" --target web --out-dir "$DESTINATION" --out-name clearra_wasm --no-typescript
`;
  const encoded = Buffer.from(script, 'utf8').toString('base64');
  await run('wsl.exe', [
    '-d',
    distribution,
    '--',
    'bash',
    '-lc',
    `printf '%s' '${encoded}' | base64 -d | bash`
  ]);
}

async function buildNative() {
  const cacheBase = process.platform === 'win32'
    ? process.env.LOCALAPPDATA || process.env.TEMP || resolve(process.env.USERPROFILE || '.', 'AppData', 'Local')
    : process.env.XDG_CACHE_HOME || resolve(process.env.HOME || '.', '.cache');
  const targetRoot = process.env.CARGO_TARGET_DIR ||
    resolve(cacheBase, 'Clearra', 'build', 'cargo-target-wasm');
  await mkdir(targetRoot, { recursive: true });
  if (options.verify) {
    await run('cargo', [
      'check', '--manifest-path', resolve(root, 'Cargo.toml'),
      '--package', 'clearra-web-command', '--lib', '--tests'
    ], { CARGO_TARGET_DIR: targetRoot });
    await run('cargo', [
      'check', '--manifest-path', resolve(root, 'Cargo.toml'),
      '--package', 'clearra-wasm', '--lib', '--tests'
    ], { CARGO_TARGET_DIR: targetRoot });
    await run('cargo', [
      'test', '--manifest-path', resolve(root, 'Cargo.toml'),
      '--package', 'clearra-wasm', '--test', 'wasm_host_contract'
    ], { CARGO_TARGET_DIR: targetRoot });
  }
  const cargoArgs = [
    'build',
    '--manifest-path',
    resolve(root, 'Cargo.toml'),
    '--target',
    'wasm32-unknown-unknown',
    '--release',
    '-p',
    'clearra-wasm-abi'
  ];
  if (options.stageProfiling) cargoArgs.push('--features', 'stage-profiling');
  await run('cargo', cargoArgs, { CARGO_TARGET_DIR: targetRoot });
  await run(process.env.WASM_BINDGEN || 'wasm-bindgen', [
    resolve(targetRoot, 'wasm32-unknown-unknown', 'release', 'clearra_wasm.wasm'),
    '--target',
    'web',
    '--out-dir',
    stagingDir,
    '--out-name',
    'clearra_wasm',
    '--no-typescript'
  ]);
}

function parseArguments(args) {
  let destination = null;
  let environment = process.env.CLEARRA_WASM_BUILD_ENVIRONMENT || 'native';
  let verify = false;
  let stageProfiling = false;
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === '--verify') {
      verify = true;
      continue;
    }
    if (argument === '--stage-profiling') {
      stageProfiling = true;
      continue;
    }
    if (argument === '--destination') {
      destination = args[index + 1];
      if (!destination) throw new Error('--destination requires a path');
      index += 1;
      continue;
    }
    if (argument === '--environment') {
      environment = args[index + 1];
      if (!environment) throw new Error('--environment requires native or wsl');
      index += 1;
      continue;
    }
    if (!argument.startsWith('-') && destination === null) {
      destination = argument;
      continue;
    }
    throw new Error(`unknown build-clearra-wasm argument: ${argument}`);
  }
  if (!['native', 'wsl'].includes(environment)) {
    throw new Error(`unsupported WASM build environment: ${environment}`);
  }
  return { destination, environment, verify, stageProfiling };
}

async function writeManifest(outputDir) {
  const bindingsPath = resolve(outputDir, 'clearra_wasm.js');
  const wasmPath = resolve(outputDir, 'clearra_wasm_bg.wasm');
  const [bindings, wasm] = await Promise.all([
    readFile(bindingsPath),
    readFile(wasmPath)
  ]);
  const manifest = {
    schema_version: 1,
    bindings: artifact('clearra_wasm.js', bindings),
    wasm: artifact('clearra_wasm_bg.wasm', wasm)
  };
  const manifestPath = resolve(outputDir, 'clearra_wasm.manifest.json');
  await mkdir(dirname(manifestPath), { recursive: true });
  await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
  return manifest;
}

async function publishArtifacts() {
  for (const name of ['clearra_wasm.js', 'clearra_wasm_bg.wasm']) {
    await replaceFileAtomically(resolve(stagingDir, name), resolve(destinationDir, name));
  }
  await replaceFileAtomically(
    resolve(stagingDir, 'clearra_wasm.manifest.json'),
    resolve(destinationDir, 'clearra_wasm.manifest.json')
  );
}

async function replaceFileAtomically(source, destination) {
  for (let attempt = 0; ; attempt += 1) {
    try {
      await rename(source, destination);
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

function artifact(path, bytes) {
  return {
    path,
    bytes: bytes.byteLength,
    sha256: createHash('sha256').update(bytes).digest('hex')
  };
}

function shellQuote(value) {
  return `'${value.replaceAll("'", "'\\''")}'`;
}

function run(command, args, extraEnvironment = {}) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(command, args, {
      stdio: 'inherit',
      shell: false,
      env: { ...process.env, ...extraEnvironment }
    });
    child.once('error', (error) => {
      rejectRun(new Error(`failed to start ${command}: ${error.message}`));
    });
    child.once('exit', (code, signal) => {
      if (code === 0) resolveRun();
      else rejectRun(new Error(`${command} failed with code=${code} signal=${signal ?? 'none'}`));
    });
  });
}
