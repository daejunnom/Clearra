import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { assertManagedBuildTransaction, buildPathIdentity } from './clearra-build-policy.mjs';

// The managed wrapper validates paths and then forwards rustc unchanged. It is
// not a performance override, but arbitrary compiler wrappers still are.
export function assertDefaultBenchmarkRustEnvironment(keys, sourceRoot, environment = process.env) {
  const owner = assertManagedBuildTransaction({ sourceRoot, environment });
  const guard = process.platform === 'win32'
    ? resolve(owner.transaction, 'build-tools/clearra-rustc-guard.exe')
    : fileURLToPath(new URL('./clearra-rustc-guard.sh', import.meta.url));
  const configured = keys.filter(key => {
    const value = String(environment[key] ?? '');
    if (!value) return false;
    return key !== 'RUSTC_WRAPPER' || buildPathIdentity(value) !== buildPathIdentity(guard);
  });
  if (configured.length) {
    throw new Error(`benchmark Rust build environment must be default; unset ${configured.join(', ')}`);
  }
}
