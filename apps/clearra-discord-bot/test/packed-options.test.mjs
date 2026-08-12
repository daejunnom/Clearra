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

test("damage packed options accept every representative single and legal pair in canonical order", () => {
  const settings = [
    ["hold", "avoid"],
    ["spin-profile", "all-mini-plus"],
    ["minimum-damage", "4"],
    ["initial-combo", "2"],
    ["initial-b2b", "3"],
    ["preserve-b2b", "on"],
  ];
  assert.deepEqual(assertSinglesAndPairs(damageArguments, settings), {
    semanticCases: 21,
    parserExecutions: 42,
  });
  assert.deepEqual(damageArguments(settings), [
    "damage",
    "--board-mask-v1", "0".repeat(60),
    "--queue", "IOTS",
    "--no-hold",
    "--spin-profile", "all-mini-plus",
    "--minimum-damage", "4",
    "--initial-combo", "2",
    "--initial-b2b", "3",
    "--preserve-b2b",
  ]);
});

test("spin-structure packed options accept every representative single and legal pair", () => {
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

test("setup packed options cover every legal pair and stable dependency errors", () => {
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
    () => setupArguments([["mode", "qb"], settings[1]], "IOTSZJ"),
    "options.setup_qb_bag_capacity",
  );
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

test("finesse packed options cover every legal single and pair with stable dependencies", () => {
  const search = [
    ["hold", "T"],
    ["knowledge", "visible-7"],
    ["source-pieces", "4"],
    ["aggregation", "spin"],
    ["spin-profile", "all-spin-plus"],
    ["preserve-b2b", "on"],
  ];
  let searchSemanticCases = 0;
  let searchParserExecutions = 0;
  for (const [index, setting] of search.entries()) {
    if (index !== 4) {
      searchSemanticCases += 1;
      searchParserExecutions += assertOrderIndependent(
        finesseSearchArguments,
        [setting],
      );
    }
  }
  for (let left = 0; left < search.length; left += 1) {
    for (let right = left + 1; right < search.length; right += 1) {
      const pair = [search[left], search[right]];
      if (pair.some(([key]) => key === "spin-profile") &&
        !pair.some(([key, value]) =>
          (key === "aggregation" && value === "spin") || key === "preserve-b2b"
        )) {
        assertErrorCode(
          () => finesseSearchArguments(pair),
          "options.finesse_spin_dependency",
        );
      } else {
        searchSemanticCases += 1;
        searchParserExecutions += assertOrderIndependent(
          finesseSearchArguments,
          pair,
        );
      }
    }
  }
  assert.deepEqual(
    { semanticCases: searchSemanticCases, parserExecutions: searchParserExecutions },
    { semanticCases: 17, parserExecutions: 34 },
  );
  assertErrorCode(
    () => finesseSearchArguments([["spin-profile", "t-spins"]]),
    "options.finesse_spin_dependency",
  );

  const score = [
    ["hold", "avoid"],
    ["knowledge", "full-queue"],
    ["source-pieces", "3"],
  ];
  assert.deepEqual(assertSinglesAndPairs(finesseScoreArguments, score), {
    semanticCases: 6,
    parserExecutions: 12,
  });
  for (const key of ["aggregation", "spin-profile", "preserve-b2b"]) {
    assertErrorCode(
      () => finesseScoreArguments([[key, key === "preserve-b2b" ? "on" : "spin"]]),
      "options.finesse_score_unsupported",
    );
  }
});

test("native no-kick is accepted while Sfinder-compatible commands retain four-rule ceiling", () => {
  assert.deepEqual(
    damageArguments([], "no-kick").slice(-2),
    ["--rule", "no-kick"],
  );
  assert.throws(
    () => buildSlashCommandArguments(findSlashCommand("path"), [
      { name: "field", value: EMPTY },
      { name: "next", value: "I" },
      { name: "kicktable", value: "no-kick" },
    ]),
    /srs-plus, srs, srs-x, or jstris-180/,
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

function damageArguments(settings, kicktable = null) {
  return buildSlashCommandArguments(findSlashCommand("damage"), [
    { name: "field", value: EMPTY },
    { name: "next", value: "IOTS" },
    ...(kicktable ? [{ name: "kicktable", value: kicktable }] : []),
    ...packed(settings),
  ]);
}

function setupArguments(settings, remaining = "IOT") {
  return buildSlashCommandArguments(findSlashCommand("pc-setup"), [
    { name: "remaining", value: remaining },
    ...packed(settings),
  ]);
}

function spinStructureArguments(settings) {
  return buildSlashCommandArguments(findSlashCommand("spin-structure"), [
    { name: "pieces", value: "IOTS" },
    { name: "field", value: EMPTY },
    ...packed(settings),
  ]);
}

function finesseSearchArguments(settings) {
  return buildSlashCommandArguments(findSlashCommand("finesse").subcommands.search, [
    { name: "base", value: EMPTY },
    { name: "target", value: "XXXX______" },
    { name: "next", value: "IOTS" },
    ...packed(settings),
  ]);
}

function finesseScoreArguments(settings) {
  return buildSlashCommandArguments(findSlashCommand("finesse").subcommands.score, [
    { name: "document", value: FINESSE_DOCUMENT },
    { name: "next", value: "I" },
    ...packed(settings),
  ]);
}

function packed(entries) {
  return entries.length === 0
    ? []
    : [{
        name: "options",
        value: entries.map(([key, value]) => `${key}=${value}`).join(" "),
      }];
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
