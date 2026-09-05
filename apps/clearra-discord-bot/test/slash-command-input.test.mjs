import assert from "node:assert/strict";
import test from "node:test";

import { encodeCtk3 } from "ctk3";
import { encoder as fumenEncoder, Field } from "tetris-fumen";

import {
  findSlashCommand,
  formatSlashCommandHelp,
} from "../src/discord/slash-command-catalog.mjs";
import {
  automaticPcLines,
  buildSlashCommandArgumentPlan,
  buildSlashCommandArgumentSets,
  buildSlashCommandArguments,
  normalizeFinesseDocument,
  normalizeSearchField,
  queuePatternPieceCount,
} from "../src/discord/slash-command-input.mjs";

test("advanced objective selection is absent from slash input contracts", () => {
  const pc = findSlashCommand("pc");
  for (const command of Object.values(pc.subcommands)) {
    assert.equal(
      command.registration.options.some(({ name }) => name === "objective"),
      false,
      command.subcommand,
    );
  }
  assert.throws(
    () => buildSlashCommandArguments(pc.subcommands.path, [
      { name: "field", value: "XXXXXX____" },
      { name: "next", value: "I" },
      { name: "lines", value: 1 },
      { name: "objective", value: "unique" },
    ]),
    /unsupported option 'objective'/,
  );
});

test("All-Spin PC slash contracts separate exact witnesses from pattern probability", () => {
  const pc = findSlashCommand("pc");
  const exact = pc.subcommands["allspin-sol"];
  const chance = pc.subcommands["allspin-pres-chance"];
  const optionNames = [
    "next", "field", "lines", "hold", "kicktable", "spin-profile",
    "max-patterns", "max-nodes", "max-frontier-states", "max-candidates",
    "max-memory-mib",
  ];
  assert.deepEqual(exact.registration.options.map(({ name }) => name), optionNames);
  assert.deepEqual(chance.registration.options.map(({ name }) => name), optionNames);
  for (const forbidden of [
    "target", "queue-knowledge", "score", "score-profile", "objective",
    "source-pieces", "solution-probabilities", "preserve-b2b",
  ]) {
    assert.equal(optionNames.includes(forbidden), false, forbidden);
  }

  const field = "grid:__________/####______";
  const base = [
    { name: "field", value: field },
    { name: "lines", value: 2 },
    { name: "spin-profile", value: "all-spin-plus" },
  ];
  const exactArguments = buildSlashCommandArguments(exact, [
    ...base,
    { name: "next", value: "iots" },
    { name: "hold", value: "off" },
    { name: "kicktable", value: "no-kick" },
    { name: "max-patterns", value: 3 },
    { name: "max-nodes", value: 5 },
    { name: "max-frontier-states", value: 7 },
    { name: "max-candidates", value: 11 },
    { name: "max-memory-mib", value: 13 },
  ]);
  assert.deepEqual(exactArguments, [
    "pc", "allspin-sol",
    "--lines", "2",
    "--board-mask", "0xf",
    "--height", "2",
    "--pieces", "4",
    "--queue", "IOTS",
    "--no-hold",
    "--spin-profile", "all-spin-plus",
    "--rule", "no-kick",
    "--max-patterns", "3",
    "--max-nodes", "5",
    "--max-frontier-states", "7",
    "--max-candidates", "11",
    "--max-memory-mib", "13",
  ]);
  assert.equal(exactArguments.includes("--preserve-b2b"), false);

  const chanceArguments = buildSlashCommandArguments(chance, [
    ...base,
    { name: "next", value: "[IOTS]!" },
    { name: "hold", value: "on" },
  ]);
  assert.deepEqual(chanceArguments.slice(0, 14), [
    "pc", "allspin-pres-chance",
    "--lines", "2",
    "--board-mask", "0xf",
    "--height", "2",
    "--pieces", "4",
    "--patterns", "[IOTS]!",
    "--spin-profile", "all-spin-plus",
  ]);
  assert.equal(chanceArguments.includes("--queue"), false);
  assert.equal(chanceArguments.includes("--hold"), false);
  assert.equal(chanceArguments.includes("--preserve-b2b"), false);

  const openingExact = buildSlashCommandArguments(exact, [
    { name: "field", value: "grid:__________/__________" },
    { name: "next", value: "iiooo" },
    { name: "lines", value: 2 },
    { name: "spin-profile", value: "all-spin-plus" },
  ]);
  assert.deepEqual(openingExact, [
    "pc", "allspin-sol",
    "--lines", "2",
    "--queue", "IIOOO",
    "--spin-profile", "all-spin-plus",
    "--rule", "srs-plus",
  ]);
  assert.equal(openingExact.includes("--board-mask"), false);
  assert.equal(openingExact.includes("--height"), false);
  assert.equal(openingExact.includes("--pieces"), false);

  const openingChance = buildSlashCommandArguments(chance, [
    { name: "field", value: "grid:__________/__________" },
    { name: "next", value: "[IO]!OOO" },
    { name: "lines", value: 2 },
    { name: "spin-profile", value: "all-mini-plus" },
    { name: "hold", value: "off" },
  ]);
  assert.deepEqual(openingChance, [
    "pc", "allspin-pres-chance",
    "--lines", "2",
    "--patterns", "[IO]!OOO",
    "--no-hold",
    "--spin-profile", "all-mini-plus",
    "--rule", "srs-plus",
  ]);

  const automaticOpening = buildSlashCommandArgumentPlan(exact, [
    { name: "field", value: "grid:__________/__________" },
    { name: "next", value: "IIOOO" },
    { name: "spin-profile", value: "t-spins" },
  ]);
  assert.equal(automaticOpening.automaticPcTargets, true);
  assert.equal(automaticOpening.argumentSets.length, 1);
  assert.equal(automaticOpening.argumentSets[0].includes("--board-mask"), false);
  assert.equal(
    automaticOpening.argumentSets[0][
      automaticOpening.argumentSets[0].indexOf("--lines") + 1
    ],
    "2",
  );

  assert.deepEqual(
    buildSlashCommandArgumentPlan(exact, [
      { name: "field", value: field },
      { name: "next", value: "IOTS" },
      { name: "spin-profile", value: "all-mini-plus" },
    ]).argumentSets.map((arguments_) =>
      arguments_[arguments_.indexOf("--lines") + 1]
    ),
    ["2"],
  );

  assert.throws(
    () => buildSlashCommandArguments(exact, [
      { name: "field", value: field },
      { name: "next", value: "[IOTS]!" },
      { name: "lines", value: 2 },
      { name: "spin-profile", value: "all-spin-plus" },
    ]),
    /exact queue/,
  );
  assert.throws(
    () => buildSlashCommandArguments(exact, [
      { name: "field", value: field },
      { name: "next", value: "IOTS" },
      { name: "lines", value: 2 },
    ]),
    /spin-profile input is required/,
  );
  assert.throws(
    () => buildSlashCommandArguments(exact, [
      ...base,
      { name: "next", value: "IOTS" },
      { name: "spin-profile", value: "t-spins" },
    ]),
    /spin-profile.*more than once/,
  );
  for (const [command, name, value] of [
    [exact, "target", "####______"],
    [exact, "patterns", "[IOTS]!"],
    [chance, "queue", "IOTS"],
    [exact, "preserve-b2b", "on"],
    [chance, "objective", "unique"],
    [chance, "solution-probabilities", "on"],
  ]) {
    assert.throws(
      () => buildSlashCommandArguments(command, [
        ...base,
        { name: "next", value: command === exact ? "IOTS" : "[IOTS]!" },
        { name, value },
      ]),
      /unsupported option/,
      name,
    );
  }
});

test("Build v2 cover source-pieces lowers once after hold", () => {
  const command = findSlashCommand("build").subcommands.cover;
  const common = [
    { name: "base-mask", value: "0x0" },
    { name: "target-mask", value: "0xf" },
    { name: "height", value: 1 },
    { name: "queue", value: "I" },
    { name: "hold", value: "T" },
  ];

  for (const value of [1, 4_294_967_295]) {
    const arguments_ = buildSlashCommandArguments(command, [
      ...common,
      { name: "source-pieces", value },
    ]);
    const sourceIndex = arguments_.indexOf("--source-pieces");
    assert.notEqual(sourceIndex, -1);
    assert.equal(arguments_[sourceIndex + 1], String(value));
    const holdIndex = arguments_.indexOf("--hold");
    assert.notEqual(holdIndex, -1);
    assert.equal(arguments_[holdIndex + 1], "T");
    assert.equal(
      arguments_.filter((argument) => argument === "--source-pieces").length,
      1,
    );
  }

  assert.equal(
    buildSlashCommandArguments(command, common).includes("--source-pieces"),
    false,
  );

  for (const value of [0, 4_294_967_296]) {
    assert.throws(
      () => buildSlashCommandArguments(command, [
        ...common,
        { name: "source-pieces", value },
      ]),
      /source-pieces must be an integer from 1 through 4294967295/,
    );
  }
  assert.throws(
    () => buildSlashCommandArguments(command, [
      ...common,
      { name: "source-pieces", value: 1 },
      { name: "source-pieces", value: 17 },
    ]),
    /source-pieces.*more than once/,
  );

});

test("Build v2 cover exposes only its closed mask-source option set", () => {
  const command = findSlashCommand("build").subcommands.cover;
  const optionNames = command.registration.options.map(({ name }) => name);
  assert.deepEqual(optionNames, [
    "base-mask",
    "target-mask",
    "height",
    "queue",
    "patterns",
    "hold",
    "queue-knowledge",
    "objective",
    "kicktable",
    "source-pieces",
  ]);
  const common = [
    { name: "base-mask", value: "0x0" },
    { name: "target-mask", value: "0xf" },
    { name: "height", value: 1 },
    { name: "queue", value: "I" },
  ];
  for (const name of [
    "aggregation",
    "spin-profile",
    "preserve-b2b",
    "solution-probabilities",
    "finesse",
    "finesse-knowledge",
    "mirror",
  ]) {
    assert.throws(
      () => buildSlashCommandArguments(command, [
        ...common,
        { name, value: "on" },
      ]),
      new RegExp(`unsupported option '${name}'`, "i"),
    );
  }
  assert.throws(
    () => buildSlashCommandArguments(command, [
      ...common,
      { name: "solution-probability", value: "on" },
    ]),
    /unsupported option 'solution-probability'/,
  );
});

test("Build probability exposes and lowers all seven CLI-owned result aggregations", () => {
  const command = findSlashCommand("build").subcommands.probability;
  assert.equal(command.capabilityId, "build.probability");
  assert.deepEqual(command.argvPrefix, ["build-probability"]);
  assert.deepEqual(
    command.registration.options.map(({ name }) => name),
    [
      "next",
      "base",
      "target",
      "kicktable",
      "height",
      "hold",
      "source-pieces",
      "aggregation",
      "result-mode",
      "spin-profile",
      "preserve-b2b",
      "solution-probabilities",
      "finesse",
      "finesse-knowledge",
      "mirror",
      "score-profile",
      "initial-b2b",
      "failed-count",
    ],
  );
  const common = [
    { name: "next", value: "I" },
    { name: "base", value: "__________" },
    { name: "target", value: "####______" },
  ];
  const modes = [
    "all-solutions",
    "complete-replay-paths",
    "minimum-solutions",
    "field-average-score",
    "fixed-queue-maximum-score",
    "highest-score-minimum-set",
    "failed-queues",
  ];
  for (const mode of modes) {
    const arguments_ = buildSlashCommandArguments(command, [
      ...common,
      { name: "result-mode", value: mode },
    ]);
    if (mode === "minimum-solutions") {
      assert.deepEqual(arguments_.slice(0, 2), ["build", "cover"]);
      assert.equal(arguments_.includes("--result-mode"), false);
      assert.deepEqual(
        arguments_.slice(arguments_.indexOf("--objective"), arguments_.indexOf("--objective") + 2),
        ["--objective", "min-cover"],
      );
      assert.deepEqual(
        arguments_.slice(arguments_.indexOf("--backend"), arguments_.indexOf("--backend") + 2),
        ["--backend", "cpu"],
      );
      assert.equal(arguments_.includes("--no-backend-fallback"), true);
    } else {
      assert.equal(arguments_[0], "build-probability");
      assert.equal(
        arguments_.includes("--result-mode"),
        mode !== "all-solutions",
      );
    }
    const scoreMode = [
      "field-average-score",
      "fixed-queue-maximum-score",
      "highest-score-minimum-set",
    ].includes(mode);
    assert.equal(arguments_.includes("--score-profile"), scoreMode);
    assert.equal(arguments_.includes("--initial-b2b"), scoreMode);
    assert.equal(arguments_.includes("--failed-count"), mode === "failed-queues");
  }
});

test("Build probability result-mode compatibility and mode-only inputs fail closed", () => {
  const command = findSlashCommand("build").subcommands.probability;
  const common = [
    { name: "next", value: "[I]!" },
    { name: "base", value: "__________" },
    { name: "target", value: "####______" },
  ];
  assert.throws(
    () => buildSlashCommandArguments(command, [
      ...common,
      { name: "result-mode", value: "fixed-queue-maximum-score" },
    ]),
    /exact queue/i,
  );
  assert.throws(
    () => buildSlashCommandArguments(command, [
      ...common,
      { name: "result-mode", value: "field-average-score" },
      { name: "aggregation", value: "spin" },
    ]),
    /Non-all Build result modes require aggregation=buildability/u,
  );
  assert.throws(
    () => buildSlashCommandArguments(command, [
      ...common,
      { name: "result-mode", value: "all-solutions" },
      { name: "score-profile", value: "tetrio" },
    ]),
    /require a score result-mode/u,
  );
  assert.throws(
    () => buildSlashCommandArguments(command, [
      ...common,
      { name: "result-mode", value: "all-solutions" },
      { name: "failed-count", value: 3 },
    ]),
    /requires result-mode=failed-queues/u,
  );
  assert.throws(
    () => buildSlashCommandArguments(command, [
      ...common,
      { name: "result-mode", value: "complete-replay-paths" },
      { name: "height", value: 7 },
    ]),
    /height from 1 through 6/u,
  );
});

test("finesse search forwards canonical masks, height, queue class, and policies", () => {
  const command = findSlashCommand("finesse");
  const direct = buildSlashCommandArguments(command, [{
    type: 1,
    name: "search",
    options: [
      { name: "target", value: "XXXX______" },
      { name: "next", value: "i" },
      { name: "base", value: "__________" },
      { name: "kicktable", value: "srs-x" },
    ],
  }]);
  assert.deepEqual(direct, [
    "build-probability",
    "--base-mask", "0".repeat(60),
    "--target-mask", `${"0".repeat(59)}f`,
    "--height", "8",
    "--queue", "I",
    "--hold", "empty",
    "--pattern-knowledge", "both",
    "--finesse", "inputs",
    "--no-mirror",
    "--rule", "srs-x",
  ]);

  const pattern = buildSlashCommandArguments(command.subcommands.search, [
    { name: "target", value: "XXXX______" },
    { name: "next", value: "*!" },
    { name: "base", value: "__________" },
    { name: "options", value: "hold=avoid knowledge=visible-7" },
  ]);
  assert.deepEqual(pattern.slice(-8), [
    "--patterns", "*!", "--no-hold", "--pattern-knowledge", "visible-7",
    "--finesse", "inputs", "--no-mirror",
  ]);
});

test("finesse search forwards completed base rows for core-only initial clearing", () => {
  const base = "__________\n##########";
  const target = "####______\n__________";
  const values = [
    { name: "base", value: base },
    { name: "target", value: target },
    { name: "next", value: "I" },
  ];
  const arguments_ = buildSlashCommandArguments(
    findSlashCommand("finesse").subcommands.search,
    values,
  );

  assert.deepEqual(arguments_.slice(0, 7), [
    "build-probability",
    "--base-mask", `${"0".repeat(57)}3ff`,
    "--target-mask", `${"0".repeat(56)}3c00`,
    "--height", "8",
  ]);
  assert.throws(
    () => buildSlashCommandArguments(findSlashCommand("cover"), values),
    /completed row/,
  );
});

test("finesse score canonicalizes operation documents without forwarding source text", () => {
  const source = encodeCtk3({
    width: 10,
    pages: [
      {
        height: 1,
        cells: ["G", ...Array(9).fill(null)],
        operation: { piece: "T", rotation: "spawn", x: 4, y: 1 },
      },
      {
        height: 0,
        cells: [],
        operation: { piece: "I", rotation: "right", x: 4, y: 2 },
      },
    ],
  });
  const normalized = normalizeFinesseDocument(source);
  assert.equal(normalized.initialMask, `${"0".repeat(59)}1`);
  assert.deepEqual(normalized.placements, ["T:spawn:3:1", "I:right:4:0"]);

  const arguments_ = buildSlashCommandArguments(
    findSlashCommand("finesse").subcommands.score,
    [
      { name: "document", value: source },
      { name: "next", value: "[TI]!" },
      { name: "options", value: "hold=use knowledge=full-queue" },
    ],
  );
  assert.equal(arguments_.includes(source), false);
  assert.deepEqual(arguments_.slice(0, 8), [
    "finesse", "score",
    "--initial-mask", `${"0".repeat(59)}1`,
    "--height", "4",
    "--placements", "T:spawn:3:1,I:right:4:0",
  ]);
  assert.deepEqual(arguments_.slice(8), [
    "--patterns", "[TI]!",
    "--hold", "empty",
    "--pattern-knowledge", "oracle",
  ]);
});

test("progressive Fumen keeps later operations in their post-clear page coordinates", () => {
  const source = fumenEncoder.encode([
    {
      field: Field.create("XXXXXXXXXX__________"),
      operation: { type: "O", rotation: "spawn", x: 1, y: 2 },
    },
    {
      operation: { type: "I", rotation: "spawn", x: 4, y: 1 },
    },
  ]);

  const normalized = normalizeFinesseDocument(source);

  assert.equal(normalized.initialMask, `${"0".repeat(55)}ffc00`);
  assert.equal(normalized.height, 4);
  assert.deepEqual(normalized.placements, ["O:spawn:1:2", "I:spawn:3:1"]);
});

test("finesse score requires an operation on every CTK3 or Fumen page", () => {
  const staticDocument = encodeCtk3({
    width: 10,
    pages: [{ height: 1, cells: ["G", ...Array(9).fill(null)] }],
  });
  assert.throws(
    () => normalizeFinesseDocument(staticDocument),
    /page 1 is missing its placement operation/,
  );
  assert.throws(
    () => normalizeFinesseDocument("XXXX______"),
    /must be one CTK3 or v115 Fumen document/,
  );
});

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
      searchCommand(fixture.name),
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
        searchCommand(fixture.name),
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
  const command = searchCommand("spin-structure");
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
        { name: "spin-profile", value: profile },
        { name: "kicktable", value: "srs-x" },
      ]),
      [
        "spin-structure",
        "search",
        "--board-mask-v1",
        "0".repeat(60),
        "--pieces",
        "TTIO",
        "--height",
        "8",
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
    ]).slice(-8),
    ["--pieces", "TIO", "--height", "8", "--lines", "1+", "--spin-profile", "t-spins"],
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
      { name: "spin-profile", value: "combined" },
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
    { name: "lines", value: 2 },
    { name: "options", value: "hold=avoid" },
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
    const arguments_ = buildSlashCommandArguments(searchCommand(name), [
      ...options,
      { name: "kicktable", value: "srs-plus" },
    ]);
    const ruleIndex = arguments_.indexOf("--rule");
    assert.notEqual(ruleIndex, -1, `/${name} omitted --rule`);
    assert.equal(arguments_[ruleIndex + 1], "srs-plus");
  }

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
    "build-probability",
    "--base-mask",
    `${"0".repeat(57)}300`,
    "--target-mask",
    `${"0".repeat(59)}f`,
    "--height",
    "1",
    "--queue",
    "I",
    "--no-hold",
    "--no-mirror",
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
    /does not support options key 'clear'/,
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
    /does not support options key 'clear'/,
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

test("fixed queues and remaining inventories fail closed", () => {
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
    ["setup-finder", "--remaining", "TIO", "--priority", "all"],
  );
});

test("setup ranking commands preserve their defaults and expose every canonical setting", () => {
  for (const [name, priority] of [
    ["pc-setup", "all"],
    ["best-setup", "build"],
    ["dpc-finder", "pc"],
  ]) {
    assert.deepEqual(
      buildSlashCommandArguments(findSlashCommand(name), [
        { name: "remaining", value: "IOTS" },
      ]),
      ["setup-finder", "--remaining", "IOTS", "--priority", priority],
    );
  }

  assert.deepEqual(
    buildSlashCommandArguments(findSlashCommand("pc-setup"), [
      { name: "remaining", value: "iots" },
      { name: "priority", value: "all" },
      { name: "max-setup-pieces", value: 10 },
      { name: "queue-knowledge", value: "visible-7" },
      { name: "next-cycle-remaining", value: "z" },
      { name: "setup-length", value: "shorter" },
      { name: "kicktable", value: "srs-x" },
    ]),
    [
      "setup-finder",
      "--remaining", "IOTS",
      "--priority", "all",
      "--max-setup-pieces", "10",
      "--queue-knowledge", "visible-7",
      "--next-cycle-remaining", "Z",
      "--setup-length", "shorter",
      "--rule", "srs-x",
    ],
  );
});

test("setup ranking inventories and enums fail closed before execution", () => {
  const build = (options) =>
    buildSlashCommandArguments(findSlashCommand("pc-setup"), options);
  for (const remaining of ["IOTSZJLI", "IIIO", "IIOO"]) {
    assert.throws(
      () => build([{ name: "remaining", value: remaining }]),
      /1 through 7|at most one piece kind twice/,
    );
  }
  assert.throws(
    () => build([
      { name: "remaining", value: "IOTS" },
      { name: "next-cycle-remaining", value: "ZJ" },
    ]),
    /exactly 1 piece/,
  );
  assert.throws(
    () => build([
      { name: "remaining", value: "IOT" },
      { name: "next-cycle-remaining", value: "IIOOTSZ" },
    ]),
    /at most one piece kind twice/,
  );
  assert.throws(
    () => build([
      { name: "remaining", value: "IOTS" },
      { name: "next-cycle-remaining", value: "Q" },
    ]),
    /IOTSZJL/,
  );
  for (const [name, value, message] of [
    ["priority", "fast", /fixes priority=all/],
    ["max-setup-pieces", 11, /integer from 1 through 10/],
    ["queue-knowledge", "both", /queue-knowledge must be/],
    ["setup-length", "medium", /setup-length must be/],
  ]) {
    assert.throws(
      () => build([
        { name: "remaining", value: "IOTS" },
        { name, value },
      ]),
      message,
    );
  }
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

test("sequence dependencies lowers one concrete document with exact defaults and no queue or hold", () => {
  const command = findSlashCommand("utility").subcommands["sequence-dependencies"];
  const source = encodeCtk3({
    width: 10,
    pages: [{
      height: 0,
      cells: [],
      operation: { piece: "O", rotation: "spawn", x: 1, y: 0 },
      flags: { lock: true },
    }],
  });

  assert.deepEqual(command.argvPrefix, ["utility", "sequence-dependencies"]);
  assert.deepEqual(command.resultAllowlist, ["sequence-dependencies"]);
  assert.deepEqual(
    command.registration.options.map(({ name }) => name),
    ["document", "attachment", "rule-profile", "kick-profile", "timeout-seconds"],
  );
  assert.deepEqual(buildSlashCommandArguments(command, [
    { name: "document", value: source },
  ]), [
    "utility", "sequence-dependencies",
    "--document", source,
    "--rule-profile", "srs-plus",
    "--kick-profile", "srs-plus",
    "--timeout-seconds", "900",
  ]);
  assert.deepEqual(buildSlashCommandArguments(command, [
    { name: "document", value: source },
    { name: "rule-profile", value: "srs-x" },
    { name: "kick-profile", value: "no-kick" },
    { name: "timeout-seconds", value: 17 },
  ]).slice(-6), [
    "--rule-profile", "srs-x",
    "--kick-profile", "no-kick",
    "--timeout-seconds", "17",
  ]);

  assert.throws(
    () => buildSlashCommandArguments(command, [
      { name: "document", value: source },
      { name: "queue", value: "O" },
    ]),
    /unsupported option 'queue'/,
  );
  assert.throws(
    () => buildSlashCommandArguments(command, [
      { name: "attachment", value: "unresolved" },
    ]),
    /resolved to its bounded CTK3 document/,
  );
  assert.throws(
    () => buildSlashCommandArguments(command, [
      { name: "document", value: source },
      { name: "timeout-seconds", value: 901 },
    ]),
    /between 1 and 900|1.*900/,
  );
  const staticDocument = encodeCtk3({
    width: 10,
    pages: [{ height: 0, cells: [], flags: { lock: true } }],
  });
  assert.throws(
    () => buildSlashCommandArguments(command, [
      { name: "document", value: staticDocument },
    ]),
    /missing its concrete operation/,
  );
});

test("sequence lowers one authoritative operation trace with exact defaults and no queue or hold", () => {
  const command = findSlashCommand("utility").subcommands.sequence;
  const source = encodeCtk3({
    width: 10,
    pages: [{
      height: 0,
      cells: [],
      operation: { piece: "O", rotation: "spawn", x: 1, y: 0 },
      flags: { lock: true },
    }],
  });

  assert.deepEqual(command.argvPrefix, ["utility", "sequence"]);
  assert.deepEqual(command.resultAllowlist, ["sequence"]);
  assert.deepEqual(
    command.registration.options.map(({ name }) => name),
    ["document", "attachment", "rule-profile", "kick-profile", "timeout-seconds"],
  );
  assert.deepEqual(buildSlashCommandArguments(command, [
    { name: "document", value: source },
  ]), [
    "utility", "sequence",
    "--document", source,
    "--rule-profile", "srs-plus",
    "--kick-profile", "srs-plus",
    "--timeout-seconds", "900",
  ]);
  assert.deepEqual(buildSlashCommandArguments(command, [
    { name: "document", value: source },
    { name: "rule-profile", value: "srs-x" },
    { name: "kick-profile", value: "no-kick" },
    { name: "timeout-seconds", value: 17 },
  ]).slice(-6), [
    "--rule-profile", "srs-x",
    "--kick-profile", "no-kick",
    "--timeout-seconds", "17",
  ]);

  for (const option of ["queue", "hold"]) {
    assert.throws(
      () => buildSlashCommandArguments(command, [
        { name: "document", value: source },
        { name: option, value: option === "queue" ? "O" : "on" },
      ]),
      new RegExp(`unsupported option '${option}'`),
    );
  }
  assert.throws(
    () => buildSlashCommandArguments(command, [
      { name: "attachment", value: "unresolved" },
    ]),
    /resolved to its bounded CTK3 document/,
  );
  assert.throws(
    () => buildSlashCommandArguments(command, [
      { name: "document", value: source },
      { name: "timeout-seconds", value: 901 },
    ]),
    /between 1 and 900|1.*900/,
  );
  const staticDocument = encodeCtk3({
    width: 10,
    pages: [{ height: 0, cells: [], flags: { lock: true } }],
  });
  assert.throws(
    () => buildSlashCommandArguments(command, [
      { name: "document", value: staticDocument },
    ]),
    /missing its concrete operation/,
  );
});

test("to-gray and mirror lower one typed field document without search inference", () => {
  const source = encodeCtk3({
    width: 4,
    pages: [{
      height: 1,
      cells: ["J", null, "S", "G"],
      comment: "identity",
      garbage: ["L", null, "Z", "G"],
      operation: { piece: "T", rotation: "right", x: 1, y: 0 },
    }],
  });
  const utility = findSlashCommand("utility");
  for (const name of ["to-gray", "mirror"]) {
    const command = utility.subcommands[name];
    assert.deepEqual(command.argvPrefix, ["utility", name]);
    assert.deepEqual(command.resultAllowlist, ["field-document.v1"]);
    assert.deepEqual(
      command.registration.options.map(({ name: option }) => option),
      ["document", "attachment"],
    );
    assert.deepEqual(buildSlashCommandArguments(command, [
      { name: "document", value: source },
    ]), ["utility", name, "--document", source]);
    for (const option of ["queue", "hold", "workers"]) {
      assert.throws(
        () => buildSlashCommandArguments(command, [
          { name: "document", value: source },
          { name: option, value: option === "queue" ? "O" : 1 },
        ]),
        new RegExp(`unsupported option '${option}'`),
      );
    }
  }
});

function searchCommand(name) {
  if (name === "setup") return findSlashCommand("build").subcommands.setup;
  if (name === "spin-structure") {
    return findSlashCommand("spin-structure").subcommands.search;
  }
  return findSlashCommand(name);
}

function textGrid(rows, bottomRow = "..........") {
  assert.ok(Number.isSafeInteger(rows) && rows >= 1);
  assert.equal(bottomRow.length, 10);
  return [...Array(rows - 1).fill(".........."), bottomRow].join("\n");
}
