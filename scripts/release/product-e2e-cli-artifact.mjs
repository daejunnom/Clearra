#!/usr/bin/env node

import { copyFileSync, lstatSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join, resolve } from 'node:path';

const RECEIPT = 'clearra-product-e2e-cli-input.v1.json';
const IDENTITY = 'clearra-native-build-identity.v1.json';
const BINARY = 'clearra.exe';
const SCHEMA = 'clearra.product-e2e-cli-input.v1';
const RECIPE = Object.freeze({
  package: 'clearra-cli',
  binary: 'clearra',
  profile: 'dev',
  target_triple: 'x86_64-pc-windows-msvc',
  features: ['native-c-core', 'webgpu-search'],
});

function fail(message) {
  throw new Error(message);
}

function sha256(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function fileRecord(path) {
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.isSymbolicLink()) fail(`artifact input must be a regular file: ${path}`);
  return { sha256: sha256(path), size_bytes: stat.size };
}

function parseJson(path, label) {
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

function parseArgs(argv) {
  const [mode, ...rest] = argv;
  if (!['seal', 'verify'].includes(mode)) fail('mode must be seal or verify');
  const allowed = mode === 'seal'
    ? new Set(['--binary', '--native-identity', '--source-commit', '--run-id', '--run-attempt', '--output'])
    : new Set(['--artifact', '--expected-source-commit', '--expected-run-id', '--expected-run-attempt', '--github-output']);
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

function validateNativeIdentity(identity, expectedCommit) {
  exactKeys(identity, [
    'schema_version', 'authority', 'source_commit', 'identity_sha256',
    'tracked_inputs_sha256', 'tracked_input_count', 'configuration',
    'native_archive', 'toolchain', 'runtime_paths',
  ], 'native identity');
  if (identity.schema_version !== 'clearra.native-build-identity.v1') fail('native identity schema mismatch');
  if (identity.authority !== 'non-authoritative-build-input') fail('native identity authority mismatch');
  if (identity.source_commit !== expectedCommit) fail('native identity source mismatch');
  if (!/^[0-9a-f]{64}$/u.test(identity.identity_sha256 ?? '')) fail('native identity digest is invalid');
  if (!/^[0-9a-f]{64}$/u.test(identity.tracked_inputs_sha256 ?? '')) fail('native tracked input digest is invalid');
  if (!Number.isInteger(identity.tracked_input_count) || identity.tracked_input_count < 1) fail('native tracked input count is invalid');
}

function seal(values) {
  const sourceCommit = exactCommit(values['--source-commit'], 'source commit');
  const runId = exactPositiveInteger(values['--run-id'], 'run ID');
  const runAttempt = exactPositiveInteger(values['--run-attempt'], 'run attempt');
  const sourceBinary = resolve(values['--binary']);
  const sourceIdentity = resolve(values['--native-identity']);
  const output = resolve(values['--output']);
  const identity = parseJson(sourceIdentity, 'native identity');
  validateNativeIdentity(identity, sourceCommit);

  mkdirSync(output, { recursive: true });
  if (readdirSync(output).length !== 0) fail(`artifact output must be empty: ${output}`);
  const binaryPath = join(output, BINARY);
  const identityPath = join(output, IDENTITY);
  copyFileSync(sourceBinary, binaryPath);
  copyFileSync(sourceIdentity, identityPath);
  const receipt = {
    schema_version: SCHEMA,
    authority: 'non-authoritative-product-input',
    source_commit: sourceCommit,
    workflow_run_id: runId,
    workflow_run_attempt: runAttempt,
    recipe: RECIPE,
    files: {
      [BINARY]: fileRecord(binaryPath),
      [IDENTITY]: fileRecord(identityPath),
    },
  };
  writeFileSync(join(output, RECEIPT), `${JSON.stringify(receipt, null, 2)}\n`, 'utf8');
  process.stdout.write(`product_e2e_cli_artifact=sealed source_commit=${sourceCommit} run_id=${runId} run_attempt=${runAttempt} binary_sha256=${receipt.files[BINARY].sha256}\n`);
}

function verify(values) {
  const artifact = resolve(values['--artifact']);
  const expectedCommit = exactCommit(values['--expected-source-commit'], 'expected source commit');
  const expectedRunId = exactPositiveInteger(values['--expected-run-id'], 'expected run ID');
  const expectedRunAttempt = exactPositiveInteger(values['--expected-run-attempt'], 'expected run attempt');
  const expectedEntries = [BINARY, IDENTITY, RECEIPT].sort();
  const entries = readdirSync(artifact).sort();
  if (JSON.stringify(entries) !== JSON.stringify(expectedEntries)) {
    fail(`artifact file set mismatch: expected ${expectedEntries.join(',')}, observed ${entries.join(',')}`);
  }

  const receipt = parseJson(join(artifact, RECEIPT), 'product CLI receipt');
  exactKeys(receipt, [
    'schema_version', 'authority', 'source_commit', 'workflow_run_id',
    'workflow_run_attempt', 'recipe', 'files',
  ], 'product CLI receipt');
  if (receipt.schema_version !== SCHEMA) fail('product CLI receipt schema mismatch');
  if (receipt.authority !== 'non-authoritative-product-input') fail('product CLI receipt authority mismatch');
  if (receipt.source_commit !== expectedCommit) fail('product CLI receipt source mismatch');
  if (String(receipt.workflow_run_id) !== expectedRunId) fail('product CLI receipt run ID mismatch');
  if (String(receipt.workflow_run_attempt) !== expectedRunAttempt) fail('product CLI receipt run attempt mismatch');
  if (JSON.stringify(receipt.recipe) !== JSON.stringify(RECIPE)) fail('product CLI build recipe mismatch');
  exactKeys(receipt.files, [BINARY, IDENTITY], 'product CLI receipt files');

  for (const name of [BINARY, IDENTITY]) {
    exactKeys(receipt.files[name], ['sha256', 'size_bytes'], `product CLI ${name} record`);
    const observed = fileRecord(join(artifact, name));
    if (receipt.files[name].sha256 !== observed.sha256 || receipt.files[name].size_bytes !== observed.size_bytes) {
      fail(`product CLI ${name} digest or size mismatch`);
    }
  }
  const identity = parseJson(join(artifact, IDENTITY), 'native identity');
  validateNativeIdentity(identity, expectedCommit);

  const binaryPath = join(artifact, BINARY);
  const githubOutput = resolve(values['--github-output']);
  writeFileSync(githubOutput, `binary_path=${binaryPath}\n`, { encoding: 'utf8', flag: 'a' });
  process.stdout.write(`product_e2e_cli_artifact=verified source_commit=${expectedCommit} run_id=${expectedRunId} run_attempt=${expectedRunAttempt} binary_sha256=${receipt.files[BINARY].sha256}\n`);
}

const { mode, values } = parseArgs(process.argv.slice(2));
if (mode === 'seal') seal(values);
else verify(values);
