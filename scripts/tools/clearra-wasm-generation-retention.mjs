import { createHash } from 'node:crypto';
import { lstat, readFile, readdir, rm } from 'node:fs/promises';
import { resolve } from 'node:path';

export const CLEARRA_WASM_GENERATION_HISTORY_FILE =
  'clearra_wasm.retention-history.json';
export const CLEARRA_WASM_RETAINED_GENERATIONS = 5;

const CURRENT_MANIFEST_FILE = 'clearra_wasm.manifest.json';
const HISTORY_SCHEMA = 'clearra.wasm.non-runtime-retention-history.v1';
const SHA256_PATTERN = /^[0-9a-f]{64}$/u;
const VERSIONED_BINDINGS_PATTERN = /^clearra_wasm\.[0-9a-f]{20,64}\.js$/u;
const VERSIONED_WASM_PATTERN = /^clearra_wasm_bg\.[0-9a-f]{20,64}\.wasm$/u;
const UNVERSIONED_ARTIFACTS = new Set(['clearra_wasm.js', 'clearra_wasm_bg.wasm']);

/**
 * Captures the deterministic generation order before the current manifest is
 * replaced. This history is local retention metadata only; the runtime
 * manifest remains the sole build authority.
 */
export async function captureClearraWasmGenerationRetention(destinationDir) {
  const destination = resolve(destinationDir);
  try {
    const [publishedManifest, history] = await Promise.all([
      readOptionalJson(resolve(destination, CURRENT_MANIFEST_FILE)),
      readOptionalJson(resolve(destination, CLEARRA_WASM_GENERATION_HISTORY_FILE)),
    ]);
    if (publishedManifest === null && history !== null) {
      return unsafeSnapshot('retention_history_without_current_manifest');
    }

    const previousGeneration = publishedManifest === null
      ? null
      : generationFromManifest(publishedManifest);
    const generations = history === null ? [] : generationsFromHistory(history);
    const knownGenerations = uniqueGenerations([
      ...(previousGeneration === null ? [] : [previousGeneration]),
      ...generations,
    ]);
    for (const generation of knownGenerations) {
      await requireCompleteGeneration(destination, generation);
    }
    return Object.freeze({
      safe: true,
      reason: null,
      previousGeneration,
      generations: Object.freeze(generations),
    });
  } catch (error) {
    return unsafeSnapshot(`retention_snapshot_invalid:${error.message}`);
  }
}

/**
 * Records a successfully published manifest, then removes only generations
 * whose complete JS/WASM pairing and deterministic age are proven by the
 * captured history. Any partial, orphaned, malformed, or untracked managed
 * artifact makes cleanup fail safe by retaining every versioned file.
 */
export async function retainPublishedClearraWasmGenerations({
  destinationDir,
  currentManifest,
  snapshot,
  publishHistory,
}) {
  if (!snapshot?.safe) {
    return skipped(snapshot?.reason ?? 'retention_snapshot_unavailable');
  }
  if (typeof publishHistory !== 'function') {
    throw new TypeError('publishHistory must be a function');
  }

  const destination = resolve(destinationDir);
  let plan;
  try {
    const currentGeneration = generationFromManifest(currentManifest);
    const publishedManifest = await readRequiredJson(
      resolve(destination, CURRENT_MANIFEST_FILE),
    );
    const publishedGeneration = generationFromManifest(publishedManifest);
    if (!sameGeneration(currentGeneration, publishedGeneration)) {
      return skipped('current_manifest_not_published');
    }

    const generations = uniqueGenerations([
      currentGeneration,
      ...(snapshot.previousGeneration === null
        ? []
        : [snapshot.previousGeneration]),
      ...snapshot.generations,
    ]);
    assertNoConflictingArtifactDescriptors(generations);
    for (const generation of generations) {
      await requireCompleteGeneration(destination, generation);
    }

    const managedArtifactIssue = await findUntrackedOrMalformedManagedArtifact(
      destination,
      generations,
    );
    if (managedArtifactIssue !== null) {
      return skipped(managedArtifactIssue);
    }

    const retained = generations.slice(0, CLEARRA_WASM_RETAINED_GENERATIONS);
    const stale = generations.slice(CLEARRA_WASM_RETAINED_GENERATIONS);
    const retainedPaths = artifactPaths(retained);
    if (
      !retainedPaths.has(currentGeneration.bindings.path) ||
      !retainedPaths.has(currentGeneration.wasm.path)
    ) {
      throw new Error('current_generation_was_not_retained');
    }
    plan = { generations, retained, stale, retainedPaths };
  } catch (error) {
    return skipped(`retention_validation_failed:${error.message}`);
  }

  if (plan.stale.length > 0) {
    // Persist the full pre-delete order first. If deletion is interrupted,
    // the next run sees the partial generation and refuses further cleanup.
    await publishHistory(serializeHistory(plan.generations));
  }

  const deleted = [];
  const stalePaths = [...artifactPaths(plan.stale)]
    .filter((path) => !plan.retainedPaths.has(path))
    .sort((left, right) => left.localeCompare(right, 'en'));
  for (const path of stalePaths) {
    await rm(resolve(destination, path));
    deleted.push(path);
  }

  await publishHistory(serializeHistory(plan.retained));
  return Object.freeze({
    status: 'retained',
    reason: null,
    retainedGenerationCount: plan.retained.length,
    deleted: Object.freeze(deleted),
  });
}

export function serializeClearraWasmGenerationHistory(generations) {
  return serializeHistory(generations.map(validateGeneration));
}

function serializeHistory(generations) {
  return `${JSON.stringify(
    {
      schema: HISTORY_SCHEMA,
      role: 'non-runtime-retention-metadata',
      retention_limit: CLEARRA_WASM_RETAINED_GENERATIONS,
      generations,
    },
    null,
    2,
  )}\n`;
}

function generationsFromHistory(history) {
  requireExactKeys(
    history,
    ['generations', 'retention_limit', 'role', 'schema'],
    'generation history',
  );
  if (
    history.schema !== HISTORY_SCHEMA ||
    history.role !== 'non-runtime-retention-metadata' ||
    history.retention_limit !== CLEARRA_WASM_RETAINED_GENERATIONS ||
    !Array.isArray(history.generations)
  ) {
    throw new Error('generation history contract is invalid');
  }
  return history.generations.map(validateGeneration);
}

function generationFromManifest(manifest) {
  if (manifest === null || typeof manifest !== 'object' || Array.isArray(manifest)) {
    throw new Error('WASM manifest must be an object');
  }
  return validateGeneration({
    bindings: manifest.bindings,
    wasm: manifest.wasm,
  });
}

function validateGeneration(generation) {
  requireExactKeys(generation, ['bindings', 'wasm'], 'WASM generation');
  return Object.freeze({
    bindings: validateArtifact(
      generation.bindings,
      'bindings',
      'clearra_wasm',
      '.js',
    ),
    wasm: validateArtifact(
      generation.wasm,
      'wasm',
      'clearra_wasm_bg',
      '.wasm',
    ),
  });
}

function validateArtifact(artifact, label, prefix, suffix) {
  requireExactKeys(artifact, ['bytes', 'path', 'sha256'], `${label} artifact`);
  if (
    !Number.isSafeInteger(artifact.bytes) ||
    artifact.bytes <= 0 ||
    typeof artifact.sha256 !== 'string' ||
    !SHA256_PATTERN.test(artifact.sha256) ||
    artifact.path !== `${prefix}.${artifact.sha256.slice(0, 24)}${suffix}`
  ) {
    throw new Error(`${label} artifact descriptor is invalid`);
  }
  return Object.freeze({
    path: artifact.path,
    bytes: artifact.bytes,
    sha256: artifact.sha256,
  });
}

async function requireCompleteGeneration(destination, generation) {
  await requireArtifact(destination, generation.bindings);
  await requireArtifact(destination, generation.wasm);
}

async function requireArtifact(destination, artifact) {
  const path = resolve(destination, artifact.path);
  const stats = await lstat(path).catch((error) => {
    if (error?.code === 'ENOENT') {
      throw new Error(`versioned artifact is incomplete: ${artifact.path}`);
    }
    throw error;
  });
  if (!stats.isFile() || stats.isSymbolicLink() || stats.size !== artifact.bytes) {
    throw new Error(`versioned artifact is incomplete: ${artifact.path}`);
  }
  const bytes = await readFile(path);
  const sha256 = createHash('sha256').update(bytes).digest('hex');
  if (sha256 !== artifact.sha256) {
    throw new Error(`versioned artifact hash mismatch: ${artifact.path}`);
  }
}

async function findUntrackedOrMalformedManagedArtifact(destination, generations) {
  const knownPaths = artifactPaths(generations);
  for (const name of await readdir(destination)) {
    if (isVersionedArtifactName(name)) {
      if (!knownPaths.has(name)) return `untracked_versioned_artifact:${name}`;
      continue;
    }
    if (looksLikeMalformedManagedArtifact(name)) {
      return `malformed_versioned_artifact:${name}`;
    }
  }
  return null;
}

function looksLikeMalformedManagedArtifact(name) {
  if (UNVERSIONED_ARTIFACTS.has(name)) return false;
  return (
    (name.startsWith('clearra_wasm.') && name.endsWith('.js')) ||
    (name.startsWith('clearra_wasm_bg.') && name.endsWith('.wasm'))
  );
}

function isVersionedArtifactName(name) {
  return VERSIONED_BINDINGS_PATTERN.test(name) || VERSIONED_WASM_PATTERN.test(name);
}

function uniqueGenerations(generations) {
  const seen = new Set();
  const unique = [];
  for (const candidate of generations) {
    const generation = validateGeneration(candidate);
    const identity = generationIdentity(generation);
    if (seen.has(identity)) continue;
    seen.add(identity);
    unique.push(generation);
  }
  return unique;
}

function generationIdentity(generation) {
  return `${generation.bindings.sha256}:${generation.wasm.sha256}`;
}

function sameGeneration(left, right) {
  return generationIdentity(left) === generationIdentity(right);
}

function artifactPaths(generations) {
  const paths = new Set();
  for (const generation of generations) {
    paths.add(generation.bindings.path);
    paths.add(generation.wasm.path);
  }
  return paths;
}

function assertNoConflictingArtifactDescriptors(generations) {
  const descriptors = new Map();
  for (const generation of generations) {
    for (const artifact of [generation.bindings, generation.wasm]) {
      const serialized = JSON.stringify(artifact);
      const existing = descriptors.get(artifact.path);
      if (existing !== undefined && existing !== serialized) {
        throw new Error(`artifact path has conflicting descriptors: ${artifact.path}`);
      }
      descriptors.set(artifact.path, serialized);
    }
  }
}

async function readOptionalJson(path) {
  let stats;
  try {
    stats = await lstat(path);
  } catch (error) {
    if (error?.code === 'ENOENT') return null;
    throw error;
  }
  if (!stats.isFile() || stats.isSymbolicLink()) {
    throw new Error(`retention metadata must be a regular file: ${path}`);
  }
  return JSON.parse(await readFile(path, 'utf8'));
}

async function readRequiredJson(path) {
  const value = await readOptionalJson(path);
  if (value === null) throw new Error(`required publication manifest is missing: ${path}`);
  return value;
}

function requireExactKeys(value, expected, description) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${description} must be an object`);
  }
  const actual = Object.keys(value).sort();
  const required = [...expected].sort();
  if (JSON.stringify(actual) !== JSON.stringify(required)) {
    throw new Error(`${description} has unexpected keys`);
  }
}

function unsafeSnapshot(reason) {
  return Object.freeze({
    safe: false,
    reason,
    previousGeneration: null,
    generations: Object.freeze([]),
  });
}

function skipped(reason) {
  return Object.freeze({
    status: 'skipped',
    reason,
    retainedGenerationCount: null,
    deleted: Object.freeze([]),
  });
}
