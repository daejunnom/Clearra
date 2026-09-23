import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { copyFile, mkdir, readFile, rename, rm, stat, writeFile } from 'node:fs/promises';
import { dirname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { assertBuildPathWithin, assertManagedBuildTransaction, assertNoBuildLinks,
  buildSourceId, canonicalBuildRoot } from './clearra-build-policy.mjs';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

/** Resolve the UI package's declared exports to its source tree for Vite.
 * pnpm injects workspace packages into a virtual store for applications. A
 * source-relative import from that copy cannot reach the sibling ctk3 package.
 * Keep the package's public export map authoritative while bundling the
 * original source, so both applications see the same CTK3 implementation.
 */
export function frontendUiSourceAliases(sourceRoot = repositoryRoot) {
  const packageRoot = resolve(sourceRoot, 'packages', 'clearra-ui');
  const manifest = JSON.parse(readFileSync(resolve(packageRoot, 'package.json'), 'utf8'));
  return Object.entries(manifest.exports).map(([subpath, target]) => {
    if (typeof target !== 'string' || !target.startsWith('./src/lib/') ||
        !/^\.(?:\/[a-z-]+)?$/u.test(subpath)) {
      throw new Error(`Invalid @clearra/ui source export: ${subpath}`);
    }
    const specifier = `@clearra/ui${subpath === '.' ? '' : subpath.slice(1)}`;
    const escaped = specifier.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&');
    return { find: new RegExp(`^${escaped}$`, 'u'), replacement: resolve(packageRoot, target) };
  });
}

/** Select paths without creating a transaction or writing compiler output. */
export function frontendPaths(app, { sourceRoot = repositoryRoot, environment = process.env,
  requireOwner = false, validateOwner = assertManagedBuildTransaction } = {}) {
  if (!['web', 'desktop'].includes(app)) throw new Error('Unknown Clearra frontend');
  sourceRoot = resolve(sourceRoot);
  const hasOwner = Boolean(environment.CLEARRA_BUILD_SESSION_ID);
  if (!hasOwner && requireOwner) {
    throw new Error('Frontend compilation requires a live Clearra build owner; use pnpm run build, dev, or sync');
  }
  if (!hasOwner && ['CLEARRA_BUILD_TRANSACTION_ROOT', 'CLEARRA_BUILD_SOURCE_ROOT',
    'CLEARRA_BUILD_SOURCE_ID', 'CLEARRA_BUILD_CACHE_OWNER_PID'].some(key => environment[key])) {
    throw new Error('Incomplete Clearra frontend owner identity');
  }
  // The unowned view is for editor config loading only. Vite and the sync CLI
  // require an active owner before they may write anything to these paths.
  const transactionRoot = hasOwner
    ? validateOwner({ environment, sourceRoot }).transaction
    : resolve(canonicalBuildRoot(environment), 'experiments', buildSourceId(sourceRoot), 'current');
  const frontendRoot = resolve(transactionRoot, 'frontend', app);
  assertBuildPathWithin(frontendRoot, transactionRoot);
  assertNoBuildLinks(frontendRoot);
  const appRoot = resolve(sourceRoot, 'apps', `clearra-${app}`);
  return {
    appRoot, transactionRoot, frontendRoot,
    kitOutDir: resolve(frontendRoot, 'svelte-kit'),
    viteCacheDir: resolve(frontendRoot, 'vite-cache'),
    publicDir: resolve(frontendRoot, 'public'),
    // This is a publication/export, never SvelteKit or Vite's compiler cache.
    exportDir: resolve(appRoot, 'build'),
  };
}

export function frontendConfigPaths(app, options = {}) {
  const compilerCli = process.argv.slice(1).some(argument =>
    /(?:^|[\\/])(?:vite|svelte-kit)(?:\.[cm]?js)?$/u.test(argument));
  return frontendPaths(app, { ...options, requireOwner: compilerCli });
}

/** The only generated source-local SvelteKit file is this small IDE forwarder. */
export async function writeFrontendTypeForwarder(paths) {
  const directory = resolve(paths.appRoot, '.svelte-kit');
  assertNoBuildLinks(directory);
  const file = resolve(directory, 'tsconfig.json');
  assertNoBuildLinks(file);
  await mkdir(directory, { recursive: true });
  await writeFile(file, `${JSON.stringify({ extends: resolve(paths.kitOutDir, 'tsconfig.json') }, null, 2)}\n`);
}

export async function stageFrontendPublicAssets(paths, sourceRoot = repositoryRoot) {
  const staticRoot = resolve(paths.appRoot, 'static');
  const tracked = execFileSync('git', ['-C', sourceRoot, 'ls-files', '-z', '--',
    relative(sourceRoot, staticRoot).replaceAll('\\', '/')], { encoding: 'utf8', windowsHide: true });
  for (const trackedPath of tracked.split('\0').filter(Boolean)) {
    const source = resolve(sourceRoot, trackedPath);
    assertBuildPathWithin(source, staticRoot);
    assertNoBuildLinks(source);
    const suffix = relative(staticRoot, source);
    // WASM is separately produced and verified in the same live transaction.
    if (suffix === 'wasm' || /^[Ww][Aa][Ss][Mm][\\/]/u.test(suffix)) continue;
    const destination = resolve(paths.publicDir, suffix);
    assertBuildPathWithin(destination, paths.publicDir);
    assertNoBuildLinks(destination);
    await mkdir(dirname(destination), { recursive: true });
    await copyFile(source, destination);
  }
}

const ACCELERATOR_CATALOGS = [
  ['exact-legal-board', 'lb', 'legal-board-product-catalog.v1.json', 64 * 1024 * 1024],
  ['board-conditioned-reachability', 'cr', 'conditioned-reachability-product-catalog.v1.json', 16 * 1024 * 1024],
];
const ACCELERATOR_PROFILES = new Set(['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick']);

// The browser cannot read GitHub Release's redirected binary response through
// CORS. Resolve only source-catalogued, qualified, immutable Release assets;
// both Pages staging and the explicit-download dev proxy use this same map.
export async function frontendAcceleratorAssets(sourceRoot = repositoryRoot) {
  const assets = [];
  for (const [product, shortName, file, maximumBytes] of ACCELERATOR_CATALOGS) {
    const catalog = JSON.parse(await readFile(resolve(sourceRoot, 'config', file), 'utf8'));
    if (catalog.product !== product || !Array.isArray(catalog.profiles)) {
      throw new Error('Invalid accelerator source catalog');
    }
    const profiles = new Set();
    let productBytes = 0;
    for (const slot of catalog.profiles) {
      if (slot.status !== 'qualified') continue;
      const metadata = slot.metadata;
      const profile = slot.profile;
      const payloadBytes = Number(metadata?.payload_bytes);
      if (!ACCELERATOR_PROFILES.has(profile) || !metadata ||
          !/^[1-9][0-9]*$/u.test(metadata.payload_bytes) ||
          !Number.isSafeInteger(payloadBytes) || payloadBytes > maximumBytes ||
          !/^[a-f0-9]{64}$/u.test(metadata.payload_identity)) {
        throw new Error('Invalid qualified accelerator source metadata');
      }
      if (profiles.has(profile)) throw new Error('Duplicate qualified accelerator profile');
      profiles.add(profile);
      productBytes += payloadBytes;
      if (productBytes > maximumBytes * ACCELERATOR_PROFILES.size) {
        throw new Error('Qualified accelerator source aggregate exceeds product limit');
      }
      const release = new URL(metadata.url);
      if (release.origin !== 'https://github.com' || release.search || release.hash ||
          !/^\/daejunnom\/Clearra\/releases\/download\/[A-Za-z0-9._-]+\/[A-Za-z0-9._-]+\.(?:cllb|cllr)$/u.test(release.pathname)) {
        throw new Error('Accelerator source is not an immutable Clearra Release asset');
      }
      assets.push({ product, profile, bytes: payloadBytes,
        digest: metadata.payload_identity, url: release.href,
        pathname: `/accel/${shortName}/${profile}/${metadata.payload_identity}.bin` });
    }
  }
  return assets;
}

export async function fetchQualifiedFrontendAccelerator(asset, fetcher = fetch) {
  const response = await fetcher(asset.url, { redirect: 'follow', signal: AbortSignal.timeout(90_000) });
  if (!response.ok || !response.body ||
      (response.headers.has('content-length') && Number(response.headers.get('content-length')) !== asset.bytes)) {
    throw new Error('Accelerator Release response size or status differs from signed source catalog');
  }
  const bytes = new Uint8Array(asset.bytes);
  const digest = createHash('sha256');
  const reader = response.body.getReader();
  let offset = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      if (offset + value.byteLength > bytes.byteLength) throw new Error('Accelerator Release asset exceeds signed size');
      bytes.set(value, offset);
      digest.update(value);
      offset += value.byteLength;
    }
  } finally { reader.releaseLock(); }
  if (offset !== bytes.byteLength || digest.digest('hex') !== asset.digest) {
    throw new Error('Accelerator Release asset differs from signed source catalog');
  }
  return bytes;
}

export async function stageFrontendAcceleratorAssets(paths, sourceRoot = repositoryRoot) {
  for (const asset of await frontendAcceleratorAssets(sourceRoot)) {
    const destination = resolve(paths.publicDir, `.${asset.pathname}`);
    assertBuildPathWithin(destination, paths.publicDir);
    assertNoBuildLinks(destination);
    await mkdir(dirname(destination), { recursive: true });
    try {
      const present = await stat(destination);
      if (present.isFile() && present.size === asset.bytes &&
          createHash('sha256').update(await readFile(destination)).digest('hex') === asset.digest) continue;
    } catch (error) { if (error.code !== 'ENOENT') throw error; }
    const bytes = await fetchQualifiedFrontendAccelerator(asset);
    const temporary = `${destination}.${process.pid}.partial`;
    assertNoBuildLinks(temporary);
    try {
      await writeFile(temporary, bytes, { flag: 'wx' });
      await rm(destination, { force: true });
      await rename(temporary, destination);
    } finally { await rm(temporary, { force: true }); }
  }
}
