// SRP: exhaustively bind one already-downloaded canonical Jstris generation's
// graph/index bytes and bounded observed KAT vectors. This does not prove that
// the upstream graph omitted no PC-capable edge and does not compare complete
// PC-search result families with Clearra's offline solver. Its receipt is
// therefore deliberately incompatible with a target-qualification receipt.
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { isAbsolute, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { openBenchmarkDataset } from '../../benchmark/pc4-local-dataset.mjs';

export const PC4_STRUCTURAL_KAT_AUDIT_RECEIPT_SCHEMA =
  'clearra.pc4.structural-kat-audit.v1';
export const PC4_STRUCTURAL_KAT_AUDIT_SCOPE =
  'complete-artifact-structure-and-bounded-observed-kat-only';
export const PC4_MISSING_PC_SEARCH_SEMANTIC_PROOF_IDENTITIES = Object.freeze({
  outgoing_edge_completeness_identity: null,
  offline_exact_parity_identity: null,
});

const TARGET_QUALIFICATION_SCHEMA = 'clearra.pc4.exact-target-qualification.v1';
const READER_CONTRACT = 'hydra-jstris-180-complete-graph-v1';
const PIECES = Object.freeze(['I', 'J', 'L', 'O', 'S', 'T', 'Z']);
const SHA256_IDENTITY = /^sha256:[0-9a-f]{64}$/u;
const REVISION = /^[0-9a-f]{40}$/u;
const FULL_FIELD = 2 ** 40 - 1;
const HEADER_BYTES = 16;

export async function auditPc4LocalPcSearchGeneration({
  generation,
  profile = 'jstris-180',
  rootKat,
  nonemptyKat,
  readArtifact,
  chunkRecords = 1024,
  maxReadBytes = 65_536,
} = {}) {
  const context = validateInputs({
    generation,
    profile,
    rootKat,
    nonemptyKat,
    readArtifact,
    chunkRecords,
    maxReadBytes,
  });
  const { slot, artifacts, fieldCount, kat } = context;
  const readExact = createExactReader(readArtifact, maxReadBytes);
  const [fieldHeader, offsetHeader] = await Promise.all([
    readExact(artifacts.fields, 0, HEADER_BYTES),
    readExact(artifacts.offsets, 0, HEADER_BYTES),
  ]);
  validateHeader(fieldHeader, 'FHIDIDX1', fieldCount);
  validateHeader(offsetHeader, 'GOFFIDX1', fieldCount);

  const hashes = {
    fields: createHash('sha256'),
    offsets: createHash('sha256'),
    graph: createHash('sha256'),
  };
  hashes.fields.update(fieldHeader);
  hashes.offsets.update(offsetHeader);

  let previousFieldHash = -1;
  let previousGraphOffset = 0;
  let edgeCount = 0;
  let maximumRecordBytes = 0;
  let terminalEdgeCount = null;
  const seenKatFields = new Set();
  const seenKatSources = new Set();
  const seenMissHashes = new Set();

  for (let firstId = 0; firstId < fieldCount; firstId += chunkRecords) {
    const records = Math.min(chunkRecords, fieldCount - firstId);
    const [fieldBytes, offsetBytes] = await Promise.all([
      readExact(artifacts.fields, HEADER_BYTES + firstId * 8, records * 8),
      readExact(artifacts.offsets, HEADER_BYTES + firstId * 4, (records + 1) * 4),
    ]);
    hashes.fields.update(fieldBytes);
    // The final offset of this block is the first offset of the next block.
    // Hash it exactly once, with the final sentinel included by the last block.
    hashes.offsets.update(offsetBytes.subarray(0, records * 4));
    if (firstId + records === fieldCount) {
      hashes.offsets.update(offsetBytes.subarray(records * 4));
    }

    const offsetView = new DataView(
      offsetBytes.buffer,
      offsetBytes.byteOffset,
      offsetBytes.byteLength,
    );
    const blockStart = offsetView.getUint32(0, true);
    const blockEnd = offsetView.getUint32(records * 4, true);
    if (blockStart !== previousGraphOffset || blockEnd <= blockStart ||
        blockEnd > artifacts.graph.byte_length) {
      fail('pc4_structural_audit_offset_chain_invalid');
    }
    const graphBytes = await readExact(
      artifacts.graph,
      blockStart,
      blockEnd - blockStart,
    );
    hashes.graph.update(graphBytes);

    for (let local = 0; local < records; local++) {
      const id = firstId + local;
      const fieldOffset = local * 8;
      const fieldHash = little(fieldBytes.subarray(fieldOffset, fieldOffset + 5));
      const encodedId = little(fieldBytes.subarray(fieldOffset + 5, fieldOffset + 8));
      if (encodedId !== id || fieldHash <= previousFieldHash) {
        fail('pc4_structural_audit_field_index_invalid');
      }
      previousFieldHash = fieldHash;
      if (kat.expectedFields.has(id)) {
        if (kat.expectedFields.get(id) !== fieldHash) {
          fail('pc4_structural_audit_kat_field_mismatch');
        }
        seenKatFields.add(id);
      }
      if (kat.missingFieldHashes.has(fieldHash)) seenMissHashes.add(fieldHash);

      const start = offsetView.getUint32(local * 4, true);
      const end = offsetView.getUint32((local + 1) * 4, true);
      if (start !== previousGraphOffset || end <= start ||
          end > artifacts.graph.byte_length || end - start > 16_384) {
        fail('pc4_structural_audit_offset_chain_invalid');
      }
      const relativeStart = start - blockStart;
      const relativeEnd = end - blockStart;
      const expectedTargets = kat.expectedAdjacency.get(id);
      const decoded = validateGraphRecord(
        graphBytes.subarray(relativeStart, relativeEnd),
        fieldHash,
        slot.target_width,
        fieldCount,
        expectedTargets,
      );
      edgeCount += decoded.edgeCount;
      maximumRecordBytes = Math.max(maximumRecordBytes, end - start);
      if (expectedTargets) seenKatSources.add(id);
      if (id === fieldCount - 1) terminalEdgeCount = decoded.edgeCount;
      previousGraphOffset = end;
    }
  }

  if (previousGraphOffset !== artifacts.graph.byte_length ||
      previousFieldHash !== FULL_FIELD || terminalEdgeCount !== 0) {
    fail('pc4_structural_audit_terminal_invalid');
  }
  if (seenKatFields.size !== kat.expectedFields.size ||
      seenKatSources.size !== kat.expectedAdjacency.size || seenMissHashes.size !== 0) {
    fail('pc4_structural_audit_kat_coverage_invalid');
  }

  const actualIdentities = {
    fields: `sha256:${hashes.fields.digest('hex')}`,
    offsets: `sha256:${hashes.offsets.digest('hex')}`,
    graph: `sha256:${hashes.graph.digest('hex')}`,
  };
  for (const role of ['fields', 'offsets', 'graph']) {
    if (actualIdentities[role] !== artifacts[role].content_identity) {
      fail('pc4_structural_audit_artifact_identity_mismatch');
    }
  }

  const boundedKatPayload = Object.freeze({
    repository: generation.repository,
    revision: generation.revision,
    profile,
    graph_identity: artifacts.graph.content_identity,
    sources: kat.normalizedSources,
    missing_fields: kat.normalizedMisses,
  });
  const boundedKatIdentity = digestCanonical(boundedKatPayload);
  const receiptCore = {
    schema: PC4_STRUCTURAL_KAT_AUDIT_RECEIPT_SCHEMA,
    authority: 'non-target-qualification-evidence',
    target_qualification_schema: TARGET_QUALIFICATION_SCHEMA,
    evidence_scope: PC4_STRUCTURAL_KAT_AUDIT_SCOPE,
    repository: generation.repository,
    revision: generation.revision,
    profile,
    reader_contract: slot.reader_contract,
    target_lines: 4,
    artifacts: Object.freeze({
      fields: publicArtifact(artifacts.fields),
      offsets: publicArtifact(artifacts.offsets),
      graph: publicArtifact(artifacts.graph),
    }),
    complete_structural_scan: Object.freeze({
      field_count: fieldCount,
      field_index_records_checked: fieldCount,
      graph_offsets_checked: fieldCount + 1,
      graph_records_checked: fieldCount,
      graph_edges_checked: edgeCount,
      maximum_record_bytes: maximumRecordBytes,
      terminal_id: fieldCount - 1,
      terminal_hash: FULL_FIELD,
      terminal_outgoing_edges: terminalEdgeCount,
      every_artifact_byte_hashed: true,
      every_record_source_bound_to_field_index: true,
      every_graph_target_in_domain: true,
    }),
    bounded_known_answers: Object.freeze({
      identity: boundedKatIdentity,
      scope: 'tracked-root-and-nonempty-observation-fixtures',
      source_records_checked: kat.expectedAdjacency.size,
      field_identities_checked: kat.expectedFields.size,
      target_references_checked: kat.targetReferenceCount,
      indexed_misses_checked: kat.missingFieldHashes.size,
    }),
    qualification_status: 'not-qualified',
    pc_search_target_receipt: null,
    missing_semantic_proof_identities:
      PC4_MISSING_PC_SEARCH_SEMANTIC_PROOF_IDENTITIES,
  };
  return deepFreeze({
    ...receiptCore,
    audit_identity: digestCanonical(receiptCore),
  });
}

function validateInputs({ generation, profile, rootKat, nonemptyKat,
  readArtifact, chunkRecords, maxReadBytes }) {
  if (profile !== 'jstris-180' || typeof readArtifact !== 'function' ||
      !Number.isSafeInteger(chunkRecords) || chunkRecords < 1 || chunkRecords > 16_384 ||
      !Number.isSafeInteger(maxReadBytes) || maxReadBytes < 4096 || maxReadBytes > 1024 * 1024 ||
      generation?.schema !== 'clearra.pc4.host-generation.v1' ||
      typeof generation.repository !== 'string' || !REVISION.test(generation.revision ?? '') ||
      !Array.isArray(generation.profiles)) {
    fail('pc4_structural_audit_input_invalid');
  }
  const slots = generation.profiles.filter(candidate => candidate?.profile === profile);
  if (slots.length !== 1) fail('pc4_structural_audit_profile_invalid');
  const slot = slots[0];
  if (slot.status !== 'ready' || slot.reader_contract !== READER_CONTRACT ||
      slot.target_width !== 3 || !Number.isSafeInteger(slot.field_count) ||
      slot.field_count < 2 || slot.field_count > 2 ** 24 ||
      slot.terminal_id !== slot.field_count - 1) {
    fail('pc4_structural_audit_profile_invalid');
  }
  const artifacts = slot.artifacts;
  for (const [role, expectedPath] of Object.entries({
    fields: 'field_hash_to_id.v1.bin',
    offsets: 'graph_offsets.u32.bin',
    graph: 'graph.bin',
  })) {
    const artifact = artifacts?.[role];
    if (artifact?.path !== expectedPath || !Number.isSafeInteger(artifact.byte_length) ||
        artifact.byte_length < 1 || !SHA256_IDENTITY.test(artifact.content_identity ?? '')) {
      fail('pc4_structural_audit_artifact_invalid');
    }
  }
  if (artifacts.fields.byte_length !== HEADER_BYTES + slot.field_count * 8 ||
      artifacts.offsets.byte_length !== HEADER_BYTES + (slot.field_count + 1) * 4) {
    fail('pc4_structural_audit_artifact_invalid');
  }
  return {
    slot,
    artifacts,
    fieldCount: slot.field_count,
    kat: normalizeKat(generation, rootKat, nonemptyKat, slot.field_count),
  };
}

function normalizeKat(generation, rootKat, nonemptyKat, fieldCount) {
  if (rootKat === null || typeof rootKat !== 'object' || Array.isArray(rootKat) ||
      nonemptyKat === null || typeof nonemptyKat !== 'object' || Array.isArray(nonemptyKat) ||
      rootKat.dataset !== generation.repository || rootKat.revision !== generation.revision ||
      rootKat.graph !== 'graph.bin' || rootKat.source_hash !== 0 ||
      nonemptyKat.revision !== generation.revision || !Array.isArray(nonemptyKat.cases)) {
    fail('pc4_structural_audit_kat_identity_invalid');
  }
  const expectedFields = new Map();
  const expectedAdjacency = new Map();
  const missingFieldHashes = new Set();
  const normalizedSources = [];
  const normalizedMisses = [];
  let targetReferenceCount = 0;

  const addField = (id, hash) => {
    if (!Number.isSafeInteger(id) || id < 0 || id >= fieldCount ||
        !Number.isSafeInteger(hash) || hash < 0 || hash > FULL_FIELD) {
      fail('pc4_structural_audit_kat_identity_invalid');
    }
    if (expectedFields.has(id) && expectedFields.get(id) !== hash) {
      fail('pc4_structural_audit_kat_identity_invalid');
    }
    expectedFields.set(id, hash);
  };
  const addSource = (name, id, hash, pieces) => {
    if (typeof name !== 'string' || name.length < 1 || expectedAdjacency.has(id)) {
      fail('pc4_structural_audit_kat_identity_invalid');
    }
    addField(id, hash);
    const normalizedPieces = {};
    const exactKeys = Object.keys(pieces ?? {});
    if (exactKeys.length !== PIECES.length || PIECES.some(piece => !exactKeys.includes(piece))) {
      fail('pc4_structural_audit_kat_identity_invalid');
    }
    for (const piece of PIECES) {
      if (!Array.isArray(pieces[piece])) fail('pc4_structural_audit_kat_identity_invalid');
      normalizedPieces[piece] = pieces[piece].map(target => {
        if (target === null || typeof target !== 'object' || Array.isArray(target)) {
          fail('pc4_structural_audit_kat_identity_invalid');
        }
        addField(target.id, target.hash);
        targetReferenceCount++;
        return Object.freeze({ id: target.id, hash: target.hash });
      });
    }
    const normalized = Object.freeze({ name, id, hash, pieces: deepFreeze(normalizedPieces) });
    normalizedSources.push(normalized);
    expectedAdjacency.set(id, normalized.pieces);
  };

  addSource('empty-root', 0, rootKat.source_hash, rootKat.pieces);
  const caseNames = new Set();
  for (const entry of nonemptyKat.cases) {
    if (entry === null || typeof entry !== 'object' || Array.isArray(entry) ||
        typeof entry.name !== 'string' || caseNames.has(entry.name) ||
        !Number.isSafeInteger(entry.source_hash) || entry.source_hash < 0 ||
        entry.source_hash > FULL_FIELD) {
      fail('pc4_structural_audit_kat_identity_invalid');
    }
    caseNames.add(entry.name);
    if (entry.source_id === null) {
      if (Object.keys(entry.pieces ?? {}).length !== 0 || missingFieldHashes.has(entry.source_hash)) {
        fail('pc4_structural_audit_kat_identity_invalid');
      }
      missingFieldHashes.add(entry.source_hash);
      normalizedMisses.push(Object.freeze({ name: entry.name, hash: entry.source_hash }));
    } else {
      addSource(entry.name, entry.source_id, entry.source_hash, entry.pieces);
    }
  }
  return Object.freeze({
    expectedFields,
    expectedAdjacency,
    missingFieldHashes,
    normalizedSources: Object.freeze(normalizedSources),
    normalizedMisses: Object.freeze(normalizedMisses),
    targetReferenceCount,
  });
}

function validateHeader(bytes, magic, fieldCount) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (new TextDecoder().decode(bytes.subarray(0, 8)) !== magic ||
      view.getUint32(8, true) !== 1 || view.getUint32(12, true) !== fieldCount) {
    fail('pc4_structural_audit_header_invalid');
  }
}

function validateGraphRecord(bytes, expectedHash, targetWidth, fieldCount, expectedTargets) {
  if (bytes.length < 12) fail('pc4_structural_audit_graph_record_invalid');
  let cursor = 0;
  let sourceHash = 0;
  for (let index = 0; index < 5; index++) sourceHash = sourceHash * 256 + bytes[cursor++];
  if (sourceHash !== expectedHash) fail('pc4_structural_audit_graph_source_mismatch');
  let edgeCount = 0;
  for (const piece of PIECES) {
    if (cursor >= bytes.length) fail('pc4_structural_audit_graph_record_invalid');
    const degree = bytes[cursor++];
    const targets = [];
    for (let edge = 0; edge < degree; edge++) {
      if (cursor + targetWidth > bytes.length) fail('pc4_structural_audit_graph_record_invalid');
      const target = little(bytes.subarray(cursor, cursor + targetWidth));
      if (target >= fieldCount) fail('pc4_structural_audit_graph_target_invalid');
      targets.push(target);
      cursor += targetWidth;
      edgeCount++;
    }
    if (expectedTargets) {
      const expected = expectedTargets[piece].map(target => target.id);
      if (targets.length !== expected.length || targets.some((target, index) => target !== expected[index])) {
        fail('pc4_structural_audit_kat_adjacency_mismatch');
      }
    }
  }
  if (cursor !== bytes.length) fail('pc4_structural_audit_graph_record_invalid');
  return Object.freeze({ edgeCount });
}

function createExactReader(readArtifact, maxReadBytes) {
  return async (artifact, offset, length) => {
    if (!Number.isSafeInteger(offset) || !Number.isSafeInteger(length) || offset < 0 ||
        length < 1 || offset > artifact.byte_length - length) {
      fail('pc4_structural_audit_read_invalid');
    }
    const output = new Uint8Array(length);
    for (let written = 0; written < length;) {
      const size = Math.min(maxReadBytes, length - written);
      const bytes = await readArtifact(artifact, offset + written, size);
      if (!(bytes instanceof Uint8Array) || bytes.length !== size) {
        fail('pc4_structural_audit_read_invalid');
      }
      output.set(bytes, written);
      written += size;
    }
    return output;
  };
}

function publicArtifact(artifact) {
  return Object.freeze({
    path: artifact.path,
    byte_length: artifact.byte_length,
    content_identity: artifact.content_identity,
  });
}

function little(bytes) {
  let value = 0;
  for (let index = bytes.length - 1; index >= 0; index--) value = value * 256 + bytes[index];
  return value;
}

function digestCanonical(value) {
  return `sha256:${createHash('sha256').update(canonicalJson(value)).digest('hex')}`;
}

function canonicalJson(value) {
  if (value === null || typeof value !== 'object') return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  return `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(',')}}`;
}

function deepFreeze(value) {
  Object.freeze(value);
  for (const child of Object.values(value)) {
    if (child !== null && typeof child === 'object' && !Object.isFrozen(child)) deepFreeze(child);
  }
  return value;
}

function fail(code) {
  throw Object.assign(new Error(code), { code });
}

async function main() {
  const { values } = parseArgs({
    options: {
      directory: { type: 'string' },
      profile: { type: 'string', default: 'jstris-180' },
      'root-kat': { type: 'string' },
      'nonempty-kat': { type: 'string' },
    },
  });
  if (!isAbsolute(values.directory ?? '') || !isAbsolute(values['root-kat'] ?? '') ||
      !isAbsolute(values['nonempty-kat'] ?? '')) {
    throw new Error('Supply absolute --directory, --root-kat, and --nonempty-kat paths');
  }
  const dataset = await openBenchmarkDataset(values.directory, values.profile);
  try {
    const [rootKat, nonemptyKat] = await Promise.all([
      readJson(values['root-kat']),
      readJson(values['nonempty-kat']),
    ]);
    const receipt = await auditPc4LocalPcSearchGeneration({
      generation: dataset.generation,
      profile: values.profile,
      rootKat,
      nonemptyKat,
      readArtifact: (artifact, offset, length) => dataset.read(artifact, offset, length),
    });
    process.stdout.write(`${JSON.stringify(receipt, null, 2)}\n`);
  } finally {
    await dataset.close();
  }
}

async function readJson(path) {
  const bytes = await readFile(resolve(path));
  if (bytes.length > 2 * 1024 * 1024) fail('pc4_structural_audit_kat_identity_invalid');
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
  } catch {
    fail('pc4_structural_audit_kat_identity_invalid');
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  await main();
}
