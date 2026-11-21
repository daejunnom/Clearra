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

const PC_COMMANDS = ["path", "chance", "minimals"];
const SCORED_PC_COMMANDS = ["score", "score-minimals"];
const SETUP_COMMANDS = ["joint", "build", "pc"];
const AUTO_PC_LINES = ["auto", ...Array.from({ length: 24 }, (_, index) => String(index + 1))];
const INITIAL_HOLD = ["empty", "disabled", "I", "O", "T", "S", "Z", "J", "L"];

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

test("full-height PC score forms match the 24-row capability, including Japanese", () => {
  for (const subcommand of ["score", "score-minimals", "score-finder"]) {
    for (const locale of ["en", "ko", "ja"]) {
      const modal = modalForSubcommand("pc", subcommand, [], locale);
      const field = label(modal, "field");
      assert.match(field.label, locale === "ja" ? /1〜24行/u : /1–24/u, `${subcommand} ${locale}`);
      const choices = component(modal, "lines").options.map(({ value }) => value);
      assert.equal(choices.length, 25);
      assert.ok(choices.includes("24"));
      assert.deepEqual(modalFor(subcommand, [], locale).data.components, modal.data.components);
    }
  }
});

test("finesse score alias uses the canonical bounded guided form", () => {
  const score = modalForSubcommand("finesse", "score");
  assert.deepEqual(score.data.components, modalForSubcommand("build", "finesse-score").data.components);
  assert.deepEqual(componentNames(score), ["document", "next", "kicktable", "hold", "knowledge"]);

  const scoreSubmit = modalInteraction(score.data.custom_id, [
    textLabel("document", "ctk3_example"),
    textLabel("next", "[TI]!"),
    selectLabel("kicktable", ["srs-plus"]),
    selectLabel("hold", ["disabled"]),
    selectLabel("knowledge", ["oracle"]),
  ]);
  const scoreCommand = findFieldModalCommand(scoreSubmit);
  assert.equal(scoreCommand.subcommand, "score");
  assert.equal(readCommandModalLocale(scoreSubmit), null);
  assert.deepEqual(optionsByName(readFieldModalOptions(scoreSubmit, scoreCommand)), {
    document: "ctk3_example",
    next: "[TI]!",
    kicktable: "srs-plus",
    hold: "disabled",
    knowledge: "oracle",
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

  assert.equal(buildMissingBoardModalResponse(slashInteraction("cover")), null);
  assert.equal(
    buildMissingBoardModalResponse(slashInteraction("build", [{
      type: 1,
      name: "cover",
      options: [],
    }])),
    null,
  );

  assert.deepEqual(modalFor("spin").data.components, modalForSubcommand("forward", "spin").data.components);
  assert.deepEqual(componentNames(modalFor("score-finder")), [
    "field",
    "next",
    "lines",
    "hold",
    "kicktable",
  ]);
  assert.deepEqual(modalFor("damage").data.components, modalForSubcommand("forward", "damage").data.components);
  assert.deepEqual(componentNames(modalForSubcommand("spin-structure", "search")), [
    "pieces",
    "field",
    "lines",
    "spin-profile",
    "kicktable",
  ]);
  for (const subcommand of SETUP_COMMANDS) {
    assert.deepEqual(componentNames(modalForSubcommand("setup", subcommand)), [
      "remaining",
      "mode",
      "qb",
      "queue-knowledge",
      "kicktable",
    ]);
  }
  assert.equal(buildMissingBoardModalResponse(slashInteraction("verify")), null);
});

test("canonical setup Modals preserve input defaults within Discord's five-component cap", () => {
  for (const subcommand of SETUP_COMMANDS) {
    const modal = modalForSubcommand("setup", subcommand);
    assertStringSelect(component(modal, "mode"), ["oracle", "qb"], "oracle");
    assertStringSelect(
      component(modal, "queue-knowledge"),
      ["full-queue", "visible-7"],
      "full-queue",
    );
    assert.equal(componentNames(modal).includes("next-cycle-remaining"), false);
    assert.equal(componentNames(modal).includes("setup-length"), false);
    assert.equal(componentNames(modal).includes("locale"), false);
  }

  const korean = modalForSubcommand("setup", "build", [], "ko");
  assert.deepEqual(
    component(korean, "queue-knowledge").options.map(({ label: text }) => text),
    ["전체 미래 큐 (기본값)", "공개 7개"],
  );

  const preselected = modalForSubcommand("setup", "joint", [
    { name: "kicktable", value: "srs-x" },
    { name: "mode", value: "qb" },
    { name: "qb", value: "Z" },
    { name: "queue-knowledge", value: "visible-7" },
  ]);
  assertStringSelect(component(preselected, "kicktable"), [
    "srs-plus", "srs", "srs-x", "jstris-180", "no-kick",
  ], "srs-x");
  assertStringSelect(component(preselected, "mode"), ["oracle", "qb"], "qb");
  assert.equal(component(preselected, "qb").value, "Z");
  assertStringSelect(
    component(preselected, "queue-knowledge"),
    ["full-queue", "visible-7"],
    "visible-7",
  );
});

test("setup ranking Modal submits explicit defaults and rejects hidden-option loss", () => {
  const modal = modalForSubcommand("setup", "build");
  const interaction = modalInteraction(modal.data.custom_id, [
    textLabel("remaining", "IOTS"),
    selectLabel("kicktable", ["srs-plus"]),
    selectLabel("mode", ["oracle"]),
    textLabel("qb", ""),
    selectLabel("queue-knowledge", ["full-queue"]),
  ]);
  const command = findFieldModalCommand(interaction);
  assert.deepEqual(optionsByName(readFieldModalOptions(interaction, command)), {
    remaining: "IOTS",
    mode: "oracle",
    "queue-knowledge": "full-queue",
    kicktable: "srs-plus",
  });

  for (const name of ["next-cycle-remaining", "setup-length"]) {
    assert.throws(
      () => modalForSubcommand("setup", "joint", [
        { name, value: name === "setup-length" ? "longer" : "Z" },
      ]),
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

test("24L Tiling, Minimum, Chance and Failed Queue Modals expose exactly Auto plus 24 rows in every released locale", () => {
  const expected = ["auto", ...Array.from({ length: 24 }, (_, index) => String(index + 1))];
  for (const locale of ["en", "ko", "ja"]) {
    for (const product of ["tiling", "minimals", "chance", "failed-queue", "path"]) {
    const modal = modalForSubcommand("pc", product, [], locale);
    assertStringSelect(component(modal, "lines"), expected, "auto");
    assert.equal(component(modal, "lines").options.length, 25, "Discord's 25-choice cap");
    assert.match(modal.data.components.find(({ component: input }) => input.custom_id === "field").label, /24/u);
    assert.match(component(modal, "lines").options[0].label, /24/u);
    }
  }
});

test("PC fields default to four rows and other board searches default to eight", () => {
  for (const name of [...PC_COMMANDS, ...SCORED_PC_COMMANDS, "score-finder"]) {
    assert.equal(component(modalFor(name), "field").value, EMPTY_FOUR_ROWS);
  }

  for (const name of ["spin", "damage"]) {
    assert.equal(component(modalFor(name), "field").value, EMPTY_EIGHT_ROWS);
  }
  assert.equal(
    component(modalForSubcommand("spin-structure", "search"), "field").value,
    EMPTY_EIGHT_ROWS,
  );
});

test("v4 lines, kicktable, hold, and spin profile use string selects", () => {
  const path = modalFor("path");
  assertStringSelect(component(path, "lines"), AUTO_PC_LINES, "auto");
  assert.deepEqual(component(path, "kicktable"), component(modalForSubcommand("pc", "path"), "kicktable"));
  assertStringSelect(component(path, "hold"), INITIAL_HOLD, "empty");
  assertStringSelect(component(modalFor("spin"), "lines"), SPIN_STRUCTURE_LINES, "any");
  assertStringSelect(component(modalFor("score-finder"), "lines"), AUTO_PC_LINES, "auto");

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
  assert.match(englishField.label, /1–24 rows/);
  assert.match(koreanField.label, /1–24줄/u);
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
  assert.match(englishScoreLines.description, /rows 1–24/);
  assert.match(koreanScoreLines.description, /1–24줄/u);
  assert.doesNotMatch(englishScoreLines.description, /default/i);
  assert.doesNotMatch(koreanScoreLines.description, /기본/u);
});

test("v4 Modal language selection is explicit only when the layout has capacity", () => {
  const tilingModal = modalForSubcommand("pc", "tiling", [], "ko");
  const tilingSubmit = modalInteraction(tilingModal.data.custom_id, [
    textLabel("field", EMPTY_FOUR_ROWS),
    textLabel("next", "I"),
    selectLabel("lines", ["auto"]),
    selectLabel("hold", ["empty"]),
    selectLabel("locale", ["ko"]),
  ]);
  assert.equal(readCommandModalLocale(tilingSubmit), "ko");
  assert.match(tilingModal.data.title, /입력/u);

  const pathModal = modalFor("path", [], "ko");
  assert.equal(componentNames(pathModal).includes("locale"), false);
  const pathSubmit = modalInteraction(pathModal.data.custom_id, [
    textLabel("field", ".........."),
    textLabel("next", "I"),
    selectLabel("lines", ["auto"]),
    selectLabel("kicktable", ["srs-plus"]),
    selectLabel("hold", ["empty"]),
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
    selectLabel("hold", ["disabled"]),
  ]);
  const command = findFieldModalCommand(interaction);
  assert.equal(command?.name, "path");
  assert.deepEqual(optionsByName(readFieldModalOptions(interaction, command)), {
    field: "..........\n....xx....",
    next: "[iosz]p2",
    kicktable: "srs",
    hold: "disabled",
  });
});

test("score-finder Modal submits named row and hold options", () => {
  const modal = modalFor("score-finder");
  const interaction = modalInteraction(modal.data.custom_id, [
    textLabel("field", EMPTY_EIGHT_ROWS),
    textLabel("next", "SIJSTLZO"),
    selectLabel("lines", ["4"]),
    selectLabel("kicktable", ["srs-plus"]),
    selectLabel("hold", ["empty"]),
  ]);
  const command = findFieldModalCommand(interaction);
  assert.deepEqual(optionsByName(readFieldModalOptions(interaction, command)), {
    field: EMPTY_EIGHT_ROWS,
    next: "SIJSTLZO",
    lines: 4,
    kicktable: "srs-plus",
    hold: "empty",
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

test("v1 form transport still reads native fields without accepting retired packed inputs", () => {
  const interaction = modalInteraction("clearra:board:v1:path", [
    textLabel("next", "[iosz]p2"),
    textLabel("field", "..........\n....xx...."),
    textLabel("lines", "6"),
    textLabel("hold", "disabled"),
  ]);
  const command = findFieldModalCommand(interaction);
  assert.equal(command?.name, "path");
  assert.deepEqual(optionsByName(readFieldModalOptions(interaction, command)), {
    field: "..........\n....xx....",
    next: "[iosz]p2",
    lines: 6,
    hold: "disabled",
  });
  const retiredOptions = modalInteraction("clearra:board:v1:path", [
    textLabel("next", "I"),
    textLabel("options", "hold=avoid"),
  ]);
  assert.throws(() => readFieldModalOptions(retiredOptions, command), /unsupported Modal input 'options'/);

  const legacyActionRows = modalInteraction("clearra:board:v1:path", [
    { type: 1, components: [{ type: 4, custom_id: "next", value: "I" }] },
    { type: 1, components: [{ type: 4, custom_id: "field", value: ".........." }] },
  ]);
  assert.deepEqual(
    optionsByName(readFieldModalOptions(legacyActionRows, command)),
    { field: "..........", next: "I" },
  );
});

test("in-flight Modal IDs for retired setup presets do not resolve to product commands", () => {
  const interaction = modalInteraction("clearra:search:v3:best-setup", [
    textLabel("remaining", "IOTS"),
    selectLabel("kicktable", ["srs-x"]),
    selectLabel("locale", ["ko"]),
  ]);
  assert.equal(findFieldModalCommand(interaction), null);
  assert.equal(readCommandModalLocale(interaction), null);

  const v2 = modalInteraction("clearra:search:v2:pc-setup", [
    textLabel("remaining", "IOT"),
    selectLabel("kicktable", ["srs"]),
  ]);
  assert.equal(findFieldModalCommand(v2), null);
  assert.equal(readCommandModalLocale(v2), null);
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
