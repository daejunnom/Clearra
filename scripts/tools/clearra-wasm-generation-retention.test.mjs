import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import test from 'node:test';

import {
  CLEARRA_WASM_GENERATION_HISTORY_FILE,
  captureClearraWasmGenerationRetention,
  retainPublishedClearraWasmGenerations,
  serializeClearraWasmGenerationHistory,
} from './clearra-wasm-generation-retention.mjs';

test('both WASM publishing entry points use the shared generation retention contract', async () => {
  for (const publisher of ['build-clearra-wasm.mjs', 'stage-clearra-wasm.mjs']) {
    const source = await readFile(resolve(import.meta.dirname, publisher), 'utf8');
    assert.match(source, /captureClearraWasmGenerationRetention/u, publisher);
    assert.match(source, /retainPublishedClearraWasmGenerations/u, publisher);
    assert.doesNotMatch(source, /function removeStaleVersionedArtifacts/u, publisher);
  }
});

test('retains five complete JS and WASM generations', async () => {
  await withFixture(async (destination) => {
    const generations = generationFixtures(5);
    for (const generation of generations) {
      const result = await publishGeneration(destination, generation);
      assert.equal(result.status, 'retained');
    }

    assert.deepEqual(
      await versionedArtifactNames(destination),
      generations.flatMap(generationPaths).sort((left, right) => left.localeCompare(right, 'en')),
    );
    const history = await readHistory(destination);
    assert.equal(history.generations.length, 5);
    assert.deepEqual(history.generations.map(generationIdentity), [5, 4, 3, 2, 1]);
  });
});

test('publishing a sixth complete generation removes only the oldest pair', async () => {
  await withFixture(async (destination) => {
    const generations = generationFixtures(6);
    for (const generation of generations.slice(0, 5)) {
      await publishGeneration(destination, generation);
    }
    const result = await publishGeneration(destination, generations[5]);

    assert.equal(result.status, 'retained');
    assert.deepEqual(
      result.deleted,
      generationPaths(generations[0]).sort((left, right) => left.localeCompare(right, 'en')),
    );
    await assertGenerationPresence(destination, generations[0], false);
    for (const generation of generations.slice(1)) {
      await assertGenerationPresence(destination, generation, true);
    }
    assert.deepEqual(
      (await readHistory(destination)).generations.map(generationIdentity),
      [6, 5, 4, 3, 2],
    );
  });
});

test('the current manifest generation is retained regardless of stale history order', async () => {
  await withFixture(async (destination) => {
    const generations = generationFixtures(6);
    await Promise.all(generations.map((generation) => writeGeneration(destination, generation)));
    await writeHistory(destination, [...generations].reverse());
    await writeCurrentManifest(destination, generations[0]);

    const snapshot = await captureClearraWasmGenerationRetention(destination);
    const result = await retain(destination, generations[0].manifest, snapshot);

    assert.equal(result.status, 'retained');
    await assertGenerationPresence(destination, generations[0], true);
    assert.equal(
      generationIdentity((await readHistory(destination)).generations[0]),
      1,
    );
  });
});

test('unrelated destination files survive successful generation pruning', async () => {
  await withFixture(async (destination) => {
    const generations = generationFixtures(6);
    for (const generation of generations.slice(0, 5)) {
      await publishGeneration(destination, generation);
    }
    await Promise.all([
      writeFile(resolve(destination, 'operator-notes.txt'), 'keep me\n', 'utf8'),
      writeFile(resolve(destination, 'clearra_wasm.js.map'), '{}\n', 'utf8'),
    ]);

    const result = await publishGeneration(destination, generations[5]);
    assert.equal(result.status, 'retained');
    assert.equal(await readFile(resolve(destination, 'operator-notes.txt'), 'utf8'), 'keep me\n');
    assert.equal(await readFile(resolve(destination, 'clearra_wasm.js.map'), 'utf8'), '{}\n');
  });
});

test('failed or partial current publication never deletes an older generation', async () => {
  await withFixture(async (destination) => {
    const generations = generationFixtures(7);
    await Promise.all(
      generations.slice(0, 6).map((generation) => writeGeneration(destination, generation)),
    );
    await writeHistory(destination, [...generations.slice(0, 6)].reverse());
    await writeCurrentManifest(destination, generations[5]);
    const snapshot = await captureClearraWasmGenerationRetention(destination);

    await writeGeneration(destination, generations[6]);
    const failedBeforeManifest = await retain(destination, generations[6].manifest, snapshot);
    assert.equal(failedBeforeManifest.status, 'skipped');
    assert.equal(failedBeforeManifest.reason, 'current_manifest_not_published');
    for (const generation of generations.slice(0, 6)) {
      await assertGenerationPresence(destination, generation, true);
    }

    await rm(resolve(destination, generations[6].manifest.wasm.path));
    await writeCurrentManifest(destination, generations[6]);
    const partialPublication = await retain(destination, generations[6].manifest, snapshot);
    assert.equal(partialPublication.status, 'skipped');
    assert.match(partialPublication.reason, /versioned artifact is incomplete/u);
    for (const generation of generations.slice(0, 6)) {
      await assertGenerationPresence(destination, generation, true);
    }
  });
});

test('retention history publication failure occurs before any stale deletion', async () => {
  await withFixture(async (destination) => {
    const generations = generationFixtures(6);
    await Promise.all(generations.map((generation) => writeGeneration(destination, generation)));
    await writeHistory(destination, [...generations].reverse());
    await writeCurrentManifest(destination, generations[5]);
    const snapshot = await captureClearraWasmGenerationRetention(destination);

    await assert.rejects(
      retainPublishedClearraWasmGenerations({
        destinationDir: destination,
        currentManifest: generations[5].manifest,
        snapshot,
        publishHistory: async () => {
          throw new Error('simulated history publication failure');
        },
      }),
      /simulated history publication failure/u,
    );
    for (const generation of generations) {
      await assertGenerationPresence(destination, generation, true);
    }
  });
});

test('orphaned or malformed managed artifacts fail safe without pruning', async () => {
  await withFixture(async (destination) => {
    const generations = generationFixtures(6);
    for (const generation of generations.slice(0, 5)) {
      await publishGeneration(destination, generation);
    }
    const snapshot = await captureClearraWasmGenerationRetention(destination);
    await writeGeneration(destination, generations[5]);
    await writeCurrentManifest(destination, generations[5]);
    await writeFile(
      resolve(destination, `clearra_wasm.${'f'.repeat(24)}.js`),
      'orphan\n',
      'utf8',
    );

    const result = await retain(destination, generations[5].manifest, snapshot);
    assert.equal(result.status, 'skipped');
    assert.match(result.reason, /untracked_versioned_artifact/u);
    await assertGenerationPresence(destination, generations[0], true);
  });

  await withFixture(async (destination) => {
    const generations = generationFixtures(6);
    for (const generation of generations.slice(0, 5)) {
      await publishGeneration(destination, generation);
    }
    const snapshot = await captureClearraWasmGenerationRetention(destination);
    await writeGeneration(destination, generations[5]);
    await writeCurrentManifest(destination, generations[5]);
    await writeFile(
      resolve(destination, 'clearra_wasm.not-a-digest.js'),
      'malformed\n',
      'utf8',
    );

    const result = await retain(destination, generations[5].manifest, snapshot);
    assert.equal(result.status, 'skipped');
    assert.match(result.reason, /malformed_versioned_artifact/u);
    await assertGenerationPresence(destination, generations[0], true);
  });
});

async function publishGeneration(destination, generation) {
  const snapshot = await captureClearraWasmGenerationRetention(destination);
  await writeGeneration(destination, generation);
  await writeCurrentManifest(destination, generation);
  return retain(destination, generation.manifest, snapshot);
}

async function retain(destination, manifest, snapshot) {
  return retainPublishedClearraWasmGenerations({
    destinationDir: destination,
    currentManifest: manifest,
    snapshot,
    publishHistory: (serialized) =>
      writeFile(
        resolve(destination, CLEARRA_WASM_GENERATION_HISTORY_FILE),
        serialized,
        'utf8',
      ),
  });
}

async function writeGeneration(destination, generation) {
  await Promise.all([
    writeFile(resolve(destination, generation.manifest.bindings.path), generation.bindings),
    writeFile(resolve(destination, generation.manifest.wasm.path), generation.wasm),
  ]);
}

async function writeCurrentManifest(destination, generation) {
  await writeFile(
    resolve(destination, 'clearra_wasm.manifest.json'),
    `${JSON.stringify(generation.manifest)}\n`,
    'utf8',
  );
}

async function writeHistory(destination, generations) {
  await writeFile(
    resolve(destination, CLEARRA_WASM_GENERATION_HISTORY_FILE),
    serializeClearraWasmGenerationHistory(generations.map(({ manifest }) => manifest)),
    'utf8',
  );
}

async function readHistory(destination) {
  return JSON.parse(
    await readFile(resolve(destination, CLEARRA_WASM_GENERATION_HISTORY_FILE), 'utf8'),
  );
}

function generationFixtures(count) {
  return Array.from({ length: count }, (_, index) => generationFixture(index + 1));
}

function generationFixture(id) {
  const bindings = Buffer.from(`bindings-generation-${id}\n`, 'utf8');
  const wasm = Buffer.from(`wasm-generation-${id}\n`, 'utf8');
  return {
    id,
    bindings,
    wasm,
    manifest: {
      bindings: artifact('clearra_wasm', '.js', bindings),
      wasm: artifact('clearra_wasm_bg', '.wasm', wasm),
    },
  };
}

function artifact(prefix, suffix, bytes) {
  const sha256 = createHash('sha256').update(bytes).digest('hex');
  return {
    path: `${prefix}.${sha256.slice(0, 24)}${suffix}`,
    bytes: bytes.byteLength,
    sha256,
  };
}

function generationIdentity(generation) {
  return GENERATION_IDS.get(
    `${generation.bindings.sha256}:${generation.wasm.sha256}`,
  );
}

const GENERATION_IDS = new Map(
  generationFixtures(20).map((generation) => [
    `${generation.manifest.bindings.sha256}:${generation.manifest.wasm.sha256}`,
    generation.id,
  ]),
);

function generationPaths(generation) {
  const manifest = generation.manifest ?? generation;
  return [manifest.bindings.path, manifest.wasm.path];
}

async function versionedArtifactNames(destination) {
  return (await readdir(destination))
    .filter(
      (name) =>
        /^clearra_wasm\.[0-9a-f]{24}\.js$/u.test(name) ||
        /^clearra_wasm_bg\.[0-9a-f]{24}\.wasm$/u.test(name),
    )
    .sort((left, right) => left.localeCompare(right, 'en'));
}

async function assertGenerationPresence(destination, generation, expected) {
  for (const path of generationPaths(generation)) {
    const present = await readFile(resolve(destination, path))
      .then(() => true)
      .catch((error) => {
        if (error?.code === 'ENOENT') return false;
        throw error;
      });
    assert.equal(present, expected, path);
  }
}

async function withFixture(body) {
  const destination = await mkdtemp(resolve(tmpdir(), 'clearra-wasm-retention-'));
  try {
    await body(destination);
  } finally {
    await rm(destination, { recursive: true, force: true });
  }
}
