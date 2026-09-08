#!/usr/bin/env node
// Transport accepted product bytes separately from the unchanged Git source
// archive. Neither this producer nor its Cloud consumer may compile a product.
import { createHash } from 'node:crypto';
import { constants, createReadStream } from 'node:fs';
import { chmod, copyFile, lstat, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { canonicalJson, requireExactKeys } from '../canonical-release-evidence.mjs';
import { validateCanonicalAcceptanceEvidence } from '../canonical-acceptance-evidence.mjs';
import { ACCEPTED_CTK3_MANIFEST, verifyAcceptedCtk3Dist } from '../../tools/accepted-ctk3-dist.mjs';

export const CLOUD_INPUT_MANIFEST = 'clearra-cloud-build-inputs.v1.json';
export const CLOUD_INPUT_SCHEMA = 'clearra.cloud-build-inputs.v1';
const SHA = /^[0-9a-f]{40}$/u;
const HASH = /^[0-9a-f]{64}$/u;
const ID = /^[1-9][0-9]{0,19}$/u;
const FIXED_FILES = ['canonical-acceptance.json', 'clearra', 'exact-source.tar.gz'];

export async function sha256File(path) {
  await regularFile(path);
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest('hex');
}

export async function readCloudInputManifest(path, expectedHash) {
  await regularFile(path);
  const raw = await readFile(path);
  const hash = createHash('sha256').update(raw).digest('hex');
  if (!HASH.test(expectedHash ?? '') || hash !== expectedHash) {
    throw new Error('Cloud input manifest SHA-256 differs');
  }
  const manifest = JSON.parse(raw.toString('utf8'));
  requireExactKeys(manifest, ['schema_id', 'repository', 'version', 'base_path',
    'source_commit', 'accepted_run_id', 'accepted_run_attempt', 'files'], 'Cloud input manifest');
  if (manifest.schema_id !== CLOUD_INPUT_SCHEMA || !SHA.test(manifest.source_commit) ||
      !ID.test(manifest.accepted_run_id) || manifest.accepted_run_attempt !== '1' ||
      !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u.test(manifest.repository) ||
      !/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/u.test(manifest.version) ||
      manifest.base_path !== `/${manifest.repository.split('/')[1]}` ||
      raw.toString('utf8') !== `${canonicalJson(manifest)}\n`) {
    throw new Error('Cloud input manifest identity or encoding is invalid');
  }
  if (!Array.isArray(manifest.files) || manifest.files.length < 8) {
    throw new Error('Cloud input manifest file set is incomplete');
  }
  const paths = new Set();
  for (const entry of manifest.files) {
    requireExactKeys(entry, ['path', 'size_bytes', 'sha256'], 'Cloud input file');
    if (!safePayloadPath(entry.path) || paths.has(entry.path) || !HASH.test(entry.sha256) ||
        !Number.isSafeInteger(entry.size_bytes) || entry.size_bytes < 0) {
      throw new Error('Cloud input file identity is invalid');
    }
    paths.add(entry.path);
  }
  if ([...FIXED_FILES, `ctk3/${ACCEPTED_CTK3_MANIFEST}`].some((path) => !paths.has(path))) {
    throw new Error('Cloud input manifest omitted a required file');
  }
  return manifest;
}

export async function verifyAcceptedCloudInputs(rootPath, expected) {
  const root = resolve(rootPath);
  await safeDirectoryChain(root);
  const manifest = await readCloudInputManifest(resolve(root, CLOUD_INPUT_MANIFEST), expected.manifestSha256);
  if (manifest.source_commit !== expected.sourceCommit ||
      manifest.accepted_run_id !== expected.runId || manifest.accepted_run_attempt !== expected.runAttempt) {
    throw new Error('Cloud input source/run/attempt differs');
  }
  const files = await inventory(root);
  if (canonicalJson(files) !== canonicalJson(manifest.files)) {
    throw new Error('Cloud input payload differs from the closed file set and hashes');
  }
  await validateProducts(root, manifest);
  return manifest;
}

export async function createAcceptedCloudInputs(options) {
  const { sourceCommit, runId, runAttempt, repository, version, basePath } = options;
  const authority = { sourceCommit, runId, runAttempt, repository, version, basePath };
  const evidencePath = resolve(options.canonicalAcceptanceEvidencePath);
  await regularFile(evidencePath);
  const evidence = JSON.parse(await readFile(evidencePath, 'utf8'));
  validateCanonicalAcceptanceEvidence(evidence, authority);
  const ctk3Root = resolve(options.acceptedCtk3DistPath);
  await safeDirectoryChain(ctk3Root);
  const ctk3 = await verifyAcceptedCtk3Dist(ctk3Root, sourceCommit, runId, runAttempt);
  checkCtk3Authority(ctk3, evidence);
  const cliRoot = resolve(options.linuxCliDirectory);
  await safeDirectoryChain(cliRoot);
  const name = `Clearra-CLI-v${version}-linux-x86_64`;
  const cliEntries = await readdir(cliRoot);
  if (cliEntries.length !== 1 || cliEntries[0] !== name) throw new Error('Accepted Linux CLI artifact file set differs');
  const cliPath = resolve(cliRoot, name);
  await checkCli(cliPath, evidence, version);
  await regularFile(options.exactSourceArchivePath);

  const root = resolve(options.outputDirectory);
  await safeDirectoryChain(dirname(root));
  await mkdir(root); // Never merge with or overwrite an earlier transport.
  for (const [from, to] of [[evidencePath, 'canonical-acceptance.json'],
    [cliPath, 'clearra'], [options.exactSourceArchivePath, 'exact-source.tar.gz']]) {
    await copyFile(from, resolve(root, to), constants.COPYFILE_EXCL);
  }
  await chmod(resolve(root, 'clearra'), 0o755);
  for (const name of [ACCEPTED_CTK3_MANIFEST, ...ctk3.files.map((file) => file.path)]) {
    const path = `ctk3/${name}`;
    if (!safePayloadPath(path)) throw new Error('Unsafe accepted CTK3 transport path');
    const to = resolve(root, path);
    await mkdir(dirname(to), { recursive: true });
    await copyFile(resolve(ctk3Root, name), to, constants.COPYFILE_EXCL);
  }
  const manifest = { schema_id: CLOUD_INPUT_SCHEMA, repository, version, base_path: basePath,
    source_commit: sourceCommit, accepted_run_id: runId, accepted_run_attempt: runAttempt,
    files: await inventory(root) };
  const path = resolve(root, CLOUD_INPUT_MANIFEST);
  await writeFile(path, `${canonicalJson(manifest)}\n`, { flag: 'wx', mode: 0o600 });
  const manifestSha256 = await sha256File(path);
  // Revalidate copied bytes, not only the inputs from before the copy.
  await verifyAcceptedCloudInputs(root, { sourceCommit, runId, runAttempt, manifestSha256 });
  if (await sha256File(options.exactSourceArchivePath) !==
      manifest.files.find((entry) => entry.path === 'exact-source.tar.gz').sha256) {
    throw new Error('Exact source archive changed during transport creation');
  }
  return { manifest, manifestSha256 };
}

async function validateProducts(root, manifest) {
  const evidence = JSON.parse(await readFile(resolve(root, 'canonical-acceptance.json'), 'utf8'));
  validateCanonicalAcceptanceEvidence(evidence, { repository: manifest.repository, version: manifest.version,
    basePath: manifest.base_path, sourceCommit: manifest.source_commit,
    runId: manifest.accepted_run_id, runAttempt: manifest.accepted_run_attempt });
  await checkCli(resolve(root, 'clearra'), evidence, manifest.version);
  const ctk3 = await verifyAcceptedCtk3Dist(resolve(root, 'ctk3'), manifest.source_commit,
    manifest.accepted_run_id, manifest.accepted_run_attempt);
  checkCtk3Authority(ctk3, evidence);
}

async function checkCli(path, evidence, version) {
  const entries = evidence.final_source_fragments.release_artifacts.filter((entry) => entry.role === 'linux-cli');
  const file = await regularFile(path);
  if (entries.length !== 1 || entries[0].name !== `Clearra-CLI-v${version}-linux-x86_64` ||
      entries[0].size_bytes !== file.size || entries[0].sha256 !== await sha256File(path)) {
    throw new Error('Linux CLI bytes differ from canonical acceptance');
  }
}

function checkCtk3Authority(manifest, evidence) {
  const hash = createHash('sha256').update(canonicalJson(manifest)).digest('hex');
  if (hash !== evidence.accepted_inputs.ctk3_manifest_sha256) {
    throw new Error('CTK3 manifest differs from canonical acceptance');
  }
}

function safePayloadPath(path) {
  return typeof path === 'string' && (FIXED_FILES.includes(path) ||
    /^ctk3\/(?:[A-Za-z0-9_-][A-Za-z0-9_.-]*\/)*[A-Za-z0-9_-][A-Za-z0-9_.-]*$/u.test(path));
}

async function inventory(root) {
  const result = [];
  async function walk(directory, prefix) {
    for (const name of await readdir(directory)) {
      const path = prefix ? `${prefix}/${name}` : name;
      if (path === CLOUD_INPUT_MANIFEST) continue;
      if (path !== 'ctk3' && !safePayloadPath(path)) throw new Error('Unexpected Cloud input path');
      const absolute = resolve(directory, name);
      const stat = await lstat(absolute);
      if (stat.isSymbolicLink()) throw new Error('Cloud input symlinks are forbidden');
      if (stat.isDirectory() && (path === 'ctk3' || path.startsWith('ctk3/'))) await walk(absolute, path);
      else if (stat.isFile()) result.push({ path, size_bytes: stat.size, sha256: await sha256File(absolute) });
      else throw new Error('Cloud input must be a regular file or CTK3 directory');
    }
  }
  await walk(root, '');
  return result.sort((a, b) => a.path.localeCompare(b.path, 'en'));
}

async function safeDirectoryChain(path) {
  for (let current = resolve(path);;) {
    const stat = await lstat(current);
    if (!stat.isDirectory() || stat.isSymbolicLink()) throw new Error('Cloud input directory uses a link or non-directory');
    const parent = dirname(current);
    if (parent === current) return;
    current = parent;
  }
}

async function regularFile(path) {
  await safeDirectoryChain(dirname(resolve(path)));
  const stat = await lstat(path);
  if (!stat.isFile() || stat.isSymbolicLink()) throw new Error('Cloud input must be a regular non-link file');
  return stat;
}

async function main() {
  const { values, positionals } = parseArgs({ allowPositionals: true, strict: true, options: Object.fromEntries([
    'source-commit', 'run-id', 'run-attempt', 'repository', 'version', 'base-path',
    'canonical-acceptance-evidence', 'accepted-ctk3-dist', 'linux-cli-directory',
    'exact-source-archive', 'output-directory', 'input-directory', 'manifest-sha256',
  ].map((key) => [key, { type: 'string' }])) });
  const expected = { sourceCommit: values['source-commit'], runId: values['run-id'], runAttempt: values['run-attempt'] };
  if (!SHA.test(expected.sourceCommit ?? '') || !ID.test(expected.runId ?? '') || expected.runAttempt !== '1' || positionals.length !== 1) {
    throw new Error('Cloud accepted-input command requires exact source/run/attempt');
  }
  if (positionals[0] === 'create') {
    const result = await createAcceptedCloudInputs({ ...expected, repository: values.repository, version: values.version,
      basePath: values['base-path'], canonicalAcceptanceEvidencePath: values['canonical-acceptance-evidence'],
      acceptedCtk3DistPath: values['accepted-ctk3-dist'], linuxCliDirectory: values['linux-cli-directory'],
      exactSourceArchivePath: values['exact-source-archive'], outputDirectory: values['output-directory'] });
    console.log(`cloud_inputs=sealed manifest_sha256=${result.manifestSha256}`);
  } else if (positionals[0] === 'verify') {
    await verifyAcceptedCloudInputs(values['input-directory'], { ...expected, manifestSha256: values['manifest-sha256'] });
    console.log('cloud_inputs=verified rebuild_count=0');
  } else throw new Error('Cloud accepted-input command must be create or verify');
}

if (resolve(process.argv[1] ?? '') === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(`cloud_inputs=failed reason=${error.message}`); process.exitCode = 2; });
}
