import assert from "node:assert/strict";
import test from "node:test";

import { encodeCtk3 } from "ctk3";

import { findSlashCommand } from "../src/discord/slash-command-catalog.mjs";
import { buildSlashCommandArguments } from "../src/discord/slash-command-input.mjs";

const EMPTY = "__________";
const FINESSE_DOCUMENT = encodeCtk3({
  width: 10,
  pages: [{
    height: 0,
    cells: [],
    operation: { piece: "I", rotation: "spawn", x: 4, y: 0 },
  }],
});

test("damage alias accepts representative named options in canonical order", () => {
  const settings = [
    ["hold", "off"],
    ["spin-profile", "all-mini-plus"],
    ["initial-combo", 2],
    ["initial-b2b", 3],
    ["preserve-b2b", "on"],
  ];
  assert.deepEqual(assertSinglesAndPairs(damageArguments, settings), {
    semanticCases: 15,
    parserExecutions: 30,
  });
  const command = findSlashCommand("forward").subcommands.damage;
  const constrained = [...settings, ["damage-mode", "at-least"], ["minimum-damage", 4]];
  assert.deepEqual(damageArguments(constrained), damageArguments(constrained, null, command));
  assert.throws(() => damageArguments([["minimum-damage", 4]]), /damage-mode=at-least/);
});

test("spin-structure named options accept every representative single and legal pair", () => {
  const settings = [
    ["fill-bottom", "1"],
    ["fill-top", "4"],
    ["max-placements", "2"],
    ["minimality", "minimum-piece-count"],
  ];
  assert.deepEqual(assertSinglesAndPairs(spinStructureArguments, settings), {
    semanticCases: 10,
    parserExecutions: 20,
  });
  assert.deepEqual(spinStructureArguments(settings).slice(-8), [
    "--fill-bottom", "1",
    "--fill-top", "4",
    "--max-placements", "2",
    "--minimality", "minimum-piece-count",
  ]);
});

test("canonical setup named options cover every legal pair and stable dependency errors", () => {
  const settings = [
    ["mode", "oracle"],
    ["qb", "OS"],
    ["post-cycle-borrow", "on"],
  ];
  assert.deepEqual(assertCaseMatrix(setupArguments, [
    ...settings.map((setting) => [setting]),
    [settings[0], settings[2]],
    [settings[1], settings[2]],
    [["mode", "qb"], settings[1]],
  ]), {
    semanticCases: 6,
    parserExecutions: 12,
  });
  assertOrderIndependent(
    (entries) => setupArguments(entries, "IOTSZ"),
    [["mode", "qb"], settings[1]],
  );

  assertErrorCode(
    () => setupArguments([["mode", "qb"]]),
    "options.setup_qb_required",
  );
  assertErrorCode(
    () => setupArguments([settings[0], settings[1]]),
    "options.setup_qb_oracle_conflict",
  );
  assertErrorCode(
    () => setupArguments([["mode", "qb"], ["qb", "IOTSZ"]], "IOTSZJL"),
    "options.setup_qb_bag_capacity",
  );
  assert.doesNotThrow(
    () => setupArguments([["mode", "qb"], ["qb", "IOTS"]], "IOTSZJL"),
  );
  for (const qb of ["OS", "[OS]!"]) {
    const arguments_ = setupArguments([["mode", "qb"], ["qb", qb]], "TI");
    const qbIndex = arguments_.indexOf("--qb");
    assert.notEqual(qbIndex, -1);
    assert.equal(arguments_[qbIndex + 1], qb);
  }
  assert.deepEqual(
    setupArguments([["post-cycle-borrow", "on"]]).slice(-1),
    ["--allow-post-cycle-borrow"],
  );
  assertErrorCode(
    () => setupArguments(
      [["post-cycle-borrow", "on"]],
      "IOTSZJL",
    ),
    "options.setup_borrow_cycle",
  );
});

test("finesse score alias accepts the canonical named controls without a search preset", () => {
  assert.equal(findSlashCommand("finesse").subcommands.search, undefined);
  const score = [
    ["hold", "disabled"],
    ["knowledge", "oracle"],
    ["source-pieces", 3],
  ];
  assert.deepEqual(assertSinglesAndPairs(finesseScoreArguments, score), {
    semanticCases: 6,
    parserExecutions: 12,
  });
  const command = findSlashCommand("build").subcommands["finesse-score"];
  assert.deepEqual(finesseScoreArguments(score), finesseScoreArguments(score, command));
  for (const key of ["options", "aggregation", "spin-profile", "preserve-b2b"]) {
    assert.throws(
      () => finesseScoreArguments([[key, key === "preserve-b2b" ? "on" : "spin"]]),
      new RegExp(`unsupported option '${key}'`),
    );
  }
});

test("native no-kick is accepted by damage and PC path aliases", () => {
  const args = damageArguments([], "no-kick");
  assert.deepEqual(args.slice(args.indexOf("--rule"), args.indexOf("--rule") + 2), [
    "--rule", "no-kick",
  ]);
  const options = [
    { name: "field", value: "######____" },
    { name: "next", value: "I" },
    { name: "lines", value: 1 },
    { name: "kicktable", value: "no-kick" },
  ];
  assert.deepEqual(
    buildSlashCommandArguments(findSlashCommand("path"), options),
    buildSlashCommandArguments(findSlashCommand("pc").subcommands.path, options),
  );
});

test("packed contradictions are stable regardless of entry order", () => {
  for (const entries of [
    [["fill-bottom", "5"], ["fill-top", "4"]],
    [["fill-top", "4"], ["fill-bottom", "5"]],
  ]) {
    assertErrorCode(
      () => spinStructureArguments(entries),
      "options.spin_fill_bounds",
    );
  }
});

function damageArguments(settings, kicktable = null, command = findSlashCommand("damage")) {
  return buildSlashCommandArguments(command, [
    { name: "field", value: EMPTY },
    { name: "next", value: "IOTS" },
    ...(kicktable ? [{ name: "kicktable", value: kicktable }] : []),
    ...namedOptions(settings),
  ]);
}

function setupArguments(settings, remaining = "IOT") {
  return buildSlashCommandArguments(findSlashCommand("setup").subcommands.joint, [
    { name: "remaining", value: remaining },
    ...namedOptions(settings),
  ]);
}

function spinStructureArguments(settings) {
  return buildSlashCommandArguments(findSlashCommand("spin-structure").subcommands.search, [
    { name: "pieces", value: "IOTS" },
    { name: "field", value: EMPTY },
    ...settings.map(([name, value]) => ({
      name,
      value: ["fill-bottom", "fill-top", "max-placements"].includes(name)
        ? Number(value)
        : value,
    })),
  ]);
}

function finesseScoreArguments(settings, command = findSlashCommand("finesse").subcommands.score) {
  return buildSlashCommandArguments(command, [
    { name: "document", value: FINESSE_DOCUMENT },
    { name: "next", value: "I" },
    ...namedOptions(settings),
  ]);
}

function namedOptions(entries) {
  return entries.map(([name, value]) => ({ name, value }));
}

function assertSinglesAndPairs(build, settings) {
  const cases = settings.map((setting) => [setting]);
  for (let left = 0; left < settings.length; left += 1) {
    for (let right = left + 1; right < settings.length; right += 1) {
      cases.push([settings[left], settings[right]]);
    }
  }
  return assertCaseMatrix(build, cases);
}

function assertCaseMatrix(build, cases) {
  let parserExecutions = 0;
  for (const settings of cases) {
    parserExecutions += assertOrderIndependent(build, settings);
  }
  return { semanticCases: cases.length, parserExecutions };
}

function assertOrderIndependent(build, settings) {
  const forward = build(settings);
  const reverse = build([...settings].reverse());
  assert.deepEqual(reverse, forward, settings.map(([key]) => key).join(" + "));
  return 2;
}

function assertErrorCode(run, code) {
  assert.throws(run, (error) => error?.code === code);
}
