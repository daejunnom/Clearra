import { documentDecoder, isCtk3 } from "ctk3";
import { decoder as fumenDecoder } from "tetris-fumen";

import { tokenizeCommand } from "../clearra/command.mjs";

const FIELD_MAX_LENGTH = 6000;
const NEXT_MAX_LENGTH = 2048;
const SETTINGS_MAX_LENGTH = 256;
const VERIFY_SCOPES = new Set(["pc", "setup", "cover", "build", "kicks"]);
const SPIN_TYPES = new Set(["TSS", "TSD", "TST", "TSPIN", "T-SPIN", "ANY"]);
const FUMEN_PAYLOAD_PATTERN = /^(?:v115|[Ddm]115)@[A-Za-z0-9+/?]+$/;
const FUMEN_V110_PATTERN = /v110@[A-Za-z0-9+/?]+/i;
const CTK3_PREFIX_PATTERN = /^ctk3(?:b_|_|@)/i;
const CTK3_COLORS = new Set(["G", "I", "O", "T", "S", "Z", "J", "L"]);
const FUMEN_COLORS = new Set(["X", "GRAY", "I", "O", "T", "S", "Z", "J", "L"]);

export function buildSlashCommandArguments(command, rawOptions = []) {
  if (!command || command.kind !== "search") {
    throw new Error("This Discord command is not a Clearra search.");
  }
  const values = optionValues(rawOptions, allowedOptionNames(command.input));

  switch (command.input) {
    case "pc":
      return fieldAndNextArguments(command, values, pcSettings(command, values));
    case "cover":
      return coverArguments(command, values);
    case "colored":
      return fieldAndNextArguments(command, values);
    case "spin":
      return fieldAndNextArguments(command, values, spinSettings(command, values));
    case "fixed-next":
      return fieldAndNextArguments(command, values, [], { fixedNext: true });
    case "remaining":
      return [
        ...command.argvPrefix,
        pieceInventory(requiredText(values, "remaining", 64)),
      ];
    case "verify": {
      const scope = optionalText(values, "scope", 16)?.toLowerCase();
      if (scope && !VERIFY_SCOPES.has(scope)) {
        throw new Error("Verify scope must be pc, setup, cover, build, or kicks.");
      }
      return scope ? [...command.argvPrefix, scope] : [...command.argvPrefix];
    }
    default:
      throw new Error(`Unknown slash-command input contract: ${command.input}`);
  }
}

export function readHelpArgument(rawOptions = []) {
  const values = optionValues(rawOptions, new Set(["arguments"]));
  return optionalText(values, "arguments", 64) ?? "";
}

export function normalizeSearchField(source, options = {}) {
  const name = options.name ?? "field";
  const maxBits = options.maxBits ?? 64;
  const value = requiredString(source, name, FIELD_MAX_LENGTH);
  const input = extractSlashFieldSource(value);
  return input.format === "fumen"
    ? readFumenSearchField(input.source, { maxBits, name })
    : readCtk3SearchField(input.source, { maxBits, name });
}

function fieldAndNextArguments(command, values, trailing = [], options = {}) {
  const field = normalizeSearchField(values.get("field"));
  const next = validatedNext(values, options.fixedNext);
  return [
    ...command.argvPrefix,
    "--field-mask-v1",
    field.mask,
    options.fixedNext ? "--queue" : "--patterns",
    next,
    ...trailing,
  ];
}

function coverArguments(command, values) {
  const base = normalizeSearchField(values.get("base"), {
    name: "base",
    maxBits: 240,
  });
  const target = normalizeSearchField(values.get("target"), {
    name: "target",
    maxBits: 240,
  });
  if (target.occupied === 0n) {
    throw new Error("target must contain at least one occupied cell.");
  }
  if ((base.occupied & target.occupied) !== 0n) {
    throw new Error("base and target must not overlap; target contains only cells to add.");
  }
  if (popcount(target.occupied) % 4 !== 0) {
    throw new Error("target occupied-cell count must be divisible by four.");
  }
  if (containsCompletedRow(base.occupied, Math.max(base.height, target.height))) {
    throw new Error("base must not contain an already completed row.");
  }
  const next = validatedNext(values, false);
  return [
    ...command.argvPrefix,
    "--base-mask-v1",
    base.mask,
    "--target-mask-v1",
    target.mask,
    "--patterns",
    next,
    ...coverSettings(command, values),
  ];
}

function validatedNext(values, fixed) {
  const next = requiredText(values, "next", NEXT_MAX_LENGTH);
  if (next.startsWith("-")) {
    throw new Error("next must be a queue or pattern, not a command-line option.");
  }
  if (fixed && !/^[IOTSZJL]+$/i.test(next)) {
    throw new Error("next must be an exact queue containing only IOTSZJL pieces.");
  }
  return next;
}

function extractSlashFieldSource(value) {
  if (/^https?:\/\//i.test(value)) return extractSourceFromUrl(value);
  return classifyDocumentSource(value);
}

function extractSourceFromUrl(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error("field URL is invalid.");
  }
  const candidates = [];
  for (const key of ["document", "ctk", "fumen", "d"]) {
    for (const source of url.searchParams.getAll(key)) candidates.push(source);
  }

  const hash = url.hash.slice(1);
  const hashQuery = hash.includes("?") ? hash.slice(hash.indexOf("?") + 1) : hash;
  if (hashQuery.includes("=")) {
    const parameters = new URLSearchParams(hashQuery);
    for (const key of ["document", "ctk", "fumen", "d"]) {
      for (const source of parameters.getAll(key)) candidates.push(source);
    }
  } else if (hashQuery) {
    candidates.push(safelyDecodeURIComponent(hashQuery));
  }

  const rawQuery = url.search.slice(1);
  if (rawQuery && !rawQuery.includes("=")) {
    candidates.push(safelyDecodeURIComponent(rawQuery));
  }

  const documents = new Map();
  for (const candidate of candidates) {
    let document;
    try {
      document = classifyDocumentSource(candidate);
    } catch (error) {
      if (FUMEN_V110_PATTERN.test(candidate)) throw error;
      continue;
    }
    documents.set(`${document.format}:${document.source}`, document);
  }
  if (documents.size === 0) {
    throw new Error(
      "field URL must contain one CTK3 or v115 Fumen value in document, ctk, fumen, or d.",
    );
  }
  if (documents.size !== 1) {
    throw new Error("field URL must contain exactly one CTK3 or Fumen document.");
  }
  return documents.values().next().value;
}

function classifyDocumentSource(value) {
  const source = String(value).trim();
  if (!source) throw new Error("field document cannot be empty.");
  if (CTK3_PREFIX_PATTERN.test(source) && isCtk3(source)) {
    return Object.freeze({ format: "ctk3", source });
  }
  if (FUMEN_V110_PATTERN.test(source)) {
    throw new Error("v110 Fumen is not supported by the Clearra search decoder; use v115.");
  }
  if (FUMEN_PAYLOAD_PATTERN.test(source)) {
    return Object.freeze({ format: "fumen", source });
  }
  throw new Error("field must contain one raw CTK3 or v115 Fumen document.");
}

function readCtk3SearchField(source, options) {
  let reader;
  try {
    reader = documentDecoder.open(source, { cacheSegments: 1 });
    if (reader.width !== 10) {
      throw new Error("CTK3 search fields must be exactly 10 columns wide.");
    }
    if (reader.pageCount !== 1) {
      throw new Error(`${options.name} CTK3 must contain exactly one page.`);
    }
    const occupied = ctk3PageOccupiedMask(reader.readPage(0), 0, options.maxBits);
    return Object.freeze({
      format: "occupied-field",
      sourceFormat: "ctk3",
      occupied,
      mask: hexMask(occupied, options.maxBits),
      height: occupiedHeight(occupied),
    });
  } catch (error) {
    if (error instanceof Error) throw error;
    throw new Error("CTK3 field could not be decoded.");
  } finally {
    reader?.clearCache();
  }
}

function readFumenSearchField(source, options) {
  const normalized = /^[Ddm]115@/.test(source) ? `v${source.slice(1)}` : source;
  let pages;
  try {
    pages = fumenDecoder.decode(normalized);
  } catch {
    throw new Error(`${options.name} Fumen could not be decoded.`);
  }
  if (pages.length !== 1) {
    throw new Error(`${options.name} Fumen must contain exactly one page.`);
  }
  const page = pages[0];
  if (page.operation) {
    throw new Error(`${options.name} Fumen contains an operation; a static field is required.`);
  }
  for (let x = 0; x < 10; x += 1) {
    if (page.field.at(x, -1) !== "_") {
      throw new Error(`${options.name} Fumen has a non-empty garbage row.`);
    }
  }
  let occupied = 0n;
  for (let y = 0; y < 23; y += 1) {
    for (let x = 0; x < 10; x += 1) {
      const color = page.field.at(x, y);
      if (color === "_") continue;
      if (!FUMEN_COLORS.has(color)) {
        throw new Error(`${options.name} Fumen contains an unsupported field color.`);
      }
      const bitIndex = y * 10 + x;
      if (bitIndex >= options.maxBits) {
        throw new Error(
          `${options.name} Fumen has a cell outside Clearra's ${options.maxBits}-bit field range at (${x}, ${y}).`,
        );
      }
      occupied |= 1n << BigInt(bitIndex);
    }
  }
  return Object.freeze({
    format: "occupied-field",
    sourceFormat: "fumen",
    occupied,
    mask: hexMask(occupied, options.maxBits),
    height: occupiedHeight(occupied),
  });
}

function ctk3PageOccupiedMask(page, pageIndex, maxBits) {
  if (
    !page ||
    !Number.isSafeInteger(page.height) ||
    page.height < 0 ||
    !Array.isArray(page.cells) ||
    page.cells.length !== page.height * 10
  ) {
    throw new Error(`CTK3 page ${pageIndex + 1} has an invalid field shape.`);
  }
  if (page.operation) {
    throw new Error(`CTK3 page ${pageIndex + 1} contains an operation; a static field is required.`);
  }
  if (page.garbage?.some((cell) => cell !== null)) {
    throw new Error(`CTK3 page ${pageIndex + 1} has a non-empty garbage row, which search fields cannot represent.`);
  }

  let occupied = 0n;
  for (let y = 0; y < page.height; y += 1) {
    for (let x = 0; x < 10; x += 1) {
      const color = page.cells[y * 10 + x];
      if (color === null) continue;
      if (!CTK3_COLORS.has(color)) {
        throw new Error(`CTK3 page ${pageIndex + 1} contains an unsupported field color.`);
      }
      const bitIndex = y * 10 + x;
      if (bitIndex >= maxBits) {
        throw new Error(
          `CTK3 page ${pageIndex + 1} has a cell outside Clearra's ${maxBits}-bit field range at (${x}, ${y}).`,
        );
      }
      occupied |= 1n << BigInt(bitIndex);
    }
  }
  return occupied;
}

function hexMask(mask, maxBits) {
  return mask.toString(16).padStart(Math.ceil(maxBits / 4), "0");
}

function occupiedHeight(mask) {
  if (mask === 0n) return 0;
  let bits = 0;
  while (mask !== 0n) {
    mask >>= 1n;
    bits += 1;
  }
  return Math.ceil(bits / 10);
}

function popcount(mask) {
  let count = 0;
  while (mask !== 0n) {
    mask &= mask - 1n;
    count += 1;
  }
  return count;
}

function containsCompletedRow(mask, height) {
  const full = 0x3ffn;
  for (let y = 0; y < height; y += 1) {
    if (((mask >> BigInt(y * 10)) & full) === full) return true;
  }
  return false;
}

function safelyDecodeURIComponent(value) {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function pcSettings(command, values) {
  const settings = parseSettings(command, values, new Map([
    ["clear", "clear"],
    ["lines", "clear"],
    ["hold", "hold"],
  ]));
  const output = [];
  if (settings.has("clear")) {
    const clear = settings.get("clear");
    if (!/^[1-6]$/.test(clear)) {
      throw new Error("options clear must be an integer from 1 through 6.");
    }
    output.push("--lines", clear);
  }
  if (settings.has("hold")) {
    output.push("--hold", booleanSetting(settings.get("hold"), "hold"));
  }
  return output;
}

function coverSettings(command, values) {
  const settings = parseSettings(command, values, new Map([["hold", "hold"]]));
  if (!settings.has("hold")) return [];
  return ["--hold", booleanSetting(settings.get("hold"), "hold")];
}

function spinSettings(command, values) {
  const settings = parseSettings(command, values, new Map([["type", "type"]]));
  if (!settings.has("type")) return [];
  const type = settings.get("type").toUpperCase();
  if (!SPIN_TYPES.has(type)) {
    throw new Error("options type must be TSS, TSD, TST, TSPIN, T-SPIN, or ANY; TSM is unavailable.");
  }
  return [type];
}

function parseSettings(command, values, aliases) {
  const source = optionalText(values, "options", SETTINGS_MAX_LENGTH);
  const settings = new Map();
  if (!source) return settings;
  for (const token of tokenizeCommand(source)) {
    const equals = token.indexOf("=");
    if (equals <= 0 || equals === token.length - 1) {
      throw new Error(
        `/${command.name} options must use space-separated key=value entries.`,
      );
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

function booleanSetting(value, name) {
  switch (value.toLowerCase()) {
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
      throw new Error(`options ${name} must be use or avoid (true or false).`);
  }
}

function pieceInventory(value) {
  if (!/^[IOTSZJL]+$/i.test(value)) {
    throw new Error("remaining must contain only IOTSZJL pieces.");
  }
  return value.toUpperCase();
}

function allowedOptionNames(input) {
  switch (input) {
    case "pc":
    case "spin":
      return new Set(["field", "next", "options"]);
    case "cover":
      return new Set(["base", "target", "next", "options"]);
    case "colored":
    case "fixed-next":
      return new Set(["field", "next"]);
    case "remaining":
      return new Set(["remaining"]);
    case "verify":
      return new Set(["scope"]);
    default:
      throw new Error(`Unknown slash-command input contract: ${input}`);
  }
}

function optionValues(rawOptions, allowedNames) {
  if (!Array.isArray(rawOptions)) {
    throw new Error("Discord supplied invalid slash-command options.");
  }
  const values = new Map();
  for (const option of rawOptions) {
    const name = typeof option?.name === "string" ? option.name : "";
    if (!allowedNames.has(name)) {
      throw new Error(`Discord supplied unsupported option '${name || "unknown"}'.`);
    }
    if (values.has(name)) {
      throw new Error(`Discord supplied option '${name}' more than once.`);
    }
    values.set(name, option.value);
  }
  return values;
}

function requiredText(values, name, maxLength) {
  if (!values.has(name)) throw new Error(`/${name} input is required.`);
  return requiredString(values.get(name), name, maxLength);
}

function optionalText(values, name, maxLength) {
  if (!values.has(name)) return null;
  return requiredString(values.get(name), name, maxLength);
}

function requiredString(value, name, maxLength) {
  if (typeof value !== "string") throw new Error(`${name} must be text.`);
  const normalized = value.trim();
  if (!normalized) throw new Error(`${name} cannot be empty.`);
  if (normalized.length > maxLength) {
    throw new Error(`${name} exceeds the ${maxLength}-character limit.`);
  }
  return normalized;
}
