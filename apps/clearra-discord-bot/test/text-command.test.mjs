import assert from "node:assert/strict";
import test from "node:test";

import { decodeCtk3, encodeCtk3 } from "ctk3";
import { encoder as fumenEncoder, Field } from "tetris-fumen";

import { Clearrabot } from "../src/bot.mjs";
import {
  classifyClearraTextCommand,
  parseClearraTextMessage,
  parseClearraTextRequest,
} from "../src/clearra/text-command.mjs";
import { buildSearchPreviewDocument } from "../src/viewer/search-preview.mjs";

const remoteExecution = Object.freeze({
  workers: 8,
  logicalProcessors: 8,
  outputFormat: "json",
  includeSolutionData: true,
});

test("text command classification shares parser aliases without retaining arguments", () => {
  const cases = [
    ["$path --field PRIVATE --next PRIVATE", "$", "path"],
    [">score-finder PRIVATE PRIVATE", ">", "score-finder"],
    ["$sfinder score-finder PRIVATE PRIVATE", "$", "score-finder"],
    [">sfinder bestsave PRIVATE", ">", null],
    ["$clearra sfinder path PRIVATE", "$", null],
    ["$clearra path --field PRIVATE", "$", null],
    ["$clearra pc --field PRIVATE", "$", null],
    ["$pc --field PRIVATE", "$", null],
    [">cat-finder PRIVATE", ">", null],
    ["$clearra sfinder catfinder PRIVATE", "$", null],
    [">unknown PRIVATE", ">", null],
  ];

  for (const [content, prefix, expected] of cases) {
    assert.equal(classifyClearraTextCommand(content, prefix), expected);
  }
});

test("text command classification keeps an exact identity when arguments are malformed", () => {
  const privateTail = "PRIVATE_FIELD";
  const cases = [
    [`$path --field \"${privateTail}`, "$", "path"],
    [`>score-finder \`${privateTail}`, ">", "score-finder"],
    [`$sfinder bestsave \"${privateTail}`, "$", null],
    [`$clearra pc \"${privateTail}`, "$", null],
  ];

  for (const [content, prefix, expected] of cases) {
    assert.equal(classifyClearraTextCommand(content, prefix), expected);
  }
});

test("sfinder compatibility spellings retain only translator-backed identities", () => {
  const cases = [
    ["bestsave", "best-save", false],
    ["bestsetup", "best-setup", false],
    ["congruentcover", "congruent-cover", false],
    ["coverpercent", "cover-percent", false],
    ["dpcfinder", "dpc-finder", false],
    ["pcsetup", "pc-setup", false],
    ["scoreminimals", "score-minimals", false],
    ["setupcover", "setup-cover", false],
    ["specialcover", "special-cover", false],
    ["spincover", "spin-cover", false],
  ];

  for (const [compatibilityName, slashName, sfinderAllowed] of cases) {
    assert.equal(
      classifyClearraTextCommand(`$sfinder ${compatibilityName} PRIVATE`, "$"),
      sfinderAllowed ? slashName : null,
    );
    assert.equal(
      classifyClearraTextCommand(`>clearra sfinder ${compatibilityName} PRIVATE`, ">"),
      null,
    );
  }
});

test("bare path text commands use the registered slash field contract", () => {
  assert.deepEqual(
    parseClearraTextMessage(
      "$path --field XXXXXX____ --patterns I --lines 1 --workers 99 --format text",
      "$",
      remoteExecution,
    ),
    [
      "sfinder",
      "path",
      "--field-mask-v1",
      "000000000000003f",
      "--queue",
      "I",
      "--lines",
      "1",
      "--auto-workers",
      "8",
      "--format",
      "json",
      "--include-solution-data",
    ],
  );
});

test("advanced text objectives resolve pc.path ingress to honest canonical PC variants", () => {
  const cases = [
    [
      "$path --field XXXXXX____ --patterns I --lines 1 --objective all",
      "$", "path", "pc.path", "path",
    ],
    [
      ">path --field XXXXXX____ --patterns I --lines 1 --objective unique",
      ">", "chance", "pc.chance", "chance",
    ],
    [
      "$pc path --field XXXXXX____ --patterns I --lines 1 --objective min-cover",
      "$", "minimals", "pc.minimals", "minimals",
    ],
    [
      ">path --field XXXXXX____ --patterns I --lines 1 --objective minimum-cover",
      ">", "minimals", "pc.minimals", "minimals",
    ],
    [
      "$path --field XXXXXX____ --patterns I --lines 1 --objective=tiling",
      "$", "tiling", "pc.tiling", "tiling",
    ],
  ];

  for (const [content, prefix, subcommand, capabilityId, resultKind] of cases) {
    const request = parseClearraTextRequest(content, prefix, remoteExecution);
    assert.equal(request.command.rootName, "pc");
    assert.equal(request.command.subcommand, subcommand);
    assert.equal(request.command.capabilityId, capabilityId);
    assert.equal(request.command.publicResultKind, resultKind);
    assert.equal(classifyClearraTextCommand(content, prefix), `pc.${subcommand}`);
    assert.equal(request.arguments_[0], "pc");
    assert.deepEqual(request.arguments_.slice(0, 2), ["pc", subcommand]);
    assert.ok(request.arguments_.includes("--board-mask"));
    assert.equal(request.arguments_.includes("--target-mask"), false);
    assert.equal(request.arguments_.includes("--objective"), false);
    assert.equal(request.arguments_.includes("--tiling-only"), false);
    assert.deepEqual(request.rawOptions, [
      { name: "field", value: "XXXXXX____" },
      { name: "next", value: "I" },
      { name: "lines", value: "1" },
    ]);
    if (subcommand === "minimals") {
      assert.equal(request.command.problemContractId, "pc-clear-to-empty.v2");
      assert.equal(request.command.resultContractId, "pc-minimum-cover.v2");
      assert.deepEqual(request.command.resultAllowlist, ["pc-minimum-cover.v2"]);
    } else if (subcommand === "path") {
      assert.equal(request.command.resultContractId, "pc-path-family.v2");
      assert.deepEqual(request.command.resultAllowlist, ["pc-path-family.v2"]);
    } else if (subcommand === "tiling") {
      assert.equal(request.command.resultContractId, "pc-tiling-family.v1");
      assert.deepEqual(request.command.resultAllowlist, ["pc-tiling-family.v1"]);
    }
  }

  assert.throws(
    () => parseClearraTextRequest(
      ">path --field XXXXXX____ --patterns I --lines 1 --objective tiling --kicktable srs",
      ">",
      remoteExecution,
    ),
    /does not expose option '--kicktable'/i,
  );
});

test("pc chance text is typed while top-level chance and percent stay generic", () => {
  const base = "--field XXXXXX____ --patterns I --lines 1 --kicktable srs";
  for (const prefix of ["$", ">"]) {
    const canonical = parseClearraTextRequest(`${prefix}pc chance ${base}`, prefix);
    assert.equal(canonical.command.capabilityId, "pc.chance");
    assert.equal(canonical.command.resultAuthorityId, "pc-chance");
    assert.deepEqual(canonical.arguments_.slice(0, 2), ["pc", "chance"]);
    assert.equal(canonical.arguments_.includes("--objective"), false);
    assert.equal(canonical.arguments_.includes("--queue-knowledge"), false);

    for (const name of ["chance", "percent"]) {
      const generic = parseClearraTextRequest(`${prefix}${name} ${base}`, prefix);
      assert.equal(generic.command.capabilityId, `discord.compat.${name}`);
      assert.equal(generic.command.resultAuthorityId, name);
      assert.deepEqual(generic.arguments_.slice(0, 2), ["sfinder", name]);
      assert.equal(generic.arguments_.includes("--objective"), false);
    }

    assert.throws(
      () => parseClearraTextRequest(`${prefix}pc percent ${base}`, prefix),
      /requires one of:/i,
    );
    for (const option of [
      "--queue-knowledge oracle",
      "--spin-profile all-spin",
      "--preserve-b2b",
      "--solution-probabilities",
      "--objective unique",
    ]) {
      assert.throws(
        () => parseClearraTextRequest(`${prefix}pc chance ${base} ${option}`, prefix),
        /does not expose option|does not support options key|only on the pc\.path base capability/i,
      );
    }
  }
});

test("pc score text is typed while top-level score remains independently generic", () => {
  const base = "--field XXXXXX____ --patterns I --lines 1 --kicktable srs";
  for (const prefix of ["$", ">"] ) {
    const canonical = parseClearraTextRequest(
      `${prefix}pc score ${base} --score-profile guideline --spin-profile all-mini-plus --initial-b2b 2`,
      prefix,
    );
    assert.equal(canonical.command.capabilityId, "pc.score");
    assert.equal(canonical.command.resultAuthorityId, "pc-score");
    assert.deepEqual(canonical.command.resultAllowlist, ["pc-score-summary.v2"]);
    assert.deepEqual(canonical.arguments_.slice(0, 2), ["pc", "score"]);
    assert.equal(canonical.arguments_.includes("--objective"), false);
    assert.equal(canonical.arguments_.includes("--score"), false);
    assert.equal(
      canonical.arguments_[canonical.arguments_.indexOf("--score-profile") + 1],
      "guideline",
    );
    assert.equal(
      canonical.arguments_[canonical.arguments_.indexOf("--spin-profile") + 1],
      "all-mini-plus",
    );
    assert.equal(
      canonical.arguments_[canonical.arguments_.indexOf("--initial-b2b") + 1],
      "2",
    );

    const legacy = parseClearraTextRequest(`${prefix}score ${base}`, prefix);
    assert.equal(legacy.command.capabilityId, "discord.compat.score");
    assert.equal(legacy.command.telemetryIdentity, "discord.compat.score");
    assert.equal(
      legacy.command.loweringAuthority,
      "discord.generic-compatibility-lowering.v1",
    );
    assert.equal(legacy.command.compatibilityPreset, null);
    assert.equal(legacy.command.resultAuthorityId, "score");
    assert.deepEqual(legacy.command.resultAllowlist, ["pc-scenario"]);
    assert.deepEqual(legacy.arguments_.slice(0, 2), ["sfinder", "score"]);

    for (const option of [
      "--objective all",
      "--queue-knowledge visible-7",
      "--preserve-b2b",
      "--solution-probabilities",
      "--max-memory-mib 128",
      "--workers 2",
    ]) {
      assert.throws(
        () => parseClearraTextRequest(`${prefix}pc score ${base} ${option}`, prefix),
        /does not expose|does not support|only on the pc\.path base capability/i,
        option,
      );
    }
  }
  const scoreMinimals = parseClearraTextRequest(`${"$"}pc score-minimals ${base}`, "$");
  assert.deepEqual(scoreMinimals.arguments_.slice(0, 2), ["pc", "score-minimals"]);
  for (const forbidden of [
    "--objective", "--score", "--solution-probabilities", "--ties",
    "--tie-snapshot", "--tie-cursor",
  ]) assert.equal(scoreMinimals.arguments_.includes(forbidden), false, forbidden);
  const scoreMinimalsAlias = parseClearraTextRequest(
    `${"$"}score-minimals ${base}`,
    "$",
  );
  assert.deepEqual(scoreMinimalsAlias.arguments_, scoreMinimals.arguments_);
  assert.equal(scoreMinimalsAlias.command.resultAuthorityId, "score-minimals");
  for (const option of ["--solution-probabilities", "--workers 2", "--ties"]) {
    assert.throws(
      () => parseClearraTextRequest(`${"$"}pc score-minimals ${base} ${option}`, "$"),
      /does not expose|does not support|unsupported option/i,
      option,
    );
  }
  assert.deepEqual(
    parseClearraTextRequest(`${"$"}pc score-finder ${base}`, "$")
      .arguments_.slice(0, 2),
    ["pc", "score-finder"],
  );
});

test("advanced text objectives reject duplicate, unknown, non-base, and incompatible use", () => {
  const base = "XXXXXX____ I 1";
  const rejected = [
    [`$path ${base} --objective all --objective unique`, /specified only once/],
    [`>path ${base} --objective=all --objective unique`, /specified only once/],
    [`$path ${base} --objective unknown`, /Unknown registered PC objective/],
    [`>path ${base} --objective minimals`, /Unknown registered PC objective/],
    [`$path ${base} --objective tiling-only`, /Unknown registered PC objective/],
    [`>path ${base} --objective minimum_cover`, /Unknown registered PC objective/],
    [`$path ${base} --objective`, /requires one registered objective ID/],
    [`>chance ${base} --objective unique`, /only on the pc\.path base capability/],
    ["$cover __________ ####______ I --objective all", /only on the pc\.path base capability/],
    [`$sfinder path ${base} --objective unique`, /unavailable under the explicit sfinder namespace/],
    [`>sfinder path ${base} --objective all`, /unavailable under the explicit sfinder namespace/],
    [`$path ${base} --objective min-cover --queue-knowledge visible-7`, /unavailable with minimum-cover/],
    [`$path ${base} --objective unique --target ####______`, /does not expose option '--target'/],
  ];
  for (const [content, expected] of rejected) {
    assert.throws(
      () => parseClearraTextRequest(content, content[0], remoteExecution),
      expected,
      content,
    );
  }

  for (const prefix of ["$", ">"]) {
    assert.equal(parseClearraTextRequest(`${prefix}objective all`, prefix), null);
    assert.equal(classifyClearraTextCommand(`${prefix}objective all`, prefix), null);
    assert.equal(
      parseClearraTextRequest(`${prefix}unknown --objective all`, prefix),
      null,
    );
  }
});

test("dollar and greater-than finesse commands share the grouped slash contracts", () => {
  const document = encodeCtk3({
    width: 10,
    pages: [{
      height: 0,
      cells: [],
      operation: { piece: "T", rotation: "spawn", x: 4, y: 0 },
    }],
  });
  const search = parseClearraTextRequest(
    '$finesse search XXXX______ I __________ srs-plus "hold=avoid knowledge=visible-7"',
    "$",
    remoteExecution,
  );
  assert.equal(search.command.subcommand, "search");
  assert.equal(classifyClearraTextCommand("$finesse search PRIVATE", "$"), "finesse.search");
  assert.deepEqual(search.arguments_.slice(0, 14), [
    "build-probability",
    "--base-mask", "0".repeat(60),
    "--target-mask", `${"0".repeat(59)}f`,
    "--height", "8",
    "--queue", "I",
    "--no-hold",
    "--pattern-knowledge", "visible-7",
    "--finesse", "inputs",
  ]);
  assert.equal(search.arguments_.includes("--no-mirror"), true);
  assert.equal(search.arguments_[search.arguments_.indexOf("--rule") + 1], "srs-plus");

  const score = parseClearraTextRequest(
    `>finesse score ${document} T --knowledge oracle`,
    ">",
    remoteExecution,
  );
  assert.equal(score.command.subcommand, "score");
  assert.equal(score.arguments_.includes(document), false);
  assert.deepEqual(score.arguments_.slice(0, 10), [
    "finesse", "score",
    "--initial-mask", "0".repeat(60),
    "--height", "2",
    "--placements", "T:spawn:3:0",
    "--queue", "T",
  ]);
  assert.equal(classifyClearraTextCommand(">finesse score PRIVATE", ">"), "finesse.score");
});

test("setup-ranking text commands share the canonical slash settings without finesse collisions", () => {
  const arguments_ = parseClearraTextMessage(
    "$best-setup IOTS --setup-order build --max-setup-pieces 10 --queue-knowledge visible-7 --next-cycle-remaining Z --setup-length shorter --rule srs-x",
    "$",
    remoteExecution,
  );
  assert.deepEqual(arguments_.slice(0, 16), [
    "setup-finder",
    "--remaining", "IOTS",
    "--priority", "build",
    "--max-setup-pieces", "10",
    "--queue-knowledge", "visible-7",
    "--next-cycle-remaining", "Z",
    "--setup-length", "shorter",
    "--rule", "srs-x",
    "--no-tablebase",
  ]);
  assert.equal(arguments_.includes("--pattern-knowledge"), false);

  assert.deepEqual(
    parseClearraTextMessage(">dpc-finder IOTS --knowledge full-queue", ">", remoteExecution)
      .slice(0, 7),
    [
      "setup-finder",
      "--remaining", "IOTS",
      "--priority", "pc",
      "--queue-knowledge", "oracle",
    ],
  );
});

test("catalog text requests retain the raw field for the parallel preview", () => {
  const request = parseClearraTextRequest(
    "$path --field XXXXXX____ --patterns I --lines 1",
    "$",
    remoteExecution,
  );
  assert.equal(request.command.name, "path");
  assert.deepEqual(request.rawOptions, [
    { name: "field", value: "XXXXXX____" },
    { name: "next", value: "I" },
    { name: "lines", value: "1" },
  ]);
  assert.equal(request.argumentSets.length, 1);
  assert.equal(request.arguments_, request.argumentSets[0]);
  assert.equal(request.automaticPcTargets, false);
  assert.equal(request.arguments_[2], "--field-mask-v1");
  assert.equal(request.helpTarget, null);
  assert.ok(Object.isFrozen(request));
  assert.ok(Object.isFrozen(request.argumentSets));
  assert.ok(Object.isFrozen(request.argumentSets[0]));
  assert.ok(Object.isFrozen(request.rawOptions));
  assert.ok(Object.isFrozen(request.rawOptions[0]));
});

test("explicit sfinder catalog commands use the same slash field normalization", () => {
  assert.deepEqual(
    parseClearraTextMessage(
      "$sfinder path --field XXXXXX____ --patterns I --lines 1",
      "$",
      remoteExecution,
    ),
    [
      "sfinder",
      "path",
      "--field-mask-v1",
      "000000000000003f",
      "--queue",
      "I",
      "--lines",
      "1",
      "--auto-workers",
      "8",
      "--format",
      "json",
      "--include-solution-data",
    ],
  );
});

test("score-finder text routes preserve the fixed-queue engine syntax", () => {
  for (const commandName of ["score-finder", "sfinder score-finder"]) {
    const request = parseClearraTextRequest(
      `$${commandName} __________ SIJSTLZO 5 true`,
      "$",
      remoteExecution,
    );
    assert.equal(request.command.name, "score-finder");
    assert.deepEqual(request.rawOptions, [
      { name: "field", value: "__________" },
      { name: "next", value: "SIJSTLZO" },
      { name: "lines", value: "5" },
      { name: "options", value: "initial-b2b=true" },
    ]);
    assert.deepEqual(request.argumentSets[0].slice(0, 10), [
      "sfinder",
      "score-finder",
      "--field-mask-v1",
      "0000000000000000",
      "--queue",
      "SIJSTLZO",
      "--lines",
      "5",
      "--initial-b2b",
      "true",
    ]);
  }
});

test("All-Spin text aliases preserve exact and pattern contracts for both prefixes", () => {
  const field = "grid:__________/####______";
  const exactCases = [
    ["$", `$pc allspin-sol --field ${field} --queue IOTS --lines 2 --spin-profile all-spin-plus --no-hold`],
    [">", `>allspin_sol_finder --field ${field} --queue IOTS --lines 2 --spin-profile all-spin-plus --no-hold`],
  ];
  for (const [prefix, content] of exactCases) {
    const request = parseClearraTextRequest(content, prefix, remoteExecution);
    assert.equal(request.command.capabilityId, "pc.allspin-sol");
    assert.deepEqual(request.argumentSets[0].slice(0, 17), [
      "pc", "allspin-sol",
      "--lines", "2",
      "--board-mask", "0xf",
      "--height", "2",
      "--pieces", "4",
      "--queue", "IOTS",
      "--no-hold",
      "--spin-profile", "all-spin-plus",
      "--rule", "srs-plus",
    ]);
    assert.equal(
      request.arguments_[request.arguments_.indexOf("--auto-workers") + 1],
      "8",
    );
    assert.equal(request.arguments_.includes("--preserve-b2b"), false);
  }

  const chanceCases = [
    ["$", `$pc allspin-pres-chance --field ${field} --patterns [IOTS]! --lines 2 --spin-profile all-mini-plus --max-nodes 17`],
    [">", `>allspin_pres_chance --field ${field} --pattern [IOTS]! --lines 2 --spin-profile all-mini-plus --max-nodes 17`],
  ];
  for (const [prefix, content] of chanceCases) {
    const request = parseClearraTextRequest(content, prefix, remoteExecution);
    assert.equal(request.command.capabilityId, "pc.allspin-pres-chance");
    assert.equal(request.arguments_[0], "pc");
    assert.equal(request.arguments_[1], "allspin-pres-chance");
    assert.equal(
      request.arguments_[request.arguments_.indexOf("--patterns") + 1],
      "[IOTS]!",
    );
    assert.equal(request.arguments_.includes("--queue"), false);
    assert.equal(request.arguments_.includes("--preserve-b2b"), false);
    assert.equal(
      request.arguments_[request.arguments_.indexOf("--max-nodes") + 1],
      "17",
    );
  }

  const openingExact = parseClearraTextRequest(
    "$allspin_sol_finder --field grid:__________/__________ --queue IIOOO --lines 2 --spin-profile all-spin-plus",
    "$",
    remoteExecution,
  );
  assert.deepEqual(openingExact.argumentSets[0].slice(0, 10), [
    "pc", "allspin-sol",
    "--lines", "2",
    "--queue", "IIOOO",
    "--spin-profile", "all-spin-plus",
    "--rule", "srs-plus",
  ]);
  assert.equal(openingExact.arguments_.includes("--board-mask"), false);
  assert.equal(openingExact.arguments_.includes("--height"), false);
  assert.equal(openingExact.arguments_.includes("--pieces"), false);

  const openingChance = parseClearraTextRequest(
    ">allspin_pres_chance --field grid:__________/__________ --pattern [IO]!OOO --lines 2 --spin-profile all-mini-plus --no-hold",
    ">",
    remoteExecution,
  );
  assert.deepEqual(openingChance.argumentSets[0].slice(0, 11), [
    "pc", "allspin-pres-chance",
    "--lines", "2",
    "--patterns", "[IO]!OOO",
    "--no-hold",
    "--spin-profile", "all-mini-plus",
    "--rule", "srs-plus",
  ]);
  assert.equal(openingChance.arguments_.includes("--board-mask"), false);
  assert.equal(openingChance.arguments_.includes("--height"), false);
  assert.equal(openingChance.arguments_.includes("--pieces"), false);

  assert.equal(
    classifyClearraTextCommand("$allspin_sol_finder PRIVATE", "$"),
    "allspin-sol-finder",
  );
  assert.equal(
    classifyClearraTextCommand(">allspin_pres_chance PRIVATE", ">"),
    "allspin-pres-chance",
  );
  assert.equal(parseClearraTextRequest("$sfinder allspin_sol_finder PRIVATE", "$"), null);

  const rejected = [
    [`$allspin_sol_finder --field ${field} --patterns [IOTS]! --lines 2 --spin-profile all-spin-plus`, /requires --queue or --next/],
    [`$allspin_pres_chance --field ${field} --queue IOTS --lines 2 --spin-profile all-spin-plus`, /requires --patterns/],
    [`$allspin_sol_finder --field ${field} --queue IOTS --lines 2`, /spin-profile input is required/],
    [`$allspin_sol_finder --field ${field} --queue IOTS --lines 2 --spin-profile all-spin --spin-profile all-mini`, /more than once/],
    [`$allspin_sol_finder --field ${field} --queue IOTS --lines 2 --spin-profile all-spin --preserve-b2b`, /does not expose option '--preserve-b2b'/],
    [`$allspin_pres_chance --field ${field} --patterns [IOTS]! --lines 2 --spin-profile all-spin --target ####______`, /does not expose option '--target'/],
  ];
  for (const [content, expected] of rejected) {
    assert.throws(
      () => parseClearraTextRequest(content, "$", remoteExecution),
      expected,
      content,
    );
  }
});

test("spin-structure text shorthand accepts positional and named field contracts", () => {
  const positional = parseClearraTextRequest(
    ">spin-structure search __________ tIo 1+ all-mini srs-plus",
    ">",
    remoteExecution,
  );
  assert.equal(positional.command.name, "spin-structure");
  assert.deepEqual(positional.rawOptions, [
    { name: "field", value: "__________" },
    { name: "pieces", value: "tIo" },
    { name: "lines", value: "1+" },
    { name: "spin-profile", value: "all-mini" },
    { name: "kicktable", value: "srs-plus" },
  ]);
  assert.deepEqual(positional.argumentSets[0].slice(0, 14), [
    "spin-structure",
    "search",
    "--board-mask-v1",
    "0".repeat(60),
    "--pieces",
    "TIO",
    "--height",
    "8",
    "--lines",
    "1+",
    "--spin-profile",
    "all-mini",
    "--rule",
    "srs-plus",
  ]);
  assert.deepEqual(positional.argumentSets[0].slice(14), [
    "--auto-workers",
    "8",
    "--format",
    "json",
    "--include-solution-data",
  ]);

  const named = parseClearraTextRequest(
    "$spin-structure search --field __________ --inventory zst --profile all-spin-plus",
    "$",
    remoteExecution,
  );
  assert.deepEqual(named.rawOptions, [
    { name: "field", value: "__________" },
    { name: "pieces", value: "zst" },
    { name: "spin-profile", value: "all-spin-plus" },
  ]);
  assert.deepEqual(named.argumentSets[0].slice(0, 12), [
    "spin-structure",
    "search",
    "--board-mask-v1",
    "0".repeat(60),
    "--pieces",
    "ZST",
    "--height",
    "8",
    "--lines",
    "1+",
    "--spin-profile",
    "all-spin-plus",
  ]);
});

test("retired cat-finder names are not accepted by either text prefix", () => {
  for (const prefix of ["$", ">"] ) {
    for (const command of [
      "cat-finder",
      "cat_finder",
      "catfinder",
      "sfinder cat-finder",
      "sfinder cat_finder",
      "sfinder catfinder",
      "clearra sfinder cat-finder",
      "clearra sfinder cat_finder",
      "clearra sfinder catfinder",
    ]) {
      const content = `${prefix}${command} __________ SIJSTLZO 5 true`;
      assert.equal(parseClearraTextRequest(content, prefix, remoteExecution), null);
      assert.equal(classifyClearraTextCommand(content, prefix), null);
    }
  }
});

test("damage text command keeps the native 24-row forward-search prefix", () => {
  const field = Array(24).fill("__________").join("\n");
  const request = parseClearraTextRequest(
    `$damage \`\`\`field\n${field}\n\`\`\` SI`,
    "$",
    remoteExecution,
  );
  assert.equal(request.command.name, "damage");
  assert.deepEqual(request.argumentSets[0].slice(0, 5), [
    "damage",
    "--board-mask-v1",
    "0".repeat(60),
    "--queue",
    "SI",
  ]);
});

test("REN text command lowers only the exact fixed-queue geometry contract", () => {
  const request = parseClearraTextRequest(
    "$forward ren --field ______XXXX --next TI --height 4 --no-hold --kicktable srs-plus",
    "$",
    remoteExecution,
  );
  assert.equal(request.command.capabilityId, "forward.ren");
  assert.deepEqual(request.argumentSets[0].slice(0, 11), [
    "ren",
    "--board-mask-v1",
    `${"0".repeat(57)}3c0`,
    "--height",
    "4",
    "--queue",
    "TI",
    "--no-hold",
    "--rule",
    "srs-plus",
    "--auto-workers",
  ]);
  assert.equal(request.argumentSets[0].includes("--spin-profile"), false);
  assert.equal(request.argumentSets[0].includes("--initial-combo"), false);
  assert.equal(request.argumentSets[0].includes("--minimum-damage"), false);
});

test("greater-than cover text commands reuse the two-field slash contract", () => {
  const arguments_ = parseClearraTextMessage(
    ">cover --base __________ --target XXXX______ --patterns I --kicktable srs-plus",
    ">",
    { ...remoteExecution, workers: 4, logicalProcessors: 4 },
  );
  assert.deepEqual(arguments_.slice(0, 14), [
    "build-probability",
    "--base-mask",
    "0".repeat(60),
    "--target-mask",
    `${"0".repeat(59)}f`,
    "--height",
    "1",
    "--queue",
    "I",
    "--hold",
    "empty",
    "--no-mirror",
    "--rule",
    "srs-plus",
  ]);
  assert.deepEqual(arguments_.slice(14), [
    "--auto-workers",
    "4",
    "--format",
    "json",
    "--include-solution-data",
  ]);
});

test("dollar and greater-than path commands accept positional CTK3 and option Fumen inputs", () => {
  const cells = ["I", "O", null, null, null, null, null, null, null, null];
  const ctk3 = encodeCtk3({
    width: 10,
    pages: [{ height: 1, cells }],
  });
  const fumen = fumenEncoder.encode([{
    field: Field.create("IO________"),
  }]);
  const cases = [
    ["$", `$path ${ctk3} I 1`, ctk3],
    [">", `>path --field ${fumen} --next I --lines 1`, fumen],
  ];

  for (const [prefix, content, source] of cases) {
    const request = parseClearraTextRequest(content, prefix, remoteExecution);
    assert.deepEqual(request.rawOptions, [
      { name: "field", value: source },
      { name: "next", value: "I" },
      { name: "lines", value: "1" },
    ]);
    assert.deepEqual(request.argumentSets[0].slice(0, 8), [
      "sfinder",
      "path",
      "--field-mask-v1",
      "0000000000000003",
      "--queue",
      "I",
      "--lines",
      "1",
    ]);

    const preview = buildSearchPreviewDocument(request.command, request.rawOptions);
    assert.equal(preview.format, "ctk3");
    assert.deepEqual(decodeCtk3(preview.source).pages[0].cells, cells);
  }
});

test("dollar and greater-than commands preserve quoted and fenced multiline grids", () => {
  const cases = [
    [
      "$",
      `$path --field "__________\n______####" --next I --lines 2`,
    ],
    [
      ">",
      `>path --field \`\`\`text\n__________\n______####\n\`\`\` --next I --lines 2`,
    ],
  ];

  for (const [prefix, content] of cases) {
    const request = parseClearraTextRequest(content, prefix, remoteExecution);
    assert.equal(request.rawOptions[0].name, "field");
    assert.equal(request.rawOptions[0].value, "__________\n______####");
    assert.deepEqual(request.argumentSets[0].slice(0, 8), [
      "sfinder",
      "path",
      "--field-mask-v1",
      "00000000000003c0",
      "--queue",
      "I",
      "--lines",
      "2",
    ]);
  }

  assert.throws(
    () => parseClearraTextRequest(
      "$path --field ```\n..........\n--next I",
      "$",
      remoteExecution,
    ),
    /unterminated code block/,
  );
});

test("greater-than cover accepts two positional multiline field code blocks", () => {
  const request = parseClearraTextRequest(
    `>cover \`\`\`text\n__________\n__________\n\`\`\` \`\`\`field\n__________\n####______\n\`\`\` I`,
    ">",
    remoteExecution,
  );
  assert.deepEqual(request.rawOptions, [
    { name: "base", value: "__________\n__________" },
    { name: "target", value: "__________\n####______" },
    { name: "next", value: "I" },
  ]);
  assert.deepEqual(request.argumentSets[0].slice(0, 12), [
    "build-probability",
    "--base-mask",
    "0".repeat(60),
    "--target-mask",
    `${"0".repeat(59)}f`,
    "--height",
    "1",
    "--queue",
    "I",
    "--hold",
    "empty",
    "--no-mirror",
  ]);
});

test("dollar and greater-than cover commands preserve both positional and option CTK3/Fumen fields in previews", () => {
  const baseCells = [
    "I", "O", null, null, null, null, null, null, null, null,
  ];
  const targetCells = [
    null, null, null, null, "T", "T", "T", "T", null, null,
  ];
  const ctk3Base = encodeCtk3({
    width: 10,
    pages: [{ height: 1, cells: baseCells }],
  });
  const ctk3Target = encodeCtk3({
    width: 10,
    pages: [{ height: 1, cells: targetCells }],
  });
  const fumenBase = fumenEncoder.encode([{
    field: Field.create("IO________"),
  }]);
  const fumenTarget = fumenEncoder.encode([{
    field: Field.create("____TTTT__"),
  }]);
  const cases = [
    ["$", `$cover ${ctk3Base} ${fumenTarget} I`, ctk3Base, fumenTarget],
    [
      ">",
      `>cover --base ${fumenBase} --target ${ctk3Target} --next I`,
      fumenBase,
      ctk3Target,
    ],
  ];

  for (const [prefix, content, base, target] of cases) {
    const request = parseClearraTextRequest(content, prefix, remoteExecution);
    assert.deepEqual(request.rawOptions, [
      { name: "base", value: base },
      { name: "target", value: target },
      { name: "next", value: "I" },
    ]);
    assert.deepEqual(request.argumentSets[0].slice(0, 12), [
      "build-probability",
      "--base-mask",
      `${"0".repeat(59)}3`,
      "--target-mask",
      `${"0".repeat(58)}f0`,
      "--height",
      "1",
      "--queue",
      "I",
      "--hold",
      "empty",
      "--no-mirror",
    ]);

    const preview = buildSearchPreviewDocument(request.command, request.rawOptions);
    assert.equal(preview.document.pages.length, 2);
    assert.deepEqual(preview.document.pages[0].cells, baseCells);
    assert.deepEqual(preview.document.pages[1].cells, [
      "I", "O", null, null, "T", "T", "T", "T", null, null,
    ]);
  }
});

test("text cover preparation prioritizes the two-field preview over an inline document candidate", async () => {
  const base = encodeCtk3({
    width: 10,
    pages: [{
      height: 1,
      cells: ["I", "O", null, null, null, null, null, null, null, null],
    }],
  });
  const target = fumenEncoder.encode([{
    field: Field.create("____TTTT__"),
  }]);
  const bot = new Clearrabot(
    {},
    {
      oracleRenderEnabled: true,
      oracleTextEnabled: true,
      oracleCommandPrefixes: ["$", ">"],
      oracleMaxInputChars: 2_000,
      oracleMaxPages: 128,
      oracleMaxCtk3FileBytes: 1024 * 1024,
      searchWorkersPerSession: 1,
      processLogicalProcessors: 1,
    },
    {
      executor: { async execute() { throw new Error("must not execute"); } },
      gifRenderer: { stop() {} },
    },
  );

  try {
    const prepared = await bot.prepareOracleMessage({
      id: "mixed-cover",
      channel_id: "channel",
      content: `$cover ${base} ${target} I`,
      author: { id: "user", bot: false },
      attachments: [],
    });
    assert.equal(prepared.previewDocument.document.pages.length, 2);
    assert.deepEqual(prepared.previewDocument.document.pages[0].cells, [
      "I", "O", null, null, null, null, null, null, null, null,
    ]);
    assert.deepEqual(prepared.previewDocument.document.pages[1].cells, [
      "I", "O", null, null, "T", "T", "T", "T", null, null,
    ]);
  } finally {
    bot.stop();
  }
});

test("only trimmed exact verify aliases reach the hidden text diagnostic", () => {
  for (const prefix of ["$", ">"] ) {
    const content = `  ${prefix}verify  `;
    assert.deepEqual(
      parseClearraTextMessage(content, prefix, remoteExecution),
      [
        "sfinder",
        "verify",
        "--format",
        "json",
        "--include-solution-data",
      ],
    );
    assert.equal(classifyClearraTextCommand(content, prefix), "verify");

    for (const scope of ["pc", "setup", "cover", "build", "kicks"]) {
      for (const rejected of [
        `${prefix}verify ${scope}`,
        `${prefix}verify --scope ${scope}`,
        `${prefix}verify --scope=${scope}`,
      ]) {
        assert.equal(parseClearraTextRequest(rejected, prefix, remoteExecution), null);
        assert.equal(classifyClearraTextCommand(rejected, prefix), null);
      }
    }
  }

  for (const [content, prefix] of [
    ["$VERIFY", "$"],
    [">Verify", ">"],
    ["$sfinder verify", "$"],
    [">sfinder verify", ">"],
    [">verify kicks --objective all", ">"],
    ["$verify \"", "$"],
    [">sfinder verify `", ">"],
    ["!verify", "!"],
  ]) {
    assert.equal(parseClearraTextRequest(content, prefix, remoteExecution), null);
    assert.equal(classifyClearraTextCommand(content, prefix), null);
  }
});

test("explicit clearra and noncatalog sfinder raw routes are disabled", () => {
  for (const content of [
    "$clearra path --field __________ --next I",
    "$clearra pc --lines 2",
    "$sfinder pc --lines 2",
    "$sfinder damage __________ I",
    "$sfinder finesse search XXXX______ I __________",
    "$sfinder pc-setup IOT",
    "$sfinder cover PRIVATE",
    "$sfinder unknown --field __________",
  ]) {
    assert.equal(parseClearraTextMessage(content, "$", remoteExecution), null);
    assert.equal(classifyClearraTextCommand(content, "$"), null);
  }
  assert.throws(
    () => parseClearraTextMessage("$pc --lines 2", "$", remoteExecution),
    /requires one of: path, chance, minimals, score, saves, best-save, score-minimals, tiling, failed-queue, score-finder/,
  );
});

test("bare PC aliases retain every automatic slash target", () => {
  const request = parseClearraTextRequest(
    "$path --field __________ --patterns IOTSZJLIOTSZJLI",
    "$",
    remoteExecution,
  );
  assert.equal(request.arguments_, null);
  assert.equal(request.automaticPcTargets, true);
  assert.deepEqual(
    request.argumentSets.map((arguments_) =>
      arguments_[arguments_.indexOf("--lines") + 1]
    ),
    ["2", "4", "6"],
  );
  assert.ok(Object.isFrozen(request.argumentSets));
  assert.ok(request.argumentSets.every(Object.isFrozen));
  assert.equal(
    parseClearraTextMessage(
      "$path --field __________ --patterns IOTSZJLIOTSZJLI",
      "$",
      remoteExecution,
    ),
    null,
  );
  assert.throws(
    () => parseClearraTextMessage(
      "$path --field-path local.txt --patterns I --lines 1",
      "$",
      remoteExecution,
    ),
    /does not expose option '--field-path'/,
  );
});

test("bare PC aliases include feasible odd automatic targets up to six rows", () => {
  const request = parseClearraTextRequest(
    "$path --field ........## --patterns IOTSZJLIOTSZ",
    "$",
    remoteExecution,
  );
  assert.deepEqual(
    request.argumentSets.map((arguments_) =>
      arguments_[arguments_.indexOf("--lines") + 1]
    ),
    ["1", "3", "5"],
  );
});

test("text help requests expose a non-executable request matching slash help", () => {
  const cases = [
    ["$help", "$", null],
    ["$help path", "$", "path"],
    ["$help --arguments path", "$", "path"],
    ["$help --arguments=score-minimals", "$", "score-minimals"],
    [">help arguments:cover", ">", "cover"],
  ];

  for (const [content, prefix, helpTarget] of cases) {
    const request = parseClearraTextRequest(content, prefix, remoteExecution);
    assert.equal(request.command.kind, "help");
    assert.equal(request.helpTarget, helpTarget);
    assert.equal(request.arguments_, null);
    assert.deepEqual(request.argumentSets, []);
    assert.deepEqual(request.rawOptions, []);
    assert.ok(Object.isFrozen(request));
    assert.ok(Object.isFrozen(request.argumentSets));
    assert.ok(Object.isFrozen(request.rawOptions));
    assert.equal(
      parseClearraTextMessage(content, prefix, remoteExecution),
      null,
    );
  }
});

test("text help rejects ambiguous or empty arguments", () => {
  assert.throws(
    () => parseClearraTextRequest("$help path cover", "$", remoteExecution),
    /at most one command name/,
  );
  assert.throws(
    () => parseClearraTextRequest("$help --arguments=", "$", remoteExecution),
    /cannot be empty/,
  );
});
