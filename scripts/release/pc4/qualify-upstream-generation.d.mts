export type Pc4Artifact = { path: string; byte_length: number; content_identity: string };
export type Pc4HostProfile = {
  profile: string; upstream_complete: boolean; status: 'ready' | 'unavailable'; reason?: string;
  reader_contract?: string; field_count?: number; target_width?: number; target_lines?: number[]; terminal_id?: number;
  artifacts?: { fields: Pc4Artifact; offsets: Pc4Artifact; graph: Pc4Artifact };
};
export type Pc4HostGeneration = {
  schema: string; repository: string; revision: string; profiles: Pc4HostProfile[]; transferred_bytes: number;
};
export class Pc4OnlineError extends Error { code: string; constructor(code: string, detail?: string); }
export function qualifyPc4UpstreamGeneration(options?: { signal?: AbortSignal; onProgress?: (progress: { transferredBytes: number; requests: number }) => void }): Promise<Pc4HostGeneration>;
export function createPc4RangeReader(generation: Pc4HostGeneration, options?: {
  signal?: AbortSignal; onProgress?: (progress: { transferredBytes: number; requests: number }) => void;
  maxBytes?: number; maxRequests?: number; cacheBytes?: number; windowBytes?: number; maxConcurrent?: number; directPaths?: string[];
}): { readonly bytes: number; readonly requests: number; readonly reads: number; readonly cacheHits: number;
  readonly joinedRequests: number; readonly retainedBytes: number; dispose(): void;
  readCached(artifact: Pc4Artifact, offset: number, length: number): Uint8Array | null;
  readMany(demands: Array<{ artifact: Pc4Artifact; offset: number; length: number }>,
    options?: { maxGapBytes?: number }): Promise<Uint8Array[]>;
  read(artifact: Pc4Artifact, offset: number, length: number): Promise<Uint8Array> };
