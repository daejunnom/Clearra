import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { enterManagedBuildOrRelaunch } from './clearra-build-policy.mjs';
import { frontendPaths, stageFrontendPublicAssets, writeFrontendTypeForwarder } from './clearra-frontend-paths.mjs';

const self = fileURLToPath(import.meta.url);
const sourceRoot = resolve(dirname(self), '..', '..');

export function frontendOptions(args) {
  const { values } = parseArgs({ args, strict: true, options: {
    app: { type: 'string' }, task: { type: 'string', default: 'build' },
    environment: { type: 'string', default: 'native' },
    port: { type: 'string' }, mode: { type: 'string' },
    host: { type: 'string', default: '127.0.0.1' },
    strictPort: { type: 'boolean' }, recovery: { type: 'boolean', default: false },
  } });
  if (!['web', 'desktop'].includes(values.app) || !['build', 'dev', 'sync', 'test'].includes(values.task) ||
      !['native', 'wsl'].includes(values.environment)) throw new Error('Invalid frontend app, task, or WASM environment');
  if (values.task === 'test' && values.app !== 'web') throw new Error('Only the web frontend owns the worker contract test task');
  if (values.host !== '127.0.0.1') throw new Error('Clearra development servers bind only to loopback');
  if (values.port && (!/^\d+$/u.test(values.port) || Number(values.port) < 1 || Number(values.port) > 65535)) {
    throw new Error('Invalid frontend port');
  }
  if (values.recovery && (values.app !== 'web' || values.task !== 'dev' || values.mode !== 'local-recovery')) {
    throw new Error('Recovery only serves existing web WASM in local-recovery mode');
  }
  return values;
}

export function frontendPlan(options, paths) {
  const commands = [{ kind: 'sync', arguments: ['sync'] }];
  if (options.task === 'sync') return commands;
  if (options.task === 'test') return [...commands,
    { kind: 'typecheck', arguments: ['--noEmit', '-p', 'tsconfig.contract.json'] },
    { kind: 'contracts', arguments: ['./test'] },
  ];
  if (options.app === 'web' && !options.recovery) {
    commands.push({ kind: 'public-assets', arguments: [] });
    commands.push({ kind: 'wasm', arguments: ['--environment', options.environment,
      '--destination', resolve(paths.publicDir, 'wasm')] });
  }
  const arguments_ = options.task === 'build' ? ['build'] : ['--host', options.host];
  // The default bundle loader writes node_modules/.vite-temp before config hooks.
  arguments_.push('--configLoader', 'runner');
  if (options.task === 'dev') {
    const port = options.port || (options.app === 'web' ? '4194' : null);
    if (port) arguments_.push('--port', port);
    if (options.app === 'web' || options.strictPort) arguments_.push('--strictPort');
  }
  if (options.mode) arguments_.push('--mode', options.mode);
  commands.push({ kind: 'vite', arguments: arguments_ });
  if (options.task === 'build' && options.app === 'web') commands.push({ kind: 'fallback', arguments: [] });
  return commands;
}

export async function executeFrontendPlan(commands, { run, afterSync }) {
  for (const command of commands) {
    await run(command);
    if (command.kind === 'sync') await afterSync();
  }
}

async function runNode(script, arguments_, environment, cwd) {
  const child = spawn(process.execPath, [script, ...arguments_], {
    cwd, env: environment, stdio: 'inherit', windowsHide: true,
  });
  const interrupt = () => child.kill('SIGINT');
  const terminate = () => child.kill('SIGTERM');
  process.on('SIGINT', interrupt);
  process.on('SIGTERM', terminate);
  try {
    const code = await new Promise((accept, reject) => {
      child.once('error', reject);
      child.once('exit', (code, signal) => accept(code ?? (signal === 'SIGINT' ? 130 : 1)));
    });
    if (code !== 0) throw new Error(`Frontend command ${script} failed (${code})`);
  } finally {
    process.off('SIGINT', interrupt);
    process.off('SIGTERM', terminate);
  }
}

export async function main(args = process.argv.slice(2)) {
  const options = frontendOptions(args);
  const purpose = process.env.CLEARRA_BUILD_PURPOSE || (options.task === 'build' ? 'product' : 'experiment');
  if (options.task === 'dev' && purpose !== 'experiment') throw new Error('A development server must own the source experiment slot');
  enterManagedBuildOrRelaunch(sourceRoot, [self, ...args], purpose);
  const paths = frontendPaths(options.app, { requireOwner: true });
  const require = createRequire(resolve(paths.appRoot, 'package.json'));
  const scripts = {
    sync: resolve(dirname(require.resolve('@sveltejs/kit/package.json')), 'svelte-kit.js'),
    vite: resolve(dirname(require.resolve('vite/package.json')), 'bin/vite.js'),
    wasm: resolve(sourceRoot, 'scripts/tools/build-clearra-wasm.mjs'),
    fallback: resolve(paths.appRoot, 'scripts/prepare-pages-fallback.mjs'),
    contracts: resolve(sourceRoot, 'scripts/tools/run-typescript-contracts.mjs'),
  };
  if (options.task === 'test') scripts.typecheck = resolve(dirname(require.resolve('typescript/package.json')), 'bin/tsc');
  // Login recovery preserves the already published runtime bytes; ordinary
  // builds/dev publish their private WASM into this same owner's public stage.
  const environment = { ...process.env };
  if (options.app === 'web' && ['build', 'dev'].includes(options.task) && !options.recovery) environment.CLEARRA_WEB_PUBLIC_DIR = paths.publicDir;
  await executeFrontendPlan(frontendPlan(options, paths), {
    run: command => command.kind === 'public-assets' ? stageFrontendPublicAssets(paths)
      : runNode(scripts[command.kind], command.arguments, environment, paths.appRoot),
    afterSync: () => writeFrontendTypeForwarder(paths),
  });
}

if (process.argv[1] && resolve(process.argv[1]) === self) {
  await main().catch(error => { process.stderr.write(`${error.message}\n`); process.exitCode = 1; });
}
