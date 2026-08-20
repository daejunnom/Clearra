import assert from 'node:assert/strict';

import type { ClearraWasmSearchReport } from '../src/lib/wasm/wasmCommandClient.ts';

type TerminalSupplyMetadata = Pick<
  ClearraWasmSearchReport,
  | 'supply_window_resolution'
  | 'projects_unplaced_lookahead'
  | 'projects_standard_bag_lookahead'
  | 'source_sequence_length'
  | 'total_possible_pattern_count'
>;

const metadata: TerminalSupplyMetadata = {
  supply_window_resolution: 'projected-terminal-lookahead',
  projects_unplaced_lookahead: true,
  projects_standard_bag_lookahead: false,
  source_sequence_length: 7,
  total_possible_pattern_count: '1'
};

assert.deepEqual(metadata, {
  supply_window_resolution: 'projected-terminal-lookahead',
  projects_unplaced_lookahead: true,
  projects_standard_bag_lookahead: false,
  source_sequence_length: 7,
  total_possible_pattern_count: '1'
});
