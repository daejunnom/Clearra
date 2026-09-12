import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { assertManagedBuildTransaction, assertNoBuildLinks, assertCargoOutputArguments } from './clearra-build-policy.mjs';

export function prepareRustcLauncher(environment = process.env) {
  const transaction = assertManagedBuildTransaction({ environment });
  const directory = resolve(transaction.transaction, 'build-tools');
  const output = resolve(directory, 'clearra-rustc-guard.exe');
  assertNoBuildLinks(output);
  if (existsSync(output)) return output; // Nested owners reuse this generation only.
  const here = dirname(fileURLToPath(import.meta.url));
  const args = [resolve(here, 'clearra-rustc-launcher.rs'), '--edition=2021', '--crate-name', 'clearra_rustc_launcher', '-C', 'debuginfo=0', '-o', output];
  assertCargoOutputArguments(args, transaction.transaction);
  mkdirSync(directory, { recursive: true });
  const result = spawnSync('rustc', args, { env: { ...process.env, ...environment,
    CLEARRA_LAUNCHER_NODE: process.execPath,
    CLEARRA_LAUNCHER_GUARD: resolve(here, 'clearra-rustc-guard.mjs'),
  }, stdio: 'inherit', windowsHide: true });
  if (result.error || result.status !== 0) throw new Error(`Cannot build native compiler guard: ${result.error?.message ?? result.status}`);
  return output;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { prepareRustcLauncher(); }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
