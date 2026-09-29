import { recoveryMiddleStages, withRecoveryMiddleStages, recoveryStageFrames, type RecoveryMiddleStage } from './recoveryStageModel';
import { boardMaskHex, type RuleProfile, type SpinProfile } from './solverWorkspaceModel';
import { cliCommandRequestForDesktop, serializeCliCommandArguments } from './cliCommandModel';
import type { WorkspaceLanguage } from './workspaceI18n';
import { recoveryResultFrame, recoveryResultMaskForEngine, type RecoveryResultFrame } from './recoveryResultFrame';
export { changeRecoveryResultFrame, recoveryResultFrame, recoveryResultMaskForEngine } from './recoveryResultFrame';
export type { RecoveryResultFrame } from './recoveryResultFrame';
export type RecoveryBuildRequest = {
  startMask: bigint; middleMask: bigint; resultMask: bigint; height: number;
  firstSupply: string; secondSupply: string;
  /** Ordered common-frame targets; one independent supply per destination. */
  middleStages?: RecoveryMiddleStage[];
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
  return compactRecoveryBoard(request.startMask | request.middleMask, request.height);
}
export function recoveryEarlyChoices(request: RecoveryBuildRequest): number[] {
  // Every early result lock consumes four different target cells. Do not guess
  // a supply cardinality by counting characters of the pattern language.
  return Array.from({ length: Math.floor(countRecoveryCells(request.resultMask) / 4) + recoveryMiddleStages(request).slice(1).reduce((sum,s)=>sum+Math.floor(countRecoveryCells(s.mask)/4),0) + 1 }, (_, i) => i);
}
export function resizeRecoveryBuild(request: RecoveryBuildRequest, height: number): RecoveryBuildRequest {
  if (!Number.isInteger(height) || height < 1 || height > 24) return request;
  const limit = (1n << BigInt(height * 10)) - 1n;
  const next = { ...request, height, startMask: request.startMask & limit,
    middleMask: request.middleMask & limit, resultMask: request.resultMask & limit };
  return request.middleStages ? withRecoveryMiddleStages(next,recoveryMiddleStages(request).map(s=>({...s,mask:s.mask&limit}))) : next;
}
export function validateRecoveryBuildRequest(request: RecoveryBuildRequest): string[] {
  const errors: string[] = [];
  if (!Number.isInteger(request.height) || request.height < 1 || request.height > 24) return ['height'];
  const limit = 1n << BigInt(request.height * 10);
  let stages: RecoveryMiddleStage[];
  try { stages = recoveryMiddleStages(request); } catch { return ['stage-draft']; }
  if (stages.length > 1) {
    // Never silently execute only the first and last targets of a longer chain.
    try { recoveryStageFrames(request); } catch { errors.push('stage-frame'); }
    if ([...stages.map(s=>s.mask),request.resultMask].some(mask=>mask===0n || countRecoveryCells(mask)%4!==0)) errors.push('target-area');
    if ([...stages.map(s=>s.supply),request.secondSupply].some(s=>!s.trim())) errors.push('supply');
    if (request.maxEarly !== 'auto' && (!Number.isSafeInteger(request.maxEarly) || request.maxEarly < 0)) errors.push('early');
    return errors;
  }
  if ([request.startMask, request.middleMask, request.resultMask].some(mask => mask < 0n || mask >= limit)) return ['board'];
  try { recoveryResultFrame(request); } catch { return ['result-frame']; }
  const occupied = recoveryResultFrame(request) === 'shared' ? request.startMask | request.middleMask : recoveryMiddleBase(request);
  if ((request.startMask & request.middleMask) !== 0n || (occupied & request.resultMask) !== 0n) errors.push('overlap');
  if ([request.middleMask, request.resultMask].some(mask => mask === 0n || countRecoveryCells(mask) % 4 !== 0)) errors.push('target-area');
  if (!request.firstSupply.trim() || !request.secondSupply.trim()) errors.push('supply');
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
  const stages = recoveryMiddleStages(request);
  const targetArguments = stages.length > 1
    ? [...stages.map(s=>s.mask),request.resultMask].flatMap(mask=>['--stage-mask',boardMaskHex(mask)])
      .concat([...stages.map(s=>s.supply),request.secondSupply].flatMap(supply=>['--stage-supply',supply.trim()]))
    : ['--middle-mask',boardMaskHex(request.middleMask),'--result-mask',boardMaskHex(recoveryResultMaskForEngine(request)),
       '--first-supply',request.firstSupply.trim(),'--second-supply',request.secondSupply.trim()];
  if (stages.length > 1) recoveryStageFrames(request);
  return ['clearra', 'recovery', 'build', '--start-mask', boardMaskHex(request.startMask),
    ...targetArguments, '--height', String(request.height), '--max-early', String(request.maxEarly),
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
  if (request.middleStages && request.middleStages.length !== 1) {
    return 'recovery-chain.v1:' + JSON.stringify({start:request.startMask,stages:request.middleStages,
      result:request.resultMask,supply:request.secondSupply,height:request.height,early:request.maxEarly,
      exchange:request.allowPieceExchange,hold:request.holdEnabled,b2b:request.preserveB2B,rule:request.rule,spin:request.spinProfile},
      (_,v)=>typeof v==='bigint'?v.toString(16):v);
  }
  return recoveryBuildCommand({...request,minimumSolutions:false,useAllLogicalProcessors:false});
}
