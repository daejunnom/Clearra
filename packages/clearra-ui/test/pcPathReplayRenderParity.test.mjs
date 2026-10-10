import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { fullHeightPcReplayWitness } from '../../../tests/fixtures/contracts/extended_pc_replay_witness.v1.mjs';

import { build } from 'esbuild';

import { renderDocumentGif } from '../../../apps/clearra-discord-bot/src/viewer/gif.mjs';
import { buildCanonicalPcPathReplayDocument } from '../../../apps/clearra-discord-bot/src/viewer/pc-path-replay.mjs';

const packageRoot = fileURLToPath(new URL('..', import.meta.url));
const bundle = await build({
  bundle: true,
  format: 'esm',
  logLevel: 'silent',
  platform: 'node',
  stdin: {
    contents: `
      export { encodePcPathReplayGif } from './src/lib/workspace/pcPathReplayGif.ts';
      export {
        PC_PATH_REPLAY_FRAME_DELAY_MS,
        groupPcPathWitnesses,
        pcPathCandidateGroupExportPages,
        buildPcPathReplayFrames
      } from './src/lib/workspace/pcPathReplayPresentation.ts';
    `,
    loader: 'ts',
    resolveDir: packageRoot
  },
  write: false
});
const production = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`
);

test('GUI PC replay bytes stay aligned with the Discord grid, palette, and 500ms timing', () => {
  const frames = production.buildPcPathReplayFrames(witness(), 4);
  const guiGif = production.encodePcPathReplayGif(frames);
  const discordGif = renderDocumentGif(
    {
      width: 10,
      pages: frames.map(({ height, cells }) => ({ height, cells }))
    },
    {
      delayMs: production.PC_PATH_REPLAY_FRAME_DELAY_MS,
      maxBytes: 8 * 1024 * 1024,
      maxFrames: 128
    }
  );

  assert.equal(production.PC_PATH_REPLAY_FRAME_DELAY_MS, 500);
  assert.deepEqual(guiGif, discordGif);
});

test('Build export retains original and mirrored paths in one canonical group', () => {
  const original = witness();
  const left = '0x000000000000000f';
  const right = '0x00000000000003c0';
  const empty = '0x0000000000000000';
  original.steps[0] = { ...original.steps[0], x: '0', placement_mask: left,
    board_before_mask: empty, board_after_placement_mask: left,
    board_after_line_clear_mask: left, cleared_row_mask: empty, cleared_lines: '0',
    line_clear_identity: 'rows:0000000000000000:count:0' };
  const mirror = { ...original, normalized_trace_key: 'trk1:mirror',
    steps: [{ ...original.steps[0], x: '6', placement_mask: right,
      board_after_placement_mask: right, board_after_line_clear_mask: right }] };
  const [group] = production.groupPcPathWitnesses([original, mirror]);
  assert.equal(group.witnessCount, 2);
  assert.equal(production.pcPathCandidateGroupExportPages(group, 4, [left, right]).length, 2);
  assert.throws(() => production.pcPathCandidateGroupExportPages(group, 4, [left]), /authorized/u);
});

test('GUI and Discord render the same full-height PC timeline without explicit mask-width metadata', () => {
  const member = fullHeightPcReplayWitness();
  const frames = production.buildPcPathReplayFrames(member, 24);
  const replay = buildCanonicalPcPathReplayDocument({
    kind: 'pc-path-family.v2', contract: { command: { kind: 'pc-path-family.v2' } },
    summary: { capability_id: 'pc.path', result_contract: 'pc-path-family.v2',
      payload_kind: 'canonical-pc-path-witness', witness_contract: 'pc-path-witness.v2',
      canonical_selection: 'smallest-canonical-candidate-id', complete: true, canonical_witness: member }
  });
  assert.equal(frames.length, 13);
  assert.deepEqual(frames.map(({ height, cells }) => ({ height, cells })), replay.document.pages);
  assert.deepEqual(production.encodePcPathReplayGif(frames), renderDocumentGif(replay.document, { delayMs: 500 }));
  const [group] = production.groupPcPathWitnesses([member]);
  const [page] = production.pcPathCandidateGroupExportPages(group, 24);
  assert.equal(page.height, 24);
  assert.equal(page.placements.length, 6);
  assert.ok(page.initialMask >> 192n);
  const bad = structuredClone(member);
  bad.steps[1].board_after_line_clear_mask = '0x0000000000000000';
  assert.throws(() => production.buildPcPathReplayFrames(bad, 24), /not canonical/u);
});

function witness() {
  return {
    candidate_id: '1',
    producer_candidate_id: '37',
    pattern_id: '0',
    trace_identity: 'trace-a',
    normalized_trace_key: 'trk1:trace-a',
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
