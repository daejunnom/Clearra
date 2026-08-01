import assert from "node:assert/strict";
import test from "node:test";

import { encodeCtk3 } from "ctk3";

import {
  decodeViewerDocument,
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

test("internal GIF encoder emits one decodable image frame per page", () => {
  const gif = renderDocumentGif(document, {
    tileSize: 8,
    delayMs: 120,
    maxBytes: 1024 * 1024,
  });
  const parsed = parseGif(gif);
  assert.equal(parsed.signature, "GIF89a");
  assert.equal(parsed.width, 80);
  assert.equal(parsed.height, 32);
  assert.equal(parsed.frames.length, 2);
  assert.equal(parsed.frames[0].pixels.length, 80 * 32);
  assert.notDeepEqual(parsed.frames[0].pixels, parsed.frames[1].pixels);
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
  if (packed & 0x80) offset += 3 * (1 << ((packed & 7) + 1));

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
  return { signature, width, height, frames };

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
      if (nextCode === (1 << codeSize) - 1 && codeSize < 12) codeSize += 1;
    }
    previous = entry;
  }
  assert.equal(output.length, expectedLength);
  return Uint8Array.from(output);
}
