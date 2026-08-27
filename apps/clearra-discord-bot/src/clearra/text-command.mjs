import {
  prepareClearraArguments,
  tokenizeCommand,
} from "./command.mjs";
import {
  findShadowedTextCommand,
  findTextCommand,
} from "../discord/slash-command-catalog.mjs";
import { hiddenTextSearchCapabilities } from "../discord/capability-registry.mjs";
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
  ["--base-mask", "base-mask"],
  ["--target-mask", "target-mask"],
  ["--target-format", "target-format"],
  ["--target-document", "target-document"],
  ["--solution-format", "solution-format"],
  ["--solution-document", "solution-document"],
  ["--document-format", "document-format"],
  ["--setup-queue", "setup-queue"],
  ["--setup-patterns", "setup-patterns"],
  ["--solution-queue", "solution-queue"],
  ["--solution-patterns", "solution-patterns"],
  ["--objective", "objective"],
  ["--next", "next"],
  ["--patterns", "next"],
  ["--pattern", "next"],
  ["--queue", "next"],
  ["-p", "next"],
  ["--lines", "lines"],
  ["--clear", "lines"],
  ["--clear-height", "clear"],
  ["-c", "lines"],
  ["--kicktable", "kicktable"],
  ["--rule", "kicktable"],
  ["--height", "height"],
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
  ["--spin-profile", "spin-profile"],
  ["--score-profile", "score-profile"],
  ["--spin-category", "spin-category"],
  ["--damage-mode", "damage-mode"],
  ["--minimum-damage", "minimum-damage"],
  ["--failed-count", "failed-count"],
  ["--max-patterns", "max-patterns"],
  ["--final-piece", "final-piece"],
  ["--dependency-report", "dependency-report"],
  ["--max-nodes", "max-nodes"],
  ["--max-frontier-states", "max-frontier-states"],
  ["--max-candidates", "max-candidates"],
  ["--max-memory-mib", "max-memory-mib"],
  ["--initial-combo", "initial-combo"],
  ["--initial-b2b", "initial-b2b"],
  ["--fill-bottom", "fill-bottom"],
  ["--fill-top", "fill-top"],
  ["--max-placements", "max-placements"],
  ["--minimality", "minimality"],
  ["--mode", "mode"],
  ["--qb", "qb"],
  ["--post-cycle-borrow", "post-cycle-borrow"],
  ["--source-pieces", "source-pieces"],
  ["--aggregate", "aggregation"],
  ["--aggregation", "aggregation"],
  ["--scope", "scope"],
  ["--image", "image"],
  ["--document", "document"],
  ["--finesse", "finesse"],
  ["--finesse-knowledge", "finesse-knowledge"],
  ["--mirror", "mirror"],
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
  ["--solution-probabilities", ["solution-probabilities", "on"]],
  ["--no-solution-probabilities", ["solution-probabilities", "off"]],
  ["--dependency-report", ["dependency-report", "on"]],
  ["--no-dependency-report", ["dependency-report", "off"]],
]);

// Objective selection is an advanced text-only projection over pc.path. Every
// accepted objective resolves to its own canonical typed calculation; the
// pc.path parser itself never receives an explicit objective override.
const ADVANCED_PC_OBJECTIVES = new Map([
  ["all", Object.freeze({ subcommand: "path", capabilityId: "pc.path" })],
  ["unique", Object.freeze({ subcommand: "chance", capabilityId: "pc.chance" })],
  ["min-cover", Object.freeze({ subcommand: "minimals", capabilityId: "pc.minimals" })],
  ["tiling", Object.freeze({ subcommand: "tiling", capabilityId: "pc.tiling" })],
]);
const ADVANCED_PC_OBJECTIVE_ALIASES = new Map([
  ["minimum-cover", "min-cover"],
]);

// Diagnostics deliberately live outside the slash/help catalog. This private
// projection supplies only the fields required by text parsing and lowering;
// it cannot be reached through an explicit `sfinder` namespace or Discord
// application-command registration.
const HIDDEN_TEXT_COMMANDS = new Map(
  hiddenTextSearchCapabilities().map((capability) => {
    const command = Object.freeze({
      name: capability.canonical.root,
      rootName: capability.canonical.root,
      subcommand: null,
      kind: "search",
      group: capability.problemFamily,
      input: capability.engine.input,
      capabilityId: capability.id,
      timeoutClass: capability.timeoutClass,
      publicResultKind: capability.publicResultKind,
      argvPrefix: Object.freeze([...capability.engine.argvPrefix]),
      registration: Object.freeze({
        name: capability.canonical.root,
        options: Object.freeze([
          Object.freeze({ name: "scope" }),
        ]),
      }),
    });
    return [command.name, command];
  }),
);

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
  if (command?.capabilityId?.startsWith("build.evaluate.")) {
    return command.capabilityId;
  }
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
  if (!allowsExactHiddenVerifySpelling(body, trimmed, prefix)) return null;

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
  const normalizedName = normalizeCatalogName(commandName);
  const candidate = textCommandCandidate(normalizedName, explicitSfinder);
  if (!allowsExactHiddenTextCommand(candidate, trimmed, prefix)) return null;
  const root = explicitSfinder && candidate?.argvPrefix?.[0] !== "sfinder"
    ? null
    : candidate;
  const variant = resolveCanonicalTextVariant(root, tokens, namespaceEnd + 1, false);
  const command = variant.command;
  return {
    tokens,
    first,
    explicitSfinder,
    command,
    argumentStart: namespaceEnd + 1 + variant.width,
  };
}

function resolveTextCommand(content, prefix) {
  if (typeof prefix !== "string" || prefix.length === 0) return null;
  const trimmed = String(content ?? "").trim();
  if (!trimmed.startsWith(prefix)) return null;
  const body = trimmed.slice(prefix.length).trim();
  if (!body) return null;
  if (!allowsExactHiddenVerifySpelling(body, trimmed, prefix)) return null;

  const tokens = tokenizeTextCommand(body);
  if (usesRetiredCatFinderName(tokens)) return null;
  const first = tokens[0]?.toLowerCase();
  if (first === "clearra") return null;
  let namespaceEnd = 0;
  const explicitSfinder = tokens[namespaceEnd]?.toLowerCase() === "sfinder";
  if (explicitSfinder) namespaceEnd += 1;
  const commandName = tokens[namespaceEnd];
  const normalizedName = normalizeCatalogName(commandName);
  const candidate = textCommandCandidate(normalizedName, explicitSfinder);
  if (!allowsExactHiddenTextCommand(candidate, trimmed, prefix)) return null;
  const root = explicitSfinder && candidate?.argvPrefix?.[0] !== "sfinder"
    ? null
    : candidate;
  const baseArgumentStart = namespaceEnd + 1;
  const variant = resolveCanonicalTextVariant(root, tokens, baseArgumentStart, true);
  const baseCommand = variant.command;
  const argumentStart = baseArgumentStart + variant.width;
  const objectiveResolution = resolveAdvancedPcObjective(
    baseCommand,
    tokens,
    argumentStart,
    explicitSfinder,
  );
  return {
    tokens: objectiveResolution.tokens,
    first,
    explicitSfinder,
    command: objectiveResolution.command,
    argumentStart,
  };
}

function resolveAdvancedPcObjective(command, tokens, argumentStart, explicitSfinder) {
  // Preserve the ingress boundary for unknown command names. An otherwise
  // unrecognised prefix must not become executable merely because its tail
  // happens to contain the advanced option spelling.
  if (!command) return { command, tokens };
  if (command.input?.startsWith("build-v2-")) return { command, tokens };

  let objectiveId = null;
  const forwarded = tokens.slice(0, argumentStart);

  for (let index = argumentStart; index < tokens.length; index += 1) {
    const parsed = optionToken(tokens[index]);
    if (parsed?.name !== "--objective") {
      forwarded.push(tokens[index]);
      continue;
    }
    if (objectiveId !== null) {
      throw new Error("Text option --objective may be specified only once.");
    }

    let supplied = parsed.value;
    if (supplied === null) {
      const following = tokens[index + 1];
      if (following === undefined || optionToken(following)) {
        throw new Error("Text option --objective requires one registered objective ID.");
      }
      supplied = following;
      index += 1;
    }
    if (typeof supplied !== "string" || supplied.length === 0) {
      throw new Error("Text option --objective requires one registered objective ID.");
    }
    objectiveId = ADVANCED_PC_OBJECTIVE_ALIASES.get(supplied) ?? supplied;
  }

  if (objectiveId === null) return { command, tokens };
  if (explicitSfinder) {
    throw new Error(
      "Text option --objective is unavailable under the explicit sfinder namespace.",
    );
  }
  if (command?.capabilityId !== "pc.path") {
    throw new Error(
      "Text option --objective is available only on the pc.path base capability.",
    );
  }
  const registered = ADVANCED_PC_OBJECTIVES.get(objectiveId);
  if (!registered) {
    throw new Error(`Unknown registered PC objective '${objectiveId}'.`);
  }
  const canonical = findTextCommand("pc")?.subcommands?.[registered.subcommand] ?? null;
  if (canonical?.capabilityId !== registered.capabilityId) {
    throw new Error(`PC objective '${objectiveId}' has no canonical command variant.`);
  }
  return {
    command: registered.pcObjective
      ? Object.freeze({ ...canonical, pcObjective: registered.pcObjective })
      : canonical,
    tokens: forwarded,
  };
}

function textCommandCandidate(normalizedName, explicitSfinder) {
  if (explicitSfinder) {
    return findShadowedTextCommand(normalizedName) ?? findTextCommand(normalizedName);
  }
  return HIDDEN_TEXT_COMMANDS.get(normalizedName) ?? findTextCommand(normalizedName);
}

function allowsExactHiddenTextCommand(command, trimmed, prefix) {
  if (command?.capabilityId !== "diagnostic.verify") return true;
  return ["$", ">"].includes(prefix) && trimmed === `${prefix}verify`;
}

function allowsExactHiddenVerifySpelling(body, trimmed, prefix) {
  const [first, second] = body.split(/\s+/u, 3);
  const directVerify = first?.toLowerCase() === "verify";
  const explicitSfinderVerify = first?.toLowerCase() === "sfinder" &&
    second?.toLowerCase() === "verify";
  if (!directVerify && !explicitSfinderVerify) return true;
  return !explicitSfinderVerify &&
    ["$", ">"].includes(prefix) &&
    trimmed === `${prefix}verify`;
}

function resolveGroupedTextVariant(command, tokens, subcommandIndex, strict) {
  if (!command?.subcommands) return command;
  const name = normalizeCatalogName(tokens[subcommandIndex]);
  const variant = command.subcommands?.[name] ?? null;
  if (variant) return variant;
  const shadowed = findShadowedTextCommand(command.name);
  if (shadowed) return shadowed;
  if (strict) {
    throw new Error(
      `Text command /${command.name} requires one of: ${Object.keys(command.subcommands).join(", ")}.`,
    );
  }
  return null;
}

function resolveCanonicalTextVariant(command, tokens, subcommandIndex, strict) {
  if (
    command?.name === "build" &&
    normalizeCatalogName(tokens[subcommandIndex]) === "evaluate"
  ) {
    const evaluation = normalizeCatalogName(tokens[subcommandIndex + 1]);
    const variant = command.subcommands?.[`evaluate-${evaluation}`] ?? null;
    if (variant) return { command: variant, width: 2 };
    if (strict) {
      throw new Error(
        "Text command /build evaluate requires cover, minimals, score, b2b-cover, or cover-percent.",
      );
    }
    return { command: null, width: 0 };
  }
  const variant = resolveGroupedTextVariant(command, tokens, subcommandIndex, strict);
  return {
    command: variant,
    width: variant?.subcommand ? 1 : 0,
  };
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
      if (["pc.score", "pc.score-minimals"].includes(command.capabilityId)) {
        throw new Error(
          `Text command /${command.name} does not expose execution option '${parsed.name}'.`,
        );
      }
      if (controlledWidth === 1 && parsed.value === null) index += 1;
      continue;
    }
    if (parsed.name === "--no-hold") {
      if (supportsNamedOption(command, "hold")) {
        const value = command.input === "pc-v2"
          ? "avoid"
          : command.input === "setup-score-v1"
            ? "off"
          : [
              "forward-spin-v2",
              "forward-damage-v2",
              "forward-ren-v1",
              "pc-allspin-exact-v1",
              "pc-allspin-pattern-v1",
            ].includes(command.input)
            ? "off"
            : "disabled";
        options.push({ name: "hold", value });
      } else {
        settings.push("hold=avoid");
      }
      continue;
    }
    const booleanPacked = PACKED_BOOLEAN_FLAGS.get(parsed.name);
    if (booleanPacked) {
      if (supportsNamedOption(command, booleanPacked[0])) {
        options.push({ name: booleanPacked[0], value: booleanPacked[1] });
        continue;
      }
      if (supportsPackedKey(command, booleanPacked[0])) {
        settings.push(`${booleanPacked[0]}=${booleanPacked[1]}`);
        continue;
      }
    }
    if (["--allow-post-cycle-borrow", "--no-post-cycle-borrow"].includes(parsed.name)) {
      const value = parsed.name === "--allow-post-cycle-borrow" ? "on" : "off";
      if (!supportsNamedOption(command, "post-cycle-borrow")) {
        throw new Error(`Text command /${command.name} does not expose option '${parsed.name}'.`);
      }
      options.push({ name: "post-cycle-borrow", value });
      continue;
    }
    if (parsed.name === "--hold" || parsed.name === "-h") {
      if (
        command.input === "setup-score-v1" &&
        parsed.value === null &&
        (tokens[index + 1] === undefined || tokens[index + 1].startsWith("-"))
      ) {
        options.push({ name: "hold", value: "on" });
        continue;
      }
      const value = optionValue(tokens, index, parsed, "--hold");
      if (parsed.value === null) index += 1;
      if (supportsNamedOption(command, "hold")) {
        options.push({ name: "hold", value });
      } else {
        settings.push(`hold=${value}`);
      }
      continue;
    }
    if (parsed.name === "--type") {
      const value = optionValue(tokens, index, parsed, "--type");
      if (parsed.value === null) index += 1;
      settings.push(`type=${value}`);
      continue;
    }
    if (
      command.input === "pc-allspin-exact-v1" &&
      ["--patterns", "--pattern"].includes(parsed.name)
    ) {
      throw new Error(
        `Text command /${command.name} requires --queue or --next, not '${parsed.name}'.`,
      );
    }
    if (
      command.input === "pc-allspin-pattern-v1" &&
      parsed.name === "--queue"
    ) {
      throw new Error(
        `Text command /${command.name} requires --patterns, --pattern, or --next, not '--queue'.`,
      );
    }
    if (
      command.input?.startsWith("build-v2-") &&
      ["--queue", "--patterns", "--pattern"].includes(parsed.name)
    ) {
      const optionName = parsed.name === "--queue" ? "queue" : "patterns";
      if (!supportsNamedOption(command, optionName)) {
        throw new Error(`Text command /${command.name} does not expose option '${parsed.name}'.`);
      }
      const value = optionValue(tokens, index, parsed, parsed.name);
      if (parsed.value === null) index += 1;
      options.push({ name: optionName, value });
      continue;
    }
    if (parsed.name === "--knowledge" || parsed.name === "--queue-knowledge" || parsed.name === "--pattern-knowledge") {
      const value = optionValue(tokens, index, parsed, parsed.name);
      if (parsed.value === null) index += 1;
      if (supportsNamedOption(command, "queue-knowledge")) {
        options.push({ name: "queue-knowledge", value });
      } else if (supportsNamedOption(command, "finesse-knowledge")) {
        options.push({ name: "finesse-knowledge", value });
      } else if (supportsNamedOption(command, "knowledge")) {
        options.push({ name: "knowledge", value });
      } else {
        settings.push(`knowledge=${value}`);
      }
      continue;
    }
    const packedKey = PACKED_TEXT_OPTIONS.get(parsed.name);
    if (packedKey) {
      const value = optionValue(tokens, index, parsed, parsed.name);
      if (parsed.value === null) index += 1;
      if (supportsNamedOption(command, packedKey)) {
        options.push({ name: packedKey, value });
        continue;
      }
      if (supportsPackedKey(command, packedKey)) {
        settings.push(`${packedKey}=${value}`);
        continue;
      }
      throw new Error(`Text command /${command.name} does not expose option '${parsed.name}'.`);
    }

    let optionName = OPTION_ALIASES.get(parsed.name);
    if (optionName === "lines" && command.input === "setup-score-v1") {
      optionName = "clear";
    }
    if (optionName === "profile" && supportsNamedOption(command, "spin-profile")) {
      optionName = "spin-profile";
    } else if (optionName === "spin-profile" && supportsNamedOption(command, "profile")) {
      optionName = "profile";
    }
    if (
      !optionName ||
      (!supportsNamedOption(command, optionName) &&
        !supportsCompatibilityPresetOption(command, optionName))
    ) {
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

function supportsNamedOption(command, name) {
  return command?.registration?.options?.some((option) => option.name === name) === true;
}

function supportsCompatibilityPresetOption(command, name) {
  const field = ({
    finesse: "finesse",
    mirror: "mirror",
    "score-profile": "scoreProfile",
    priority: "setupPriority",
  })[name];
  return Boolean(field && Object.hasOwn(command?.compatibilityPreset ?? {}, field));
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
    case "pc-v2":
    case "pc-path-v2":
    case "pc-chance-v2":
      return ["field", "next", "lines", "hold", "kicktable"];
    case "pc-allspin-exact-v1":
    case "pc-allspin-pattern-v1":
      return ["field", "next", "lines", "spin-profile", "kicktable"];
    case "pc-score-v2":
      return ["field", "next", "lines", "score-profile", "kicktable"];
    case "pc-tiling-v2":
      return ["field", "next", "lines", "hold"];
    case "pc-failed-v2":
      return ["field", "next", "lines", "hold", "kicktable"];
    case "score-fixed-next":
      return ["field", "next", "lines", "options"];
    case "score-fixed-next-v2":
      return ["field", "next", "lines", "initial-b2b", "kicktable"];
    case "pc-score-finder-v2":
      return ["field", "next", "lines", "hold", "kicktable"];
    case "cover":
      return ["base", "target", "next"];
    case "build-cover":
      return ["base", "target", "next", "kicktable"];
    case "build-v2-cover":
    case "build-v2-target":
    case "build-v2-supplied":
      return [];
    case "spin-structure":
      return ["field", "pieces", "lines", "profile", "kicktable"];
    case "spin-structure-v2":
    case "spin-structure-cover-v1":
    case "spin-structure-guaranteed-v1":
      return ["field", "pieces", "lines", "spin-profile", "kicktable"];
    case "colored":
    case "spin":
    case "fixed-next":
      return ["field", "next"];
    case "remaining":
      return ["remaining"];
    case "setup-v2":
      return ["remaining"];
    case "setup-score-v1":
      return [];
    case "verify":
      return ["scope"];
    case "finesse-search":
      return ["target", "next", "base", "kicktable", "options"];
    case "finesse-score":
      return ["document", "next", "kicktable", "options"];
    case "finesse-score-v2":
      return ["document", "next", "kicktable"];
    case "forward-spin-v2":
    case "forward-damage-v2":
    case "forward-ren-v1":
      return ["field", "next"];
    case "operation-document-v1":
    case "field-document-v1":
      return ["document"];
    case "fumen-transform-v1":
      return ["transform", "document", "page", "offset", "comments"];
    case "render-document-v1":
      return ["document", "artifact-format", "page"];
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
