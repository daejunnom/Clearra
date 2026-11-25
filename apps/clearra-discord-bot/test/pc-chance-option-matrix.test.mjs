import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { prepareClearraArguments } from "../src/clearra/command.mjs";
import { parseClearraTextRequest } from "../src/clearra/text-command.mjs";
import { findSlashCommand } from "../src/discord/slash-command-catalog.mjs";
import { buildSlashCommandArgumentPlan } from "../src/discord/slash-command-input.mjs";

const rules = ["srs-plus", "srs", "srs-x", "jstris-180", "no-kick"];
const holds = ["disabled", "empty", "I", "O", "T", "S", "Z", "J", "L"];
const spins = [null, "t-spins", "t-spins-plus", "all-spin", "all-spin-plus",
  "all-mini", "all-mini-plus"];
const sources = [
  { value: "P7P4", flag: "--patterns", length: 11 },
  { value: "P7P7", flag: "--patterns", length: 14 },
  { value: "IOTSZLJIOTSZLJ", flag: "--queue", length: 14 },
];
const execution = { workers: 8, logicalProcessors: 8, useAllLogicalProcessors: true,
  outputFormat: "json", includeSolutionData: true };
const chance = () => findSlashCommand("pc").subcommands.chance;

function optionValue(argv, flag) {
  const index = argv.indexOf(flag);
  return index < 0 ? undefined : argv[index + 1];
}

function inputOptions({ field = "grid:__________", source, hold, rule, spin, lines }) {
  return [{ name: "field", value: field }, { name: "next", value: source.value },
    { name: "hold", value: hold }, { name: "kicktable", value: rule },
    { name: "preserve-b2b", value: spin === null ? "off" : "on" },
    ...(spin === null ? [] : [{ name: "spin-profile", value: spin }]),
    ...(lines === undefined ? [] : [{ name: "lines", value: lines }])];
}

function textInput({ field = "grid:__________", source, hold, rule, spin, lines }) {
  return ["$pc chance", "--field", field, "--next", source.value, "--hold", hold,
    "--kicktable", rule, spin === null ? "--no-preserve-b2b" : "--preserve-b2b",
    ...(spin === null ? [] : ["--spin-profile", spin]),
    ...(lines === undefined ? [] : ["--lines", String(lines)])].join(" ");
}

function assertPhysicalChance(argv, { source, hold, rule, spin, lines, boardMask = 0n, pieces }) {
  assert.deepEqual(argv.slice(0, 2), ["pc", "chance"]);
  assert.equal(optionValue(argv, source.flag), source.value, "the complete original source is retained");
  assert.equal(argv.includes(source.flag === "--queue" ? "--patterns" : "--queue"), false);
  assert.equal(optionValue(argv, "--lines"), String(lines));
  assert.equal(optionValue(argv, "--height"), String(lines));
  assert.equal(optionValue(argv, "--pieces"), String(pieces ?? lines * 10 / 4));
  assert.equal(BigInt(optionValue(argv, "--board-mask")), boardMask);
  assert.equal(optionValue(argv, "--rule"), rule);
  assert.equal(argv.includes("--no-hold"), hold === "disabled");
  assert.equal(optionValue(argv, "--hold"), hold === "disabled" ? undefined : hold);
  assert.equal(argv.includes("--preserve-b2b"), spin !== null);
  assert.equal(optionValue(argv, "--spin-profile"), spin ?? undefined);
  for (const forbidden of ["--objective", "--count", "--score", "--solution-probabilities",
    "--queue-knowledge", "--source-pieces", "--retained-traces"]) {
    assert.equal(argv.includes(forbidden), false, `typed Chance must not acquire ${forbidden}`);
  }
}

test("typed Chance exposes all built-in rules, Hold states and meaningful B2B spin profiles", () => {
  const fields = new Map(chance().registration.options.map((option) => [option.name, option]));
  assert.deepEqual(fields.get("kicktable").choices.map((choice) => choice.value), rules);
  assert.deepEqual(new Set(fields.get("hold").choices.map((choice) => choice.value)), new Set(holds));
  assert.deepEqual(new Set(fields.get("spin-profile").choices.map((choice) => choice.value)),
    new Set(spins.filter((spin) => spin !== null)));
  assert.ok(fields.has("preserve-b2b"));
  for (const inactive of ["objective", "queue-knowledge", "solution-probabilities", "initial-b2b"]) {
    assert.equal(fields.has(inactive), false, inactive);
  }
});

test("945 typed Chance option combinations keep slash/text source and eight-worker parity without solving", () => {
  let checked = 0;
  for (const source of sources) for (const hold of holds) for (const rule of rules) for (const spin of spins) {
    const input = { source, hold, rule, spin, lines: 4 };
    const plan = buildSlashCommandArgumentPlan(chance(), inputOptions(input));
    assert.equal(plan.automaticPcTargets, false);
    assert.equal(plan.argumentSets.length, 1);
    assertPhysicalChance(plan.argumentSets[0], input);
    const prepared = prepareClearraArguments(plan.argumentSets[0], execution);
    assert.equal(optionValue(prepared, "--auto-workers"), "8");
    assert.equal(prepared.includes("--use-all-cpu-threads"), true);
    const text = parseClearraTextRequest(textInput(input), "$", execution);
    assert.equal(text.command.capabilityId, "pc.chance");
    assert.equal(text.command.timeoutClass, "pc_reverse");
    assert.equal(text.automaticPcTargets, false);
    assert.deepEqual(text.arguments_, prepared);
    checked += 1;
  }
  assert.equal(checked, 945);
});

test("automatic Chance target planning retains long queues and the independent initial Hold piece", () => {
  let checked = 0;
  for (const source of sources) for (const hold of holds) for (const spin of spins) {
    const input = { source, hold, spin, rule: "srs-plus" };
    const plan = buildSlashCommandArgumentPlan(chance(), inputOptions(input));
    const supplied = source.length + Number(!["disabled", "empty"].includes(hold));
    const expectedLines = Array.from({ length: Math.floor(supplied / 5) }, (_, index) => 2 * (index + 1));
    assert.equal(plan.automaticPcTargets, true);
    assert.deepEqual(plan.argumentSets.map((argv) => Number(optionValue(argv, "--lines"))), expectedLines);
    for (const [index, argv] of plan.argumentSets.entries()) {
      assertPhysicalChance(argv, { ...input, lines: expectedLines[index] });
    }
    const text = parseClearraTextRequest(textInput(input), "$", execution);
    assert.equal(text.automaticPcTargets, true);
    assert.deepEqual(text.argumentSets,
      plan.argumentSets.map((argv) => prepareClearraArguments(argv, execution)));
    checked += 1;
  }
  assert.equal(checked, 189);
});

test("extended Chance option combinations retain every board word and exact placement count", () => {
  const rows = readFileSync(new URL("../../../tests/fixtures/contracts/extended_pc_surface_input.v1.tsv", import.meta.url), "utf8")
    .split(/\r?\n/u).filter((line) => line && !line.startsWith("#"));
  let checked = 0;
  for (const row of rows) {
    const [, height, hex, pieces, queue] = row.split("\t");
    const boardMask = BigInt(`0x${hex}`);
    const lines = Number(height);
    const fieldRows = [];
    for (let y = lines - 1; y >= 0; y -= 1) {
      let cells = "";
      for (let x = 0; x < 10; x += 1) cells += (boardMask & (1n << BigInt(y * 10 + x))) === 0n ? "_" : "X";
      fieldRows.push(cells);
    }
    const field = `grid:${fieldRows.join("/")}`;
    for (const hold of holds) for (const rule of rules) for (const spin of spins) {
      const input = { field, source: { value: queue, flag: "--queue", length: queue.length },
        hold, rule, spin, lines };
      const plan = buildSlashCommandArgumentPlan(chance(), inputOptions(input));
      assert.equal(plan.argumentSets.length, 1);
      assertPhysicalChance(plan.argumentSets[0], { ...input, boardMask, pieces: Number(pieces) });
      checked += 1;
    }
  }
  assert.equal(checked, rows.length * 315);
});

test("Chance never silently ignores an inactive spin profile or changes the requested probability product", () => {
  const input = { source: sources[0], hold: "empty", rule: "srs-plus", spin: null, lines: 4 };
  for (const spin of spins.filter((value) => value !== null)) {
    assert.throws(() => buildSlashCommandArgumentPlan(chance(), [
      ...inputOptions(input), { name: "spin-profile", value: spin },
    ]), /spin-profile requires preserve-b2b=on/u);
  }
  for (const [name, value] of [["queue-knowledge", "visible-7"], ["solution-probabilities", "on"],
    ["objective", "all"], ["initial-b2b", 1]]) {
    assert.throws(() => buildSlashCommandArgumentPlan(chance(), [...inputOptions(input), { name, value }]),
      /unsupported option/u);
  }
});
