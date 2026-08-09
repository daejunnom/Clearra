import { createHash } from 'node:crypto';
import { lstat, readdir, readFile } from 'node:fs/promises';
import { basename, extname, relative, resolve } from 'node:path';

export const CLEARRA_WASM_BUILD_CONTRACT_VERSION = 1;

export const CLEARRA_WASM_REQUIRED_CAPABILITIES = Object.freeze([
  'command-envelope/v1',
  'build-probability/finesse-inputs/v1',
  'finesse/search/v1',
  'finesse/score/v1',
]);

const REQUIRED_ROOT_FILES = Object.freeze(['Cargo.toml', 'Cargo.lock']);
const OPTIONAL_ROOT_FILES = Object.freeze(['rust-toolchain', 'rust-toolchain.toml']);
const OPTIONAL_PRODUCER_FILES = Object.freeze([
  'scripts/tools/build-clearra-wasm.mjs',
  'scripts/tools/clearra-wasm-build-contract.mjs',
]);
const SOURCE_ROOTS = Object.freeze(['crates', 'core-c', 'tools/vendor']);
const SOURCE_EXTENSIONS = new Set(['.c', '.cmake', '.frag', '.h', '.rs', '.vert', '.wgsl']);
const IGNORED_DIRECTORIES = new Set([
  '.cache',
  'build',
  'checkpoints',
  'coverage',
  'dist',
  'dist-server',
  'models',
  'node_modules',
  'target',
]);

/**
 * Creates the build contract embedded in the standard browser WASM manifest.
 * Only compile inputs with explicitly safe names/extensions are read. In
 * particular, package-lock files, environment files, credentials, and JSON
 * documents are never part of this snapshot.
 */
export async function createClearraWasmBuildContract(repositoryRoot) {
  const root = resolve(repositoryRoot);
  const files = await collectClearraWasmBuildSourceFiles(root);
  const sourceHash = createHash('sha256');
  for (const path of files) {
    const bytes = await readFile(resolve(root, path));
    sourceHash.update(path, 'utf8');
    sourceHash.update('\0');
    sourceHash.update(String(bytes.byteLength), 'utf8');
    sourceHash.update('\0');
    sourceHash.update(bytes);
    sourceHash.update('\0');
  }
  return Object.freeze({
    contract_version: CLEARRA_WASM_BUILD_CONTRACT_VERSION,
    source_sha256: sourceHash.digest('hex'),
    source_file_count: files.length,
    capabilities_sha256: clearraWasmCapabilitiesSha256(),
  });
}

export function clearraWasmCapabilitiesSha256() {
  const hash = createHash('sha256');
  hash.update(`clearra-wasm-capabilities-v${CLEARRA_WASM_BUILD_CONTRACT_VERSION}\n`);
  for (const capability of CLEARRA_WASM_REQUIRED_CAPABILITIES) {
    hash.update(capability, 'utf8');
    hash.update('\n');
  }
  return hash.digest('hex');
}

export function isClearraWasmBuildContract(value) {
  return Boolean(
    value &&
      typeof value === 'object' &&
      value.contract_version === CLEARRA_WASM_BUILD_CONTRACT_VERSION &&
      isSha256(value.source_sha256) &&
      Number.isSafeInteger(value.source_file_count) &&
      value.source_file_count > 0 &&
      value.capabilities_sha256 === clearraWasmCapabilitiesSha256()
  );
}

export function clearraWasmBuildContractsEqual(left, right) {
  return Boolean(
    isClearraWasmBuildContract(left) &&
      isClearraWasmBuildContract(right) &&
      left.contract_version === right.contract_version &&
      left.source_sha256 === right.source_sha256 &&
      left.source_file_count === right.source_file_count &&
      left.capabilities_sha256 === right.capabilities_sha256
  );
}

export async function collectClearraWasmBuildSourceFiles(repositoryRoot) {
  const root = resolve(repositoryRoot);
  const files = new Set();

  for (const path of REQUIRED_ROOT_FILES) {
    if (!(await isRegularFile(resolve(root, path)))) {
      throw new Error(`Clearra WASM build input is missing: ${resolve(root, path)}`);
    }
    files.add(path);
  }
  for (const path of [...OPTIONAL_ROOT_FILES, ...OPTIONAL_PRODUCER_FILES]) {
    if (await isRegularFile(resolve(root, path))) files.add(path);
  }
  for (const path of ['.cargo/config', '.cargo/config.toml']) {
    if (await isRegularFile(resolve(root, path))) files.add(path);
  }
  for (const sourceRoot of SOURCE_ROOTS) {
    await collectSourceTree(root, sourceRoot, files);
  }

  return [...files].sort(comparePaths);
}

async function collectSourceTree(root, relativeDirectory, files) {
  const directory = resolve(root, relativeDirectory);
  let entries;
  try {
    const directoryMetadata = await lstat(directory);
    if (directoryMetadata.isSymbolicLink()) {
      throw new Error(`Clearra WASM build inputs must not contain symlinks: ${relativeDirectory}`);
    }
    if (!directoryMetadata.isDirectory()) return;
    entries = await readdir(directory, { withFileTypes: true });
  } catch (error) {
    if (error?.code === 'ENOENT') return;
    throw error;
  }
  entries.sort((left, right) => comparePaths(left.name, right.name));
  for (const entry of entries) {
    const relativePath = normalizePath(`${relativeDirectory}/${entry.name}`);
    if (entry.isSymbolicLink()) {
      throw new Error(`Clearra WASM build inputs must not contain symlinks: ${relativePath}`);
    }
    if (entry.isDirectory()) {
      if (!IGNORED_DIRECTORIES.has(entry.name)) {
        await collectSourceTree(root, relativePath, files);
      }
      continue;
    }
    if (!entry.isFile() || !isCompileInput(relativePath)) continue;
    files.add(normalizePath(relative(root, resolve(root, relativePath))));
  }
}

function isCompileInput(path) {
  const name = basename(path);
  return (
    name === 'Cargo.toml' ||
    name === 'CMakeLists.txt' ||
    name === 'build.rs' ||
    SOURCE_EXTENSIONS.has(extname(name).toLowerCase())
  );
}

async function isRegularFile(path) {
  try {
    const metadata = await lstat(path);
    if (metadata.isSymbolicLink()) {
      throw new Error(`Clearra WASM build inputs must not contain symlinks: ${path}`);
    }
    return metadata.isFile();
  } catch (error) {
    if (error?.code === 'ENOENT') return false;
    throw error;
  }
}

function isSha256(value) {
  return typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
}

function normalizePath(path) {
  return path.replaceAll('\\', '/');
}

function comparePaths(left, right) {
  return left < right ? -1 : left > right ? 1 : 0;
}
