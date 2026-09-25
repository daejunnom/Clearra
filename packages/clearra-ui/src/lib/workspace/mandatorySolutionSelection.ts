import type { SolverWorkspaceRequest } from './solverWorkspaceModel.ts';
import type { BuildProbabilityRequest } from './buildProbabilityModel.ts';

export type MandatorySolutionSelection = {
  pinnedSolutionKeys: string[];
  pinnedSolutionDocument?: string;
  pinnedSourceSetHash?: string;
};
export function emptyMandatorySelection(): MandatorySolutionSelection {
  return { pinnedSolutionKeys: [], pinnedSolutionDocument: undefined, pinnedSourceSetHash: undefined };
}
function identity(values: unknown[]): string {
  return JSON.stringify(values, (_, value) => typeof value === 'bigint' ? value.toString(16) : value);
}
export function pcMandatorySource(request: SolverWorkspaceRequest): string {
  return identity([request.lines, request.boardMask, request.queue, request.holdEnabled,
    request.holdPiece ?? 'empty', request.queueKnowledge, request.rule, request.preserveB2B,
    request.spinProfile, request.initialB2B, request.maxPatterns ?? null]);
}
export function buildMandatorySource(request: BuildProbabilityRequest): string {
  return identity([request.height, request.existingMask, request.targetMask, request.queue,
    request.sourcePieces, request.holdEnabled, request.rule, request.aggregation,
    request.preserveB2B, request.spinProfile, request.finesse, request.patternKnowledge]);
}
export function toggleMandatorySolution(
  current: MandatorySolutionSelection, key: string, sourceHash: string,
  encode: (keys: string[]) => string
): MandatorySolutionSelection {
  if (!/^cts1:[0-9a-f]{16}$/u.test(sourceHash) || !key.startsWith('ctk1|')) {
    throw new TypeError('Mandatory selection requires a complete canonical solution source');
  }
  if (current.pinnedSourceSetHash && current.pinnedSourceSetHash !== sourceHash) {
    throw new Error('Mandatory selection belongs to a different result');
  }
  const keys = new Set(current.pinnedSolutionKeys);
  if (keys.has(key)) keys.delete(key); else keys.add(key);
  if (!keys.size) return emptyMandatorySelection();
  const ordered = [...keys].sort();
  return { pinnedSolutionKeys: ordered, pinnedSolutionDocument: encode(ordered), pinnedSourceSetHash: sourceHash };
}
