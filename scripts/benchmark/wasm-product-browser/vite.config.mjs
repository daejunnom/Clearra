import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, version as viteVersion } from 'vite';
import { finesseSourceSnapshot } from '../finesse-source-snapshot.mjs';

const configRoot = dirname(fileURLToPath(import.meta.url));
const toolingRepository = resolve(configRoot, '../../..');
const repository = process.env.CLEARRA_BENCHMARK_SOURCE_ROOT
  ? resolve(process.env.CLEARRA_BENCHMARK_SOURCE_ROOT)
  : resolve(configRoot, '../../..');
const root = resolve(repository, 'scripts/benchmark/wasm-product-browser');
const stagedWasmRoot = process.env.CLEARRA_BENCHMARK_WASM_ROOT
  ? resolve(process.env.CLEARRA_BENCHMARK_WASM_ROOT)
  : null;
const benchmarkProducer = resolve(configRoot, '../../tools/build-clearra-wasm.mjs');
const snapshotTool = resolve(configRoot, '../finesse-source-snapshot.mjs');
const benchmarkProvenanceFile = 'clearra-finesse-wasm-build-provenance.json';
const benchmarkSourceSnapshot = stagedWasmRoot === null
  ? null
  : finesseSourceSnapshot(repository);
const packagerSha256 = sha256(readFileSync(fileURLToPath(import.meta.url)));
const snapshotToolSha256 = sha256(readFileSync(snapshotTool));
const browserBuildToolchain = browserBuildToolchainIdentity(toolingRepository);
const isolationHeaders = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Cross-Origin-Embedder-Policy': 'require-corp'
};

export default defineConfig({
  root,
  // A provenance-bound benchmark copies only the explicitly built WASM
  // directory below. Avoid silently inheriting unrelated static artifacts.
  publicDir: stagedWasmRoot === null
    ? resolve(repository, 'apps/clearra-web/static')
    : false,
  resolve: {
    alias: {
      '@clearra/ui/wasm-lifecycle': resolve(
        repository,
        'packages/clearra-ui/src/lib/wasm/wasmWorkerLifecycle.ts'
      ),
      '@clearra/ui/wasm': resolve(repository, 'packages/clearra-ui/src/lib/wasm/index.ts')
    }
  },
  plugins: stagedWasmRoot === null
    ? []
    : [benchmarkArtifactProvenancePlugin(
        repository,
        stagedWasmRoot,
        benchmarkSourceSnapshot,
        packagerSha256,
        snapshotToolSha256,
        browserBuildToolchain,
      )],
  build: {
    target: 'es2022'
  },
  server: {
    fs: { allow: [repository] },
    headers: isolationHeaders
  },
  preview: {
    headers: isolationHeaders
  }
});

function benchmarkArtifactProvenancePlugin(
  sourceRoot,
  wasmRoot,
  initialSnapshot,
  initialPackagerSha256,
  initialSnapshotToolSha256,
  initialBrowserBuildToolchain,
) {
  let outputRoot = null;
  return {
    name: 'clearra-finesse-benchmark-provenance',
    apply: 'build',
    configResolved(config) {
      outputRoot = resolve(config.root, config.build.outDir);
    },
    closeBundle() {
      if (outputRoot === null) throw new Error('benchmark output directory was not resolved');
      const manifestPath = resolve(wasmRoot, 'clearra_wasm.manifest.json');
      const buildProvenancePath = resolve(wasmRoot, benchmarkProvenanceFile);
      if (!existsSync(manifestPath)) {
        throw new Error(`benchmark WASM manifest is missing: ${manifestPath}`);
      }
      if (!existsSync(buildProvenancePath)) {
        throw new Error(`benchmark WASM build provenance is missing: ${buildProvenancePath}`);
      }
      const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
      const buildProvenance = JSON.parse(readFileSync(buildProvenancePath, 'utf8'));
      const wasmPath = resolve(wasmRoot, manifest?.wasm?.path ?? '');
      const bindingsPath = resolve(wasmRoot, manifest?.bindings?.path ?? '');
      if (!existsSync(wasmPath)) {
        throw new Error(`benchmark WASM artifact is missing: ${wasmPath}`);
      }
      if (!existsSync(bindingsPath)) {
        throw new Error(`benchmark WASM bindings are missing: ${bindingsPath}`);
      }
      const wasmSha256 = sha256(readFileSync(wasmPath));
      const bindingsSha256 = sha256(readFileSync(bindingsPath));
      if (manifest?.wasm?.sha256 !== wasmSha256) {
        throw new Error('benchmark WASM manifest hash does not match the artifact');
      }
      if (manifest?.bindings?.sha256 !== bindingsSha256) {
        throw new Error('benchmark bindings manifest hash does not match the artifact');
      }
      const snapshot = finesseSourceSnapshot(sourceRoot);
      if (
        snapshot.digest !== initialSnapshot.digest ||
        snapshot.files.length !== initialSnapshot.files.length
      ) {
        throw new Error('benchmark source changed while the browser artifact was being built');
      }
      const producerSha256 = sha256(readFileSync(benchmarkProducer));
      const finalPackagerSha256 = sha256(readFileSync(fileURLToPath(import.meta.url)));
      const finalSnapshotToolSha256 = sha256(readFileSync(snapshotTool));
      const finalBrowserBuildToolchain = browserBuildToolchainIdentity(toolingRepository);
      if (
        finalPackagerSha256 !== initialPackagerSha256 ||
        finalSnapshotToolSha256 !== initialSnapshotToolSha256 ||
        stableJson(finalBrowserBuildToolchain) !== stableJson(initialBrowserBuildToolchain)
      ) {
        throw new Error('benchmark packager changed while the browser artifact was being built');
      }
      if (
        buildProvenance?.schema_version !== 1 ||
        buildProvenance.source_snapshot_sha256 !== snapshot.digest ||
        buildProvenance.source_file_count !== snapshot.files.length ||
        buildProvenance.wasm_sha256 !== wasmSha256 ||
        buildProvenance.bindings_sha256 !== bindingsSha256 ||
        buildProvenance.producer_sha256 !== producerSha256 ||
        buildProvenance.snapshot_tool_sha256 !== initialSnapshotToolSha256 ||
        !validToolchain(buildProvenance.toolchain) ||
        buildProvenance?.build_options?.stage_profiling !== true ||
        buildProvenance?.build_options?.environment !== buildProvenance.toolchain.environment
      ) {
        throw new Error('benchmark WASM build provenance does not match source or producer');
      }
      const destination = resolve(outputRoot, 'wasm');
      rmSync(destination, { recursive: true, force: true });
      cpSync(wasmRoot, destination, { recursive: true, force: true });
      writeFileSync(
        resolve(outputRoot, 'clearra-finesse-build-provenance.json'),
        `${JSON.stringify({
          schema_version: 1,
          source_snapshot_sha256: snapshot.digest,
          source_file_count: snapshot.files.length,
          wasm_sha256: wasmSha256,
          bindings_sha256: bindingsSha256,
          producer_sha256: producerSha256,
          snapshot_tool_sha256: initialSnapshotToolSha256,
          packager_sha256: initialPackagerSha256,
          toolchain: buildProvenance.toolchain,
          browser_build_toolchain: initialBrowserBuildToolchain,
          build_options: buildProvenance.build_options,
        }, null, 2)}\n`,
        'utf8'
      );
    },
  };
}

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

function validToolchain(toolchain) {
  return toolchain !== null && typeof toolchain === 'object' &&
    (toolchain.environment === 'native' || toolchain.environment === 'wsl') &&
    typeof toolchain.rustc === 'string' && toolchain.rustc.length > 0 &&
    typeof toolchain.cargo === 'string' && toolchain.cargo.length > 0 &&
    typeof toolchain.wasm_bindgen === 'string' && toolchain.wasm_bindgen.length > 0;
}

function browserBuildToolchainIdentity(root) {
  const lockStatus = commandOutput(
    'git',
    ['status', '--porcelain', '--untracked-files=no', '--', 'package-lock.json'],
    root
  );
  if (lockStatus.length > 0) {
    throw new Error('package-lock.json must be clean for a provenance-bound benchmark build');
  }
  return {
    package_manager: 'npm',
    package_lock_git_oid: commandOutput(
      'git',
      ['rev-parse', 'HEAD:package-lock.json'],
      root
    ),
    npm: npmCommandOutput(['--version'], root),
    vite: viteVersion,
  };
}

function npmCommandOutput(args, cwd) {
  if (process.platform === 'win32') {
    return commandOutput(
      process.env.ComSpec || 'cmd.exe',
      ['/d', '/s', '/c', ['npm', ...args].join(' ')],
      cwd
    );
  }
  return commandOutput('npm', args, cwd);
}

function commandOutput(command, args, cwd) {
  return execFileSync(command, args, {
    cwd,
    encoding: 'utf8',
    windowsHide: true,
  }).trim();
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
