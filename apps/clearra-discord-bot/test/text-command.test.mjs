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
    [">sfinder bestsave PRIVATE", ">", "sfinder.best-save"],
    ["$clearra pc --field PRIVATE", "$", "pc"],
    ["$pc --field PRIVATE", "$", null],
    [">cat-finder PRIVATE", ">", null],
    ["$clearra sfinder catfinder PRIVATE", "$", null],
    [">unknown PRIVATE", ">", null],
  ];

  for (const [content, prefix, expected] of cases) {
    assert.equal(classifyClearraTextCommand(content, prefix), expected);
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
      "--patterns",
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
      "--patterns",
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
      { name: "options", value: "initial_b2b=true" },
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

test("spin-structure text shorthand accepts positional and named field contracts", () => {
  const positional = parseClearraTextRequest(
    ">spin-structure __________ tIo 1+ all-mini srs-plus",
    ">",
    remoteExecution,
  );
  assert.equal(positional.command.name, "spin-structure");
  assert.deepEqual(positional.rawOptions, [
    { name: "field", value: "__________" },
    { name: "pieces", value: "tIo" },
    { name: "lines", value: "1+" },
    { name: "profile", value: "all-mini" },
    { name: "kicktable", value: "srs-plus" },
  ]);
  assert.deepEqual(positional.argumentSets[0].slice(0, 12), [
    "spin-structure",
    "--board-mask-v1",
    "0".repeat(60),
    "--pieces",
    "TIO",
    "--lines",
    "1+",
    "--spin-profile",
    "all-mini",
    "--rule",
    "srs-plus",
    "--auto-workers",
  ]);

  const named = parseClearraTextRequest(
    "$spin-structure --field __________ --inventory zst --profile all-spin-plus",
    "$",
    remoteExecution,
  );
  assert.deepEqual(named.rawOptions, [
    { name: "field", value: "__________" },
    { name: "pieces", value: "zst" },
    { name: "profile", value: "all-spin-plus" },
  ]);
  assert.deepEqual(named.argumentSets[0].slice(0, 9), [
    "spin-structure",
    "--board-mask-v1",
    "0".repeat(60),
    "--pieces",
    "ZST",
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

test("greater-than cover text commands reuse the two-field slash contract", () => {
  const arguments_ = parseClearraTextMessage(
    ">cover --base __________ --target XXXX______ --patterns I --kicktable srs-plus",
    ">",
    { ...remoteExecution, workers: 4, logicalProcessors: 4 },
  );
  assert.deepEqual(arguments_.slice(0, 8), [
    "sfinder",
    "cover",
    "--base-mask-v1",
    "0".repeat(60),
    "--target-mask-v1",
    `${"0".repeat(59)}f`,
    "--patterns",
    "I",
  ]);
  assert.deepEqual(arguments_.slice(8), [
    "--rule",
    "srs-plus",
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
      "--patterns",
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
      "--patterns",
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
  assert.deepEqual(request.argumentSets[0].slice(0, 8), [
    "sfinder",
    "cover",
    "--base-mask-v1",
    "0".repeat(60),
    "--target-mask-v1",
    `${"0".repeat(59)}f`,
    "--patterns",
    "I",
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
    assert.deepEqual(request.argumentSets[0].slice(0, 8), [
      "sfinder",
      "cover",
      "--base-mask-v1",
      `${"0".repeat(59)}3`,
      "--target-mask-v1",
      `${"0".repeat(58)}f0`,
      "--patterns",
      "I",
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

test("bare verify aliases use the registered slash scope contract", () => {
  assert.deepEqual(
    parseClearraTextMessage("$verify kicks", "$", remoteExecution),
    [
      "sfinder",
      "verify",
      "kicks",
      "--format",
      "json",
      "--include-solution-data",
    ],
  );
});

test("explicit clearra keeps native commands behind the existing host policy", () => {
  assert.deepEqual(
    parseClearraTextMessage(
      "$clearra pc --lines 2 --workers 99 --format text",
      "$",
      { ...remoteExecution, workers: 3, logicalProcessors: 3 },
    ),
    [
      "pc",
      "--lines",
      "2",
      "--no-tablebase",
      "--no-build-dependency-dag",
      "--auto-workers",
      "3",
      "--format",
      "json",
      "--include-solution-data",
    ],
  );
  assert.equal(
    parseClearraTextMessage("$pc --lines 2", "$", remoteExecution),
    null,
  );

  const request = parseClearraTextRequest(
    "$clearra pc --lines 2",
    "$",
    remoteExecution,
  );
  assert.equal(request.argumentSets.length, 1);
  assert.equal(request.arguments_, request.argumentSets[0]);
  assert.ok(Object.isFrozen(request.argumentSets));
  assert.ok(Object.isFrozen(request.arguments_));
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
