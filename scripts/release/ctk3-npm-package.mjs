import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ACCEPTED_CTK3_MANIFEST } from '../tools/accepted-ctk3-dist.mjs';

// Validate the npm pack result against the already verified distribution.
// Publishing never invokes prepack, recompiles it, or executes package scripts.
export function validateCtk3NpmPackage(metadata, report, accepted) {
  if (metadata?.name !== 'ctk3' ||
      !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/u.test(metadata.version ?? '') ||
      metadata.repository?.url !== 'git+https://github.com/daejunnom/Clearra.git' ||
      metadata.repository?.directory !== 'packages/ctk3' ||
      metadata.publishConfig?.access !== 'public') {
    throw new Error('CTK3 npm package metadata does not match its public repository');
  }
  const packed = Array.isArray(report) && report.length === 1 ? report[0] : null;
  const filename = `ctk3-${metadata.version}.tgz`;
  if (packed?.name !== metadata.name || packed.version !== metadata.version ||
      packed.filename !== filename || !Array.isArray(packed.files)) {
    throw new Error('npm pack must produce exactly one matching CTK3 package');
  }
  if (!Array.isArray(accepted?.files) || accepted.files.length === 0) {
    throw new Error('Verified CTK3 distribution file list is missing');
  }
  const expected = new Set([
    'package.json', 'README.md', 'LICENSE', `dist/${ACCEPTED_CTK3_MANIFEST}`,
    ...accepted.files.map(file => `dist/${file.path}`),
  ]);
  const observed = packed.files.map(file => file.path);
  if (new Set(observed).size !== observed.length || observed.length !== expected.size ||
      observed.some(path => !expected.has(path))) {
    throw new Error('npm package differs from the accepted CTK3 distribution and public metadata');
  }
  return filename;
}

async function main() {
  const root = resolve(fileURLToPath(new URL('../..', import.meta.url)));
  const readJson = async path => JSON.parse(await readFile(resolve(root, path), 'utf8'));
  const filename = validateCtk3NpmPackage(
    await readJson('packages/ctk3/package.json'),
    await readJson('build/npm/pack.json'),
    await readJson(`packages/ctk3/dist/${ACCEPTED_CTK3_MANIFEST}`),
  );
  console.log(`package_filename=${filename}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
