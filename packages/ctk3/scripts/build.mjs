import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { cp, mkdir, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { assertBuildPathWithin, assertNoBuildLinks, enterManagedBuildOrRelaunch } from '../../../scripts/tools/clearra-build-policy.mjs';

const self = fileURLToPath(import.meta.url);
const packageRoot = resolve(dirname(self), '..');
const sourceRoot = resolve(packageRoot, '..', '..');

export function ctk3BuildKind(args) {
  if (args.length === 0) return 'all';
  if (args.length === 2 && args[0] === '--kind' && ['js', 'types'].includes(args[1])) return args[1];
  throw new Error('CTK3 accepts only --kind js or --kind types; output paths are owner-selected');
}

export function ctk3BuildPlan(transaction, root = packageRoot, stageName = 'dist-stage') {
  if (!/^[a-zA-Z0-9-]+$/u.test(stageName)) throw new Error('Invalid CTK3 staging name');
  const stage = resolve(transaction, 'ctk3', stageName);
  assertBuildPathWithin(stage, transaction);
  const shared = {
    absWorkingDir: root, entryPoints: ['src/index.ts'], bundle: true, platform: 'neutral',
    target: 'es2020', sourcemap: true, external: ['tetris-fumen'], logLevel: 'info',
  };
  return { stage, destination: resolve(root, 'dist'),
    javascript: [
      { ...shared, format: 'esm', outfile: resolve(stage, 'index.js') },
      { ...shared, format: 'cjs', platform: 'node', define: { 'import.meta.url': 'undefined' }, outfile: resolve(stage, 'index.cjs') },
      { absWorkingDir: root, entryPoints: ['src/decodeWorker.ts'], bundle: true, platform: 'browser',
        target: 'es2020', format: 'esm', sourcemap: true, outfile: resolve(stage, 'decodeWorker.js'), logLevel: 'info' },
    ],
    types: ['-p', resolve(root, 'tsconfig.json'), '--outDir', stage, '--declarationDir', stage],
  };
}

export function rebaseCtk3SourceMap(map, sourceMap, exportedMap) {
  if (!Array.isArray(map.sources) || map.sourceRoot) throw new Error('Unsupported CTK3 source-map layout');
  return { ...map, sources: map.sources.map(source => {
    if (typeof source !== 'string' || /^[A-Za-z]+:\/\//u.test(source)) throw new Error('Unexpected CTK3 source-map source');
    return relative(dirname(exportedMap), resolve(dirname(sourceMap), source)).replaceAll('\\', '/');
  }) };
}

export async function executeCtk3Build(kind, plan, { buildJavaScript, buildTypes, publish }) {
  if (kind !== 'types') {
    const results = await Promise.allSettled(plan.javascript.map(options => Promise.resolve().then(() => buildJavaScript(options))));
    const failure = results.find(result => result.status === 'rejected');
    if (failure) throw failure.reason;
  }
  if (kind !== 'js') await buildTypes(plan.types);
  // A failed compiler never mutates the exported package.
  await publish(plan, kind);
}

async function publish(plan, kind) {
  const walk = async directory => {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = resolve(directory, entry.name);
      assertNoBuildLinks(path);
      if (entry.isDirectory()) await walk(path);
      else if (entry.name.endsWith('.map')) {
        const target = resolve(plan.destination, relative(plan.stage, path));
        const map = rebaseCtk3SourceMap(JSON.parse(await readFile(path, 'utf8')), path, target);
        await writeFile(path, `${JSON.stringify(map)}\n`);
      }
    }
  };
  await walk(plan.stage);
  const expectedDestination = resolve(packageRoot, 'dist');
  if (plan.destination !== expectedDestination) throw new Error('CTK3 export path changed');
  assertNoBuildLinks(expectedDestination);
  // Only the final package export is replaced, after all requested compilers
  // succeeded. Compiler output and staging stay in the managed transaction.
  if (kind === 'all') await rm(expectedDestination, { recursive: true, force: true });
  await cp(plan.stage, expectedDestination, { recursive: true });
}

async function main() {
  const kind = ctk3BuildKind(process.argv.slice(2));
  const owner = enterManagedBuildOrRelaunch(sourceRoot, process.argv.slice(1), process.env.CLEARRA_BUILD_PURPOSE || 'product');
  const plan = ctk3BuildPlan(owner.transaction, packageRoot, `build-${randomUUID()}`);
  assertNoBuildLinks(plan.stage);
  await mkdir(plan.stage, { recursive: true });
  const { build } = await import('esbuild');
  const require = createRequire(resolve(packageRoot, 'package.json'));
  const tsc = resolve(dirname(require.resolve('typescript/package.json')), 'bin/tsc');
  await executeCtk3Build(kind, plan, {
    buildJavaScript: options => build(options),
    buildTypes: arguments_ => {
      const result = spawnSync(process.execPath, [tsc, ...arguments_], {
        cwd: packageRoot, env: process.env, stdio: 'inherit', windowsHide: true,
      });
      if (result.error || result.status !== 0) throw result.error || new Error('CTK3 declaration compiler failed');
    },
    publish,
  });
}

if (process.argv[1] && resolve(process.argv[1]) === self) {
  await main().catch(error => { process.stderr.write(`${error.message}\n`); process.exitCode = 1; });
}
