import { performance } from "node:perf_hooks";

import {
  decodeCtk3Async,
  encodeCtk3Async,
  encodeCtk3Compact,
  inspectCtk3,
  openCtk3Document,
} from "../dist/index.js";

const pageCount = parsePageCount(process.argv[2]);
const height = 4;
const base = Array(height * 10).fill(null);
for (const index of [0, 1, 2, 3, 10, 13, 20, 21, 22, 23, 30, 39]) {
  base[index] = "G";
}
const colors = ["I", "O", "T", "S", "Z", "J", "L"];
const pages = Array.from({ length: pageCount }, (_, pageIndex) => {
  const cells = base.slice();
  for (let offset = 0; offset < 10; offset += 1) {
    const index = (pageIndex * 7 + offset * 11) % cells.length;
    if (cells[index] === null) {
      cells[index] = colors[(pageIndex + offset) % colors.length];
    }
  }
  return { height, cells };
});

const before = process.memoryUsage();
const encodeStarted = performance.now();
const value = await encodeCtk3Async(
  { width: 10, pages },
  { workers: 1, segmentPages: 1024 },
);
const encodeMs = performance.now() - encodeStarted;
const afterEncode = process.memoryUsage();

const inspectStarted = performance.now();
const info = inspectCtk3(value);
const inspectMs = performance.now() - inspectStarted;
const reader = openCtk3Document(value, { cacheSegments: 1 });
const firstStarted = performance.now();
reader.readPage(0);
const firstPageMs = performance.now() - firstStarted;
const lastStarted = performance.now();
reader.readPage(pageCount - 1);
const lastPageMs = performance.now() - lastStarted;
reader.clearCache();

let compactEncodedCharacters = null;
let compactEncodeMs = null;
if (pageCount <= 4096) {
  const compactEncodeStarted = performance.now();
  compactEncodedCharacters = encodeCtk3Compact({ width: 10, pages }).length;
  compactEncodeMs = performance.now() - compactEncodeStarted;
}

let fullDecodeMs = null;
if (process.argv.includes("--full-decode")) {
  const decodeStarted = performance.now();
  await decodeCtk3Async(value, { workers: 1 });
  fullDecodeMs = performance.now() - decodeStarted;
}

console.log(
  JSON.stringify(
    {
      page_count: pageCount,
      encoded_characters: value.length,
      segment_count: info.segmentCount,
      serial_encode_ms: round(encodeMs),
      inspect_ms: round(inspectMs),
      first_page_ms: round(firstPageMs),
      last_page_ms: round(lastPageMs),
      compact_encoded_characters: compactEncodedCharacters,
      compact_encode_ms:
        compactEncodeMs === null ? null : round(compactEncodeMs),
      full_decode_ms: fullDecodeMs === null ? null : round(fullDecodeMs),
      heap_delta_mib: round(
        (afterEncode.heapUsed - before.heapUsed) / 1024 / 1024,
      ),
      rss_delta_mib: round((afterEncode.rss - before.rss) / 1024 / 1024),
    },
    null,
    2,
  ),
);

function parsePageCount(value) {
  if (value === undefined) return 1000;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1 || parsed > 1_048_576) {
    throw new RangeError("Page count must be between 1 and 1,048,576.");
  }
  return parsed;
}

function round(value) {
  return Number(value.toFixed(2));
}
