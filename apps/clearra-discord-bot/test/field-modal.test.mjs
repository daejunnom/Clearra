import assert from "node:assert/strict";
import test from "node:test";

import {
  buildMissingBoardModalResponse,
  findFieldModalCommand,
  readCommandModalLocale,
  readFieldModalOptions,
} from "../src/discord/field-modal.mjs";
import {
  findSlashCommand,
  slashCommandCatalog,
} from "../src/discord/slash-command-catalog.mjs";

const EMPTY_FOUR_ROWS = emptyGrid(4);
const EMPTY_EIGHT_ROWS = emptyGrid(8);
const BUILTIN_KICKTABLES = ["srs-plus", "srs", "srs-x", "jstris-180"];
const SPIN_STRUCTURE_LINES = [
  "any",
  "0",
  "1",
  "2",
  "3",
  "4",
  "1+",
  "2+",
  "3+",
  "4+",
];
const SPIN_STRUCTURE_PROFILES = [
  "t-spins",
  "t-spins-plus",
  "all-mini",
  "all-mini-plus",
  "all-spin",
  "all-spin-plus",
];

const PC_COMMANDS = [
  "path",
  "percent",
  "chance",
  "minimals",
  "score",
  "score-minimals",
  "saves",
  "best-save",
];
const COLORED_COMMANDS = [
  "setup",
  "congruent",
  "congruent-cover",
  "setup-cover",
  "cover-percent",
  "special-cover",
];
const SPIN_COMMANDS = ["spin-cover", "spin"];
const REMAINING_COMMANDS = ["pc-setup", "best-setup", "dpc-finder"];

test("every ungrouped bare search command opens a v4 search Modal while help stays direct", () => {
  for (const command of slashCommandCatalog.filter(({ kind, input }) => kind === "search" && input !== "finesse")) {
    const response = buildMissingBoardModalResponse(slashInteraction(command.name));
    assert.equal(response?.type, 9, `/${command.name} must open a Modal`);
    assert.equal(response.data.custom_id, `clearra:search:v4:${command.name}`);
    assert.ok(
      response.data.components.length <= 5,
      `/${command.name} exceeded Discord's five-component Modal limit`,
    );
    assert.equal(
      findFieldModalCommand(modalInteraction(response.data.custom_id, []))?.name,
      command.name,
    );
  }

  for (const subcommand of ["search", "score"]) {
    const response = buildMissingBoardModalResponse(
      slashInteraction("finesse", [{ type: 1, name: subcommand, options: [] }]),
    );
    assert.equal(response?.type, 9);
    assert.equal(response.data.custom_id, `clearra:search:v4:finesse~${subcommand}`);
    const command = findFieldModalCommand(modalInteraction(response.data.custom_id, []));
    assert.equal(command?.name, "finesse");
    assert.equal(command?.subcommand, subcommand);
  }

  assert.equal(buildMissingBoardModalResponse(slashInteraction("help")), null);
});

test("render-file stays direct and removed render Modal routes stay inactive", () => {
  assert.equal(buildMissingBoardModalResponse(slashInteraction("render-file")), null);
  assert.equal(buildMissingBoardModalResponse(slashInteraction("render")), null);
  assert.equal(
    findFieldModalCommand(modalInteraction("clearra:search:v3:render", [])),
    null,
  );
});

test("finesse subcommands use bounded localized guided forms", () => {
  const search = buildMissingBoardModalResponse(
    slashInteraction("finesse", [{ type: 1, name: "search", options: [] }]),
    "ko",
  );
  assert.equal(search.data.title, "피네스 탐색 입력");
  assert.deepEqual(componentNames(search), ["target", "next", "base", "kicktable", "options"]);
  assert.equal(search.data.components.length, 5);
  assertStringSelect(
    component(search, "options"),
    [
      "hold=use knowledge=both",
      "hold=use knowledge=oracle",
      "hold=use knowledge=visible-7",
      "hold=avoid knowledge=both",
      "hold=avoid knowledge=oracle",
      "hold=avoid knowledge=visible-7",
    ],
    "hold=use knowledge=both",
  );

  const score = buildMissingBoardModalResponse(
    slashInteraction("finesse", [{ type: 1, name: "score", options: [] }]),
    "en",
  );
  assert.equal(score.data.title, "finesse score form");
  assert.deepEqual(componentNames(score), ["document", "next", "kicktable", "options", "locale"]);
  assert.equal(score.data.components.length, 5);

  const scoreSubmit = modalInteraction(score.data.custom_id, [
    textLabel("document", "ctk3_example"),
    textLabel("next", "[TI]!"),
    selectLabel("kicktable", ["srs-plus"]),
    selectLabel("options", ["hold=avoid knowledge=oracle"]),
    selectLabel("locale", ["en"]),
  ]);
  const scoreCommand = findFieldModalCommand(scoreSubmit);
  assert.equal(scoreCommand.subcommand, "score");
  assert.equal(readCommandModalLocale(scoreSubmit), "en");
  assert.deepEqual(optionsByName(readFieldModalOptions(scoreSubmit, scoreCommand)), {
    document: "ctk3_example",
    next: "[TI]!",
    kicktable: "srs-plus",
    options: "hold=avoid knowledge=oracle",
  });
});

test("missing inputs and rich-text multi-line boards open the Modal", () => {
  for (const command of slashCommandCatalog.filter(({ kind, input }) => kind === "search" && input !== "finesse")) {
    const complete = completeOptions(command.input);
    const response = buildMissingBoardModalResponse(
      slashInteraction(command.name, complete),
    );
    const hasMultilineBoard = complete.some(({ name, value }) =>
      ["field", "base", "target"].includes(name) && String(value).includes("\n")
    );
    assert.equal(
      response?.type ?? null,
      hasMultilineBoard ? 9 : null,
      `/${command.name} used the wrong direct-input boundary`,
    );

    const missing = complete.slice(1);
    assert.equal(
      buildMissingBoardModalResponse(slashInteraction(command.name, missing))?.type,
      9,
      `/${command.name} with a missing runtime input must open a Modal`,
    );
  }

  assert.equal(
    buildMissingBoardModalResponse(slashInteraction("spin-structure", [
      { name: "pieces", value: "IOTS" },
      { name: "field", value: "grid:__________ / ####______".replaceAll(" ", "") },
    ])),
    null,
    "compact single-line grids must remain direct slash options",
  );
});

test("v4 Modal layouts are localized where capacity allows and never exceed five components", () => {
  for (const name of PC_COMMANDS) {
    assert.deepEqual(componentNames(modalFor(name)), [
      "field",
      "next",
      "lines",
      "kicktable",
      "options",
    ]);
  }

  assert.deepEqual(componentNames(modalFor("cover")), [
    "base",
    "target",
    "next",
    "kicktable",
    "options",
  ]);

  for (const name of COLORED_COMMANDS) {
    assert.deepEqual(componentNames(modalFor(name)), [
      "field",
      "next",
      "kicktable",
      "locale",
    ]);
  }
  for (const name of SPIN_COMMANDS) {
    assert.deepEqual(componentNames(modalFor(name)), [
      "field",
      "next",
      "kicktable",
      "options",
      "locale",
    ]);
  }
  assert.deepEqual(componentNames(modalFor("score-finder")), [
    "field",
    "next",
    "lines",
    "kicktable",
    "options",
  ]);
  assert.deepEqual(componentNames(modalFor("damage")), [
    "field",
    "next",
    "kicktable",
    "locale",
  ]);
  assert.deepEqual(componentNames(modalFor("spin-structure")), [
    "pieces",
    "field",
    "lines",
    "profile",
    "kicktable",
  ]);
  for (const name of REMAINING_COMMANDS) {
    assert.deepEqual(componentNames(modalFor(name)), [
      "remaining",
      "kicktable",
      "priority",
      "max-setup-pieces",
      "queue-knowledge",
    ]);
  }
  assert.deepEqual(componentNames(modalFor("verify")), ["scope", "locale"]);
});

test("setup ranking Modals preserve command defaults within Discord's five-component cap", () => {
  for (const [name, priority] of [
    ["pc-setup", "all"],
    ["best-setup", "build"],
    ["dpc-finder", "pc"],
  ]) {
    const modal = modalFor(name);
    assertStringSelect(component(modal, "priority"), ["all", "build", "pc"], priority);
    assertStringSelect(
      component(modal, "max-setup-pieces"),
      Array.from({ length: 10 }, (_, index) => String(index + 1)),
      "9",
    );
    assertStringSelect(
      component(modal, "queue-knowledge"),
      ["oracle", "visible-7"],
      "oracle",
    );
    assert.equal(componentNames(modal).includes("next-cycle-remaining"), false);
    assert.equal(componentNames(modal).includes("setup-length"), false);
    assert.equal(componentNames(modal).includes("locale"), false);
  }

  const korean = modalFor("best-setup", [], "ko");
  assert.match(label(korean, "priority").label, /셋업 정렬/u);
  assert.match(label(korean, "max-setup-pieces").description, /1–10개/u);
  assert.deepEqual(
    component(korean, "queue-knowledge").options.map(({ label: text }) => text),
    ["전체 미래 큐 (기본값)", "공개 7개"],
  );

  const preselected = modalFor("pc-setup", [
    { name: "kicktable", value: "srs-x" },
    { name: "priority", value: "build" },
    { name: "max-setup-pieces", value: 8 },
    { name: "queue-knowledge", value: "visible-7" },
  ]);
  assertStringSelect(component(preselected, "kicktable"), [
    "srs-plus", "srs", "srs-x", "jstris-180",
  ], "srs-x");
  assertStringSelect(component(preselected, "priority"), ["all", "build", "pc"], "build");
  assertStringSelect(
    component(preselected, "max-setup-pieces"),
    Array.from({ length: 10 }, (_, index) => String(index + 1)),
    "8",
  );
  assertStringSelect(
    component(preselected, "queue-knowledge"),
    ["oracle", "visible-7"],
    "visible-7",
  );
});

test("setup ranking Modal submits explicit defaults and rejects hidden-option loss", () => {
  const modal = modalFor("best-setup");
  const interaction = modalInteraction(modal.data.custom_id, [
    textLabel("remaining", "IOTS"),
    selectLabel("kicktable", ["srs-plus"]),
    selectLabel("priority", ["build"]),
    selectLabel("max-setup-pieces", ["9"]),
    selectLabel("queue-knowledge", ["oracle"]),
  ]);
  const command = findFieldModalCommand(interaction);
  assert.deepEqual(optionsByName(readFieldModalOptions(interaction, command)), {
    remaining: "IOTS",
    priority: "build",
    "max-setup-pieces": 9,
    "queue-knowledge": "oracle",
    kicktable: "srs-plus",
  });

  for (const name of ["next-cycle-remaining", "setup-length"]) {
    assert.throws(
      () => buildMissingBoardModalResponse(slashInteraction("pc-setup", [
        { name, value: name === "setup-length" ? "longer" : "Z" },
      ])),
      /remaining must also be supplied directly/,
    );
  }
});

test("PC fields default to four rows and other board searches default to eight", () => {
  for (const name of PC_COMMANDS) {
    assert.equal(component(modalFor(name), "field").value, EMPTY_FOUR_ROWS);
  }

  const eightRowFields = [
    ["cover", "base"],
    ["cover", "target"],
    ...COLORED_COMMANDS.map((name) => [name, "field"]),
    ...SPIN_COMMANDS.map((name) => [name, "field"]),
    ["damage", "field"],
    ["spin-structure", "field"],
  ];
  for (const [name, input] of eightRowFields) {
    assert.equal(component(modalFor(name), input).value, EMPTY_EIGHT_ROWS);
  }
  assert.equal(component(modalFor("score-finder"), "field").value, EMPTY_FOUR_ROWS);
});

test("v4 lines, kicktable, hold, spin type, language, and verify scope use string selects", () => {
  const path = modalFor("path");
  assertStringSelect(
    component(path, "lines"),
    ["auto", "1", "2", "3", "4", "5", "6"],
    "auto",
  );
  assert.match(component(path, "lines").options[0].label, /rows 1–6/);
  assertStringSelect(component(path, "kicktable"), BUILTIN_KICKTABLES, "srs-plus");
  assertStringSelect(
    component(path, "options"),
    ["hold=use", "hold=avoid"],
    "hold=use",
  );

  const cover = modalFor("cover");
  assertStringSelect(component(cover, "kicktable"), BUILTIN_KICKTABLES, "srs-plus");
  assertStringSelect(
    component(cover, "options"),
    ["hold=use", "hold=avoid"],
    "hold=use",
  );

  const spin = modalFor("spin");
  assertStringSelect(component(spin, "kicktable"), BUILTIN_KICKTABLES, "srs-plus");
  assertStringSelect(
    component(spin, "options"),
    [
      "type=TSS",
      "type=TSD",
      "type=TST",
      "type=ANY",
    ],
    "type=TSS",
  );

  const scoreFinder = modalFor("score-finder");
  assertStringSelect(
    component(scoreFinder, "lines"),
    ["1", "2", "3", "4", "5", "6"],
    "4",
  );
  assertStringSelect(
    component(scoreFinder, "options"),
    ["initial_b2b=false", "initial_b2b=true"],
    "initial_b2b=false",
  );

  const spinStructure = modalFor("spin-structure");
  assertStringSelect(
    component(spinStructure, "lines"),
    SPIN_STRUCTURE_LINES,
    "1+",
  );
  assertStringSelect(
    component(spinStructure, "profile"),
    SPIN_STRUCTURE_PROFILES,
    "t-spins",
  );
  assertStringSelect(
    component(spinStructure, "kicktable"),
    BUILTIN_KICKTABLES,
    "srs-plus",
  );
  assert.equal(componentNames(spinStructure).includes("locale"), false);

  assertStringSelect(
    component(modalFor("verify"), "scope"),
    ["all", "pc", "setup", "cover", "build", "kicks"],
    "all",
  );
  assertStringSelect(component(modalFor("verify"), "locale"), ["en", "ko"], "en");

  const koreanVerify = modalFor("verify", [], "ko");
  assert.equal(koreanVerify.data.title, "검증 탐색 입력");
  assert.deepEqual(
    component(koreanVerify, "scope").options.map(({ label: text }) => text),
    ["전체 검증", "퍼펙트 클리어", "셋업", "커버리지", "빌드", "킥"],
  );
  assert.deepEqual(
    component(koreanVerify, "locale").options.map(({ label: text }) => text),
    ["영어", "한국어"],
  );
  assert.deepEqual(
    component(modalFor("verify"), "locale").options.map(({ label: text }) => text),
    ["English", "Korean"],
  );

  const koreanPath = modalFor("path", [], "ko");
  assert.deepEqual(
    component(koreanPath, "kicktable").options.map(({ label: text }) => text),
    ["SRS+ (기본값)", "SRS", "SRS-X", "Jstris 180"],
  );

  const koreanSpin = modalFor("spin", [], "ko");
  assert.deepEqual(
    component(koreanSpin, "options").options.map(({ label: text }) => text),
    ["T-spin 싱글", "T-spin 더블", "T-spin 트리플", "모든 T-spin"],
  );

  const koreanScoreFinder = modalFor("score-finder", [], "ko");
  assert.deepEqual(
    component(koreanScoreFinder, "options").options.map(({ label: text }) => text),
    ["초기 B2B 사용 안 함 (기본값)", "초기 B2B 사용"],
  );
  assert.equal(componentNames(koreanScoreFinder).includes("locale"), false);
  assert.equal(componentNames(modalFor("damage", [], "ko")).includes("locale"), true);

  const koreanSpinStructure = modalFor("spin-structure", [], "ko");
  assert.equal(koreanSpinStructure.data.title, "스핀-구조 탐색 입력");
  assert.deepEqual(
    component(koreanSpinStructure, "lines").options.map(({ label: text }) => text),
    [
      "모든 줄 수",
      "정확히 0줄",
      "정확히 1줄",
      "정확히 2줄",
      "정확히 3줄",
      "정확히 4줄",
      "최소 1줄",
      "최소 2줄",
      "최소 3줄",
      "최소 4줄",
    ],
  );
  assert.deepEqual(
    component(koreanSpinStructure, "profile").options.map(({ label: text }) => text),
    ["T 스핀", "T 스핀+", "전체 Mini", "전체 Mini+", "전체 스핀", "전체 스핀+"],
  );
  assert.match(label(koreanSpinStructure, "profile").description, /Regular.*Mini.*분리/u);
  assert.match(label(spinStructure, "profile").description, /Regular.*Mini separate/);
});

test("board forms prefer keyboard # and _ cells without advertising G", () => {
  const english = modalFor("path", [], "en");
  const korean = modalFor("path", [], "ko");
  const englishField = label(english, "field");
  const koreanField = label(korean, "field");
  const englishBuildField = label(modalFor("setup", [], "en"), "field");
  const koreanBuildField = label(modalFor("setup", [], "ko"), "field");

  assert.equal(englishField.component.value, EMPTY_FOUR_ROWS);
  assert.match(englishField.label, /1–6 rows/);
  assert.match(koreanField.label, /1–6줄/u);
  assert.match(englishBuildField.label, /1–24 rows/);
  assert.match(koreanBuildField.label, /1–24줄/u);
  assert.doesNotMatch(englishField.label, /prefill|default/i);
  assert.doesNotMatch(englishBuildField.label, /prefill|default/i);
  assert.doesNotMatch(koreanField.label, /기본/u);
  assert.doesNotMatch(koreanBuildField.label, /기본/u);
  assert.match(englishField.description, /# for filled and _ for empty/);
  assert.doesNotMatch(englishField.description, /(?:^|\W)G(?:\W|$)/);
  assert.match(koreanField.description, /#은 채움, _는 빈칸/);
  assert.equal(korean.data.title, "경로 탐색 입력");

  const englishScoreLines = label(modalFor("score-finder", [], "en"), "lines");
  const koreanScoreLines = label(modalFor("score-finder", [], "ko"), "lines");
  assert.match(englishScoreLines.description, /1 through 6 rows/);
  assert.match(koreanScoreLines.description, /1–6줄/u);
  assert.doesNotMatch(englishScoreLines.description, /default/i);
  assert.doesNotMatch(koreanScoreLines.description, /기본/u);
});

test("v4 Modal language selection is explicit only when the layout has capacity", () => {
  const verifyModal = modalFor("verify", [], "ko");
  const verifySubmit = modalInteraction(verifyModal.data.custom_id, [
    selectLabel("scope", ["all"]),
    selectLabel("locale", ["ko"]),
  ]);
  assert.equal(readCommandModalLocale(verifySubmit), "ko");
  assert.match(verifyModal.data.title, /입력/u);

  const pathModal = modalFor("path", [], "ko");
  assert.equal(componentNames(pathModal).includes("locale"), false);
  const pathSubmit = modalInteraction(pathModal.data.custom_id, [
    textLabel("field", ".........."),
    textLabel("next", "I"),
    selectLabel("lines", ["auto"]),
    selectLabel("kicktable", ["srs-plus"]),
    selectLabel("options", ["hold=use"]),
  ]);
  assert.equal(readCommandModalLocale(pathSubmit), null);
});

test("v4 submit reads select values arrays and omits automatic lines", () => {
  const customId = modalFor("path").data.custom_id;
  const interaction = modalInteraction(customId, [
    textLabel("field", "..........\n....xx...."),
    textLabel("next", "[iosz]p2"),
    selectLabel("lines", ["auto"]),
    selectLabel("kicktable", ["srs"]),
    selectLabel("options", ["hold=avoid"]),
  ]);
  const command = findFieldModalCommand(interaction);
  assert.equal(command?.name, "path");
  assert.deepEqual(optionsByName(readFieldModalOptions(interaction, command)), {
    field: "..........\n....xx....",
    next: "[iosz]p2",
    kicktable: "srs",
    options: "hold=avoid",
  });
});

test("score-finder Modal submits explicit row and initial-B2B defaults", () => {
  const modal = modalFor("score-finder");
  const interaction = modalInteraction(modal.data.custom_id, [
    textLabel("field", EMPTY_EIGHT_ROWS),
    textLabel("next", "SIJSTLZO"),
    selectLabel("lines", ["4"]),
    selectLabel("kicktable", ["srs-plus"]),
    selectLabel("options", ["initial_b2b=false"]),
  ]);
  const command = findFieldModalCommand(interaction);
  assert.deepEqual(optionsByName(readFieldModalOptions(interaction, command)), {
    field: EMPTY_EIGHT_ROWS,
    next: "SIJSTLZO",
    lines: 4,
    kicktable: "srs-plus",
    options: "initial_b2b=false",
  });
});

test("spin-structure Modal submits inventory, terminal lines, profile, and rule", () => {
  const modal = modalFor("spin-structure");
  const interaction = modalInteraction(modal.data.custom_id, [
    textLabel("pieces", "Ttio"),
    textLabel("field", EMPTY_EIGHT_ROWS),
    selectLabel("lines", ["2+"]),
    selectLabel("profile", ["all-mini-plus"]),
    selectLabel("kicktable", ["srs-x"]),
  ]);
  const command = findFieldModalCommand(interaction);
  assert.equal(command?.name, "spin-structure");
  assert.equal(readCommandModalLocale(interaction), null);
  assert.deepEqual(optionsByName(readFieldModalOptions(interaction, command)), {
    pieces: "Ttio",
    field: EMPTY_EIGHT_ROWS,
    lines: "2+",
    profile: "all-mini-plus",
    kicktable: "srs-x",
  });
});

test("legacy v1 text-input submits remain compatible", () => {
  const interaction = modalInteraction("clearra:board:v1:path", [
    textLabel("next", "[iosz]p2"),
    textLabel("field", "..........\n....xx...."),
    textLabel("lines", "6"),
    textLabel("options", "hold=avoid"),
  ]);
  const command = findFieldModalCommand(interaction);
  assert.equal(command?.name, "path");
  assert.deepEqual(optionsByName(readFieldModalOptions(interaction, command)), {
    field: "..........\n....xx....",
    next: "[iosz]p2",
    lines: 6,
    options: "hold=avoid",
  });

  const legacyActionRows = modalInteraction("clearra:board:v1:path", [
    { type: 1, components: [{ type: 4, custom_id: "next", value: "I" }] },
    { type: 1, components: [{ type: 4, custom_id: "field", value: ".........." }] },
  ]);
  assert.deepEqual(
    optionsByName(readFieldModalOptions(legacyActionRows, command)),
    { field: "..........", next: "I" },
  );
});

test("in-flight v3 setup-ranking Modals retain their two inputs and locale selector", () => {
  const interaction = modalInteraction("clearra:search:v3:best-setup", [
    textLabel("remaining", "IOTS"),
    selectLabel("kicktable", ["srs-x"]),
    selectLabel("locale", ["ko"]),
  ]);
  const command = findFieldModalCommand(interaction);
  assert.equal(command?.name, "best-setup");
  assert.equal(readCommandModalLocale(interaction), "ko");
  assert.deepEqual(optionsByName(readFieldModalOptions(interaction, command)), {
    remaining: "IOTS",
    kicktable: "srs-x",
  });

  const v2 = modalInteraction("clearra:search:v2:pc-setup", [
    textLabel("remaining", "IOT"),
    selectLabel("kicktable", ["srs"]),
  ]);
  const v2Command = findFieldModalCommand(v2);
  assert.equal(readCommandModalLocale(v2), null);
  assert.deepEqual(optionsByName(readFieldModalOptions(v2, v2Command)), {
    remaining: "IOT",
    kicktable: "srs",
  });
});

test("Modal submit rejects duplicate, unknown, malformed, and multi-value inputs", () => {
  const command = findSlashCommand("path");
  const duplicate = modalInteraction("clearra:search:v2:path", [
    textLabel("field", ".........."),
    textLabel("field", ".........."),
    textLabel("next", "I"),
  ]);
  assert.throws(() => readFieldModalOptions(duplicate, command), /more than once/);

  const unknown = modalInteraction("clearra:search:v2:path", [
    textLabel("field", ".........."),
    textLabel("next", "I"),
    textLabel("surprise", "value"),
  ]);
  assert.throws(() => readFieldModalOptions(unknown, command), /unsupported Modal input/);

  const nonIntegerV1 = modalInteraction("clearra:board:v1:path", [
    textLabel("field", ".........."),
    textLabel("next", "I"),
    textLabel("lines", "auto"),
  ]);
  assert.throws(() => readFieldModalOptions(nonIntegerV1, command), /must be an integer/);

  const multipleValues = modalInteraction("clearra:search:v2:path", [
    textLabel("field", ".........."),
    textLabel("next", "I"),
    selectLabel("lines", ["auto", "4"]),
  ]);
  assert.throws(
    () => readFieldModalOptions(multipleValues, command),
    /exactly one|single value|one value/,
  );

  const unknownLayout = modalInteraction("clearra:search:v2:path", [
    { type: 999, components: [{ type: 4, custom_id: "next", value: "I" }] },
  ]);
  assert.throws(
    () => readFieldModalOptions(unknownLayout, command),
    /invalid field Modal layout|invalid search Modal layout|invalid command Modal layout/,
  );
});

function modalFor(name, options = [], locale = "en") {
  const response = buildMissingBoardModalResponse(
    slashInteraction(name, options),
    locale,
  );
  assert.equal(response?.type, 9, `/${name} did not open a Modal`);
  return response;
}

function slashInteraction(name, options = []) {
  return {
    type: 2,
    data: { type: 1, name, options },
  };
}

function completeOptions(input) {
  switch (input) {
    case "pc":
      return [
        { name: "field", value: EMPTY_FOUR_ROWS },
        { name: "next", value: "I" },
      ];
    case "cover":
      return [
        { name: "base", value: EMPTY_EIGHT_ROWS },
        { name: "target", value: `${emptyGrid(7)}\n....####..` },
        { name: "next", value: "I" },
      ];
    case "colored":
      return [
        { name: "field", value: `${emptyGrid(7)}\n....####..` },
        { name: "next", value: "I" },
      ];
    case "spin":
      return [
        { name: "field", value: EMPTY_EIGHT_ROWS },
        { name: "next", value: "T" },
      ];
    case "fixed-next":
      return [
        { name: "field", value: EMPTY_EIGHT_ROWS },
        { name: "next", value: "I" },
      ];
    case "score-fixed-next":
      return [
        { name: "field", value: EMPTY_FOUR_ROWS },
        { name: "next", value: "I" },
      ];
    case "spin-structure":
      return [
        { name: "pieces", value: "IOTS" },
        { name: "field", value: EMPTY_EIGHT_ROWS },
      ];
    case "remaining":
      return [{ name: "remaining", value: "IOTS" }];
    case "verify":
      return [{ name: "scope", value: "pc" }];
    default:
      throw new Error(`unknown test input contract: ${input}`);
  }
}

function componentNames(response) {
  return response.data.components.map(({ component: input }) => input.custom_id);
}

function component(response, name) {
  const value = response.data.components
    .map(({ component: input }) => input)
    .find(({ custom_id: customId }) => customId === name);
  assert.ok(value, `missing ${name} component`);
  return value;
}

function assertStringSelect(input, expectedValues, defaultValue) {
  assert.equal(input.type, 3);
  assert.deepEqual(input.options.map(({ value }) => value), expectedValues);
  assert.deepEqual(
    input.options.filter(({ default: selected }) => selected).map(({ value }) => value),
    [defaultValue],
  );
}

function modalInteraction(customId, components) {
  return {
    id: "modal-id",
    application_id: "application-id",
    token: "modal-token",
    type: 5,
    data: { custom_id: customId, components },
  };
}

function textLabel(name, value) {
  return {
    type: 18,
    component: { type: 4, custom_id: name, value },
  };
}

function selectLabel(name, values) {
  return {
    type: 18,
    component: { type: 3, custom_id: name, values },
  };
}

function optionsByName(options) {
  return Object.fromEntries(options.map(({ name, value }) => [name, value]));
}

function emptyGrid(rows) {
  return Array(rows).fill("__________").join("\n");
}

function label(modal, name) {
  return modal.data.components.find(
    ({ component: candidate }) => candidate.custom_id === name,
  );
}
