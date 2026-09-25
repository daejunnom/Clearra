import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { prepareUiTestDependencies } from '../scripts/prepare-test-dependencies.mjs';
import {
  ACCEPTED_CTK3_MANIFEST,
  sealAcceptedCtk3Dist,
} from '../../../scripts/tools/accepted-ctk3-dist.mjs';

const source = 'a'.repeat(40);
const authority = Object.freeze({
  CLEARRA_SOURCE_COMMIT: source,
  CLEARRA_ACCEPTED_RUN_ID: '101',
  CLEARRA_ACCEPTED_RUN_ATTEMPT: '1',
});

async function fixture(t, sealed = false) {
  const root = await mkdtemp(join(tmpdir(), 'clearra-ui-test-input-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const directory = join(root, 'dist');
  if (sealed) {
    await mkdir(directory);
    for (const name of ['decodeWorker.js', 'index.cjs', 'index.d.ts', 'index.js']) {
      await writeFile(join(directory, name), `// ${name}\n`);
    }
    await sealAcceptedCtk3Dist(directory, source, '101', '1');
  }
  return directory;
}

function unexpectedBuild() {
  throw new Error('An accepted input must never be rebuilt');
}

test('clean UI invocation builds its codec dependency before loading parity tests', async t => {
  const directory = await fixture(t);
  let builds = 0;
  const mode = await prepareUiTestDependencies({ directory, environment: {}, build: async () => {
    builds += 1;
    await mkdir(directory);
    await writeFile(join(directory, 'index.js'), 'export {};\n');
  } });
  assert.equal(mode, 'source-built');
  assert.equal(builds, 1);
  assert.equal(await readFile(join(directory, 'index.js'), 'utf8'), 'export {};\n');
});

test('UI tests reuse accepted CTK3 bytes without replacing the producer seal', async t => {
  const directory = await fixture(t, true);
  const manifest = join(directory, ACCEPTED_CTK3_MANIFEST);
  const before = await readFile(manifest);
  assert.equal(await prepareUiTestDependencies({ directory, environment: authority,
    build: unexpectedBuild }), 'accepted-verified');
  assert.deepEqual(await readFile(manifest), before);
});

test('current canonical run identity can verify its accepted CTK3 input', async t => {
  const directory = await fixture(t, true);
  assert.equal(await prepareUiTestDependencies({ directory, environment: {
    CLEARRA_SOURCE_COMMIT: source, GITHUB_RUN_ID: '101', GITHUB_RUN_ATTEMPT: '1',
  }, build: unexpectedBuild }), 'accepted-verified');
});

test('missing accepted input fails instead of silently creating replacement evidence', async t => {
  const directory = await fixture(t);
  await assert.rejects(prepareUiTestDependencies({ directory, environment: authority,
    build: unexpectedBuild }), /distribution is missing/u);
});

test('foreign or corrupt accepted CTK3 cannot enter UI parity tests', async t => {
  const directory = await fixture(t, true);
  await assert.rejects(prepareUiTestDependencies({ directory, environment: {
    ...authority, CLEARRA_SOURCE_COMMIT: 'b'.repeat(40),
  }, build: unexpectedBuild }), /source commit mismatch/u);
  await writeFile(join(directory, 'index.js'), 'changed\n');
  await assert.rejects(prepareUiTestDependencies({ directory, environment: authority,
    build: unexpectedBuild }), /sealed file set and hashes/u);
});

test('a seal without external authority and a partial authority both fail closed', async t => {
  const directory = await fixture(t, true);
  await assert.rejects(prepareUiTestDependencies({ directory, environment: {},
    build: unexpectedBuild }), /expected source commit/u);
  await assert.rejects(prepareUiTestDependencies({ directory, environment: {
    CLEARRA_SOURCE_COMMIT: source, CLEARRA_ACCEPTED_RUN_ID: '101',
  }, build: unexpectedBuild }), /both run ID and run attempt/u);
});

test('codec build failures stop the UI test entrypoint', async t => {
  const directory = await fixture(t);
  await assert.rejects(prepareUiTestDependencies({ directory, environment: {}, build: async () => {
    throw new Error('codec compiler failed');
  } }), /codec compiler failed/u);
});
