// Bounded physical timeline for transport/render parity, not reachability or
// complete-family authority. Six vertical I locks clear a 24-row initial board.
export function fullHeightPcReplayWitness() {
  const height = 24;
  let holes = 0n;
  for (let row = 0; row < height; row += 1) holes |= 1n << BigInt(row * 10 + 9);
  let board = ((1n << 240n) - 1n) ^ holes;
  const placement = 0x8020080200n;
  const hex = (value) => `0x${value.toString(16).padStart(64, '0')}`;
  const steps = [];
  for (let index = 0; index < 6; index += 1) {
    const afterPlacement = board | placement;
    const afterClear = afterPlacement >> 40n;
    steps.push({
      step_index: String(index), operation_id: String(index), active_piece: 'I',
      input_cursor: String(index), output_cursor: String(index + 1),
      input_hold_piece: null, output_hold_piece: null, hold_decision: 'none',
      rotation: '1', x: '9', y: '0', placement_mask: hex(placement),
      board_before_mask: hex(board), board_after_placement_mask: hex(afterPlacement),
      board_after_line_clear_mask: hex(afterClear), cleared_row_mask: '0x000000000000000f',
      cleared_lines: '4', line_clear_identity: 'rows:000000000000000f:count:4'
    });
    board = afterClear;
  }
  return {
    candidate_id: '1', producer_candidate_id: '1', pattern_id: '0',
    trace_identity: 'full-height-physical-fixture', normalized_trace_key: 'trk2:physical-fixture',
    consumed_piece_count: '6', terminal_hold_piece: null, steps
  };
}
