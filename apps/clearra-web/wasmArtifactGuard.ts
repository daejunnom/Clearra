import { createHash } from 'node:crypto';
import { readFile, stat } from 'node:fs/promises';
import { resolve } from 'node:path';

import type { Plugin } from 'vite';

type WasmArtifact = {
  path: string;
  bytes: number;
  sha256: string;
};

type WasmArtifactManifest = {
  schema_version: number;
  bindings: WasmArtifact;
  wasm: WasmArtifact;
};

export function wasmArtifactGuard(): Plugin {
  return {
    name: 'clearra-wasm-artifact-guard',
    async configResolved(config) {
      const root = resolve(config.root, 'static', 'wasm');
      const manifestPath = resolve(root, 'clearra_wasm.manifest.json');
      let manifest: WasmArtifactManifest;
      try {
        manifest = JSON.parse(await readFile(manifestPath, 'utf8')) as WasmArtifactManifest;
      } catch (error) {
        throw missingArtifactError(manifestPath, error);
      }
      if (
        manifest.schema_version !== 1 ||
        manifest.bindings.path !== 'clearra_wasm.js' ||
        manifest.wasm.path !== 'clearra_wasm_bg.wasm' ||
        !isSha256(manifest.bindings.sha256) ||
        !isSha256(manifest.wasm.sha256)
      ) {
        throw new Error(`Invalid Clearra WASM artifact manifest: ${manifestPath}`);
      }
      await Promise.all([
        assertArtifact(root, manifest.bindings),
        assertArtifact(root, manifest.wasm)
      ]);
    }
  };
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

function missingArtifactError(path: string, cause: unknown): Error {
  return new Error(
    `Clearra WASM artifact is missing or incomplete: ${path}. ` +
      'Run "npm run wasm:build" before invoking Vite directly.',
    { cause }
  );
}
