import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export async function validateManagedFrontendSource(sourceRoot, readSource = path => readFile(resolve(sourceRoot, path), 'utf8')) {
  const unsupported = reason => new Error(`Unsupported frontend build policy snapshot: ${reason}. Merge the managed build policy before building this source.`);
  try {
    const package_ = JSON.parse(await readSource('apps/clearra-web/package.json'));
    for (const task of ['build', 'dev', 'sync', 'test']) {
      if (package_.scripts?.[task] !== `node ../../scripts/tools/build-clearra-frontend.mjs --app web --task ${task}`) {
        throw unsupported(`web ${task} has no managed owner`);
      }
    }
    if (['prebuild', 'predev', 'pretest'].some(key => package_.scripts[key])) throw unsupported('split npm owners');
    for (const [path, markers] of [
      ['apps/clearra-web/svelte.config.js', ['frontendConfigPaths(\'web\')', 'outDir: frontend.kitOutDir']],
      ['apps/clearra-web/vite.config.ts', ["frontendPaths('web', { requireOwner: true })", 'cacheDir: frontend.viteCacheDir']],
      ['scripts/tools/build-clearra-frontend.mjs', ['enterManagedBuildOrRelaunch(sourceRoot', "'--configLoader', 'runner'", 'writeFrontendTypeForwarder(paths)']],
      ['scripts/tools/invoke-clearra-build.ps1', ['Ensure-ClearraBuildArtifactCache -RepositoryRoot $source -Purpose $Purpose', 'Complete-ClearraBuildTransaction', 'Exit-ClearraBuildArtifactCacheUsage']],
      ['scripts/tools/clearra-frontend-paths.mjs', ['assertManagedBuildTransaction', "'frontend', app", "'svelte-kit'", "'vite-cache'"]],
    ]) {
      const source = (await readSource(path)).replace(/^\s*\/\/[^\n]*(?:\n|$)/gmu, '');
      if (markers.some(marker => !source.includes(marker))) throw unsupported(path);
    }
  } catch (error) {
    if (error.message.startsWith('Unsupported frontend build policy snapshot:')) throw error;
    throw unsupported(error.code || 'missing or malformed source contract');
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.length !== 4 || process.argv[2] !== '--source-root') throw new Error('Expected --source-root <snapshot>');
  await validateManagedFrontendSource(resolve(process.argv[3]));
  process.stdout.write('managed_frontend_source=supported\n');
}
