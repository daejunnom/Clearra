import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, readFile, readdir, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  clearraWasmBuildContractsEqual,
  createClearraWasmBuildContract,
} from './clearra-wasm-build-contract.mjs';
import { acquireManagedTransientDirectory } from './managed-transient-directory.mjs';
import { finesseSourceSnapshot } from '../benchmark/finesse-source-snapshot.mjs';

const scriptDir = fileURLToPath(new URL('.', import.meta.url));
const scriptRoot = resolve(scriptDir, '..', '..');
const MANIFEST_BYTES = 768;
const GENERATION_HEX_LENGTH = 24;
const BENCHMARK_PROVENANCE_FILE = 'clearra-finesse-wasm-build-provenance.json';
const PERFORMANCE_RUST_ENV_KEYS = [
  'RUSTFLAGS',
  'CARGO_ENCODED_RUSTFLAGS',
  'RUSTC_WRAPPER',
  'RUSTC_WORKSPACE_WRAPPER',
  'CARGO_BUILD_RUSTFLAGS',
  'CARGO_PROFILE_RELEASE_CODEGEN_UNITS',
  'CARGO_PROFILE_RELEASE_LTO',
  'CARGO_PROFILE_RELEASE_OPT_LEVEL',
  'CARGO_PROFILE_RELEASE_DEBUG',
  'CARGO_PROFILE_RELEASE_INCREMENTAL',
  'CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS',
  'CARGO_PROFILE_RELEASE_PANIC',
  'CARGO_PROFILE_RELEASE_STRIP',
];
const options = parseArguments(process.argv.slice(2));
const root = options.sourceRoot ? resolve(options.sourceRoot) : scriptRoot;
const destinationDir = options.destination
  ? resolve(options.destination)
  : resolve(root, 'apps', 'clearra-web', 'static', 'wasm');

await mkdir(dirname(destinationDir), { recursive: true });
await mkdir(destinationDir, { recursive: true });
const stagingLease = await acquireManagedTransientDirectory(
  resolve(dirname(destinationDir), '.clearra-wasm-stage')
);
const stagingDir = stagingLease.path;
const wasmBuildContract = await createClearraWasmBuildContract(root);
const benchmarkSourceSnapshot = options.benchmarkProvenance
  ? finesseSourceSnapshot(root)
  : null;
const benchmarkProducer = options.benchmarkProvenance
  ? await benchmarkProducerIdentity()
  : null;
const benchmarkToolchain = options.benchmarkProvenance
  ? await benchmarkToolchainIdentity()
  : null;
try {
  if (options.environment === 'wsl') {
    if (process.platform !== 'win32') {
      throw new Error('--environment wsl is available only from a Windows host');
    }
    await buildWithWsl();
  } else {
    await buildNative();
  }
  const manifest = await writeManifest(stagingDir, wasmBuildContract);
  const finalWasmBuildContract = await createClearraWasmBuildContract(root);
  if (!clearraWasmBuildContractsEqual(wasmBuildContract, finalWasmBuildContract)) {
    throw new Error('Clearra WASM build sources changed while the artifact was being built');
  }
  if (options.benchmarkProvenance) {
    const finalSnapshot = finesseSourceSnapshot(root);
    if (
      benchmarkSourceSnapshot.digest !== finalSnapshot.digest ||
      benchmarkSourceSnapshot.files.length !== finalSnapshot.files.length
    ) {
      throw new Error('benchmark source changed while the WASM artifact was being built');
    }
    const finalProducer = await benchmarkProducerIdentity();
    if (
      benchmarkProducer.producer_sha256 !== finalProducer.producer_sha256 ||
      benchmarkProducer.snapshot_tool_sha256 !== finalProducer.snapshot_tool_sha256
    ) {
      throw new Error('benchmark producer changed while the WASM artifact was being built');
    }
    const finalToolchain = await benchmarkToolchainIdentity();
    if (stableJson(benchmarkToolchain) !== stableJson(finalToolchain)) {
      throw new Error('benchmark toolchain changed while the WASM artifact was being built');
    }
    await writeBenchmarkProvenance(
      stagingDir,
      manifest,
      benchmarkSourceSnapshot,
      benchmarkToolchain,
      benchmarkProducer
    );
  }
  await publishArtifacts(manifest);
  console.log(
    `staged_wasm=${resolve(destinationDir, manifest.wasm.path)} bytes=${manifest.wasm.bytes} wasm_sha256=${manifest.wasm.sha256} bindings=${resolve(destinationDir, manifest.bindings.path)} bindings_bytes=${manifest.bindings.bytes} manifest=${resolve(destinationDir, 'clearra_wasm.manifest.json')}`
  );
} finally {
  await stagingLease.release();
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
  let benchmarkProvenance = false;
  let sourceRoot = null;
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
    if (argument === '--benchmark-provenance') {
      benchmarkProvenance = true;
      continue;
    }
    if (argument === '--source-root') {
      sourceRoot = args[index + 1];
      if (!sourceRoot) throw new Error('--source-root requires a path');
      index += 1;
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
  return {
    destination,
    environment,
    verify,
    stageProfiling,
    benchmarkProvenance,
    sourceRoot,
  };
}

async function writeManifest(outputDir, buildContract) {
  const bindingsPath = resolve(outputDir, 'clearra_wasm.js');
  const wasmPath = resolve(outputDir, 'clearra_wasm_bg.wasm');
  const [bindings, wasm] = await Promise.all([
    readFile(bindingsPath),
    readFile(wasmPath)
  ]);
  const bindingsArtifact = versionedArtifact('clearra_wasm', '.js', bindings);
  const wasmArtifact = versionedArtifact('clearra_wasm_bg', '.wasm', wasm);
  const manifest = {
    schema_version: 1,
    build: buildContract,
    bindings: bindingsArtifact,
    wasm: wasmArtifact
  };
  await Promise.all([
    writeFile(resolve(outputDir, bindingsArtifact.path), bindings),
    writeFile(resolve(outputDir, wasmArtifact.path), wasm)
  ]);
  const manifestPath = resolve(outputDir, 'clearra_wasm.manifest.json');
  await mkdir(dirname(manifestPath), { recursive: true });
  await writeFile(manifestPath, serializeManifest(manifest), 'utf8');
  return manifest;
}

async function publishArtifacts(manifest) {
  for (const name of [
    manifest.bindings.path,
    manifest.wasm.path,
    'clearra_wasm.js',
    'clearra_wasm_bg.wasm'
  ]) {
    await replaceFileAtomically(resolve(stagingDir, name), resolve(destinationDir, name));
  }
  await replaceFileAtomically(
    resolve(stagingDir, 'clearra_wasm.manifest.json'),
    resolve(destinationDir, 'clearra_wasm.manifest.json')
  );
  if (options.benchmarkProvenance) {
    await replaceFileAtomically(
      resolve(stagingDir, BENCHMARK_PROVENANCE_FILE),
      resolve(destinationDir, BENCHMARK_PROVENANCE_FILE)
    );
  } else {
    await rm(resolve(destinationDir, BENCHMARK_PROVENANCE_FILE), { force: true });
  }
  await removeStaleVersionedArtifacts(manifest);
}

async function writeBenchmarkProvenance(outputDir, manifest, snapshot, toolchain, producer) {
  const provenance = {
    schema_version: 1,
    source_snapshot_sha256: snapshot.digest,
    source_file_count: snapshot.files.length,
    wasm_sha256: manifest.wasm.sha256,
    bindings_sha256: manifest.bindings.sha256,
    ...producer,
    toolchain,
    build_options: {
      environment: options.environment,
      stage_profiling: options.stageProfiling,
    },
  };
  await writeFile(
    resolve(outputDir, BENCHMARK_PROVENANCE_FILE),
    `${JSON.stringify(provenance, null, 2)}\n`,
    'utf8'
  );
}

async function benchmarkProducerIdentity() {
  const [producer, snapshotTool] = await Promise.all([
    readFile(fileURLToPath(import.meta.url)),
    readFile(resolve(scriptDir, '..', 'benchmark', 'finesse-source-snapshot.mjs')),
  ]);
  return {
    producer_sha256: createHash('sha256').update(producer).digest('hex'),
    snapshot_tool_sha256: createHash('sha256').update(snapshotTool).digest('hex'),
  };
}

async function benchmarkToolchainIdentity() {
  await assertDefaultRustBuildEnvironment();
  if (options.environment === 'wsl') {
    const distribution = process.env.CLEARRA_WSL_DISTRIBUTION || 'Ubuntu';
    return {
      environment: 'wsl',
      distribution,
      rustc: await capture('wsl.exe', ['-d', distribution, '--', 'bash', '-lc', 'rustc -Vv']),
      cargo: await capture('wsl.exe', ['-d', distribution, '--', 'bash', '-lc', 'cargo -V']),
      wasm_bindgen: await capture(
        'wsl.exe',
        ['-d', distribution, '--', 'bash', '-lc', 'wasm-bindgen --version']
      ),
      rust_build_environment: 'default',
    };
  }
  return {
    environment: 'native',
    rustc: await capture('rustc', ['-Vv']),
    cargo: await capture('cargo', ['-V']),
    wasm_bindgen: await capture(process.env.WASM_BINDGEN || 'wasm-bindgen', ['--version']),
    rust_build_environment: 'default',
  };
}

async function assertDefaultRustBuildEnvironment() {
  if (options.environment === 'wsl') {
    const distribution = process.env.CLEARRA_WSL_DISTRIBUTION || 'Ubuntu';
    const keys = PERFORMANCE_RUST_ENV_KEYS.join(' ');
    const result = await capture('wsl.exe', [
      '-d',
      distribution,
      '--',
      'bash',
      '-lc',
      `for key in ${keys}; do if [ -n "\${!key}" ]; then printf '%s\\n' "$key"; fi; done; printf '%s\\n' checked`,
    ]);
    const configured = result.split(/\r?\n/).filter((entry) => entry !== 'checked');
    if (configured.length > 0) {
      throw new Error(
        `benchmark Rust build environment must be default; unset ${configured.join(', ')}`
      );
    }
    return;
  }
  const configured = PERFORMANCE_RUST_ENV_KEYS.filter(
    (key) => String(process.env[key] ?? '').length > 0
  );
  if (configured.length > 0) {
    throw new Error(
      `benchmark Rust build environment must be default; unset ${configured.join(', ')}`
    );
  }
}

function stableJson(value) {
  if (Array.isArray(value)) return `[${value.map(stableJson).join(',')}]`;
  if (value && typeof value === 'object') {
    return `{${Object.keys(value).sort().map((key) =>
      `${JSON.stringify(key)}:${stableJson(value[key])}`
    ).join(',')}}`;
  }
  return JSON.stringify(value);
}

async function removeStaleVersionedArtifacts(manifest) {
  const retained = new Set([manifest.bindings.path, manifest.wasm.path]);
  for (const name of await readdir(destinationDir)) {
    if (retained.has(name) || !isVersionedArtifactName(name)) continue;
    await rm(resolve(destinationDir, name), { force: true });
  }
}

function isVersionedArtifactName(name) {
  return (
    /^clearra_wasm\.[0-9a-f]{20,64}\.js$/.test(name) ||
    /^clearra_wasm_bg\.[0-9a-f]{20,64}\.wasm$/.test(name)
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

function versionedArtifact(prefix, suffix, bytes) {
  const sha256 = createHash('sha256').update(bytes).digest('hex');
  return {
    path: `${prefix}.${sha256.slice(0, GENERATION_HEX_LENGTH)}${suffix}`,
    bytes: bytes.byteLength,
    sha256
  };
}

function serializeManifest(manifest) {
  const json = JSON.stringify(manifest);
  const byteLength = Buffer.byteLength(json, 'utf8') + 1;
  if (byteLength > MANIFEST_BYTES) {
    throw new Error(
      `Clearra WASM manifest exceeds the fixed ${MANIFEST_BYTES}-byte deployment contract`
    );
  }
  return `${json}${' '.repeat(MANIFEST_BYTES - byteLength)}\n`;
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

function capture(command, args) {
  return new Promise((resolveCapture, rejectCapture) => {
    let stdout = '';
    let stderr = '';
    const child = spawn(command, args, {
      stdio: ['ignore', 'pipe', 'pipe'],
      shell: false,
      env: process.env,
      windowsHide: true,
    });
    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', (chunk) => { stdout += chunk; });
    child.stderr.on('data', (chunk) => { stderr += chunk; });
    child.once('error', (error) => {
      rejectCapture(new Error(`failed to inspect ${command}: ${error.message}`));
    });
    child.once('exit', (code, signal) => {
      if (code === 0 && stdout.trim().length > 0) resolveCapture(stdout.trim());
      else {
        rejectCapture(new Error(
          `${command} version check failed with code=${code} signal=${signal ?? 'none'}: ${stderr.trim()}`
        ));
      }
    });
  });
}
