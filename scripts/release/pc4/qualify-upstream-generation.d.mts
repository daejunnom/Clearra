export const PC4_READER_CONTRACT: 'hydra-jstris-180-complete-graph-v1';
export const PC4_TARGET_QUALIFICATION_RECEIPT_SCHEMA:
  'clearra.pc4.exact-target-qualification.v1';
export const PC4_PC_TERMINAL_SEMANTICS:
  'clearra.pc4.full-bottom-rows-after-clear.v1';
export type Pc4Artifact = { path: string; byte_length: number; content_identity: string };
export type Pc4TargetQualificationReceipt = {
  schema: 'clearra.pc4.exact-target-qualification.v1';
  repository: string; revision: string; profile: string;
  reader_contract: string; use_case: 'pc-search'; target_lines: 4;
  terminal_id: number; terminal_hash: number;
  terminal_semantics_identity: string;
  outgoing_edge_completeness_identity: string;
  known_answer_identity: string;
  offline_exact_parity_identity: string;
};
export type Pc4HostProfile = {
  profile: string; upstream_complete: boolean; status: 'ready' | 'unavailable'; reason?: string;
  reader_contract?: string; field_count?: number; target_width?: number; target_lines?: number[];
  pc_search_target_lines?: number[]; setup_search_target_lines?: number[]; terminal_id?: number;
  target_qualification_receipts?: Pc4TargetQualificationReceipt[];
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
