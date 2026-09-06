import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { decodeDiagnosticMatrix, validateExactCardinality, QNIA_REVISION } from "./benchmark-qnia-cpsat.mjs";

function fixture() {
  const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
  const m = { schema: "clearra-exact-cover-diagnostic-matrix.v1", diagnostic_only: true,
    row_count: 3, pattern_count: 4, word_count: 1, required_words_hex: ["000000000000000f"],
    queues: ["I", "O", "T", "S"], rows: [
      { source_ordinal: 0, normalized_ordinal: 2, candidate_key: "c", coverage_words_hex: ["0000000000000009"] },
      { source_ordinal: 1, normalized_ordinal: 0, candidate_key: "a", coverage_words_hex: ["0000000000000003"] },
      { source_ordinal: 2, normalized_ordinal: 1, candidate_key: "b", coverage_words_hex: ["000000000000000c"] },
    ] };
  const hash = createHash("sha256").update(`${m.schema}\0`);
  for (const n of [3, 4, 1, 15, 9, 3, 12]) hash.update(u64(n));
  m.matrix_sha256 = hash.digest("hex");
  const digest = Buffer.from(m.matrix_sha256, "hex");
  const candidate = createHash("sha256").update("clearra-exact-cover-candidate-binding.v1\0").update(digest);
  for (const row of m.rows) candidate.update(u64(row.source_ordinal)).update(u64(row.normalized_ordinal))
    .update(u64(Buffer.byteLength(row.candidate_key))).update(row.candidate_key);
  m.candidate_binding_sha256 = candidate.digest("hex");
  const queue = createHash("sha256").update("clearra-exact-cover-queue-binding.v1\0").update(digest);
  m.queues.forEach((q, i) => queue.update(u64(i)).update(u64(q.length)).update(q));
  m.queue_binding_sha256 = queue.digest("hex");
  return m;
}

test("reference adapter preserves raw coverage while binding normalized canonical IDs and queues", () => {
  const { coverage, normalizedKeys } = decodeDiagnosticMatrix(fixture());
  assert.deepEqual(normalizedKeys, ["a", "b", "c"]);
  assert.deepEqual([...coverage].map(([q, ids]) => [q, [...ids]]), [
    ["I", ["a", "c"]], ["O", ["a"]], ["T", ["b"]], ["S", ["b", "c"]],
  ]);
  assert.deepEqual(validateExactCardinality({ status: "OPTIMAL", count: 2, proofBound: 2, selected: [0, 1] },
    { forced: [], solutionIds: [0, 1, 2], cases: [[0, 2], [0], [1], [1, 2]] }, normalizedKeys, coverage), ["a", "b"]);
});

test("matrix transposition rejects content drift, narrowed universe, duplicate IDs and invalid encoding", () => {
  for (const mutate of [
    (m) => { m.rows[1].coverage_words_hex[0] = "0000000000000002"; },
    (m) => { m.rows[0].candidate_key = "forged"; },
    (m) => { m.rows[1].normalized_ordinal = 2; },
    (m) => { m.queues[0] = "Z"; },
    (m) => { m.queues[0] = m.queues[1]; },
    (m) => { m.required_words_hex[0] = "0000000000000007"; },
    (m) => { m.rows[0].coverage_words_hex[0] = "0000000000000010"; },
    (m) => { m.word_count = 0; }, (m) => { m.rows[0].source_ordinal = 1; },
    (m) => { m.rows[0].coverage_words_hex[0] = "f"; },
  ]) { const m = fixture(); mutate(m); assert.throws(() => decodeDiagnosticMatrix(m)); }
});

test("CP-SAT OPTIMAL must prove K and cover the original matrix, not just a reduced kernel", () => {
  const { coverage, normalizedKeys } = decodeDiagnosticMatrix(fixture());
  const kernel = { forced: [], solutionIds: [0, 1, 2], cases: [[0, 2]] };
  const valid = { status: "OPTIMAL", count: 2, proofBound: 2, selected: [0, 1] };
  for (const patch of [{ status: "FEASIBLE" }, { proofBound: 1 }, { selected: [0, 0] },
    { selected: [0, 3] }, { selected: [0, 2] }, { selected: [0, 1.5] }])
    assert.throws(() => validateExactCardinality({ ...valid, ...patch }, kernel, normalizedKeys, coverage));
  assert.throws(() => validateExactCardinality(valid, { ...kernel, forced: [2] }, normalizedKeys, coverage));
});

test("reference benchmark is bounded, source pinned, original API only and never claims first canonical", async () => {
  const source = await readFile(new URL("./benchmark-qnia-cpsat.mjs", import.meta.url), "utf8");
  assert.match(source, /240_000/u);
  assert.match(source, /first_canonical_proven: false/u);
  assert.match(source, /product_solver_enabled: false/u);
  assert.match(source, /solveORToolsCardinalityKernel\(kernel\)/u);
  assert.match(source, /known K is a postcondition only/u);
  assert.ok(source.includes(QNIA_REVISION));
  assert.doesNotMatch(source, /addHint|hint=|Primary:|registerHooks|solver.*\.wasm|git clone/u);
});
