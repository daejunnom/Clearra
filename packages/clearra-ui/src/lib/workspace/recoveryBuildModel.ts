import { boardMaskHex, type RuleProfile, type SpinProfile } from './solverWorkspaceModel';
import { cliCommandRequestForDesktop, serializeCliCommandArguments } from './cliCommandModel';
import type { WorkspaceLanguage } from './workspaceI18n';
export type RecoveryBuildRequest = {
  /** All three editor masks use the same pre-clear logical canvas. */
  startMask: bigint; middleMask: bigint; resultMask: bigint; height: number;
  firstSupply: string; secondSupply: string;
  maxEarly: 'auto' | number; allowPieceExchange: boolean; holdEnabled: boolean;
  preserveB2B: boolean; rule: RuleProfile; spinProfile: SpinProfile;
};
export function createRecoveryBuildRequest(): RecoveryBuildRequest {
  return { startMask: 0n, middleMask: 0n, resultMask: 0n, height: 8,
    firstSupply: '', secondSupply: '', maxEarly: 'auto', allowPieceExchange: false,
    holdEnabled: true, preserveB2B: false, rule: 'srs-plus', spinProfile: 'all-spin-plus' };
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
/** Translate only at the execution boundary, never when changing the selected color.
 * Full middle rows are absent from the engine's second Build coordinate frame. */
export function recoveryResultForExecution(request: RecoveryBuildRequest): bigint {
  let result = 0n; let outputRow = 0;
  const completed = request.startMask | request.middleMask;
  for (let y = 0; y < request.height; y++) {
    const shift = BigInt(y * 10);
    if (((completed >> shift) & 1023n) === 1023n) continue;
    result |= ((request.resultMask >> shift) & 1023n) << BigInt(outputRow * 10);
    outputRow++;
  }
  return result;
}
export function recoveryEarlyChoices(request: RecoveryBuildRequest): number[] {
  // Every early result lock consumes four different target cells. Do not guess
  // a supply cardinality by counting characters of the pattern language.
  return Array.from({ length: Math.floor(countRecoveryCells(request.resultMask) / 4) + 1 }, (_, i) => i);
}
export function resizeRecoveryBuild(request: RecoveryBuildRequest, height: number): RecoveryBuildRequest {
  if (!Number.isInteger(height) || height < 1 || height > 24) return request;
  const limit = (1n << BigInt(height * 10)) - 1n;
  return { ...request, height, startMask: request.startMask & limit,
    middleMask: request.middleMask & limit, resultMask: request.resultMask & limit };
}
export function validateRecoveryBuildRequest(request: RecoveryBuildRequest): string[] {
  const errors: string[] = [];
  if (!Number.isInteger(request.height) || request.height < 1 || request.height > 24) return ['height'];
  const limit = 1n << BigInt(request.height * 10);
  if ([request.startMask, request.middleMask, request.resultMask].some(mask => mask < 0n || mask >= limit)) errors.push('board');
  if ((request.startMask & request.middleMask) !== 0n || ((request.startMask | request.middleMask) & request.resultMask) !== 0n) errors.push('overlap');
  if ([request.middleMask, request.resultMask].some(mask => mask === 0n || countRecoveryCells(mask) % 4 !== 0)) errors.push('target-area');
  if (!request.firstSupply.trim() || !request.secondSupply.trim()) errors.push('supply');
  if (request.maxEarly !== 'auto' && (!Number.isSafeInteger(request.maxEarly) || request.maxEarly < 0)) errors.push('early');
  return errors;
}
export function recoveryBuildArguments(request: RecoveryBuildRequest): string[] {
  return ['clearra', 'recovery', 'build', '--start-mask', boardMaskHex(request.startMask),
    '--middle-mask', boardMaskHex(request.middleMask), '--result-mask', boardMaskHex(recoveryResultForExecution(request)),
    '--height', String(request.height), '--first-supply', request.firstSupply.trim(),
    '--second-supply', request.secondSupply.trim(), '--max-early', String(request.maxEarly),
    request.allowPieceExchange ? '--allow-piece-exchange' : '--no-piece-exchange',
    request.holdEnabled ? '--hold' : '--no-hold', request.preserveB2B ? '--preserve-b2b' : '--no-preserve-b2b',
    '--initial-b2b', '1', '--rule', request.rule, '--spin-profile', request.spinProfile];
}
export const recoveryBuildCommand = (request: RecoveryBuildRequest): string => serializeCliCommandArguments(recoveryBuildArguments(request));
export const recoveryBuildDesktopRequest = (request: RecoveryBuildRequest, language: WorkspaceLanguage) => cliCommandRequestForDesktop(recoveryBuildArguments(request), language);
