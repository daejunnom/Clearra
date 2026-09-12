import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { frontendOptions, frontendPlan, executeFrontendPlan } from './build-clearra-frontend.mjs';
import { frontendPaths } from './clearra-frontend-paths.mjs';
import { validateManagedFrontendSource } from './validate-managed-frontend-source.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const fakeSource = resolve(root, 'source-policy-fixture');
const cacheHome = resolve(root, 'cache-policy-fixture');
const environment = { LOCALAPPDATA: cacheHome, XDG_CACHE_HOME: cacheHome, HOME: cacheHome };
const transaction = resolve(cacheHome, 'Clearra/build/products/source-test-only');
const paths = frontendPaths('web', { sourceRoot: fakeSource,
  environment: { ...environment, CLEARRA_BUILD_SESSION_ID: 'fixture' },
  validateOwner: options => {
    assert.equal(options.sourceRoot, fakeSource);
    return { transaction };
  },
});

test('frontend compiler and cache paths are selected inside the validated transaction', () => {
  assert.equal(paths.kitOutDir, resolve(transaction, 'frontend/web/svelte-kit'));
  assert.equal(paths.viteCacheDir, resolve(transaction, 'frontend/web/vite-cache'));
  assert.equal(paths.publicDir, resolve(transaction, 'frontend/web/public'));
  assert.equal(paths.exportDir, resolve(fakeSource, 'apps/clearra-web/build'));
  const desktop = frontendPaths('desktop', { sourceRoot: fakeSource,
    environment: { ...environment, CLEARRA_BUILD_SESSION_ID: 'fixture' },
    validateOwner: () => ({ transaction }),
  });
  assert.notEqual(desktop.kitOutDir, paths.kitOutDir);
  assert.throws(() => frontendPaths('../escape', { environment }));
});

test('only a read-only editor view may select an unowned current path', () => {
  const editor = frontendPaths('web', { sourceRoot: fakeSource, environment });
  assert.match(editor.kitOutDir.replaceAll('\\', '/'), /\/experiments\/[a-f0-9]{24}\/current\/frontend\/web\/svelte-kit$/u);
  assert.throws(() => frontendPaths('web', { sourceRoot: fakeSource, environment, requireOwner: true }), /live Clearra build owner/);
  assert.throws(() => frontendPaths('web', { sourceRoot: fakeSource,
    environment: { ...environment, CLEARRA_BUILD_TRANSACTION_ROOT: transaction } }), /Incomplete/);
});

test('stale or mismatched owners are rejected before frontend path consumption', () => {
  for (const reason of ['stale', 'wrong source', 'wrong purpose', 'outside root']) {
    assert.throws(() => frontendPaths('web', { sourceRoot: fakeSource,
      environment: { ...environment, CLEARRA_BUILD_SESSION_ID: 'fixture' }, requireOwner: true,
      validateOwner: () => { throw new Error(reason); },
    }), new RegExp(reason));
  }
});

test('web build, WSL build, dev, sync and tests keep one ordered owner payload', () => {
  for (const task of ['build', 'dev']) {
    const options = frontendOptions(['--app', 'web', '--task', task, '--environment', 'wsl']);
    const plan = frontendPlan(options, paths);
    assert.deepEqual(plan.map(command => command.kind), task === 'build'
      ? ['sync', 'public-assets', 'wasm', 'vite', 'fallback'] : ['sync', 'public-assets', 'wasm', 'vite']);
    assert.deepEqual(plan.find(command => command.kind === 'wasm').arguments,
      ['--environment', 'wsl', '--destination', resolve(paths.publicDir, 'wasm')]);
    const vite = plan.find(command => command.kind === 'vite').arguments;
    assert.deepEqual(vite.slice(vite.indexOf('--configLoader'), vite.indexOf('--configLoader') + 2), ['--configLoader', 'runner']);
    if (task === 'dev') assert.deepEqual(vite.slice(-3), ['--port', '4194', '--strictPort']);
  }
  assert.deepEqual(frontendPlan(frontendOptions(['--app', 'web', '--task', 'test']), paths).map(command => command.kind),
    ['sync', 'typecheck', 'contracts']);
  assert.deepEqual(frontendPlan(frontendOptions(['--app', 'web', '--task', 'sync']), paths).map(command => command.kind), ['sync']);
  assert.deepEqual(frontendPlan(frontendOptions(['--app', 'desktop']), paths).map(command => command.kind), ['sync', 'vite']);
});

test('login recovery preserves published WASM and only starts the requested loopback listener', () => {
  const options = frontendOptions(['--app', 'web', '--task', 'dev', '--recovery', '--mode', 'local-recovery', '--port', '4194']);
  const plan = frontendPlan(options, paths);
  assert.deepEqual(plan.map(command => command.kind), ['sync', 'vite']);
  assert.ok(plan[1].arguments.includes('local-recovery'));
  for (const args of [
    ['--app', 'web', '--recovery'], ['--app', 'desktop', '--task', 'test'],
    ['--app', 'web', '--task', 'dev', '--host', '0.0.0.0'],
    ['--app', 'web', '--port', '65536'], ['--app', 'web', '--outDir', '/tmp/escape'],
  ]) assert.throws(() => frontendOptions(args));
});

test('type forwarding follows successful sync and every failed payload stops later work', async () => {
  const commands = frontendPlan(frontendOptions(['--app', 'web']), paths);
  const order = [];
  await executeFrontendPlan(commands, { run: async command => order.push(command.kind), afterSync: async () => order.push('types-forwarded') });
  assert.deepEqual(order, ['sync', 'types-forwarded', 'public-assets', 'wasm', 'vite', 'fallback']);
  for (const failure of commands.map(command => command.kind)) {
    const seen = [];
    await assert.rejects(executeFrontendPlan(commands, {
      run: async command => { seen.push(command.kind); if (command.kind === failure) throw new Error(failure); },
      afterSync: async () => seen.push('types-forwarded'),
    }), new RegExp(failure));
    assert.equal(seen.at(-1), failure);
  }
});

test('both packages route lifecycle work through the owner and keep only final adapter exports local', async () => {
  for (const app of ['web', 'desktop']) {
    const package_ = JSON.parse(await readFile(resolve(root, `apps/clearra-${app}/package.json`), 'utf8'));
    for (const task of ['sync', 'dev', 'build']) assert.equal(package_.scripts[task],
      `node ../../scripts/tools/build-clearra-frontend.mjs --app ${app} --task ${task}`);
    for (const task of ['predev', 'prebuild', 'pretest']) assert.equal(package_.scripts[task], undefined);
    const svelte = await readFile(resolve(root, `apps/clearra-${app}/svelte.config.js`), 'utf8');
    assert.match(svelte, /outDir: frontend\.kitOutDir/u);
    assert.match(svelte, /pages: frontend\.exportDir, assets: frontend\.exportDir/u);
    const vite = await readFile(resolve(root, `apps/clearra-${app}/vite.config.ts`), 'utf8');
    assert.match(vite, /requireOwner: true/u);
    assert.match(vite, /cacheDir: frontend\.viteCacheDir/u);
    assert.match(vite, /searchForWorkspaceRoot\(frontend\.appRoot\), frontend\.frontendRoot/u);
  }
  const helper = await readFile(new URL('./clearra-frontend-paths.mjs', import.meta.url), 'utf8');
  assert.match(helper, /JSON\.stringify\(\{ extends: resolve\(paths\.kitOutDir, 'tsconfig\.json'\) \}/u);
  assert.match(helper, /'git', \['-C', sourceRoot, 'ls-files', '-z'/u);
  const builder = await readFile(new URL('./build-clearra-frontend.mjs', import.meta.url), 'utf8');
  assert.match(builder, /enterManagedBuildOrRelaunch\(sourceRoot/u);
  assert.match(builder, /options\.task === 'dev' && purpose !== 'experiment'/u);
});

test('older snapshots fail closed before frontend compilation without source policy adoption', async () => {
  await validateManagedFrontendSource(root);
  for (const mutation of ['package', 'outDir', 'cacheDir', 'loader', 'launcher', 'missing']) {
    await assert.rejects(validateManagedFrontendSource(root, async path => {
      if (mutation === 'missing') throw Object.assign(new Error('missing'), { code: 'ENOENT' });
      let source = await readFile(resolve(root, path), 'utf8');
      if (mutation === 'package' && path.endsWith('package.json')) {
        const package_ = JSON.parse(source); package_.scripts.build = 'vite build'; source = JSON.stringify(package_);
      }
      if (mutation === 'outDir') source = source.replace('outDir: frontend.kitOutDir', "outDir: '.svelte-kit'");
      if (mutation === 'cacheDir') source = source.replace('cacheDir: frontend.viteCacheDir', "cacheDir: 'node_modules/.vite'");
      if (mutation === 'loader') source = source.replace("'--configLoader', 'runner'", "'--configLoader', 'bundle'");
      if (mutation === 'launcher') source = source.replace('Ensure-ClearraBuildArtifactCache -RepositoryRoot $source -Purpose $Purpose', '# missing snapshot owner');
      return source;
    }), /Unsupported frontend build policy snapshot/);
  }
  const workflow = await readFile(resolve(root, '.github/workflows/pages-rollback.yml'), 'utf8');
  const guard = workflow.indexOf('node authority-source/scripts/tools/validate-managed-frontend-source.mjs --source-root snapshot-source');
  assert.ok(guard > 0 && guard < workflow.indexOf('- name: Prepare Rust WASM toolchain'));
});
