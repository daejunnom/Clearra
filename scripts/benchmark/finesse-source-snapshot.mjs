import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import { extname, resolve } from 'node:path';

const EXPLICIT_UNTRACKED_SOURCES = [
  'crates/clearra-app/src/native_build_probability_execution.rs',
  'crates/clearra-core-executor/src/backend/wasm_cpu/finesse_score.rs',
  'crates/clearra-core-executor/src/buildup/native_compact_finesse_language.rs',
  'crates/clearra-core-executor/src/finesse_report.rs',
  'crates/clearra-finesse/Cargo.toml',
  'crates/clearra-finesse/src/language.rs',
  'crates/clearra-finesse/src/lib.rs',
];

const SOURCE_EXTENSIONS = new Set([
  '.c',
  '.h',
  '.html',
  '.mjs',
  '.rs',
  '.svelte',
  '.toml',
  '.ts',
  '.wgsl',
]);

export function finesseSourceSnapshot(root) {
  const files = finesseSourceFiles(root).map((path) => snapshotFile(root, path));
  const hash = createHash('sha256');
  for (const file of files) {
    hash.update(`${file.path}\0${file.present}\0${file.sha256 ?? ''}\0${file.size ?? ''}\n`);
  }
  return { digest: hash.digest('hex'), files };
}

function finesseSourceFiles(root) {
  const tracked = execFileSync('git', ['ls-files', '-z'], {
    cwd: root,
    encoding: 'utf8',
    windowsHide: true,
  })
    .split('\0')
    .filter(Boolean)
    .map(normalizePath)
    .filter(isBuildRelevantSource);
  const files = new Set(tracked);
  for (const path of EXPLICIT_UNTRACKED_SOURCES) {
    if (fs.existsSync(resolve(root, path))) files.add(path);
  }
  return [...files].sort();
}

function isBuildRelevantSource(path) {
  if (path === 'Cargo.lock' || path === 'Cargo.toml') return true;
  if (path === 'rust-toolchain' || path === 'rust-toolchain.toml') return true;
  if (path.startsWith('.cargo/') && extname(path) === '.toml') return true;
  // Package manifests affect ESM and workspace resolution, but arbitrary JSON
  // can contain credentials and is deliberately excluded from this snapshot.
  if (path === 'package.json' || path.endsWith('/package.json')) return true;
  if (!SOURCE_EXTENSIONS.has(extname(path))) return false;
  return path.startsWith('core-c/include/') ||
    path.startsWith('core-c/src/') ||
    path.startsWith('crates/') ||
    path.startsWith('apps/clearra-web/src/') ||
    path.startsWith('packages/clearra-ui/src/lib/wasm/') ||
    path.startsWith('scripts/benchmark/wasm-product-browser/') ||
    path.startsWith('scripts/tools/');
}

function snapshotFile(root, path) {
  const sourcePath = resolve(root, path);
  if (!fs.existsSync(sourcePath)) return { path, present: false };
  const bytes = fs.readFileSync(sourcePath);
  return {
    path,
    present: true,
    sha256: createHash('sha256').update(bytes).digest('hex'),
    size: bytes.byteLength,
  };
}

function normalizePath(path) {
  return path.replaceAll('\\', '/');
}
