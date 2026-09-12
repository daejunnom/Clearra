import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('TypeScript contracts compile only inside an owned generation', () => {
  const source = readFileSync(new URL('./run-typescript-contracts.mjs', import.meta.url), 'utf8');
  assert.match(source, /enterManagedBuildOrRelaunch\(repositoryRoot/u);
  assert.match(source, /inputs\.map\(input => resolve\(invocationDirectory, input\)\)/u);
  assert.match(source, /mkdtemp\(join\(owner\.transaction, 'typescript-contracts-'/u);
  assert.doesNotMatch(source, /tmpdir\(/u);
  assert.ok(source.indexOf('enterManagedBuildOrRelaunch(repositoryRoot') < source.indexOf('await mkdtemp('));
  assert.match(source, /finally\s*\{\s*await rm\(bundleDirectory/u);
});

test('WASM bindgen staging requires the existing build owner, not a legacy fallback', () => {
  const source = readFileSync(new URL('./stage-clearra-wasm.mjs', import.meta.url), 'utf8');
  assert.match(source, /assertManagedBuildTransaction\(\{ sourceRoot: root \}\)/u);
  assert.match(source, /resolve\(owner\.transaction, 'wasm-stage'\)/u);
  assert.match(source, /const targetRoot = owner\.cargoTarget/u);
  assert.doesNotMatch(source, /const cacheBase|cargo-target-wasm/u);
  assert.ok(source.indexOf('assertManagedBuildTransaction({') < source.indexOf('await mkdir('));
});
