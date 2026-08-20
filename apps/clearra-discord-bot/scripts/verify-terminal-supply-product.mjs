import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import { decodeCtk3 } from "ctk3";

import { buildCtk3Result } from "../src/clearra/ctk3-result.mjs";

const ARTIFACT_SCHEMA = "clearra.solution-data.v1";
const TERMINAL_SUPPLY_P0_INITIAL_MASK = 0x1c0701c07n;
export const TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT = 18;
export const TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH =
  "cts1:8a7fc484d9b49994";
const TERMINAL_SUPPLY_P0_COMMAND = Object.freeze([
  "--format",
  "json",
  "--include-solution-data",
  "pc-scenario",
  "--field",
  "0x1c0701c07",
  "--visible-height",
  "4",
  "--queue",
  "STOILJZ",
  "--max-pieces",
  "7",
  "--exact-pieces",
  "7",
  "--count-policy",
  "count-unique",
  "--backend",
  "cpu",
  "--workers",
  "1",
]);
const CANONICAL_PIECE_ORDER = Object.freeze(["I", "O", "T", "S", "Z", "J", "L"]);
const PIECES = new Set(CANONICAL_PIECE_ORDER);
const U64_MASK = (1n << 64n) - 1n;
const FNV_OFFSET = 0xcbf29ce484222325n;
const FNV_PRIME = 0x100000001b3n;
const REPOSITORY_ROOT = fileURLToPath(new URL("../../../", import.meta.url));

function main() {
  const { values } = parseArgs({
    options: {
      clearra: { type: "string" },
    },
    strict: true,
  });
  assert.ok(values.clearra, "--clearra must name the built release-facing CLI");

  const response = runTerminalSupplyCli(values.clearra);
  const solutionKeys = response.contract.artifacts.solution_keys;

  const result = buildCtk3Result(response);
  assert.ok(result, "Discord CTK3 adapter must materialize the complete result");
  assert.equal(result.complete, true);
  assert.equal(result.pageCount, TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT);

  const decoded = decodeCtk3(result.source);
  const renderedKeys = decoded.pages.map(keyFromDecodedPage).sort();
  assert.deepEqual(renderedKeys, solutionKeys);
  assert.equal(
    normalizedSetHash(renderedKeys),
    TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH,
  );

  console.log(
    "[discord-terminal-supply-product] passed" +
      ` | solutions=${renderedKeys.length}` +
      ` | hash=${TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH}` +
      " | projection=projected-terminal-lookahead",
  );
}

export function runTerminalSupplyCli(clearraPath) {
  const execution = spawnSync(clearraPath, TERMINAL_SUPPLY_P0_COMMAND, {
    cwd: REPOSITORY_ROOT,
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
    windowsHide: true,
  });
  if (execution.error) throw execution.error;
  assert.equal(
    execution.signal,
    null,
    `terminal-supply CLI was terminated by ${execution.signal}`,
  );
  assert.equal(
    execution.status,
    0,
    `terminal-supply CLI failed with exit ${execution.status}\n${execution.stderr}`,
  );

  let response;
  try {
    response = JSON.parse(execution.stdout);
  } catch (cause) {
    throw new Error(
      `terminal-supply CLI did not return one JSON document\n${execution.stdout}`,
      { cause },
    );
  }
  assertTerminalSupplyProjection(response);

  const solutionKeys = response.contract.artifacts.solution_keys;
  assertCanonicalSolutionSet(solutionKeys);
  assert.equal(
    normalizedSetHash(solutionKeys),
    TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH,
  );
  return response;
}

function assertTerminalSupplyProjection(response) {
  assert.equal(response?.schema_version, 2);
  assert.equal(response?.kind, "pc-scenario");

  const summary = response?.summary;
  assert.equal(summary?.unique_solution_count, TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT);
  assert.equal(
    summary?.normalized_unique_solution_count,
    TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT,
  );
  assert.equal(
    summary?.actual_normalized_unique_solution_count,
    TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT,
  );
  assert.equal(summary?.solution_count_calculated, true);
  assert.equal(summary?.solution_set_materialized, true);
  assert.equal(
    summary?.solution_keys_materialized_count,
    TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT,
  );
  assert.equal(summary?.solution_keys_complete, true);
  assert.equal(summary?.count_complete, true);
  assert.equal(summary?.supply_window_resolution, "projected-terminal-lookahead");
  assert.equal(summary?.projects_unplaced_lookahead, true);
  assert.equal(summary?.projects_standard_bag_lookahead, false);
  assert.equal(summary?.source_sequence_length, 7);
  assert.equal(summary?.total_possible_pattern_count, "1");
  assert.equal(
    summary?.normalized_solution_set_hash,
    TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH,
  );
  assert.equal(
    summary?.actual_normalized_solution_set_hash,
    TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH,
  );

  assert.equal(response?.contract?.solution_data?.requested, true);
  assert.equal(response?.contract?.solution_data?.status, "complete");
  assert.equal(response?.contract?.solution_data?.reason, null);
  assert.equal(response?.contract?.artifacts?.schema_version, ARTIFACT_SCHEMA);
}

function assertCanonicalSolutionSet(keys) {
  assert.ok(Array.isArray(keys), "complete solution data must contain solution keys");
  assert.equal(keys.length, TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT);
  assert.ok(keys.every((key) => typeof key === "string"));
  assert.ok(keys.every((key) =>
    key.startsWith("ctk1|initial=00000001c0701c07|placements=")));
  assert.ok(keys.every((key) => keyFromCanonicalKey(key) === key));
  assert.ok(keys.every((key, index) => index === 0 || keys[index - 1] < key));
}

function keyFromCanonicalKey(key) {
  const match = /^ctk1\|initial=([0-9a-f]{16})\|placements=(.*)$/.exec(key);
  assert.ok(match, `non-canonical solution key: ${key}`);
  const masks = new Map(CANONICAL_PIECE_ORDER.map((piece) => [piece, []]));
  if (match[2]) {
    for (const encoded of match[2].split(",")) {
      const placement = /^([IOTSZJL]):([0-9a-f]{16})$/.exec(encoded);
      assert.ok(placement, `non-canonical placement: ${encoded}`);
      masks.get(placement[1]).push(BigInt(`0x${placement[2]}`));
    }
  }
  return canonicalKey(BigInt(`0x${match[1]}`), masks);
}

export function keyFromDecodedPage(page) {
  assert.ok(Array.isArray(page?.cells), "decoded CTK3 page must contain cells");
  let initialMask = 0n;
  const masks = new Map(CANONICAL_PIECE_ORDER.map((piece) => [piece, []]));
  const pieceMasks = new Map(CANONICAL_PIECE_ORDER.map((piece) => [piece, 0n]));

  for (let index = 0; index < page.cells.length; index += 1) {
    const cell = page.cells[index];
    if (cell === null) continue;
    const bit = 1n << BigInt(index);
    if (cell === "G") {
      initialMask |= bit;
      continue;
    }
    assert.ok(PIECES.has(cell), `unexpected decoded CTK3 cell ${cell}`);
    pieceMasks.set(cell, pieceMasks.get(cell) | bit);
  }

  assert.equal(initialMask, TERMINAL_SUPPLY_P0_INITIAL_MASK);
  for (const piece of CANONICAL_PIECE_ORDER) {
    const mask = pieceMasks.get(piece);
    assert.equal(popcount(mask), 4, `expected one decoded ${piece} placement`);
    masks.get(piece).push(mask);
  }
  return canonicalKey(initialMask, masks);
}

function canonicalKey(initialMask, masks) {
  const placements = [];
  let occupied = initialMask;
  for (const piece of CANONICAL_PIECE_ORDER) {
    const pieceMasks = [...masks.get(piece)].sort(compareBigInt);
    for (const mask of pieceMasks) {
      assert.notEqual(mask, 0n);
      assert.equal(popcount(mask), 4);
      assert.equal(occupied & mask, 0n);
      occupied |= mask;
      placements.push(`${piece}:${hex64(mask)}`);
    }
  }
  return `ctk1|initial=${hex64(initialMask)}|placements=${placements.join(",")}`;
}

export function normalizedSetHash(keys) {
  let hash = FNV_OFFSET;
  for (const key of [...keys].sort()) {
    for (const byte of new TextEncoder().encode(`${key}\0`)) {
      hash ^= BigInt(byte);
      hash = (hash * FNV_PRIME) & U64_MASK;
    }
  }
  return `cts1:${hash.toString(16).padStart(16, "0")}`;
}

function hex64(value) {
  return value.toString(16).padStart(16, "0");
}

function compareBigInt(left, right) {
  return left < right ? -1 : left > right ? 1 : 0;
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

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  main();
}
