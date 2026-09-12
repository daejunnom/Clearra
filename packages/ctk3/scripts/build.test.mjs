import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { ctk3BuildKind, ctk3BuildPlan, executeCtk3Build, rebaseCtk3SourceMap } from './build.mjs';

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const transaction = resolve(packageRoot, 'source-test-only-transaction');
const plan = ctk3BuildPlan(transaction, packageRoot);

test('all CTK3 compiler outputs stay in one managed stage with unchanged package export names', () => {
  assert.equal(ctk3BuildKind([]), 'all');
  assert.equal(ctk3BuildKind(['--kind', 'types']), 'types');
  assert.throws(() => ctk3BuildKind(['--outdir', 'dist']));
  assert.throws(() => ctk3BuildPlan(transaction, packageRoot, '../../outside'));
  assert.deepEqual(plan.javascript.map(options => options.outfile),
    ['index.js', 'index.cjs', 'decodeWorker.js'].map(name => resolve(plan.stage, name)));
  assert.equal(plan.javascript[1].define['import.meta.url'], 'undefined');
  assert.equal(plan.javascript[2].platform, 'browser');
  assert.deepEqual(plan.types.slice(-4), ['--outDir', plan.stage, '--declarationDir', plan.stage]);
  assert.equal(plan.destination, resolve(packageRoot, 'dist'));
});

test('CTK3 publishes only after JS and declarations finish under their shared owner', async () => {
  for (const kind of ['all', 'js', 'types']) {
    const calls = [];
    await executeCtk3Build(kind, plan, {
      buildJavaScript: async options => calls.push(options.format),
      buildTypes: async () => calls.push('types'),
      publish: async () => calls.push('publish'),
    });
    assert.equal(calls.at(-1), 'publish');
    assert.equal(calls.includes('types'), kind !== 'js');
    assert.equal(calls.includes('esm'), kind !== 'types');
  }
});

test('JS or declaration failure does not replace the currently exported package', async () => {
  for (const failing of ['js', 'types']) {
    let published = false;
    let finished = 0;
    await assert.rejects(executeCtk3Build('all', plan, {
      buildJavaScript: async options => { await Promise.resolve(); finished++; if (failing === 'js' && options.format === 'cjs') throw new Error(failing); },
      buildTypes: async () => { if (failing === 'types') throw new Error(failing); },
      publish: async () => { published = true; },
    }), new RegExp(failing));
    assert.equal(finished, 3, 'all sibling compiler jobs must settle before owner cleanup');
    assert.equal(published, false);
  }
});

test('source-map references are rebased from private compiler stage to the final package', () => {
  const sourceMap = resolve(plan.stage, 'index.js.map');
  const exportedMap = resolve(plan.destination, 'index.js.map');
  const map = { version: 3, sources: ['../../src/index.ts'], sourcesContent: ['export {};'], names: [], mappings: '' };
  const actual = rebaseCtk3SourceMap(map, sourceMap, exportedMap);
  assert.equal(resolve(dirname(exportedMap), actual.sources[0]), resolve(dirname(sourceMap), map.sources[0]));
  assert.deepEqual(actual.sourcesContent, map.sourcesContent);
  assert.throws(() => rebaseCtk3SourceMap({ ...map, sourceRoot: '/unexpected' }, sourceMap, exportedMap));
});

test('package and repository entrypoints preserve public contracts while adopting one full build owner', async () => {
  const package_ = JSON.parse(await readFile(resolve(packageRoot, 'package.json'), 'utf8'));
  assert.equal(package_.scripts.build, 'node ./scripts/build.mjs');
  assert.equal(package_.scripts['build:js'], 'node ./scripts/build.mjs --kind js');
  assert.equal(package_.scripts['build:types'], 'node ./scripts/build.mjs --kind types');
  assert.equal(package_.main, './dist/index.cjs');
  assert.equal(package_.module, './dist/index.js');
  assert.equal(package_.types, './dist/index.d.ts');
  assert.deepEqual(package_.exports['.'], { types: './dist/index.d.ts', import: './dist/index.js', require: './dist/index.cjs', default: './dist/index.js' });
  const workspace = JSON.parse(await readFile(resolve(packageRoot, '../../package.json'), 'utf8'));
  assert.equal(workspace.scripts['build:ctk3'], 'npm run build --workspace ctk3');
  const source = await readFile(new URL('./build.mjs', import.meta.url), 'utf8');
  assert.ok(source.indexOf('enterManagedBuildOrRelaunch(sourceRoot') < source.indexOf("await import('esbuild')"));
  assert.match(source, /`build-\$\{randomUUID\(\)\}`/u);
});
