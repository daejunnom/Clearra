import type { Pc4HostGeneration } from './qualify-upstream-generation.mjs';
export function pc4SearchRangePolicy(generation: Pc4HostGeneration, profile: string): {
  windowBytes: number; directPaths: string[];
};
