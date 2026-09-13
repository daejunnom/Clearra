import type { Pc4Artifact, Pc4HostGeneration } from './qualify-upstream-generation.mjs';
export function prefetchPc4LookupFrontier(reader: {
  read(artifact: Pc4Artifact, offset: number, length: number): Promise<Uint8Array>;
  readCached(artifact: Pc4Artifact, offset: number, length: number): Uint8Array | null;
  readMany(demands: Array<{artifact: Pc4Artifact; offset: number; length: number}>): Promise<Uint8Array[]>;
}, generation: Pc4HostGeneration, range: {
  lookup_frontier?: number[]; profile: string; artifact: Pc4Artifact; offset: number; length: number;
}): Promise<void>;
