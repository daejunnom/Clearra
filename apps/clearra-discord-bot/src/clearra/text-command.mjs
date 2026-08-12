import {
  prepareClearraArguments,
  tokenizeCommand,
} from "./command.mjs";
import { findSlashCommand } from "../discord/slash-command-catalog.mjs";
import {
  buildSlashCommandArgumentPlan,
  DISCORD_PACKED_OPTION_KEYS,
} from "../discord/slash-command-input.mjs";

const HOST_CONTROLLED_OPTIONS = new Map([
  ["--tablebase", 0],
  ["--no-tablebase", 0],
  ["--tb", 0],
  ["--no-tb", 0],
  ["--build-dependency-dag", 0],
  ["--no-build-dependency-dag", 0],
  ["--workers", 1],
  ["--auto-workers", 1],
  ["--cpu-threads", 1],
  ["--use-all-cpu-threads", 0],
  ["--format", 1],
  ["--include-solution-data", 0],
]);

const OPTION_ALIASES = new Map([
  ["--field", "field"],
  ["--fumen", "field"],
  ["--tetfu", "field"],
  ["-t", "field"],
  ["--base", "base"],
  ["--target", "target"],
  ["--next", "next"],
  ["--patterns", "next"],
  ["--pattern", "next"],
  ["--queue", "next"],
  ["-p", "next"],
  ["--lines", "lines"],
  ["--clear", "lines"],
  ["-c", "lines"],
  ["--kicktable", "kicktable"],
  ["--rule", "kicktable"],
  ["--options", "options"],
  ["--remaining", "remaining"],
  ["--priority", "priority"],
  ["--setup-order", "priority"],
  ["--max-setup-pieces", "max-setup-pieces"],
  ["--next-cycle-remaining", "next-cycle-remaining"],
  ["--setup-length", "setup-length"],
  ["--pieces", "pieces"],
  ["--inventory", "pieces"],
  ["--profile", "profile"],
  ["--spin-profile", "profile"],
  ["--scope", "scope"],
  ["--image", "image"],
  ["--document", "document"],
]);

const PACKED_TEXT_OPTIONS = new Map([
  ["--mode", "mode"],
  ["--qb", "qb"],
  ["--post-cycle-borrow", "post-cycle-borrow"],
  ["--minimum-damage", "minimum-damage"],
  ["--initial-combo", "initial-combo"],
  ["--initial-b2b", "initial-b2b"],
  ["--spin-profile", "spin-profile"],
  ["--fill-bottom", "fill-bottom"],
  ["--fill-top", "fill-top"],
  ["--max-placements", "max-placements"],
  ["--minimality", "minimality"],
  ["--source-pieces", "source-pieces"],
  ["--aggregate", "aggregation"],
  ["--aggregation", "aggregation"],
]);

const PACKED_BOOLEAN_FLAGS = new Map([
  ["--allow-post-cycle-borrow", ["post-cycle-borrow", "on"]],
  ["--no-post-cycle-borrow", ["post-cycle-borrow", "off"]],
  ["--preserve-b2b", ["preserve-b2b", "on"]],
  ["--no-preserve-b2b", ["preserve-b2b", "off"]],
]);

export function parseClearraTextMessage(
  content,
  prefix = "!",
  execution = {},
) {
  return parseClearraTextRequest(content, prefix, execution)?.arguments_ ?? null;
}

export function parseClearraTextRequest(
  content,
  prefix = "!",
  execution = {},
) {
  const resolution = resolveTextCommand(content, prefix);
  if (!resolution) return null;
  const { tokens, explicitSfinder, command, argumentStart } = resolution;
  if (!explicitSfinder && command?.kind === "help") {
    return helpRequest(command, tokens.slice(1));
  }
  if (!explicitSfinder && command?.kind === "render-file") {
    return nonSearchRequest(command, tokens.slice(1));
  }
  if (!command || command.kind !== "search") return null;
  const rawOptions = readCatalogTextOptions(
    command,
    tokens.slice(argumentStart),
  );
  const argumentPlan = buildSlashCommandArgumentPlan(command, rawOptions);
  const argumentSets = freezeArgumentSets(
    argumentPlan.argumentSets
      .map((arguments_) => prepareClearraArguments(arguments_, execution)),
  );
  return Object.freeze({
    argumentSets,
    arguments_: argumentSets.length === 1 ? argumentSets[0] : null,
    automaticPcTargets: argumentPlan.automaticPcTargets,
    command,
    helpTarget: null,
    rawOptions: Object.freeze(
      rawOptions.map((option) => Object.freeze({ ...option })),
    ),
  });
}

/**
 * Returns only the allow-listed command identity selected by the text parser.
 * Arguments are never retained. This deliberately shares command resolution
 * with parseClearraTextRequest so aliases and the curated explicit sfinder
 * route cannot drift away from private operational telemetry.
 */
export function classifyClearraTextCommand(content, prefix = "!") {
  let resolution;
  try {
    resolution = resolveTextCommand(content, prefix);
  } catch {
    // Command identity is independent from argument validity. In particular,
    // an unterminated quote or code block in a field must still be recorded as
    // the same allow-listed command that will return the validation error.
    resolution = resolveTextCommandHead(content, prefix);
  }
  if (!resolution) return null;
  return commandIdentityFromResolution(resolution);
}

function commandIdentityFromResolution(resolution) {
  const { command } = resolution;
  return command?.subcommand
    ? `${command.rootName ?? command.name}.${command.subcommand}`
    : command?.name ?? null;
}

function resolveTextCommandHead(content, prefix) {
  if (typeof prefix !== "string" || prefix.length === 0) return null;
  const trimmed = String(content ?? "").trim();
  if (!trimmed.startsWith(prefix)) return null;
  const body = trimmed.slice(prefix.length).trim();
  if (!body) return null;

  // Only command-path tokens are read here. Values after that boundary are
  // deliberately ignored, so this fallback cannot retain fields, queues, or
  // other user input in operational telemetry.
  const tokens = body.split(/\s+/u, 3);
  if (usesRetiredCatFinderName(tokens)) return null;
  const first = tokens[0]?.toLowerCase();
  if (first === "clearra") return null;
  let namespaceEnd = 0;
  const explicitSfinder = tokens[namespaceEnd]?.toLowerCase() === "sfinder";
  if (explicitSfinder) namespaceEnd += 1;
  const commandName = tokens[namespaceEnd];
  const candidate = findSlashCommand(normalizeCatalogName(commandName));
  const root = explicitSfinder && candidate?.argvPrefix?.[0] !== "sfinder"
    ? null
    : candidate;
  const command = resolveFinesseTextVariant(root, tokens, namespaceEnd + 1, false);
  return {
    tokens,
    first,
    explicitSfinder,
    command,
    argumentStart: command?.subcommand ? namespaceEnd + 2 : namespaceEnd + 1,
  };
}

function resolveTextCommand(content, prefix) {
  if (typeof prefix !== "string" || prefix.length === 0) return null;
  const trimmed = String(content ?? "").trim();
  if (!trimmed.startsWith(prefix)) return null;
  const body = trimmed.slice(prefix.length).trim();
  if (!body) return null;

  const tokens = tokenizeTextCommand(body);
  if (usesRetiredCatFinderName(tokens)) return null;
  const first = tokens[0]?.toLowerCase();
  if (first === "clearra") return null;
  let namespaceEnd = 0;
  const explicitSfinder = tokens[namespaceEnd]?.toLowerCase() === "sfinder";
  if (explicitSfinder) namespaceEnd += 1;
  const commandName = tokens[namespaceEnd];
  const candidate = findSlashCommand(normalizeCatalogName(commandName));
  const root = explicitSfinder && candidate?.argvPrefix?.[0] !== "sfinder"
    ? null
    : candidate;
  const baseArgumentStart = namespaceEnd + 1;
  const command = resolveFinesseTextVariant(root, tokens, baseArgumentStart, true);
  return {
    tokens,
    first,
    explicitSfinder,
    command,
    argumentStart: command?.subcommand ? baseArgumentStart + 1 : baseArgumentStart,
  };
}

function resolveFinesseTextVariant(command, tokens, subcommandIndex, strict) {
  if (command?.input !== "finesse") return command;
  const name = normalizeCatalogName(tokens[subcommandIndex]);
  const variant = command.subcommands?.[name] ?? null;
  if (!variant && strict) {
    throw new Error("Text command /finesse requires a search or score subcommand.");
  }
  return variant;
}

function usesRetiredCatFinderName(tokens) {
  let cursor = 0;
  if (tokens[cursor]?.toLowerCase() === "sfinder") cursor += 1;
  const name = normalizeCatalogName(tokens[cursor]);
  return name === "cat-finder" || name === "catfinder";
}

function nonSearchRequest(command, tokens) {
  const rawOptions = readCatalogTextOptions(command, tokens);
  return Object.freeze({
    argumentSets: Object.freeze([]),
    arguments_: null,
    automaticPcTargets: false,
    command,
    helpTarget: null,
    rawOptions: Object.freeze(
      rawOptions.map((option) => Object.freeze({ ...option })),
    ),
  });
}

function helpRequest(command, tokens) {
  return Object.freeze({
    argumentSets: Object.freeze([]),
    arguments_: null,
    automaticPcTargets: false,
    command,
    helpTarget: readTextHelpTarget(tokens),
    rawOptions: Object.freeze([]),
  });
}

function readTextHelpTarget(tokens) {
  if (tokens.length === 0) return null;

  let target;
  if (tokens[0].toLowerCase() === "--arguments") {
    if (tokens.length !== 2) {
      throw new Error("--arguments requires exactly one command name.");
    }
    target = tokens[1];
  } else if (tokens.length === 1) {
    const parsed = optionToken(tokens[0]);
    if (parsed) {
      if (parsed.name !== "--arguments") {
        throw new Error(`Text command /help does not expose option '${parsed.name}'.`);
      }
      if (parsed.value === null) {
        throw new Error("--arguments requires exactly one command name.");
      }
      target = parsed.value;
    } else if (tokens[0].toLowerCase().startsWith("arguments:")) {
      target = tokens[0].slice("arguments:".length);
    } else {
      target = tokens[0];
    }
  } else {
    throw new Error("Text command /help accepts at most one command name.");
  }

  const normalized = String(target ?? "").trim();
  if (!normalized) throw new Error("help arguments cannot be empty.");
  if (normalized.length > 64) {
    throw new Error("help arguments exceeds the 64-character limit.");
  }
  return normalized;
}

function freezeArgumentSets(argumentSets) {
  return Object.freeze(
    argumentSets.map((arguments_) => Object.freeze([...arguments_])),
  );
}

function readCatalogTextOptions(command, tokens) {
  if (tokens.length > 256) throw new Error("The command has too many arguments.");
  const options = [];
  const positional = [];
  const settings = [];

  for (let index = 0; index < tokens.length; index += 1) {
    const token = tokens[index];
    const parsed = optionToken(token);
    if (!parsed) {
      positional.push(token);
      continue;
    }

    const controlledWidth = HOST_CONTROLLED_OPTIONS.get(parsed.name);
    if (controlledWidth !== undefined) {
      if (controlledWidth === 1 && parsed.value === null) index += 1;
      continue;
    }
    if (parsed.name === "--no-hold") {
      settings.push("hold=avoid");
      continue;
    }
    const booleanPacked = PACKED_BOOLEAN_FLAGS.get(parsed.name);
    if (booleanPacked && supportsPackedKey(command, booleanPacked[0])) {
      settings.push(`${booleanPacked[0]}=${booleanPacked[1]}`);
      continue;
    }
    if (parsed.name === "--hold" || parsed.name === "-h") {
      const value = optionValue(tokens, index, parsed, "--hold");
      if (parsed.value === null) index += 1;
      settings.push(`hold=${value}`);
      continue;
    }
    if (parsed.name === "--type") {
      const value = optionValue(tokens, index, parsed, "--type");
      if (parsed.value === null) index += 1;
      settings.push(`type=${value}`);
      continue;
    }
    if (parsed.name === "--knowledge" || parsed.name === "--queue-knowledge" || parsed.name === "--pattern-knowledge") {
      const value = optionValue(tokens, index, parsed, parsed.name);
      if (parsed.value === null) index += 1;
      if (command.input === "remaining") {
        options.push({ name: "queue-knowledge", value });
      } else {
        settings.push(`knowledge=${value}`);
      }
      continue;
    }
    const packedKey = PACKED_TEXT_OPTIONS.get(parsed.name);
    if (packedKey && supportsPackedKey(command, packedKey)) {
      const value = optionValue(tokens, index, parsed, parsed.name);
      if (parsed.value === null) index += 1;
      settings.push(`${packedKey}=${value}`);
      continue;
    }

    const optionName = OPTION_ALIASES.get(parsed.name);
    if (!optionName) {
      throw new Error(
        `Text command /${command.name} does not expose option '${parsed.name}'.`,
      );
    }
    const value = optionValue(tokens, index, parsed, parsed.name);
    if (parsed.value === null) index += 1;
    options.push({ name: optionName, value });
  }

  appendPositionals(command, options, positional);
  if (settings.length > 0) {
    const existing = options.find(({ name }) => name === "options");
    if (existing) existing.value = `${existing.value} ${settings.join(" ")}`;
    else options.push({ name: "options", value: settings.join(" ") });
  }
  const packed = options.find(({ name }) => name === "options");
  if (packed) packed.value = canonicalPackedOptions(command, packed.value);
  return options;
}

function supportsPackedKey(command, key) {
  return DISCORD_PACKED_OPTION_KEYS[command.input]?.includes(key) === true;
}

function canonicalPackedOptions(command, source) {
  const order = DISCORD_PACKED_OPTION_KEYS[command.input] ?? [];
  const settings = new Map();
  for (const token of tokenizeCommand(String(source))) {
    const equals = token.indexOf("=");
    if (equals <= 0 || equals === token.length - 1) {
      throw new Error(`/${command.name} options must use space-separated key=value entries.`);
    }
    let key = token.slice(0, equals).trim().toLowerCase().replaceAll("_", "-");
    if (command.input.startsWith("finesse-") && ["queue-knowledge", "pattern-knowledge"].includes(key)) {
      key = "knowledge";
    }
    if (!order.includes(key)) {
      throw new Error(`/${command.name} does not support options key '${key}'.`);
    }
    if (settings.has(key)) {
      throw new Error(`/${command.name} options key '${key}' may be specified only once.`);
    }
    settings.set(key, token.slice(equals + 1).trim());
  }
  return order
    .filter((key) => settings.has(key))
    .map((key) => `${key}=${settings.get(key)}`)
    .join(" ");
}

function appendPositionals(command, options, positional) {
  const order = positionalOptionOrder(command.input);
  const supplied = new Set(options.map(({ name }) => name));
  let cursor = 0;
  for (const value of positional) {
    while (cursor < order.length && supplied.has(order[cursor])) cursor += 1;
    if (cursor >= order.length) {
      throw new Error(`Text command /${command.name} received extra arguments.`);
    }
    const name = order[cursor];
    options.push({
      name,
      value: command.input === "score-fixed-next" && name === "options" && /^(?:true|false)$/i.test(value)
        ? `initial_b2b=${value.toLowerCase()}`
        : value,
    });
    supplied.add(name);
    cursor += 1;
  }
}

function positionalOptionOrder(input) {
  switch (input) {
    case "render-file":
      return ["image"];
    case "pc":
      return ["field", "next", "lines"];
    case "score-fixed-next":
      return ["field", "next", "lines", "options"];
    case "cover":
      return ["base", "target", "next"];
    case "spin-structure":
      return ["field", "pieces", "lines", "profile", "kicktable"];
    case "colored":
    case "spin":
    case "fixed-next":
      return ["field", "next"];
    case "remaining":
      return ["remaining"];
    case "verify":
      return ["scope"];
    case "finesse-search":
      return ["target", "next", "base", "kicktable", "options"];
    case "finesse-score":
      return ["document", "next", "kicktable", "options"];
    default:
      throw new Error(`Unknown text-command input contract: ${input}`);
  }
}

function optionToken(token) {
  if (typeof token !== "string" || !token.startsWith("-")) return null;
  const equals = token.indexOf("=");
  return {
    name: (equals < 0 ? token : token.slice(0, equals)).toLowerCase(),
    value: equals < 0 ? null : token.slice(equals + 1),
  };
}

function optionValue(tokens, index, parsed, name) {
  const value = parsed.value ?? tokens[index + 1];
  if (!value || (parsed.value === null && value.startsWith("-"))) {
    throw new Error(`${name} requires a value.`);
  }
  return value;
}

function normalizeCatalogName(value) {
  if (typeof value !== "string") return "";
  const normalized = value.trim().toLowerCase().replaceAll("_", "-");
  return TEXT_COMPATIBILITY_NAMES.get(normalized) ?? normalized;
}

const TEXT_COMPATIBILITY_NAMES = new Map([
  ["bestsave", "best-save"],
  ["bestsetup", "best-setup"],
  ["congruentcover", "congruent-cover"],
  ["coverpercent", "cover-percent"],
  ["dpcfinder", "dpc-finder"],
  ["pcsetup", "pc-setup"],
  ["scoreminimals", "score-minimals"],
  ["setupcover", "setup-cover"],
  ["specialcover", "special-cover"],
  ["spincover", "spin-cover"],
]);

function tokenizeTextCommand(source) {
  if (!source.includes("```")) return tokenizeCommand(source);
  if (source.length > 8192) throw new Error("The command is too long.");

  const fencedArguments = [];
  let rewritten = "";
  let cursor = 0;
  while (cursor < source.length) {
    const opening = source.indexOf("```", cursor);
    if (opening < 0) {
      rewritten += source.slice(cursor);
      break;
    }
    rewritten += source.slice(cursor, opening);
    const closing = source.indexOf("```", opening + 3);
    if (closing < 0) {
      throw new Error("The command contains an unterminated code block.");
    }

    const value = fencedArgument(source.slice(opening + 3, closing));
    let placeholder = `CLEARRA_FENCED_ARGUMENT_${fencedArguments.length}`;
    while (source.includes(placeholder)) placeholder += "_";
    fencedArguments.push({ placeholder, value });
    rewritten += placeholder;
    cursor = closing + 3;
  }

  return tokenizeCommand(rewritten).map((token) => {
    let restored = token;
    for (const { placeholder, value } of fencedArguments) {
      restored = restored.replaceAll(placeholder, value);
    }
    return restored;
  });
}

function fencedArgument(source) {
  let value = source.replaceAll("\r\n", "\n");
  if (value.startsWith("\n")) value = value.slice(1);
  if (value.endsWith("\n")) value = value.slice(0, -1);

  const firstLineEnd = value.indexOf("\n");
  const language = firstLineEnd < 0 ? value : value.slice(0, firstLineEnd);
  if (/^(?:text|txt|field)$/i.test(language.trim())) {
    value = firstLineEnd < 0 ? "" : value.slice(firstLineEnd + 1);
  }
  if (!value.trim()) throw new Error("A command code block cannot be empty.");
  return value;
}
