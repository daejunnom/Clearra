// SRP: validate and delimit one already-addressed Hydra graph block. This
// module also derives the immutable sparse block directory from qualified
// GOFF bytes. It performs no caching, profile discovery, traversal or fallback.

import { Pc4StreamSha256 } from './pc4-stream-sha256.mjs';

const HEADER_BYTES = 16;
export async function buildPc4GraphBlockDirectory(readOffsets, { fieldCount, graphBytes,
  blockRecords = 16, maxBlockBytes = 65_536, readBytes = 1024 * 1024 } = {}) {
  if (typeof readOffsets !== 'function' || !Number.isSafeInteger(fieldCount) || fieldCount < 1 ||
      fieldCount > 2 ** 24 || !Number.isSafeInteger(graphBytes) || graphBytes < 1 ||
      graphBytes > 0xffff_ffff || !Number.isSafeInteger(blockRecords) || blockRecords < 1 ||
      blockRecords > 1024 || (blockRecords & (blockRecords - 1)) ||
      !Number.isSafeInteger(maxBlockBytes) || maxBlockBytes < 12 || maxBlockBytes > 65_536 ||
      !Number.isSafeInteger(readBytes) || readBytes < 4096 || readBytes > 1024 * 1024) fail();
  const header = await exactRead(readOffsets, 0, HEADER_BYTES);
  const headerView = new DataView(header.buffer, header.byteOffset, header.byteLength);
  if (new TextDecoder().decode(header.subarray(0, 8)) !== 'GOFFIDX1' ||
      headerView.getUint32(8, true) !== 1 || headerView.getUint32(12, true) !== fieldCount) fail();

  const blockCount = Math.ceil(fieldCount / blockRecords);
  const output = new Uint8Array(HEADER_BYTES + 4 * (blockCount + 1));
  output.set(new TextEncoder().encode('GBLKIDX1'));
  const view = new DataView(output.buffer);
  view.setUint32(8, 1, true); view.setUint32(12, fieldCount, true);
  const entriesPerRead = Math.max(1, Math.floor(readBytes / (blockRecords * 4)));
  let previous = null;
  for (let firstBlock = 0; firstBlock <= blockCount; firstBlock += entriesPerRead) {
    const endBlock = Math.min(blockCount + 1, firstBlock + entriesPerRead);
    const firstOrdinal = Math.min(fieldCount, firstBlock * blockRecords);
    const lastOrdinal = Math.min(fieldCount, (endBlock - 1) * blockRecords);
    const source = await exactRead(readOffsets, HEADER_BYTES + firstOrdinal * 4,
      (lastOrdinal - firstOrdinal) * 4 + 4);
    const sourceView = new DataView(source.buffer, source.byteOffset, source.byteLength);
    for (let block = firstBlock; block < endBlock; block++) {
      const ordinal = Math.min(fieldCount, block * blockRecords);
      const value = sourceView.getUint32((ordinal - firstOrdinal) * 4, true);
      if (previous !== null && (value < previous || value - previous > maxBlockBytes)) fail();
      view.setUint32(HEADER_BYTES + block * 4, value, true);
      previous = value;
    }
  }
  if (view.getUint32(HEADER_BYTES, true) !== 0 || previous !== graphBytes) fail();
  const digest = new Pc4StreamSha256(); digest.update(output);
  return Object.freeze({ bytes: output, contentIdentity: `sha256:${digest.hex()}`,
    blockRecords, fieldCount, maximumBlockBytes: maxBlockBytes });
}

export function parsePc4HydraGraphBlock(bytes, { recordCount, targetWidth, fieldCount }) {
  if (!(bytes instanceof Uint8Array) || !Number.isSafeInteger(recordCount) || recordCount < 1 ||
      recordCount > 1024 || ![3, 4].includes(targetWidth) || !Number.isSafeInteger(fieldCount) ||
      fieldCount < recordCount || fieldCount > 2 ** 24) fail();
  const bounds = [];
  let cursor = 0;
  for (let record = 0; record < recordCount; record++) {
    const start = cursor;
    if (cursor + 5 > bytes.length) fail();
    cursor += 5;
    let cumulative = 0;
    for (let piece = 0; piece < 7; piece++) {
      if (cursor >= bytes.length) fail();
      const degree = bytes[cursor++];
      cumulative += degree;
      if (cumulative > 255 || cursor + degree * targetWidth > bytes.length) fail();
      // This owner only delimits records. The qualified Rust materializer
      // validates every requested target ID and source hash before use.
      cursor += degree * targetWidth;
    }
    bounds.push(Object.freeze([start, cursor]));
  }
  if (cursor !== bytes.length) fail();
  return Object.freeze(bounds);
}

function fail() {
  throw Object.assign(new Error('pc4_local_graph_block_invalid'), { code: 'pc4_local_graph_block_invalid' });
}

async function exactRead(read, offset, length) {
  const bytes = await read(offset, length);
  if (!(bytes instanceof Uint8Array) || bytes.length !== length) fail();
  return bytes;
}
