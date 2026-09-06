// Preserve only a source-verified WASM generation as unqualified CI feedback.
// This owner never builds, imports acceptance authority, or publishes a product.
import { appendFile, lstat, mkdir, realpath, writeFile } from 'node:fs/promises';
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { inspectVerifiedClearraWasmDirectory } from '../tools/import-verified-clearra-wasm.mjs';

const ROOT = resolve(fileURLToPath(new URL('../..', import.meta.url)));
const MANIFEST = 'clearra_wasm.manifest.json';

function overlaps(left, right) {
  const contains = (parent, child) => {
    const path = relative(parent, child);
    return path === '' || (path !== '..' && !path.startsWith(`..${sep}`) && !isAbsolute(path));
  };
  return contains(left, right) || contains(right, left);
}

export async function preserveCandidateWasm({
  sourceDirectory,
  outputDirectory,
  sourceCommit,
  repositoryRoot = ROOT,
}) {
  if (!/^[0-9a-f]{40}$/u.test(sourceCommit ?? '')) throw new Error('Candidate source must be an exact commit');
  const source = resolve(sourceDirectory);
  const manifest = await lstat(resolve(source, MANIFEST)).catch((error) => {
    if (error?.code !== 'ENOENT') throw error;
    return null;
  });
  if (!manifest) return Object.freeze({ ready: false, reason: 'wasm-not-built' });
  const inspected = await inspectVerifiedClearraWasmDirectory({ sourceDirectory: source, repositoryRoot });
  const identity = inspected.manifest.build.runtime_identity;
  if (identity.source_commit !== sourceCommit || identity.engine_build_id !== sourceCommit) {
    throw new Error('Candidate WASM runtime identity differs from the exact source');
  }
  // The parent must already exist. A fresh leaf, no overwrite and no recursive
  // enumeration/deletion, prevents a failed preservation from changing user data.
  const requested = resolve(outputDirectory);
  const parent = await realpath(dirname(requested));
  const output = resolve(parent, relative(dirname(requested), requested));
  if (overlaps(output, inspected.source) || overlaps(output, await realpath(repositoryRoot))) {
    throw new Error('Candidate output must be outside the source and repository');
  }
  await mkdir(output);
  for (const [name, bytes] of inspected.files) {
    await writeFile(resolve(output, name), bytes, { flag: 'wx' });
  }
  // Re-read exactly the staged five files and recheck the current source before
  // the workflow is allowed to upload. A partial/changed generation is not ready.
  const staged = await inspectVerifiedClearraWasmDirectory({ sourceDirectory: output, repositoryRoot });
  if (!staged.files.get(MANIFEST).equals(inspected.files.get(MANIFEST))) {
    throw new Error('Candidate WASM manifest changed during preservation');
  }
  return Object.freeze({ ready: true, copiedFiles: 5 });
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  try {
    const args = process.argv.slice(2);
    if (args.length !== 6 || args[0] !== '--from' || args[2] !== '--output' || args[4] !== '--source-commit') {
      throw new Error('usage: candidate-preflight-artifacts.mjs --from DIRECTORY --output DIRECTORY --source-commit SHA');
    }
    const result = await preserveCandidateWasm({
      sourceDirectory: args[1], outputDirectory: args[3], sourceCommit: args[5],
    });
    if (process.env.GITHUB_OUTPUT) await appendFile(process.env.GITHUB_OUTPUT, `ready=${result.ready}\n`);
    console.log(`candidate_wasm=${result.ready ? 'verified-unqualified' : 'not-built'} copied_files=${result.copiedFiles ?? 0} release_authority=false`);
  } catch (error) {
    console.error(`candidate_wasm=failed reason=${error.message}`);
    process.exitCode = 1;
  }
}
