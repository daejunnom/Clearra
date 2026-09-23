import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { copyFile, mkdir, writeFile } from 'node:fs/promises';
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
