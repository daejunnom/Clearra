import assert from 'node:assert/strict';

import type { ClearraPcPathWitnessPayload } from '../src/lib/wasm/wasmCommandClient';
import { encodePcPathReplayGif } from '../src/lib/workspace/pcPathReplayGif';
import {
  PC_PATH_REPLAY_FRAME_DELAY_MS,
  buildPcPathReplayFrames,
  groupPcPathWitnesses,
  pcPathCandidateGroupExportPages,
  pcPathWitnessExportPage
} from '../src/lib/workspace/pcPathReplayPresentation';
import { PRODUCT_MEMBER_PAGE_SIZE } from '../src/lib/workspace/productResultPager';

const first = witness('1', '0', 'trace-a');
const second = witness('1', '1', 'trace-b');
const third = witness('2', '1', 'trace-c');
const groups = groupPcPathWitnesses([first, second, third]);

assert.equal(groups.length, 2);
assert.equal(groups[0].candidateId, '1');
assert.equal(groups[0].representative, first);
assert.equal(groups[0].witnessCount, 2);
assert.equal(groups[0].distinctPatternCount, 2);
assert.equal(groups[1].candidateId, '2');
assert.equal(groups[1].representative, third);

const largeFirstGroup = Array.from(
  { length: PRODUCT_MEMBER_PAGE_SIZE + 5 },
  (_, index) => witness('10', String(index), `geometry-a-${index}`)
);
const secondGroup = Array.from(
  { length: 3 },
  (_, index) => verticalWitness('20', String(index), `geometry-b-${index}`)
);
const copyGroups = groupPcPathWitnesses([...largeFirstGroup, ...secondGroup]);
assert.equal(copyGroups.length, 2);
assert.equal(
  copyGroups[0].witnesses.slice(0, PRODUCT_MEMBER_PAGE_SIZE).length,
  PRODUCT_MEMBER_PAGE_SIZE,
  'the render slice remains bounded to 100 witnesses'
);
const copiedFirstGeometry = pcPathCandidateGroupExportPages(copyGroups[0], 4);
assert.equal(
  copiedFirstGeometry.length,
  PRODUCT_MEMBER_PAGE_SIZE + 5,
  'copy materializes every witness in the selected geometry beyond the render slice'
);
assert.ok(
  copiedFirstGeometry.every((page) => page.placements[0]?.mask === 0x3c0n),
  'the first outer geometry exports only its own replay witnesses'
);
const copiedSecondGeometry = pcPathCandidateGroupExportPages(copyGroups[1], 4);
assert.equal(copiedSecondGeometry.length, 3);
assert.ok(
  copiedSecondGeometry.every((page) => page.placements[0]?.mask === 0x8020080200n),
  'switching the outer geometry replaces the copied set without mixing the previous group'
);
assert.ok(
  copiedSecondGeometry.every((page) => page.placements[0]?.mask !== 0x3c0n)
);

const exportPage = pcPathWitnessExportPage(first, 1);
assert.ok(exportPage);
assert.equal(exportPage.initialMask, 0x3fn);
assert.deepEqual(exportPage.placements, [{ piece: 'I', mask: 0x3c0n }]);
assert.equal(
  pcPathWitnessExportPage({ ...first, steps: [] }, 1),
  null,
  'an invalid replay cannot make a raw internal trace key the clipboard fallback'
);

const frames = buildPcPathReplayFrames(first, 1);
assert.deepEqual(frames.map((frame) => frame.phase), ['initial', 'lock', 'after-clear']);
assert.deepEqual(frames[0].cells.slice(0, 10), [
  'G', 'G', 'G', 'G', 'G', 'G', null, null, null, null
]);
assert.deepEqual(frames[1].cells.slice(0, 10), [
  'G', 'G', 'G', 'G', 'G', 'G', 'I', 'I', 'I', 'I'
]);
assert.ok(frames.at(-1)?.cells.every((cell) => cell === null));

const gif = encodePcPathReplayGif(frames);
assert.equal(new TextDecoder().decode(gif.subarray(0, 6)), 'GIF89a');
const delays = gifFrameDelays(gif);
assert.deepEqual(delays, [50, 50, 50]);
assert.equal(PC_PATH_REPLAY_FRAME_DELAY_MS, 500);

const fullSequence = witness('3', '0', 'multi-step');
fullSequence.consumed_piece_count = '2';
fullSequence.steps = [
  {
    ...fullSequence.steps[0],
    step_index: '0',
    operation_id: '0',
    input_cursor: '0',
    output_cursor: '1',
    x: '2',
    placement_mask: '0x000000000000003c',
    board_before_mask: '0x0000000000000003',
    board_after_placement_mask: '0x000000000000003f',
    board_after_line_clear_mask: '0x000000000000003f',
    cleared_row_mask: '0x0000000000000000',
    cleared_lines: '0',
    line_clear_identity: 'rows:0000000000000000:count:0'
  },
  {
    ...fullSequence.steps[0],
    step_index: '1',
    operation_id: '1',
    input_cursor: '1',
    output_cursor: '2',
    x: '6',
    placement_mask: '0x00000000000003c0',
    board_before_mask: '0x000000000000003f',
    board_after_placement_mask: '0x00000000000003ff',
    board_after_line_clear_mask: '0x0000000000000000',
    cleared_row_mask: '0x0000000000000001',
    cleared_lines: '1',
    line_clear_identity: 'rows:0000000000000001:count:1'
  }
];
const fullSequenceFrames = buildPcPathReplayFrames(fullSequence, 1);
assert.deepEqual(
  fullSequenceFrames.map((frame) => frame.phase),
  ['initial', 'lock', 'lock', 'after-clear'],
  'a no-clear lock must not fabricate a duplicate after-clear frame'
);
assert.deepEqual(fullSequenceFrames[0].cells.slice(0, 10), [
  'G', 'G', null, null, null, null, null, null, null, null
]);
assert.deepEqual(fullSequenceFrames[1].cells.slice(0, 10), [
  'G', 'G', 'I', 'I', 'I', 'I', null, null, null, null
]);
assert.ok(fullSequenceFrames.at(-1)?.cells.every((cell) => cell === null));

const repeatedLogicalRow = witness('4', '0', 'repeated-logical-row');
repeatedLogicalRow.consumed_piece_count = '2';
repeatedLogicalRow.steps = [
  {
    ...repeatedLogicalRow.steps[0],
    step_index: '0',
    operation_id: '0',
    input_cursor: '0',
    output_cursor: '1',
    board_before_mask: '0x000000000000fc3f',
    board_after_placement_mask: '0x000000000000ffff',
    board_after_line_clear_mask: '0x000000000000003f'
  },
  {
    ...repeatedLogicalRow.steps[0],
    step_index: '1',
    operation_id: '1',
    input_cursor: '1',
    output_cursor: '2',
    board_before_mask: '0x000000000000003f',
    board_after_placement_mask: '0x00000000000003ff',
    board_after_line_clear_mask: '0x0000000000000000'
  }
];
const repeatedLogicalRowPage = pcPathWitnessExportPage(repeatedLogicalRow, 2);
assert.ok(repeatedLogicalRowPage);
assert.equal(repeatedLogicalRowPage.initialMask, 0xfc3fn);
assert.deepEqual(repeatedLogicalRowPage.placements, [
  { piece: 'I', mask: 0x3c0n },
  { piece: 'I', mask: 0xf0000n }
]);

const corrupted = witness('1', '0', 'broken');
corrupted.steps[0].board_after_line_clear_mask = '0x0000000000000001';
assert.throws(
  () => buildPcPathReplayFrames(corrupted, 1),
  /after-clear mask is inconsistent/u
);

const nonFullClear = witness('1', '0', 'non-full-clear');
nonFullClear.steps[0].cleared_row_mask = '0x0000000000000002';
nonFullClear.steps[0].board_after_line_clear_mask = '0x00000000000003ff';
assert.throws(
  () => buildPcPathReplayFrames(nonFullClear, 2),
  /line-clear count is inconsistent/u,
  'a replay must not label a non-full row as a line clear'
);

console.log(JSON.stringify({
  grouped_geometry_candidates: groups.length,
  replay_frames: frames.length,
  replay_delay_ms: PC_PATH_REPLAY_FRAME_DELAY_MS
}));

function witness(
  candidateId: string,
  patternId: string,
  traceIdentity: string
): ClearraPcPathWitnessPayload {
  return {
    candidate_id: candidateId,
    producer_candidate_id: candidateId,
    pattern_id: patternId,
    trace_identity: traceIdentity,
    normalized_trace_key: `trk1:${traceIdentity}`,
    consumed_piece_count: '1',
    terminal_hold_piece: null,
    steps: [{
      step_index: '0',
      operation_id: '0',
      active_piece: 'I',
      input_cursor: '0',
      output_cursor: '1',
      input_hold_piece: null,
      output_hold_piece: null,
      hold_decision: 'none',
      rotation: '0',
      x: '6',
      y: '0',
      placement_mask: '0x00000000000003c0',
      board_before_mask: '0x000000000000003f',
      board_after_placement_mask: '0x00000000000003ff',
      board_after_line_clear_mask: '0x0000000000000000',
      cleared_row_mask: '0x0000000000000001',
      cleared_lines: '1',
      line_clear_identity: 'rows:0000000000000001:count:1'
    }]
  };
}

function verticalWitness(
  candidateId: string,
  patternId: string,
  traceIdentity: string
): ClearraPcPathWitnessPayload {
  const result = witness(candidateId, patternId, traceIdentity);
  const placement = 0x8020080200n;
  const fullBoard = (1n << 40n) - 1n;
  const before = fullBoard ^ placement;
  result.consumed_piece_count = '1';
  result.steps = [{
    ...result.steps[0],
    rotation: '1',
    x: '9',
    placement_mask: canonicalMask(placement),
    board_before_mask: canonicalMask(before),
    board_after_placement_mask: canonicalMask(fullBoard),
    board_after_line_clear_mask: canonicalMask(0n),
    cleared_row_mask: canonicalMask(0xfn),
    cleared_lines: '4',
    line_clear_identity: 'rows:000000000000000f:count:4'
  }];
  return result;
}

function canonicalMask(value: bigint): string {
  return `0x${value.toString(16).padStart(16, '0')}`;
}

function gifFrameDelays(bytes: Uint8Array): number[] {
  const delays: number[] = [];
  for (let index = 0; index + 7 < bytes.length; index += 1) {
    if (
      bytes[index] === 0x21 &&
      bytes[index + 1] === 0xf9 &&
      bytes[index + 2] === 0x04
    ) {
      delays.push(bytes[index + 4] | (bytes[index + 5] << 8));
      index += 7;
    }
  }
  return delays;
}
