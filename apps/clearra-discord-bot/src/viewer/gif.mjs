import { operationCells } from "ctk3";

const DEFAULT_TILE_SIZE = 20;
const DEFAULT_DELAY_MS = 500;
const DEFAULT_MAX_BYTES = 24 * 1024 * 1024;
const DEFAULT_MAX_FRAMES = 128;
const MIN_VIEW_ROWS = 4;
const TOP_EDGE = 1;
const LEFT_EDGE = 2;
const BOTTOM_EDGE = 4;
const RIGHT_EDGE = 8;
const ALL_EDGES = TOP_EDGE | LEFT_EDGE | BOTTOM_EDGE | RIGHT_EDGE;
// Matches the default field palette in CtkColorBoardEditor.svelte.
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
  [0, 0, 0],
];
const COLOR_INDEX = new Map([
  [null, 0],
  ["G", 2],
  ["I", 3],
  ["O", 4],
  ["T", 5],
  ["S", 6],
  ["Z", 7],
  ["J", 8],
  ["L", 9],
]);

export class GifRenderLimitError extends Error {
  constructor(message) {
    super(message);
    this.name = "GifRenderLimitError";
  }
}

export function renderDocumentGif(document, options = {}) {
  const maxFrames = integerOption(
    options.maxFrames,
    DEFAULT_MAX_FRAMES,
    1,
    4096,
  );
  validateDocument(document, maxFrames);
  const tileSize = integerOption(options.tileSize, DEFAULT_TILE_SIZE, 8, 48);
  const delayMs = integerOption(options.delayMs, DEFAULT_DELAY_MS, 20, 60_000);
  const maxBytes = integerOption(
    options.maxBytes,
    DEFAULT_MAX_BYTES,
    1024,
    256 * 1024 * 1024,
  );
  const viewRows = visibleRows(document);
  const width = document.width * tileSize;
  const height = viewRows * tileSize;
  const writer = new ByteWriter(maxBytes);

  writeHeader(writer, width, height);
  writeLoopExtension(writer);
  for (const page of document.pages) {
    writeGraphicControlExtension(writer, delayMs);
    writeImageFrame(
      writer,
      renderPage(document.width, viewRows, tileSize, page),
      width,
      height,
    );
  }
  writer.byte(0x3b);
  return writer.finish();
}

function validateDocument(document, maxFrames) {
  if (
    !document ||
    !Number.isInteger(document.width) ||
    document.width < 1 ||
    document.width > 31 ||
    !Array.isArray(document.pages) ||
    document.pages.length === 0
  ) {
    throw new Error("The viewer document is invalid.");
  }
  if (document.pages.length > maxFrames) {
    throw new GifRenderLimitError(
      `The viewer document exceeds the ${maxFrames}-frame GIF limit.`,
    );
  }
  for (const page of document.pages) {
    if (
      !page ||
      !Number.isInteger(page.height) ||
      page.height < 0 ||
      page.height > 31 ||
      !Array.isArray(page.cells) ||
      page.cells.length !== document.width * page.height ||
      page.cells.some((cell) => !COLOR_INDEX.has(cell))
    ) {
      throw new Error("A viewer document page is invalid.");
    }
  }
}

function visibleRows(document) {
  let rows = MIN_VIEW_ROWS;
  for (const page of document.pages) {
    rows = Math.max(rows, page.height || 0);
    for (let index = 0; index < page.cells.length; index += 1) {
      if (page.cells[index] !== null) {
        rows = Math.max(rows, Math.floor(index / document.width) + 1);
      }
    }
    if (page.operation) {
      for (const cell of operationCells(page.operation)) {
        rows = Math.max(rows, cell.y + 1);
      }
    }
  }
  return Math.min(31, rows);
}

function renderPage(width, rows, tileSize, page) {
  const pixelWidth = width * tileSize;
  const pixelHeight = rows * tileSize;
  const pixels = new Uint8Array(pixelWidth * pixelHeight);
  const cells = Array(width * rows).fill(null);
  const owners = new Uint8Array(width * rows);

  for (let y = 0; y < Math.min(rows, page.height); y += 1) {
    for (let x = 0; x < width; x += 1) {
      cells[y * width + x] = page.cells[y * width + x] ?? null;
    }
  }
  if (page.operation) {
    for (const cell of operationCells(page.operation)) {
      if (cell.x < 0 || cell.x >= width || cell.y < 0 || cell.y >= rows) continue;
      cells[cell.y * width + cell.x] = page.operation.piece;
      owners[cell.y * width + cell.x] = 1;
    }
  }
  for (let y = 0; y < rows; y += 1) {
    for (let x = 0; x < width; x += 1) {
      paintCell(pixels, pixelWidth, rows, tileSize, cells, owners, width, x, y);
    }
  }
  return pixels;
}

function paintCell(
  pixels,
  pixelWidth,
  rows,
  tileSize,
  cells,
  owners,
  width,
  x,
  boardY,
) {
  const color = cells[boardY * width + x];
  const owner = owners[boardY * width + x];
  const screenY = rows - boardY - 1;
  const left = x * tileSize;
  const top = screenY * tileSize;
  const fill = COLOR_INDEX.get(color) ?? 2;

  fillRectangle(pixels, pixelWidth, left, top, tileSize, tileSize, fill);
  if (color === null) {
    paintEdges(pixels, pixelWidth, left, top, tileSize, 1, 1, ALL_EDGES);
    return;
  }

  let edgeMask = 0;
  if (!samePlacement(cells, owners, width, rows, x, boardY + 1, color, owner)) {
    edgeMask |= TOP_EDGE;
  }
  if (!samePlacement(cells, owners, width, rows, x - 1, boardY, color, owner)) {
    edgeMask |= LEFT_EDGE;
  }
  if (!samePlacement(cells, owners, width, rows, x, boardY - 1, color, owner)) {
    edgeMask |= BOTTOM_EDGE;
  }
  if (!samePlacement(cells, owners, width, rows, x + 1, boardY, color, owner)) {
    edgeMask |= RIGHT_EDGE;
  }
  paintEdges(pixels, pixelWidth, left, top, tileSize, 11, 10, edgeMask);
}

function samePlacement(cells, owners, width, rows, x, y, color, owner) {
  return x >= 0 &&
    x < width &&
    y >= 0 &&
    y < rows &&
    cells[y * width + x] === color &&
    owners[y * width + x] === owner;
}

function paintEdges(
  pixels,
  pixelWidth,
  left,
  top,
  tileSize,
  highlight,
  border,
  edgeMask,
) {
  if (edgeMask & TOP_EDGE) {
    fillRectangle(pixels, pixelWidth, left, top, tileSize, 1, highlight);
  }
  if (edgeMask & LEFT_EDGE) {
    fillRectangle(pixels, pixelWidth, left, top, 1, tileSize, highlight);
  }
  if (edgeMask & BOTTOM_EDGE) {
    fillRectangle(
      pixels,
      pixelWidth,
      left,
      top + tileSize - 1,
      tileSize,
      1,
      border,
    );
  }
  if (edgeMask & RIGHT_EDGE) {
    fillRectangle(
      pixels,
      pixelWidth,
      left + tileSize - 1,
      top,
      1,
      tileSize,
      border,
    );
  }
}

function fillRectangle(pixels, width, x, y, rectWidth, rectHeight, value) {
  for (let row = 0; row < rectHeight; row += 1) {
    pixels.fill(value, (y + row) * width + x, (y + row) * width + x + rectWidth);
  }
}

function writeHeader(writer, width, height) {
  writer.ascii("GIF89a");
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

function writeLoopExtension(writer) {
  writer.bytes([
    0x21, 0xff, 0x0b,
    ...new TextEncoder().encode("NETSCAPE2.0"),
    0x03, 0x01, 0x00, 0x00, 0x00,
  ]);
}

function writeGraphicControlExtension(writer, delayMs) {
  const delay = Math.max(1, Math.round(delayMs / 10));
  writer.bytes([0x21, 0xf9, 0x04, 0x04]);
  writer.word(delay);
  writer.bytes([0x00, 0x00]);
}

function writeImageFrame(writer, pixels, width, height) {
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

function lzwEncode(input, minimumCodeSize) {
  if (input.length === 0) return new Uint8Array();
  const clearCode = 1 << minimumCodeSize;
  const endCode = clearCode + 1;
  const output = new BitWriter();
  let dictionary;
  let nextCode;
  let codeSize;

  const reset = () => {
    dictionary = new Map();
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

function integerOption(value, fallback, minimum, maximum) {
  const result = value ?? fallback;
  if (!Number.isInteger(result) || result < minimum || result > maximum) {
    throw new Error("A GIF renderer option is out of range.");
  }
  return result;
}

class BitWriter {
  constructor() {
    this.bytes = [];
    this.value = 0;
    this.bits = 0;
  }

  write(value, bitCount) {
    this.value |= value << this.bits;
    this.bits += bitCount;
    while (this.bits >= 8) {
      this.bytes.push(this.value & 0xff);
      this.value >>>= 8;
      this.bits -= 8;
    }
  }

  finish() {
    if (this.bits > 0) this.bytes.push(this.value & 0xff);
    return Uint8Array.from(this.bytes);
  }
}

class ByteWriter {
  constructor(limit) {
    this.limit = limit;
    this.length = 0;
    this.output = new Uint8Array(Math.min(limit, 64 * 1024));
  }

  byte(value) {
    this.ensureCapacity(1);
    this.output[this.length] = value & 0xff;
    this.length += 1;
  }

  word(value) {
    this.byte(value);
    this.byte(value >>> 8);
  }

  bytes(values) {
    this.ensureCapacity(values.length);
    this.output.set(values, this.length);
    this.length += values.length;
  }

  ascii(value) {
    this.bytes(new TextEncoder().encode(value));
  }

  finish() {
    return this.output.slice(0, this.length);
  }

  ensureCapacity(additionalLength) {
    const required = this.length + additionalLength;
    if (required > this.limit) {
      throw new GifRenderLimitError("The rendered GIF exceeds the Discord attachment limit.");
    }
    if (required <= this.output.length) return;
    const nextLength = Math.min(
      this.limit,
      Math.max(required, this.output.length * 2),
    );
    const expanded = new Uint8Array(nextLength);
    expanded.set(this.output.subarray(0, this.length));
    this.output = expanded;
  }
}
