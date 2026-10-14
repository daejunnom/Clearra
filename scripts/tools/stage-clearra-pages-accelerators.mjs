import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { assertBuildPathWithin, assertManagedBuildTransaction, assertNoBuildLinks } from './clearra-build-policy.mjs';
import { stageFrontendAcceleratorAssets } from './clearra-frontend-paths.mjs';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

// The acceptance leaf builds Vite directly to retain its already accepted WASM.
// Share the ordinary frontend's immutable mirror producer, without invoking a
// second frontend/WASM build or admitting an unowned output directory.
export async function stagePagesAccelerators(publicDirectory, {
  sourceRoot = repositoryRoot, environment = process.env,
  validateOwner = assertManagedBuildTransaction, fetcher = fetch,
} = {}) {
  const owner = validateOwner({ sourceRoot, environment });
  const publicDir = resolve(publicDirectory);
  assertBuildPathWithin(publicDir, owner.transaction);
  assertNoBuildLinks(publicDir);
  await stageFrontendAcceleratorAssets({ publicDir }, sourceRoot, fetcher);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const args = process.argv.slice(2);
    if (args.length !== 2 || args[0] !== '--public-dir' || !args[1]) {
      throw new Error('usage: node stage-clearra-pages-accelerators.mjs --public-dir DIRECTORY');
    }
    await stagePagesAccelerators(args[1]);
    console.log('pages_accelerators=staged source=signed-qualified-catalog wasm_build_count=0');
  } catch (error) {
    console.error(`pages_accelerators=failed reason=${error.message}`);
    process.exitCode = 1;
  }
}
