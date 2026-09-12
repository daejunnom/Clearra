import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { buildSourceId } from './clearra-build-policy.mjs';

const sourceRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const guardPath = join(sourceRoot, 'core-c/cmake/clearra_build_root.cmake');
const guardSource = await readFile(guardPath, 'utf8');
const cmakeAvailable = spawnSync('cmake', ['--version'], { encoding: 'utf8', windowsHide: true }).status === 0;
const quote = value => `[=[${value.replaceAll('\\', '/')}]=]`;

test('both CMake entry points validate the owner before project enables compiler probes', async () => {
  for (const path of ['CMakeLists.txt', 'core-c/CMakeLists.txt']) {
    const source = await readFile(join(sourceRoot, path), 'utf8');
    assert.ok(source.indexOf('clearra_build_root.cmake') > 0);
    assert.ok(source.indexOf('clearra_build_root.cmake') < source.indexOf('project('), path);
  }
  assert.match(guardSource, /--source-root "\$\{_clearra_cmake_authority\}" --field transaction/u);
  assert.match(guardSource, /p\.assertBuildPathWithin\(process\.argv\[2\], process\.argv\[3\]\)/u);
  assert.match(guardSource, /p\.assertNoBuildLinks\(process\.argv\[2\]\)/u);
  assert.match(guardSource, /cmake_language\(DEFER CALL _clearra_check_cmake_target_outputs\)/u);
  assert.doesNotMatch(guardSource, /if\([^\n]*IN_TRY_COMPILE[^\n]*\)\s*return\(/u);
});

// These tests configure no project and enable no language. Their tiny temporary
// marker/lease fixtures exercise validation only, never a compiler or build owner.
async function runPolicy({ session = true, binary = 'managed', outputs = [] } = {}) {
  const fixture = await mkdtemp(join(tmpdir(), 'clearra-cmake-policy-'));
  try {
    const environment = { ...process.env };
    for (const key of Object.keys(environment)) {
      if (/^(?:CLEARRA_|CARGO_|RUSTC_|WSL_)/u.test(key) || ['NODE_OPTIONS', 'BASH_ENV', 'ENV'].includes(key)) delete environment[key];
    }
    environment.LOCALAPPDATA = fixture;
    environment.XDG_CACHE_HOME = fixture;
    const root = join(fixture, 'Clearra/build');
    const sourceId = buildSourceId(sourceRoot);
    const sessionId = '1'.repeat(32);
    const transaction = join(root, 'experiments', sourceId, 'current');
    const cargoTarget = join(transaction, 'cargo-target');
    const lease = { schema_version: 1, purpose: 'experiment', source_id: sourceId, session_id: sessionId, owner_pid: process.pid };
    if (session) {
      await mkdir(transaction, { recursive: true });
      await mkdir(join(root, '.leases'), { recursive: true });
      await writeFile(join(transaction, '.clearra-build-transaction.json'), JSON.stringify({
        ...lease, schema_version: 3, source_root: sourceRoot, transaction_root: transaction,
        cargo_target_dir: cargoTarget, status: 'active', created_utc: new Date().toISOString(), completed_utc: null,
      }));
      await writeFile(join(root, '.leases', `experiment-${sourceId}.lock`), JSON.stringify(lease));
      Object.assign(environment, {
        CLEARRA_BUILD_ROOT: root, CLEARRA_BUILD_PURPOSE: 'experiment', CLEARRA_BUILD_SOURCE_ROOT: sourceRoot,
        CLEARRA_BUILD_SOURCE_ID: sourceId, CLEARRA_BUILD_SESSION_ID: sessionId,
        CLEARRA_BUILD_CACHE_SESSION_KEY: sessionId,
        CLEARRA_BUILD_TRANSACTION_ROOT: transaction, CARGO_TARGET_DIR: cargoTarget,
        CLEARRA_BUILD_CACHE_OWNER_PID: String(process.pid),
      });
    }
    const binaryDirectory = binary === 'outside'
      ? join(fixture, 'unmanaged-output')
      : join(transaction, 'core-c/CMakeFiles/CMakeScratch/TryCompile-contract');
    const script = join(fixture, 'policy.cmake');
    const statements = outputs.map(([key, value]) => `set(${key} ${quote(value === 'outside' ? join(fixture, 'other-output') : value)})`);
    await writeFile(script, [
      'cmake_minimum_required(VERSION 3.20)',
      `set(CMAKE_SOURCE_DIR ${quote(join(fixture, 'generated-compiler-probe-source'))})`,
      `set(CMAKE_BINARY_DIR ${quote(binaryDirectory)})`,
      `set(CMAKE_CURRENT_BINARY_DIR ${quote(binaryDirectory)})`,
      ...statements,
      `include(${quote(guardPath)})`,
      'message(STATUS "clearra-cmake-policy-script-pass")',
    ].join('\n'));
    return spawnSync('cmake', ['-P', script], { cwd: fixture, env: environment, encoding: 'utf8', windowsHide: true });
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
}

test('raw CMake rejects a missing managed session before any compiler probe', { skip: !cmakeAvailable }, async () => {
  const result = await runPolicy({ session: false });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /requires an active managed build session/u);
  assert.doesNotMatch(result.stdout, /clearra-cmake-policy-script-pass/u);
});

test('CMake rejects an external binary root despite a valid owner', { skip: !cmakeAvailable }, async () => {
  const result = await runPolicy({ binary: 'outside' });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /outside its managed transaction|outside its selected root/u);
});

test('nested compiler-probe binary directories remain allowed without borrowing their source identity', { skip: !cmakeAvailable }, async () => {
  const result = await runPolicy();
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  assert.match(result.stdout, /clearra-cmake-policy-script-pass/u);
});

for (const key of ['CMAKE_RUNTIME_OUTPUT_DIRECTORY', 'CMAKE_ARCHIVE_OUTPUT_DIRECTORY_RELEASE', 'CMAKE_PDB_OUTPUT_DIRECTORY', 'EXECUTABLE_OUTPUT_PATH']) {
  test(`CMake rejects external ${key}`, { skip: !cmakeAvailable }, async () => {
    const result = await runPolicy({ outputs: [[key, 'outside']] });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /outside its managed transaction|outside its selected root/u);
  });
}

test('CMake fails closed on unverifiable output generator expressions', { skip: !cmakeAvailable }, async () => {
  const result = await runPolicy({ outputs: [['CMAKE_RUNTIME_OUTPUT_DIRECTORY', '$<IF:$<CONFIG:Debug>,../escape,bin>']] });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /cannot validate an indirect CMake output path/u);
});

test('CMake install export prefix is not misclassified as compiler-cache output', { skip: !cmakeAvailable }, async () => {
  const result = await runPolicy({ outputs: [['CMAKE_INSTALL_PREFIX', 'outside']] });
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
});
