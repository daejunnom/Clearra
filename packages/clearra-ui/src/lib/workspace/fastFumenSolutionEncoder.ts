import type {
  SolutionExportPage,
  SolutionPiece
} from './solutionExport';
import {
  escapeFumenComment,
  FumenCommentCodecError
} from './ctk3Codec';

const BOARD_WIDTH = 10;
const FUMEN_VISIBLE_HEIGHT = 23;
const FUMEN_FIELD_HEIGHT = 24;
const FUMEN_FIELD_CELLS = BOARD_WIDTH * FUMEN_FIELD_HEIGHT;
const FUMEN_ALPHABET =
  'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
const EMPTY_ACTION = 30_720;
const COMMENT_ACTION_FLAG = 61_440;
const COMMENT_BASE = 96;
const COMMENT_TABLE =
  ' !"#$%&\'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~';
const VALUE_CHUNK_SIZE = 65_536;

const PIECE_CODES: Record<SolutionPiece, number> = {
  I: 1,
  L: 2,
  O: 3,
  Z: 4,
  T: 5,
  J: 6,
  S: 7
};

export function encodeFastColoredFumenPages(
  pages: Iterable<SolutionExportPage>
): string {
  const encoder = new FastColoredFumenEncoder();
  let pageCount = 0;
  for (const page of pages) {
    encoder.append(page);
    pageCount += 1;
  }
  if (pageCount === 0) throw new Error('invalid-page');
  return encoder.finish();
}

export class FastColoredFumenEncoder {
  private readonly values = new FumenValueWriter();
  private previous = new Uint8Array(FUMEN_FIELD_CELLS);
  private current = new Uint8Array(FUMEN_FIELD_CELLS);
  private lastRepeatIndex = -1;
  private repeatCount = 0;
  private previousComment: string | undefined = '';
  private pageCount = 0;

  append(page: SolutionExportPage) {
    if (!Number.isInteger(page.height) || page.height < 1 || page.height > 24) {
      throw new Error('invalid-page');
    }
    this.current.fill(0);
    paintMask(this.current, page.initialMask, 8);
    for (const placement of page.placements) {
      paintMask(this.current, placement.mask, PIECE_CODES[placement.piece]);
    }
    if (highestOccupiedRow(this.current) >= FUMEN_VISIBLE_HEIGHT) {
      throw new Error('fumen-height-unsupported');
    }

    const currentComment =
      page.comment !== undefined && (this.pageCount !== 0 || page.comment !== '')
        ? page.comment
        : undefined;
    const changedComment =
      currentComment !== undefined && currentComment !== this.previousComment
        ? currentComment
        : undefined;
    const escapedComment =
      changedComment === undefined
        ? undefined
        : escapeFumenComment(changedComment);
    this.writeFieldDiff();
    this.values.push(
      EMPTY_ACTION + (changedComment === undefined ? 0 : COMMENT_ACTION_FLAG),
      3
    );
    if (escapedComment !== undefined) this.writeComment(escapedComment);
    this.previousComment = currentComment;
    this.pageCount += 1;
    clearFullLines(this.current);
    const swap = this.previous;
    this.previous = this.current;
    this.current = swap;
  }

  finish(): string {
    return `v115@${formatFumenPayload(this.values.toString())}`;
  }

  private writeFieldDiff() {
    let previousDiff = this.diffAt(0, 0);
    let runLength = 0;
    const runs: Array<{ diff: number; length: number }> = [];
    for (let yIndex = 0; yIndex < FUMEN_FIELD_HEIGHT; yIndex += 1) {
      for (let x = 0; x < BOARD_WIDTH; x += 1) {
        const diff = this.diffAt(x, yIndex);
        if (diff === previousDiff) {
          runLength += 1;
        } else {
          runs.push({ diff: previousDiff, length: runLength });
          previousDiff = diff;
          runLength = 1;
        }
      }
    }
    runs.push({ diff: previousDiff, length: runLength });

    const unchanged =
      runs.length === 1 &&
      runs[0].diff === 8 &&
      runs[0].length === FUMEN_FIELD_CELLS;
    if (!unchanged) {
      for (const run of runs) {
        this.values.push(run.diff * FUMEN_FIELD_CELLS + run.length - 1, 2);
      }
      this.lastRepeatIndex = -1;
      this.repeatCount = 0;
      return;
    }
    if (this.lastRepeatIndex < 0 || this.repeatCount === 63) {
      this.values.push(8 * FUMEN_FIELD_CELLS + FUMEN_FIELD_CELLS - 1, 2);
      this.lastRepeatIndex = this.values.length;
      this.repeatCount = 0;
      this.values.push(0);
      return;
    }
    this.repeatCount += 1;
    this.values.set(this.lastRepeatIndex, this.repeatCount);
  }

  private diffAt(x: number, yIndex: number): number {
    const y = FUMEN_VISIBLE_HEIGHT - yIndex - 1;
    const index = (y + 1) * BOARD_WIDTH + x;
    return this.current[index] - this.previous[index] + 8;
  }

  private writeComment(escaped: string) {
    this.values.push(escaped.length, 2);
    for (let offset = 0; offset < escaped.length; offset += 4) {
      let value = 0;
      let multiplier = 1;
      for (let index = 0; index < 4; index += 1) {
        const character = escaped[offset + index];
        if (character === undefined) break;
        const code = COMMENT_TABLE.indexOf(character);
        if (code < 0) {
          throw new FumenCommentCodecError('invalid-fumen-comment');
        }
        value += code * multiplier;
        multiplier *= COMMENT_BASE;
      }
      this.values.push(value, 5);
    }
  }
}

class FumenValueWriter {
  private readonly chunks: Uint8Array[] = [new Uint8Array(VALUE_CHUNK_SIZE)];
  private usedInLastChunk = 0;
  length = 0;

  push(value: number, splitCount = 1) {
    let current = value;
    for (let count = 0; count < splitCount; count += 1) {
      this.pushValue(current % 64);
      current = Math.floor(current / 64);
    }
  }

  set(index: number, value: number) {
    if (index < 0 || index >= this.length || value < 0 || value >= 64) {
      throw new Error('invalid-fumen-buffer-index');
    }
    const chunkIndex = Math.floor(index / VALUE_CHUNK_SIZE);
    const chunkOffset = index % VALUE_CHUNK_SIZE;
    this.chunks[chunkIndex][chunkOffset] = FUMEN_ALPHABET.charCodeAt(value);
  }

  toString(): string {
    const decoder = new TextDecoder('ascii');
    return this.chunks
      .map((chunk, index) => {
        const used =
          index === this.chunks.length - 1
            ? this.usedInLastChunk
            : VALUE_CHUNK_SIZE;
        return decoder.decode(chunk.subarray(0, used));
      })
      .join('');
  }

  private pushValue(value: number) {
    if (this.usedInLastChunk === VALUE_CHUNK_SIZE) {
      this.chunks.push(new Uint8Array(VALUE_CHUNK_SIZE));
      this.usedInLastChunk = 0;
    }
    this.chunks[this.chunks.length - 1][this.usedInLastChunk] =
      FUMEN_ALPHABET.charCodeAt(value);
    this.usedInLastChunk += 1;
    this.length += 1;
  }
}

function paintMask(field: Uint8Array, source: bigint, value: number) {
  let mask = source;
  while (mask !== 0n) {
    const bit = trailingZeroes(mask);
    const x = bit % BOARD_WIDTH;
    const y = Math.floor(bit / BOARD_WIDTH);
    if (y >= FUMEN_VISIBLE_HEIGHT) {
      throw new Error('fumen-height-unsupported');
    }
    field[(y + 1) * BOARD_WIDTH + x] = value;
    mask &= mask - 1n;
  }
}

function highestOccupiedRow(field: Uint8Array): number {
  for (let y = FUMEN_VISIBLE_HEIGHT - 1; y >= 0; y -= 1) {
    const offset = (y + 1) * BOARD_WIDTH;
    for (let x = 0; x < BOARD_WIDTH; x += 1) {
      if (field[offset + x] !== 0) return y;
    }
  }
  return 0;
}

function clearFullLines(field: Uint8Array) {
  let targetY = 0;
  for (let sourceY = 0; sourceY < FUMEN_VISIBLE_HEIGHT; sourceY += 1) {
    const sourceOffset = (sourceY + 1) * BOARD_WIDTH;
    let full = true;
    for (let x = 0; x < BOARD_WIDTH; x += 1) {
      if (field[sourceOffset + x] === 0) {
        full = false;
        break;
      }
    }
    if (full) continue;
    const targetOffset = (targetY + 1) * BOARD_WIDTH;
    if (targetOffset !== sourceOffset) {
      field.copyWithin(targetOffset, sourceOffset, sourceOffset + BOARD_WIDTH);
    }
    targetY += 1;
  }
  field.fill(0, (targetY + 1) * BOARD_WIDTH);
}

function formatFumenPayload(payload: string): string {
  if (payload.length < 41) return payload;
  const parts = [payload.slice(0, 42)];
  for (let offset = 42; offset < payload.length; offset += 47) {
    parts.push(payload.slice(offset, offset + 47));
  }
  return parts.join('?');
}

function trailingZeroes(value: bigint): number {
  let count = 0;
  let current = value;
  while ((current & 1n) === 0n) {
    current >>= 1n;
    count += 1;
  }
  return count;
}
