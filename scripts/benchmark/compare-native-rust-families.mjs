// Local correctness diagnostic, not performance or release acceptance evidence.
import { createHash } from 'node:crypto';
import { lstat } from 'node:fs/promises';
import { createReadStream } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const CAPTURE_SCHEMA = 'clearra.native-rust-family.v1';
const RESULT_SCHEMA = 'clearra.native-rust-family-diff.v1';
const CAPTURE_LIMIT = 128 * 1024 * 1024;
const KEY_COUNT_LIMIT = 1_000_000;
const KEY_BYTE_LIMIT = 1024;
const LINE_BYTE_LIMIT = 8192;
const DEFAULT_WITNESS_LIMIT = 16;
const MAX_WITNESS_LIMIT = 64;
const CASES = new Set(['fixed-2l', 'existing-p7', 'full-p7p4']);
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const COMMIT_PATTERN = /^[0-9a-f]{40}$/;
const SOLUTION_HASH_PATTERN = /^cts1:[0-9a-f]{16}$/;
const CONTROL_CHARACTER_PATTERN = /[\u0000-\u001f\u007f-\u009f]/u;

const HEADER_FIELDS = [
  'kind', 'schema', 'source_commit', 'arm', 'case', 'requested_workers',
  'available_parallelism', 'backend_selected', 'reported_workers',
  'count_complete', 'resource_truncated',
];
const KEY_FIELDS = ['kind', 'key'];
const FOOTER_FIELDS = ['kind', 'count', 'keys_sha256', 'solution_set_hash'];
const SHARED_HEADER_FIELDS = ['source_commit', 'case', 'requested_workers'];

const integer = (value, min, max = Number.MAX_SAFE_INTEGER) =>
  Number.isSafeInteger(value) && value >= min && value <= max;
const matches = (value, pattern, length) =>
  typeof value === 'string' && value.length === length && pattern.test(value);

function requireValid(condition, message) {
  if (!condition) throw new Error(message);
}

// The captured file is LF-terminated UTF-8. Bounded splitting happens before
// decoding or JSON parsing; an oversized unterminated line never accumulates.
async function* readRecords(chunks, arm) {
  const line = Buffer.alloc(LINE_BYTE_LIMIT);
  const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
  let bytesRead = 0;
  let lineLength = 0;
  for await (const chunk of chunks) {
    requireValid(Buffer.isBuffer(chunk), arm + ': expected byte stream');
    bytesRead += chunk.length;
    requireValid(bytesRead <= CAPTURE_LIMIT, arm + ': capture byte limit exceeded');
    let offset = 0;
    while (offset < chunk.length) {
      const lf = chunk.indexOf(0x0a, offset);
      const end = lf === -1 ? chunk.length : lf;
      const copyLength = end - offset;
      requireValid(lineLength + copyLength <= LINE_BYTE_LIMIT, arm + ': line byte limit exceeded');
      chunk.copy(line, lineLength, offset, end);
      lineLength += copyLength;
      if (lf === -1) break;
      let record;
      try {
        record = JSON.parse(decoder.decode(line.subarray(0, lineLength)));
      } catch {
        throw new Error(arm + ': invalid UTF-8 or JSON capture record');
      }
      lineLength = 0;
      offset = lf + 1;
      requireValid(record !== null && typeof record === 'object' && !Array.isArray(record),
        arm + ': expected JSON object record');
      yield record;
    }
  }
  requireValid(lineLength === 0, arm + ': missing final LF');
}

function hasExactFields(record, fields) {
  return Object.keys(record).length === fields.length &&
    fields.every(field => Object.hasOwn(record, field));
}

function validateHeader(record, arm) {
  requireValid(record.kind === 'header' && hasExactFields(record, HEADER_FIELDS),
    arm + ': missing or invalid capture header');
  requireValid(record.schema === CAPTURE_SCHEMA && record.arm === arm,
    arm + ': unexpected capture schema or arm');
  requireValid(matches(record.source_commit, COMMIT_PATTERN, 40) && CASES.has(record.case),
    arm + ': invalid source commit or fixture');
  requireValid(integer(record.requested_workers, 1, 256) &&
    integer(record.available_parallelism, record.requested_workers) &&
    integer(record.reported_workers, 1, record.requested_workers),
  arm + ': invalid worker metadata');
  requireValid(record.backend_selected === (arm === 'native-c' ? 'cpu' : 'wasm-cpu'),
    arm + ': unexpected selected backend');
  requireValid(record.count_complete === true && record.resource_truncated === false,
    arm + ': incomplete or resource-truncated capture');
}

async function* validateCapture(chunks, arm) {
  const hash = createHash('sha256');
  const lengthPrefix = Buffer.alloc(8);
  let started = false;
  let footer;
  let previous;
  let count = 0;
  for await (const record of readRecords(chunks, arm)) {
    requireValid(footer === undefined, arm + ': record after footer');
    if (!started) {
      validateHeader(record, arm);
      started = true;
      yield record;
    } else if (record.kind === 'key') {
      requireValid(hasExactFields(record, KEY_FIELDS) && typeof record.key === 'string' &&
        !CONTROL_CHARACTER_PATTERN.test(record.key), arm + ': invalid key record');
      const bytes = Buffer.from(record.key, 'utf8');
      requireValid(integer(bytes.length, 1, KEY_BYTE_LIMIT) && bytes.toString('utf8') === record.key,
        arm + ': invalid canonical key length or encoding');
      requireValid(previous === undefined || Buffer.compare(previous, bytes) < 0,
        arm + ': keys are not strictly increasing in UTF-8 byte order');
      count += 1;
      requireValid(count <= KEY_COUNT_LIMIT, arm + ': key count limit exceeded');
      lengthPrefix.writeBigUInt64LE(BigInt(bytes.length));
      hash.update(lengthPrefix);
      hash.update(bytes);
      previous = bytes;
      yield { kind: 'key', key: record.key, bytes };
    } else {
      requireValid(record.kind === 'footer' && hasExactFields(record, FOOTER_FIELDS),
        arm + ': invalid capture footer or record kind');
      requireValid(count > 0 && record.count === count && matches(record.keys_sha256, SHA256_PATTERN, 64) &&
        matches(record.solution_set_hash, SOLUTION_HASH_PATTERN, 21),
      arm + ': invalid footer count or hash metadata');
      requireValid(record.keys_sha256 === hash.digest('hex'), arm + ': canonical key hash mismatch');
      footer = record;
    }
  }
  requireValid(started && footer !== undefined, arm + ': incomplete capture (missing header or footer)');
  // The independent SHA-256 was recomputed above. The engine's cts1 is
  // provenance metadata, not an independently reproduced set hash.
  yield footer;
}

function sideSummary(header, footer, exclusiveCount, witnesses) {
  return {
    backend_selected: header.backend_selected,
    available_parallelism: header.available_parallelism,
    reported_workers: header.reported_workers,
    count: footer.count,
    keys_sha256: footer.keys_sha256,
    reported_solution_set_hash: footer.solution_set_hash,
    exclusive_count: exclusiveCount,
    first_exclusive_keys: witnesses,
  };
}

export async function compareNativeRustFamilies(
  nativeChunks, rustChunks, { witnessLimit = DEFAULT_WITNESS_LIMIT } = {},
) {
  requireValid(integer(witnessLimit, 0, MAX_WITNESS_LIMIT), 'invalid witness limit');
  const native = validateCapture(nativeChunks, 'native-c');
  const rust = validateCapture(rustChunks, 'rust-cpu');
  try {
    const { value: nativeHeader } = await native.next();
    const { value: rustHeader } = await rust.next();
    for (const field of SHARED_HEADER_FIELDS) {
      requireValid(nativeHeader[field] === rustHeader[field], 'capture ' + field + ' mismatch');
    }
    let { value: left } = await native.next();
    let { value: right } = await rust.next();
    let shared = 0;
    let nativeOnly = 0;
    let rustOnly = 0;
    const nativeWitnesses = [];
    const rustWitnesses = [];
    while (left.kind === 'key' || right.kind === 'key') {
      const order = left.kind === 'footer' ? 1 : right.kind === 'footer' ? -1 :
        Buffer.compare(left.bytes, right.bytes);
      if (order === 0) {
        shared += 1;
      } else if (order < 0) {
        nativeOnly += 1;
        if (nativeWitnesses.length < witnessLimit) nativeWitnesses.push(left.key);
      } else {
        rustOnly += 1;
        if (rustWitnesses.length < witnessLimit) rustWitnesses.push(right.key);
      }
      if (order <= 0) ({ value: left } = await native.next());
      if (order >= 0) ({ value: right } = await rust.next());
    }
    requireValid(shared + nativeOnly === left.count && shared + rustOnly === right.count,
      'validated counts disagree with merge');
    const relationship = nativeOnly === 0 && rustOnly === 0 ? 'identical' :
      nativeOnly === 0 ? 'native-c-strict-subset' :
        rustOnly === 0 ? 'rust-cpu-strict-subset' : 'different';
    return {
      schema: RESULT_SCHEMA,
      evidence_scope: 'local-canonical-key-diagnostic',
      source_commit: nativeHeader.source_commit,
      case: nativeHeader.case,
      requested_workers: nativeHeader.requested_workers,
      relationship,
      shared_count: shared,
      witness_limit_per_arm: witnessLimit,
      arms: {
        'native-c': sideSummary(nativeHeader, left, nativeOnly, nativeWitnesses),
        'rust-cpu': sideSummary(rustHeader, right, rustOnly, rustWitnesses),
      },
    };
  } finally {
    await Promise.all([native.return(), rust.return()]);
  }
}

function parseArgs(argv) {
  const allowed = new Set(['--native', '--rust', '--witness-limit']);
  const options = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    const [flag, value] = argv.slice(index, index + 2);
    requireValid(allowed.has(flag) && value !== undefined && !options.has(flag),
      'usage: compare-native-rust-families.mjs --native ABSOLUTE_PATH --rust ABSOLUTE_PATH [--witness-limit N]');
    options.set(flag, value);
  }
  const nativePath = options.get('--native');
  const rustPath = options.get('--rust');
  requireValid(path.isAbsolute(nativePath ?? '') && path.isAbsolute(rustPath ?? ''),
    '--native and --rust must be absolute capture paths');
  const rawLimit = options.get('--witness-limit');
  const witnessLimit = rawLimit === undefined ? DEFAULT_WITNESS_LIMIT : Number(rawLimit);
  requireValid((rawLimit === undefined || /^[0-9]{1,2}$/.test(rawLimit)) &&
    integer(witnessLimit, 0, MAX_WITNESS_LIMIT), 'invalid witness limit');
  return { nativePath, rustPath, witnessLimit };
}

async function* readCaptureFile(filePath) {
  const entry = await lstat(filePath);
  requireValid(entry.isFile(), 'capture must be a regular file');
  requireValid(entry.size <= CAPTURE_LIMIT, 'capture byte limit exceeded');
  yield* createReadStream(filePath, { highWaterMark: 64 * 1024 });
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  try {
    const { nativePath, rustPath, witnessLimit } = parseArgs(process.argv.slice(2));
    const result = await compareNativeRustFamilies(
      readCaptureFile(nativePath), readCaptureFile(rustPath), { witnessLimit },
    );
    process.stdout.write(JSON.stringify(result) + '\n');
  } catch (error) {
    process.stderr.write('Family comparison failed: ' + error.message + '\n');
    process.exitCode = 1;
  }
}
