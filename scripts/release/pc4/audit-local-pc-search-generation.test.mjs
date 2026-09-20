import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import {
  auditPc4LocalPcSearchGeneration,
  PC4_MISSING_PC_SEARCH_SEMANTIC_PROOF_IDENTITIES,
  PC4_STRUCTURAL_KAT_AUDIT_RECEIPT_SCHEMA,
} from './audit-local-pc-search-generation.mjs';
import { qualifyPc4UpstreamGeneration } from './qualify-upstream-generation.mjs';

const REVISION = 'a'.repeat(40);
const FULL = 2 ** 40 - 1;
const PIECES = ['I', 'J', 'L', 'O', 'S', 'T', 'Z'];

function le(value, width) {
  return Uint8Array.from({ length: width }, (_, index) => Math.floor(value / 256 ** index) % 256);
}

function be40(value) {
  return Uint8Array.from({ length: 5 }, (_, index) => Math.floor(value / 256 ** (4 - index)) % 256);
}

function concat(...parts) {
  const output = new Uint8Array(parts.reduce((total, part) => total + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    output.set(part, offset);
    offset += part.length;
  }
  return output;
}

function header(magic, count) {
  return concat(new TextEncoder().encode(magic), le(1, 4), le(count, 4));
}

function graphRecord(hash, pieces) {
  return concat(be40(hash), ...PIECES.map(piece => {
    const targets = pieces[piece] ?? [];
    return concat(Uint8Array.of(targets.length), ...targets.map(target => le(target, 3)));
  }));
}

function identity(bytes) {
  return `sha256:${createHash('sha256').update(bytes).digest('hex')}`;
}

function allPieces(overrides = {}) {
  return Object.fromEntries(PIECES.map(piece => [piece, overrides[piece] ?? []]));
}

function fixture() {
  const fields = [0, 15, 30, FULL];
  const records = [
    graphRecord(0, { I: [1] }),
    graphRecord(15, { O: [2] }),
    graphRecord(30, { T: [3] }),
    graphRecord(FULL, {}),
  ];
  const offsets = [0];
  for (const record of records) offsets.push(offsets.at(-1) + record.length);
  const files = new Map([
    ['field_hash_to_id.v1.bin', concat(
      header('FHIDIDX1', fields.length),
      ...fields.map((hash, id) => concat(le(hash, 5), le(id, 3))),
    )],
    ['graph_offsets.u32.bin', concat(
      header('GOFFIDX1', fields.length),
      ...offsets.map(offset => le(offset, 4)),
    )],
    ['graph.bin', concat(...records)],
  ]);
  const artifacts = {
    fields: artifact(files, 'field_hash_to_id.v1.bin'),
    offsets: artifact(files, 'graph_offsets.u32.bin'),
    graph: artifact(files, 'graph.bin'),
  };
  const generation = {
    schema: 'clearra.pc4.host-generation.v1',
    repository: 'example/pc4',
    revision: REVISION,
    profiles: [{
      profile: 'jstris-180',
      status: 'ready',
      reader_contract: 'hydra-jstris-180-complete-graph-v1',
      field_count: fields.length,
      target_width: 3,
      terminal_id: fields.length - 1,
      artifacts,
    }],
  };
  const rootKat = {
    dataset: generation.repository,
    revision: REVISION,
    graph: 'graph.bin',
    source_hash: 0,
    pieces: allPieces({ I: [{ id: 1, hash: 15 }] }),
  };
  const nonemptyKat = {
    revision: REVISION,
    cases: [
      { name: 'first', source_id: 1, source_hash: 15,
        pieces: allPieces({ O: [{ id: 2, hash: 30 }] }) },
      { name: 'second', source_id: 2, source_hash: 30,
        pieces: allPieces({ T: [{ id: 3, hash: FULL }] }) },
      { name: 'index-miss', source_id: null, source_hash: 16, pieces: {} },
    ],
  };
  return {
    files,
    generation,
    rootKat,
    nonemptyKat,
    async readArtifact(input, offset, length) {
      const bytes = files.get(input.path);
      assert.ok(bytes);
      return bytes.slice(offset, offset + length);
    },
  };
}

function artifact(files, path) {
  const bytes = files.get(path);
  return { path, byte_length: bytes.length, content_identity: identity(bytes) };
}

async function audit(value = fixture()) {
  return auditPc4LocalPcSearchGeneration({
    generation: value.generation,
    rootKat: value.rootKat,
    nonemptyKat: value.nonemptyKat,
    readArtifact: value.readArtifact,
    chunkRecords: 2,
    maxReadBytes: 4096,
  });
}

test('complete structural and bounded KAT scan remains explicitly non-authoritative', async () => {
  const receipt = await audit();
  assert.equal(receipt.schema, PC4_STRUCTURAL_KAT_AUDIT_RECEIPT_SCHEMA);
  assert.equal(receipt.authority, 'non-target-qualification-evidence');
  assert.equal(receipt.qualification_status, 'not-qualified');
  assert.equal(receipt.pc_search_target_receipt, null);
  assert.deepEqual(
    receipt.missing_semantic_proof_identities,
    PC4_MISSING_PC_SEARCH_SEMANTIC_PROOF_IDENTITIES,
  );
  assert.deepEqual(receipt.complete_structural_scan, {
    field_count: 4,
    field_index_records_checked: 4,
    graph_offsets_checked: 5,
    graph_records_checked: 4,
    graph_edges_checked: 3,
    maximum_record_bytes: 15,
    terminal_id: 3,
    terminal_hash: FULL,
    terminal_outgoing_edges: 0,
    every_artifact_byte_hashed: true,
    every_record_source_bound_to_field_index: true,
    every_graph_target_in_domain: true,
  });
  assert.equal(receipt.bounded_known_answers.source_records_checked, 3);
  assert.equal(receipt.bounded_known_answers.target_references_checked, 3);
  assert.equal(receipt.bounded_known_answers.indexed_misses_checked, 1);
  assert.match(receipt.bounded_known_answers.identity, /^sha256:[0-9a-f]{64}$/u);
  assert.match(receipt.audit_identity, /^sha256:[0-9a-f]{64}$/u);
  assert.ok(Object.isFrozen(receipt));
});

test('an audit receipt cannot be consumed as exact target qualification', async () => {
  const receipt = await audit();
  await assert.rejects(
    qualifyPc4UpstreamGeneration({}, {
      discover: async () => ({
        repository: receipt.repository,
        resolved_revision: receipt.revision,
        candidates: [],
      }),
      targetQualificationReceipts: [receipt],
    }),
    error => error.code === 'pc4_online_target_qualification_invalid',
  );
});

test('the exhaustive scan rejects an out-of-domain graph target before hashing authority', async () => {
  const value = fixture();
  const graph = value.files.get('graph.bin');
  graph[6] = 9;
  value.generation.profiles[0].artifacts.graph = artifact(value.files, 'graph.bin');
  await assert.rejects(audit(value), { code: 'pc4_structural_audit_graph_target_invalid' });
});

test('bounded KAT adjacency and fixture revision drift fail closed', async () => {
  const adjacency = fixture();
  adjacency.rootKat.pieces.I[0] = { id: 2, hash: 30 };
  await assert.rejects(audit(adjacency), { code: 'pc4_structural_audit_kat_adjacency_mismatch' });

  const revision = fixture();
  revision.nonemptyKat.revision = 'b'.repeat(40);
  await assert.rejects(audit(revision), { code: 'pc4_structural_audit_kat_identity_invalid' });
});

test('artifact content drift cannot retain a structural receipt', async () => {
  const value = fixture();
  const original = value.generation.profiles[0].artifacts.graph.content_identity;
  value.generation.profiles[0].artifacts.graph.content_identity = `sha256:${'f'.repeat(64)}`;
  assert.notEqual(value.generation.profiles[0].artifacts.graph.content_identity, original);
  await assert.rejects(audit(value), { code: 'pc4_structural_audit_artifact_identity_mismatch' });
});

test('the preserved real-generation receipt retains the non-qualification invariant', async () => {
  const receipt = JSON.parse(await readFile(
    new URL('../../../docs/research/pc4-jstris-180-structural-kat-audit-2026-09-20.json', import.meta.url),
    'utf8',
  ));
  assert.equal(receipt.schema, PC4_STRUCTURAL_KAT_AUDIT_RECEIPT_SCHEMA);
  assert.equal(receipt.authority, 'non-target-qualification-evidence');
  assert.equal(receipt.qualification_status, 'not-qualified');
  assert.equal(receipt.pc_search_target_receipt, null);
  assert.deepEqual(receipt.missing_semantic_proof_identities, {
    outgoing_edge_completeness_identity: null,
    offline_exact_parity_identity: null,
  });
  assert.equal(receipt.complete_structural_scan.graph_records_checked, 15_185_706);
  assert.equal(receipt.complete_structural_scan.graph_edges_checked, 109_562_993);
});
