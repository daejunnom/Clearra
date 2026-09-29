import { recoveryAllMiddleMask, recoveryStageEntries, type RecoveryMiddleStage } from './recoveryStages';
import { boardMaskHex, type RuleProfile, type SpinProfile } from './solverWorkspaceModel';
import { cliCommandRequestForDesktop, serializeCliCommandArguments } from './cliCommandModel';
import type { WorkspaceLanguage } from './workspaceI18n';
import { recoveryResultFrame, recoveryResultMaskForEngine, type RecoveryResultFrame } from './recoveryResultFrame';
export { changeRecoveryResultFrame, recoveryResultFrame, recoveryResultMaskForEngine } from './recoveryResultFrame';
export type { RecoveryResultFrame } from './recoveryResultFrame';
export type RecoveryBuildRequest = {
  extraMiddles?: RecoveryMiddleStage[]; nextMiddleId?: number;
  startMask: bigint; middleMask: bigint; resultMask: bigint; height: number;
  firstSupply: string; secondSupply: string;
  maxEarly: 'auto' | number; allowPieceExchange: boolean; holdEnabled: boolean;
  preserveB2B: boolean; rule: RuleProfile; spinProfile: SpinProfile;
  useAllLogicalProcessors: boolean;
  resultFrame?: RecoveryResultFrame;
  pngRender?: boolean;
  minimumSolutions?: boolean;
  solutionProbabilities?: boolean;
};
export function createRecoveryBuildRequest(): RecoveryBuildRequest {
  return { startMask: 0n, middleMask: 0n, resultMask: 0n, height: 8, resultFrame: 'shared', pngRender: false,
    firstSupply: '', secondSupply: '', maxEarly: 'auto', allowPieceExchange: false,
    holdEnabled: true, preserveB2B: false, useAllLogicalProcessors: false, rule: 'srs-plus', spinProfile: 'all-spin-plus' };
}
export function countRecoveryCells(value: bigint): number {
  let count = 0;
  for (let mask = value; mask > 0n; mask &= mask - 1n) count++;
  return count;
}
/** Preview only. The engine owns the actual geometry and row-history proof. */
export function compactRecoveryBoard(mask: bigint, height: number): bigint {
  let result = 0n; let out = 0;
  for (let y = 0; y < height; y++) {
    const row = (mask >> BigInt(y * 10)) & 1023n;
    if (row !== 1023n) { result |= row << BigInt(out * 10); out++; }
  }
  return result;
}
export function recoveryMiddleBase(request: RecoveryBuildRequest): bigint {
  return compactRecoveryBoard(request.startMask | recoveryAllMiddleMask(request), request.height);
}
export function recoveryEarlyChoices(request: RecoveryBuildRequest): number[] {
  // Every early result lock consumes four different target cells. Do not guess
  // a supply cardinality by counting characters of the pattern language.
  return Array.from({ length: Math.floor(recoveryStageEntries(request).slice(1).reduce((n, stage) => n + countRecoveryCells(stage.mask), 0) / 4) + 1 }, (_, i) => i);
}
export function resizeRecoveryBuild(request: RecoveryBuildRequest, height: number): RecoveryBuildRequest {
  if (!Number.isInteger(height) || height < 1 || height > 24) return request;
  const limit = (1n << BigInt(height * 10)) - 1n;
  return { ...request, height, startMask: request.startMask & limit,
    middleMask: request.middleMask & limit, resultMask: request.resultMask & limit,
    extraMiddles: request.extraMiddles?.map(stage => ({...stage,mask:stage.mask & limit})) };
}
export function validateRecoveryBuildRequest(request: RecoveryBuildRequest): string[] {
  const errors: string[] = [];
  if (!Number.isInteger(request.height) || request.height < 1 || request.height > 24) return ['height'];
  const limit = 1n << BigInt(request.height * 10);
  if ([request.startMask, request.middleMask, request.resultMask].some(mask => mask < 0n || mask >= limit)) return ['board'];
  try { recoveryResultFrame(request); } catch { return ['result-frame']; }
  const stages = recoveryStageEntries(request);
  if (stages.length > 60 || new Set((request.extraMiddles ?? []).map(stage=>stage.id)).size !== (request.extraMiddles?.length ?? 0)) errors.push('stages');
  if (stages.length > 2 && recoveryResultFrame(request) !== 'shared') errors.push('result-frame');
  if (stages.some(stage=>stage.mask < 0n || stage.mask >= limit)) errors.push('board');
  let union = request.startMask;
  for (const stage of stages.slice(0,-1)) { if (union & stage.mask) errors.push('overlap'); union |= stage.mask; }
  const occupied = recoveryResultFrame(request) === 'shared' ? union : recoveryMiddleBase(request);
  if ((request.startMask & request.middleMask) !== 0n || (occupied & request.resultMask) !== 0n) errors.push('overlap');
  if (stages.some(({mask}) => mask === 0n || countRecoveryCells(mask) % 4 !== 0)) errors.push('target-area');
  if (stages.some(stage=>!stage.supply.trim())) errors.push('supply');
  if (request.maxEarly !== 'auto' && (!Number.isSafeInteger(request.maxEarly) || request.maxEarly < 0)) errors.push('early');
  return errors;
}
export type RecoveryMinimumSelection = { sourceIdentity: string; keys: string[] };
export function recoveryBuildArguments(request: RecoveryBuildRequest, workers?: number, selection?: RecoveryMinimumSelection): string[] {
  if (workers !== undefined && (!Number.isSafeInteger(workers) || workers < 1 || workers > 65535)) {
    throw new RangeError('recovery worker count must be an integer in 1..65535');
  }
  // Lower the editor frame once. CLI, WASM, native search and replay continue
  // to receive the same established after-middle coordinate contract.
  const stages = recoveryStageEntries(request);
  const resultMask = recoveryResultMaskForEngine({...request,middleMask:recoveryAllMiddleMask(request)});
  const stageArgs = stages.length === 2
    ? ['--middle-mask',boardMaskHex(request.middleMask),'--result-mask',boardMaskHex(resultMask),
       '--first-supply',request.firstSupply.trim(),'--second-supply',request.secondSupply.trim()]
    : stages.flatMap(stage=>['--stage-target',boardMaskHex(stage.mask),'--stage-supply',stage.supply.trim()]);
  return ['clearra', 'recovery', 'build', '--start-mask', boardMaskHex(request.startMask),
    ...stageArgs, '--height', String(request.height), '--max-early', String(request.maxEarly),
    request.allowPieceExchange ? '--allow-piece-exchange' : '--no-piece-exchange',
    request.holdEnabled ? '--hold' : '--no-hold', request.preserveB2B ? '--preserve-b2b' : '--no-preserve-b2b',
    '--initial-b2b', '1', '--rule', request.rule, '--spin-profile', request.spinProfile,
    '--all-solutions',
    ...((request.minimumSolutions || selection) ? ['--minimum-solutions'] : []),
    ...(selection ? ['--minimum-source',selection.sourceIdentity,...selection.keys.flatMap(key=>['--required-solution',key])] : []),
    ...(request.useAllLogicalProcessors ? ['--use-all-cpu-threads'] : []),
    ...(workers === undefined ? [] : ['--workers', String(workers)])];
}
export const recoveryBuildCommand = (request: RecoveryBuildRequest, workers?: number, selection?: RecoveryMinimumSelection): string => serializeCliCommandArguments(recoveryBuildArguments(request, workers, selection));
export const recoveryBuildDesktopRequest = (request: RecoveryBuildRequest, language: WorkspaceLanguage, workers?: number, selection?: RecoveryMinimumSelection) => cliCommandRequestForDesktop(recoveryBuildArguments(request, workers, selection), language);

/** Physics-only request binding for result actions. Display toggles do not invalidate pins. */
export function recoveryBuildInputKey(request: RecoveryBuildRequest): string {
  return recoveryBuildCommand({...request,minimumSolutions:false,useAllLogicalProcessors:false});
}
