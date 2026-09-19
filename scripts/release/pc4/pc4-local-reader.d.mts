import type { Pc4Artifact } from './qualify-upstream-generation.mjs';
export function createPc4LocalReader(artifacts: Pc4Artifact[],
  readSlice: (artifact: Pc4Artifact, offset: number, length: number) => Promise<Uint8Array>,
  options?: { signal?: AbortSignal; pageBytes?: number; cacheBytes?: number; directPaths?: string[];
    graphBlock?: { directoryPath: string; offsetsPath: string; graphPath: string; fieldCount: number; targetWidth: 3 | 4;
      blockRecords?: number; maxBlockBytes?: number } }): {
    provider: 'local-graph'; requests: number; bytes: number; reads: number; fileReads: number;
    localBytes: number; cacheHits: number; joinedRequests: number; retainedBytes: number;
    read(artifact: Pc4Artifact, offset: number, length: number): Promise<Uint8Array>;
    readMany(demands: { artifact: Pc4Artifact; offset: number; length: number }[]): Promise<Uint8Array[]>;
    dispose(): void;
  };
