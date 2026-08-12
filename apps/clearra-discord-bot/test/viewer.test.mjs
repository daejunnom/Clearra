import assert from "node:assert/strict";
import test from "node:test";

import { encodeCtk3, encodeCtk3File } from "ctk3";

import {
  decodeViewerDocument,
  decodeViewerFile,
  extractViewerDocuments,
} from "../src/viewer/document.mjs";
import { renderDocumentGif } from "../src/viewer/gif.mjs";
import { buildClearraViewerUrl } from "../src/viewer/link.mjs";

const document = {
  width: 10,
  pages: [
    {
      height: 2,
      cells: [
        "J", "J", "J", null, null, null, null, null, null, null,
        "J", null, null, null, null, null, null, null, null, null,
      ],
      comment: "first",
    },
    {
      height: 2,
      cells: [
        "J", "J", "J", "O", "O", null, null, null, null, null,
        "J", null, null, "O", "O", null, null, null, null, null,
      ],
      operation: { piece: "T", rotation: "spawn", x: 6, y: 1 },
    },
  ],
};

test("CTK3 input, URL extraction, and viewer link preserve the document", () => {
  const source = encodeCtk3(document);
  const viewerUrl = buildClearraViewerUrl(
    "https://daejunnom.github.io/Clearra/",
    { format: "ctk3", source },
  );
  const extracted = extractViewerDocuments(`inspect ${viewerUrl}`);
  assert.equal(extracted.length, 1);
  assert.deepEqual(extracted[0].document, decodeViewerDocument(source));
  assert.equal(new URL(viewerUrl).searchParams.get("tool"), "ctk");
  assert.equal(new URL(viewerUrl).searchParams.get("ctk"), source);
});

test("Fumen values and query URLs decode through the same viewer document", () => {
  const source = "v115@7gB8HeC8BeB8BeG8CeH8AeD8JeAgH";
  const direct = decodeViewerDocument(source);
  const extracted = extractViewerDocuments(
    `https://example.test/Clearra/?tool=ctk&fumen=${encodeURIComponent(source)}`,
  );
  assert.equal(extracted.length, 1);
  assert.deepEqual(extracted[0].document, direct);
  assert.equal(direct.width, 10);
  assert.equal(direct.pages.length, 1);
});

test("viewer limits preflight CTK3, Fumen, files, and document extraction", () => {
  const source = encodeCtk3(document);
  assert.throws(
    () => decodeViewerDocument(source, { maxPages: 1 }),
    /1-page limit/,
  );
  assert.throws(
    () => decodeViewerDocument(source, { maxSourceChars: source.length - 1 }),
    /character limit/,
  );

  const fumen = "v115@7gB8HeC8BeB8BeG8CeH8AeD8JeAgH";
  assert.throws(
    () => decodeViewerDocument(fumen, { maxSourceChars: fumen.length - 1 }),
    /character limit/,
  );

  const file = encodeCtk3File(document);
  assert.throws(
    () => decodeViewerFile(file, { maxFileBytes: file.byteLength - 1 }),
    /byte limit/,
  );
  assert.throws(
    () => decodeViewerFile(file, { maxPages: 1 }),
    /1-page limit/,
  );

  const secondSource = encodeCtk3({ width: 10, pages: [document.pages[0]] });
  const extracted = extractViewerDocuments(`${source} ${secondSource}`, {
    maxDocuments: 1,
  });
  assert.equal(extracted.length, 1);
  assert.equal(extracted[0].source, source);
});

test("internal GIF encoder emits one decodable image frame per page", () => {
  const gif = renderDocumentGif(document, {
    tileSize: 8,
    delayMs: 120,
    maxBytes: 1024 * 1024,
  });
  const parsed = parseGif(gif);
  assert.equal(parsed.signature, "GIF89a");
  assert.equal(parsed.width, 80);
  assert.equal(parsed.height, 55);
  assert.equal(parsed.frames.length, 2);
  assert.equal(parsed.frames[0].pixels.length, 80 * 55);
  assert.notDeepEqual(parsed.frames[0].pixels, parsed.frames[1].pixels);
  assert.deepEqual(parsed.palette.slice(0, 10), [
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
  ]);

  const firstFrame = parsed.frames[0].pixels;
  const pixelWidth = parsed.width;
  const occupiedTop = 2 * 8;
  assert.equal(firstFrame[occupiedTop * pixelWidth], 11);
  assert.equal(firstFrame[(occupiedTop + 1) * pixelWidth + 1], 8);
  assert.equal(firstFrame[72], 1);
  assert.equal(firstFrame[pixelWidth + 73], 0);
  assert.equal(firstFrame[32 * pixelWidth], 11, "comment panel separator");
  assert.equal(
    firstFrame.subarray(32 * pixelWidth).some((pixel) => pixel === 12),
    true,
    "page comment is rasterized below the board",
  );
  assert.equal(
    parsed.frames[1].pixels.subarray(32 * pixelWidth).some((pixel) => pixel === 12),
    false,
    "a page without a comment leaves the shared panel empty",
  );
});

test("GIF interior pixels use the GUI field editor's default mino colors", () => {
  const colors = ["G", "I", "O", "T", "S", "Z", "J", "L"];
  const expectedRgb = [
    [123, 133, 129],
    [85, 203, 211],
    [243, 207, 77],
    [182, 106, 208],
    [101, 199, 120],
    [233, 110, 110],
    [98, 138, 224],
    [239, 156, 77],
  ];
  const gif = renderDocumentGif({
    width: colors.length,
    pages: [{ height: 1, cells: colors }],
  }, {
    tileSize: 8,
    maxBytes: 1024 * 1024,
  });
  const parsed = parseGif(gif);
  const interiorY = parsed.height - 4;
  const actualRgb = colors.map((_, x) => {
    const colorIndex = parsed.frames[0].pixels[
      interiorY * parsed.width + x * 8 + 4
    ];
    return parsed.palette[colorIndex];
  });

  assert.deepEqual(actualRgb, expectedRgb);
});

test("GIF joins a placed tetromino's cells and keeps only its outer bevel", () => {
  const gif = renderDocumentGif({
    width: 4,
    pages: [{
      height: 2,
      cells: [
        "T", "T", "T", null,
        null, "T", null, null,
      ],
    }],
  }, {
    tileSize: 8,
    maxBytes: 1024 * 1024,
  });
  const parsed = parseGif(gif);
  const pixels = parsed.frames[0].pixels;
  const at = (x, y) => pixels[y * parsed.width + x];

  assert.deepEqual(parsed.palette.slice(10, 12), [
    [38, 50, 46],
    [103, 116, 111],
  ]);
  assert.equal(at(7, 25), 5, "horizontal internal edge on the left cell");
  assert.equal(at(8, 25), 5, "horizontal internal edge on the right cell");
  assert.equal(at(9, 23), 5, "vertical internal edge on the upper cell");
  assert.equal(at(9, 24), 5, "vertical internal edge on the lower cell");
  assert.equal(at(9, 16), 11, "top outer highlight");
  assert.equal(at(0, 25), 11, "left outer highlight");
  assert.equal(at(23, 25), 10, "right outer shadow");
  assert.equal(at(9, 31), 10, "bottom outer shadow");
  assert.equal(at(25, 25), 0, "empty-cell interior remains the board background");
  assert.equal(at(24, 24), 1, "empty-cell grid remains visible");
});

test("GIF joins adjacent garbage cells and keeps only their outer bevel", () => {
  const gif = renderDocumentGif({
    width: 4,
    pages: [{
      height: 2,
      cells: [
        "G", "G", null, null,
        "G", "G", null, null,
      ],
    }],
  }, {
    tileSize: 8,
    maxBytes: 1024 * 1024,
  });
  const parsed = parseGif(gif);
  const pixels = parsed.frames[0].pixels;
  const at = (x, y) => pixels[y * parsed.width + x];

  assert.equal(at(7, 25), 2, "garbage has no right edge inside the region");
  assert.equal(at(8, 25), 2, "adjacent garbage has no left edge inside the region");
  assert.equal(at(1, 23), 2, "garbage has no bottom edge inside the region");
  assert.equal(at(1, 24), 2, "adjacent garbage has no top edge inside the region");
  assert.equal(at(1, 16), 11, "garbage region keeps its top highlight");
  assert.equal(at(0, 25), 11, "garbage region keeps its left highlight");
  assert.equal(at(15, 25), 10, "garbage region keeps its right shadow");
  assert.equal(at(1, 31), 10, "garbage region keeps its bottom shadow");
  assert.equal(at(17, 25), 0, "empty-cell interior remains the board background");
  assert.equal(at(16, 24), 1, "empty-cell grid remains visible");
});

test("GIF keeps an operation's bevel when it touches the same field color", () => {
  const gif = renderDocumentGif({
    width: 4,
    pages: [{
      height: 2,
      cells: [
        null, null, null, null,
        null, null, "T", null,
      ],
      operation: { piece: "T", rotation: "spawn", x: 1, y: 0 },
    }],
  }, {
    tileSize: 8,
    maxBytes: 1024 * 1024,
  });
  const parsed = parseGif(gif);
  const pixels = parsed.frames[0].pixels;
  const at = (x, y) => pixels[y * parsed.width + x];

  assert.equal(at(15, 20), 10, "operation right edge remains a shadow");
  assert.equal(at(16, 20), 11, "same-color field cell keeps its left highlight");
  assert.equal(at(20, 23), 10, "field cell bottom edge remains a shadow");
  assert.equal(at(20, 24), 11, "operation top edge remains a highlight");
  assert.equal(at(7, 28), 5, "operation cells still share their internal edge");
  assert.equal(at(8, 28), 5, "operation neighbor remains one joined piece");
});

test("GIF LZW code widths remain compatible across dictionary boundaries", () => {
  const colors = [null, "G", "I", "O", "T", "S", "Z", "J", "L"];
  const cells = Array.from(
    { length: 10 * 4 },
    (_, index) => colors[(index * 7 + 3) % colors.length],
  );
  const gif = renderDocumentGif({
    width: 10,
    pages: [{ height: 4, cells }],
  }, {
    tileSize: 20,
    maxBytes: 1024 * 1024,
  });

  const parsed = parseGif(gif);
  assert.equal(parsed.width, 200);
  assert.equal(parsed.height, 80);
  assert.equal(parsed.frames.length, 1);
  assert.equal(parsed.frames[0].pixels.length, 200 * 80);
});

test("GIF rendering rejects excessive frames and malformed page cells", () => {
  assert.throws(
    () => renderDocumentGif(document, { maxFrames: 1 }),
    /1-frame GIF limit/,
  );
  assert.throws(
    () =>
      renderDocumentGif({
        width: 10,
        pages: [{ height: 1, cells: ["I"] }],
      }),
    /page is invalid/,
  );
  assert.throws(
    () =>
      renderDocumentGif({
        width: 1,
        pages: [{ height: 1, cells: ["X"] }],
      }),
    /page is invalid/,
  );
});

function parseGif(bytes) {
  let offset = 0;
  const ascii = (length) => {
    const value = new TextDecoder().decode(bytes.subarray(offset, offset + length));
    offset += length;
    return value;
  };
  const byte = () => bytes[offset++];
  const word = () => byte() | (byte() << 8);
  const signature = ascii(6);
  const width = word();
  const height = word();
  const packed = byte();
  byte();
  byte();
  const palette = [];
  if (packed & 0x80) {
    const colorCount = 1 << ((packed & 7) + 1);
    for (let index = 0; index < colorCount; index += 1) {
      palette.push([byte(), byte(), byte()]);
    }
  }

  const frames = [];
  while (offset < bytes.length) {
    const marker = byte();
    if (marker === 0x3b) break;
    if (marker === 0x21) {
      byte();
      skipBlocks();
      continue;
    }
    assert.equal(marker, 0x2c);
    word();
    word();
    const frameWidth = word();
    const frameHeight = word();
    const imagePacked = byte();
    if (imagePacked & 0x80) offset += 3 * (1 << ((imagePacked & 7) + 1));
    const minimumCodeSize = byte();
    const compressed = readBlocks();
    frames.push({
      pixels: decodeLzw(compressed, minimumCodeSize, frameWidth * frameHeight),
    });
  }
  return { signature, width, height, palette, frames };

  function skipBlocks() {
    while (true) {
      const length = byte();
      if (length === 0) return;
      offset += length;
    }
  }

  function readBlocks() {
    const output = [];
    while (true) {
      const length = byte();
      if (length === 0) return Uint8Array.from(output);
      output.push(...bytes.subarray(offset, offset + length));
      offset += length;
    }
  }
}

function decodeLzw(bytes, minimumCodeSize, expectedLength) {
  const clearCode = 1 << minimumCodeSize;
  const endCode = clearCode + 1;
  let bitOffset = 0;
  let codeSize;
  let dictionary;
  let nextCode;
  let previous = null;
  const output = [];

  const reset = () => {
    dictionary = Array.from({ length: clearCode }, (_, value) => [value]);
    dictionary[clearCode] = null;
    dictionary[endCode] = null;
    nextCode = endCode + 1;
    codeSize = minimumCodeSize + 1;
    previous = null;
  };
  const readCode = () => {
    let value = 0;
    for (let bit = 0; bit < codeSize; bit += 1) {
      const index = bitOffset + bit;
      value |= ((bytes[index >>> 3] >>> (index & 7)) & 1) << bit;
    }
    bitOffset += codeSize;
    return value;
  };

  reset();
  while (bitOffset + codeSize <= bytes.length * 8) {
    const code = readCode();
    if (code === clearCode) {
      reset();
      continue;
    }
    if (code === endCode) break;
    let entry = dictionary[code];
    if (!entry && code === nextCode && previous) {
      entry = [...previous, previous[0]];
    }
    assert.ok(entry, `invalid GIF LZW code ${code}`);
    output.push(...entry);
    if (previous && nextCode < 4096) {
      dictionary[nextCode++] = [...previous, entry[0]];
      if (nextCode === 1 << codeSize && codeSize < 12) codeSize += 1;
    }
    previous = entry;
  }
  assert.equal(output.length, expectedLength);
  return Uint8Array.from(output);
}
