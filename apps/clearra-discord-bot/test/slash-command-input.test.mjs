import assert from "node:assert/strict";
import test from "node:test";

import { encodeCtk3 } from "ctk3";
import { encoder as fumenEncoder, Field } from "tetris-fumen";

import { findSlashCommand } from "../src/discord/slash-command-catalog.mjs";
import {
  automaticPcLines,
  buildSlashCommandArgumentPlan,
  buildSlashCommandArgumentSets,
  buildSlashCommandArguments,
  normalizeSearchField,
  queuePatternPieceCount,
} from "../src/discord/slash-command-input.mjs";

test("PC text grids are top-down, colorless, case-insensitive, and limited to 6x10", () => {
  assert.deepEqual(normalizeSearchField("..........\n....xT...."), {
    format: "occupied-field",
    sourceFormat: "grid",
    occupied: 48n,
    mask: "0000000000000030",
    height: 1,
  });
  assert.equal(normalizeSearchField("giotszjl##").mask, "00000000000003ff");
  assert.equal(normalizeSearchField("##________").mask, "0000000000000003");
  assert.equal(
    normalizeSearchField(Array(6).fill("..........").join("\n")).occupied,
    0n,
  );
  assert.throws(() => normalizeSearchField("........."), /exactly 10 columns/);
  assert.throws(() => normalizeSearchField("..........."), /exactly 10 columns/);
  assert.throws(
    () => normalizeSearchField(Array(7).fill("..........").join("\n")),
    /one through six rows|at most six rows|1.*6 rows/i,
  );
  assert.throws(
    () => normalizeSearchField("____?_____"),
    (error) => {
      assert.match(error.message, /contains '\?'.*use #.*filled.*_.*empty/);
      assert.doesNotMatch(error.message, /[+~■□]|\b[CGI]\b|\b[01]\b/);
      return true;
    },
  );
});

test("compact grid:row/row syntax avoids Discord rich-text line parsing", () => {
  assert.deepEqual(
    normalizeSearchField("grid:__________ / ####______".replaceAll(" ", "")),
    normalizeSearchField("__________\n####______"),
  );
  assert.equal(
    normalizeSearchField("grid:_#________/##_______#").mask,
    normalizeSearchField("_#________\n##_______#").mask,
  );
});

test("cover, colored, spin, and damage commands accept 24-row text grids", () => {
  const empty24 = textGrid(24);
  const target24 = textGrid(24, "....####..");
  const empty25 = textGrid(25);
  const target25 = textGrid(25, "....####..");
  const cases = [
    {
      name: "cover",
      valid: [
        { name: "base", value: empty24 },
        { name: "target", value: target24 },
        { name: "next", value: "I" },
      ],
      overflow: [
        { name: "base", value: empty25 },
        { name: "target", value: target25 },
        { name: "next", value: "I" },
      ],
    },
    {
      name: "setup",
      valid: [
        { name: "field", value: target24 },
        { name: "next", value: "I" },
      ],
      overflow: [
        { name: "field", value: target25 },
        { name: "next", value: "I" },
      ],
    },
    {
      name: "spin",
      valid: [
        { name: "field", value: empty24 },
        { name: "next", value: "T" },
      ],
      overflow: [
        { name: "field", value: empty25 },
        { name: "next", value: "T" },
      ],
    },
    {
      name: "damage",
      valid: [
        { name: "field", value: empty24 },
        { name: "next", value: "SI" },
      ],
      overflow: [
        { name: "field", value: empty25 },
        { name: "next", value: "SI" },
      ],
    },
  ];

  for (const fixture of cases) {
    const arguments_ = buildSlashCommandArguments(
      findSlashCommand(fixture.name),
      fixture.valid,
    );
    assert.equal(
      arguments_.some((value) => /^[0-9a-f]{60}$/.test(value)),
      true,
      `/${fixture.name} did not use the 240-bit field projection`,
    );
    if (fixture.name === "damage") {
      assert.deepEqual(arguments_.slice(0, 5), [
        "damage",
        "--board-mask-v1",
        "0".repeat(60),
        "--queue",
        "SI",
      ]);
      assert.equal(arguments_.includes("--initial-b2b"), false);
    }
    assert.throws(
      () => buildSlashCommandArguments(
        findSlashCommand(fixture.name),
        fixture.overflow,
      ),
      /one through twenty-four rows|at most 24 rows|1.*24 rows/i,
    );
  }
});

test("score-finder uses the six-row fixed-queue PC field and score inputs", () => {
  const arguments_ = buildSlashCommandArguments(findSlashCommand("score-finder"), [
    { name: "field", value: textGrid(6) },
    { name: "next", value: "sijstlzo" },
    { name: "lines", value: 5 },
    { name: "options", value: "initial_b2b=true" },
  ]);
  assert.deepEqual(arguments_, [
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
  assert.throws(
    () => buildSlashCommandArguments(findSlashCommand("score-finder"), [
      { name: "field", value: textGrid(7) },
      { name: "next", value: "SIJSTLZO" },
    ]),
    /one through six rows|at most six rows|1.*6 rows/i,
  );
  assert.throws(
    () => buildSlashCommandArguments(findSlashCommand("score-finder"), [
      { name: "field", value: textGrid(5) },
      { name: "next", value: "SIJSTLZO" },
      { name: "options", value: "initial_combo=1" },
    ]),
    /does not support options key 'initial_combo'/,
  );
});

test("spin-structure preserves inventory multiplicity and lowers every profile explicitly", () => {
  const command = findSlashCommand("spin-structure");
  const field = textGrid(24);
  const profiles = [
    "t-spins",
    "t-spins-plus",
    "all-mini",
    "all-mini-plus",
    "all-spin",
    "all-spin-plus",
  ];

  for (const profile of profiles) {
    assert.deepEqual(
      buildSlashCommandArguments(command, [
        { name: "pieces", value: "tTio" },
        { name: "field", value: field },
        { name: "lines", value: "2+" },
        { name: "profile", value: profile },
        { name: "kicktable", value: "srs-x" },
      ]),
      [
        "spin-structure",
        "--board-mask-v1",
        "0".repeat(60),
        "--pieces",
        "TTIO",
        "--lines",
        "2+",
        "--spin-profile",
        profile,
        "--rule",
        "srs-x",
      ],
    );
  }

  assert.deepEqual(
    buildSlashCommandArguments(command, [
      { name: "pieces", value: "TIO" },
      { name: "field", value: textGrid(8) },
    ]).slice(-6),
    ["--pieces", "TIO", "--lines", "1+", "--spin-profile", "t-spins"],
  );
  assert.throws(
    () => buildSlashCommandArguments(command, [
      { name: "pieces", value: "TIO" },
      { name: "field", value: textGrid(25) },
    ]),
    /one through twenty-four rows|at most 24 rows|1.*24 rows/i,
  );
  assert.throws(
    () => buildSlashCommandArguments(command, [
      { name: "pieces", value: "TIO" },
      { name: "field", value: textGrid(8) },
      { name: "profile", value: "combined" },
    ]),
    /profile must be/i,
  );
  assert.throws(
    () => buildSlashCommandArguments(command, [
      { name: "pieces", value: "TIO" },
      { name: "field", value: textGrid(8) },
      { name: "lines", value: "5+" },
    ]),
    /lines must be/i,
  );
});

test("automatic PC targets use field blocks and the exact required piece window", () => {
  assert.equal(queuePatternPieceCount("iotszjliotszjl"), 14);
  assert.equal(queuePatternPieceCount("iotszjliotszjli"), 15);
  assert.equal(queuePatternPieceCount("*p7,*p3"), 10);
  assert.equal(queuePatternPieceCount("ip7p3"), 11);
  assert.equal(queuePatternPieceCount("[^tsz]!p4"), 8);
  assert.equal(queuePatternPieceCount("[iosz]t"), 2);
  assert.equal(queuePatternPieceCount("*p 4"), 4);
  assert.throws(() => queuePatternPieceCount("* p4"), /followed immediately/);
  assert.throws(() => queuePatternPieceCount("[iosz] p2"), /unexpected bag token/);
  assert.throws(() => queuePatternPieceCount("ß"), /unsupported token/);
  assert.throws(() => queuePatternPieceCount("*p8"), /may not exceed seven/);
  assert.deepEqual(
    automaticPcLines({ occupied: 0n, pieceCount: 14 }),
    [2, 4],
  );
  assert.deepEqual(
    automaticPcLines({ occupied: 0n, pieceCount: 15 }),
    [2, 4, 6],
  );
  assert.deepEqual(
    automaticPcLines({ occupied: 0xfn << 40n, pieceCount: 14 }),
    [6],
  );
  assert.deepEqual(
    automaticPcLines({ occupied: 0xfn, pieceCount: 9 }),
    [2, 4],
  );
  assert.deepEqual(
    automaticPcLines({ occupied: 0x3n, pieceCount: 12 }),
    [1, 3, 5],
  );
  assert.deepEqual(
    automaticPcLines({ occupied: 0x3n << 20n, pieceCount: 7 }),
    [3],
  );
  assert.throws(
    () => automaticPcLines({ occupied: 0n, pieceCount: 4 }),
    /no valid automatic/,
  );
  assert.throws(
    () => automaticPcLines({ occupied: (1n << 20n) - 1n, pieceCount: 1 }),
    /no valid automatic/,
  );
});

test("PC slash lines can be explicit or expand to serial automatic targets", () => {
  // Display padding is not field height: only the highest occupied cell
  // constrains the target. This is also the Modal's default four-row value.
  const field = "..........\n..........\n..........\n..........";
  const auto = buildSlashCommandArgumentSets(findSlashCommand("path"), [
    { name: "next", value: "iotszjliotszjli" },
    { name: "field", value: field },
  ]);
  assert.deepEqual(auto.map((arguments_) => arguments_.slice(-2)), [
    ["--lines", "2"],
    ["--lines", "4"],
    ["--lines", "6"],
  ]);

  const oneAutomatic = buildSlashCommandArgumentPlan(findSlashCommand("path"), [
    { name: "next", value: "I" },
    { name: "field", value: "######____" },
  ]);
  assert.equal(oneAutomatic.automaticPcTargets, true);
  assert.equal(oneAutomatic.argumentSets.length, 1);
  assert.deepEqual(oneAutomatic.argumentSets[0].slice(-2), ["--lines", "1"]);

  for (const lineCount of [1, 3, 5]) {
    const explicit = buildSlashCommandArgumentPlan(findSlashCommand("path"), [
      { name: "next", value: "iotszjliotszjl" },
      { name: "field", value: "........##" },
      { name: "lines", value: lineCount },
    ]);
    assert.equal(explicit.automaticPcTargets, false);
    assert.equal(explicit.argumentSets.length, 1);
    assert.deepEqual(
      explicit.argumentSets[0].slice(-2),
      ["--lines", String(lineCount)],
    );
  }

  const highCells = Array(50).fill(null);
  highCells.splice(40, 4, "G", "G", "G", "G");
  const highField = buildSlashCommandArgumentSets(findSlashCommand("path"), [
    { name: "next", value: "iotszjliotszjl" },
    {
      name: "field",
      value: encodeCtk3({
        width: 10,
        pages: [{ height: 5, cells: highCells }],
      }),
    },
  ]);
  assert.deepEqual(highField.map((arguments_) => arguments_.slice(-2)), [
    ["--lines", "6"],
  ]);
});

test("structured PC options use CTK3 masks directly without Fumen conversion", () => {
  const ctk3 = encodeCtk3({
    width: 10,
    pages: [
      {
        height: 1,
        cells: ["G", "G", "G", "G", ...Array(6).fill(null)],
      },
    ],
  });

  const arguments_ = buildSlashCommandArguments(findSlashCommand("path"), [
    { name: "field", value: ctk3 },
    { name: "next", value: "*!" },
    { name: "options", value: "clear=2 hold=avoid" },
  ]);

  assert.deepEqual(arguments_, [
    "sfinder",
    "path",
    "--field-mask-v1",
    "000000000000000f",
    "--patterns",
    "*!",
    "--lines",
    "2",
    "--hold",
    "false",
  ]);
  assert.equal(arguments_.some((value) => value === "--fumen"), false);
  assert.equal(arguments_.some((value) => /^v115@/.test(value)), false);
});

test("kicktable accepts exactly four built-ins and lowers to the sfinder rule option", () => {
  const field = textGrid(4);
  for (const kicktable of ["srs-plus", "srs", "srs-x", "jstris-180"]) {
    const arguments_ = buildSlashCommandArguments(findSlashCommand("path"), [
      { name: "field", value: field },
      { name: "next", value: "IOTSZ" },
      { name: "lines", value: 2 },
      { name: "kicktable", value: kicktable },
    ]);
    const ruleIndex = arguments_.indexOf("--rule");
    assert.notEqual(ruleIndex, -1);
    assert.equal(arguments_[ruleIndex + 1], kicktable);
  }

  assert.throws(
    () => buildSlashCommandArguments(findSlashCommand("path"), [
      { name: "field", value: field },
      { name: "next", value: "IOTSZ" },
      { name: "lines", value: 2 },
      { name: "kicktable", value: "ars" },
    ]),
    /kicktable.*srs-plus.*srs.*srs-x.*jstris-180|unsupported kicktable/i,
  );
});

test("kicktable is available to every rule-aware Discord input contract", () => {
  const inputs = [
    ["cover", [
      { name: "base", value: textGrid(8) },
      { name: "target", value: textGrid(8, "....####..") },
      { name: "next", value: "I" },
    ]],
    ["setup", [
      { name: "field", value: textGrid(8, "....####..") },
      { name: "next", value: "I" },
    ]],
    ["spin", [
      { name: "field", value: textGrid(8) },
      { name: "next", value: "T" },
    ]],
    ["score-finder", [
      { name: "field", value: textGrid(6) },
      { name: "next", value: "TI" },
    ]],
    ["damage", [
      { name: "field", value: textGrid(8) },
      { name: "next", value: "TI" },
    ]],
    ["spin-structure", [
      { name: "pieces", value: "TTIO" },
      { name: "field", value: textGrid(8) },
    ]],
    ["pc-setup", [{ name: "remaining", value: "IOTS" }]],
  ];

  for (const [name, options] of inputs) {
    const arguments_ = buildSlashCommandArguments(findSlashCommand(name), [
      ...options,
      { name: "kicktable", value: "srs-plus" },
    ]);
    const ruleIndex = arguments_.indexOf("--rule");
    assert.notEqual(ruleIndex, -1, `/${name} omitted --rule`);
    assert.equal(arguments_[ruleIndex + 1], "srs-plus");
  }

  assert.throws(
    () => buildSlashCommandArguments(findSlashCommand("verify"), [
      { name: "scope", value: "pc" },
      { name: "kicktable", value: "srs" },
    ]),
    /unsupported option 'kicktable'/,
  );
});

test("raw Fumen and an encoded Fumen URL use the same colorless field decoder", () => {
  const source = fumenEncoder.encode([{ field: Field.create("TTT_______") }]);
  const url = `https://example.test/view?fumen=${encodeURIComponent(source)}`;

  assert.deepEqual(normalizeSearchField(source), {
    format: "occupied-field",
    sourceFormat: "fumen",
    occupied: 7n,
    mask: "0000000000000007",
    height: 1,
  });
  assert.deepEqual(normalizeSearchField(url), normalizeSearchField(source));
});

test("CTK3 stays single-page and cover accepts two colorless CTK3/Fumen fields", () => {
  const source = encodeCtk3({
    width: 10,
    pages: [
      { height: 1, cells: ["I", "I", "I", "I", ...Array(6).fill(null)] },
      { height: 1, cells: ["O", "O", "O", "O", ...Array(6).fill(null)] },
    ],
  });

  assert.throws(
    () =>
      buildSlashCommandArguments(findSlashCommand("path"), [
        { name: "field", value: source },
        { name: "next", value: "I" },
      ]),
    /exactly one page/,
  );

  const base = fumenEncoder.encode([{ field: Field.create("________XX") }]);
  const target = encodeCtk3({
    width: 10,
    pages: [{ height: 1, cells: ["I", "I", "I", "I", ...Array(6).fill(null)] }],
  });
  const arguments_ = buildSlashCommandArguments(findSlashCommand("cover"), [
    { name: "base", value: base },
    { name: "target", value: target },
    { name: "next", value: "I" },
    { name: "options", value: "hold=avoid" },
  ]);
  assert.deepEqual(arguments_, [
    "sfinder",
    "cover",
    "--base-mask-v1",
    `${"0".repeat(57)}300`,
    "--target-mask-v1",
    `${"0".repeat(59)}f`,
    "--patterns",
    "I",
    "--hold",
    "false",
  ]);
});

test("compressed multi-page CTK3 is rejected from metadata before full decoding", () => {
  const source = encodeCtk3({
    width: 10,
    pages: Array.from({ length: 4096 }, () => ({ height: 0, cells: [] })),
  });
  assert.equal(source.length < 100, true);
  assert.throws(() => normalizeSearchField(source), /exactly one page/);
});

test("CTK3 colors share one occupied-field projection", () => {
  const mixed = encodeCtk3({
    width: 10,
    pages: [
      {
        height: 1,
        cells: ["G", "I", "O", "T", "S", "Z", "J", "L", null, null],
      },
    ],
  });
  const grey = encodeCtk3({
    width: 10,
    pages: [
      {
        height: 1,
        cells: [...Array(8).fill("G"), null, null],
      },
    ],
  });
  assert.equal(normalizeSearchField(mixed).mask, "00000000000000ff");
  assert.deepEqual(normalizeSearchField(mixed), normalizeSearchField(grey));
});

test("Fumen grey and piece colors share the same occupied-field projection", () => {
  const mixed = fumenEncoder.encode([{ field: Field.create("XIOT______") }]);
  const grey = fumenEncoder.encode([{ field: Field.create("XXXX______") }]);
  assert.equal(normalizeSearchField(mixed).mask, "000000000000000f");
  assert.equal(normalizeSearchField(grey).mask, "000000000000000f");
});

test("CTK3 static-field projection rejects operations, garbage, and Board64 overflow", () => {
  const operation = encodeCtk3({
    width: 10,
    pages: [
      {
        height: 0,
        cells: [],
        operation: { piece: "T", rotation: "spawn", x: 4, y: 0 },
      },
    ],
  });
  assert.throws(() => normalizeSearchField(operation), /contains an operation/);

  const garbage = encodeCtk3({
    width: 10,
    pages: [
      {
        height: 0,
        cells: [],
        garbage: ["G", ...Array(9).fill(null)],
      },
    ],
  });
  assert.throws(() => normalizeSearchField(garbage), /non-empty garbage row/);

  const cells = Array(70).fill(null);
  cells[64] = "G";
  const overflow = encodeCtk3({
    width: 10,
    pages: [{ height: 7, cells }],
  });
  assert.throws(() => normalizeSearchField(overflow), /64-bit field range|6-row limit/);
});

test("cover rejects overlap, empty/non-tetromino targets, and completed base rows", () => {
  const field = (cells) => encodeCtk3({
    width: 10,
    pages: [{ height: Math.ceil(cells.length / 10), cells }],
  });
  const empty = field([]);
  const one = field(["G", ...Array(9).fill(null)]);
  const four = field(["G", "G", "G", "G", ...Array(6).fill(null)]);
  const full = field(Array(10).fill("G"));
  const run = (base, target) => buildSlashCommandArguments(findSlashCommand("cover"), [
    { name: "base", value: base },
    { name: "target", value: target },
    { name: "next", value: "I" },
  ]);

  assert.throws(() => run(empty, empty), /at least one occupied cell/);
  assert.throws(() => run(empty, one), /divisible by four/);
  assert.throws(() => run(four, four), /must not overlap/);
  assert.throws(() => run(full, field([
    ...Array(10).fill(null),
    "I", "I", "I", "I", ...Array(6).fill(null),
  ])), /completed row/);
});

test("legacy CTK3 query URLs keep payload punctuation intact", () => {
  const source = "ctk3@.:)aB*t&hPEXlYu:YoUl/4cH8Ga[PB0z";
  const url = `https://example.test/view?ctk=${encodeURIComponent(source)}`;
  assert.equal(normalizeSearchField(url).format, "occupied-field");
});

test("optional settings are command-specific and cannot override host policy", () => {
  const source = fumenEncoder.encode([{ field: Field.create() }]);
  const base = [
    { name: "field", value: source },
    { name: "next", value: "I" },
  ];

  assert.throws(
    () =>
      buildSlashCommandArguments(findSlashCommand("path"), [
        ...base,
        { name: "lines", value: 4 },
        { name: "options", value: "clear=4" },
      ]),
    /may not be specified together/,
  );
  assert.throws(
    () =>
      buildSlashCommandArguments(findSlashCommand("path"), [
        ...base,
        { name: "options", value: "workers=64" },
      ]),
    /does not support options key 'workers'/,
  );
  assert.throws(
    () =>
      buildSlashCommandArguments(findSlashCommand("path"), [
        ...base,
        { name: "options", value: "clear=4 lines=3" },
      ]),
    /may be specified only once/,
  );
  assert.throws(
    () =>
      buildSlashCommandArguments(findSlashCommand("spin"), [
        ...base,
        { name: "options", value: "type=TSM" },
      ]),
    /TSM is unavailable/,
  );
});

test("fixed queues, remaining inventories, and verify scopes fail closed", () => {
  const source = fumenEncoder.encode([{ field: Field.create() }]);
  assert.throws(
    () =>
      buildSlashCommandArguments(findSlashCommand("score-finder"), [
        { name: "field", value: source },
        { name: "next", value: "*!" },
      ]),
    /exact queue/,
  );
  assert.deepEqual(
    buildSlashCommandArguments(findSlashCommand("pc-setup"), [
      { name: "remaining", value: "tio" },
    ]),
    ["sfinder", "pc-setup", "TIO"],
  );
  assert.throws(
    () =>
      buildSlashCommandArguments(findSlashCommand("verify"), [
        { name: "scope", value: "files" },
      ]),
    /Verify scope/,
  );
});

test("unsupported CTK3 width and Fumen v110 fail before execution", () => {
  const source = encodeCtk3({
    width: 12,
    pages: [{ height: 0, cells: [] }],
  });
  assert.throws(() => normalizeSearchField(source), /exactly 10 columns/);
  assert.throws(
    () => normalizeSearchField("v110@vhAAgH"),
    /v110 Fumen is not supported/,
  );
});

function textGrid(rows, bottomRow = "..........") {
  assert.ok(Number.isSafeInteger(rows) && rows >= 1);
  assert.equal(bottomRow.length, 10);
  return [...Array(rows - 1).fill(".........."), bottomRow].join("\n");
}
