import assert from "node:assert/strict";
import { createRequire } from "node:module";
import test from "node:test";

import {
  Ctk3FumenCompatibilityError,
  Ctk3PageLimitError,
  FumenCommentCodecError,
  CTK3_FILE_EXTENSION,
  CTK3_FILE_MIME_TYPE,
  Field,
  Mino,
  decodeCtk3,
  decodeCtk3Async,
  decodeCtk3Exact,
  decodeCtk3File,
  decoder,
  documentDecoder,
  documentEncoder,
  encodeCtk3Bundle,
  encodeCtk3Async,
  encodeCtk3Compact,
  encodeCtk3File,
  encodeCtk3PageSourceAsync,
  encoder,
  inspectCtk3,
  inspectCtk3WithinPageLimit,
  isCtk3File,
  isCtk3,
  openCtk3Document,
  openCtk3DocumentAsync,
  parseCtk3File,
} from "../dist/index.js";
import {
  decoder as fumenDecoder,
  encoder as fumenEncoder,
  Field as TetrisFumenField,
  Mino as TetrisFumenMino,
} from "tetris-fumen";

const require = createRequire(import.meta.url);
const commonJs = require("../dist/index.cjs");

test("decoder matches the tetris-fumen page contract", () => {
  const input = [
    {
      field: Field.create(),
      operation: { type: "T", rotation: "spawn", x: 4, y: 0 },
      comment: "Opening",
      flags: { lock: true, colorize: true },
    },
    {
      operation: { type: "I", rotation: "right", x: 7, y: 2 },
      comment: "Opening",
      flags: { lock: true, colorize: false },
    },
    {
      operation: { type: "L", rotation: "left", x: 2, y: 2 },
      comment: "Next",
      flags: { lock: false, mirror: true },
    },
  ];

  const fumenPages = fumenDecoder.decode(fumenEncoder.encode(input));
  const ctkValue = encoder.encode(input);
  const ctkPages = decoder.decode(ctkValue);

  assert.match(ctkValue, /^ctk3_/);
  assert.equal(ctkPages.length, fumenPages.length);
  for (let index = 0; index < fumenPages.length; index += 1) {
    const expected = fumenPages[index];
    const actual = ctkPages[index];
    assert.equal(actual.index, expected.index);
    assert.equal(actual.comment, expected.comment);
    assert.deepEqual(actual.flags, expected.flags);
    assert.deepEqual(
      operationSnapshot(actual.operation),
      operationSnapshot(expected.operation),
    );
    assert.equal(fieldSnapshot(actual.field), fieldSnapshot(expected.field));
    assert.deepEqual(actual.refs, expected.refs);
    assert.ok(actual.field instanceof TetrisFumenField);
    assert.deepEqual(Object.keys(actual), Object.keys(expected));
    if (actual.operation) {
      assert.ok(actual.operation instanceof TetrisFumenMino);
      assert.ok(actual.mino() instanceof Mino);
      assert.deepEqual(actual.mino().positions(), expected.mino().positions());
    }
  }

  const detached = ctkPages[0].field;
  detached.set(0, 0, "I");
  assert.equal(ctkPages[0].field.at(0, 0), "_");
  ctkPages[0].field = detached;
  assert.equal(ctkPages[0].field.at(0, 0), "I");
});

test("line clear, garbage rise, and mirror produce the same next page", () => {
  const input = [
    {
      field: Field.create("XXXXXXXX__", "X_X_X_X_X_"),
      operation: { type: "O", rotation: "spawn", x: 8, y: 0 },
      comment: "Transform",
      flags: {
        lock: true,
        colorize: true,
        rise: true,
        mirror: true,
      },
    },
    {
      comment: "After",
    },
  ];
  const fumenPages = fumenDecoder.decode(fumenEncoder.encode(input));
  const ctkPages = decoder.decode(encoder.encode(input));
  assert.equal(ctkPages.length, fumenPages.length);
  for (let index = 0; index < fumenPages.length; index += 1) {
    assert.equal(
      fieldSnapshot(ctkPages[index].field),
      fieldSnapshot(fumenPages[index].field),
    );
    assert.deepEqual(ctkPages[index].refs, fumenPages[index].refs);
    assert.deepEqual(ctkPages[index].flags, fumenPages[index].flags);
  }
});

test("native document API preserves CTK3-only row 23", () => {
  const cells = Array(24 * 10).fill(null);
  cells[23 * 10 + 4] = "T";
  const value = documentEncoder.encode({
    width: 10,
    pages: [{ height: 24, cells }],
  });
  const document = documentDecoder.decode(value);
  assert.equal(document.pages[0].cells[23 * 10 + 4], "T");
  assert.throws(() => decoder.decode(value), Ctk3FumenCompatibilityError);
});

test("Fumen compatibility rejects lossy comments before third-party encoding", () => {
  const unicode = "주석 100% 😀";
  const encoded = encoder.encode([
    { field: Field.create(), comment: unicode },
  ]);
  assert.equal(decodeCtk3(encoded).pages[0].comment, unicode);

  for (const comment of ["A".repeat(4095), `${"A".repeat(4092)}%`]) {
    const boundary = encoder.encode([{ field: Field.create(), comment }]);
    assert.equal(decodeCtk3(boundary).pages[0].comment, comment);
  }

  assert.throws(
    () =>
      encoder.encode([
        { field: Field.create(), comment: "A".repeat(4096) },
      ]),
    (error) =>
      error instanceof FumenCommentCodecError &&
      error.code === "fumen-comment-too-long",
  );
});

test("Fumen compatibility rejects oversized page sets before normalization", () => {
  assert.throws(
    () =>
      encoder.encode(
        Array.from({ length: 4097 }, () => ({ field: Field.create() })),
      ),
    (error) =>
      error instanceof Ctk3FumenCompatibilityError &&
      error.code === "fumen-page-limit" &&
      /4096-page limit/.test(error.message),
  );

  const emptyPage = () => ({
    height: 1,
    cells: Array(10).fill(null),
  });
  const onePageSegment = documentEncoder.encode({
    width: 10,
    pages: [emptyPage()],
  });
  const oversizedCtk3 = encodeCtk3Bundle(
    Array(4097).fill(onePageSegment),
  );
  assert.throws(
    () => inspectCtk3WithinPageLimit(oversizedCtk3, 4096),
    (error) =>
      error instanceof Ctk3PageLimitError && error.maximumPages === 4096,
  );
  assert.throws(
    () => decoder.decode(oversizedCtk3),
    (error) =>
      error instanceof Ctk3FumenCompatibilityError &&
      error.code === "fumen-page-limit",
  );
});

test(".ctk3 files use the exact UTF-8 document contract", () => {
  const document = {
    width: 10,
    pages: [
      {
        height: 2,
        cells: ["T", "T", "T", ...Array(7).fill(null), null, "T", ...Array(8).fill(null)],
        comment: "file roundtrip",
      },
    ],
  };
  const bytes = encodeCtk3File(document);
  const source = new TextDecoder().decode(bytes);
  const canonical = decodeCtk3(source);
  assert.match(source, /^ctk3_/);
  assert.deepEqual(decodeCtk3File(bytes), canonical);
  assert.deepEqual(parseCtk3File(bytes), { source, document: canonical });
  assert.deepEqual(decodeCtk3File(`\ufeff${source}\n`), canonical);
  assert.equal(CTK3_FILE_EXTENSION, ".ctk3");
  assert.equal(CTK3_FILE_MIME_TYPE, "application/vnd.clearra.ctk3");
  assert.equal(isCtk3File("opening.CTK3"), true);
  assert.equal(isCtk3File({ name: "opening.txt", type: CTK3_FILE_MIME_TYPE }), true);
  assert.equal(isCtk3File("opening.txt"), false);
});

test("legacy CTK85 remains readable and CommonJS exposes the same API", () => {
  const legacy = "ctk3@.:)aB*t&hPEXlYu:YoUl/4cH8Ga[PB0z";
  const document = decodeCtk3(legacy);
  assert.equal(document.width, 10);
  assert.equal(commonJs.documentDecoder.decode(legacy).width, 10);
  assert.equal(typeof commonJs.decoder.decode, "function");
});

test("exact decoding rejects envelopes while compatibility decoding still extracts them", () => {
  const source = encodeCtk3Compact({
    width: 10,
    pages: [{ height: 0, cells: [] }],
  });
  const envelope = `https://example.invalid/view?document=${source}&theme=dark`;

  assert.deepEqual(decodeCtk3(envelope), decodeCtk3Exact(source));
  assert.throws(
    () => decodeCtk3Exact(envelope),
    /No CTK3 header was found/,
  );
  assert.throws(
    () => decodeCtk3File(`${source} trailing-text`),
    /trailing data/,
  );
});

test("every CTK3 revision enforces the shared operation-coordinate boundary", () => {
  const maximum = 0x3fffffff;
  for (const revision of [0, 1, 2, 3]) {
    for (const axis of ["x", "y"]) {
      for (const coordinate of [maximum, -maximum]) {
        const document = decodeCtk3Exact(
          operationFixture({ revision, [axis]: coordinate }),
        );
        assert.equal(
          document.pages[0].operation?.[axis],
          coordinate,
          `revision ${revision}, ${axis}`,
        );
      }
      for (const coordinate of [maximum + 1, -maximum - 1]) {
        assert.throws(
          () =>
            decodeCtk3Exact(
              operationFixture({ revision, [axis]: coordinate }),
            ),
          /CTK3 operation is invalid/,
          `revision ${revision}, ${axis} coordinate ${coordinate}`,
        );
      }
    }
  }
});

test("temporal operation deltas cannot cross the operation-coordinate boundary", () => {
  const maximum = 0x3fffffff;
  for (const revision of [2, 3]) {
    for (const axis of ["X", "Y"]) {
      for (const sign of [1, -1]) {
        const coordinate = sign * maximum;
        assert.throws(
          () =>
            decodeCtk3Exact(
              operationFixture({
                revision,
                [axis.toLowerCase()]: coordinate,
                [`delta${axis}`]: sign,
              }),
            ),
          /CTK3 operation is invalid/,
          `${axis} cumulative overflow in revision ${revision}`,
        );
      }
    }
  }
});

test("compact segments combine into one exact large-document bundle", () => {
  const first = encodeCtk3Compact({
    width: 10,
    pages: [
      { height: 1, cells: ["I", "I", "I", "I", ...Array(6).fill(null)] },
      { height: 1, cells: ["O", "O", null, null, ...Array(6).fill(null)] },
    ],
  });
  const second = encodeCtk3Compact({
    width: 10,
    pages: [
      { height: 1, cells: [null, null, "T", "T", "T", ...Array(5).fill(null)] },
    ],
  });
  const bundled = encodeCtk3Bundle([first, second]);
  const decoded = decodeCtk3(bundled);

  assert.match(bundled, /^ctk3b_/);
  assert.ok(isCtk3(bundled));
  assert.equal(decoded.width, 10);
  assert.equal(decoded.pages.length, 3);
  assert.deepEqual(decoded.pages[0].cells.slice(0, 4), ["I", "I", "I", "I"]);
  assert.deepEqual(decoded.pages[2].cells.slice(2, 5), ["T", "T", "T"]);
  assert.equal(encodeCtk3Bundle([first]), first);
});

test("large bundles are limited by pages rather than 4096-page segment count", async () => {
  const segment = encodeCtk3Compact({
    width: 10,
    pages: [{ height: 1, cells: ["L", "L", "L", "L", ...Array(6).fill(null)] }],
  });
  const bundled = encodeCtk3Bundle(Array(300).fill(segment));
  const info = inspectCtk3(bundled);
  assert.equal(info.pageCount, 300);
  assert.equal(info.segmentCount, 300);

  const reader = openCtk3Document(bundled, { cacheSegments: 1 });
  assert.deepEqual(reader.readPage(299).cells.slice(0, 4), ["L", "L", "L", "L"]);

  const asyncReader = openCtk3DocumentAsync(bundled, {
    workers: 1,
    cacheSegments: 1,
  });
  assert.deepEqual(
    (await asyncReader.readPage(150)).cells.slice(0, 4),
    ["L", "L", "L", "L"],
  );
  asyncReader.close();
  assert.equal((await decodeCtk3Async(bundled, { workers: 1 })).pages.length, 300);
});

test("async encoder segments documents beyond the single-payload limit", async () => {
  const pages = Array.from({ length: 5000 }, (_, index) => ({
    height: 1,
    cells: [
      index % 2 ? "S" : "Z",
      index % 2 ? "S" : "Z",
      null,
      null,
      ...Array(6).fill(null),
    ],
  }));
  const encoded = await encodeCtk3Async(
    { width: 10, pages },
    { workers: 1, segmentPages: 1024 },
  );
  assert.match(encoded, /^ctk3b_/);
  assert.equal(decodeCtk3(encoded).pages.length, 5000);
});

test("page-source encoder reads bounded segments instead of materializing the document", async () => {
  const pageCount = 5000;
  const ranges = [];
  const encoded = await encodeCtk3PageSourceAsync(
    {
      width: 10,
      pageCount,
      readPages(start, count) {
        ranges.push([start, count]);
        return Array.from({ length: count }, (_, offset) => {
          const color = (start + offset) % 2 ? "J" : "L";
          return {
            height: 1,
            cells: [color, color, color, color, ...Array(6).fill(null)],
          };
        });
      },
    },
    { workers: 1, segmentPages: 512 },
  );

  assert.equal(decodeCtk3(encoded).pages.length, pageCount);
  assert.equal(ranges.length, Math.ceil(pageCount / 512));
  assert.ok(ranges.every(([, count]) => count <= 512));
});

test("async encoder terminates every worker when aborted", async () => {
  const workers = [];
  const workerFactory = () => {
    const worker = hangingWorker();
    workers.push(worker);
    return worker;
  };
  const controller = new AbortController();
  const pending = encodeCtk3Async(
    {
      width: 10,
      pages: [
        { height: 1, cells: ["I", "I", "I", "I", ...Array(6).fill(null)] },
        { height: 1, cells: ["O", "O", null, null, ...Array(6).fill(null)] },
      ],
    },
    {
      workers: 2,
      segmentPages: 1,
      workerFactory,
      signal: controller.signal,
    },
  );
  controller.abort();

  await assert.rejects(pending, (error) => error?.name === "AbortError");
  assert.equal(workers.length, 2);
  assert.equal(workers.filter((worker) => worker.terminated).length, 2);
});

test("async reader rejects pending work and terminates workers when aborted", async () => {
  const first = encodeCtk3Compact({
    width: 10,
    pages: [{ height: 1, cells: ["S", "S", null, null, ...Array(6).fill(null)] }],
  });
  const second = encodeCtk3Compact({
    width: 10,
    pages: [{ height: 1, cells: ["Z", "Z", null, null, ...Array(6).fill(null)] }],
  });
  const workers = [];
  const controller = new AbortController();
  const reader = openCtk3DocumentAsync(encodeCtk3Bundle([first, second]), {
    workers: 2,
    workerFactory: () => {
      const worker = hangingWorker();
      workers.push(worker);
      return worker;
    },
    signal: controller.signal,
  });
  const pending = reader.readPage(1);
  controller.abort();

  await assert.rejects(pending, (error) => error?.name === "AbortError");
  assert.equal(workers.length, 2);
  assert.equal(workers.filter((worker) => worker.terminated).length, 2);
});

function operationSnapshot(operation) {
  return operation
    ? {
        type: operation.type,
        rotation: operation.rotation,
        x: operation.x,
        y: operation.y,
      }
    : null;
}

function fieldSnapshot(field) {
  return field.str({ reduced: false, separator: "", garbage: true });
}

function hangingWorker() {
  return {
    onmessage: null,
    onerror: null,
    terminated: false,
    postMessage() {},
    terminate() {
      this.terminated = true;
    },
  };
}

function operationFixture({ revision, x = 0, y = 0, deltaX, deltaY }) {
  const writer = new FixtureBitWriter();
  writer.writeBits(0xc3, 8);
  writer.writeBits(revision, 3);
  writer.writeBits(9, 5);
  writer.writeVarUint(deltaX === undefined && deltaY === undefined ? 1 : 2);
  writer.writeBit(0);

  if (revision === 3) {
    writer.writeVarUint(1);
    writer.writeBits(7, 4);
  }

  if (revision === 0) {
    writer.writeBits(1 << 6, 8);
    writer.writeVarUint(0);
    writer.writeBits(0, 2);
    writer.writeBits(1, 9);
    writeFixtureOperationBody(writer, x, y);
  } else if (revision === 1) {
    writer.writeBits(0, 2);
    writer.writeBit(0);
    writer.writeBit(1);
    writer.writeBit(0);
    writer.writeVarUint(0);
    writer.writeBits(7, 3);
    writeFixtureOperationBody(writer, x, y);
  } else {
    writeFixtureTemporalPage(writer, x, y);
    if (deltaX !== undefined || deltaY !== undefined) {
      writer.writeBits(0, 2);
      writer.writeBit(0);
      writer.writeBit(0);
      writer.writeBits(0, 4);
      writer.writeBits(7, 4);
      writer.writeBit(0);
      writer.writeBit(0);
      writer.writeBits(3, 3);
      writer.writeSignedVarInt(deltaX ?? 0);
      writer.writeSignedVarInt(deltaY ?? 0);
    }
  }

  const body = writer.toBytes();
  const checksum = fixtureCrc16(body);
  const payload = Buffer.concat([
    Buffer.from(body),
    Buffer.from([checksum >>> 8, checksum & 0xff]),
  ]);
  return `ctk3_${payload.toString("base64url")}`;
}

function writeFixtureTemporalPage(writer, x, y) {
  writer.writeBits(0, 2);
  writer.writeBit(0);
  writer.writeVarUint(0);
  writer.writeBits(0, 4);
  writer.writeBits(7, 4);
  writer.writeBit(0);
  writer.writeBit(0);
  writer.writeBits(7, 3);
  writeFixtureOperationBody(writer, x, y);
}

function writeFixtureOperationBody(writer, x, y) {
  writer.writeBits(2, 3);
  writer.writeBits(0, 2);
  writer.writeSignedVarInt(x);
  writer.writeSignedVarInt(y);
}

function fixtureCrc16(bytes) {
  let crc = 0xffff;
  for (const byte of bytes) {
    crc ^= byte << 8;
    for (let bit = 0; bit < 8; bit += 1) {
      crc =
        crc & 0x8000
          ? ((crc << 1) ^ 0x1021) & 0xffff
          : (crc << 1) & 0xffff;
    }
  }
  return crc;
}

class FixtureBitWriter {
  bytes = [];
  bitLength = 0;

  writeBit(value) {
    const byteIndex = Math.floor(this.bitLength / 8);
    const bitIndex = this.bitLength % 8;
    if (byteIndex === this.bytes.length) this.bytes.push(0);
    if (value & 1) this.bytes[byteIndex] |= 1 << bitIndex;
    this.bitLength += 1;
  }

  writeBits(value, width) {
    for (let bit = 0; bit < width; bit += 1) {
      this.writeBit(Math.floor(value / 2 ** bit) & 1);
    }
  }

  writeVarUint(value) {
    if (value < 16) {
      this.writeBit(0);
      this.writeBits(value, 4);
    } else if (value < 256) {
      this.writeBits(1, 2);
      this.writeBits(value, 8);
    } else if (value < 65536) {
      this.writeBits(3, 3);
      this.writeBits(value, 16);
    } else {
      this.writeBits(7, 3);
      this.writeBits(value, 32);
    }
  }

  writeSignedVarInt(value) {
    this.writeVarUint(value >= 0 ? value * 2 : -value * 2 - 1);
  }

  toBytes() {
    return Uint8Array.from(this.bytes);
  }
}
