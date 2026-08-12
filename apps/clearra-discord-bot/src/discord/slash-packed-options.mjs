import { tokenizeCommand } from "../clearra/command.mjs";
import { DiscordInputError } from "./i18n.mjs";

const SETTINGS_MAX_LENGTH = 256;
const DAMAGE_SPIN_PROFILES = new Set([
  "disabled",
  "t-spin-simple",
  "t-spins",
  "t-spins-plus",
  "all-mini",
  "all-mini-plus",
  "all-spin",
  "all-spin-plus",
]);
const SPIN_PROFILES = new Set([
  "t-spins",
  "t-spins-plus",
  "all-mini",
  "all-mini-plus",
  "all-spin",
  "all-spin-plus",
]);
const SPIN_STRUCTURE_MINIMALITY = new Set([
  "subset-minimal",
  "minimum-piece-count",
]);

export const DISCORD_PACKED_OPTION_KEYS = Object.freeze({
  pc: Object.freeze(["hold"]),
  cover: Object.freeze(["hold"]),
  spin: Object.freeze(["type"]),
  "score-fixed-next": Object.freeze(["initial-b2b"]),
  remaining: Object.freeze(["mode", "qb", "post-cycle-borrow"]),
  "fixed-next": Object.freeze([
    "hold",
    "spin-profile",
    "minimum-damage",
    "initial-combo",
    "initial-b2b",
    "preserve-b2b",
  ]),
  "spin-structure": Object.freeze([
    "fill-bottom",
    "fill-top",
    "max-placements",
    "minimality",
  ]),
  "finesse-search": Object.freeze([
    "hold",
    "knowledge",
    "source-pieces",
    "aggregation",
    "spin-profile",
    "preserve-b2b",
  ]),
  "finesse-score": Object.freeze(["hold", "knowledge", "source-pieces"]),
});

export function setupFinderPackedArguments(command, values, remaining) {
  const settings = parseSettings(command, values, new Map([
    ["mode", "mode"],
    ["qb", "qb"],
    ["post-cycle-borrow", "post-cycle-borrow"],
    ["post_cycle_borrow", "post-cycle-borrow"],
    ["borrow", "post-cycle-borrow"],
  ]));
  const output = [];
  const requestedMode = settings.has("mode")
    ? normalizedSetting(settings.get("mode"))
    : null;
  if (requestedMode !== null && !["oracle", "qb"].includes(requestedMode)) {
    throw invalidOption("mode", "options mode must be oracle or qb.");
  }

  const qbSource = settings.get("qb") ?? null;
  if (requestedMode === "qb" && qbSource === null) {
    throw new DiscordInputError("options.setup_qb_required");
  }
  if (requestedMode === "oracle" && qbSource !== null) {
    throw new DiscordInputError("options.setup_qb_oracle_conflict");
  }
  if (qbSource !== null) {
    const qb = pieceInventory(qbSource, "qb");
    if (qb.length > 7 || new Set(qb).size !== qb.length) {
      throw invalidOption(
        "qb",
        "options qb must contain from 1 through 7 unique IOTSZJL pieces.",
      );
    }
    if (qb.length + remaining.length > 7) {
      throw new DiscordInputError("options.setup_qb_bag_capacity");
    }
    output.push("--mode", "qb", "--qb", qb);
  } else if (requestedMode !== null) {
    output.push("--mode", requestedMode);
  }

  if (settings.has("post-cycle-borrow")) {
    const enabled = booleanSetting(
      settings.get("post-cycle-borrow"),
      "post-cycle-borrow",
    ) === "true";
    if (enabled && remaining.length !== 3) {
      throw new DiscordInputError("options.setup_borrow_cycle");
    }
    if (enabled) output.push("--allow-post-cycle-borrow");
  }
  return output;
}

export function damagePackedArguments(command, values) {
  const settings = parseSettings(command, values, new Map([
    ["hold", "hold"],
    ["spin-profile", "spin-profile"],
    ["spin_profile", "spin-profile"],
    ["profile", "spin-profile"],
    ["minimum-damage", "minimum-damage"],
    ["minimum_damage", "minimum-damage"],
    ["initial-combo", "initial-combo"],
    ["initial_combo", "initial-combo"],
    ["initial-b2b", "initial-b2b"],
    ["initial_b2b", "initial-b2b"],
    ["preserve-b2b", "preserve-b2b"],
    ["preserve_b2b", "preserve-b2b"],
  ]));
  const output = [];
  if (settings.has("hold")) {
    output.push(booleanSetting(settings.get("hold"), "hold") === "true"
      ? "--hold"
      : "--no-hold");
  }
  if (settings.has("spin-profile")) {
    const profile = normalizedSetting(settings.get("spin-profile"));
    if (!DAMAGE_SPIN_PROFILES.has(profile)) {
      throw invalidOption(
        "spin-profile",
        "options spin-profile must be disabled, t-spin-simple, T-Spins(+), All-Mini(+), or All-Spin(+).",
      );
    }
    output.push("--spin-profile", profile);
  }
  if (settings.has("minimum-damage")) {
    output.push(
      "--minimum-damage",
      String(integerSetting(settings.get("minimum-damage"), "minimum-damage", 0, 4_294_967_295)),
    );
  }
  if (settings.has("initial-combo")) {
    const combo = integerSetting(settings.get("initial-combo"), "initial-combo", 0, 65_535);
    if (combo > 0) output.push("--initial-combo", String(combo));
  }
  if (settings.has("initial-b2b")) {
    output.push(
      "--initial-b2b",
      String(integerSetting(settings.get("initial-b2b"), "initial-b2b", 0, 65_535)),
    );
  }
  if (
    settings.has("preserve-b2b") &&
    booleanSetting(settings.get("preserve-b2b"), "preserve-b2b") === "true"
  ) {
    output.push("--preserve-b2b");
  }
  return output;
}

export function spinStructurePackedArguments(
  command,
  values,
  fieldHeight,
  pieceCount,
) {
  const settings = parseSettings(command, values, new Map([
    ["fill-bottom", "fill-bottom"],
    ["fill_bottom", "fill-bottom"],
    ["fill-top", "fill-top"],
    ["fill_top", "fill-top"],
    ["max-placements", "max-placements"],
    ["max_placements", "max-placements"],
    ["minimality", "minimality"],
  ]));
  const effectiveHeight = Math.max(8, fieldHeight);
  const fillBottom = settings.has("fill-bottom")
    ? integerSetting(settings.get("fill-bottom"), "fill-bottom", 0, effectiveHeight - 1)
    : 0;
  const fillTop = settings.has("fill-top")
    ? integerSetting(settings.get("fill-top"), "fill-top", 1, effectiveHeight)
    : Math.min(5, effectiveHeight);
  if (fillBottom >= fillTop) {
    throw new DiscordInputError("options.spin_fill_bounds");
  }
  const output = [];
  if (settings.has("fill-bottom")) output.push("--fill-bottom", String(fillBottom));
  if (settings.has("fill-top")) output.push("--fill-top", String(fillTop));
  if (settings.has("max-placements")) {
    output.push(
      "--max-placements",
      String(integerSetting(settings.get("max-placements"), "max-placements", 1, pieceCount)),
    );
  }
  if (settings.has("minimality")) {
    const minimality = normalizedSetting(settings.get("minimality"));
    if (!SPIN_STRUCTURE_MINIMALITY.has(minimality)) {
      throw invalidOption(
        "minimality",
        "options minimality must be subset-minimal or minimum-piece-count.",
      );
    }
    output.push("--minimality", minimality);
  }
  return output;
}

export function finessePackedArguments(command, values) {
  const settings = parseSettings(command, values, new Map([
    ["hold", "hold"],
    ["knowledge", "knowledge"],
    ["queue-knowledge", "knowledge"],
    ["pattern-knowledge", "knowledge"],
    ["source-pieces", "source-pieces"],
    ["source_pieces", "source-pieces"],
    ["aggregation", "aggregation"],
    ["aggregate", "aggregation"],
    ["spin-profile", "spin-profile"],
    ["spin_profile", "spin-profile"],
    ["preserve-b2b", "preserve-b2b"],
    ["preserve_b2b", "preserve-b2b"],
  ]));
  const output = [];
  const hold = normalizedSetting(settings.get("hold") ?? "empty");
  if (["true", "yes", "on", "use", "empty"].includes(hold)) {
    output.push("--hold", "empty");
  } else if (["false", "no", "off", "avoid"].includes(hold)) {
    output.push("--no-hold");
  } else if (/^[IOTSZJL]$/i.test(hold)) {
    output.push("--hold", hold.toUpperCase());
  } else {
    throw invalidOption(
      "hold",
      "options hold must be empty, avoid, or one IOTSZJL piece.",
    );
  }

  const requestedKnowledge = normalizedSetting(settings.get("knowledge") ?? "both");
  const knowledge = requestedKnowledge === "full-queue" ? "oracle" : requestedKnowledge;
  if (!["both", "oracle", "visible-7"].includes(knowledge)) {
    throw invalidOption(
      "knowledge",
      "options knowledge must be both, full-queue, or visible-7.",
    );
  }
  output.push("--pattern-knowledge", knowledge);

  if (settings.has("source-pieces")) {
    output.push(
      "--source-pieces",
      String(integerSetting(settings.get("source-pieces"), "source-pieces", 1, 128)),
    );
  }

  const searchMode = command.input === "finesse-search";
  if (!searchMode && ["aggregation", "spin-profile", "preserve-b2b"].some((key) => settings.has(key))) {
    throw new DiscordInputError("options.finesse_score_unsupported");
  }
  if (searchMode && settings.has("aggregation")) {
    const aggregation = normalizedSetting(settings.get("aggregation"));
    if (!["buildability", "build", "spin"].includes(aggregation)) {
      throw invalidOption(
        "aggregation",
        "options aggregation must be buildability or spin; tiling is not exposed through Discord.",
      );
    }
    output.push("--aggregate", aggregation === "build" ? "buildability" : aggregation);
  }
  if (searchMode && settings.has("spin-profile")) {
    const profile = normalizedSetting(settings.get("spin-profile"));
    if (!SPIN_PROFILES.has(profile)) {
      throw invalidOption(
        "spin-profile",
        "options spin-profile must be T-Spins(+), All-Mini(+), or All-Spin(+).",
      );
    }
    const spinAggregation = normalizedSetting(settings.get("aggregation") ?? "") === "spin";
    const preservesB2b = settings.has("preserve-b2b") &&
      booleanSetting(settings.get("preserve-b2b"), "preserve-b2b") === "true";
    if (!spinAggregation && !preservesB2b) {
      throw new DiscordInputError("options.finesse_spin_dependency");
    }
    output.push("--spin-profile", profile);
  }
  if (
    searchMode &&
    settings.has("preserve-b2b") &&
    booleanSetting(settings.get("preserve-b2b"), "preserve-b2b") === "true"
  ) {
    output.push("--preserve-b2b");
  }
  return output;
}

export function parseSettings(command, values, aliases) {
  const source = optionalText(values, "options", SETTINGS_MAX_LENGTH);
  const settings = new Map();
  if (!source) return settings;
  for (const token of tokenizeCommand(source)) {
    const equals = token.indexOf("=");
    if (equals <= 0 || equals === token.length - 1) {
      throw new Error(`/${command.name} options must use space-separated key=value entries.`);
    }
    const suppliedKey = token.slice(0, equals).trim().toLowerCase();
    const key = aliases.get(suppliedKey);
    if (!key) {
      throw new Error(`/${command.name} does not support options key '${suppliedKey}'.`);
    }
    if (settings.has(key)) {
      throw new Error(`/${command.name} options key '${key}' may be specified only once.`);
    }
    settings.set(key, token.slice(equals + 1).trim());
  }
  return settings;
}

export function booleanSetting(value, name) {
  switch (String(value).toLowerCase()) {
    case "true":
    case "yes":
    case "on":
    case "use":
      return "true";
    case "false":
    case "no":
    case "off":
    case "avoid":
      return "false";
    default:
      throw invalidOption(
        name,
        `options ${name} must be use or avoid (true or false).`,
      );
  }
}

function normalizedSetting(value) {
  return String(value ?? "").trim().toLowerCase().replaceAll("_", "-");
}

function integerSetting(value, name, minimum, maximum) {
  const source = String(value ?? "").trim();
  if (!/^\d+$/.test(source)) {
    throw invalidOption(
      name,
      `options ${name} must be an integer from ${minimum} through ${maximum}.`,
    );
  }
  const parsed = Number(source);
  if (!Number.isSafeInteger(parsed) || parsed < minimum || parsed > maximum) {
    throw invalidOption(
      name,
      `options ${name} must be an integer from ${minimum} through ${maximum}.`,
    );
  }
  return parsed;
}

function pieceInventory(value, name) {
  const source = String(value).trim();
  if (!/^[IOTSZJL]+$/i.test(source)) {
    throw invalidOption(name, `options ${name} must contain only IOTSZJL pieces.`);
  }
  return source.toUpperCase();
}

function invalidOption(option, message) {
  return new DiscordInputError("options.invalid", { option }, message);
}

function optionalText(values, name, maxLength) {
  if (!values.has(name)) return null;
  const value = values.get(name);
  if (typeof value !== "string") throw new Error(`${name} must be text.`);
  const normalized = value.trim();
  if (!normalized) throw new Error(`${name} cannot be empty.`);
  if (normalized.length > maxLength) {
    throw new Error(`${name} exceeds the ${maxLength}-character limit.`);
  }
  return normalized;
}
