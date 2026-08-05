import { documentDecoder, isCtk3 } from "ctk3";
import { decoder as fumenDecoder } from "tetris-fumen";

import { tokenizeCommand } from "../clearra/command.mjs";

const FIELD_MAX_LENGTH = 6000;
const NEXT_MAX_LENGTH = 2048;
const SETTINGS_MAX_LENGTH = 256;
const VERIFY_SCOPES = new Set(["pc", "setup", "cover", "build", "kicks"]);
const SPIN_TYPES = new Set(["TSS", "TSD", "TST", "TSPIN", "T-SPIN", "ANY"]);
const SPIN_STRUCTURE_PROFILES = new Set([
  "t-spins",
  "t-spins-plus",
  "all-mini",
  "all-mini-plus",
  "all-spin",
  "all-spin-plus",
]);
const BUILTIN_KICKTABLES = new Set(["srs-plus", "srs", "srs-x", "jstris-180"]);
const FUMEN_PAYLOAD_PATTERN = /^(?:v115|[Ddm]115)@[A-Za-z0-9+/?]+$/;
const FUMEN_V110_PATTERN = /v110@[A-Za-z0-9+/?]+/i;
const CTK3_PREFIX_PATTERN = /^ctk3(?:b_|_|@)/i;
const COMPACT_GRID_PREFIX_PATTERN = /^grid:/i;
const CTK3_COLORS = new Set(["G", "I", "O", "T", "S", "Z", "J", "L"]);
const FUMEN_COLORS = new Set(["X", "GRAY", "I", "O", "T", "S", "Z", "J", "L"]);
const GRID_OCCUPIED_PATTERN = /^[+■#1XGIOTSZJL]$/i;
const GRID_EMPTY_PATTERN = /^[C~□._0]$/i;

export const DISCORD_PC_FIELD_MAX_ROWS = 6;
export const DISCORD_WIDE_FIELD_MAX_ROWS = 24;

export function buildSlashCommandArguments(command, rawOptions = []) {
  if (!command || command.kind !== "search") {
    throw new Error("This Discord command is not a Clearra search.");
  }
  const values = optionValues(rawOptions, allowedOptionNames(command.input));

  switch (command.input) {
    case "pc":
      return fieldAndNextArguments(command, values, [
        ...pcSettings(command, values),
        ...kicktableArguments(values),
      ]);
    case "cover":
      return coverArguments(command, values);
    case "colored":
      return fieldAndNextArguments(command, values, kicktableArguments(values), {
        wideField: true,
      });
    case "spin":
      return fieldAndNextArguments(command, values, [
        ...spinSettings(command, values),
        ...kicktableArguments(values),
      ], { wideField: true });
    case "fixed-next":
      return fieldAndNextArguments(command, values, kicktableArguments(values), {
        fixedNext: true,
        wideField: true,
      });
    case "score-fixed-next":
      return fieldAndNextArguments(command, values, [
        ...catFinderSettings(command, values),
        ...kicktableArguments(values),
      ], {
        fixedNext: true,
      });
    case "remaining":
      return [
        ...command.argvPrefix,
        pieceInventory(requiredText(values, "remaining", 64)),
        ...kicktableArguments(values),
      ];
    case "spin-structure":
      return spinStructureArguments(command, values);
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

export function buildSlashCommandArgumentSets(command, rawOptions = []) {
  return buildSlashCommandArgumentPlan(command, rawOptions).argumentSets;
}

export function buildSlashCommandArgumentPlan(command, rawOptions = []) {
  const arguments_ = buildSlashCommandArguments(command, rawOptions);
  if (command?.input !== "pc" || arguments_.includes("--lines")) {
    return Object.freeze({
      argumentSets: Object.freeze([Object.freeze(arguments_)]),
      automaticPcTargets: false,
    });
  }

  const values = optionValues(rawOptions, allowedOptionNames(command.input));
  const next = requiredText(values, "next", NEXT_MAX_LENGTH);
  const pieceCount = queuePatternPieceCount(next);
  const maskIndex = arguments_.indexOf("--field-mask-v1");
  const occupied = BigInt(`0x${arguments_[maskIndex + 1]}`);
  const lines = automaticPcLines({ occupied, pieceCount });
  return Object.freeze({
    argumentSets: Object.freeze(
      lines.map((lineCount) => Object.freeze([
        ...arguments_,
        "--lines",
        String(lineCount),
      ])),
    ),
    automaticPcTargets: true,
  });
}

export function readHelpArgument(rawOptions = []) {
  const values = optionValues(rawOptions, new Set(["arguments"]));
  return optionalText(values, "arguments", 64) ?? "";
}

export function normalizeSearchField(source, options = {}) {
  const name = options.name ?? "field";
  const maxBits = options.maxBits ?? 64;
  const maxRows = options.maxRows ?? (
    maxBits > 64 ? DISCORD_WIDE_FIELD_MAX_ROWS : DISCORD_PC_FIELD_MAX_ROWS
  );
  const value = requiredString(source, name, FIELD_MAX_LENGTH);
  const input = extractSlashFieldSource(value);
  switch (input.format) {
    case "grid":
      return readGridSearchField(input.source, { maxBits, maxRows, name });
    case "fumen":
      return readFumenSearchField(input.source, { maxBits, maxRows, name });
    case "ctk3":
      return readCtk3SearchField(input.source, { maxBits, maxRows, name });
    default:
      throw new Error(`${name} has an unsupported field format.`);
  }
}

// Discord's slash-command composer is a rich-text editor. A pasted multi-line
// #/_ board can therefore be interpreted as message formatting before the
// interaction is submitted. Those boards are collected in the plain-text
// Modal instead; compact `grid:row/row` values remain safe for direct slash
// options and are decoded by the same authoritative grid parser.
export function requiresDiscordFieldModal(source) {
  return typeof source === "string" && /[\r\n]/.test(source);
}

function fieldAndNextArguments(command, values, trailing = [], options = {}) {
  const wideField = options.wideField === true;
  const field = normalizeSearchField(values.get("field"), {
    maxBits: wideField ? 240 : 64,
    maxRows: wideField
      ? DISCORD_WIDE_FIELD_MAX_ROWS
      : DISCORD_PC_FIELD_MAX_ROWS,
  });
  const next = validatedNext(values, options.fixedNext);
  return [
    ...command.argvPrefix,
    wideField ? "--board-mask-v1" : "--field-mask-v1",
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
    maxRows: DISCORD_WIDE_FIELD_MAX_ROWS,
  });
  const target = normalizeSearchField(values.get("target"), {
    name: "target",
    maxBits: 240,
    maxRows: DISCORD_WIDE_FIELD_MAX_ROWS,
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
    ...kicktableArguments(values),
  ];
}

function spinStructureArguments(command, values) {
  const pieces = pieceInventory(
    requiredText(values, "pieces", 64),
    "pieces",
  );
  const field = normalizeSearchField(values.get("field"), {
    maxBits: 240,
    maxRows: DISCORD_WIDE_FIELD_MAX_ROWS,
  });
  const lines = (optionalText(values, "lines", 8) ?? "1+").toLowerCase();
  if (!/^(?:any|[0-4]|[1-4]\+)$/.test(lines)) {
    throw new Error("lines must be any, 0 through 4, or 1+ through 4+.");
  }
  const profile = (optionalText(values, "profile", 32) ?? "t-spins")
    .toLowerCase()
    .replaceAll("_", "-");
  if (!SPIN_STRUCTURE_PROFILES.has(profile)) {
    throw new Error("profile must be T-Spins, T-Spins+, All-Mini(+), or All-Spin(+).");
  }
  return [
    ...command.argvPrefix,
    "--board-mask-v1",
    field.mask,
    "--pieces",
    pieces,
    "--lines",
    lines,
    "--spin-profile",
    profile,
    ...kicktableArguments(values),
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
  return fixed ? next.toUpperCase() : next;
}

function extractSlashFieldSource(value) {
  if (/^https?:\/\//i.test(value)) return extractSourceFromUrl(value);
  const source = String(value).trim();
  if (CTK3_PREFIX_PATTERN.test(source) || /^(?:v11[05]|[Ddm]115)@/i.test(source)) {
    return classifyDocumentSource(source);
  }
  if (COMPACT_GRID_PREFIX_PATTERN.test(source)) {
    return Object.freeze({
      format: "grid",
      source: source.slice(source.indexOf(":") + 1).split("/").join("\n"),
    });
  }
  return Object.freeze({ format: "grid", source });
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
    const page = reader.readPage(0);
    if (page.height > options.maxRows) {
      throw new Error(`${options.name} CTK3 exceeds the ${options.maxRows}-row limit.`);
    }
    const occupied = ctk3PageOccupiedMask(page, 0, options.maxBits);
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

function readGridSearchField(source, options) {
  const rows = source.replaceAll("\r\n", "\n").split("\n");
  if (rows.length < 1 || rows.length > options.maxRows) {
    throw new Error(
      `${options.name} grid must contain from one through ${rowLimitName(options.maxRows)} rows.`,
    );
  }
  if (rows.some((row) => row.length !== 10)) {
    throw new Error(`${options.name} grid rows must be exactly 10 columns wide.`);
  }

  let occupied = 0n;
  for (let displayY = 0; displayY < rows.length; displayY += 1) {
    const boardY = rows.length - displayY - 1;
    for (let x = 0; x < 10; x += 1) {
      const cell = rows[displayY][x];
      if (GRID_EMPTY_PATTERN.test(cell)) continue;
      if (!GRID_OCCUPIED_PATTERN.test(cell)) {
        throw new Error(
          `${options.name} grid contains '${cell}' at column ${x + 1}; use # for filled cells and _ for empty cells.`,
        );
      }
      const bitIndex = boardY * 10 + x;
      if (bitIndex >= options.maxBits) {
        throw new Error(
          `${options.name} grid has a cell outside Clearra's ${options.maxBits}-bit field range at (${x}, ${boardY}).`,
        );
      }
      occupied |= 1n << BigInt(bitIndex);
    }
  }
  return Object.freeze({
    format: "occupied-field",
    sourceFormat: "grid",
    occupied,
    mask: hexMask(occupied, options.maxBits),
    height: occupiedHeight(occupied),
  });
}

function rowLimitName(rows) {
  if (rows === 6) return "six";
  if (rows === 24) return "twenty-four";
  return String(rows);
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

export function automaticPcLines({ occupied, pieceCount }) {
  if (typeof occupied !== "bigint" || occupied < 0n) {
    throw new Error("Clearra received an invalid PC field mask.");
  }
  const height = occupiedHeight(occupied);
  if (
    !Number.isSafeInteger(height) ||
    height < 0 ||
    height > DISCORD_PC_FIELD_MAX_ROWS
  ) {
    throw new Error("Automatic PC search supports fields up to six rows high.");
  }
  if (!Number.isSafeInteger(pieceCount) || pieceCount < 1) {
    throw new Error("Automatic PC search requires a finite next-pattern length.");
  }
  const lines = Array.from(
    { length: DISCORD_PC_FIELD_MAX_ROWS },
    (_, index) => index + 1,
  ).filter((lineCount) => {
    if (lineCount < height) return false;
    const target = (1n << BigInt(lineCount * 10)) - 1n;
    if ((occupied & ~target) !== 0n) return false;
    const missingCellCount = popcount(target & ~occupied);
    return missingCellCount > 0 &&
      missingCellCount % 4 === 0 &&
      missingCellCount / 4 <= pieceCount;
  });
  if (lines.length === 0) {
    throw new Error(
      "The field and next length produce no valid automatic PC target from one through six lines.",
    );
  }
  return Object.freeze(lines);
}

export function queuePatternPieceCount(source) {
  const normalized = normalizeSfinderPatternForLength(
    requiredString(source, "next", NEXT_MAX_LENGTH),
  );
  const lengths = normalized.split(";").map(patternAlternativeLength);
  if (lengths.some((length) => length !== lengths[0])) {
    throw new Error("next pattern alternatives must have the same piece count.");
  }
  return lengths[0];
}

function normalizeSfinderPatternForLength(source) {
  const characters = [...source];
  let output = "";
  for (let index = 0; index < characters.length; index += 1) {
    const character = characters[index];
    if (/\s/.test(character) || character === ",") continue;
    if (character === "*") {
      if (characters[index + 1] === "!") {
        output += "P7";
        index += 1;
        continue;
      }
      if (/^[pP]$/.test(characters[index + 1] ?? "")) {
        output += "P";
        index += 1;
        continue;
      }
      throw new Error("next '*' must be followed immediately by ! or pN.");
    }
    if (/^[pP]$/.test(character) && index > 0 && characters[index - 1] === "]") {
      continue;
    }
    output += asciiUppercase(character);
  }
  return output;
}

function asciiUppercase(character) {
  const code = character.codePointAt(0);
  return code >= 0x61 && code <= 0x7a
    ? String.fromCodePoint(code - 0x20)
    : character;
}

function patternAlternativeLength(source) {
  if (!source) throw new Error("next pattern contains an empty alternative.");
  let count = 0;
  let cursor = 0;
  while (cursor < source.length) {
    const character = source[cursor];
    if (/[IOTSZJL]/.test(character)) {
      count += 1;
      cursor += 1;
      continue;
    }
    if (character === "*") {
      if (source[cursor + 1] === "!") {
        count += 7;
        cursor += 2;
        continue;
      }
      if (source[cursor + 1] !== "P") {
        throw new Error("next '*' must be followed by ! or pN.");
      }
      const parsed = readPatternCount(source, cursor + 2);
      if (parsed.value > 7) {
        throw new Error("next standard-bag draws may not exceed seven pieces per group.");
      }
      count += parsed.value;
      cursor = parsed.cursor;
      continue;
    }
    if (character === "P") {
      const parsed = readPatternCount(source, cursor + 1);
      if (parsed.value > 7) {
        throw new Error("next standard-bag draws may not exceed seven pieces per group.");
      }
      count += parsed.value;
      cursor = parsed.cursor;
      continue;
    }
    if (character === "[") {
      const close = source.indexOf("]", cursor + 1);
      if (close < 0) throw new Error("next pattern has an unterminated piece group.");
      const group = source.slice(cursor + 1, close);
      const complement = group.startsWith("^");
      const pieces = complement ? group.slice(1) : group;
      if (!pieces || !/^[IOTSZJL]+$/.test(pieces)) {
        throw new Error("next pattern contains an invalid piece group.");
      }
      const unique = new Set(pieces);
      const groupSize = complement ? 7 - unique.size : unique.size;
      if (groupSize < 1) {
        throw new Error("next pattern piece group must leave at least one choice.");
      }
      cursor = close + 1;
      if (source[cursor] === "!") {
        count += groupSize;
        cursor += 1;
        continue;
      }
      if (source[cursor] === "P") {
        throw new Error("next pattern has an unexpected bag token after a piece group.");
      }
      if (!/\d/.test(source[cursor] ?? "")) {
        count += 1;
        continue;
      }
      const parsed = readPatternCount(source, cursor);
      if (parsed.value > groupSize) {
        throw new Error("next pattern draws more pieces than its group contains.");
      }
      count += parsed.value;
      cursor = parsed.cursor;
      continue;
    }
    throw new Error(`next pattern contains unsupported token '${character}'.`);
  }
  if (count < 1) throw new Error("next pattern must contain at least one piece.");
  return count;
}

function readPatternCount(source, cursor) {
  const start = cursor;
  while (cursor < source.length && /\d/.test(source[cursor])) cursor += 1;
  if (cursor === start) throw new Error("next pattern draw count is missing.");
  const value = Number(source.slice(start, cursor));
  if (!Number.isSafeInteger(value) || value < 1) {
    throw new Error("next pattern draw count must be a positive integer.");
  }
  return { value, cursor };
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
  const selectedLines = optionalInteger(
    values,
    "lines",
    1,
    DISCORD_PC_FIELD_MAX_ROWS,
  );
  if (selectedLines !== null && settings.has("clear")) {
    throw new Error("lines and legacy options clear/lines may not be specified together.");
  }
  if (selectedLines !== null) {
    output.push("--lines", String(selectedLines));
  } else if (settings.has("clear")) {
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

function catFinderSettings(command, values) {
  const settings = parseSettings(command, values, new Map([
    ["initial_b2b", "initial_b2b"],
    ["initial-b2b", "initial_b2b"],
    ["b2b", "initial_b2b"],
  ]));
  const output = [];
  const selectedLines = optionalInteger(values, "lines", 1, 6);
  if (selectedLines !== null) {
    output.push("--lines", String(selectedLines));
  }
  if (settings.has("initial_b2b")) {
    output.push(
      "--initial-b2b",
      booleanSetting(settings.get("initial_b2b"), "initial_b2b"),
    );
  }
  return output;
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

function pieceInventory(value, name = "remaining") {
  if (!/^[IOTSZJL]+$/i.test(value)) {
    throw new Error(`${name} must contain only IOTSZJL pieces.`);
  }
  return value.toUpperCase();
}

function allowedOptionNames(input) {
  switch (input) {
    case "pc":
      return new Set(["field", "next", "lines", "options", "kicktable"]);
    case "spin":
      return new Set(["field", "next", "options", "kicktable"]);
    case "cover":
      return new Set(["base", "target", "next", "options", "kicktable"]);
    case "colored":
    case "fixed-next":
      return new Set(["field", "next", "kicktable"]);
    case "score-fixed-next":
      return new Set(["field", "next", "lines", "options", "kicktable"]);
    case "remaining":
      return new Set(["remaining", "kicktable"]);
    case "spin-structure":
      return new Set(["pieces", "field", "lines", "profile", "kicktable"]);
    case "verify":
      return new Set(["scope"]);
    default:
      throw new Error(`Unknown slash-command input contract: ${input}`);
  }
}

function kicktableArguments(values) {
  const selected = optionalText(values, "kicktable", 32);
  if (!selected) return [];
  const normalized = selected.toLowerCase();
  if (!BUILTIN_KICKTABLES.has(normalized)) {
    throw new Error(
      "kicktable must be srs-plus, srs, srs-x, or jstris-180; custom kick tables are unavailable.",
    );
  }
  return ["--rule", normalized];
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

function optionalInteger(values, name, minimum, maximum) {
  if (!values.has(name)) return null;
  const raw = values.get(name);
  const value = typeof raw === "string" && /^\d+$/.test(raw.trim())
    ? Number(raw.trim())
    : raw;
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw new Error(`${name} must be an integer from ${minimum} through ${maximum}.`);
  }
  return value;
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
