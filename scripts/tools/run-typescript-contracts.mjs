import { mkdtemp, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { build } from 'esbuild';

const invocationDirectory = process.cwd();
const repositoryRoot = resolve(fileURLToPath(new URL('../..', import.meta.url)));
const inputs = process.argv.slice(2);

if (inputs.length === 0) {
  throw new Error('at least one TypeScript contract file or directory is required');
}

const contractFiles = [];
for (const input of inputs) {
  const path = resolve(invocationDirectory, input);
  if (path.endsWith('.contract.ts')) {
    contractFiles.push(path);
    continue;
  }
  for (const entry of await readdir(path, { withFileTypes: true })) {
    if (entry.isFile() && entry.name.endsWith('.contract.ts')) {
      contractFiles.push(resolve(path, entry.name));
    }
  }
}

contractFiles.sort((left, right) => left.localeCompare(right, 'en'));
if (contractFiles.length === 0) {
  throw new Error('no TypeScript contract files were found');
}

process.chdir(repositoryRoot);
const bundleDirectory = await mkdtemp(join(tmpdir(), 'clearra-typescript-contracts-'));
try {
  for (const [index, contractFile] of contractFiles.entries()) {
    const bundle = await build({
      absWorkingDir: repositoryRoot,
      bundle: true,
      entryPoints: [contractFile],
      format: 'esm',
      logLevel: 'silent',
      platform: 'node',
      target: 'node22',
      write: false
    });
    if (bundle.outputFiles.length !== 1) {
      throw new Error(`TypeScript contract produced ${bundle.outputFiles.length} outputs`);
    }
    const bundlePath = join(bundleDirectory, `contract-${index}.mjs`);
    await writeFile(bundlePath, bundle.outputFiles[0].contents);
    await import(pathToFileURL(bundlePath).href);
    process.stdout.write(`typescript_contract=passed file=${contractFile}\n`);
  }
} finally {
  await rm(bundleDirectory, { force: true, recursive: true });
}
