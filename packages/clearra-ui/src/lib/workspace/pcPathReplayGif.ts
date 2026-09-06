import {
  PC_PATH_REPLAY_FRAME_DELAY_MS,
  type PcPathReplayCell,
  type PcPathReplayFrame
} from './pcPathReplayPresentation';

const TILE_SIZE = 20;
const MAX_GIF_BYTES = 8 * 1024 * 1024;
const TOP_EDGE = 1;
const LEFT_EDGE = 2;
const BOTTOM_EDGE = 4;
const RIGHT_EDGE = 8;
const ALL_EDGES = TOP_EDGE | LEFT_EDGE | BOTTOM_EDGE | RIGHT_EDGE;

// Kept byte-for-byte aligned with the bounded Discord viewer palette.
const PALETTE = [
  [30, 41, 39],
  [63, 74, 72],
  [123, 133, 129],
  [85, 203, 211],
  [243, 207, 77],
  [182, 106, 208],
  [101, 199, 120],
  [233, 110, 110],
  [98, 138, 224],
  [239, 156, 77],
  [38, 50, 46],
  [103, 116, 111],
  [255, 255, 255],
  [0, 0, 0],
  [0, 0, 0],
  [0, 0, 0]
] as const;

const COLOR_INDEX = new Map<PcPathReplayCell, number>([
  [null, 0],
  ['G', 2],
  ['I', 3],
  ['O', 4],
  ['T', 5],
  ['S', 6],
  ['Z', 7],
  ['J', 8],
  ['L', 9]
]);

/** Encodes the validated replay frames as a looping, local-only GIF. */
export function encodePcPathReplayGif(frames: readonly PcPathReplayFrame[]): Uint8Array {
  const first = frames[0];
  if (!first || frames.some((frame) =>
    frame.width !== first.width ||
    frame.height !== first.height ||
    frame.cells.length !== frame.width * frame.height
  )) {
    throw new Error('The PC path GIF frames do not share one board shape.');
  }
  const width = first.width * TILE_SIZE;
  const height = first.height * TILE_SIZE;
  const writer = new ByteWriter(MAX_GIF_BYTES);
  writeHeader(writer, width, height);
  writeLoopExtension(writer);
  for (const frame of frames) {
    writeGraphicControlExtension(writer, PC_PATH_REPLAY_FRAME_DELAY_MS);
    writeImageFrame(writer, renderFrame(frame, TILE_SIZE), width, height);
  }
  writer.byte(0x3b);
  return writer.finish();
}

function renderFrame(frame: PcPathReplayFrame, tileSize: number): Uint8Array {
  const pixelWidth = frame.width * tileSize;
  const pixelHeight = frame.height * tileSize;
  const pixels = new Uint8Array(pixelWidth * pixelHeight);
  for (let y = 0; y < frame.height; y += 1) {
    for (let x = 0; x < frame.width; x += 1) {
      paintCell(pixels, pixelWidth, frame, x, y, tileSize);
    }
  }
  return pixels;
}

function paintCell(
  pixels: Uint8Array,
  pixelWidth: number,
  frame: PcPathReplayFrame,
  x: number,
  boardY: number,
  tileSize: number
) {
  const color = frame.cells[boardY * frame.width + x];
  const screenY = frame.height - boardY - 1;
  const left = x * tileSize;
  const top = screenY * tileSize;
  const fill = COLOR_INDEX.get(color) ?? 2;
  fillRectangle(pixels, pixelWidth, left, top, tileSize, tileSize, fill);
  if (color === null) {
    paintEdges(pixels, pixelWidth, left, top, tileSize, 1, 1, ALL_EDGES);
    return;
  }

  let edgeMask = 0;
  if (!sameColor(frame, x, boardY + 1, color)) edgeMask |= TOP_EDGE;
  if (!sameColor(frame, x - 1, boardY, color)) edgeMask |= LEFT_EDGE;
  if (!sameColor(frame, x, boardY - 1, color)) edgeMask |= BOTTOM_EDGE;
  if (!sameColor(frame, x + 1, boardY, color)) edgeMask |= RIGHT_EDGE;
  paintEdges(pixels, pixelWidth, left, top, tileSize, 11, 10, edgeMask);
}

function sameColor(
  frame: PcPathReplayFrame,
  x: number,
  y: number,
  color: PcPathReplayCell
): boolean {
  return x >= 0 &&
    x < frame.width &&
    y >= 0 &&
    y < frame.height &&
    frame.cells[y * frame.width + x] === color;
}

function paintEdges(
  pixels: Uint8Array,
  pixelWidth: number,
  left: number,
  top: number,
  tileSize: number,
  highlight: number,
  border: number,
  edgeMask: number
) {
  if (edgeMask & TOP_EDGE) fillRectangle(pixels, pixelWidth, left, top, tileSize, 1, highlight);
  if (edgeMask & LEFT_EDGE) fillRectangle(pixels, pixelWidth, left, top, 1, tileSize, highlight);
  if (edgeMask & BOTTOM_EDGE) {
    fillRectangle(pixels, pixelWidth, left, top + tileSize - 1, tileSize, 1, border);
  }
  if (edgeMask & RIGHT_EDGE) {
    fillRectangle(pixels, pixelWidth, left + tileSize - 1, top, 1, tileSize, border);
  }
}

function fillRectangle(
  pixels: Uint8Array,
  width: number,
  x: number,
  y: number,
  rectangleWidth: number,
  rectangleHeight: number,
  value: number
) {
  for (let row = 0; row < rectangleHeight; row += 1) {
    pixels.fill(value, (y + row) * width + x, (y + row) * width + x + rectangleWidth);
  }
}

function writeHeader(writer: ByteWriter, width: number, height: number) {
  writer.ascii('GIF89a');
  writer.word(width);
  writer.word(height);
  writer.byte(0xf3);
  writer.byte(0);
  writer.byte(0);
  for (const [red, green, blue] of PALETTE) {
    writer.byte(red);
    writer.byte(green);
    writer.byte(blue);
  }
}

function writeLoopExtension(writer: ByteWriter) {
  writer.bytes([
    0x21, 0xff, 0x0b,
    ...new TextEncoder().encode('NETSCAPE2.0'),
    0x03, 0x01, 0x00, 0x00, 0x00
  ]);
}

function writeGraphicControlExtension(writer: ByteWriter, delayMs: number) {
  const delay = Math.max(1, Math.round(delayMs / 10));
  writer.bytes([0x21, 0xf9, 0x04, 0x04]);
  writer.word(delay);
  writer.bytes([0x00, 0x00]);
}

function writeImageFrame(
  writer: ByteWriter,
  pixels: Uint8Array,
  width: number,
  height: number
) {
  writer.byte(0x2c);
  writer.word(0);
  writer.word(0);
  writer.word(width);
  writer.word(height);
  writer.byte(0);

  const minimumCodeSize = 4;
  writer.byte(minimumCodeSize);
  const compressed = lzwEncode(pixels, minimumCodeSize);
  for (let offset = 0; offset < compressed.length; offset += 255) {
    const length = Math.min(255, compressed.length - offset);
    writer.byte(length);
    writer.bytes(compressed.subarray(offset, offset + length));
  }
  writer.byte(0);
}

function lzwEncode(input: Uint8Array, minimumCodeSize: number): Uint8Array {
  if (input.length === 0) return new Uint8Array();
  const clearCode = 1 << minimumCodeSize;
  const endCode = clearCode + 1;
  const output = new BitWriter();
  let dictionary = new Map<number, number>();
  let nextCode = endCode + 1;
  let codeSize = minimumCodeSize + 1;
  const reset = () => {
    dictionary = new Map<number, number>();
    nextCode = endCode + 1;
    codeSize = minimumCodeSize + 1;
  };

  reset();
  output.write(clearCode, codeSize);
  let prefix = input[0];
  for (let index = 1; index < input.length; index += 1) {
    const suffix = input[index];
    const key = prefix * 256 + suffix;
    const existing = dictionary.get(key);
    if (existing !== undefined) {
      prefix = existing;
      continue;
    }
    output.write(prefix, codeSize);
    if (nextCode < 4096) {
      dictionary.set(key, nextCode);
      nextCode += 1;
      if (nextCode > 1 << codeSize && codeSize < 12) codeSize += 1;
    } else {
      output.write(clearCode, codeSize);
      reset();
    }
    prefix = suffix;
  }
  output.write(prefix, codeSize);
  output.write(endCode, codeSize);
  return output.finish();
}

class BitWriter {
  private output: number[] = [];
  private value = 0;
  private bits = 0;

  write(value: number, bitCount: number) {
    this.value |= value << this.bits;
    this.bits += bitCount;
    while (this.bits >= 8) {
      this.output.push(this.value & 0xff);
      this.value >>>= 8;
      this.bits -= 8;
    }
  }

  finish(): Uint8Array {
    if (this.bits > 0) this.output.push(this.value & 0xff);
    return Uint8Array.from(this.output);
  }
}

class ByteWriter {
  private readonly limit: number;
  private length = 0;
  private output: Uint8Array;

  constructor(limit: number) {
    this.limit = limit;
    this.output = new Uint8Array(Math.min(limit, 64 * 1024));
  }

  byte(value: number) {
    this.ensureCapacity(1);
    this.output[this.length] = value & 0xff;
    this.length += 1;
  }

  word(value: number) {
    this.byte(value);
    this.byte(value >>> 8);
  }

  bytes(values: ArrayLike<number>) {
    this.ensureCapacity(values.length);
    this.output.set(values, this.length);
    this.length += values.length;
  }

  ascii(value: string) {
    this.bytes(new TextEncoder().encode(value));
  }

  finish(): Uint8Array {
    return this.output.slice(0, this.length);
  }

  private ensureCapacity(additionalLength: number) {
    const required = this.length + additionalLength;
    if (required > this.limit) {
      throw new Error('The PC path GIF exceeds the local artifact limit.');
    }
    if (required <= this.output.length) return;
    const nextLength = Math.min(
      this.limit,
      Math.max(required, this.output.length * 2)
    );
    const expanded = new Uint8Array(nextLength);
    expanded.set(this.output.subarray(0, this.length));
    this.output = expanded;
  }
}
