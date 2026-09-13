import type { Pc4Artifact, Pc4HostGeneration } from './qualify-upstream-generation.mjs';
type DownloadPlan = { profile: string; revision: string; repository: string; files: Pc4Artifact[]; totalBytes: number };
type Writer = { write(bytes: Uint8Array): Promise<void>; close(): Promise<void>; abort(): Promise<void> };
type Store = { begin(plan: DownloadPlan): Promise<{ open(file: Pc4Artifact): Promise<Writer>;
  commit(generation: Pc4HostGeneration): Promise<void | { cleanupPending: boolean }>; abort(): Promise<void> }> };
export function pc4DownloadPlan(generation: Pc4HostGeneration, profile?: string): DownloadPlan;
export function downloadPc4Profile(generation: Pc4HostGeneration, store: Store, options: {
  intent: 'explicit-download'; profile?: string; signal?: AbortSignal;
  onProgress?: (p: { transferredBytes: number; totalBytes: number; file: string }) => void;
}): Promise<{ profile: string; revision: string; storedBytes: number; cleanupPending: boolean }>;
