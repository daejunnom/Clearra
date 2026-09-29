/** Editor coordinate conversion only. The engine retains its after-middle
 * contract and owns the actual placement/clear history of each recovery. */
export type RecoveryResultFrame = 'shared' | 'after-middle';
export type RecoveryCoordinateInput = {
  height: number; startMask: bigint; middleMask: bigint; resultMask: bigint;
  /** Absent in legacy programmatic requests: keep their after-middle meaning. */
  resultFrame?: RecoveryResultFrame;
};
const ROW = 1023n;

export function recoveryResultFrame(input: RecoveryCoordinateInput): RecoveryResultFrame {
  const frame = input.resultFrame ?? 'after-middle';
  if (frame !== 'shared' && frame !== 'after-middle') throw new RangeError('invalid recovery result frame');
  return frame;
}
function check(input: RecoveryCoordinateInput): void {
  if (!Number.isInteger(input.height) || input.height < 1 || input.height > 24) throw new RangeError('invalid recovery height');
  const limit = 1n << BigInt(input.height * 10);
  if ([input.startMask, input.middleMask, input.resultMask].some(mask => mask < 0n || mask >= limit)) {
    throw new RangeError('recovery mask is outside the input frame');
  }
}
/** Remove precisely the rows completed by Start + Middle. Empty rows, holes,
 * and rows completed only by Result are not deleted by this coordinate map. */
export function projectRecoveryRows(mask: bigint, completed: bigint, height: number): bigint {
  let result = 0n;
  let out = 0;
  for (let y = 0; y < height; y++) {
    const shift = BigInt(y * 10);
    if (((completed >> shift) & ROW) !== ROW) {
      result |= ((mask >> shift) & ROW) << BigInt(out * 10);
      out++;
    }
  }
  return result;
}
export function recoveryResultMaskForEngine(input: RecoveryCoordinateInput): bigint {
  check(input);
  if (recoveryResultFrame(input) === 'after-middle') return input.resultMask;
  const completed = input.startMask | input.middleMask;
  // A shared-frame Result cell on an occupied/completed row is not a cell that
  // may silently disappear during projection. Preserve the draft and reject it.
  if ((completed & input.resultMask) !== 0n) throw new RangeError('shared result overlaps Start or Middle');
  return projectRecoveryRows(input.resultMask, completed, input.height);
}
/** Switch representation without changing the logical target. Grow the visible
 * board when necessary; never clip high cells or move a target to the floor. */
export function changeRecoveryResultFrame<T extends RecoveryCoordinateInput>(
  input: T, target: RecoveryResultFrame
): T {
  check(input);
  if (target !== 'shared' && target !== 'after-middle') throw new RangeError('invalid recovery result frame');
  const current = recoveryResultFrame(input);
  if (target === current) return input;
  if (target === 'after-middle') return { ...input, resultFrame: target, resultMask: recoveryResultMaskForEngine(input) };
  const completed = input.startMask | input.middleMask;
  const retained = projectRecoveryRows(completed, completed, input.height);
  if ((retained & input.resultMask) !== 0n) throw new RangeError('result overlaps retained Start or Middle');
  let resultMask = 0n;
  let logical = 0;
  let height = input.height;
  for (let physical = 0; physical < input.height; physical++, logical++) {
    while (((completed >> BigInt(logical * 10)) & ROW) === ROW) logical++;
    const row = (input.resultMask >> BigInt(physical * 10)) & ROW;
    if (row !== 0n) {
      if (logical >= 24) throw new RangeError('shared result exceeds the 24-row editor');
      resultMask |= row << BigInt(logical * 10);
      height = Math.max(height, logical + 1);
    }
  }
  return { ...input, resultFrame: target, resultMask, height };
}
