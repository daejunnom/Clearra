import { createHash } from 'node:crypto';
import { readFile, stat } from 'node:fs/promises';
import { resolve } from 'node:path';

import type { Plugin } from 'vite';
import {
  clearraWasmBuildContractsEqual,
  createClearraWasmBuildContract,
  isClearraWasmBuildContract,
  type ClearraWasmBuildContract
} from '../../scripts/tools/clearra-wasm-build-contract.mjs';

type WasmArtifact = {
  path: string;
  bytes: number;
  sha256: string;
};

type WasmArtifactManifest = {
  schema_version: number;
  build: ClearraWasmBuildContract;
  bindings: WasmArtifact;
  wasm: WasmArtifact;
};

export const CLEARRA_WASM_ARTIFACT_UPDATE_EVENT =
  'clearra:wasm-artifact-updated' as const;
export const CLEARRA_WASM_ARTIFACT_SYNC_EVENT =
  'clearra:wasm-artifact-sync' as const;
export const CLEARRA_WASM_GENERATION_ENDPOINT = '/__clearra/wasm-generation';

type WasmArtifactGeneration = Readonly<{
  sourceSha256: string;
  bindingsSha256: string;
  wasmSha256: string;
}>;

type ArtifactContext = Readonly<{
  root: string;
  repositoryRoot: string;
  manifestPath: string;
}>;

export function wasmArtifactGuard(): Plugin {
  let artifactContext: ArtifactContext | null = null;
  let acceptedGeneration = '';
  let acceptedArtifactGeneration: WasmArtifactGeneration | null = null;
  return {
    name: 'clearra-wasm-artifact-guard',
    async configResolved(config) {
      const publicDir = process.env.CLEARRA_WEB_PUBLIC_DIR || 'static';
      const root = resolve(config.root, publicDir, 'wasm');
      const repositoryRoot = resolve(config.root, '..', '..');
      const manifestPath = resolve(root, 'clearra_wasm.manifest.json');
      artifactContext = { root, repositoryRoot, manifestPath };
      // Recovery after login must not rebuild paused work. Integrity remains
      // mandatory; production builds and live artifact updates stay strict.
      const restoreExisting = config.command === 'serve' &&
        config.mode === 'local-recovery';
      const manifest = await loadVerifiedManifest(artifactContext, restoreExisting);
      if (restoreExisting) config.logger.warn(
        '[clearra-wasm] local recovery: serving the existing verified artifact; source freshness is not guaranteed'
      );
      acceptedGeneration = manifestGeneration(manifest);
      acceptedArtifactGeneration = artifactGeneration(manifest);
    },
    configureServer(server) {
      const context = artifactContext;
      if (!context) return;
      // Read only the generation already accepted by the integrity guard. Never
      // expose raw paths or let a failed/in-progress build invalidate a job.
      server.middlewares.use((request, response, next) => {
        if (request.url?.split('?')[0] !== CLEARRA_WASM_GENERATION_ENDPOINT) {
          next();
          return;
        }
        response.setHeader('Cache-Control', 'no-store');
        response.setHeader('Content-Type', 'application/json');
        response.statusCode = acceptedArtifactGeneration ? 200 : 503;
        response.end(JSON.stringify(acceptedArtifactGeneration));
      });
      let updateTimer: ReturnType<typeof setTimeout> | null = null;
      const publishVerifiedGeneration = async () => {
        updateTimer = null;
        try {
          const manifest = await loadVerifiedManifest(context);
          const generation = manifestGeneration(manifest);
          if (generation === acceptedGeneration) return;
          acceptedGeneration = generation;
          acceptedArtifactGeneration = artifactGeneration(manifest);
          server.ws.send({
            type: 'custom',
            event: CLEARRA_WASM_ARTIFACT_UPDATE_EVENT,
            data: acceptedArtifactGeneration
          });
          server.config.logger.info(
            `[clearra-wasm] accepted development artifact ${manifest.wasm.sha256.slice(0, 12)}; ` +
              'the next GUI search will use the new worker generation'
          );
        } catch (error) {
          // A failed or in-progress build must never invalidate the running
          // worker. The atomic publisher writes the manifest last, and this
          // second verification keeps manual/partial edits fail closed too.
          server.config.logger.warn(
            `[clearra-wasm] ignored an unverified development artifact update: ${errorMessage(error)}`
          );
        }
      };
      const observeManifest = (changedPath: string) => {
        if (!samePath(changedPath, context.manifestPath)) return;
        if (updateTimer !== null) clearTimeout(updateTimer);
        updateTimer = setTimeout(() => void publishVerifiedGeneration(), 50);
      };
      const synchronizeClient = (
        _payload: unknown,
        client: { send: (event: string, data?: unknown) => void }
      ) => {
        if (acceptedArtifactGeneration) {
          client.send(
            CLEARRA_WASM_ARTIFACT_UPDATE_EVENT,
            acceptedArtifactGeneration
          );
        }
      };
      const cleanup = () => {
        if (updateTimer !== null) clearTimeout(updateTimer);
        updateTimer = null;
        server.watcher.off('add', observeManifest);
        server.watcher.off('change', observeManifest);
        server.ws.off(CLEARRA_WASM_ARTIFACT_SYNC_EVENT, synchronizeClient);
      };
      server.watcher.add(context.manifestPath);
      server.watcher.on('add', observeManifest);
      server.watcher.on('change', observeManifest);
      server.ws.on(CLEARRA_WASM_ARTIFACT_SYNC_EVENT, synchronizeClient);
      server.httpServer?.once('close', cleanup);
    }
  };
}

async function loadVerifiedManifest(
  context: ArtifactContext,
  restoreExisting = false
): Promise<WasmArtifactManifest> {
  let manifest: WasmArtifactManifest;
  try {
    manifest = JSON.parse(
      await readFile(context.manifestPath, 'utf8')
    ) as WasmArtifactManifest;
  } catch (error) {
    throw missingArtifactError(context.manifestPath, error);
  }
  if (
    manifest.schema_version !== 1 ||
    !isClearraWasmBuildContract(manifest.build) ||
    !isSha256(manifest.bindings.sha256) ||
    !isSha256(manifest.wasm.sha256) ||
    !isArtifactPath(manifest.bindings, 'clearra_wasm.js', 'clearra_wasm', '.js') ||
    !isArtifactPath(manifest.wasm, 'clearra_wasm_bg.wasm', 'clearra_wasm_bg', '.wasm')
  ) {
    throw staleArtifactError(
      context.manifestPath,
      'the manifest does not contain the current WASM build capability contract'
    );
  }
  const expectedBuild = await createClearraWasmBuildContract(context.repositoryRoot);
  if (!restoreExisting && !clearraWasmBuildContractsEqual(manifest.build, expectedBuild)) {
    throw staleArtifactError(
      context.manifestPath,
      `the artifact source fingerprint ${manifest.build.source_sha256} does not match ` +
        `the current source fingerprint ${expectedBuild.source_sha256}`
    );
  }
  await Promise.all([
    assertArtifact(context.root, manifest.bindings),
    assertArtifact(context.root, manifest.wasm)
  ]);
  return manifest;
}

function manifestGeneration(manifest: WasmArtifactManifest): string {
  return [
    manifest.build.source_sha256,
    manifest.bindings.sha256,
    manifest.wasm.sha256
  ].join(':');
}

function artifactGeneration(
  manifest: WasmArtifactManifest
): WasmArtifactGeneration {
  return {
    sourceSha256: manifest.build.source_sha256,
    bindingsSha256: manifest.bindings.sha256,
    wasmSha256: manifest.wasm.sha256
  };
}

function samePath(left: string, right: string): boolean {
  const normalize = (value: string) =>
    process.platform === 'win32' ? resolve(value).toLowerCase() : resolve(value);
  return normalize(left) === normalize(right);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function assertArtifact(root: string, artifact: WasmArtifact): Promise<void> {
  const path = resolve(root, artifact.path);
  try {
    const metadata = await stat(path);
    if (!metadata.isFile() || metadata.size !== artifact.bytes || metadata.size === 0) {
      throw new Error(`expected ${artifact.bytes} bytes, found ${metadata.size}`);
    }
    const bytes = await readFile(path);
    const actualSha256 = createHash('sha256').update(bytes).digest('hex');
    if (actualSha256 !== artifact.sha256) {
      throw new Error(`expected SHA-256 ${artifact.sha256}, found ${actualSha256}`);
    }
  } catch (error) {
    throw missingArtifactError(path, error);
  }
}

function isSha256(value: string): boolean {
  return /^[0-9a-f]{64}$/.test(value);
}

function isArtifactPath(
  artifact: WasmArtifact,
  legacyPath: string,
  versionedPrefix: string,
  versionedSuffix: string
): boolean {
  if (artifact.path === legacyPath) return true;
  return [20, 24, 64].some(
    (length) =>
      artifact.path ===
      `${versionedPrefix}.${artifact.sha256.slice(0, length)}${versionedSuffix}`
  );
}

function missingArtifactError(path: string, cause: unknown): Error {
  return new Error(
    `Clearra WASM artifact is missing or incomplete: ${path}. ` +
      'Run "npm run wasm:build" before invoking Vite directly.',
    { cause }
  );
}

function staleArtifactError(path: string, reason: string): Error {
  return new Error(
    `Clearra WASM artifact is stale: ${path}; ${reason}. ` +
      'From the repository root, run "npm --workspace @clearra/web run wasm:build" ' +
      'before starting or building the GUI.'
  );
}
