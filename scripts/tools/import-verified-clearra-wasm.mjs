import { createHash } from 'node:crypto';
import { lstat, mkdir, readFile, realpath, rename, writeFile } from 'node:fs/promises';
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import {
  CLEARRA_WASM_MANIFEST_BYTES,
  clearraWasmBuildContractsEqual,
  createClearraWasmBuildContract,
  isClearraWasmBuildContract,
} from './clearra-wasm-build-contract.mjs';
import {
  CLEARRA_WASM_GENERATION_HISTORY_FILE,
  captureClearraWasmGenerationRetention,
  retainPublishedClearraWasmGenerations,
} from './clearra-wasm-generation-retention.mjs';
import { acquireManagedTransientDirectory } from './managed-transient-directory.mjs';

const MANIFEST = 'clearra_wasm.manifest.json';
const ROOT = resolve(fileURLToPath(new URL('../..', import.meta.url)));
const MAX_ARTIFACT_BYTES = 256 * 1024 * 1024;
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

function exactKeys(value, keys, label) {
  if (!value || typeof value !== 'object' || Array.isArray(value) ||
      Object.keys(value).sort().join('|') !== [...keys].sort().join('|')) {
    throw new Error(`Invalid ${label} fields`);
  }
}

async function readRegularFile(path, expectedBytes) {
  const metadata = await lstat(path);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size !== expectedBytes) {
    throw new Error(`Missing, linked, or incorrectly sized WASM artifact: ${path}`);
  }
  const bytes = await readFile(path);
  if (bytes.length !== expectedBytes) throw new Error('WASM artifact changed while being read');
  return bytes;
}

async function requireDirectory(path) {
  const metadata = await lstat(path);
  if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
    throw new Error(`WASM publication requires a real directory: ${path}`);
  }
  return realpath(path);
}

function artifactDescriptor(value, prefix, extension) {
  exactKeys(value, ['path', 'bytes', 'sha256'], 'WASM artifact descriptor');
  if (!/^[0-9a-f]{64}$/u.test(value.sha256) ||
      value.path !== `${prefix}.${value.sha256.slice(0, 24)}.${extension}` ||
      !Number.isSafeInteger(value.bytes) || value.bytes < 1 || value.bytes > MAX_ARTIFACT_BYTES) {
    throw new Error('WASM artifact descriptor is not a bounded current-generation path');
  }
}

/** Read only the five authoritative files. No Cargo, bindgen, credentials, or
 * accepted-run authority is involved; unrelated files are neither read nor copied. */
export async function inspectVerifiedClearraWasmDirectory({
  sourceDirectory,
  repositoryRoot = ROOT,
}) {
  const source = await requireDirectory(resolve(sourceDirectory));
  const manifestBytes = await readRegularFile(resolve(source, MANIFEST), CLEARRA_WASM_MANIFEST_BYTES);
  const manifest = JSON.parse(manifestBytes.toString('utf8'));
  exactKeys(manifest, ['schema_version', 'build', 'bindings', 'wasm'], 'WASM manifest');
  if (manifest.schema_version !== 1 || !isClearraWasmBuildContract(manifest.build)) {
    throw new Error('WASM manifest has an unsupported build contract');
  }
  artifactDescriptor(manifest.bindings, 'clearra_wasm', 'js');
  artifactDescriptor(manifest.wasm, 'clearra_wasm_bg', 'wasm');
  await requireFreshBuild(manifest, repositoryRoot);
  const files = new Map();
  for (const [descriptor, alias] of [
    [manifest.bindings, 'clearra_wasm.js'],
    [manifest.wasm, 'clearra_wasm_bg.wasm'],
  ]) {
    const versioned = await readRegularFile(resolve(source, descriptor.path), descriptor.bytes);
    const unversioned = await readRegularFile(resolve(source, alias), descriptor.bytes);
    if (sha256(versioned) !== descriptor.sha256 || !versioned.equals(unversioned)) {
      throw new Error(`WASM generation or alias hash mismatch: ${descriptor.path}`);
    }
    files.set(descriptor.path, versioned);
    files.set(alias, unversioned);
  }
  files.set(MANIFEST, manifestBytes);
  return { source, manifest, files };
}

async function requireFreshBuild(manifest, repositoryRoot) {
  const expected = await createClearraWasmBuildContract(resolve(repositoryRoot));
  if (!clearraWasmBuildContractsEqual(manifest.build, expected)) {
    throw new Error('Verified WASM source/build contract differs from the current source');
  }
}

function pathsOverlap(left, right) {
  const within = (parent, child) => {
    const path = relative(parent, child);
    return path === '' || (path !== '..' && !path.startsWith(`..${sep}`) && !isAbsolute(path));
  };
  return within(left, right) || within(right, left);
}

async function replaceAtomically(source, destination) {
  for (let attempt = 0; ; attempt += 1) {
    try { await rename(source, destination); return; }
    catch (error) {
      if (attempt >= 9 || !['EACCES', 'EBUSY', 'EEXIST', 'EPERM'].includes(error?.code)) throw error;
      await new Promise((resume) => setTimeout(resume, 20 * (attempt + 1)));
    }
  }
}

/** Import already-built bytes without changing their source identity. The old
 * manifest continues to name its immutable generation until the last rename. */
export async function importVerifiedClearraWasmDirectory({
  sourceDirectory,
  destinationDirectory,
  repositoryRoot = ROOT,
}) {
  const inspected = await inspectVerifiedClearraWasmDirectory({ sourceDirectory, repositoryRoot });
  const requestedDestination = resolve(destinationDirectory);
  if (pathsOverlap(inspected.source, requestedDestination) ||
      requestedDestination === resolve(repositoryRoot)) {
    throw new Error('Source and destination publication directories must be separate');
  }
  await mkdir(requestedDestination, { recursive: true });
  const destination = await requireDirectory(requestedDestination);
  if (pathsOverlap(inspected.source, destination) || destination === await realpath(repositoryRoot)) {
    throw new Error('Resolved publication directories overlap');
  }
  // Share the producer's existing lock: an import cannot race a source build.
  const stagingPath = resolve(dirname(destination), '.clearra-wasm-stage');
  if (pathsOverlap(inspected.source, stagingPath) || pathsOverlap(destination, stagingPath)) {
    throw new Error('Source and destination cannot overlap the managed publication staging directory');
  }
  const previousStaging = await lstat(stagingPath).catch((error) => {
    if (error?.code !== 'ENOENT') throw error;
    return null;
  });
  if (previousStaging && (!previousStaging.isDirectory() || previousStaging.isSymbolicLink())) {
    throw new Error('Managed WASM staging must not be a file or symbolic link');
  }
  const staging = await acquireManagedTransientDirectory(stagingPath);
  try {
    const snapshot = await captureClearraWasmGenerationRetention(destination);
    for (const [name, bytes] of inspected.files) {
      await writeFile(resolve(staging.path, name), bytes, { flag: 'wx' });
    }
    await requireFreshBuild(inspected.manifest, repositoryRoot);
    const currentSourceManifest = await readRegularFile(resolve(inspected.source, MANIFEST), CLEARRA_WASM_MANIFEST_BYTES);
    if (!currentSourceManifest.equals(inspected.files.get(MANIFEST))) {
      throw new Error('Source WASM generation changed during import');
    }
    for (const name of inspected.files.keys()) {
      if (name !== MANIFEST) await replaceAtomically(resolve(staging.path, name), resolve(destination, name));
    }
    await requireFreshBuild(inspected.manifest, repositoryRoot);
    await replaceAtomically(resolve(staging.path, MANIFEST), resolve(destination, MANIFEST));
    const retention = await retainPublishedClearraWasmGenerations({
      destinationDir: destination,
      currentManifest: inspected.manifest,
      snapshot,
      publishHistory: async (serialized) => {
        const staged = resolve(staging.path, CLEARRA_WASM_GENERATION_HISTORY_FILE);
        await writeFile(staged, serialized, { flag: 'wx' });
        await replaceAtomically(staged, resolve(destination, CLEARRA_WASM_GENERATION_HISTORY_FILE));
      },
    });
    return Object.freeze({
      generation: inspected.manifest.wasm.sha256,
      sourceSha256: inspected.manifest.build.source_sha256,
      copiedFiles: 5,
      retention,
    });
  } finally { await staging.release(); }
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  try {
    const args = process.argv.slice(2);
    if (args.length !== 4 || args[0] !== '--from' || args[2] !== '--destination') {
      throw new Error('usage: node import-verified-clearra-wasm.mjs --from DIRECTORY --destination DIRECTORY');
    }
    const result = await importVerifiedClearraWasmDirectory({
      sourceDirectory: args[1], destinationDirectory: args[3],
    });
    console.log(`wasm_import=verified copied_files=5 wasm_sha256=${result.generation} source_sha256=${result.sourceSha256} build_count=0 accepted_authority=false`);
    if (result.retention.status === 'skipped') {
      console.warn(`wasm_generation_cleanup=skipped reason=${result.retention.reason}`);
    }
  } catch (error) {
    console.error(`wasm_import=failed reason=${error.message}`);
    process.exitCode = 1;
  }
}
