import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  clearraWasmBuildContractsEqual,
  createClearraWasmBuildContract,
  serializeClearraWasmManifest,
} from './clearra-wasm-build-contract.mjs';
import {
  CLEARRA_WASM_GENERATION_HISTORY_FILE,
  captureClearraWasmGenerationRetention,
  retainPublishedClearraWasmGenerations,
} from './clearra-wasm-generation-retention.mjs';
import { acquireManagedTransientDirectory } from './managed-transient-directory.mjs';
import { finesseSourceSnapshot } from '../benchmark/finesse-source-snapshot.mjs';
import { enterManagedBuildOrRelaunch } from './clearra-build-policy.mjs';

const scriptDir = fileURLToPath(new URL('.', import.meta.url));
const scriptRoot = resolve(scriptDir, '..', '..');
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
// Reject outside targets before creating either a build or publication directory.
const buildOwner = enterManagedBuildOrRelaunch(root, process.argv.slice(1),
  process.env.CLEARRA_BUILD_PURPOSE || (options.benchmarkProvenance ? 'experiment' : 'product'));
if (process.env.CLEARRA_WSL_CARGO_TARGET_DIR) {
  throw new Error('CLEARRA_WSL_CARGO_TARGET_DIR is no longer configurable; WSL shares the managed build target');
}
const destinationDir = options.destination
  ? resolve(options.destination)
  : resolve(root, 'apps', 'clearra-web', 'static', 'wasm');

await mkdir(dirname(destinationDir), { recursive: true });
await mkdir(destinationDir, { recursive: true });
const stagingLease = await acquireManagedTransientDirectory(
  resolve(buildOwner.transaction, 'wasm-stage')
);
const stagingDir = stagingLease.path;
const wasmBuildContract = await createClearraWasmBuildContract(root);
const benchmarkSourceSnapshot = options.benchmarkProvenance
  ? finesseSourceSnapshot(root)
  : null;
const benchmarkProducer = options.benchmarkProvenance
  ? await benchmarkProducerIdentity()
  : null;
let benchmarkToolchain = options.benchmarkProvenance && options.environment !== 'wsl'
  ? await benchmarkToolchainIdentity()
  : null;
try {
  if (options.environment === 'wsl') {
    if (process.platform !== 'win32') {
      throw new Error('--environment wsl is available only from a Windows host');
    }
    const wslToolchain = await buildWithWsl();
    if (options.benchmarkProvenance) benchmarkToolchain = wslToolchain;
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
    // A dedicated WSL build verifies its immutable toolchain marker and emits
    // the exact versions inside the same bounded session. Starting a second
    // distribution session merely to repeat the probe would violate the
    // one-boot-per-high-level-job contract.
    const finalToolchain = options.environment === 'wsl'
      ? benchmarkToolchain
      : await benchmarkToolchainIdentity();
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
  await assertDefaultRustBuildEnvironment();
  const args = [
    '-B',
    resolve(root, '_local', 'clearra_manage.py'),
    'runtime',
    'wsl',
    'run',
    '--entry',
    'wasm-build',
    '--',
    '--staging',
    stagingDir,
    '--source-commit',
    wasmBuildContract.runtime_identity.source_commit,
    '--engine-build-id',
    wasmBuildContract.runtime_identity.engine_build_id,
  ];
  if (options.verify) args.push('--verify');
  if (options.stageProfiling) args.push('--stage-profiling');
  await run(process.env.PYTHON || 'python', args);
  const identityPath = resolve(stagingDir, '.clearra-wsl-toolchain.json');
  const identity = JSON.parse(await readFile(identityPath, 'utf8'));
  await rm(identityPath, { force: true });
  return identity;
}

async function buildNative() {
  const targetRoot = buildOwner.cargoTarget;
  await mkdir(targetRoot, { recursive: true });
  if (options.verify) {
    await run('cargo', [
      'check', '--locked', '--manifest-path', resolve(root, 'Cargo.toml'),
      '--package', 'clearra-cli-command', '--lib', '--tests'
    ], { CARGO_TARGET_DIR: targetRoot });
    await run('cargo', [
      'check', '--locked', '--manifest-path', resolve(root, 'Cargo.toml'),
      '--package', 'clearra-wasm', '--lib', '--tests'
    ], { CARGO_TARGET_DIR: targetRoot });
    await run('cargo', [
      'test', '--locked', '--manifest-path', resolve(root, 'Cargo.toml'),
      '--package', 'clearra-wasm', '--test', 'wasm_host_contract'
    ], { CARGO_TARGET_DIR: targetRoot });
  }
  const cargoArgs = [
    'build', '--locked',
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
  await writeFile(manifestPath, serializeClearraWasmManifest(manifest), 'utf8');
  return manifest;
}

async function publishArtifacts(manifest) {
  const retentionSnapshot = await captureClearraWasmGenerationRetention(destinationDir);
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
  const retention = await retainPublishedClearraWasmGenerations({
    destinationDir,
    currentManifest: manifest,
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
    throw new Error('WSL toolchain identity is emitted by the single managed build session');
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

function run(command, args, extraEnvironment = {}) {
  return new Promise((resolveRun, rejectRun) => {
    const startedAt = Date.now();
    const child = spawn(command, args, {
      stdio: 'inherit',
      shell: false,
      env: { ...process.env, ...extraEnvironment }
    });
    child.once('error', (error) => {
      rejectRun(new Error(`failed to start ${command}: ${error.message}`));
    });
    child.once('exit', (code, signal) => {
      if (code === 0) {
        const commandName = command === 'cargo' ? `cargo-${args[0] ?? 'unknown'}` : command;
        console.log(`wasm_build_command=${commandName} duration_ms=${Date.now() - startedAt}`);
        resolveRun();
      }
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
