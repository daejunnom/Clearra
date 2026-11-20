import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  existsSync, mkdtempSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const script = fileURLToPath(new URL('./product-e2e-cli-artifact.mjs', import.meta.url));
const source = 'a'.repeat(40);
const version = '0.8.6';
const shippingName = `Clearra-CLI-v${version}-windows-x86_64.exe`;
const receiptName = 'clearra-product-e2e-cli-input.v2.json';
const shippingBytes = 'exact-binary-bytes';

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'clearra-product-cli-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const artifact = join(root, 'downloaded Windows CLI');
  const receipt = join(root, 'receipt');
  const binary = join(artifact, shippingName);
  const receiptFile = join(receipt, receiptName);
  const output = join(root, 'github-output.txt');
  mkdirSync(artifact);
  writeFileSync(binary, shippingBytes);
  return { binary, artifact, receipt, receiptFile, output };
}

function run(args) {
  const result = spawnSync(process.execPath, [script, ...args], {
    encoding: 'utf8',
    timeout: 10_000,
    windowsHide: true,
  });
  assert.ifError(result.error);
  return result;
}

function seal(paths, { binary = paths.binary } = {}) {
  return run(['seal',
    '--binary', binary,
    '--version', version,
    '--source-commit', source,
    '--run-id', '41',
    '--run-attempt', '2',
    '--output', paths.receipt,
  ]);
}

function verify(paths, overrides = {}) {
  return run(['verify',
    '--artifact', paths.artifact,
    '--receipt', paths.receipt,
    '--expected-version', overrides.version ?? version,
    '--expected-source-commit', overrides.source ?? source,
    '--expected-run-id', overrides.runId ?? '41',
    '--expected-run-attempt', overrides.runAttempt ?? '2',
    '--github-output', paths.output,
  ]);
}

function sealSuccessfully(paths) {
  const result = seal(paths);
  assert.equal(result.status, 0, result.stderr);
}

function assertVerifyRejected(paths, overrides) {
  const result = verify(paths, overrides);
  assert.ok(Number.isInteger(result.status) && result.status !== 0);
  assert.equal(existsSync(paths.output), false, 'failed verification must not authorize an executable');
}

test('seals only a receipt and returns the actual downloaded shipping CLI', (t) => {
  const paths = fixture(t);
  sealSuccessfully(paths);
  assert.deepEqual(readdirSync(paths.receipt), [receiptName]);
  const result = verify(paths);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(readFileSync(paths.output, 'utf8'), `binary_path=${paths.binary}\n`);
  assert.deepEqual(readdirSync(paths.artifact), [shippingName]);
  assert.equal(readFileSync(paths.binary, 'utf8'), shippingBytes);
  assert.deepEqual(readdirSync(paths.receipt), [receiptName], 'verification must not copy the executable');
});

test('rejects different bytes of the same size without rebuilding or falling back', (t) => {
  const paths = fixture(t);
  sealSuccessfully(paths);
  const mutated = 'X' + shippingBytes.slice(1);
  writeFileSync(paths.binary, mutated);
  assertVerifyRejected(paths);
  assert.equal(readFileSync(paths.binary, 'utf8'), mutated);
});

test('rejects version, source, run, and attempt mismatches', (t) => {
  const paths = fixture(t);
  sealSuccessfully(paths);
  for (const overrides of [
    { version: '0.8.7' }, { source: 'd'.repeat(40) }, { runId: '42' }, { runAttempt: '3' },
  ]) {
    assertVerifyRejected(paths, overrides);
  }
});

test('rejects a nonshipping executable name at seal and verify', (t) => {
  const sealing = fixture(t);
  const unexpected = join(sealing.artifact, 'clearra.exe');
  renameSync(sealing.binary, unexpected);
  const result = seal(sealing, { binary: unexpected });
  assert.ok(Number.isInteger(result.status) && result.status !== 0);
  assert.equal(existsSync(sealing.receiptFile), false);

  const verifying = fixture(t);
  sealSuccessfully(verifying);
  renameSync(verifying.binary, join(verifying.artifact, 'clearra.exe'));
  assertVerifyRejected(verifying);
});

test('rejects extra files in either artifact directory', (t) => {
  for (const name of ['artifact', 'receipt']) {
    const paths = fixture(t);
    sealSuccessfully(paths);
    writeFileSync(join(paths[name], 'extra.txt'), 'extra');
    assertVerifyRejected(paths);
  }
});

test('cannot use the historical C ProductE2E receipt as shipping authority', (t) => {
  for (const historicalSchema of [true, false]) {
    const paths = fixture(t);
    sealSuccessfully(paths);
    const receipt = JSON.parse(readFileSync(paths.receiptFile, 'utf8'));
    if (historicalSchema) {
      receipt.schema_version = 'clearra.product-e2e-cli-input.v1';
    } else {
      receipt.recipe = {
        package: 'clearra-cli',
        binary: 'clearra',
        profile: 'dev',
        target_triple: 'x86_64-pc-windows-msvc',
        features: ['native-c-core', 'webgpu-search'],
      };
    }
    writeFileSync(paths.receiptFile, `${JSON.stringify(receipt)}\n`);
    assertVerifyRejected(paths);
  }
});

test('does not overwrite a non-empty seal destination', (t) => {
  const paths = fixture(t);
  mkdirSync(paths.receipt);
  const existing = join(paths.receipt, 'existing');
  writeFileSync(existing, 'preserve');
  const result = seal(paths);
  assert.ok(Number.isInteger(result.status) && result.status !== 0);
  assert.deepEqual(readdirSync(paths.receipt), ['existing']);
  assert.equal(readFileSync(existing, 'utf8'), 'preserve');
});
