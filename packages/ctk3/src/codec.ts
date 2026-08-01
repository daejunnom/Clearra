// SRP rationale: this module has one change reason: the versioned CTK3 wire-format encode and decode contract.
import {
  canonicalizeCtkOperation,
  ctkOperationRotations,
  operationCells,
} from "./operationGeometry.js";

export const CTK3_PREFIX = "ctk3@";
export const CTK3_BUNDLE_PREFIX = "ctk3b_";
const CTK64_PREFIX = "ctk3_";
export const CTK3_MAX_SEGMENT_PAGES = 4096;
export const CTK3_MAX_BUNDLE_PAGES = 1_048_576;

export type Ctk3Piece = "I" | "O" | "T" | "S" | "Z" | "J" | "L";
export type Ctk3Color = Ctk3Piece | "G" | null;
export type Ctk3Rotation = "spawn" | "right" | "reverse" | "left";

export type Ctk3Operation = {
  piece: Ctk3Piece;
  rotation: Ctk3Rotation;
  x: number;
  y: number;
};

export type Ctk3PageFlags = {
  lock: boolean;
  mirror: boolean;
  colorize: boolean;
  rise: boolean;
  quiz: boolean;
};

export type Ctk3Page = {
  height: number;
  cells: Ctk3Color[];
  comment?: string;
  operation?: Ctk3Operation;
  flags?: Partial<Ctk3PageFlags>;
  garbage?: Ctk3Color[];
};

export type Ctk3Document = {
  width: number;
  pages: Ctk3Page[];
};

export type Ctk3DocumentInfo = {
  width: number;
  pageCount: number;
  segmentCount: number;
  segmentPageCounts: number[];
  bundled: boolean;
};

export type Ctk3SegmentIndex = {
  info: Ctk3DocumentInfo;
  segments: string[];
};

const MAGIC = 0xc3;
const LEGACY_SCHEMA_REVISION = 0;
const COMPACT_SCHEMA_REVISION = 1;
const TEMPORAL_SCHEMA_REVISION = 2;
const SHARED_FIELD_SCHEMA_REVISION = 3;
const MAX_WIDTH = 31;
const MAX_HEIGHT = 31;
const MAX_PAGES = CTK3_MAX_SEGMENT_PAGES;
const MAX_BUNDLE_PAGES = CTK3_MAX_BUNDLE_PAGES;
const MAX_BUNDLE_SEGMENTS = MAX_BUNDLE_PAGES;
const MAX_COMMENT_BYTES = 1 << 20;
const MAX_PAYLOAD_BYTES = 16 << 20;
const TEMPORAL_REFERENCE_WINDOW = 16;
const MAX_CTK64_CHARACTERS = Math.ceil((MAX_PAYLOAD_BYTES * 8) / 6);
const MAX_CTK85_CHARACTERS = Math.ceil(MAX_PAYLOAD_BYTES / 4) * 5;

// CTK64 keeps the compact CTK bitstream and framing while using only URL-safe
// transport characters. CTK85 remains decode-only for existing documents.
const CTK64_ALPHABET =
  "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const CTK64_INDEX = new Map(
  Array.from(CTK64_ALPHABET, (character, index) => [character, index]),
);
const CTK85_ALPHABET =
  "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-:+=^!/*?&<>()[]{}@%$#";
const CTK85_INDEX = new Map(
  Array.from(CTK85_ALPHABET, (character, index) => [character, index]),
);

const COLORS: Ctk3Color[] = [null, "G", "I", "O", "T", "S", "Z", "J", "L"];
const PIECES: Ctk3Piece[] = ["I", "O", "T", "S", "Z", "J", "L"];
const ROTATIONS: Ctk3Rotation[] = ["spawn", "right", "reverse", "left"];
const DEFAULT_FLAGS: Ctk3PageFlags = {
  lock: true,
  mirror: false,
  colorize: true,
  rise: false,
  quiz: false,
};

type NormalizedPage = {
  height: number;
  codes: number[];
  comment: string;
  operation?: Ctk3Operation;
  flags: Ctk3PageFlags;
  garbageCodes: number[] | null;
};

type PreviousPageState = {
  height: number;
  codes: number[];
  flags: Ctk3PageFlags;
};

type SharedFieldPredictor = {
  height: number;
  codes: number[];
};

type TemporalPageContext = {
  pages: NormalizedPage[];
  sharedField: SharedFieldPredictor | null;
  latestFieldIndex: Map<string, number>;
  latestPageIndex: Map<string, number>;
  latestCommentIndex: Map<string, number>;
};

export class Ctk3CodecError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "Ctk3CodecError";
  }
}

export function encodeCtk3(document: Ctk3Document): string {
  const normalized = normalizeDocument(document);
  const candidates = [
    encodeNormalizedDocument(normalized, COMPACT_SCHEMA_REVISION),
    encodeNormalizedDocument(normalized, TEMPORAL_SCHEMA_REVISION),
  ];
  const sharedField = buildSharedFieldPredictor(
    normalized.pages,
    normalized.width,
  );
  if (sharedField) {
    candidates.push(
      encodeNormalizedDocument(
        normalized,
        SHARED_FIELD_SCHEMA_REVISION,
        sharedField,
      ),
    );
  }

  candidates.sort(
    (left, right) =>
      left.payload.length - right.payload.length ||
      left.revision - right.revision,
  );
  return `${CTK64_PREFIX}${encodeCtk64(candidates[0].payload)}`;
}

export function encodeCtk3Compact(document: Ctk3Document): string {
  const normalized = normalizeDocument(document);
  const encoded = encodeNormalizedDocument(
    normalized,
    COMPACT_SCHEMA_REVISION,
  );
  return `${CTK64_PREFIX}${encodeCtk64(encoded.payload)}`;
}

export function encodeCtk3Bundle(segments: readonly string[]): string {
  if (segments.length < 1 || segments.length > MAX_BUNDLE_SEGMENTS) {
    throw new Ctk3CodecError("CTK3 bundle segment count is invalid.");
  }
  let width: number | null = null;
  let pageCount = 0;
  const payloads = new Array<string>(segments.length);
  for (let index = 0; index < segments.length; index += 1) {
    const payload = exactCtk64Payload(segments[index]);
    const header = inspectCtk64PayloadHeader(payload);
    if (width === null) {
      width = header.width;
    } else if (header.width !== width) {
      throw new Ctk3CodecError("CTK3 bundle widths do not match.");
    }
    pageCount += header.pageCount;
    if (pageCount > MAX_BUNDLE_PAGES) {
      throw new Ctk3CodecError("CTK3 bundle page count is invalid.");
    }
    payloads[index] = payload;
  }
  if (segments.length === 1) {
    return `${CTK64_PREFIX}${payloads[0]}`;
  }
  return `${CTK3_BUNDLE_PREFIX}${payloads.join(".")}`;
}

function encodeNormalizedDocument(
  normalized: { width: number; pages: NormalizedPage[] },
  revision: number,
  sharedField: SharedFieldPredictor | null = null,
): { revision: number; payload: Uint8Array } {
  const writer = new BitWriter();
  writer.writeBits(MAGIC, 8);
  writer.writeBits(revision, 3);
  writer.writeBits(normalized.width - 1, 5);
  writer.writeVarUint(normalized.pages.length);
  writer.writeBit(0); // Reserved extension block.

  if (revision === COMPACT_SCHEMA_REVISION) {
    let previous: NormalizedPage | null = null;
    for (const page of normalized.pages) {
      writePage(writer, page, previous);
      previous = page;
    }
  } else if (revision === TEMPORAL_SCHEMA_REVISION) {
    writeTemporalPages(writer, normalized.pages, normalized.width);
  } else if (revision === SHARED_FIELD_SCHEMA_REVISION && sharedField) {
    writer.writeVarUint(sharedField.height);
    writeBestCellEncoding(writer, sharedField.codes, null, 4, true);
    writeTemporalPages(writer, normalized.pages, normalized.width, sharedField);
  } else {
    throw new Ctk3CodecError("CTK3 schema revision is unsupported.");
  }

  const body = writer.toBytes();
  const payload = new Uint8Array(body.length + 2);
  payload.set(body);
  const checksum = crc16(body);
  payload[body.length] = checksum >>> 8;
  payload[body.length + 1] = checksum & 0xff;
  return { revision, payload };
}

export function decodeCtk3(input: string): Ctk3Document {
  const bundle = extractCtk3BundlePayloads(input);
  if (bundle) {
    const pages: Ctk3Page[] = [];
    let width: number | null = null;
    for (const payload of bundle) {
      const document = decodeSingleCtk3(`${CTK64_PREFIX}${payload}`);
      if (width === null) {
        width = document.width;
      } else if (document.width !== width) {
        throw new Ctk3CodecError("CTK3 bundle widths do not match.");
      }
      if (pages.length + document.pages.length > MAX_BUNDLE_PAGES) {
        throw new Ctk3CodecError("CTK3 bundle page count is invalid.");
      }
      pages.push(...document.pages);
    }
    return { width: width!, pages };
  }
  return decodeSingleCtk3(input);
}

export function decodeCtk3Segment(input: string): Ctk3Document {
  if (extractCtk3BundlePayloads(input)) {
    throw new Ctk3CodecError("A CTK3 segment cannot contain a bundle.");
  }
  return decodeSingleCtk3(input);
}

export function splitCtk3Segments(input: string): string[] {
  return indexCtk3Segments(input).segments;
}

export function inspectCtk3(input: string): Ctk3DocumentInfo {
  return indexCtk3Segments(input).info;
}

export function indexCtk3Segments(input: string): Ctk3SegmentIndex {
  const bundle = extractCtk3BundlePayloads(input);
  const segments = bundle
    ? bundle.map((payload) => `${CTK64_PREFIX}${payload}`)
    : [input.trim()];
  const segmentPageCounts = new Array<number>(segments.length);
  let width: number | null = null;
  let pageCount = 0;
  for (let index = 0; index < segments.length; index += 1) {
    const header = inspectSingleCtk3(segments[index]);
    if (width === null) {
      width = header.width;
    } else if (header.width !== width) {
      throw new Ctk3CodecError("CTK3 bundle widths do not match.");
    }
    pageCount += header.pageCount;
    if (pageCount > MAX_BUNDLE_PAGES) {
      throw new Ctk3CodecError("CTK3 bundle page count is invalid.");
    }
    segmentPageCounts[index] = header.pageCount;
  }
  return {
    segments,
    info: {
      width: width!,
      pageCount,
      segmentCount: segments.length,
      segmentPageCounts,
      bundled: segments.length > 1,
    },
  };
}

function decodeSingleCtk3(input: string): Ctk3Document {
  const encoded = extractCtk3Payload(input);
  const maximumLength =
    encoded.transport === "ctk64" ? MAX_CTK64_CHARACTERS : MAX_CTK85_CHARACTERS;
  if (encoded.payload.length > maximumLength) {
    throw new Ctk3CodecError("CTK3 payload length is invalid.");
  }
  const payload =
    encoded.transport === "ctk64"
      ? decodeCtk64(encoded.payload)
      : decodeCtk85(encoded.payload);
  if (payload.length < 4 || payload.length > MAX_PAYLOAD_BYTES) {
    throw new Ctk3CodecError("CTK3 payload length is invalid.");
  }

  const body = payload.subarray(0, payload.length - 2);
  const expectedChecksum =
    (payload[payload.length - 2] << 8) | payload[payload.length - 1];
  if (crc16(body) !== expectedChecksum) {
    throw new Ctk3CodecError("CTK3 checksum does not match.");
  }

  const reader = new BitReader(body);
  if (reader.readBits(8) !== MAGIC) {
    throw new Ctk3CodecError("CTK3 payload header is invalid.");
  }
  const schemaRevision = reader.readBits(3);
  if (
    schemaRevision !== LEGACY_SCHEMA_REVISION &&
    schemaRevision !== COMPACT_SCHEMA_REVISION &&
    schemaRevision !== TEMPORAL_SCHEMA_REVISION &&
    schemaRevision !== SHARED_FIELD_SCHEMA_REVISION
  ) {
    throw new Ctk3CodecError("CTK3 schema revision is unsupported.");
  }
  const width = reader.readBits(5) + 1;
  if (width < 1 || width > MAX_WIDTH) {
    throw new Ctk3CodecError("CTK3 board width is invalid.");
  }
  const pageCount = reader.readVarUint();
  if (pageCount < 1 || pageCount > MAX_PAGES) {
    throw new Ctk3CodecError("CTK3 page count is invalid.");
  }
  if (reader.readBit() !== 0) {
    throw new Ctk3CodecError("CTK3 extension block is unsupported.");
  }

  if (
    schemaRevision === TEMPORAL_SCHEMA_REVISION ||
    schemaRevision === SHARED_FIELD_SCHEMA_REVISION
  ) {
    let sharedField: SharedFieldPredictor | null = null;
    if (schemaRevision === SHARED_FIELD_SCHEMA_REVISION) {
      const height = reader.readVarUint();
      if (height < 1 || height > MAX_HEIGHT) {
        throw new Ctk3CodecError("CTK3 shared field height is invalid.");
      }
      sharedField = {
        height,
        codes: readCellEncoding(reader, width * height, null, 4),
      };
    }
    const pages = readTemporalPages(reader, width, pageCount, sharedField);
    reader.assertZeroPadding();
    return { width, pages };
  }

  const pages: Ctk3Page[] = [];
  let previous: {
    height: number;
    codes: number[];
    flags: Ctk3PageFlags;
  } | null = null;
  for (let index = 0; index < pageCount; index += 1) {
    const decoded: { page: Ctk3Page; codes: number[] } =
      schemaRevision === LEGACY_SCHEMA_REVISION
        ? readLegacyPage(reader, width, previous?.codes ?? null)
        : readPage(reader, width, previous);
    pages.push(decoded.page);
    previous = {
      height: decoded.page.height,
      codes: decoded.codes,
      flags: {
        ...DEFAULT_FLAGS,
        ...(decoded.page.flags ?? {}),
      },
    };
  }
  reader.assertZeroPadding();
  return { width, pages };
}

export function isCtk3(input: string): boolean {
  const source = input.trim().toLowerCase();
  return (
    source.includes(CTK3_PREFIX) ||
    source.includes(CTK64_PREFIX) ||
    source.includes(CTK3_BUNDLE_PREFIX)
  );
}

export function defaultCtk3Flags(): Ctk3PageFlags {
  return { ...DEFAULT_FLAGS };
}

function normalizeDocument(document: Ctk3Document): {
  width: number;
  pages: NormalizedPage[];
} {
  if (
    !Number.isInteger(document.width) ||
    document.width < 1 ||
    document.width > MAX_WIDTH
  ) {
    throw new Ctk3CodecError("CTK3 board width is invalid.");
  }
  if (
    !Array.isArray(document.pages) ||
    document.pages.length < 1 ||
    document.pages.length > MAX_PAGES
  ) {
    throw new Ctk3CodecError("CTK3 page count is invalid.");
  }
  return {
    width: document.width,
    pages: document.pages.map((page) => normalizePage(page, document.width)),
  };
}

function normalizePage(page: Ctk3Page, width: number): NormalizedPage {
  if (
    !Number.isInteger(page.height) ||
    page.height < 0 ||
    page.height > MAX_HEIGHT ||
    page.cells.length !== page.height * width
  ) {
    throw new Ctk3CodecError("CTK3 page field is invalid.");
  }

  const sourceCodes = page.cells.map(colorCode);
  let height = page.height;
  while (
    height > 0 &&
    sourceCodes
      .slice((height - 1) * width, height * width)
      .every((code) => code === 0)
  ) {
    height -= 1;
  }
  const codes = sourceCodes.slice(0, height * width);
  const comment = page.comment ?? "";
  const commentBytes = new TextEncoder().encode(comment);
  if (commentBytes.length > MAX_COMMENT_BYTES) {
    throw new Ctk3CodecError("CTK3 page comment is too long.");
  }
  const operation = page.operation
    ? normalizeOperation(page.operation)
    : undefined;
  const flags: Ctk3PageFlags = {
    ...DEFAULT_FLAGS,
    ...(page.flags ?? {}),
    quiz: page.flags?.quiz ?? comment.startsWith("#Q="),
  };
  const garbageCodes = page.garbage
    ? normalizeGarbage(page.garbage, width)
    : null;
  return { height, codes, comment, operation, flags, garbageCodes };
}

function normalizeOperation(operation: Ctk3Operation): Ctk3Operation {
  if (
    !PIECES.includes(operation.piece) ||
    !ROTATIONS.includes(operation.rotation) ||
    !Number.isSafeInteger(operation.x) ||
    !Number.isSafeInteger(operation.y) ||
    Math.abs(operation.x) > 0x3fffffff ||
    Math.abs(operation.y) > 0x3fffffff
  ) {
    throw new Ctk3CodecError("CTK3 operation is invalid.");
  }
  return canonicalizeCtkOperation(operation);
}

function normalizeGarbage(
  garbage: Ctk3Color[],
  width: number,
): number[] | null {
  if (garbage.length !== width) {
    throw new Ctk3CodecError("CTK3 garbage row width is invalid.");
  }
  const codes = garbage.map(colorCode);
  return codes.some((code) => code !== 0) ? codes : null;
}

function buildSharedFieldPredictor(
  pages: NormalizedPage[],
  width: number,
): SharedFieldPredictor | null {
  if (pages.length < 2 || pages[0].codes.length === 0) return null;
  const common = pages[0].codes.slice();
  for (let pageIndex = 1; pageIndex < pages.length; pageIndex += 1) {
    const codes = pages[pageIndex].codes;
    for (let index = 0; index < common.length; index += 1) {
      if (common[index] !== 0 && common[index] !== (codes[index] ?? 0)) {
        common[index] = 0;
      }
    }
  }

  let height = Math.ceil(common.length / width);
  while (
    height > 0 &&
    common
      .slice((height - 1) * width, height * width)
      .every((code) => code === 0)
  ) {
    height -= 1;
  }
  if (height === 0) return null;
  return {
    height,
    codes: common.slice(0, height * width),
  };
}

function writeTemporalPages(
  writer: BitWriter,
  pages: NormalizedPage[],
  width: number,
  sharedField: SharedFieldPredictor | null = null,
) {
  const context: TemporalPageContext = {
    pages,
    sharedField,
    latestFieldIndex: new Map(),
    latestPageIndex: new Map(),
    latestCommentIndex: new Map(),
  };
  let index = 0;
  while (index < pages.length) {
    const page = pages[index];
    const previous = index > 0 ? pages[index - 1] : null;
    if (previous && normalizedPagesEqual(page, previous)) {
      let repeatCount = 1;
      while (
        index + repeatCount < pages.length &&
        normalizedPagesEqual(pages[index + repeatCount], previous)
      ) {
        repeatCount += 1;
      }
      if (repeatCount >= 2) {
        const run = new BitWriter();
        run.writeBits(3, 2);
        run.writeVarUint(repeatCount - 2);
        if (run.bitLength < repeatCount * 2) {
          writer.append(run);
          for (let offset = 0; offset < repeatCount; offset += 1) {
            recordTemporalPage(context, index + offset);
          }
          index += repeatCount;
          continue;
        }
      }
    }

    const candidates: BitWriter[] = [];
    const normal = new BitWriter();
    normal.writeBits(0, 2);
    writeTemporalPage(normal, page, context, index, width);
    candidates.push(normal);

    if (previous && normalizedPagesEqual(page, previous)) {
      const copied = new BitWriter();
      copied.writeBits(1, 2);
      candidates.push(copied);
    }
    const priorIndex = context.latestPageIndex.get(normalizedPageKey(page));
    if (priorIndex !== undefined && priorIndex !== index - 1) {
      const referenced = new BitWriter();
      referenced.writeBits(2, 2);
      referenced.writeVarUint(index - priorIndex - 1);
      candidates.push(referenced);
    }

    candidates.sort((left, right) => left.bitLength - right.bitLength);
    writer.append(candidates[0]);
    recordTemporalPage(context, index);
    index += 1;
  }
}

function readTemporalPages(
  reader: BitReader,
  width: number,
  pageCount: number,
  sharedField: SharedFieldPredictor | null = null,
): Ctk3Page[] {
  const states: NormalizedPage[] = [];
  while (states.length < pageCount) {
    const mode = reader.readBits(2);
    if (mode === 0) {
      states.push(readTemporalPage(reader, width, states, sharedField));
      continue;
    }
    if (mode === 1) {
      const previous = states[states.length - 1];
      if (!previous) {
        throw new Ctk3CodecError("CTK3 repeated page has no reference.");
      }
      states.push(cloneNormalizedPage(previous));
      continue;
    }
    if (mode === 2) {
      const distance = reader.readVarUint() + 1;
      const reference = states[states.length - distance];
      if (!reference) {
        throw new Ctk3CodecError("CTK3 page reference is invalid.");
      }
      states.push(cloneNormalizedPage(reference));
      continue;
    }

    const previous = states[states.length - 1];
    const repeatCount = reader.readVarUint() + 2;
    if (!previous || repeatCount > pageCount - states.length) {
      throw new Ctk3CodecError("CTK3 repeated page run is invalid.");
    }
    for (let offset = 0; offset < repeatCount; offset += 1) {
      states.push(cloneNormalizedPage(previous));
    }
  }
  return states.map(normalizedPageToPage);
}

function writeTemporalPage(
  writer: BitWriter,
  page: NormalizedPage,
  context: TemporalPageContext,
  index: number,
  width: number,
) {
  const previous = index > 0 ? context.pages[index - 1] : null;
  writeTemporalFlags(writer, page.flags, previous?.flags ?? null);
  if (previous) {
    const sameHeight = page.height === previous.height;
    writer.writeBit(Number(!sameHeight));
    if (!sameHeight) writer.writeVarUint(page.height);
  } else {
    writer.writeVarUint(page.height);
  }
  writeTemporalField(writer, page, context, index, width);
  writeTemporalGarbage(
    writer,
    page.garbageCodes,
    previous?.garbageCodes ?? null,
  );
  writeTemporalComment(writer, page.comment, context, index);
  writeTemporalOperation(writer, page.operation, previous?.operation);
}

function readTemporalPage(
  reader: BitReader,
  width: number,
  history: NormalizedPage[],
  sharedField: SharedFieldPredictor | null,
): NormalizedPage {
  const previous = history[history.length - 1] ?? null;
  const flags = readTemporalFlags(reader, previous?.flags ?? null);
  const height = previous
    ? reader.readBit()
      ? reader.readVarUint()
      : previous.height
    : reader.readVarUint();
  if (height > MAX_HEIGHT) {
    throw new Ctk3CodecError("CTK3 page height is invalid.");
  }
  const codes = readTemporalField(reader, width, height, history, sharedField);
  const garbageCodes = readTemporalGarbage(
    reader,
    width,
    previous?.garbageCodes ?? null,
  );
  const comment = readTemporalComment(reader, history);
  const operation = readTemporalOperation(reader, previous?.operation);
  return { height, codes, comment, operation, flags, garbageCodes };
}

function writeTemporalFlags(
  writer: BitWriter,
  flags: Ctk3PageFlags,
  previous: Ctk3PageFlags | null,
) {
  if (flagsEqual(flags, DEFAULT_FLAGS)) {
    writer.writeBit(0);
  } else if (previous && flagsEqual(flags, previous)) {
    writer.writeBits(1, 2);
  } else {
    writer.writeBits(3, 2);
    writer.writeBits(flagBits(flags), 5);
  }
}

function readTemporalFlags(
  reader: BitReader,
  previous: Ctk3PageFlags | null,
): Ctk3PageFlags {
  if (reader.readBit() === 0) return { ...DEFAULT_FLAGS };
  if (reader.readBit() === 0) {
    if (!previous) {
      throw new Ctk3CodecError("CTK3 page flag reference is invalid.");
    }
    return { ...previous };
  }
  return flagsFromBits(reader.readBits(5));
}

function writeTemporalGarbage(
  writer: BitWriter,
  garbage: number[] | null,
  previous: number[] | null,
) {
  if (!garbage) {
    writer.writeBit(0);
    return;
  }
  if (previous && arraysEqual(garbage, previous)) {
    writer.writeBits(1, 2);
    return;
  }
  writer.writeBits(3, 2);
  writeBestCellEncoding(writer, garbage, previous, 4, true);
}

function readTemporalGarbage(
  reader: BitReader,
  width: number,
  previous: number[] | null,
): number[] | null {
  if (reader.readBit() === 0) return null;
  if (reader.readBit() === 0) {
    if (!previous) {
      throw new Ctk3CodecError("CTK3 garbage reference is invalid.");
    }
    return previous.slice();
  }
  return readCellEncoding(reader, width, previous, 4);
}

function writeTemporalComment(
  writer: BitWriter,
  comment: string,
  context: TemporalPageContext,
  index: number,
) {
  if (!comment) {
    writer.writeBit(0);
    return;
  }
  const candidates: BitWriter[] = [];
  const literal = new BitWriter();
  literal.writeBits(7, 3);
  const bytes = new TextEncoder().encode(comment);
  literal.writeVarUint(bytes.length);
  literal.writeBytes(bytes);
  candidates.push(literal);

  if (index > 0 && context.pages[index - 1].comment === comment) {
    const copied = new BitWriter();
    copied.writeBits(1, 2);
    candidates.push(copied);
  }
  const priorIndex = context.latestCommentIndex.get(comment);
  if (priorIndex !== undefined && priorIndex !== index - 1) {
    const referenced = new BitWriter();
    referenced.writeBits(3, 3);
    referenced.writeVarUint(index - priorIndex - 1);
    candidates.push(referenced);
  }
  candidates.sort((left, right) => left.bitLength - right.bitLength);
  writer.append(candidates[0]);
}

function readTemporalComment(
  reader: BitReader,
  history: NormalizedPage[],
): string {
  if (reader.readBit() === 0) return "";
  if (reader.readBit() === 0) {
    const previous = history[history.length - 1];
    if (!previous) {
      throw new Ctk3CodecError("CTK3 comment reference is invalid.");
    }
    return previous.comment;
  }
  if (reader.readBit() === 0) {
    const distance = reader.readVarUint() + 1;
    const reference = history[history.length - distance];
    if (!reference || !reference.comment) {
      throw new Ctk3CodecError("CTK3 comment reference is invalid.");
    }
    return reference.comment;
  }
  const byteLength = reader.readVarUint();
  if (byteLength > MAX_COMMENT_BYTES) {
    throw new Ctk3CodecError("CTK3 page comment is too long.");
  }
  return new TextDecoder("utf-8", { fatal: true }).decode(
    reader.readBytes(byteLength),
  );
}

function writeTemporalOperation(
  writer: BitWriter,
  operation: Ctk3Operation | undefined,
  previous: Ctk3Operation | undefined,
) {
  if (!operation) {
    writer.writeBit(0);
    return;
  }
  if (previous && operationsEqual(operation, previous)) {
    writer.writeBits(1, 2);
    return;
  }

  const candidates: BitWriter[] = [];
  const literal = new BitWriter();
  literal.writeBits(7, 3);
  writeOperationBody(literal, operation);
  candidates.push(literal);
  if (
    previous &&
    operation.piece === previous.piece &&
    operation.rotation === previous.rotation
  ) {
    const delta = new BitWriter();
    delta.writeBits(3, 3);
    delta.writeSignedVarInt(operation.x - previous.x);
    delta.writeSignedVarInt(operation.y - previous.y);
    candidates.push(delta);
  }
  candidates.sort((left, right) => left.bitLength - right.bitLength);
  writer.append(candidates[0]);
}

function readTemporalOperation(
  reader: BitReader,
  previous: Ctk3Operation | undefined,
): Ctk3Operation | undefined {
  if (reader.readBit() === 0) return undefined;
  if (reader.readBit() === 0) {
    if (!previous) {
      throw new Ctk3CodecError("CTK3 operation reference is invalid.");
    }
    return { ...previous };
  }
  if (reader.readBit() === 0) {
    if (!previous) {
      throw new Ctk3CodecError("CTK3 operation delta is invalid.");
    }
    return {
      ...previous,
      x: previous.x + reader.readSignedVarInt(),
      y: previous.y + reader.readSignedVarInt(),
    };
  }
  return readOperationBody(reader);
}

function writeOperationBody(writer: BitWriter, operation: Ctk3Operation) {
  writer.writeBits(PIECES.indexOf(operation.piece), 3);
  const rotations = ctkOperationRotations(operation.piece);
  writer.writeBits(
    rotations.indexOf(operation.rotation),
    bitsForChoices(rotations.length),
  );
  writer.writeSignedVarInt(operation.x);
  writer.writeSignedVarInt(operation.y);
}

function readOperationBody(reader: BitReader): Ctk3Operation {
  const piece = PIECES[reader.readBits(3)];
  if (!piece) {
    throw new Ctk3CodecError("CTK3 operation is invalid.");
  }
  const rotations = ctkOperationRotations(piece);
  const rotation = rotations[reader.readBits(bitsForChoices(rotations.length))];
  if (!rotation) {
    throw new Ctk3CodecError("CTK3 operation is invalid.");
  }
  return {
    piece,
    rotation,
    x: reader.readSignedVarInt(),
    y: reader.readSignedVarInt(),
  };
}

function writeTemporalField(
  writer: BitWriter,
  page: NormalizedPage,
  context: TemporalPageContext,
  index: number,
  width: number,
) {
  const candidates: BitWriter[] = [];
  const targetLength = page.height * width;
  const addCandidate = (
    mode: number,
    predictor: number[] | null,
    referenceDistance?: number,
  ) => {
    const candidate = new BitWriter();
    candidate.writeBits(mode, 4);
    if (referenceDistance !== undefined) {
      candidate.writeVarUint(referenceDistance - 1);
    }
    if (predictor) {
      writeBestPredictedCellEncoding(candidate, page.codes, predictor, 4);
    } else {
      writeBestCellEncoding(candidate, page.codes, null, 4, true);
    }
    candidates.push(candidate);
  };

  addCandidate(0, null);
  const previous = index > 0 ? context.pages[index - 1] : null;
  if (previous) {
    addCandidate(1, fitCodes(previous.codes, targetLength));
    addCandidate(2, grayscaleCodes(previous.codes, targetLength));
    addCandidate(
      3,
      mirrorCodes(previous.codes, previous.height, width, page.height),
    );
    addCandidate(
      4,
      predictLockedCodes(previous, width, page.height, false, false),
    );
    addCandidate(
      5,
      predictLockedCodes(previous, width, page.height, false, true),
    );
    addCandidate(
      6,
      predictLockedCodes(previous, width, page.height, true, false),
    );
    addCandidate(
      7,
      predictLockedCodes(previous, width, page.height, true, true),
    );
  }

  const referenced = new Set<number>();
  const exactFieldIndex = context.latestFieldIndex.get(
    normalizedFieldKey(page),
  );
  if (exactFieldIndex !== undefined && exactFieldIndex !== index - 1) {
    referenced.add(exactFieldIndex);
  }
  const start = Math.max(0, index - TEMPORAL_REFERENCE_WINDOW);
  for (
    let referenceIndex = start;
    referenceIndex < index - 1;
    referenceIndex += 1
  ) {
    referenced.add(referenceIndex);
  }
  for (const referenceIndex of referenced) {
    const reference = context.pages[referenceIndex];
    const distance = index - referenceIndex;
    addCandidate(8, fitCodes(reference.codes, targetLength), distance);
    addCandidate(
      9,
      mirrorCodes(reference.codes, reference.height, width, page.height),
      distance,
    );
  }
  if (context.sharedField) {
    addCandidate(10, fitCodes(context.sharedField.codes, targetLength));
  }

  candidates.sort((left, right) => left.bitLength - right.bitLength);
  writer.append(candidates[0]);
}

function readTemporalField(
  reader: BitReader,
  width: number,
  height: number,
  history: NormalizedPage[],
  sharedField: SharedFieldPredictor | null,
): number[] {
  const mode = reader.readBits(4);
  const targetLength = width * height;
  const previous = history[history.length - 1] ?? null;
  let predictor: number[] | null = null;
  if (mode === 0) {
    predictor = null;
  } else if (mode >= 1 && mode <= 7) {
    if (!previous) {
      throw new Ctk3CodecError("CTK3 temporal field has no previous page.");
    }
    if (mode === 1) predictor = fitCodes(previous.codes, targetLength);
    if (mode === 2) predictor = grayscaleCodes(previous.codes, targetLength);
    if (mode === 3) {
      predictor = mirrorCodes(previous.codes, previous.height, width, height);
    }
    if (mode === 4) {
      predictor = predictLockedCodes(previous, width, height, false, false);
    }
    if (mode === 5) {
      predictor = predictLockedCodes(previous, width, height, false, true);
    }
    if (mode === 6) {
      predictor = predictLockedCodes(previous, width, height, true, false);
    }
    if (mode === 7) {
      predictor = predictLockedCodes(previous, width, height, true, true);
    }
  } else if (mode === 8 || mode === 9) {
    const distance = reader.readVarUint() + 1;
    const reference = history[history.length - distance];
    if (!reference) {
      throw new Ctk3CodecError("CTK3 temporal field reference is invalid.");
    }
    predictor =
      mode === 8
        ? fitCodes(reference.codes, targetLength)
        : mirrorCodes(reference.codes, reference.height, width, height);
  } else if (mode === 10) {
    if (!sharedField) {
      throw new Ctk3CodecError("CTK3 shared field reference is invalid.");
    }
    predictor = fitCodes(sharedField.codes, targetLength);
  } else {
    throw new Ctk3CodecError("CTK3 temporal field mode is invalid.");
  }
  return readCellEncoding(reader, targetLength, predictor, 4);
}

function predictLockedCodes(
  previous: NormalizedPage,
  width: number,
  targetHeight: number,
  clearRows: boolean,
  grayscale: boolean,
): number[] {
  const operation = previous.operation;
  const occupied = operation ? operationCells(operation) : [];
  const operationHeight = occupied.reduce(
    (height, cell) => Math.max(height, cell.y + 1),
    0,
  );
  const sourceHeight =
    operationHeight <= MAX_HEIGHT
      ? Math.max(previous.height, operationHeight)
      : previous.height;
  const cells = fitCodes(previous.codes, sourceHeight * width);
  if (operation && previous.flags.lock) {
    if (
      occupied.every(
        ({ x, y }) =>
          x >= 0 &&
          x < width &&
          y >= 0 &&
          y < sourceHeight &&
          cells[y * width + x] === 0,
      )
    ) {
      const code = previous.flags.colorize
        ? PIECES.indexOf(operation.piece) + 2
        : 1;
      for (const { x, y } of occupied) cells[y * width + x] = code;
    }
  }

  const transformed: number[] = [];
  for (let y = 0; y < sourceHeight; y += 1) {
    const row = cells.slice(y * width, (y + 1) * width);
    if (clearRows && row.every((code) => code !== 0)) continue;
    transformed.push(
      ...row.map((code) => (grayscale && code !== 0 ? 1 : code)),
    );
  }
  return fitCodes(transformed, targetHeight * width);
}

function grayscaleCodes(codes: number[], targetLength: number): number[] {
  return Array.from({ length: targetLength }, (_, index) =>
    (codes[index] ?? 0) === 0 ? 0 : 1,
  );
}

function mirrorCodes(
  source: number[],
  sourceHeight: number,
  width: number,
  targetHeight: number,
): number[] {
  const mirrored = Array<number>(targetHeight * width).fill(0);
  const rowCount = Math.min(sourceHeight, targetHeight);
  for (let y = 0; y < rowCount; y += 1) {
    for (let x = 0; x < width; x += 1) {
      mirrored[y * width + (width - x - 1)] = mirrorColorCode(
        source[y * width + x] ?? 0,
      );
    }
  }
  return mirrored;
}

function mirrorColorCode(code: number): number {
  if (code === 5) return 6;
  if (code === 6) return 5;
  if (code === 7) return 8;
  if (code === 8) return 7;
  return code;
}

function fitCodes(codes: number[], targetLength: number): number[] {
  return Array.from({ length: targetLength }, (_, index) => codes[index] ?? 0);
}

function recordTemporalPage(context: TemporalPageContext, index: number) {
  const page = context.pages[index];
  context.latestFieldIndex.set(normalizedFieldKey(page), index);
  context.latestPageIndex.set(normalizedPageKey(page), index);
  if (page.comment) context.latestCommentIndex.set(page.comment, index);
}

function normalizedFieldKey(page: NormalizedPage): string {
  return `${page.height}:${String.fromCharCode(...page.codes)}`;
}

function normalizedPageKey(page: NormalizedPage): string {
  return JSON.stringify([
    page.height,
    page.codes,
    page.comment,
    page.operation
      ? [
          page.operation.piece,
          page.operation.rotation,
          page.operation.x,
          page.operation.y,
        ]
      : null,
    flagBits(page.flags),
    page.garbageCodes,
  ]);
}

function normalizedPagesEqual(
  left: NormalizedPage,
  right: NormalizedPage,
): boolean {
  return (
    left.height === right.height &&
    arraysEqual(left.codes, right.codes) &&
    left.comment === right.comment &&
    operationsEqual(left.operation, right.operation) &&
    flagsEqual(left.flags, right.flags) &&
    nullableArraysEqual(left.garbageCodes, right.garbageCodes)
  );
}

function operationsEqual(
  left: Ctk3Operation | undefined,
  right: Ctk3Operation | undefined,
): boolean {
  return (
    left === right ||
    (left !== undefined &&
      right !== undefined &&
      left.piece === right.piece &&
      left.rotation === right.rotation &&
      left.x === right.x &&
      left.y === right.y)
  );
}

function arraysEqual(left: number[], right: number[]): boolean {
  return (
    left.length === right.length &&
    left.every((value, index) => value === right[index])
  );
}

function nullableArraysEqual(
  left: number[] | null,
  right: number[] | null,
): boolean {
  return (
    left === right ||
    (left !== null && right !== null && arraysEqual(left, right))
  );
}

function cloneNormalizedPage(page: NormalizedPage): NormalizedPage {
  return {
    height: page.height,
    codes: page.codes.slice(),
    comment: page.comment,
    operation: page.operation ? { ...page.operation } : undefined,
    flags: { ...page.flags },
    garbageCodes: page.garbageCodes?.slice() ?? null,
  };
}

function normalizedPageToPage(page: NormalizedPage): Ctk3Page {
  return {
    height: page.height,
    cells: page.codes.map(codeColor),
    ...(page.comment ? { comment: page.comment } : {}),
    ...(page.operation ? { operation: { ...page.operation } } : {}),
    flags: { ...page.flags },
    ...(page.garbageCodes ? { garbage: page.garbageCodes.map(codeColor) } : {}),
  };
}

function writePage(
  writer: BitWriter,
  page: NormalizedPage,
  previous: NormalizedPage | null,
) {
  const hasComment = page.comment.length > 0;
  const hasOperation = page.operation !== undefined;
  const hasGarbage = page.garbageCodes !== null;
  const flagMode = flagsEqual(page.flags, DEFAULT_FLAGS)
    ? 0
    : previous && flagsEqual(page.flags, previous.flags)
      ? 1
      : 2;
  writer.writeBits(flagMode, 2);
  if (flagMode === 2) writer.writeBits(flagBits(page.flags), 5);
  writer.writeBit(Number(hasComment));
  writer.writeBit(Number(hasOperation));
  writer.writeBit(Number(hasGarbage));

  if (previous) {
    const sameHeight = page.height === previous.height;
    writer.writeBit(Number(sameHeight));
    if (!sameHeight) writer.writeVarUint(page.height);
  } else {
    writer.writeVarUint(page.height);
  }
  writeBestCellEncoding(writer, page.codes, previous?.codes ?? null, 3, true);

  if (hasGarbage) {
    writeBestCellEncoding(writer, page.garbageCodes!, null, 3, true);
  }
  if (hasComment) {
    const bytes = new TextEncoder().encode(page.comment);
    writer.writeVarUint(bytes.length);
    writer.writeBytes(bytes);
  }
  if (hasOperation) {
    const operation = page.operation!;
    writer.writeBits(PIECES.indexOf(operation.piece), 3);
    const rotations = ctkOperationRotations(operation.piece);
    writer.writeBits(
      rotations.indexOf(operation.rotation),
      bitsForChoices(rotations.length),
    );
    writer.writeSignedVarInt(operation.x);
    writer.writeSignedVarInt(operation.y);
  }
}

function readPage(
  reader: BitReader,
  width: number,
  previous: PreviousPageState | null,
): { page: Ctk3Page; codes: number[] } {
  const flagMode = reader.readBits(2);
  let flags: Ctk3PageFlags;
  if (flagMode === 0) {
    flags = { ...DEFAULT_FLAGS };
  } else if (flagMode === 1) {
    if (!previous) {
      throw new Ctk3CodecError("CTK3 page flag reference is invalid.");
    }
    flags = { ...previous.flags };
  } else if (flagMode === 2) {
    flags = flagsFromBits(reader.readBits(5));
  } else {
    throw new Ctk3CodecError("CTK3 page flag mode is invalid.");
  }
  const hasComment = Boolean(reader.readBit());
  const hasOperation = Boolean(reader.readBit());
  const hasGarbage = Boolean(reader.readBit());
  const height =
    previous && reader.readBit() ? previous.height : reader.readVarUint();
  if (height > MAX_HEIGHT) {
    throw new Ctk3CodecError("CTK3 page height is invalid.");
  }
  const cellCount = width * height;
  const codes = readCellEncoding(reader, cellCount, previous?.codes ?? null, 3);
  const garbage = hasGarbage
    ? readCellEncoding(reader, width, null, 3).map(codeColor)
    : undefined;
  let comment: string | undefined;
  if (hasComment) {
    const byteLength = reader.readVarUint();
    if (byteLength > MAX_COMMENT_BYTES) {
      throw new Ctk3CodecError("CTK3 page comment is too long.");
    }
    comment = new TextDecoder("utf-8", { fatal: true }).decode(
      reader.readBytes(byteLength),
    );
  }
  let operation: Ctk3Operation | undefined;
  if (hasOperation) {
    const piece = PIECES[reader.readBits(3)];
    if (!piece) {
      throw new Ctk3CodecError("CTK3 operation is invalid.");
    }
    const rotations = ctkOperationRotations(piece);
    const rotation =
      rotations[reader.readBits(bitsForChoices(rotations.length))];
    if (!rotation) {
      throw new Ctk3CodecError("CTK3 operation is invalid.");
    }
    operation = {
      piece,
      rotation,
      x: reader.readSignedVarInt(),
      y: reader.readSignedVarInt(),
    };
  }
  return {
    codes,
    page: {
      height,
      cells: codes.map(codeColor),
      ...(comment === undefined ? {} : { comment }),
      ...(operation === undefined ? {} : { operation }),
      flags,
      ...(garbage === undefined ? {} : { garbage }),
    },
  };
}

function readLegacyPage(
  reader: BitReader,
  width: number,
  previousCodes: number[] | null,
): { page: Ctk3Page; codes: number[] } {
  const metadata = reader.readBits(8);
  const flags: Ctk3PageFlags = {
    lock: Boolean(metadata & 1),
    mirror: Boolean(metadata & 2),
    colorize: Boolean(metadata & 4),
    rise: Boolean(metadata & 8),
    quiz: Boolean(metadata & 16),
  };
  const hasComment = Boolean(metadata & 32);
  const hasOperation = Boolean(metadata & 64);
  const hasGarbage = Boolean(metadata & 128);
  const height = reader.readVarUint();
  if (height > MAX_HEIGHT) {
    throw new Ctk3CodecError("CTK3 page height is invalid.");
  }
  const cellCount = width * height;
  const codes = readCellEncoding(reader, cellCount, previousCodes, 2);
  const garbage = hasGarbage
    ? readCellEncoding(reader, width, null, 2).map(codeColor)
    : undefined;
  let comment: string | undefined;
  if (hasComment) {
    const byteLength = reader.readVarUint();
    if (byteLength > MAX_COMMENT_BYTES) {
      throw new Ctk3CodecError("CTK3 page comment is too long.");
    }
    comment = new TextDecoder("utf-8", { fatal: true }).decode(
      reader.readBytes(byteLength),
    );
  }
  let operation: Ctk3Operation | undefined;
  if (hasOperation) {
    const piece = PIECES[reader.readBits(3)];
    const rotation = ROTATIONS[reader.readBits(2)];
    if (!piece || !rotation) {
      throw new Ctk3CodecError("CTK3 operation is invalid.");
    }
    operation = {
      piece,
      rotation,
      x: reader.readSignedVarInt(),
      y: reader.readSignedVarInt(),
    };
  }
  return {
    codes,
    page: {
      height,
      cells: codes.map(codeColor),
      ...(comment === undefined ? {} : { comment }),
      ...(operation === undefined ? {} : { operation }),
      flags,
      ...(garbage === undefined ? {} : { garbage }),
    },
  };
}

function writeBestCellEncoding(
  writer: BitWriter,
  codes: number[],
  previousCodes: number[] | null,
  modeWidth = 2,
  includeMultiset = false,
) {
  const candidates = [
    paletteEncoding(codes, modeWidth),
    runLengthEncoding(codes, modeWidth),
    occupancyEncoding(codes, modeWidth),
  ];
  if (previousCodes) {
    candidates.push(deltaEncoding(codes, previousCodes, modeWidth));
    if (modeWidth >= 4) {
      candidates.push(changeMaskEncoding(codes, previousCodes, modeWidth));
      candidates.push(deltaRunEncoding(codes, previousCodes, modeWidth));
      const singleColor = singleColorDeltaEncoding(
        codes,
        previousCodes,
        modeWidth,
      );
      if (singleColor) candidates.push(singleColor);
      candidates.push(
        combinatorialDeltaEncoding(codes, previousCodes, modeWidth),
      );
    }
  }
  if (includeMultiset) {
    if (codes.every((code) => code === 0)) {
      candidates.push(singleModeEncoding(7, modeWidth));
    }
    if (
      previousCodes &&
      codes.every((code, index) => code === (previousCodes[index] ?? 0))
    ) {
      candidates.push(singleModeEncoding(6, modeWidth));
    }
    candidates.push(multisetEncoding(codes, modeWidth));
    const tetrominoColors = tetrominoColorEncoding(codes, modeWidth);
    if (tetrominoColors) candidates.push(tetrominoColors);
  }
  candidates.sort((left, right) => left.bitLength - right.bitLength);
  writer.append(candidates[0]);
}

function writeBestPredictedCellEncoding(
  writer: BitWriter,
  codes: number[],
  predictor: number[],
  modeWidth: number,
) {
  const candidates = [
    deltaEncoding(codes, predictor, modeWidth),
    changeMaskEncoding(codes, predictor, modeWidth),
    deltaRunEncoding(codes, predictor, modeWidth),
    combinatorialDeltaEncoding(codes, predictor, modeWidth),
  ];
  if (arraysEqual(codes, predictor)) {
    candidates.push(singleModeEncoding(6, modeWidth));
  }
  const singleColor = singleColorDeltaEncoding(codes, predictor, modeWidth);
  if (singleColor) candidates.push(singleColor);
  candidates.sort((left, right) => left.bitLength - right.bitLength);
  writer.append(candidates[0]);
}

function singleModeEncoding(mode: number, modeWidth: number): BitWriter {
  const writer = new BitWriter();
  writer.writeBits(mode, modeWidth);
  return writer;
}

function paletteEncoding(codes: number[], modeWidth: number): BitWriter {
  const writer = new BitWriter();
  writer.writeBits(0, modeWidth);
  const palette = uniqueCodes(codes);
  if (palette.length === 0) palette.push(0);
  writer.writeBits(colorMask(palette), 9);
  const width = bitsForChoices(palette.length);
  const indexes = new Map(palette.map((code, index) => [code, index]));
  for (const code of codes) writer.writeBits(indexes.get(code)!, width);
  return writer;
}

function runLengthEncoding(codes: number[], modeWidth: number): BitWriter {
  const writer = new BitWriter();
  writer.writeBits(1, modeWidth);
  const runs: Array<{ code: number; length: number }> = [];
  for (const code of codes) {
    const last = runs[runs.length - 1];
    if (last?.code === code) {
      last.length += 1;
    } else {
      runs.push({ code, length: 1 });
    }
  }
  writer.writeVarUint(runs.length);
  for (const run of runs) {
    writer.writeBits(run.code, 4);
    writer.writeVarUint(run.length - 1);
  }
  return writer;
}

function deltaEncoding(
  codes: number[],
  previousCodes: number[],
  modeWidth: number,
): BitWriter {
  const writer = new BitWriter();
  writer.writeBits(2, modeWidth);
  const changes: Array<{ index: number; code: number }> = [];
  for (let index = 0; index < codes.length; index += 1) {
    if (codes[index] !== (previousCodes[index] ?? 0)) {
      changes.push({ index, code: codes[index] });
    }
  }
  writer.writeVarUint(changes.length);
  let previousIndex = -1;
  for (const change of changes) {
    writer.writeVarUint(change.index - previousIndex - 1);
    writer.writeBits(change.code, 4);
    previousIndex = change.index;
  }
  return writer;
}

function changeMaskEncoding(
  codes: number[],
  previousCodes: number[],
  modeWidth: number,
): BitWriter {
  const writer = new BitWriter();
  writer.writeBits(8, modeWidth);
  const changed = codes.map((code, index) =>
    Number(code !== (previousCodes[index] ?? 0)),
  );
  writeOccupancyMask(writer, changed);
  const palette = uniqueCodes(codes.filter((_, index) => changed[index] !== 0));
  writer.writeBits(colorMask(palette), 9);
  const width = bitsForChoices(palette.length);
  const indexes = new Map(palette.map((code, index) => [code, index]));
  for (let index = 0; index < codes.length; index += 1) {
    if (changed[index]) writer.writeBits(indexes.get(codes[index])!, width);
  }
  return writer;
}

function deltaRunEncoding(
  codes: number[],
  previousCodes: number[],
  modeWidth: number,
): BitWriter {
  const changes = codes
    .map((code, index) => ({ code, index }))
    .filter(({ code, index }) => code !== (previousCodes[index] ?? 0));
  const runs: Array<{ start: number; codes: number[] }> = [];
  for (const change of changes) {
    const last = runs[runs.length - 1];
    if (last && last.start + last.codes.length === change.index) {
      last.codes.push(change.code);
    } else {
      runs.push({ start: change.index, codes: [change.code] });
    }
  }

  const writer = new BitWriter();
  writer.writeBits(9, modeWidth);
  const palette = uniqueCodes(changes.map(({ code }) => code));
  writer.writeBits(colorMask(palette), 9);
  writer.writeVarUint(runs.length);
  let previousEnd = 0;
  for (const run of runs) {
    writer.writeVarUint(run.start - previousEnd);
    writer.writeVarUint(run.codes.length - 1);
    previousEnd = run.start + run.codes.length;
  }
  const width = bitsForChoices(palette.length);
  const indexes = new Map(palette.map((code, index) => [code, index]));
  for (const run of runs) {
    for (const code of run.codes) writer.writeBits(indexes.get(code)!, width);
  }
  return writer;
}

function singleColorDeltaEncoding(
  codes: number[],
  previousCodes: number[],
  modeWidth: number,
): BitWriter | null {
  const changed = codes.map((code, index) =>
    Number(code !== (previousCodes[index] ?? 0)),
  );
  const colors = uniqueCodes(codes.filter((_, index) => changed[index] !== 0));
  if (colors.length !== 1) return null;
  const writer = new BitWriter();
  writer.writeBits(10, modeWidth);
  writer.writeBits(colors[0], 4);
  writeOccupancyMask(writer, changed);
  return writer;
}

function combinatorialDeltaEncoding(
  codes: number[],
  previousCodes: number[],
  modeWidth: number,
): BitWriter {
  const positions = codes
    .map((code, index) => ({ code, index }))
    .filter(({ code, index }) => code !== (previousCodes[index] ?? 0))
    .map(({ index }) => index);
  const writer = new BitWriter();
  writer.writeBits(11, modeWidth);
  writer.writeVarUint(positions.length);
  writer.writeBigBits(
    combinationRank(positions),
    bitsForBigChoices(combinationCount(codes.length, positions.length)),
  );
  const palette = uniqueCodes(positions.map((index) => codes[index]));
  writer.writeBits(colorMask(palette), 9);
  const width = bitsForChoices(palette.length);
  const indexes = new Map(palette.map((code, index) => [code, index]));
  for (const position of positions) {
    writer.writeBits(indexes.get(codes[position])!, width);
  }
  return writer;
}

function occupancyEncoding(codes: number[], modeWidth: number): BitWriter {
  const writer = new BitWriter();
  writer.writeBits(3, modeWidth);
  const occupied = codes.map((code) => Number(code !== 0));
  writeOccupancyMask(writer, occupied);

  const palette = uniqueCodes(codes.filter((code) => code !== 0));
  writer.writeBits(colorMask(palette) >>> 1, 8);
  const width = bitsForChoices(palette.length);
  const indexes = new Map(palette.map((code, index) => [code, index]));
  for (const code of codes) {
    if (code !== 0) writer.writeBits(indexes.get(code)!, width);
  }
  return writer;
}

function writeOccupancyMask(writer: BitWriter, occupied: number[]) {
  const raw = new BitWriter();
  raw.writeBit(0);
  for (const value of occupied) raw.writeBit(value);
  const runs = new BitWriter();
  runs.writeBit(1);
  if (occupied.length) {
    runs.writeBit(occupied[0]);
    let runLength = 1;
    for (let index = 1; index <= occupied.length; index += 1) {
      if (index < occupied.length && occupied[index] === occupied[index - 1]) {
        runLength += 1;
      } else {
        runs.writeVarUint(runLength - 1);
        runLength = 1;
      }
    }
  }
  writer.append(raw.bitLength <= runs.bitLength ? raw : runs);
}

function multisetEncoding(codes: number[], modeWidth: number): BitWriter {
  const writer = new BitWriter();
  writer.writeBits(4, modeWidth);
  const palette = uniqueCodes(codes);
  if (palette.length === 0) palette.push(0);
  writer.writeBits(colorMask(palette), 9);

  let remaining = Array.from({ length: codes.length }, (_, index) => index);
  for (
    let paletteIndex = 0;
    paletteIndex < palette.length - 1;
    paletteIndex += 1
  ) {
    const code = palette[paletteIndex];
    const positions: number[] = [];
    for (let index = 0; index < remaining.length; index += 1) {
      if (codes[remaining[index]] === code) positions.push(index);
    }
    writer.writeVarUint(positions.length);
    const choiceCount = combinationCount(remaining.length, positions.length);
    writer.writeBigBits(
      combinationRank(positions),
      bitsForBigChoices(choiceCount),
    );
    const selected = new Set(positions);
    remaining = remaining.filter((_, index) => !selected.has(index));
  }
  return writer;
}

function tetrominoColorEncoding(
  codes: number[],
  modeWidth: number,
): BitWriter | null {
  const counts = new Map<number, number>();
  for (const code of codes) {
    if (code === 0) continue;
    if (code === 1) return null;
    counts.set(code, (counts.get(code) ?? 0) + 1);
  }
  const palette = Array.from(counts.keys()).sort((left, right) => left - right);
  if (palette.length === 0 || palette.some((code) => counts.get(code) !== 4)) {
    return null;
  }

  const writer = new BitWriter();
  writer.writeBits(5, modeWidth);
  const occupied = codes.map((code) => Number(code !== 0));
  writeOccupancyMask(writer, occupied);
  writer.writeBits(colorMask(palette) >>> 2, 7);

  let remaining = codes
    .map((code, index) => ({ code, index }))
    .filter(({ code }) => code !== 0)
    .map(({ index }) => index);
  for (
    let paletteIndex = 0;
    paletteIndex < palette.length - 1;
    paletteIndex += 1
  ) {
    const code = palette[paletteIndex];
    const positions: number[] = [];
    for (let index = 0; index < remaining.length; index += 1) {
      if (codes[remaining[index]] === code) positions.push(index);
    }
    const choiceCount = combinationCount(remaining.length, 4);
    writer.writeBigBits(
      combinationRank(positions),
      bitsForBigChoices(choiceCount),
    );
    const selected = new Set(positions);
    remaining = remaining.filter((_, index) => !selected.has(index));
  }
  return writer;
}

function readCellEncoding(
  reader: BitReader,
  cellCount: number,
  previousCodes: number[] | null,
  modeWidth: number,
): number[] {
  const mode = reader.readBits(modeWidth);
  if (mode === 0) {
    const palette = paletteFromMask(reader.readBits(9), 0);
    if (palette.length === 0) {
      throw new Ctk3CodecError("CTK3 palette is empty.");
    }
    const width = bitsForChoices(palette.length);
    return Array.from({ length: cellCount }, () => {
      const index = reader.readBits(width);
      const code = palette[index];
      if (code === undefined) {
        throw new Ctk3CodecError("CTK3 palette index is invalid.");
      }
      return code;
    });
  }
  if (mode === 1) {
    const runCount = reader.readVarUint();
    const codes: number[] = [];
    for (let index = 0; index < runCount; index += 1) {
      const code = reader.readBits(4);
      const length = reader.readVarUint() + 1;
      assertColorCode(code);
      if (length > cellCount - codes.length) {
        throw new Ctk3CodecError("CTK3 color run exceeds the field.");
      }
      codes.push(...Array<number>(length).fill(code));
    }
    if (codes.length !== cellCount) {
      throw new Ctk3CodecError("CTK3 color runs do not fill the field.");
    }
    return codes;
  }
  if (mode === 2) {
    if (!previousCodes) {
      throw new Ctk3CodecError("CTK3 delta field has no previous page.");
    }
    const codes = Array.from(
      { length: cellCount },
      (_, index) => previousCodes[index] ?? 0,
    );
    const changeCount = reader.readVarUint();
    let previousIndex = -1;
    for (let index = 0; index < changeCount; index += 1) {
      const cellIndex = previousIndex + reader.readVarUint() + 1;
      const code = reader.readBits(4);
      assertColorCode(code);
      if (cellIndex <= previousIndex || cellIndex >= cellCount) {
        throw new Ctk3CodecError("CTK3 delta cell index is invalid.");
      }
      codes[cellIndex] = code;
      previousIndex = cellIndex;
    }
    return codes;
  }

  if (mode === 4 && modeWidth >= 3) {
    return readMultisetEncoding(reader, cellCount);
  }
  if (mode === 5 && modeWidth >= 3) {
    return readTetrominoColorEncoding(reader, cellCount);
  }
  if (mode === 6 && modeWidth >= 3) {
    if (!previousCodes) {
      throw new Ctk3CodecError("CTK3 copied field has no previous page.");
    }
    return Array.from(
      { length: cellCount },
      (_, index) => previousCodes[index] ?? 0,
    );
  }
  if (mode === 7 && modeWidth >= 3) {
    return Array<number>(cellCount).fill(0);
  }
  if (mode === 8 && modeWidth >= 4) {
    const codes = fittedPreviousCodes(previousCodes, cellCount);
    const changed = readOccupancyMask(reader, cellCount);
    const palette = paletteFromMask(reader.readBits(9), 0);
    const changeCount = changed.reduce((total, value) => total + value, 0);
    if (changeCount > 0 && palette.length === 0) {
      throw new Ctk3CodecError("CTK3 delta palette is empty.");
    }
    const width = bitsForChoices(palette.length);
    for (let index = 0; index < cellCount; index += 1) {
      if (!changed[index]) continue;
      const code = palette[reader.readBits(width)];
      if (code === undefined) {
        throw new Ctk3CodecError("CTK3 delta palette index is invalid.");
      }
      codes[index] = code;
    }
    return codes;
  }
  if (mode === 9 && modeWidth >= 4) {
    const codes = fittedPreviousCodes(previousCodes, cellCount);
    const palette = paletteFromMask(reader.readBits(9), 0);
    const runCount = reader.readVarUint();
    const runs: Array<{ start: number; length: number }> = [];
    let previousEnd = 0;
    let changeCount = 0;
    for (let index = 0; index < runCount; index += 1) {
      const start = previousEnd + reader.readVarUint();
      const length = reader.readVarUint() + 1;
      if (start < previousEnd || length > cellCount - start) {
        throw new Ctk3CodecError("CTK3 delta run is invalid.");
      }
      runs.push({ start, length });
      previousEnd = start + length;
      changeCount += length;
    }
    if (changeCount > 0 && palette.length === 0) {
      throw new Ctk3CodecError("CTK3 delta palette is empty.");
    }
    const width = bitsForChoices(palette.length);
    for (const run of runs) {
      for (let offset = 0; offset < run.length; offset += 1) {
        const code = palette[reader.readBits(width)];
        if (code === undefined) {
          throw new Ctk3CodecError("CTK3 delta palette index is invalid.");
        }
        codes[run.start + offset] = code;
      }
    }
    return codes;
  }
  if (mode === 10 && modeWidth >= 4) {
    const codes = fittedPreviousCodes(previousCodes, cellCount);
    const code = reader.readBits(4);
    assertColorCode(code);
    const changed = readOccupancyMask(reader, cellCount);
    for (let index = 0; index < cellCount; index += 1) {
      if (changed[index]) codes[index] = code;
    }
    return codes;
  }
  if (mode === 11 && modeWidth >= 4) {
    const codes = fittedPreviousCodes(previousCodes, cellCount);
    const count = reader.readVarUint();
    if (count > cellCount) {
      throw new Ctk3CodecError("CTK3 delta count exceeds the field.");
    }
    const choiceCount = combinationCount(cellCount, count);
    const rank = reader.readBigBits(bitsForBigChoices(choiceCount));
    if (rank >= choiceCount) {
      throw new Ctk3CodecError("CTK3 delta rank is invalid.");
    }
    const positions = combinationUnrank(cellCount, count, rank);
    const palette = paletteFromMask(reader.readBits(9), 0);
    if (count > 0 && palette.length === 0) {
      throw new Ctk3CodecError("CTK3 delta palette is empty.");
    }
    const width = bitsForChoices(palette.length);
    for (const position of positions) {
      const code = palette[reader.readBits(width)];
      if (code === undefined) {
        throw new Ctk3CodecError("CTK3 delta palette index is invalid.");
      }
      codes[position] = code;
    }
    return codes;
  }
  if (mode !== 3) {
    throw new Ctk3CodecError("CTK3 field encoding mode is invalid.");
  }

  const occupied = readOccupancyMask(reader, cellCount);
  const palette = paletteFromMask(reader.readBits(8) << 1, 1);
  const occupiedCount = occupied.reduce((total, value) => total + value, 0);
  if (occupiedCount > 0 && palette.length === 0) {
    throw new Ctk3CodecError("CTK3 occupied color palette is empty.");
  }
  const width = bitsForChoices(palette.length);
  return occupied.map((value) => {
    if (!value) return 0;
    const code = palette[reader.readBits(width)];
    if (code === undefined) {
      throw new Ctk3CodecError("CTK3 occupied color index is invalid.");
    }
    return code;
  });
}

function fittedPreviousCodes(
  previousCodes: number[] | null,
  cellCount: number,
): number[] {
  if (!previousCodes) {
    throw new Ctk3CodecError("CTK3 delta field has no predictor.");
  }
  return Array.from(
    { length: cellCount },
    (_, index) => previousCodes[index] ?? 0,
  );
}

function readOccupancyMask(reader: BitReader, cellCount: number): number[] {
  const occupancyMode = reader.readBit();
  const occupied: number[] = [];
  if (occupancyMode === 0) {
    for (let index = 0; index < cellCount; index += 1) {
      occupied.push(reader.readBit());
    }
  } else if (cellCount > 0) {
    let value = reader.readBit();
    while (occupied.length < cellCount) {
      const length = reader.readVarUint() + 1;
      if (length > cellCount - occupied.length) {
        throw new Ctk3CodecError("CTK3 occupancy run exceeds the field.");
      }
      occupied.push(...Array<number>(length).fill(value));
      value ^= 1;
    }
  }
  return occupied;
}

function readMultisetEncoding(reader: BitReader, cellCount: number): number[] {
  const palette = paletteFromMask(reader.readBits(9), 0);
  if (palette.length === 0) {
    throw new Ctk3CodecError("CTK3 multiset palette is empty.");
  }
  const codes = Array<number>(cellCount).fill(-1);
  let remaining = Array.from({ length: cellCount }, (_, index) => index);
  for (
    let paletteIndex = 0;
    paletteIndex < palette.length - 1;
    paletteIndex += 1
  ) {
    const count = reader.readVarUint();
    if (count > remaining.length) {
      throw new Ctk3CodecError("CTK3 multiset count exceeds the field.");
    }
    const choiceCount = combinationCount(remaining.length, count);
    const rank = reader.readBigBits(bitsForBigChoices(choiceCount));
    if (rank >= choiceCount) {
      throw new Ctk3CodecError("CTK3 multiset rank is invalid.");
    }
    const positions = combinationUnrank(remaining.length, count, rank);
    const selected = new Set(positions);
    for (const position of positions) {
      codes[remaining[position]] = palette[paletteIndex];
    }
    remaining = remaining.filter((_, index) => !selected.has(index));
  }
  for (const index of remaining) codes[index] = palette[palette.length - 1];
  return codes;
}

function readTetrominoColorEncoding(
  reader: BitReader,
  cellCount: number,
): number[] {
  const occupied = readOccupancyMask(reader, cellCount);
  const palette = paletteFromMask(reader.readBits(7) << 2, 2);
  const occupiedCount = occupied.reduce((total, value) => total + value, 0);
  if (palette.length === 0 || occupiedCount !== palette.length * 4) {
    throw new Ctk3CodecError("CTK3 tetromino color field is invalid.");
  }

  const codes = Array<number>(cellCount).fill(0);
  let remaining = occupied
    .map((value, index) => ({ value, index }))
    .filter(({ value }) => value !== 0)
    .map(({ index }) => index);
  for (
    let paletteIndex = 0;
    paletteIndex < palette.length - 1;
    paletteIndex += 1
  ) {
    const choiceCount = combinationCount(remaining.length, 4);
    const rank = reader.readBigBits(bitsForBigChoices(choiceCount));
    if (rank >= choiceCount) {
      throw new Ctk3CodecError("CTK3 tetromino color rank is invalid.");
    }
    const positions = combinationUnrank(remaining.length, 4, rank);
    const selected = new Set(positions);
    for (const position of positions) {
      codes[remaining[position]] = palette[paletteIndex];
    }
    remaining = remaining.filter((_, index) => !selected.has(index));
  }
  for (const index of remaining) codes[index] = palette[palette.length - 1];
  return codes;
}

function uniqueCodes(codes: number[]): number[] {
  return Array.from(new Set(codes)).sort((left, right) => left - right);
}

function colorMask(codes: number[]): number {
  return codes.reduce((mask, code) => mask | (1 << code), 0);
}

function paletteFromMask(mask: number, minimumCode: number): number[] {
  const palette: number[] = [];
  for (let code = minimumCode; code < COLORS.length; code += 1) {
    if (mask & (1 << code)) palette.push(code);
  }
  return palette;
}

function bitsForChoices(choiceCount: number): number {
  if (choiceCount <= 1) return 0;
  return Math.ceil(Math.log2(choiceCount));
}

function bitsForBigChoices(choiceCount: bigint): number {
  if (choiceCount <= 1n) return 0;
  return (choiceCount - 1n).toString(2).length;
}

function combinationCount(total: number, selected: number): bigint {
  if (
    !Number.isInteger(total) ||
    !Number.isInteger(selected) ||
    selected < 0 ||
    selected > total
  ) {
    return 0n;
  }
  const count = Math.min(selected, total - selected);
  let value = 1n;
  for (let index = 1; index <= count; index += 1) {
    value = (value * BigInt(total - count + index)) / BigInt(index);
  }
  return value;
}

function combinationRank(positions: number[]): bigint {
  return positions.reduce(
    (rank, position, index) => rank + combinationCount(position, index + 1),
    0n,
  );
}

function combinationUnrank(
  total: number,
  selected: number,
  sourceRank: bigint,
): number[] {
  const positions = Array<number>(selected);
  let rank = sourceRank;
  let upper = total - 1;
  for (let index = selected; index >= 1; index -= 1) {
    let low = index - 1;
    let high = upper;
    let position = low;
    while (low <= high) {
      const candidate = Math.floor((low + high) / 2);
      if (combinationCount(candidate, index) <= rank) {
        position = candidate;
        low = candidate + 1;
      } else {
        high = candidate - 1;
      }
    }
    positions[index - 1] = position;
    rank -= combinationCount(position, index);
    upper = position - 1;
  }
  if (rank !== 0n) {
    throw new Ctk3CodecError("CTK3 multiset rank is invalid.");
  }
  return positions;
}

function flagsEqual(left: Ctk3PageFlags, right: Ctk3PageFlags): boolean {
  return flagBits(left) === flagBits(right);
}

function flagBits(flags: Ctk3PageFlags): number {
  return (
    Number(flags.lock) |
    (Number(flags.mirror) << 1) |
    (Number(flags.colorize) << 2) |
    (Number(flags.rise) << 3) |
    (Number(flags.quiz) << 4)
  );
}

function flagsFromBits(bits: number): Ctk3PageFlags {
  return {
    lock: Boolean(bits & 1),
    mirror: Boolean(bits & 2),
    colorize: Boolean(bits & 4),
    rise: Boolean(bits & 8),
    quiz: Boolean(bits & 16),
  };
}

function colorCode(color: Ctk3Color): number {
  const code = COLORS.indexOf(color);
  if (code < 0) throw new Ctk3CodecError("CTK3 field color is invalid.");
  return code;
}

function codeColor(code: number): Ctk3Color {
  assertColorCode(code);
  return COLORS[code];
}

function assertColorCode(code: number) {
  if (!Number.isInteger(code) || code < 0 || code >= COLORS.length) {
    throw new Ctk3CodecError("CTK3 field color is invalid.");
  }
}

function exactCtk64Payload(segment: string): string {
  const source = segment.trim();
  if (
    source.slice(0, CTK64_PREFIX.length).toLowerCase() !== CTK64_PREFIX ||
    source.length === CTK64_PREFIX.length
  ) {
    throw new Ctk3CodecError("CTK3 bundle segment is invalid.");
  }
  const payload = source.slice(CTK64_PREFIX.length);
  if (
    payload.length % 4 === 1 ||
    payload.length > MAX_CTK64_CHARACTERS ||
    !isCtk64Payload(payload)
  ) {
    throw new Ctk3CodecError("CTK3 bundle segment is invalid.");
  }
  return payload;
}

function inspectSingleCtk3(input: string): {
  width: number;
  pageCount: number;
} {
  const encoded = extractCtk3Payload(input);
  if (encoded.transport === "ctk64") {
    if (
      encoded.payload.length > MAX_CTK64_CHARACTERS ||
      encoded.payload.length % 4 === 1
    ) {
      throw new Ctk3CodecError("CTK3 payload length is invalid.");
    }
    return readDocumentHeader(decodeCtk64Prefix(encoded.payload, 8));
  }
  if (encoded.payload.length > MAX_CTK85_CHARACTERS) {
    throw new Ctk3CodecError("CTK3 payload length is invalid.");
  }
  return readDocumentHeader(decodeCtk85(encoded.payload));
}

function inspectCtk64PayloadHeader(payload: string): {
  width: number;
  pageCount: number;
} {
  if (!payload.length || payload.length % 4 === 1) {
    throw new Ctk3CodecError("CTK64 payload length is invalid.");
  }
  return readDocumentHeader(decodeCtk64Prefix(payload, 8));
}

function readDocumentHeader(payload: Uint8Array): {
  width: number;
  pageCount: number;
} {
  const reader = new BitReader(payload);
  if (reader.readBits(8) !== MAGIC) {
    throw new Ctk3CodecError("CTK3 payload header is invalid.");
  }
  const schemaRevision = reader.readBits(3);
  if (
    schemaRevision !== LEGACY_SCHEMA_REVISION &&
    schemaRevision !== COMPACT_SCHEMA_REVISION &&
    schemaRevision !== TEMPORAL_SCHEMA_REVISION &&
    schemaRevision !== SHARED_FIELD_SCHEMA_REVISION
  ) {
    throw new Ctk3CodecError("CTK3 schema revision is unsupported.");
  }
  const width = reader.readBits(5) + 1;
  if (width < 1 || width > MAX_WIDTH) {
    throw new Ctk3CodecError("CTK3 board width is invalid.");
  }
  const pageCount = reader.readVarUint();
  if (pageCount < 1 || pageCount > MAX_PAGES) {
    throw new Ctk3CodecError("CTK3 page count is invalid.");
  }
  if (reader.readBit() !== 0) {
    throw new Ctk3CodecError("CTK3 extension block is unsupported.");
  }
  return { width, pageCount };
}

function extractCtk3BundlePayloads(input: string): string[] | null {
  let source = input.trim();
  let prefixIndex = source.toLowerCase().indexOf(CTK3_BUNDLE_PREFIX);
  if (prefixIndex < 0 && /%[0-9a-f]{2}/i.test(source)) {
    try {
      source = decodeURIComponent(source);
      prefixIndex = source.toLowerCase().indexOf(CTK3_BUNDLE_PREFIX);
    } catch {
      return null;
    }
  }
  if (prefixIndex < 0) return null;

  let end = prefixIndex + CTK3_BUNDLE_PREFIX.length;
  while (
    end < source.length &&
    (source[end] === "." || CTK64_INDEX.has(source[end]))
  ) {
    end += 1;
  }
  const payloads = source
    .slice(prefixIndex + CTK3_BUNDLE_PREFIX.length, end)
    .split(".");
  if (
    payloads.length < 2 ||
    payloads.length > MAX_BUNDLE_SEGMENTS ||
    payloads.some(
      (payload) =>
        payload.length < 1 ||
        payload.length > MAX_CTK64_CHARACTERS ||
        !isCtk64Payload(payload),
    )
  ) {
    throw new Ctk3CodecError("CTK3 bundle payload is invalid.");
  }
  return payloads;
}

function isCtk64Payload(payload: string): boolean {
  for (let index = 0; index < payload.length; index += 1) {
    if (!CTK64_INDEX.has(payload[index])) return false;
  }
  return true;
}

function extractCtk3Payload(input: string): {
  transport: "ctk64" | "ctk85";
  payload: string;
} {
  let source = input.trim();
  if (!isCtk3(source) && /%[0-9a-f]{2}/i.test(source)) {
    try {
      source = decodeURIComponent(source);
    } catch {
      // The raw CTK85 alphabet includes %, so malformed URL escaping is kept raw.
    }
  }
  const lower = source.toLowerCase();
  const ctk85Index = lower.indexOf(CTK3_PREFIX);
  const ctk64Index = lower.indexOf(CTK64_PREFIX);
  const useCtk64 =
    ctk64Index >= 0 && (ctk85Index < 0 || ctk64Index < ctk85Index);
  const prefix = useCtk64 ? CTK64_PREFIX : CTK3_PREFIX;
  const prefixIndex = useCtk64 ? ctk64Index : ctk85Index;
  if (prefixIndex < 0) throw new Ctk3CodecError("No CTK3 header was found.");
  const alphabet = useCtk64 ? CTK64_INDEX : CTK85_INDEX;
  let end = prefixIndex + prefix.length;
  while (end < source.length && alphabet.has(source[end])) end += 1;
  const payload = source.slice(prefixIndex + prefix.length, end);
  if (!payload.length) {
    throw new Ctk3CodecError("CTK3 payload is empty.");
  }
  return {
    transport: useCtk64 ? "ctk64" : "ctk85",
    payload,
  };
}

function encodeCtk64(bytes: Uint8Array): string {
  let output = "";
  let value = 0;
  let bitCount = 0;
  for (const byte of bytes) {
    value = value * 256 + byte;
    bitCount += 8;
    while (bitCount >= 6) {
      bitCount -= 6;
      const divisor = 2 ** bitCount;
      const digit = Math.floor(value / divisor);
      output += CTK64_ALPHABET[digit];
      value %= divisor;
    }
  }
  if (bitCount > 0) {
    output += CTK64_ALPHABET[value * 2 ** (6 - bitCount)];
  }
  return output;
}

function decodeCtk64(encoded: string): Uint8Array {
  if (!encoded.length || encoded.length % 4 === 1) {
    throw new Ctk3CodecError("CTK64 payload length is invalid.");
  }
  const bytes: number[] = [];
  let value = 0;
  let bitCount = 0;
  for (const character of encoded) {
    const digit = CTK64_INDEX.get(character);
    if (digit === undefined) {
      throw new Ctk3CodecError("CTK64 payload contains an invalid character.");
    }
    value = value * 64 + digit;
    bitCount += 6;
    while (bitCount >= 8) {
      bitCount -= 8;
      const divisor = 2 ** bitCount;
      bytes.push(Math.floor(value / divisor));
      value %= divisor;
    }
  }
  if (value !== 0) {
    throw new Ctk3CodecError("CTK64 payload has non-zero trailing bits.");
  }
  const decoded = Uint8Array.from(bytes);
  if (encodeCtk64(decoded) !== encoded) {
    throw new Ctk3CodecError("CTK64 payload is not canonical.");
  }
  return decoded;
}

function decodeCtk64Prefix(encoded: string, maximumBytes: number): Uint8Array {
  const targetLength = Math.min(
    maximumBytes,
    Math.floor((encoded.length * 6) / 8),
  );
  const bytes = new Uint8Array(targetLength);
  let byteIndex = 0;
  let value = 0;
  let bitCount = 0;
  for (let index = 0; index < encoded.length && byteIndex < targetLength; index += 1) {
    const digit = CTK64_INDEX.get(encoded[index]);
    if (digit === undefined) {
      throw new Ctk3CodecError("CTK64 payload contains an invalid character.");
    }
    value = value * 64 + digit;
    bitCount += 6;
    while (bitCount >= 8 && byteIndex < targetLength) {
      bitCount -= 8;
      const divisor = 2 ** bitCount;
      bytes[byteIndex] = Math.floor(value / divisor);
      byteIndex += 1;
      value %= divisor;
    }
  }
  return bytes;
}

function encodeCtk85(bytes: Uint8Array): string {
  let output = "";
  let offset = 0;
  while (offset + 4 <= bytes.length) {
    const value =
      bytes[offset] * 0x1000000 +
      bytes[offset + 1] * 0x10000 +
      bytes[offset + 2] * 0x100 +
      bytes[offset + 3];
    output += encodeCtk85Value(value, 5);
    offset += 4;
  }
  const remaining = bytes.length - offset;
  if (remaining > 0) {
    let value = 0;
    for (let index = 0; index < remaining; index += 1) {
      value = value * 256 + bytes[offset + index];
    }
    output += encodeCtk85Value(value, remaining + 1);
  }
  return output;
}

function encodeCtk85Value(value: number, digits: number): string {
  const encoded = Array<string>(digits);
  for (let index = digits - 1; index >= 0; index -= 1) {
    encoded[index] = CTK85_ALPHABET[value % 85];
    value = Math.floor(value / 85);
  }
  if (value !== 0) throw new Ctk3CodecError("CTK85 value overflow.");
  return encoded.join("");
}

function decodeCtk85(encoded: string): Uint8Array {
  if (!encoded.length || encoded.length % 5 === 1) {
    throw new Ctk3CodecError("CTK85 payload length is invalid.");
  }
  const bytes: number[] = [];
  const completeLength = encoded.length - (encoded.length % 5);
  for (let offset = 0; offset < completeLength; offset += 5) {
    const value = decodeCtk85Value(encoded.slice(offset, offset + 5));
    if (value > 0xffffffff) {
      throw new Ctk3CodecError("CTK85 block overflows 32 bits.");
    }
    bytes.push(
      Math.floor(value / 0x1000000),
      Math.floor(value / 0x10000) & 0xff,
      Math.floor(value / 0x100) & 0xff,
      value & 0xff,
    );
  }
  const remaining = encoded.length - completeLength;
  if (remaining > 0) {
    const byteCount = remaining - 1;
    const value = decodeCtk85Value(encoded.slice(completeLength));
    if (value >= 2 ** (byteCount * 8)) {
      throw new Ctk3CodecError("CTK85 partial block overflows its byte range.");
    }
    for (let shift = byteCount - 1; shift >= 0; shift -= 1) {
      bytes.push(Math.floor(value / 2 ** (shift * 8)) & 0xff);
    }
  }
  return Uint8Array.from(bytes);
}

function decodeCtk85Value(encoded: string): number {
  let value = 0;
  for (const character of encoded) {
    const digit = CTK85_INDEX.get(character);
    if (digit === undefined) {
      throw new Ctk3CodecError("CTK85 payload contains an invalid character.");
    }
    value = value * 85 + digit;
  }
  return value;
}

function crc16(bytes: Uint8Array): number {
  let crc = 0xffff;
  for (const byte of bytes) {
    crc ^= byte << 8;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = crc & 0x8000 ? ((crc << 1) ^ 0x1021) & 0xffff : (crc << 1) & 0xffff;
    }
  }
  return crc;
}

class BitWriter {
  private readonly bytes: number[] = [];
  bitLength = 0;

  writeBit(value: number) {
    const byteIndex = Math.floor(this.bitLength / 8);
    const bitIndex = this.bitLength % 8;
    if (byteIndex === this.bytes.length) this.bytes.push(0);
    if (value & 1) this.bytes[byteIndex] |= 1 << bitIndex;
    this.bitLength += 1;
  }

  writeBits(value: number, width: number) {
    if (!Number.isSafeInteger(value) || value < 0 || width < 0 || width > 32) {
      throw new Ctk3CodecError("CTK3 bit value is invalid.");
    }
    if (width < 32 && value >= 2 ** width) {
      throw new Ctk3CodecError("CTK3 bit value exceeds its width.");
    }
    for (let bit = 0; bit < width; bit += 1) {
      this.writeBit(Math.floor(value / 2 ** bit) & 1);
    }
  }

  writeBigBits(value: bigint, width: number) {
    if (value < 0n || width < 0 || value >= 1n << BigInt(width)) {
      throw new Ctk3CodecError("CTK3 large bit value is invalid.");
    }
    for (let bit = 0; bit < width; bit += 1) {
      this.writeBit(Number((value >> BigInt(bit)) & 1n));
    }
  }

  writeVarUint(value: number) {
    if (!Number.isSafeInteger(value) || value < 0 || value > 0xffffffff) {
      throw new Ctk3CodecError("CTK3 unsigned integer is invalid.");
    }
    if (value < 16) {
      this.writeBit(0);
      this.writeBits(value, 4);
    } else if (value < 256) {
      this.writeBits(1, 2); // 10 when read least-significant bit first.
      this.writeBits(value, 8);
    } else if (value < 65536) {
      this.writeBits(3, 3); // 110
      this.writeBits(value, 16);
    } else {
      this.writeBits(7, 3); // 111
      this.writeBits(value, 32);
    }
  }

  writeSignedVarInt(value: number) {
    const encoded = value >= 0 ? value * 2 : -value * 2 - 1;
    this.writeVarUint(encoded);
  }

  writeBytes(bytes: Uint8Array) {
    for (const byte of bytes) this.writeBits(byte, 8);
  }

  append(other: BitWriter) {
    const bytes = other.toBytes();
    for (let bit = 0; bit < other.bitLength; bit += 1) {
      this.writeBit((bytes[Math.floor(bit / 8)] >>> (bit % 8)) & 1);
    }
  }

  toBytes(): Uint8Array {
    return Uint8Array.from(this.bytes);
  }
}

class BitReader {
  private bitOffset = 0;
  private readonly bytes: Uint8Array;

  constructor(bytes: Uint8Array) {
    this.bytes = bytes;
  }

  readBit(): number {
    if (this.bitOffset >= this.bytes.length * 8) {
      throw new Ctk3CodecError("CTK3 payload ended unexpectedly.");
    }
    const value =
      (this.bytes[Math.floor(this.bitOffset / 8)] >>> (this.bitOffset % 8)) & 1;
    this.bitOffset += 1;
    return value;
  }

  readBits(width: number): number {
    let value = 0;
    for (let bit = 0; bit < width; bit += 1) {
      value += this.readBit() * 2 ** bit;
    }
    return value;
  }

  readBigBits(width: number): bigint {
    let value = 0n;
    for (let bit = 0; bit < width; bit += 1) {
      if (this.readBit()) value |= 1n << BigInt(bit);
    }
    return value;
  }

  readVarUint(): number {
    if (this.readBit() === 0) return this.readBits(4);
    if (this.readBit() === 0) return this.readBits(8);
    if (this.readBit() === 0) return this.readBits(16);
    return this.readBits(32);
  }

  readSignedVarInt(): number {
    const value = this.readVarUint();
    return value & 1 ? -(Math.floor(value / 2) + 1) : Math.floor(value / 2);
  }

  readBytes(length: number): Uint8Array {
    const bytes = new Uint8Array(length);
    for (let index = 0; index < length; index += 1) {
      bytes[index] = this.readBits(8);
    }
    return bytes;
  }

  assertZeroPadding() {
    while (this.bitOffset < this.bytes.length * 8) {
      if (this.readBit() !== 0) {
        throw new Ctk3CodecError("CTK3 payload has trailing data.");
      }
    }
  }
}
