import { operationCells } from "ctk3";

const DEFAULT_TILE_SIZE = 20;
const DEFAULT_DELAY_MS = 500;
const DEFAULT_MAX_BYTES = 24 * 1024 * 1024;
const MIN_VIEW_ROWS = 4;
const PALETTE = [
  [22, 31, 29],
  [54, 67, 63],
  [132, 143, 139],
  [74, 194, 214],
  [244, 211, 74],
  [178, 91, 204],
  [91, 194, 118],
  [232, 91, 102],
  [83, 128, 216],
  [238, 153, 75],
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
  validateDocument(document);
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

function validateDocument(document) {
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

  for (let y = 0; y < rows; y += 1) {
    for (let x = 0; x < width; x += 1) {
      paintCell(pixels, pixelWidth, rows, tileSize, x, y, cellAt(page, width, x, y));
    }
  }
  if (page.operation) {
    for (const cell of operationCells(page.operation)) {
      if (cell.x < 0 || cell.x >= width || cell.y < 0 || cell.y >= rows) continue;
      paintCell(
        pixels,
        pixelWidth,
        rows,
        tileSize,
        cell.x,
        cell.y,
        page.operation.piece,
      );
    }
  }
  return pixels;
}

function cellAt(page, width, x, y) {
  if (y >= page.height) return null;
  return page.cells[y * width + x] ?? null;
}

function paintCell(pixels, pixelWidth, rows, tileSize, x, boardY, color) {
  const screenY = rows - boardY - 1;
  const left = x * tileSize;
  const top = screenY * tileSize;
  const fill = COLOR_INDEX.get(color) ?? 2;
  const border = color === null ? 1 : 10;
  const highlight = color === null ? 1 : 11;

  fillRectangle(pixels, pixelWidth, left, top, tileSize, tileSize, fill);
  fillRectangle(pixels, pixelWidth, left, top, tileSize, 1, highlight);
  fillRectangle(pixels, pixelWidth, left, top, 1, tileSize, highlight);
  fillRectangle(
    pixels,
    pixelWidth,
    left,
    top + tileSize - 1,
    tileSize,
    1,
    border,
  );
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
      if (nextCode === 1 << codeSize && codeSize < 12) codeSize += 1;
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
    this.output = [];
  }

  byte(value) {
    if (this.output.length >= this.limit) {
      throw new GifRenderLimitError("The rendered GIF exceeds the Discord attachment limit.");
    }
    this.output.push(value & 0xff);
  }

  word(value) {
    this.byte(value);
    this.byte(value >>> 8);
  }

  bytes(values) {
    if (this.output.length + values.length > this.limit) {
      throw new GifRenderLimitError("The rendered GIF exceeds the Discord attachment limit.");
    }
    for (const value of values) this.output.push(value);
  }

  ascii(value) {
    this.bytes(new TextEncoder().encode(value));
  }

  finish() {
    return Uint8Array.from(this.output);
  }
}
