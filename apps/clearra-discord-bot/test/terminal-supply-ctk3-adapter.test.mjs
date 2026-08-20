import assert from "node:assert/strict";
import test from "node:test";

import { decodeCtk3 } from "ctk3";

import { buildCtk3Result } from "../src/clearra/ctk3-result.mjs";

const ARTIFACT_SCHEMA = "clearra.solution-data.v1";
const TERMINAL_SUPPLY_P0_INITIAL_MASK = 0x1c0701c07n;
const TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT = 18;
const U64_MASK = (1n << 64n) - 1n;
const FNV_OFFSET = 0xcbf29ce484222325n;
const FNV_PRIME = 0x100000001b3n;

function hex64(value) {
  return value.toString(16).padStart(16, "0");
}

function terminalSupplySyntheticKeys() {
  const freeCells = [];
  for (let bit = 0; bit < 40; bit += 1) {
    if ((TERMINAL_SUPPLY_P0_INITIAL_MASK & (1n << BigInt(bit))) === 0n) {
      freeCells.push(bit);
    }
  }
  assert.ok(freeCells.length >= TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT + 3);

  return Array.from(
    { length: TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT },
    (_, index) => {
      const placementMask = freeCells
        .slice(index, index + 4)
        .reduce((mask, bit) => mask | (1n << BigInt(bit)), 0n);
      return `ctk1|initial=${hex64(TERMINAL_SUPPLY_P0_INITIAL_MASK)}` +
        `|placements=I:${hex64(placementMask)}`;
    },
  ).sort();
}

function normalizedSetHash(keys) {
  let hash = FNV_OFFSET;
  for (const key of [...keys].sort()) {
    for (const byte of new TextEncoder().encode(`${key}\0`)) {
      hash ^= BigInt(byte);
      hash = (hash * FNV_PRIME) & U64_MASK;
    }
  }
  return `cts1:${hash.toString(16).padStart(16, "0")}`;
}

function keyFromSyntheticPage(page) {
  let initialMask = 0n;
  let placementMask = 0n;
  for (let index = 0; index < page.cells.length; index += 1) {
    const bit = 1n << BigInt(index);
    if (page.cells[index] === "G") initialMask |= bit;
    if (page.cells[index] === "I") placementMask |= bit;
  }
  assert.equal(initialMask, TERMINAL_SUPPLY_P0_INITIAL_MASK);
  assert.equal(popcount(placementMask), 4);
  return `ctk1|initial=${hex64(initialMask)}|placements=I:${hex64(placementMask)}`;
}

function popcount(value) {
  let remaining = value;
  let count = 0;
  while (remaining !== 0n) {
    remaining &= remaining - 1n;
    count += 1;
  }
  return count;
}

test("terminal-supply-shaped complete sets keep count, keys, and hash through Discord CTK3", () => {
  // These pages are deliberately synthetic adapter data. The authoritative P0
  // solution identities come only from the Rust production search fixture.
  const solutionKeys = terminalSupplySyntheticKeys();
  const expectedHash = normalizedSetHash(solutionKeys);
  const result = buildCtk3Result({
    schema_version: 2,
    kind: "pc-scenario",
    summary: {
      unique_solution_count: TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT,
      normalized_unique_solution_count: TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT,
      solution_count_calculated: true,
      solution_set_materialized: true,
      solution_keys_materialized_count: TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT,
      solution_keys_complete: true,
      normalized_solution_set_hash: expectedHash,
      actual_normalized_solution_set_hash: expectedHash,
      count_complete: true,
    },
    contract: {
      artifacts: {
        schema_version: ARTIFACT_SCHEMA,
        solution_keys: solutionKeys,
      },
    },
  });

  assert.ok(result);
  assert.equal(result.complete, true);
  assert.equal(result.pageCount, TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT);
  const renderedKeys = decodeCtk3(result.source).pages
    .map(keyFromSyntheticPage)
    .sort();
  assert.deepEqual(renderedKeys, solutionKeys);
  assert.equal(normalizedSetHash(renderedKeys), expectedHash);
});
