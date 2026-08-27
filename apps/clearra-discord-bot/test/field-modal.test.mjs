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
const NATIVE_KICKTABLES = [...BUILTIN_KICKTABLES, "no-kick"];
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
  "score",
];
const SCORED_PC_COMMANDS = ["score-minimals"];
const TYPED_PC_COMMANDS = ["minimals"];
const COLORED_COMMANDS = [];
const SPIN_COMMANDS = ["spin-cover", "spin"];
const REMAINING_COMMANDS = ["pc-setup", "best-setup", "dpc-finder"];

test("every modal-backed search route opens its capability-specific v4 Modal while help stays direct", () => {
  for (const route of searchRoutes()) {
    const response = buildMissingBoardModalResponse(routeInteraction(route));
    assert.equal(response?.type, 9, `/${route.path} must open a Modal`);
    assert.equal(response.data.custom_id, `clearra:search:v4:${route.modalKey}`);
    assert.ok(
      response.data.components.length <= 5,
      `/${route.path} exceeded Discord's five-component Modal limit`,
    );
    assert.equal(
      componentNames(response).includes("objective"),
      false,
      `/${route.path} exposed the advanced text-only objective in a Modal`,
    );
    assert.equal(
      findFieldModalCommand(modalInteraction(response.data.custom_id, []))?.capabilityId,
      route.command.capabilityId,
    );
  }

  for (const command of Object.values(findSlashCommand("build").subcommands)) {
    if (!command.input?.startsWith("build-v2-")) continue;
    assert.equal(
      buildMissingBoardModalResponse(
        slashInteraction("build", [{ type: 1, name: command.subcommand, options: [] }]),
      ),
      null,
      `/build ${command.subcommand} must remain direct-only`,
    );
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

test("forward REN uses the bounded exact-queue guided form", () => {
  const response = buildMissingBoardModalResponse(
    slashInteraction("forward", [{ type: 1, name: "ren", options: [] }]),
    "en",
  );
  assert.equal(response?.type, 9);
  assert.equal(response.data.custom_id, "clearra:search:v4:forward~ren");
  assert.deepEqual(componentNames(response), ["field", "next", "height", "hold", "kicktable"]);
  const command = findFieldModalCommand(modalInteraction(response.data.custom_id, []));
  assert.equal(command?.capabilityId, "forward.ren");
});

test("pc score Modal discloses approximation accuracy in both locales", () => {
  const interaction = slashInteraction("pc", [{
    type: 1,
    name: "score",
    options: [],
  }]);
  const english = buildMissingBoardModalResponse(interaction, "en");
  const korean = buildMissingBoardModalResponse(interaction, "ko");
  assert.match(label(english, "score-profile").label, /basic approximation/i);
  assert.match(
    label(english, "score-profile").description,
    /profile-specific exactness is false/i,
  );
  assert.match(label(korean, "score-profile").label, /기초 근삿/u);
  assert.match(label(korean, "score-profile").description, /profile-specific exact.*false/iu);
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
  for (const route of searchRoutes()) {
    const complete = completeOptions(route.command.input);
    const response = buildMissingBoardModalResponse(
      routeInteraction(route, complete),
    );
    const hasMultilineBoard = complete.some(({ name, value }) =>
      ["field", "base", "target"].includes(name) && String(value).includes("\n")
    );
    assert.equal(
      response?.type ?? null,
      hasMultilineBoard ? 9 : null,
      `/${route.path} used the wrong direct-input boundary`,
    );

    const missing = complete.slice(1);
    assert.equal(
      buildMissingBoardModalResponse(routeInteraction(route, missing))?.type,
      9,
      `/${route.path} with a missing runtime input must open a Modal`,
    );
  }

  assert.equal(
    buildMissingBoardModalResponse(slashInteraction("spin-structure", [{
      type: 1,
      name: "search",
      options: [
        { name: "pieces", value: "IOTS" },
        { name: "field", value: "grid:__________ / ####______".replaceAll(" ", "") },
      ],
    }])),
    null,
    "compact single-line grids must remain direct slash options",
  );
});

test("All-Spin exact and pattern Modals keep five typed fields and fail closed on lost prefill", () => {
  const exact = modalForSubcommand("pc", "allspin-sol");
  const chance = modalForSubcommand("pc", "allspin-pres-chance");
  for (const modal of [exact, chance]) {
    assert.deepEqual(componentNames(modal), [
      "field", "next", "lines", "spin-profile", "kicktable",
    ]);
    assert.equal(componentNames(modal).includes("locale"), false);
    assertStringSelect(
      component(modal, "spin-profile"),
      [
        "t-spins", "t-spins-plus", "all-spin", "all-spin-plus",
        "all-mini", "all-mini-plus",
      ],
      "all-spin-plus",
    );
    assertStringSelect(component(modal, "kicktable"), NATIVE_KICKTABLES, "srs-plus");
    assert.equal(component(modal, "field").required, true);
    assert.equal(component(modal, "next").required, true);
    assert.equal(component(modal, "spin-profile").required, true);
  }
  assert.equal(component(exact, "next").placeholder, "IOTSZJL");
  assert.match(component(chance, "next").placeholder, /\*!|\[IOSZ\]/);

  const exactSubmit = modalInteraction(exact.data.custom_id, [
    textLabel("field", "__________\n####______"),
    textLabel("next", "IOTS"),
    selectLabel("lines", ["2"]),
    selectLabel("spin-profile", ["all-mini-plus"]),
    selectLabel("kicktable", ["no-kick"]),
  ]);
  const exactCommand = findFieldModalCommand(exactSubmit);
  assert.equal(exactCommand.inputSchemaId, "pc-allspin-exact-queue.v1");
  assert.deepEqual(optionsByName(readFieldModalOptions(exactSubmit, exactCommand)), {
    field: "__________\n####______",
    next: "IOTS",
    lines: 2,
    "spin-profile": "all-mini-plus",
    kicktable: "no-kick",
  });
  assert.equal(
    findFieldModalCommand(modalInteraction(chance.data.custom_id, []))?.inputSchemaId,
    "pc-allspin-pattern.v1",
  );

  for (const [subcommand, name, value] of [
    ["allspin-sol", "hold", "off"],
    ["allspin-sol", "max-nodes", 17],
    ["allspin-pres-chance", "max-memory-mib", 64],
  ]) {
    assert.throws(
      () => buildMissingBoardModalResponse(slashInteraction("pc", [{
        type: 1,
        name: subcommand,
        options: [{ name, value }],
      }])),
      (error) => error?.code === "options.modal_unrepresented" &&
        error?.details?.options === name,
      `${subcommand}/${name}`,
    );
  }

  assert.equal(buildMissingBoardModalResponse(slashInteraction("pc", [{
    type: 1,
    name: "allspin-sol",
    options: [
      { name: "field", value: "grid:__________/####______" },
      { name: "next", value: "IOTS" },
      { name: "spin-profile", value: "all-spin-plus" },
    ],
  }])), null);
});

test("Build v2 source-pieces stays named while the typed mask source remains direct-only", () => {
  const build = findSlashCommand("build").subcommands.cover;
  assert.equal(
    build.registration.options.some(({ name }) => name === "source-pieces"),
    true,
  );
  assert.equal(
    buildMissingBoardModalResponse(slashInteraction("build", [{
      type: 1,
      name: "cover",
      options: [{ name: "source-pieces", value: 17 }],
    }])),
    null,
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
  for (const name of TYPED_PC_COMMANDS) {
    assert.deepEqual(componentNames(modalFor(name)), [
      "field",
      "next",
      "lines",
      "hold",
      "kicktable",
    ]);
  }
  for (const name of SCORED_PC_COMMANDS) {
    assert.deepEqual(componentNames(modalFor(name)), [
      "field",
      "next",
      "lines",
      "score-profile",
      "kicktable",
    ]);
  }

  assert.deepEqual(componentNames(modalFor("cover")), [
    "base",
    "target",
    "next",
    "kicktable",
    "options",
  ]);
  assert.equal(
    buildMissingBoardModalResponse(slashInteraction("build", [{
      type: 1,
      name: "cover",
      options: [],
    }])),
    null,
  );

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
    "options",
    "locale",
  ]);
  assert.deepEqual(componentNames(modalForSubcommand("spin-structure", "search")), [
    "pieces",
    "field",
    "lines",
    "spin-profile",
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
  assert.equal(buildMissingBoardModalResponse(slashInteraction("verify")), null);
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
      ["full-queue", "visible-7"],
      "full-queue",
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
    "srs-plus", "srs", "srs-x", "jstris-180", "no-kick",
  ], "srs-x");
  assertStringSelect(component(preselected, "priority"), ["all", "build", "pc"], "build");
  assertStringSelect(
    component(preselected, "max-setup-pieces"),
    Array.from({ length: 10 }, (_, index) => String(index + 1)),
    "8",
  );
  assertStringSelect(
    component(preselected, "queue-knowledge"),
    ["full-queue", "visible-7"],
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
    selectLabel("queue-knowledge", ["full-queue"]),
  ]);
  const command = findFieldModalCommand(interaction);
  assert.deepEqual(optionsByName(readFieldModalOptions(interaction, command)), {
    remaining: "IOTS",
    priority: "build",
    "max-setup-pieces": 9,
    "queue-knowledge": "full-queue",
    kicktable: "srs-plus",
  });

  for (const name of ["next-cycle-remaining", "setup-length"]) {
    assert.throws(
      () => buildMissingBoardModalResponse(slashInteraction("pc-setup", [
        { name, value: name === "setup-length" ? "longer" : "Z" },
      ])),
      (error) => error?.code === "options.modal_unrepresented" &&
        error?.details?.options === name,
    );
  }
});

test("pc tiling Modal resolves only the canonical typed tiling authority", () => {
  const modal = modalForSubcommand("pc", "tiling");
  const command = findFieldModalCommand(modalInteraction(modal.data.custom_id, []));
  assert.equal(command.capabilityId, "pc.tiling");
  assert.equal(command.input, "pc-tiling-v2");
  assert.deepEqual(command.argvPrefix, ["pc", "tiling"]);
  assert.equal(command.resultContractId, "pc-tiling-family.v1");
  assert.deepEqual(command.resultAllowlist, ["pc-tiling-family.v1"]);
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
  ];
  for (const [name, input] of eightRowFields) {
    assert.equal(component(modalFor(name), input).value, EMPTY_EIGHT_ROWS);
  }
  assert.equal(
    component(modalForSubcommand("spin-structure", "search"), "field").value,
    EMPTY_EIGHT_ROWS,
  );
  assert.equal(component(modalFor("score-finder"), "field").value, EMPTY_FOUR_ROWS);
});

test("v4 lines, kicktable, hold, spin type, and language use string selects", () => {
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
    ["initial-b2b=false", "initial-b2b=true"],
    "initial-b2b=false",
  );

  const spinStructure = modalForSubcommand("spin-structure", "search");
  assertStringSelect(
    component(spinStructure, "lines"),
    SPIN_STRUCTURE_LINES,
    "1+",
  );
  assertStringSelect(
    component(spinStructure, "spin-profile"),
    SPIN_STRUCTURE_PROFILES,
    "t-spins",
  );
  assertStringSelect(
    component(spinStructure, "kicktable"),
    NATIVE_KICKTABLES,
    "srs-plus",
  );
  assert.equal(componentNames(spinStructure).includes("locale"), false);

  assert.equal(buildMissingBoardModalResponse(slashInteraction("verify")), null);

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

  const koreanSpinStructure = modalForSubcommand("spin-structure", "search", [], "ko");
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
    component(koreanSpinStructure, "spin-profile").options.map(({ label: text }) => text),
    ["T 스핀", "T 스핀+", "전체 Mini", "전체 Mini+", "전체 스핀", "전체 스핀+"],
  );
  assert.match(label(koreanSpinStructure, "spin-profile").description, /Regular.*Mini.*분리/u);
  assert.match(label(spinStructure, "spin-profile").description, /Regular.*Mini separate/);
});

test("board forms prefer keyboard # and _ cells without advertising G", () => {
  const english = modalFor("path", [], "en");
  const korean = modalFor("path", [], "ko");
  const englishField = label(english, "field");
  const koreanField = label(korean, "field");
  const englishBuildField = label(modalFor("spin", [], "en"), "field");
  const koreanBuildField = label(modalFor("spin", [], "ko"), "field");

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
  const coloredModal = modalFor("spin", [], "ko");
  const coloredSubmit = modalInteraction(coloredModal.data.custom_id, [
    textLabel("field", EMPTY_EIGHT_ROWS),
    textLabel("next", "I"),
    selectLabel("kicktable", ["srs-plus"]),
    selectLabel("options", ["type=TSS"]),
    selectLabel("locale", ["ko"]),
  ]);
  assert.equal(readCommandModalLocale(coloredSubmit), "ko");
  assert.match(coloredModal.data.title, /입력/u);

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
    selectLabel("options", ["initial-b2b=false"]),
  ]);
  const command = findFieldModalCommand(interaction);
  assert.deepEqual(optionsByName(readFieldModalOptions(interaction, command)), {
    field: EMPTY_EIGHT_ROWS,
    next: "SIJSTLZO",
    lines: 4,
    kicktable: "srs-plus",
    options: "initial-b2b=false",
  });
});

test("spin-structure Modal submits inventory, terminal lines, profile, and rule", () => {
  const modal = modalForSubcommand("spin-structure", "search");
  const interaction = modalInteraction(modal.data.custom_id, [
    textLabel("pieces", "Ttio"),
    textLabel("field", EMPTY_EIGHT_ROWS),
    selectLabel("lines", ["2+"]),
    selectLabel("spin-profile", ["all-mini-plus"]),
    selectLabel("kicktable", ["srs-x"]),
  ]);
  const command = findFieldModalCommand(interaction);
  assert.equal(command?.capabilityId, "spin-structure.search");
  assert.equal(readCommandModalLocale(interaction), null);
  assert.deepEqual(optionsByName(readFieldModalOptions(interaction, command)), {
    pieces: "Ttio",
    field: EMPTY_EIGHT_ROWS,
    lines: "2+",
    "spin-profile": "all-mini-plus",
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

function modalForSubcommand(root, subcommand, options = [], locale = "en") {
  const response = buildMissingBoardModalResponse(
    slashInteraction(root, [{ type: 1, name: subcommand, options }]),
    locale,
  );
  assert.equal(response?.type, 9, `/${root} ${subcommand} did not open a Modal`);
  return response;
}

function searchRoutes() {
  return slashCommandCatalog.flatMap((command) => {
    if (command.kind !== "search") return [];
    if (!command.subcommands) {
      return command.input === "group" ? [] : [{
        command,
        path: command.name,
        modalKey: command.name,
        root: command.name,
        subcommand: null,
      }];
    }
    return Object.values(command.subcommands).map((variant) => ({
      command: variant,
      path: `${command.name} ${variant.subcommand}`,
      modalKey: `${command.name}~${variant.subcommand}`,
      root: command.name,
      subcommand: variant.subcommand,
    }));
  }).filter(({ command }) =>
    command.modalSchemaId !== null && !command.input?.startsWith("build-v2-")
  );
}

function routeInteraction(route, options = []) {
  return route.subcommand
    ? slashInteraction(route.root, [{ type: 1, name: route.subcommand, options }])
    : slashInteraction(route.root, options);
}

function slashInteraction(name, options = []) {
  return {
    type: 2,
    data: { type: 1, name, options },
  };
}

function completeOptions(input) {
  switch (input) {
    case "pc-v2":
    case "pc-path-v2":
    case "pc-chance-v2":
    case "pc-save-v2":
      return [
        { name: "field", value: EMPTY_FOUR_ROWS },
        { name: "next", value: "I" },
      ];
    case "pc-allspin-exact-v1":
      return [
        { name: "field", value: EMPTY_FOUR_ROWS },
        { name: "next", value: "IOTSZJLOIT" },
        { name: "spin-profile", value: "all-spin-plus" },
      ];
    case "pc-allspin-pattern-v1":
      return [
        { name: "field", value: EMPTY_FOUR_ROWS },
        { name: "next", value: "*!P3" },
        { name: "spin-profile", value: "all-spin-plus" },
      ];
    case "pc-score-v2":
      return [
        { name: "field", value: EMPTY_FOUR_ROWS },
        { name: "next", value: "IOTSZ" },
      ];
    case "pc-tiling-v2":
    case "pc-failed-v2":
      return [
        { name: "field", value: EMPTY_FOUR_ROWS },
        { name: "next", value: "I" },
      ];
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
    case "build-cover":
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
    case "forward-spin-v2":
      return [
        { name: "field", value: EMPTY_EIGHT_ROWS },
        { name: "next", value: "T" },
      ];
    case "forward-damage-v2":
      return [
        { name: "field", value: EMPTY_EIGHT_ROWS },
        { name: "next", value: "I" },
      ];
    case "forward-ren-v1":
      return [
        { name: "field", value: EMPTY_EIGHT_ROWS },
        { name: "next", value: "I" },
      ];
    case "score-fixed-next":
      return [
        { name: "field", value: EMPTY_FOUR_ROWS },
        { name: "next", value: "I" },
      ];
    case "score-fixed-next-v2":
    case "pc-score-finder-v2":
      return [
        { name: "field", value: EMPTY_FOUR_ROWS },
        { name: "next", value: "I" },
      ];
    case "spin-structure":
      return [
        { name: "pieces", value: "IOTS" },
        { name: "field", value: EMPTY_EIGHT_ROWS },
      ];
    case "spin-structure-v2":
    case "spin-structure-cover-v1":
    case "spin-structure-guaranteed-v1":
      return [
        { name: "pieces", value: "IOTS" },
        { name: "field", value: EMPTY_EIGHT_ROWS },
      ];
    case "remaining":
      return [{ name: "remaining", value: "IOTS" }];
    case "setup-v2":
      return [{ name: "remaining", value: "IOTS" }];
    case "setup-score-v1":
      return [
        { name: "document-format", value: "fumen" },
        { name: "document", value: "v115@vhAAgH" },
        { name: "setup-queue", value: "IOTS" },
        { name: "solution-queue", value: "ZJLT" },
      ];
    case "finesse-search":
      return [
        { name: "target", value: `${emptyGrid(7)}\n....####..` },
        { name: "next", value: "I" },
        { name: "base", value: EMPTY_EIGHT_ROWS },
      ];
    case "finesse-score":
      return [
        { name: "document", value: "ctk3_example" },
        { name: "next", value: "I" },
      ];
    case "finesse-score-v2":
      return [
        { name: "document", value: "ctk3_example" },
        { name: "next", value: "I" },
      ];
    case "operation-document-v1":
      return [{ name: "document", value: "ctk3_example" }];
    case "field-document-v1":
      return [{ name: "document", value: "v115@vhAAgH" }];
    case "fumen-transform-v1":
      return [
        { name: "transform", value: "roundtrip" },
        { name: "document", value: "v115@vhAAgH" },
      ];
    case "render-document-v1":
      return [
        { name: "document", value: "v115@vhAAgH" },
        { name: "artifact-format", value: "png" },
      ];
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
