import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream, readFileSync } from "node:fs";
import { stat } from "node:fs/promises";
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

test("tiny real native Chance retains exact serial probability with eight public workers", {
  skip: !process.env.CLEARRA_PARITY_EXECUTABLE, timeout: 25_000,
}, async (t) => {
  const executable = process.env.CLEARRA_PARITY_EXECUTABLE;
  const sourceCommit = process.env.CLEARRA_PARITY_SOURCE_COMMIT;
  assert.match(sourceCommit ?? "", /^[0-9a-f]{40}$/u);
  assert.match(process.env.CLEARRA_PARITY_EXECUTABLE_SHA256 ?? "", /^[0-9a-f]{64}$/u);
  const metadata = await stat(executable);
  assert.ok(metadata.isFile() && metadata.size > 0 && metadata.size < 256 * 1024 ** 2);
  const digest = createHash("sha256");
  for await (const bytes of createReadStream(executable)) digest.update(bytes);
  assert.equal(digest.digest("hex"), process.env.CLEARRA_PARITY_EXECUTABLE_SHA256);

  // Both fixtures succeeded serially in v0.9.2 but lost complete coverage-row
  // evidence on its native parallel route. The second also guards a proper
  // probability below one, rather than accepting any successful envelope.
  const fixtures = [
    { name: "empty 2L", field: "grid:__________", lines: 2, pieces: 5,
      source: { value: "IJLOO", flag: "--queue", length: 5 },
      boardMask: 0n, patternCount: 1, probability: 1 },
    { name: "bounded 4L", field: "grid:######____/######____", lines: 4, pieces: 7,
      source: { value: "IOOOOOO;OOOOOOO", flag: "--patterns", length: 7 },
      boardMask: 0xfc3fn, patternCount: 2, probability: 0.5 },
  ];
  for (const fixture of fixtures) await t.test(fixture.name, (caseTest) => {
    const input = { ...fixture, hold: "disabled", rule: "srs-plus", spin: null };
    const plan = buildSlashCommandArgumentPlan(chance(), inputOptions(input));
    assert.equal(plan.automaticPcTargets, false);
    assert.equal(plan.argumentSets.length, 1);
    assertPhysicalChance(plan.argumentSets[0], input);
    const results = [];
    for (const workers of [1, 8]) {
      const argv = prepareClearraArguments(plan.argumentSets[0], {
        ...execution, workers, logicalProcessors: workers,
      });
      assert.equal(optionValue(argv, "--auto-workers"), String(workers));
      assert.equal(argv.includes("--use-all-cpu-threads"), true);
      const run = spawnSync(executable, argv, { encoding: "utf8", timeout: 5000,
        maxBuffer: 1024 * 1024, env: { ...process.env, CLEARRA_EXPECTED_VCPUS: String(workers) },
        stdio: ["ignore", "pipe", "pipe"] });
      assert.ifError(run.error);
      assert.equal(run.signal, null);
      const native = JSON.parse(run.stdout);
      assert.equal(run.status, 0, `${fixture.name}/${workers}: ${JSON.stringify(native.error)}`);
      assert.equal(native.kind, "pc-probability.v2");
      assert.equal(native.contract?.command?.kind, native.kind);
      assert.equal(native.runtime_identity?.source_commit, sourceCommit);
      assert.equal(native.runtime_identity?.engine_build_id, sourceCommit);
      const summary = native.summary;
      assert.equal(summary.workers_requested, "auto");
      assert.equal(summary.workers_used, workers);
      assert.equal(String(summary.cpu_parallel_execution), String(workers > 1));
      assert.equal(summary.objective, "unique");
      assert.equal(summary.search_output_policy, "coverage-summary");
      assert.equal(summary.queue_knowledge, "oracle");
      assert.equal(summary.source_sequence_length, fixture.source.length);
      assert.equal(summary.total_possible_pattern_count, String(fixture.patternCount));
      assert.equal(summary.materialized_pattern_count, fixture.patternCount);
      assert.equal(summary.covered_pattern_count, 1);
      assert.equal(summary.coverage_probability, fixture.probability);
      assert.equal(summary.materialized_probability_mass, 1);
      for (const field of ["probability_calculated", "probability_complete", "supply_probability_complete",
        "resource_probability_complete", "count_complete", "objective_complete"]) {
        assert.equal(summary[field], true, `${fixture.name}/${workers}: ${field}`);
      }
      assert.equal(native.resource_report?.truncated, false);
      assert.equal(native.resource_report?.probability_complete, true);
      results.push(native);
      caseTest.diagnostic(`workers=${workers} covered=1/${fixture.patternCount} probability=${summary.coverage_probability}`);
    }
    assert.deepEqual(results[1].runtime_identity, results[0].runtime_identity);
    assert.equal(results[1].summary.coverage_probability, results[0].summary.coverage_probability);
    assert.equal(results[1].summary.covered_pattern_count, results[0].summary.covered_pattern_count);
  });
  t.diagnostic(`source=${sourceCommit} cli_sha256=${process.env.CLEARRA_PARITY_EXECUTABLE_SHA256} tiny_cases=2 workers=1,8`);
});
