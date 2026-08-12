import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  findSlashCommand,
  formatSlashCommandHelp,
  slashCommandCatalog,
} from "../src/discord/slash-command-catalog.mjs";
import {
  buildSlashCommandArguments,
  DISCORD_PACKED_OPTION_KEYS,
} from "../src/discord/slash-command-input.mjs";
import { DiscordInputError } from "../src/discord/i18n.mjs";

const CONTRACT_URL = new URL(
  "../../../tests/fixtures/contracts/search_option_contract.tsv",
  import.meta.url,
);

test("Discord option exposure follows the shared search-option contract fixture", () => {
  const rows = contractRows();
  const packedByFamily = new Map([
    ["pc", new Set(DISCORD_PACKED_OPTION_KEYS.pc)],
    ["setup", new Set(DISCORD_PACKED_OPTION_KEYS.remaining)],
    ["build", new Set(DISCORD_PACKED_OPTION_KEYS.cover)],
    ["damage", new Set(DISCORD_PACKED_OPTION_KEYS["fixed-next"])],
    ["spin-finder", new Set(DISCORD_PACKED_OPTION_KEYS["spin-structure"])],
  ]);

  for (const row of rows) {
    if (!row.surface.startsWith("packed:")) continue;
    if (row.family === "spin-finder" && !row.constraints.includes("spin-structure-only")) {
      continue;
    }
    const key = row.surface.slice("packed:".length);
    assert.ok(
      packedByFamily.get(row.family)?.has(key),
      `${row.family}.${row.option} must expose packed key ${key}`,
    );
  }

  const registeredNames = new Set(
    slashCommandCatalog.flatMap((command) =>
      registrationOptions(command).map(({ name }) => name)
    ),
  );
  const packedNames = new Set(Object.values(DISCORD_PACKED_OPTION_KEYS).flat());
  for (const forbidden of [
    "backend",
    "fallback",
    "workers",
    "tablebase",
    "dependency-dag",
    "gpu-device",
    "tiling",
    "tiling-only",
  ]) {
    assert.equal(registeredNames.has(forbidden), false, forbidden);
    assert.equal(packedNames.has(forbidden), false, forbidden);
  }
});

test("every documented packed option key appears in its public EN/KO help or description", () => {
  const helpTarget = new Map([
    ["pc", "path"],
    ["cover", "cover"],
    ["spin", "spin"],
    ["score-fixed-next", "score-finder"],
    ["remaining", "pc-setup"],
    ["fixed-next", "damage"],
    ["spin-structure", "spin-structure"],
    ["finesse-search", "finesse search"],
    ["finesse-score", "finesse score"],
  ]);
  for (const [input, keys] of Object.entries(DISCORD_PACKED_OPTION_KEYS)) {
    const target = helpTarget.get(input);
    const command = target.startsWith("finesse")
      ? findSlashCommand("finesse").subcommands[target.split(" ")[1]]
      : findSlashCommand(target);
    const optionDescription = command.registration.options
      .find(({ name }) => name === "options")?.description ?? "";
    const corpus = [
      optionDescription,
      formatSlashCommandHelp(target, "en"),
      formatSlashCommandHelp(target, "ko"),
    ].join("\n").replaceAll("_", "-");
    for (const key of keys) {
      assert.match(corpus, new RegExp(escapeRegex(key)), `${input}.${key}`);
    }
  }
});

test("shared Discord representatives reach the production parser for every legal single and pair Cartesian", () => {
  const rows = contractRows();
  const statistics = {};
  for (const scope of DISCORD_FIXTURE_SCOPES) {
    const exposed = rows.filter((row) =>
      row.family === scope.family && scope.options.has(row.option)
    );
    const settings = exposed.flatMap((row) =>
      row.valid.map((value) => Object.freeze({ row, value }))
    );
    let semanticCases = 0;
    let parserExecutions = 0;

    for (const setting of settings) {
      assert.equal(isLegalFixtureCase(scope.id, [setting]), true, fixtureLabel([setting]));
      parserExecutions += assertProductionParserCase(scope.id, [setting]);
      semanticCases += 1;
    }
    for (let left = 0; left < settings.length; left += 1) {
      for (let right = left + 1; right < settings.length; right += 1) {
        const pair = [settings[left], settings[right]];
        if (pair[0].row.option === pair[1].row.option) continue;
        if (!isLegalFixtureCase(scope.id, pair)) continue;
        parserExecutions += assertProductionParserCase(scope.id, pair);
        semanticCases += 1;
      }
    }
    statistics[scope.id] = { semanticCases, parserExecutions };
  }

  assert.deepEqual(statistics, EXPECTED_PRODUCTION_MATRIX_COUNTS);
});

test("fixture dependency and contradiction cases expose stable DiscordInputError codes", () => {
  const cases = [
    [
      "setup qb source required",
      () => setupArguments([
        setting("setup", "mode", "qb"),
      ], { omitInferredQb: true }),
      "options.setup_qb_required",
    ],
    [
      "setup oracle/qb conflict",
      () => setupArguments([
        setting("setup", "mode", "oracle"),
        setting("setup", "qb", "OS"),
      ], { preserveConflict: true }),
      "options.setup_qb_oracle_conflict",
    ],
    [
      "setup bag capacity",
      () => setupArguments([
        setting("setup", "remaining", "IOTSZJ"),
        setting("setup", "qb", "OS"),
      ], { preserveConflict: true }),
      "options.setup_qb_bag_capacity",
    ],
    [
      "setup cycle-seven borrow",
      () => setupArguments([
        setting("setup", "remaining", "IOTSZJL"),
        setting("setup", "post-cycle-borrow", "on"),
      ]),
      "options.setup_borrow_cycle",
    ],
    [
      "spin fill bounds",
      () => spinStructureArguments([
        setting("spin-finder", "fill-bottom", "5"),
        setting("spin-finder", "fill-top", "4"),
      ], { preserveConflict: true }),
      "options.spin_fill_bounds",
    ],
  ];
  for (const [label, run, code] of cases) {
    assert.throws(run, (error) =>
      error instanceof DiscordInputError && error.code === code,
      label,
    );
  }
});

test("shared invalid representatives fail at the production Discord boundary with stable codes", () => {
  const rows = contractRows();
  const cases = [];
  addInvalidCases(cases, rows, "pc", "lines", (value) =>
    pcArguments([setting("pc", "lines", value)]), "options.invalid", {
      exclude: new Set(["1"]),
    });
  for (const option of [
    "remaining",
    "qb",
    "queue-knowledge",
    "next-cycle-remaining",
    "max-setup-pieces",
  ]) {
    addInvalidCases(cases, rows, "setup", option, (value) =>
      setupArguments([setting("setup", option, value)]), "options.invalid");
  }
  addInvalidCases(cases, rows, "damage", "source", (value) =>
    damageArguments([setting("damage", "source", value)]), "source.invalid");
  for (const option of [
    "spin-profile",
    "minimum-damage",
    "initial-combo",
    "initial-b2b",
  ]) {
    addInvalidCases(cases, rows, "damage", option, (value) =>
      damageArguments([setting("damage", option, value)]), "options.invalid");
  }
  addInvalidCases(cases, rows, "spin-finder", "source", () =>
    buildSlashCommandArguments(findSlashCommand("spin"), [
      { name: "field", value: EMPTY_FIELD },
      { name: "next", value: "-" },
    ]), "source.invalid", { include: new Set(["empty"]) });
  addInvalidCases(cases, rows, "spin-finder", "inventory", (value) =>
    spinStructureArguments([setting("spin-finder", "inventory", value)]), "pieces.invalid");
  for (const option of ["fill-bottom", "fill-top", "max-placements", "minimality"]) {
    addInvalidCases(cases, rows, "spin-finder", option, (value) =>
      spinStructureArguments([setting("spin-finder", option, value)]), "options.invalid");
  }

  for (const { label, run, code } of cases) {
    assert.throws(run, (error) =>
      error instanceof DiscordInputError && error.code === code,
      label,
    );
  }
  assert.equal(cases.length, 22, "exact represented invalid fixture cases");

  // The PC fixture is shared by opening and scenario surfaces. Discord's
  // curated sfinder aliases compile to Scenario, where an odd line height is
  // valid, so the opening-only invalid representative must remain accepted.
  assert.doesNotThrow(() => pcArguments([setting("pc", "lines", "1")]));

  // A single Discord `next` field cannot express the direct typed conflict of
  // supplying both a fixed queue and a pattern. The backend-facing argv has
  // exactly one canonical source switch, so those abstract `both` rows are not
  // silently counted as parser executions.
  for (const name of ["path", "cover", "spin"]) {
    const command = findSlashCommand(name);
    assert.equal(command.registration.options.filter(({ name }) => name === "next").length, 1);
    assert.equal(command.registration.options.some(({ name }) =>
      name === "queue" || name === "patterns"
    ), false);
  }
});

const DISCORD_FIXTURE_SCOPES = Object.freeze([
  scope("pc", "pc", ["lines", "source", "hold", "rule"]),
  scope("setup", "setup", [
    "mode",
    "remaining",
    "qb",
    "queue-knowledge",
    "next-cycle-remaining",
    "post-cycle-borrow",
    "priority",
    "length",
    "max-setup-pieces",
    "rule",
  ]),
  scope("build", "build", ["source", "hold", "rule"]),
  scope("damage", "damage", [
    "source",
    "hold",
    "rule",
    "spin-profile",
    "damage-mode",
    "minimum-damage",
    "initial-combo",
    "initial-b2b",
    "preserve-b2b",
  ]),
  scope("spin-source", "spin-finder", ["source"]),
  scope("spin-structure", "spin-finder", [
    "inventory",
    "fill-bottom",
    "fill-top",
    "max-placements",
    "minimality",
  ]),
]);

// Filled from the deterministic fixture matrix. Keeping exact totals makes it
// impossible to silently lose a representative or legal pair while retaining
// a superficially green schema-enumeration test.
const EXPECTED_PRODUCTION_MATRIX_COUNTS = Object.freeze({
  pc: { semanticCases: 65, parserExecutions: 118 },
  setup: { semanticCases: 433, parserExecutions: 835 },
  build: { semanticCases: 35, parserExecutions: 61 },
  damage: { semanticCases: 382, parserExecutions: 735 },
  "spin-source": { semanticCases: 2, parserExecutions: 2 },
  "spin-structure": { semanticCases: 85, parserExecutions: 156 },
});

function scope(id, family, options) {
  return Object.freeze({ id, family, options: new Set(options) });
}

function addInvalidCases(
  cases,
  rows,
  family,
  option,
  build,
  code,
  filters = {},
) {
  const row = rows.find((candidate) =>
    candidate.family === family && candidate.option === option
  );
  assert.ok(row, `${family}.${option} fixture row`);
  for (const value of row.invalid) {
    if (filters.include && !filters.include.has(value)) continue;
    if (filters.exclude?.has(value)) continue;
    cases.push({
      label: `${family}.${option}=${value}`,
      run: () => build(value),
      code,
    });
  }
}

function setting(family, option, value) {
  return Object.freeze({ row: Object.freeze({ family, option }), value });
}

function assertProductionParserCase(scopeId, settings) {
  const forward = fixtureArguments(scopeId, settings);
  assert.ok(forward.length > 0, fixtureLabel(settings));
  if (settings.length === 1) return 1;
  const reverse = fixtureArguments(scopeId, [...settings].reverse());
  assert.deepEqual(reverse, forward, fixtureLabel(settings));
  return 2;
}

function fixtureArguments(scopeId, settings) {
  switch (scopeId) {
    case "pc":
      return pcArguments(settings);
    case "setup":
      return setupArguments(settings);
    case "build":
      return buildArguments(settings);
    case "damage":
      return damageArguments(settings);
    case "spin-source":
      return spinSourceArguments(settings);
    case "spin-structure":
      return spinStructureArguments(settings);
    default:
      throw new Error(`unmapped Discord fixture scope '${scopeId}'`);
  }
}

function pcArguments(settings) {
  const raw = [{ name: "field", value: EMPTY_FIELD }];
  const packed = [];
  let hasSource = false;
  for (const { row: { option }, value } of settings) {
    if (option === "source") {
      raw.push({ name: "next", value: sourceValue(value) });
      hasSource = true;
    } else if (option === "lines") {
      raw.push({ name: "lines", value: Number(value) });
    } else if (option === "hold") {
      packed.push(`hold=${value}`);
    } else if (option === "rule") {
      raw.push({ name: "kicktable", value });
    }
  }
  if (!hasSource) raw.push({ name: "next", value: "IOTS" });
  appendPacked(raw, packed);
  return buildSlashCommandArguments(findSlashCommand("path"), raw);
}

function buildArguments(settings) {
  const raw = [
    { name: "base", value: EMPTY_FIELD },
    { name: "target", value: FOUR_CELL_TARGET },
  ];
  const packed = [];
  let hasSource = false;
  for (const { row: { option }, value } of settings) {
    if (option === "source") {
      raw.push({ name: "next", value: sourceValue(value) });
      hasSource = true;
    } else if (option === "hold") {
      packed.push(`hold=${value}`);
    } else if (option === "rule") {
      raw.push({ name: "kicktable", value });
    }
  }
  if (!hasSource) raw.push({ name: "next", value: "IOTS" });
  appendPacked(raw, packed);
  return buildSlashCommandArguments(findSlashCommand("cover"), raw);
}

function damageArguments(settings) {
  const raw = [{ name: "field", value: EMPTY_FIELD }];
  const packed = [];
  const values = fixtureValueMap(settings);
  let hasSource = false;
  for (const { row: { option }, value } of settings) {
    if (option === "source") {
      raw.push({ name: "next", value: value === "fixed" ? "IOTS" : value });
      hasSource = true;
    } else if (option === "hold") {
      packed.push(`hold=${value}`);
    } else if (option === "rule") {
      raw.push({ name: "kicktable", value });
    } else if (option === "spin-profile") {
      packed.push(`spin-profile=${value}`);
    } else if (option === "damage-mode") {
      if (value === "at-least" && !values.has("minimum-damage")) {
        packed.push("minimum-damage=0");
      }
    } else if ([
      "minimum-damage",
      "initial-combo",
      "initial-b2b",
      "preserve-b2b",
    ].includes(option)) {
      packed.push(`${option}=${value}`);
    }
  }
  if (!hasSource) raw.push({ name: "next", value: "IOTS" });
  appendPacked(raw, packed);
  return buildSlashCommandArguments(findSlashCommand("damage"), raw);
}

function spinSourceArguments(settings) {
  const source = settings.find(({ row }) => row.option === "source")?.value ?? "fixed";
  return buildSlashCommandArguments(findSlashCommand("spin"), [
    { name: "field", value: EMPTY_FIELD },
    { name: "next", value: sourceValue(source) },
  ]);
}

function spinStructureArguments(settings, options = {}) {
  const raw = [{ name: "field", value: TWENTY_FOUR_ROW_FIELD }];
  const packed = [];
  const values = fixtureValueMap(settings);
  let hasInventory = false;
  for (const { row: { option }, value } of settings) {
    if (option === "inventory") {
      raw.push({ name: "pieces", value });
      hasInventory = true;
    } else if (["fill-bottom", "fill-top", "max-placements", "minimality"].includes(option)) {
      packed.push(`${option}=${value}`);
    }
  }
  if (!hasInventory) raw.push({ name: "pieces", value: "IOTSZJL" });
  if (values.has("fill-bottom") && !values.has("fill-top") && !options.preserveConflict) {
    packed.push("fill-top=24");
  }
  appendPacked(raw, packed);
  return buildSlashCommandArguments(findSlashCommand("spin-structure"), raw);
}

function setupArguments(settings, options = {}) {
  const values = fixtureValueMap(settings);
  const remaining = effectiveSetupRemaining(values);
  const raw = [];
  const packed = [];
  let hasRemaining = false;
  for (const { row: { option }, value } of settings) {
    if (option === "remaining") {
      raw.push({ name: "remaining", value });
      hasRemaining = true;
    } else if (option === "mode") {
      packed.push(`mode=${value}`);
    } else if (option === "qb") {
      packed.push(`qb=${value}`);
    } else if (option === "queue-knowledge") {
      raw.push({ name: "queue-knowledge", value });
    } else if (option === "next-cycle-remaining") {
      raw.push({ name: "next-cycle-remaining", value });
    } else if (option === "post-cycle-borrow") {
      packed.push(`post-cycle-borrow=${value}`);
    } else if (option === "priority") {
      raw.push({ name: "priority", value });
    } else if (option === "length") {
      raw.push({ name: "setup-length", value });
    } else if (option === "max-setup-pieces") {
      raw.push({ name: "max-setup-pieces", value: Number(value) });
    } else if (option === "rule") {
      raw.push({ name: "kicktable", value });
    }
  }
  if (!hasRemaining) raw.push({ name: "remaining", value: remaining });
  const mode = values.get("mode");
  const qb = values.get("qb");
  if (mode === "qb" && qb === undefined && !options.omitInferredQb) packed.push("qb=I");
  if (qb !== undefined && mode === undefined) packed.unshift("mode=qb");
  if (options.preserveConflict && mode === "oracle" && qb !== undefined) {
    // The explicit conflict is already represented by the two packed entries.
  }
  appendPacked(raw, packed);
  return buildSlashCommandArguments(findSlashCommand("pc-setup"), raw);
}

function isLegalFixtureCase(scopeId, settings) {
  const values = fixtureValueMap(settings);
  if (scopeId === "damage") {
    return !(values.get("damage-mode") === "maximum" && values.has("minimum-damage"));
  }
  if (scopeId === "setup") {
    const remaining = effectiveSetupRemaining(values);
    const mode = values.get("mode");
    const qb = values.get("qb") ?? (mode === "qb" ? "I" : "");
    if (mode === "oracle" && qb) return false;
    if (qb && remaining.length + qb.length > 7) return false;
    if (values.get("post-cycle-borrow") === "on" && remaining.length !== 3) return false;
    const next = values.get("next-cycle-remaining");
    if (next !== undefined && next.length !== nextCycleRemainingCount(remaining.length)) {
      return false;
    }
  }
  if (scopeId === "spin-structure") {
    const bottom = Number(values.get("fill-bottom") ?? 0);
    const top = Number(values.get("fill-top") ?? 24);
    if (bottom >= top) return false;
    const pieces = values.get("inventory") ?? "IOTSZJL";
    if (Number(values.get("max-placements") ?? 1) > pieces.length) return false;
  }
  return true;
}

function fixtureValueMap(settings) {
  return new Map(settings.map(({ row, value }) => [row.option, value]));
}

function effectiveSetupRemaining(values) {
  if (values.has("remaining")) return values.get("remaining");
  const next = values.get("next-cycle-remaining");
  if (next !== undefined) {
    return ({ 1: "IOTS", 4: "IOTSZJL", 7: "IOT" })[next.length];
  }
  return "IOT";
}

function nextCycleRemainingCount(remainingCount) {
  return ({ 7: 4, 4: 1, 1: 5, 5: 2, 2: 6, 6: 3, 3: 7 })[remainingCount];
}

function sourceValue(value) {
  return ({ fixed: "IOTS", pattern: "*p4", empty: "*!" })[value] ?? value;
}

function appendPacked(raw, packed) {
  if (packed.length > 0) raw.push({ name: "options", value: packed.join(" ") });
}

function fixtureLabel(settings) {
  return settings.map(({ row, value }) => `${row.family}.${row.option}=${value}`).join(" + ");
}

const EMPTY_FIELD = "__________";
const FOUR_CELL_TARGET = "XXXX______";
const TWENTY_FOUR_ROW_FIELD = `grid:${[
  "#_________",
  ...Array(23).fill("__________"),
].join("/")}`;

function contractRows() {
  return readFileSync(CONTRACT_URL, "utf8")
    .split(/\r?\n/)
    .filter((line) => line && !line.startsWith("#"))
    .map((line) => {
      const [
        family,
        option,
        kind,
        valid,
        invalid,
        webDefault,
        nativeDefault,
        surface,
        constraints,
      ] = line.split("\t");
      return {
        family,
        option,
        kind,
        valid: representatives(valid),
        invalid: representatives(invalid),
        webDefault,
        nativeDefault,
        surface,
        constraints,
      };
    });
}

function representatives(value) {
  return value === "-" ? [] : value.split("|");
}

function registrationOptions(command) {
  if (command.input === "finesse") {
    return Object.values(command.subcommands).flatMap((subcommand) =>
      subcommand.registration.options
    );
  }
  return command.registration.options ?? [];
}

function escapeRegex(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
