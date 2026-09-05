export const PC_PATH_REPLAY_FRAME_DELAY_MS = 500;

const WIDTH = 10;
const MIN_VIEW_ROWS = 4;
const MAX_VIEW_ROWS = 24;
const PIECES = new Set(["I", "O", "T", "S", "Z", "J", "L"]);

/**
 * Projects Discord's already-canonical PC/Build path result into the same
 * event timeline used by the GUI: initial field, every lock, and only actual
 * clears. Exhaustive path families must be reduced at the executor boundary;
 * this viewer deliberately accepts only that canonical envelope.
 */
export function buildCanonicalPathReplayDocument(structured) {
  const contract = pathReplayContract(structured?.kind);
  if (contract === null) return null;
  const summary = structured.summary;
  if (
    !plainObject(summary) ||
    structured.contract?.command?.kind !== contract.resultContract ||
    summary.capability_id !== contract.capabilityId ||
    summary.result_contract !== contract.resultContract ||
    summary.payload_kind !== contract.payloadKind ||
    summary.witness_contract !== contract.witnessContract ||
    summary.canonical_selection !== "smallest-canonical-candidate-id" ||
    summary.complete !== true
  ) {
    throw new Error("Discord received an invalid canonical path result.");
  }
  const terminalTarget = contract.build
    ? parseCanonicalHexMask(summary.target_terminal_board_mask)
    : 0n;
  if (
    (!contract.build && Object.hasOwn(summary, "target_terminal_board_mask")) ||
    (contract.build && !Object.hasOwn(summary, "target_terminal_board_mask"))
  ) throw new Error("Discord received an invalid path terminal contract.");
  const witness = summary.canonical_witness;
  if (witness === null) return null;
  if (!plainObject(witness) || !Array.isArray(witness.steps) || witness.steps.length === 0) {
    throw new Error("Discord received an invalid canonical path witness.");
  }

  const height = replayHeight(witness.steps);
  const cellCount = WIDTH * height;
  const firstBefore = parseMask(witness.steps[0].board_before_mask, cellCount);
  let cells = maskCells(firstBefore, cellCount, "G");
  let occupied = firstBefore;
  const pages = [page(height, cells)];

  for (let index = 0; index < witness.steps.length; index += 1) {
    const step = witness.steps[index];
    if (!plainObject(step) || step.step_index !== String(index) || !PIECES.has(step.active_piece)) {
      throw new Error("Discord received an invalid path replay step.");
    }
    const before = parseMask(step.board_before_mask, cellCount);
    const placement = parseMask(step.placement_mask, cellCount);
    const afterPlacement = parseMask(step.board_after_placement_mask, cellCount);
    const afterClear = parseMask(step.board_after_line_clear_mask, cellCount);
    const clearedRows = parseMask(step.cleared_row_mask, height);
    if (
      before !== occupied ||
      placement === 0n ||
      (before & placement) !== 0n ||
      (before | placement) !== afterPlacement
    ) {
      throw new Error("Discord received inconsistent path replay lock masks.");
    }
    forEachSetBit(placement, cellCount, (cellIndex) => {
      cells[cellIndex] = step.active_piece;
    });
    occupied = afterPlacement;
    pages.push(page(height, cells));

    const declaredClears = canonicalNonNegativeInteger(step.cleared_lines);
    if (
      popcount(clearedRows) !== declaredClears ||
      !clearedRowsAreFull(afterPlacement, height, clearedRows)
    ) {
      throw new Error("Discord received an inconsistent path replay clear count.");
    }
    if (clearedRows !== 0n) {
      cells = compactClearedRows(cells, height, clearedRows);
      occupied = occupiedMask(cells);
      if (occupied !== afterClear) {
        throw new Error("Discord received an inconsistent path replay clear field.");
      }
      pages.push(page(height, cells));
    } else if (afterClear !== afterPlacement) {
      throw new Error("Discord received a path replay field change without a clear.");
    }
  }
  if (occupied !== terminalTarget) {
    throw new Error("Discord received a path replay with the wrong terminal field.");
  }
  return Object.freeze({
    kind: contract.build ? "build" : "pc",
    frameCount: pages.length,
    document: Object.freeze({ width: WIDTH, pages: Object.freeze(pages) }),
  });
}

// Retained for callers/tests that use the original PC-specific API name.
export function buildCanonicalPcPathReplayDocument(structured) {
  return buildCanonicalPathReplayDocument(structured);
}

function pathReplayContract(kind) {
  if (kind === "pc-path-family.v2") {
    return Object.freeze({
      build: false,
      capabilityId: "pc.path",
      resultContract: "pc-path-family.v2",
      payloadKind: "canonical-pc-path-witness",
      witnessContract: "pc-path-witness.v2",
    });
  }
  if (kind === "build-path-family.v1") {
    return Object.freeze({
      build: true,
      capabilityId: "build.complete-replay-paths",
      resultContract: "build-path-family.v1",
      payloadKind: "canonical-build-path-witness",
      witnessContract: "build-path-witness.v1",
    });
  }
  return null;
}

function replayHeight(steps) {
  let height = MIN_VIEW_ROWS;
  for (const step of steps) {
    for (const value of [
      step?.placement_mask,
      step?.board_before_mask,
      step?.board_after_placement_mask,
      step?.board_after_line_clear_mask,
    ]) {
      const mask = parseCanonicalHexMask(value);
      if (mask !== 0n) {
        height = Math.max(height, Math.ceil(mask.toString(2).length / WIDTH));
      }
    }
  }
  if (height > MAX_VIEW_ROWS) {
    throw new Error("Discord received a path replay above the height limit.");
  }
  return height;
}

function page(height, cells) {
  return Object.freeze({ height, cells: Object.freeze([...cells]) });
}

function maskCells(mask, length, color) {
  const cells = Array(length).fill(null);
  forEachSetBit(mask, length, (index) => {
    cells[index] = color;
  });
  return cells;
}

function compactClearedRows(source, height, clearedRows) {
  const compacted = Array(source.length).fill(null);
  let destinationRow = 0;
  for (let sourceRow = 0; sourceRow < height; sourceRow += 1) {
    if ((clearedRows & (1n << BigInt(sourceRow))) !== 0n) continue;
    const sourceOffset = sourceRow * WIDTH;
    const destinationOffset = destinationRow * WIDTH;
    for (let x = 0; x < WIDTH; x += 1) {
      compacted[destinationOffset + x] = source[sourceOffset + x];
    }
    destinationRow += 1;
  }
  return compacted;
}

function occupiedMask(cells) {
  let mask = 0n;
  for (let index = 0; index < cells.length; index += 1) {
    if (cells[index] !== null) mask |= 1n << BigInt(index);
  }
  return mask;
}

function clearedRowsAreFull(board, height, clearedRows) {
  const fullRow = (1n << BigInt(WIDTH)) - 1n;
  for (let row = 0; row < height; row += 1) {
    if ((clearedRows & (1n << BigInt(row))) === 0n) continue;
    if (((board >> BigInt(row * WIDTH)) & fullRow) !== fullRow) return false;
  }
  return true;
}

function parseMask(value, bitLimit) {
  const mask = parseCanonicalHexMask(value);
  if ((mask >> BigInt(bitLimit)) !== 0n) {
    throw new Error("Discord received a path replay mask outside its board.");
  }
  return mask;
}

function parseCanonicalHexMask(value) {
  if (typeof value !== "string" || !/^0x[0-9a-f]{16}$/u.test(value)) {
    throw new Error("Discord received a noncanonical path replay mask.");
  }
  return BigInt(value);
}

function canonicalNonNegativeInteger(value) {
  if (typeof value !== "string" || !/^(?:0|[1-9][0-9]*)$/u.test(value)) {
    throw new Error("Discord received a noncanonical path replay count.");
  }
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed)) {
    throw new Error("Discord received an oversized path replay count.");
  }
  return parsed;
}

function forEachSetBit(mask, limit, visit) {
  for (let index = 0; index < limit; index += 1) {
    if ((mask & (1n << BigInt(index))) !== 0n) visit(index);
  }
}

function popcount(mask) {
  let count = 0;
  while (mask !== 0n) {
    mask &= mask - 1n;
    count += 1;
  }
  return count;
}

function plainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}
