#!/usr/bin/env node

import { createReadStream, lstatSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { basename, join, resolve } from 'node:path';

const RECEIPT = 'clearra-product-e2e-cli-input.v2.json';
const SCHEMA = 'clearra.product-e2e-cli-input.v2';
const RECIPE = Object.freeze({
  producer: 'windows-cli',
  package: 'clearra-cli',
  binary: 'clearra',
  profile: 'release',
  target_triple: 'x86_64-pc-windows-msvc',
  features: ['online-pc4-tablebase', 'wasm-cpu-runtime'],
});

function fail(message) {
  throw new Error(message);
}

async function fileRecord(path) {
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.size <= 0) fail(`artifact input must be a nonempty regular file: ${path}`);
  const hash = createHash('sha256');
  let sizeBytes = 0;
  for await (const bytes of createReadStream(path)) {
    hash.update(bytes);
    sizeBytes += bytes.length;
  }
  if (sizeBytes !== stat.size) fail(`artifact input changed length while hashing: ${path}`);
  return { sha256: hash.digest('hex'), size_bytes: sizeBytes };
}

function directoryEntries(directory) {
  if (!lstatSync(directory).isDirectory()) fail(`artifact must be a regular directory: ${directory}`);
  return readdirSync(directory);
}

function requireOnlyFile(directory, file, label) {
  const entries = directoryEntries(directory);
  if (entries.length !== 1 || entries[0] !== file) fail(`${label} file set mismatch: expected only ${file}`);
  return join(directory, file);
}

function parseJson(path, label) {
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.size > 16_384) fail(`${label} must be a regular file at most 16 KiB`);
  let value;
  try {
    value = JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    fail(`${label} is not valid JSON: ${error.message}`);
  }
  if (value === null || Array.isArray(value) || typeof value !== 'object') {
    fail(`${label} must be a JSON object`);
  }
  return value;
}

function exactKeys(value, expected, label) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) fail(`${label} must be a JSON object`);
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (JSON.stringify(actual) !== JSON.stringify(wanted)) {
    fail(`${label} keys mismatch: expected ${wanted.join(',')}, observed ${actual.join(',')}`);
  }
}

function exactCommit(value, label) {
  if (!/^[0-9a-f]{40}$/u.test(value ?? '')) fail(`${label} must be an exact lowercase commit SHA`);
  return value;
}

function exactPositiveInteger(value, label) {
  if (!/^[1-9][0-9]{0,19}$/u.test(String(value ?? ''))) fail(`${label} must be a positive decimal integer`);
  return String(value);
}

function exactVersion(value) {
  if (!/^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$/u.test(value ?? '')) {
    fail('version must be a release version without a v prefix');
  }
  return value;
}

function shippingFile(version) {
  return `Clearra-CLI-v${version}-windows-x86_64.exe`;
}

function parseArgs(argv) {
  const [mode, ...rest] = argv;
  if (!['seal', 'verify'].includes(mode)) fail('mode must be seal or verify');
  const allowed = mode === 'seal'
    ? new Set(['--binary', '--version', '--source-commit', '--run-id', '--run-attempt', '--output'])
    : new Set(['--artifact', '--receipt', '--expected-version', '--expected-source-commit',
      '--expected-run-id', '--expected-run-attempt', '--github-output']);
  const values = {};
  for (let index = 0; index < rest.length; index += 2) {
    const name = rest[index];
    const value = rest[index + 1];
    if (!allowed.has(name) || typeof value !== 'string' || value.length === 0) fail(`invalid ${mode} option: ${name ?? '<missing>'}`);
    if (Object.hasOwn(values, name)) fail(`duplicate ${mode} option: ${name}`);
    values[name] = value;
  }
  for (const name of allowed) if (!Object.hasOwn(values, name)) fail(`${name} is required`);
  return { mode, values };
}

async function seal(values) {
  const version = exactVersion(values['--version']);
  const sourceCommit = exactCommit(values['--source-commit'], 'source commit');
  const runId = exactPositiveInteger(values['--run-id'], 'run ID');
  const runAttempt = exactPositiveInteger(values['--run-attempt'], 'run attempt');
  const binaryPath = resolve(values['--binary']);
  const output = resolve(values['--output']);
  const filename = shippingFile(version);
  if (basename(binaryPath) !== filename) fail(`shipping artifact name mismatch: expected ${filename}`);
  const observed = await fileRecord(binaryPath);
  mkdirSync(output, { recursive: true });
  if (directoryEntries(output).length !== 0) fail(`artifact output must be empty: ${output}`);
  const receipt = {
    schema_version: SCHEMA,
    authority: 'non-authoritative-product-input',
    release_version: version,
    source_commit: sourceCommit,
    workflow_run_id: runId,
    workflow_run_attempt: runAttempt,
    recipe: RECIPE,
    files: { [filename]: observed },
  };
  writeFileSync(join(output, RECEIPT), `${JSON.stringify(receipt, null, 2)}\n`, { encoding: 'utf8', flag: 'wx' });
  process.stdout.write(`product_e2e_cli_artifact=sealed source_commit=${sourceCommit} run_id=${runId} run_attempt=${runAttempt} binary_sha256=${observed.sha256}\n`);
}

async function verify(values) {
  const artifact = resolve(values['--artifact']);
  const receiptDirectory = resolve(values['--receipt']);
  const version = exactVersion(values['--expected-version']);
  const expectedCommit = exactCommit(values['--expected-source-commit'], 'expected source commit');
  const expectedRunId = exactPositiveInteger(values['--expected-run-id'], 'expected run ID');
  const expectedRunAttempt = exactPositiveInteger(values['--expected-run-attempt'], 'expected run attempt');
  const filename = shippingFile(version);
  const binaryPath = requireOnlyFile(artifact, filename, 'shipping CLI artifact');
  const receiptPath = requireOnlyFile(receiptDirectory, RECEIPT, 'product CLI receipt');
  const receipt = parseJson(receiptPath, 'product CLI receipt');
  exactKeys(receipt, [
    'schema_version', 'authority', 'release_version', 'source_commit', 'workflow_run_id',
    'workflow_run_attempt', 'recipe', 'files',
  ], 'product CLI receipt');
  if (receipt.schema_version !== SCHEMA) fail('product CLI receipt schema mismatch');
  if (receipt.authority !== 'non-authoritative-product-input') fail('product CLI receipt authority mismatch');
  if (receipt.release_version !== version) fail('product CLI receipt release version mismatch');
  if (receipt.source_commit !== expectedCommit) fail('product CLI receipt source mismatch');
  if (receipt.workflow_run_id !== expectedRunId) fail('product CLI receipt run ID mismatch');
  if (receipt.workflow_run_attempt !== expectedRunAttempt) fail('product CLI receipt run attempt mismatch');
  if (JSON.stringify(receipt.recipe) !== JSON.stringify(RECIPE)) fail('product CLI build recipe mismatch');
  exactKeys(receipt.files, [filename], 'product CLI receipt files');
  const record = receipt.files[filename];
  exactKeys(record, ['sha256', 'size_bytes'], 'product CLI executable record');
  const observed = await fileRecord(binaryPath);
  if (record.sha256 !== observed.sha256 || record.size_bytes !== observed.size_bytes) {
    fail('shipping CLI artifact digest or size mismatch');
  }

  const githubOutput = resolve(values['--github-output']);
  if (/[\r\n]/u.test(binaryPath)) fail('shipping CLI path must be a single line');
  writeFileSync(githubOutput, `binary_path=${binaryPath}\n`, { encoding: 'utf8', flag: 'a' });
  process.stdout.write(`product_e2e_cli_artifact=verified source_commit=${expectedCommit} run_id=${expectedRunId} run_attempt=${expectedRunAttempt} binary_sha256=${observed.sha256}\n`);
}

const { mode, values } = parseArgs(process.argv.slice(2));
if (mode === 'seal') await seal(values);
else await verify(values);
