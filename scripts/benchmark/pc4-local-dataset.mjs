// Opt-in, local-only benchmark data. Reuses the product's explicit downloader;
// never installs into the CLI/OPFS stores or changes their active generation.
import { createHash, randomUUID } from 'node:crypto';
import { mkdir, open, readFile, lstat, realpath, rename, unlink, rmdir } from 'node:fs/promises';
import { isAbsolute, join, resolve, parse } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { qualifyPc4UpstreamGeneration } from '../release/pc4/qualify-upstream-generation.mjs';
import { downloadPc4Profile, pc4DownloadPlan } from '../release/pc4/pc4-download.mjs';

export async function checkedDatasetRoot(directory, profile, create = false) {
  if (!isAbsolute(directory ?? '') || resolve(directory) === parse(resolve(directory)).root ||
      !['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick'].includes(profile)) {
    throw new Error('Supply an absolute dedicated --directory and explicit --profile');
  }
  const base = resolve(directory);
  // Reject aliases before creating or removing any managed child.
  let current = parse(base).root;
  for (const part of base.slice(current.length).split(/[\\/]/).filter(Boolean)) {
    current = join(current, part);
    try { if ((await lstat(current)).isSymbolicLink()) throw new Error('Dataset directory must not use links'); }
    catch (e) { if (e.code !== 'ENOENT') throw e; }
  }
  if (create) await mkdir(base, { recursive: true });
  if ((await realpath(base)).toLowerCase() !== base.toLowerCase()) throw new Error('Dataset directory alias');
  const root = join(base, profile);
  if (create) await mkdir(root, { recursive: true });
  if (!(await lstat(root)).isDirectory() || (await lstat(root)).isSymbolicLink()) throw new Error('Invalid profile directory');
  return root;
}

export async function openBenchmarkDataset(directory, profile) {
  const root = await checkedDatasetRoot(directory, profile);
  const marker = join(root, 'active.json');
  if ((await lstat(marker)).isSymbolicLink() || (await lstat(marker)).size > 131072) throw new Error('Invalid benchmark pointer');
  const active = JSON.parse(await readFile(marker, 'utf8'));
  if (active.schema !== 'clearra.pc4.benchmark-files.v1' || !/^gen-[0-9a-f-]{36}$/.test(active.directory)) throw new Error('Invalid benchmark pointer');
  const plan = pc4DownloadPlan(active.generation, profile);
  const data = join(root, active.directory);
  if ((await lstat(data)).isSymbolicLink()) throw new Error('Invalid benchmark generation');
  const handles = new Map();
  try {
    for (const artifact of plan.files) {
      const path = join(data, artifact.path);
      if ((await lstat(path)).isSymbolicLink()) throw new Error('Dataset file link rejected');
      const handle = await open(path, 'r');
      handles.set(artifact.path, { artifact, handle });
      if ((await handle.stat()).size !== artifact.byte_length) throw new Error('Dataset size mismatch');
    }
  } catch (error) { await Promise.all([...handles.values()].map(f => f.handle.close())); throw error; }
  let calls = 0, bytes = 0;
  return {
    generation: active.generation, plan,
    get calls() { return calls; }, get bytes() { return bytes; },
    async close() { await Promise.all([...handles.values()].map(f => f.handle.close())); },
    async read(artifact, offset, length) {
      const found = handles.get(artifact.path);
      if (!found || found.artifact.byte_length !== artifact.byte_length || found.artifact.content_identity !== artifact.content_identity ||
          !Number.isSafeInteger(offset) || !Number.isSafeInteger(length) || offset < 0 || length < 1 || length > 65536 ||
          offset > artifact.byte_length - length) throw new Error('Dataset slice mismatch');
      const result = new Uint8Array(length);
      const { bytesRead } = await found.handle.read(result, 0, length, offset);
      calls++; bytes += bytesRead;
      if (bytesRead !== length) throw new Error('Dataset truncated');
      return result;
    }
  };
}

async function install(directory, profile) {
  const root = await checkedDatasetRoot(directory, profile, true);
  // Existing data is intentionally not replaced or cleaned by an experiment.
  try { await lstat(join(root, 'active.json')); throw new Error('Dataset already installed; reuse it'); }
  catch (e) { if (e.code !== 'ENOENT') throw e; }
  const lock = await open(join(root, 'download.lock'), 'wx');
  try {
    const generation = await qualifyPc4UpstreamGeneration();
    const plan = pc4DownloadPlan(generation, profile);
    console.log(JSON.stringify({ event: 'download-start', profile, revision: plan.revision, bytes: plan.totalBytes }));
    let lastReport = 0;
    const result = await downloadPc4Profile(generation, {
      async begin() {
        const name = `gen-${randomUUID()}`, stage = join(root, name), created = [];
        await mkdir(stage);
        let committed = false;
        return {
          async open(file) {
            const path = join(stage, file.path), handle = await open(path, 'wx');
            created.push(path);
            return {
              async write(bytes) { let offset = 0; while (offset < bytes.length) { const n = await handle.write(bytes.subarray(offset)); if (!n.bytesWritten) throw new Error('Short dataset write'); offset += n.bytesWritten; } },
              close: () => handle.close(), abort: () => handle.close()
            };
          },
          async commit(pinned) {
            const pending = join(root, 'active.pending');
            const output = await open(pending, 'wx');
            try { await output.writeFile(JSON.stringify({ schema: 'clearra.pc4.benchmark-files.v1', directory: name, generation: pinned })); await output.sync(); }
            finally { await output.close(); }
            await rename(pending, join(root, 'active.json'));
            committed = true;
          },
          async abort() {
            if (committed) return;
            // Only exact paths exclusively created by this transaction. No recursive removal.
            for (const path of created) await unlink(path);
            await rmdir(stage);
          }
        };
      }
    }, { intent: 'explicit-download', profile,
      hasherFactory() { const hash = createHash('sha256'); return { update: bytes => hash.update(bytes), hex: () => hash.digest('hex') }; },
      onProgress(p) { if (Date.now() - lastReport >= 10000) { lastReport = Date.now(); console.log(JSON.stringify({ event: 'download-progress', ...p })); } }
    });
    console.log(JSON.stringify({ event: 'download-complete', ...result }));
  } finally { await lock.close(); await unlink(join(root, 'download.lock')); }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const { values } = parseArgs({ options: { directory: { type: 'string' }, profile: { type: 'string' }, download: { type: 'boolean' } } });
  if (!values.download) throw new Error('Full download requires --download --profile PROFILE --directory DIRECTORY');
  await install(values.directory, values.profile);
}
