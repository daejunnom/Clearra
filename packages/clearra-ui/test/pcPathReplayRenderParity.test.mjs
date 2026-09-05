import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { build } from 'esbuild';

import { renderDocumentGif } from '../../../apps/clearra-discord-bot/src/viewer/gif.mjs';

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
