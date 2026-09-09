import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const script = fileURLToPath(new URL('./product-e2e-cli-artifact.mjs', import.meta.url));
const source = 'a'.repeat(40);

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'clearra-product-cli-'));
  const binary = join(root, 'source.exe');
  const identity = join(root, 'identity.json');
  const artifact = join(root, 'artifact');
  const output = join(root, 'github-output.txt');
  writeFileSync(binary, 'exact-binary-bytes');
  writeFileSync(identity, `${JSON.stringify({
    schema_version: 'clearra.native-build-identity.v1',
    authority: 'non-authoritative-build-input',
    source_commit: source,
    identity_sha256: 'b'.repeat(64),
    tracked_inputs_sha256: 'c'.repeat(64),
    tracked_input_count: 7,
    configuration: {},
    native_archive: {},
    toolchain: {},
    runtime_paths: {},
  })}\n`);
  return { root, binary, identity, artifact, output };
}

function seal(paths) {
  execFileSync(process.execPath, [script, 'seal',
    '--binary', paths.binary,
    '--native-identity', paths.identity,
    '--source-commit', source,
    '--run-id', '41',
    '--run-attempt', '2',
    '--output', paths.artifact,
  ]);
}

function verify(paths, overrides = {}) {
  return spawnSync(process.execPath, [script, 'verify',
    '--artifact', paths.artifact,
    '--expected-source-commit', overrides.source ?? source,
    '--expected-run-id', overrides.runId ?? '41',
    '--expected-run-attempt', overrides.runAttempt ?? '2',
    '--github-output', paths.output,
  ], { encoding: 'utf8' });
}

test('seals and verifies one exact source/run/attempt-bound product CLI', () => {
  const paths = fixture();
  try {
    seal(paths);
    const result = verify(paths);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /product_e2e_cli_artifact=verified/u);
    assert.equal(readFileSync(paths.output, 'utf8'), `binary_path=${join(paths.artifact, 'clearra.exe')}\n`);
  } finally {
    rmSync(paths.root, { recursive: true, force: true });
  }
});

test('rejects a mutated binary without rebuilding or falling back', () => {
  const paths = fixture();
  try {
    seal(paths);
    writeFileSync(join(paths.artifact, 'clearra.exe'), 'mutated');
    const result = verify(paths);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /digest or size mismatch/u);
  } finally {
    rmSync(paths.root, { recursive: true, force: true });
  }
});

test('rejects source, run, attempt, and extra-file mismatches', () => {
  for (const mutate of [
    (paths) => verify(paths, { source: 'd'.repeat(40) }),
    (paths) => verify(paths, { runId: '42' }),
    (paths) => verify(paths, { runAttempt: '3' }),
    (paths) => { writeFileSync(join(paths.artifact, 'extra.txt'), 'extra'); return verify(paths); },
  ]) {
    const paths = fixture();
    try {
      seal(paths);
      const result = mutate(paths);
      assert.notEqual(result.status, 0);
    } finally {
      rmSync(paths.root, { recursive: true, force: true });
    }
  }
});

test('rejects a non-empty seal destination', () => {
  const paths = fixture();
  try {
    mkdirSync(paths.artifact);
    writeFileSync(join(paths.artifact, 'existing'), 'x');
    const result = spawnSync(process.execPath, [script, 'seal',
      '--binary', paths.binary,
      '--native-identity', paths.identity,
      '--source-commit', source,
      '--run-id', '41',
      '--run-attempt', '2',
      '--output', paths.artifact,
    ], { encoding: 'utf8' });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /must be empty/u);
  } finally {
    rmSync(paths.root, { recursive: true, force: true });
  }
});
