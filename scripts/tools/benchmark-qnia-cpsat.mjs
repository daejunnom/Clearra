#!/usr/bin/env node
// Diagnostic adapter only. Uses the separately checked-out upstream public API;
// no upstream solver source, legal-board data, or WASM is included in Clearra.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync, spawn } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import { performance } from "node:perf_hooks";

export const QNIA_REVISION = "03b637730c5b541f4f2934be613498fbe65327fd";
export const JSTRIS_MATRIX = "63de33e1d86077c179a38f6311df893ba9abcc13c9b18fa433384f5961eeee91";
const SCHEMA = "clearra-qnia-jstris-cardinality-diagnostic.v1";
const DOMAIN = "clearra-exact-cover-diagnostic-matrix.v1";
const HEX64 = /^[0-9a-f]{16}$/u;
const SHA = /^[0-9a-f]{64}$/u;
const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const sha = (value) => createHash("sha256").update(value).digest("hex");

/** Transpose only after validating the complete raw matrix and both ID bindings. */
export function decodeDiagnosticMatrix(matrix) {
  assert.equal(matrix?.schema, DOMAIN);
  assert.equal(matrix.diagnostic_only, true);
  const { row_count: count, pattern_count: patterns, word_count: words } = matrix;
  assert(Number.isSafeInteger(count) && count > 0 && count <= 10_000);
  assert(Number.isSafeInteger(patterns) && patterns > 0 && patterns <= 100_000);
  assert.equal(words, Math.ceil(patterns / 64));
  assert.equal(matrix.rows?.length, count);
  assert.equal(matrix.queues?.length, patterns);
  assert.equal(new Set(matrix.queues).size, patterns);
  assert(matrix.queues.every((q) => typeof q === "string" && /^[IOTSZJL]{1,64}$/u.test(q)));
  const hash = createHash("sha256").update(`${DOMAIN}\0`);
  for (const n of [count, patterns, words]) hash.update(u64(n));
  const validateWords = (values) => {
    assert.equal(values?.length, words);
    return values.map((s) => { assert(HEX64.test(s)); const n = BigInt(`0x${s}`); hash.update(u64(n)); return n; });
  };
  const required = validateWords(matrix.required_words_hex);
  const full = (1n << 64n) - 1n;
  assert(required.every((n, i) => n === (i === words - 1 && patterns % 64 ? (1n << BigInt(patterns % 64)) - 1n : full)),
    "this fixture compares the complete successful P7 universe, not a silently reduced one");
  const bits = matrix.rows.map((row, i) => {
    assert.equal(row.source_ordinal, i);
    assert(Number.isSafeInteger(row.normalized_ordinal) && row.normalized_ordinal >= 0 && row.normalized_ordinal < count);
    assert(typeof row.candidate_key === "string" && row.candidate_key.length > 0 && row.candidate_key.length <= 65_536);
    const values = validateWords(row.coverage_words_hex);
    assert(values.some((n) => n !== 0n));
    assert(values.every((n, w) => (n & ~required[w]) === 0n));
    return values;
  });
  assert.equal(hash.digest("hex"), matrix.matrix_sha256);
  assert(SHA.test(matrix.matrix_sha256));
  const digest = Buffer.from(matrix.matrix_sha256, "hex");
  const candidate = createHash("sha256").update("clearra-exact-cover-candidate-binding.v1\0").update(digest);
  for (const row of matrix.rows) {
    candidate.update(u64(row.source_ordinal)).update(u64(row.normalized_ordinal));
    candidate.update(u64(Buffer.byteLength(row.candidate_key))).update(row.candidate_key);
  }
  assert.equal(candidate.digest("hex"), matrix.candidate_binding_sha256);
  const queue = createHash("sha256").update("clearra-exact-cover-queue-binding.v1\0").update(digest);
  for (const [i, q] of matrix.queues.entries()) queue.update(u64(i)).update(u64(Buffer.byteLength(q))).update(q);
  assert.equal(queue.digest("hex"), matrix.queue_binding_sha256);
  const normalized = [...matrix.rows].sort((a, b) => a.normalized_ordinal - b.normalized_ordinal);
  normalized.forEach((row, i) => {
    assert.equal(row.normalized_ordinal, i);
    assert(i === 0 || normalized[i - 1].candidate_key < row.candidate_key);
  });
  const coverage = new Map(matrix.queues.map((q) => [q, new Set()]));
  for (const row of normalized) {
    const values = bits[row.source_ordinal];
    for (let i = 0; i < patterns; i += 1) if ((values[i >>> 6] & (1n << BigInt(i % 64))) !== 0n)
      coverage.get(matrix.queues[i]).add(row.candidate_key);
  }
  assert([...coverage.values()].every((set) => set.size > 0));
  return { coverage, normalizedKeys: normalized.map((row) => row.candidate_key) };
}

export function validateExactCardinality(result, kernel, keys, coverage) {
  assert.equal(result.status, "OPTIMAL");
  assert(Number.isSafeInteger(result.count) && result.count > 0);
  assert.equal(result.count, result.proofBound);
  assert.equal(result.selected?.length, result.count);
  const selected = new Set(result.selected);
  assert.equal(selected.size, result.count);
  assert([...selected].every((i) => Number.isSafeInteger(i) && i >= 0 && i < keys.length &&
    (kernel.forced.includes(i) || kernel.solutionIds.includes(i))));
  assert(kernel.forced.every((i) => selected.has(i)));
  assert(kernel.cases.every((row) => row.some((i) => selected.has(kernel.solutionIds[i]))));
  const selectedKeys = new Set([...selected].map((i) => keys[i]));
  assert([...coverage.values()].every((row) => [...row].some((key) => selectedKeys.has(key))));
  return [...selectedKeys].sort();
}

async function measure(values) {
  assert.equal(execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8", windowsHide: true }).trim(), values["source-commit"]);
  const reference = resolve(values.reference);
  assert.equal(execFileSync("git", ["-C", reference, "rev-parse", "HEAD"], { encoding: "utf8", windowsHide: true }).trim(), QNIA_REVISION);
  assert.equal(execFileSync("git", ["-C", reference, "status", "--porcelain", "--untracked-files=no"],
    { encoding: "utf8", windowsHide: true }).trim(), "", "upstream source must not be patched");
  const bytes = await readFile(resolve(values.matrix));
  assert(bytes.length <= 8 * 1024 * 1024);
  const matrix = JSON.parse(bytes);
  assert.equal(matrix.matrix_sha256, JSTRIS_MATRIX);
  assert.match(matrix.source_command, /--board-mask 0x3c0f03c0f .*--patterns P7 .*--rule jstris-180$/u);
  const conversionStart = performance.now();
  const { coverage, normalizedKeys } = decodeDiagnosticMatrix(matrix);
  const conversionMs = performance.now() - conversionStart;
  const fromReference = (path) => import(pathToFileURL(resolve(reference, path)).href);
  const { prepareCoverageMatrix, kernelizeCardinality, primaryKernelStats } = await fromReference("src/highs-cardinality.mjs");
  const { solveORToolsCardinalityKernel, isORToolsSupported, ORTOOLS_PRIMARY_PARAMETERS } = await fromReference("src/ortools-min-cover.mjs");
  assert(isORToolsSupported(), "explicit CP-SAT diagnostic does not silently fall back");
  const numericStart = performance.now();
  const prepared = prepareCoverageMatrix(coverage, null);
  assert.deepEqual(prepared.keys, normalizedKeys);
  const numericMs = performance.now() - numericStart;
  const kernelStart = performance.now();
  const kernel = kernelizeCardinality(prepared.primaryCases, prepared.keys.length);
  const kernelMs = performance.now() - kernelStart;
  const samples = [];
  for (let run = 0; run < 3; run += 1) {
    const start = performance.now();
    const result = await solveORToolsCardinalityKernel(kernel);
    const elapsed = performance.now() - start;
    const keys = validateExactCardinality(result, kernel, prepared.keys, coverage);
    assert.equal(result.count, 25, "known K is a postcondition only, never a solver hint");
    samples.push({ run: run + 1, primary_with_worker_load_cleanup_ms: elapsed,
      cardinality: result.count, proof_bound: result.proofBound, status: result.status,
      selected_keys_sha256: sha(JSON.stringify(keys)), first_canonical_proven: false });
  }
  const report = { schema: SCHEMA, diagnostic_only: true, release_authority: false,
    source_commit: values["source-commit"], reference_revision: QNIA_REVISION, node: process.version,
    source_matrix_sha256: matrix.matrix_sha256, candidate_binding_sha256: matrix.candidate_binding_sha256,
    queue_binding_sha256: matrix.queue_binding_sha256, input_file_sha256: sha(bytes),
    row_order: "normalized key ascending", row_count: matrix.row_count, pattern_count: matrix.pattern_count,
    source_elapsed_ms: matrix.source_elapsed_ms, transpose_and_validation_ms: conversionMs,
    numeric_preparation_ms: numericMs, kernelization_ms: kernelMs, kernel: primaryKernelStats(kernel),
    parameters: ORTOOLS_PRIMARY_PARAMETERS, samples,
    scope: "CP-SAT cardinality proof only; excludes Clearra canonical selection, lazy ties, GUI and cloud cold start",
    product_solver_enabled: false };
  await writeFile(resolve(values.output), `${JSON.stringify(report, null, 2)}\n`, { flag: "wx" });
}

async function main() {
  const { values } = parseArgs({ strict: true, options: {
    reference: { type: "string" }, matrix: { type: "string" }, output: { type: "string" },
    "source-commit": { type: "string" }, child: { type: "boolean", default: false },
  } });
  for (const key of ["reference", "matrix", "output"]) assert(typeof values[key] === "string" && values[key].length > 0);
  assert(/^[0-9a-f]{40}$/u.test(values["source-commit"] ?? ""));
  if (values.child) return measure(values);
  // A diagnostic deadline is not a product timeout. Killing this process also
  // disposes all in-process WASM pthread Workers, without touching user sessions.
  const child = spawn(process.execPath, ["--experimental-wasm-stack-switching", fileURLToPath(import.meta.url),
    ...process.argv.slice(2), "--child"], { stdio: "inherit", windowsHide: true });
  const timer = setTimeout(() => child.kill(), 240_000);
  child.once("error", () => { clearTimeout(timer); process.exitCode = 1; });
  child.once("exit", (code, signal) => { clearTimeout(timer); process.exitCode = signal ? 124 : code ?? 1; });
}
if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try { await main(); } catch { process.stderr.write(`${SCHEMA} failed\n`); process.exitCode = 1; }
}
