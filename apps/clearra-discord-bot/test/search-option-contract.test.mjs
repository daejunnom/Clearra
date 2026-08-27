import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { encodeCtk3 } from "ctk3";

import {
  findSlashCommand,
} from "../src/discord/slash-command-catalog.mjs";
import {
  buildSlashCommandArguments,
} from "../src/discord/slash-command-input.mjs";

const CONTRACT_URL = new URL(
  "../../../tests/fixtures/contracts/search_option_contract.tsv",
  import.meta.url,
);
const COLUMNS = Object.freeze([
  "family",
  "option",
  "kind",
  "valid",
  "invalid",
  "discordDefault",
  "nativeDefault",
  "disposition",
  "discordPath",
  "exposure",
  "lowering",
  "reason",
  "dependencies",
]);

test("every live v2 named option is a typed slash field on every declared route", () => {
  const checkedRoutes = new Set();
  for (const row of liveContractRows().filter(({ disposition }) => disposition === "named")) {
    for (const path of row.discordPath.split("|")) {
      const command = commandAt(path);
      assert.ok(command, `${row.family}.${row.option} route ${path}`);
      const names = new Set(command.registration?.options?.map(({ name }) => name));
      for (const exposure of row.exposure.split("|")) {
        assert.ok(
          names.has(exposure),
          `${row.family}.${row.option} must expose ${exposure} on ${path}`,
        );
      }
      checkedRoutes.add(path);
    }
  }

  for (const path of checkedRoutes) {
    const names = commandAt(path).registration.options.map(({ name }) => name);
    assert.equal(
      names.includes("options"),
      false,
      `${path} must not hide result-affecting v2 fields in packed free text`,
    );
  }

  const archivedBuildRows = contractRows().filter(({ family }) => family === "build");
  assert.equal(archivedBuildRows.length, 18);
  const retiredTypedBuildRows = archivedBuildRows.filter(
    ({ disposition }) => disposition === "named",
  );
  assert.equal(retiredTypedBuildRows.length, 12);
  assert.ok(retiredTypedBuildRows.every(
    ({ discordPath }) => discordPath === "/build cover",
  ));
  const buildV2Names = new Set(
    commandAt("/build cover").registration.options.map(({ name }) => name),
  );
  for (const archived of retiredTypedBuildRows.flatMap(
    ({ exposure }) => exposure.split("|"),
  )) {
    if (["height", "hold", "source-pieces", "kicktable"].includes(archived)) continue;
    assert.equal(buildV2Names.has(archived), false, archived);
  }
});

test("every named or preset v2 ledger row reaches a production lowering token", () => {
  const samples = loweringSamples();
  for (const row of liveContractRows().filter(({ disposition }) => disposition !== "excluded")) {
    const outputs = samples.get(row.family) ?? [];
    const tokens = row.lowering.split("|");
    assert.ok(outputs.length > 0, `${row.family}.${row.option} sample family`);
    assert.ok(
      outputs.some((arguments_) => tokens.some((token) => arguments_.includes(token))),
      `${row.family}.${row.option} must lower through ${row.lowering}`,
    );
  }
});

test("native PC routes preserve objective ownership while chance owns typed unique semantics", () => {
  const path = slashArguments("/pc path", pcBase());
  assert.deepEqual(path.slice(0, 2), ["pc", "path"]);
  assert.equal(path.includes("--objective"), false);
  assert.equal(path.includes("--queue-knowledge"), false);
  assert.equal(row("pc", "objective").nativeDefault, "all");
  assert.equal(row("pc", "objective").discordDefault, "all");

  const chanceCommand = commandAt("/pc chance");
  const chance = slashArguments("/pc chance", pcBase());
  assert.deepEqual(chance.slice(0, 2), ["pc", "chance"]);
  assert.equal(chance.includes("--objective"), false);
  assertValue(chance, "--queue", "IOTSZJL");
  assertValue(chance, "--rule", "srs-plus");
  assert.deepEqual(
    chanceCommand.registration.options.map(({ name }) => name),
    ["next", "field", "lines", "hold", "kicktable"],
  );
  for (const name of [
    "queue-knowledge",
    "spin-profile",
    "preserve-b2b",
    "solution-probabilities",
    "objective",
  ]) {
    assert.throws(
      () => slashArguments("/pc chance", pcBase([{ name, value: "on" }])),
      new RegExp(`unsupported option '${name}'`, "i"),
    );
  }
  const minimals = slashArguments("/pc minimals", pcBase());
  assert.deepEqual(minimals.slice(0, 2), ["pc", "minimals"]);
  assert.equal(minimals.includes("--objective"), false);

  const tiling = slashArguments("/pc tiling", pcBase([
    { name: "hold", value: "disabled" },
  ]));
  assert.deepEqual(tiling.slice(0, 2), ["pc", "tiling"]);
  assert.equal(tiling.includes("--tiling-only"), false);
  assert.equal(tiling.includes("--objective"), false);
  assert.equal(tiling.includes("--rule"), false);

  const failed = slashArguments("/pc failed-queue", pcBase([
    { name: "queue-knowledge", value: "visible-7" },
    { name: "spin-profile", value: "all-spin-plus" },
    { name: "preserve-b2b", value: "on" },
    { name: "failed-count", value: 17 },
  ]));
  assert.deepEqual(failed.slice(0, 2), ["pc", "failed-queue"]);
  assertValue(failed, "--failed-count", "17");
  assertValue(failed, "--queue-knowledge", "visible-7");
  assert.equal(failed.includes("--objective"), false);
  assert.equal(failed.includes("--solution-probabilities"), false);

  assert.throws(
    () => slashArguments("/pc minimals", pcBase([
      { name: "queue-knowledge", value: "visible-7" },
    ])),
    /visible-7.*unavailable.*minimum-cover/i,
  );
  assert.throws(
    () => slashArguments("/pc path", pcBase([
      { name: "spin-profile", value: "all-mini" },
    ])),
    /requires preserve-b2b=on/i,
  );
});

test("canonical score and Build surfaces retain every fieldwise semantic distinction", () => {
  const score = slashArguments("/pc score", pcBase([
    { name: "score-profile", value: "guideline" },
    { name: "spin-profile", value: "all-mini-plus" },
    { name: "initial-b2b", value: 3 },
  ]));
  assert.deepEqual(score.slice(0, 2), ["pc", "score"]);
  assert.equal(score.includes("--objective"), false);
  assert.equal(score.includes("--score"), false);
  assertValue(score, "--score-profile", "guideline");
  assertValue(score, "--initial-b2b", "3");
  for (const name of [
    "queue-knowledge",
    "preserve-b2b",
    "solution-probabilities",
  ]) {
    assert.throws(
      () => slashArguments("/pc score", pcBase([{ name, value: "on" }])),
      new RegExp(`unsupported option '${name}'`, "i"),
    );
  }
  const scoreMinimals = slashArguments("/pc score-minimals", pcBase([
    { name: "score-profile", value: "guideline" },
    { name: "spin-profile", value: "all-mini-plus" },
    { name: "initial-b2b", value: 3 },
  ]));
  assert.deepEqual(scoreMinimals.slice(0, 2), ["pc", "score-minimals"]);
  assertValue(scoreMinimals, "--score-profile", "guideline");
  assertValue(scoreMinimals, "--initial-b2b", "3");
  for (const forbidden of [
    "--objective", "--score", "--solution-probabilities", "--ties",
    "--tie-snapshot", "--tie-cursor",
  ]) assert.equal(scoreMinimals.includes(forbidden), false, forbidden);
  for (const name of ["queue-knowledge", "preserve-b2b", "solution-probabilities"]) {
    assert.throws(
      () => slashArguments("/pc score-minimals", pcBase([{ name, value: "on" }])),
      new RegExp(`unsupported option '${name}'`, "i"),
    );
  }

  const buildV2 = slashArguments("/build cover", [
    { name: "base-mask", value: "0x0" },
    { name: "target-mask", value: "0xf" },
    { name: "height", value: 1 },
    { name: "queue", value: "I" },
  ]);
  assert.deepEqual(buildV2.slice(0, 2), ["build", "cover"]);
  assertValue(buildV2, "--base-mask", "0x0");
  assertValue(buildV2, "--target-mask", "0xf");

  assertValue(buildV2, "--height", "1");
  assertValue(buildV2, "--queue", "I");
  assertValue(buildV2, "--hold", "empty");
  assertValue(buildV2, "--queue-knowledge", "oracle");
  assertValue(buildV2, "--objective", "min-cover");
  assertValue(buildV2, "--rule", "srs-plus");
  assertValue(buildV2, "--backend", "cpu");
  assert.equal(buildV2.includes("--no-backend-fallback"), true);
  for (const forbidden of [
    "--aggregate",
    "--solution-probabilities",
    "--finesse",
    "--max-memory",
    "--ties",
  ]) assert.equal(buildV2.includes(forbidden), false, forbidden);

  const legacyCover = slashArguments("/cover", [
    { name: "base", value: EMPTY_FIELD },
    { name: "target", value: FOUR_CELL_TARGET },
    { name: "next", value: "I" },
    { name: "options", value: "hold=use" },
  ]);
  assert.equal(legacyCover[0], "build-probability");
  assert.equal(legacyCover.slice(0, 2).includes("sfinder"), false);
  assert.equal(legacyCover.includes("--no-mirror"), true);

  const finesse = slashArguments("/build finesse-score", finesseScoreOptions());
  assert.deepEqual(finesse.slice(0, 2), ["finesse", "score"]);
  assert.equal(finesse.includes("--initial-mask"), true);
  assert.equal(finesse.includes("--placements"), true);
  assert.equal(finesse.includes("--base-mask"), false);
  assertValue(finesse, "--source-pieces", "7");
});

test("forward and structural families lower independent named controls", () => {
  const damage = slashArguments("/forward damage", forwardDamageOptions());
  assert.equal(damage[0], "damage");
  assertValue(damage, "--height", "10");
  assert.equal(damage.includes("--no-hold"), true);
  assertValue(damage, "--spin-profile", "all-mini-plus");
  assertValue(damage, "--minimum-damage", "4");
  assertValue(damage, "--initial-combo", "2");
  assertValue(damage, "--initial-b2b", "3");
  assert.equal(damage.includes("--preserve-b2b"), true);

  const defaultDamage = slashArguments("/forward damage", [
    { name: "field", value: EMPTY_FIELD },
    { name: "next", value: "IOTS" },
  ]);
  assertValue(defaultDamage, "--spin-profile", "all-mini-plus");

  const spin = slashArguments("/forward spin", forwardSpinOptions());
  assert.equal(spin[0], "spin-finder");
  assertValue(spin, "--patterns", "[I]!" );
  assertValue(spin, "--spin-profile", "all-spin-plus");
  assertValue(spin, "--lines", "2+");
  assertValue(spin, "--spin-category", "other");
  assert.equal(spin.includes("--preserve-b2b"), true);
  assert.doesNotThrow(() => slashArguments("/forward spin", forwardSpinOptions([
    { name: "spin-profile", value: "all-mini" },
  ])));
  assert.throws(
    () => slashArguments("/forward spin", forwardSpinOptions([
      { name: "spin-profile", value: "t-spins" },
    ])),
    /other requires.*All-Spin or All-Mini/i,
  );

  const ren = slashArguments("/forward ren", [
    { name: "field", value: EMPTY_FIELD },
    { name: "next", value: "IOT" },
    { name: "height", value: 10 },
    { name: "hold", value: "off" },
    { name: "kicktable", value: "no-kick" },
  ]);
  assert.deepEqual(ren.slice(0, 1), ["ren"]);
  assertValue(ren, "--queue", "IOT");
  assertValue(ren, "--height", "10");
  assertValue(ren, "--rule", "no-kick");
  assert.equal(ren.includes("--no-hold"), true);
  assert.equal(ren.includes("--spin-profile"), false);
  assert.throws(
    () => slashArguments("/forward ren", [
      { name: "field", value: EMPTY_FIELD },
      { name: "next", value: "I".repeat(23) },
    ]),
    /at most 22 pieces/i,
  );

  const structure = slashArguments("/spin-structure search", spinStructureOptions());
  assert.deepEqual(structure.slice(0, 2), ["spin-structure", "search"]);
  assertValue(structure, "--pieces", "IOTS");
  assertValue(structure, "--height", "8");
  assertValue(structure, "--fill-bottom", "1");
  assertValue(structure, "--fill-top", "5");
  assertValue(structure, "--max-placements", "4");
  assertValue(structure, "--minimality", "minimum-piece-count");
  assert.ok(findSlashCommand("spin-structure").subcommands.search);
});

function loweringSamples() {
  const pcPath = slashArguments("/pc path", pcBase([
    { name: "hold", value: "I" },
    { name: "kicktable", value: "no-kick" },
    { name: "spin-profile", value: "all-spin-plus" },
    { name: "preserve-b2b", value: "on" },
  ]));
  const pcScore = slashArguments("/pc score", pcBase([
    { name: "score-profile", value: "guideline" },
    { name: "initial-b2b", value: 1 },
  ]));
  const pcScoreMinimals = slashArguments("/pc score-minimals", pcBase());
  const pcTiling = slashArguments("/pc tiling", pcBase());
  const pcFailed = slashArguments("/pc failed-queue", pcBase([
    { name: "failed-count", value: 17 },
  ]));

  const legacyCover = slashArguments("/cover", [
    { name: "base", value: EMPTY_FIELD },
    { name: "target", value: FOUR_CELL_TARGET },
    { name: "next", value: "I" },
  ]);

  const setup = slashArguments("/setup joint", setupOptions());
  const setupBuild = slashArguments("/setup build", [
    { name: "remaining", value: "IOT" },
  ]);
  const setupPc = slashArguments("/setup pc", [
    { name: "remaining", value: "IOT" },
  ]);

  return new Map([
    ["pc", [pcPath, pcScore, pcScoreMinimals, pcTiling, pcFailed]],
    ["setup", [setup, setupBuild, setupPc]],
    ["forward-damage", [slashArguments("/forward damage", forwardDamageOptions())]],
    ["forward-spin", [slashArguments("/forward spin", forwardSpinOptions())]],
    ["spin-structure", [slashArguments("/spin-structure search", spinStructureOptions())]],
    ["finesse-score", [slashArguments("/build finesse-score", finesseScoreOptions())]],
    ["sequence-dependencies", [
      slashArguments("/utility sequence-dependencies", sequenceDependenciesOptions()),
    ]],
    ["sequence", [
      slashArguments("/utility sequence", sequenceOptions()),
    ]],
  ]);
}

function pcBase(additional = []) {
  return mergeOptions([
    { name: "field", value: EMPTY_FIELD },
    { name: "next", value: "IOTSZJL" },
    { name: "lines", value: 4 },
  ], additional);
}

function buildCoverOptions(additional = []) {
  return mergeOptions([
    { name: "base", value: EMPTY_FIELD },
    { name: "target", value: FOUR_CELL_TARGET },
    { name: "next", value: "IOTSZJL" },
    { name: "height", value: 12 },
    { name: "hold", value: "I" },
    { name: "source-pieces", value: 17 },
    { name: "kicktable", value: "no-kick" },
    { name: "aggregation", value: "spin" },
    { name: "spin-profile", value: "all-spin-plus" },
    { name: "preserve-b2b", value: "on" },
    { name: "solution-probabilities", value: "on" },
    { name: "finesse", value: "inputs" },
    { name: "finesse-knowledge", value: "visible-7" },
    { name: "mirror", value: "exclude" },
  ], additional);
}

function setupOptions(additional = []) {
  return mergeOptions([
    { name: "remaining", value: "OTS" },
    { name: "mode", value: "qb" },
    { name: "qb", value: "I" },
    { name: "queue-knowledge", value: "visible-7" },
    { name: "next-cycle-remaining", value: "IOTSZJL" },
    { name: "post-cycle-borrow", value: "on" },
    { name: "setup-length", value: "longer" },
    { name: "max-setup-pieces", value: 9 },
    { name: "kicktable", value: "no-kick" },
  ], additional);
}

function forwardDamageOptions(additional = []) {
  return mergeOptions([
    { name: "field", value: EMPTY_FIELD },
    { name: "next", value: "IOTS" },
    { name: "height", value: 10 },
    { name: "hold", value: "off" },
    { name: "kicktable", value: "no-kick" },
    { name: "spin-profile", value: "all-mini-plus" },
    { name: "damage-mode", value: "at-least" },
    { name: "minimum-damage", value: 4 },
    { name: "initial-combo", value: 2 },
    { name: "initial-b2b", value: 3 },
    { name: "preserve-b2b", value: "on" },
  ], additional);
}

function forwardSpinOptions(additional = []) {
  return mergeOptions([
    { name: "field", value: EMPTY_FIELD },
    { name: "next", value: "[I]!" },
    { name: "height", value: 9 },
    { name: "hold", value: "off" },
    { name: "kicktable", value: "no-kick" },
    { name: "spin-profile", value: "all-spin-plus" },
    { name: "lines", value: "2+" },
    { name: "spin-category", value: "other" },
    { name: "initial-combo", value: 2 },
    { name: "initial-b2b", value: 3 },
    { name: "preserve-b2b", value: "on" },
  ], additional);
}

function spinStructureOptions(additional = []) {
  return mergeOptions([
    { name: "pieces", value: "IOTS" },
    { name: "field", value: EMPTY_FIELD },
    { name: "height", value: 8 },
    { name: "lines", value: "2+" },
    { name: "spin-profile", value: "all-mini-plus" },
    { name: "kicktable", value: "no-kick" },
    { name: "fill-bottom", value: 1 },
    { name: "fill-top", value: 5 },
    { name: "max-placements", value: 4 },
    { name: "minimality", value: "minimum-piece-count" },
  ], additional);
}

function finesseScoreOptions() {
  return [
    { name: "document", value: encodeCtk3({
      width: 10,
      pages: [{
        height: 1,
        cells: ["G", ...Array(9).fill(null)],
        operation: { piece: "T", rotation: "spawn", x: 4, y: 1 },
      }],
    }) },
    { name: "next", value: "[T]!" },
    { name: "hold", value: "I" },
    { name: "knowledge", value: "visible-7" },
    { name: "source-pieces", value: 7 },
    { name: "kicktable", value: "no-kick" },
  ];
}

function sequenceDependenciesOptions() {
  return [
    { name: "document", value: encodeCtk3({
      width: 10,
      pages: [{
        height: 0,
        cells: [],
        operation: { piece: "O", rotation: "spawn", x: 0, y: 0 },
      }],
    }) },
    { name: "rule-profile", value: "srs-x" },
    { name: "kick-profile", value: "no-kick" },
    { name: "timeout-seconds", value: 17 },
  ];
}

function sequenceOptions() {
  return [
    { name: "document", value: encodeCtk3({
      width: 10,
      pages: [{
        height: 0,
        cells: [],
        operation: { piece: "O", rotation: "spawn", x: 0, y: 0 },
      }],
    }) },
    { name: "rule-profile", value: "srs-x" },
    { name: "kick-profile", value: "no-kick" },
    { name: "timeout-seconds", value: 17 },
  ];
}

function mergeOptions(base, additional) {
  const replacements = new Map(additional.map((option) => [option.name, option]));
  return [
    ...base.map((option) => replacements.get(option.name) ?? option),
    ...additional.filter(({ name }) => !base.some((option) => option.name === name)),
  ];
}

function slashArguments(path, options) {
  return buildSlashCommandArguments(commandAt(path), options);
}

function commandAt(path) {
  const [root, subcommand, ...extra] = path.replace(/^\//u, "").trim().split(/\s+/u);
  if (extra.length > 0) return null;
  const command = findSlashCommand(root);
  if (root === "spin-structure" && subcommand === undefined) {
    return command?.subcommands?.search ?? null;
  }
  return subcommand ? command?.subcommands?.[subcommand] ?? null : command;
}

function assertValue(arguments_, flag, expected) {
  const index = arguments_.indexOf(flag);
  assert.notEqual(index, -1, `${flag} in ${arguments_.join(" ")}`);
  assert.equal(arguments_[index + 1], expected, flag);
}

function row(family, option) {
  const value = contractRows().find((candidate) =>
    candidate.family === family && candidate.option === option
  );
  assert.ok(value, `${family}.${option}`);
  return value;
}

function contractRows() {
  return readFileSync(CONTRACT_URL, "utf8")
    .split(/\r?\n/u)
    .filter((line) => line && !line.startsWith("#"))
    .map((line) => {
      const values = line.split("\t");
      assert.equal(values.length, COLUMNS.length, line);
      return Object.fromEntries(COLUMNS.map((name, index) => [name, values[index]]));
    });
}

function liveContractRows() {
  // The frozen Build rows describe the retired fieldwise /build cover preset.
  // They remain immutable evidence, while live /build is governed by the
  // exhaustive Build v2 surface tests. The promoted pc.path v2 authority also
  // fixes objective=all and full-oracle knowledge internally, so the old
  // fieldwise score/B2B/probability knobs are historical evidence only.
  const retiredPcPathOptions = new Set([
    "queue-knowledge",
    "objective",
    "solution-probabilities",
  ]);
  return contractRows().filter(({ family, option }) =>
    family !== "build" && !(family === "pc" && retiredPcPathOptions.has(option))
  );
}

const EMPTY_FIELD = "__________";
const FOUR_CELL_TARGET = "XXXX______";
