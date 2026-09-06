import {
  decodeFumenWithinPageLimit,
  documentDecoder,
  isCtk3,
  operationCells,
} from "ctk3";
import { decoder as fumenDecoder } from "tetris-fumen";

import { decodeViewerDocument } from "../viewer/document.mjs";
import { DiscordInputError } from "./i18n.mjs";
import {
  booleanSetting,
  damagePackedArguments,
  finessePackedArguments,
  parseSettings,
  setupFinderPackedArguments,
  spinStructurePackedArguments,
} from "./slash-packed-options.mjs";

export { DISCORD_PACKED_OPTION_KEYS } from "./slash-packed-options.mjs";

// SRP rationale: this module has one behavior-level change reason: decoding every
// Discord search input surface into the canonical typed Clearra argv contract.

const FIELD_MAX_LENGTH = 6000;
const NEXT_MAX_LENGTH = 2048;
const OPERATION_DOCUMENT_MAX_SOURCE_CHARS = 2_000_000;
const OPERATION_DOCUMENT_MAX_PAGES = 4096;
const FUMEN_TRANSFORMS = new Set([
  "roundtrip",
  "combine",
  "split",
  "get-page",
  "page-shift",
  "clean-comments",
  "preserve-comments",
  "to-gray",
  "mirror",
  "text-to-fumen",
]);
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
const DAMAGE_SPIN_PROFILES = new Set([
  "disabled",
  "t-spins",
  "t-spins-plus",
  "all-mini",
  "all-mini-plus",
  "all-spin",
  "all-spin-plus",
]);
const FORWARD_SPIN_LINES = new Set([
  "any", "0", "1", "2", "3", "4", "1+", "2+", "3+", "4+",
]);
const FORWARD_SPIN_CATEGORIES = new Set(["any", "t", "other"]);
const PC_SCORE_PROFILES = new Set(["tetrio", "guideline", "jstris-ultra"]);
const BUILD_V2_SCORE_CAPABILITIES = new Set([
  "build.setup-cover-score",
  "build.evaluate.score",
]);
const BUILD_V2_OBJECTIVES = Object.freeze({
  "build.cover": Object.freeze(["min-cover", "max-probability-minimum"]),
  "build.setup": Object.freeze(["unique", "all"]),
  "build.congruent": Object.freeze(["unique", "all"]),
  "build.congruent-cover": Object.freeze(["min-cover", "max-probability-minimum"]),
  "build.setup-cover": Object.freeze(["min-cover", "max-probability-minimum"]),
  "build.setup-cover-percent": Object.freeze(["unique", "all"]),
  "build.setup-cover-score": Object.freeze(["max-score-cover"]),
  "build.evaluate.cover": Object.freeze(["all"]),
  "build.evaluate.minimals": Object.freeze(["min-cover"]),
  "build.evaluate.score": Object.freeze(["max-score-cover"]),
  "build.evaluate.b2b-cover": Object.freeze(["all"]),
  "build.evaluate.cover-percent": Object.freeze(["unique"]),
});
const SFINDER_KICKTABLES = new Set(["srs-plus", "srs", "srs-x", "jstris-180"]);
const NATIVE_KICKTABLES = new Set([...SFINDER_KICKTABLES, "no-kick"]);
const SETUP_PRIORITIES = new Set(["all", "build", "pc"]);
const SETUP_QUEUE_KNOWLEDGE = new Set(["oracle", "visible-7"]);
const SETUP_LENGTHS = new Set(["auto", "longer", "shorter"]);
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

const COMPATIBILITY_PRESET_OPTIONS = Object.freeze({
  finesse: "finesse",
  mirror: "mirror",
  scoreProfile: "score-profile",
  setupPriority: "priority",
});

export function buildSlashCommandArguments(command, rawOptions = []) {
  if (!command || command.kind !== "search") {
    throw new Error("This Discord command is not a Clearra search.");
  }
  if (command.subcommands) {
    const selected = resolveGroupedInvocation(command, rawOptions);
    return buildSlashCommandArguments(selected.command, selected.rawOptions);
  }
  rawOptions = applyCompatibilityPreset(command, rawOptions);
  const values = optionValues(rawOptions, allowedOptionNames(command));

  switch (command.input) {
    case "pc":
      return fieldAndNextArguments(command, values, [
        ...pcSettings(command, values),
        ...(command.capabilityId === "pc.path" &&
          typeof command.pcObjective === "string" &&
          command.pcObjective !== "all"
          ? ["--objective", command.pcObjective]
          : []),
        ...kicktableArguments(values),
      ]);
    case "pc-v2":
      return nativePcArguments(command, values);
    case "pc-path-v2":
      return nativePcArguments(command, values, { path: true });
    case "pc-chance-v2":
      return nativePcArguments(command, values, { chance: true });
    case "pc-score-v2":
      return nativePcArguments(command, values, { score: true });
    case "pc-save-v2":
      return nativePcArguments(command, values, { save: true });
    case "pc-tiling-v2":
      return nativePcArguments(command, values, { tiling: true });
    case "pc-failed-v2":
      return nativePcArguments(command, values, { failedQueue: true });
    case "pc-allspin-exact-v1":
      return nativePcAllspinArguments(command, values, true);
    case "pc-allspin-pattern-v1":
      return nativePcAllspinArguments(command, values, false);
    case "cover":
      return coverArguments(command, values);
    case "build-cover":
      return buildCoverArguments(command, values);
    case "build-v2-cover":
    case "build-v2-target":
    case "build-v2-supplied":
      return buildV2Arguments(command, values);
    case "colored":
      return fieldAndNextArguments(command, values, kicktableArguments(values), {
        wideField: true,
      });
    case "spin":
      return fieldAndNextArguments(command, values, [
        ...spinSettings(command, values),
        ...kicktableArguments(values),
      ], { wideField: true });
    case "forward-spin-v2":
      return forwardSearchArguments(command, values, true);
    case "fixed-next":
      return fieldAndNextArguments(command, values, [
        ...damagePackedArguments(command, values),
        ...kicktableArguments(values, true),
      ], {
        fixedNext: true,
        wideField: true,
      });
    case "forward-damage-v2":
      return forwardSearchArguments(command, values, false);
    case "forward-ren-v1":
      return forwardRenArguments(command, values);
    case "score-fixed-next":
      return fieldAndNextArguments(command, values, [
        ...catFinderSettings(command, values),
        ...kicktableArguments(values),
      ], {
        fixedNext: true,
      });
    case "score-fixed-next-v2":
      return fieldAndNextArguments(command, values, [
        ...catFinderSettings(command, values),
        ...kicktableArguments(values),
      ], {
        fixedNext: true,
      });
    case "pc-score-finder-v2":
      return nativePcScoreFinderArguments(command, values);
    case "remaining":
      return setupFinderArguments(command, values);
    case "setup-v2":
      return setupFinderArguments(command, values);
    case "setup-score-v1":
      return setupScoreArguments(command, values);
    case "spin-structure":
      return spinStructureArguments(command, values);
    case "spin-structure-v2":
      return spinStructureArguments(command, values);
    case "spin-structure-cover-v1":
    case "spin-structure-guaranteed-v1":
      return spinStructureArguments(command, values);
    case "verify": {
      const scope = optionalText(values, "scope", 16)?.toLowerCase();
      if (scope && !VERIFY_SCOPES.has(scope)) {
        throw new Error("Verify scope must be pc, setup, cover, build, or kicks.");
      }
      return scope ? [...command.argvPrefix, scope] : [...command.argvPrefix];
    }
    case "finesse-search":
      return finesseSearchArguments(command, values);
    case "finesse-score":
      return finesseScoreArguments(command, values);
    case "finesse-score-v2":
      return finesseScoreArguments(command, values);
    case "operation-document-v1":
      return operationDocumentArguments(command, values);
    case "field-document-v1":
      return fieldDocumentArguments(command, values);
    case "fumen-transform-v1":
      return fumenTransformArguments(command, values);
    case "render-document-v1":
      return renderDocumentArguments(command, values);
    default:
      throw new Error(`Unknown slash-command input contract: ${command.input}`);
  }
}

function applyCompatibilityPreset(command, rawOptions) {
  const preset = command?.compatibilityPreset;
  if (!preset) return rawOptions;
  if (!Array.isArray(rawOptions)) {
    throw new Error("Discord supplied invalid compatibility options.");
  }

  const allowed = allowedOptionNames(command);
  let lowered = [...rawOptions];
  for (const [field, expected] of Object.entries(preset)) {
    const optionName = COMPATIBILITY_PRESET_OPTIONS[field];
    if (!optionName) {
      throw new Error(`Unknown compatibility preset field '${field}'.`);
    }
    const matching = lowered.filter((option) => option?.name === optionName);
    for (const option of matching) {
      if (normalizeCompatibilityPresetValue(option.value) !==
          normalizeCompatibilityPresetValue(expected)) {
        throw new Error(
          `Compatibility command /${command.name} fixes ${optionName}=${expected}.`,
        );
      }
    }
    if (matching.length === 0 && allowed.has(optionName)) {
      lowered.push({ name: optionName, value: expected });
    } else if (!allowed.has(optionName)) {
      lowered = lowered.filter((option) => option?.name !== optionName);
    }
  }
  return lowered;
}

function normalizeCompatibilityPresetValue(value) {
  return String(value).trim().toLowerCase().replaceAll("_", "-");
}

function resolveGroupedInvocation(command, rawOptions) {
  if (!Array.isArray(rawOptions) || rawOptions.length !== 1) {
    throw new Error(`/${command.name} requires exactly one subcommand.`);
  }
  const selected = rawOptions[0];
  const variant = selected?.type === 1 && typeof selected.name === "string"
    ? command.subcommands?.[selected.name]
    : null;
  if (!variant) {
    throw new Error(`/${command.name} requires one registered subcommand.`);
  }
  if (selected.options !== undefined && !Array.isArray(selected.options)) {
    throw new Error(`Discord supplied invalid /${command.name} subcommand options.`);
  }
  return { command: variant, rawOptions: selected.options ?? [] };
}

export function buildSlashCommandArgumentSets(command, rawOptions = []) {
  return buildSlashCommandArgumentPlan(command, rawOptions).argumentSets;
}

export function buildSlashCommandArgumentPlan(command, rawOptions = []) {
  if (command?.subcommands) {
    const selected = resolveGroupedInvocation(command, rawOptions);
    return buildSlashCommandArgumentPlan(selected.command, selected.rawOptions);
  }
  if ([
    "pc-v2",
    "pc-path-v2",
    "pc-chance-v2",
    "pc-score-v2",
    "pc-save-v2",
    "pc-tiling-v2",
    "pc-failed-v2",
    "pc-allspin-exact-v1",
    "pc-allspin-pattern-v1",
    "pc-score-finder-v2",
  ].includes(command?.input)) {
    const values = optionValues(rawOptions, allowedOptionNames(command));
    if (!values.has("lines")) {
      const field = normalizeSearchField(values.get("field"));
      const next = requiredText(values, "next", NEXT_MAX_LENGTH);
      const lines = automaticPcLines({
        occupied: field.occupied,
        pieceCount: queuePatternPieceCount(next),
      });
      return Object.freeze({
        argumentSets: Object.freeze(lines.map((lineCount) => Object.freeze(
          buildSlashCommandArguments(command, [
            ...rawOptions,
            { name: "lines", value: lineCount },
          ]),
        ))),
        automaticPcTargets: true,
      });
    }
  }
  const arguments_ = buildSlashCommandArguments(command, rawOptions);
  if (command?.input !== "pc" || arguments_.includes("--lines")) {
    return Object.freeze({
      argumentSets: Object.freeze([Object.freeze(arguments_)]),
      automaticPcTargets: false,
    });
  }

  const values = optionValues(rawOptions, allowedOptionNames(command));
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

export function normalizeFinesseDocument(source) {
  try {
    return normalizeFinesseDocumentUnchecked(source);
  } catch (error) {
    if (error instanceof DiscordInputError) throw error;
    throw new DiscordInputError(
      "document.invalid",
      {},
      error instanceof Error ? error.message : "document is invalid.",
    );
  }
}

export function normalizeOperationDocument(source) {
  try {
    const value = requiredString(
      source,
      "document",
      OPERATION_DOCUMENT_MAX_SOURCE_CHARS,
    );
    const input = extractSlashFieldSource(value);
    if (input.format !== "ctk3" && input.format !== "fumen") {
      throw new Error(
        "document must be one CTK3 or v115 Fumen operation document.",
      );
    }
    const document = decodeViewerDocument(input.source, {
      maxPages: OPERATION_DOCUMENT_MAX_PAGES,
      maxSourceChars: OPERATION_DOCUMENT_MAX_SOURCE_CHARS,
    });
    if (
      document.width !== 10 ||
      !Array.isArray(document.pages) ||
      document.pages.length === 0
    ) {
      throw new Error("document must contain at least one 10-column page.");
    }
    for (let index = 0; index < document.pages.length; index += 1) {
      const page = document.pages[index];
      pageOccupiedMask(page, `document page ${index + 1}`);
      if (page.height > DISCORD_PC_FIELD_MAX_ROWS) {
        throw new Error(
          `document page ${index + 1} exceeds the ${DISCORD_PC_FIELD_MAX_ROWS}-row exact-analysis board.`,
        );
      }
      if (!page.operation) {
        throw new Error(
          `document page ${index + 1} is missing its concrete operation.`,
        );
      }
      const flags = page.flags ?? {};
      if (
        flags.lock !== true ||
        flags.mirror === true ||
        flags.rise === true ||
        flags.quiz === true
      ) {
        throw new Error(
          `document page ${index + 1} has unsupported operation-document flags.`,
        );
      }
      if (
        Array.isArray(page.garbage) &&
        page.garbage.some((cell) => cell !== null)
      ) {
        throw new Error(`document page ${index + 1} has a non-empty garbage row.`);
      }
      const placement = canonicalFinessePlacement(page.operation, index);
      if (placement.height > DISCORD_PC_FIELD_MAX_ROWS) {
        throw new Error(
          `document page ${index + 1} places a piece outside the ${DISCORD_PC_FIELD_MAX_ROWS}-row exact-analysis board.`,
        );
      }
    }
    return Object.freeze({
      source: input.source,
      sourceFormat: input.format,
      operationCount: document.pages.length,
    });
  } catch (error) {
    if (error instanceof DiscordInputError) throw error;
    throw new DiscordInputError(
      "document.invalid",
      {},
      error instanceof Error ? error.message : "document is invalid.",
    );
  }
}

export function normalizeTypedFieldDocument(source, options = {}) {
  try {
    const name = options.name ?? "document";
    const value = requiredString(
      source,
      name,
      OPERATION_DOCUMENT_MAX_SOURCE_CHARS,
    );
    if (/^https?:\/\//i.test(value)) {
      throw new Error(`${name} must be a canonical document, not a URL.`);
    }
    const input = extractSlashFieldSource(value);
    if (input.format !== "ctk3" && input.format !== "fumen") {
      throw new Error(`${name} must be one canonical CTK3 or v115 Fumen document.`);
    }
    if (options.requireFumen === true && input.format !== "fumen") {
      throw new Error(`${name} must be a canonical v115 Fumen document.`);
    }
    const document = decodeViewerDocument(input.source, {
      maxPages: OPERATION_DOCUMENT_MAX_PAGES,
      maxSourceChars: OPERATION_DOCUMENT_MAX_SOURCE_CHARS,
    });
    if (
      !Number.isSafeInteger(document.width) ||
      document.width < 1 ||
      document.width > 31 ||
      !Array.isArray(document.pages) ||
      document.pages.length === 0
    ) {
      throw new Error(`${name} must contain at least one page with a supported positive width.`);
    }
    return Object.freeze({
      source: input.source,
      sourceFormat: input.format,
      pageCount: document.pages.length,
    });
  } catch (error) {
    if (error instanceof DiscordInputError) throw error;
    throw new DiscordInputError(
      "document.invalid",
      {},
      error instanceof Error ? error.message : "document is invalid.",
    );
  }
}

export function normalizeBuildV2ColoredDocument(source, options = {}) {
  try {
    const name = options.name ?? "document";
    const expectedFormat = normalizedChoice(options.format);
    if (!new Set(["ctk3", "fumen"]).has(expectedFormat)) {
      throw new Error(`${name} format must be ctk3 or fumen.`);
    }
    const normalized = normalizeTypedFieldDocument(source, { name });
    if (normalized.sourceFormat !== expectedFormat) {
      throw new Error(
        `${name} format '${expectedFormat}' does not match its canonical ${normalized.sourceFormat} payload.`,
      );
    }
    const document = decodeViewerDocument(normalized.source, {
      maxPages: OPERATION_DOCUMENT_MAX_PAGES,
      maxSourceChars: FIELD_MAX_LENGTH,
    });
    if (document.width !== 10 || !Array.isArray(document.pages) || document.pages.length === 0) {
      throw new Error(`${name} must contain at least one 10-column colored page.`);
    }

    let initialMask = null;
    let targetMask = null;
    for (let pageIndex = 0; pageIndex < document.pages.length; pageIndex += 1) {
      const page = document.pages[pageIndex];
      if (
        !Number.isSafeInteger(page.height) ||
        page.height < 1 ||
        page.height > DISCORD_PC_FIELD_MAX_ROWS ||
        !Array.isArray(page.cells) ||
        page.cells.length !== page.height * 10
      ) {
        throw new Error(
          `${name} page ${pageIndex + 1} must be a 1–${DISCORD_PC_FIELD_MAX_ROWS}-row colored board.`,
        );
      }
      if (Array.isArray(page.garbage) && page.garbage.some((cell) => cell !== null)) {
        throw new Error(`${name} page ${pageIndex + 1} has unsupported pending garbage.`);
      }

      let pageInitial = 0n;
      let pageTarget = 0n;
      const pieceCounts = new Map([..."IOTSZJL"].map((piece) => [piece, 0]));
      for (let index = 0; index < page.cells.length; index += 1) {
        const color = page.cells[index];
        if (color === null) continue;
        const bit = 1n << BigInt(index);
        if (color === "G") {
          pageInitial |= bit;
          continue;
        }
        if (!pieceCounts.has(color)) {
          throw new Error(`${name} page ${pageIndex + 1} contains an unsupported color.`);
        }
        pageTarget |= bit;
        pieceCounts.set(color, pieceCounts.get(color) + 1);
      }
      if (pageTarget === 0n) {
        throw new Error(
          `${name} page ${pageIndex + 1} lost its piece colors; gray or occupancy-only targets are unavailable.`,
        );
      }
      if ([...pieceCounts.values()].some((count) => count % 4 !== 0)) {
        throw new Error(
          `${name} page ${pageIndex + 1} must contain complete four-cell colored tetromino areas.`,
        );
      }
      if (pageIndex === 0) {
        initialMask = pageInitial;
        targetMask = pageTarget;
      } else if (pageInitial !== initialMask || pageTarget !== targetMask) {
        throw new Error(
          `${name} pages must preserve one nominal base mask and one colored target mask.`,
        );
      }
    }

    return Object.freeze({
      ...normalized,
      initialMask,
      targetMask,
    });
  } catch (error) {
    if (error instanceof DiscordInputError) throw error;
    throw new DiscordInputError(
      "document.invalid",
      {},
      error instanceof Error ? error.message : "Build v2 document is invalid.",
    );
  }
}

function normalizeFinesseDocumentUnchecked(source) {
  const value = requiredString(source, "document", FIELD_MAX_LENGTH);
  const input = extractSlashFieldSource(value);
  if (input.format !== "ctk3" && input.format !== "fumen") {
    throw new Error("document must be one CTK3 or v115 Fumen document with placement operations.");
  }
  let document;
  try {
    document = decodeViewerDocument(input.source, {
      maxPages: 128,
      maxSourceChars: FIELD_MAX_LENGTH,
    });
  } catch {
    throw new Error("document could not be decoded as CTK3 or v115 Fumen.");
  }
  if (document.width !== 10 || !Array.isArray(document.pages) || document.pages.length === 0) {
    throw new Error("document must contain at least one 10-column page.");
  }

  const initial = document.pages[0];
  const occupied = pageOccupiedMask(initial, "document initial field");
  const placements = [];
  let height = Math.max(1, initial.height);
  for (let index = 0; index < document.pages.length; index += 1) {
    const page = document.pages[index];
    pageOccupiedMask(page, `document page ${index + 1}`);
    if (Array.isArray(page.garbage) && page.garbage.some((cell) => cell !== null)) {
      throw new Error(`document page ${index + 1} has a non-empty garbage row.`);
    }
    if (!page.operation) {
      throw new Error(`document page ${index + 1} is missing its placement operation.`);
    }
    const placement = canonicalFinessePlacement(page.operation, index);
    placements.push(placement.value);
    height = Math.max(height, page.height, placement.height);
  }
  if (height > DISCORD_WIDE_FIELD_MAX_ROWS) {
    throw new Error(`document placements exceed the ${DISCORD_WIDE_FIELD_MAX_ROWS}-row limit.`);
  }
  return Object.freeze({
    source: input.source,
    sourceFormat: input.format,
    initialMask: hexMask(occupied, 240),
    height,
    placements: Object.freeze(placements),
  });
}

function pageOccupiedMask(page, name) {
  const height = Number(page?.height);
  if (!Number.isSafeInteger(height) || height < 0 || height > DISCORD_WIDE_FIELD_MAX_ROWS) {
    throw new Error(`${name} exceeds the ${DISCORD_WIDE_FIELD_MAX_ROWS}-row limit.`);
  }
  if (!Array.isArray(page.cells) || page.cells.length !== height * 10) {
    throw new Error(`${name} has an invalid 10-column field.`);
  }
  let occupied = 0n;
  for (let index = 0; index < page.cells.length; index += 1) {
    const cell = page.cells[index];
    if (cell === null) continue;
    if (!CTK3_COLORS.has(String(cell).toUpperCase())) {
      throw new Error(`${name} contains an unsupported field color.`);
    }
    occupied |= 1n << BigInt(index);
  }
  return occupied;
}

function canonicalFinessePlacement(operation, pageIndex) {
  const piece = String(operation?.piece ?? "").toUpperCase();
  if (!/^[IOTSZJL]$/.test(piece)) {
    throw new Error(`document page ${pageIndex + 1} has an invalid operation piece.`);
  }
  const rotation = normalizeFinesseRotation(operation?.rotation);
  const x = Number(operation?.x);
  const y = Number(operation?.y);
  if (!Number.isSafeInteger(x) || !Number.isSafeInteger(y)) {
    throw new Error(`document page ${pageIndex + 1} has invalid operation coordinates.`);
  }
  let cells;
  try {
    cells = operationCells({ piece, rotation, x, y });
  } catch {
    throw new Error(`document page ${pageIndex + 1} has an invalid placement operation.`);
  }
  if (!Array.isArray(cells) || cells.length !== 4 || cells.some((cell) =>
    !Number.isSafeInteger(cell?.x) || !Number.isSafeInteger(cell?.y) ||
    cell.x < 0 || cell.x >= 10 || cell.y < 0 || cell.y >= DISCORD_WIDE_FIELD_MAX_ROWS
  )) {
    throw new Error(`document page ${pageIndex + 1} places a piece outside the supported field.`);
  }
  return Object.freeze({
    // The engine uses a lower-left normalized pose. CTK3/Fumen operation
    // anchors vary by piece and rotation, so derive the canonical pose from
    // the decoded occupied cells rather than forwarding the document anchor.
    value: `${piece}:${rotation}:${Math.min(...cells.map((cell) => cell.x))}:${Math.min(...cells.map((cell) => cell.y))}`,
    height: Math.max(...cells.map((cell) => cell.y + 1)),
  });
}

function normalizeFinesseRotation(value) {
  const normalized = String(value ?? "").trim().toLowerCase();
  const rotation = ({
    spawn: "spawn",
    north: "spawn",
    right: "right",
    east: "right",
    reverse: "reverse",
    south: "reverse",
    left: "left",
    west: "left",
  })[normalized];
  if (!rotation) throw new Error("document contains an invalid operation rotation.");
  return rotation;
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
    ...(/^[IOTSZJL]+$/i.test(next)
      ? ["--queue", next.toUpperCase()]
      : ["--patterns", next]),
    ...trailing,
  ];
}

function nativePcArguments(command, values, mode = {}) {
  const typedScoreSummary = mode.score && command.capabilityId === "pc.score";
  const typedScoreMinimals = mode.score && command.capabilityId === "pc.score-minimals";
  const typedScoreProduct = typedScoreSummary || typedScoreMinimals;
  const typedMinimumCover = command.capabilityId === "pc.minimals";
  const typedSave = mode.save === true;
  const field = normalizeSearchField(values.get("field"));
  const next = validatedNext(values, false);
  const lines = optionalInteger(values, "lines", 1, DISCORD_PC_FIELD_MAX_ROWS);
  if (lines === null) {
    throw new Error("Native PC search requires one exact lines value after automatic target planning.");
  }
  const target = (1n << BigInt(lines * 10)) - 1n;
  if ((field.occupied & ~target) !== 0n) {
    throw new Error("field has occupied cells above the requested PC target.");
  }
  const emptyCount = popcount(target & ~field.occupied);
  if (emptyCount % 4 !== 0) {
    throw new Error("the PC target does not contain a whole number of tetrominoes.");
  }
  const holdSource = optionalText(values, "hold", 16) ?? "empty";
  const hold = normalizedChoice(holdSource);
  if (!["disabled", "empty"].includes(hold) && !/^[iotszjl]$/.test(hold)) {
    throw invalidOption(
      "hold",
      "hold must be disabled, empty, or one IOTSZJL piece.",
    );
  }
  const queueKnowledge = normalizedChoice(
    optionalText(values, "queue-knowledge", 16) ?? "oracle",
  );
  if (!SETUP_QUEUE_KNOWLEDGE.has(queueKnowledge)) {
    throw invalidOption(
      "queue-knowledge",
      "queue-knowledge must be oracle or visible-7.",
    );
  }
  const objective = command.pcObjective ?? (
    mode.failedQueue || mode.chance || typedScoreProduct ? null : "all"
  );
  if (
    !mode.failedQueue && !mode.chance && !typedScoreProduct &&
    !new Set(["all", "unique", "minimum-cover", "tiling"]).has(objective)
  ) {
    throw new Error(`/${command.name} has an invalid PC objective.`);
  }
  if (queueKnowledge === "visible-7" && objective === "minimum-cover") {
    throw invalidOption(
      "queue-knowledge",
      "queue-knowledge visible-7 is unavailable with minimum-cover.",
    );
  }
  const preserveB2b = onOffValue(values, "preserve-b2b", false);
  const requestedSpinProfile = optionalText(values, "spin-profile", 32);
  const spinProfile = requestedSpinProfile === null
    ? null
    : normalizedChoice(requestedSpinProfile);
  if (spinProfile !== null && !SPIN_STRUCTURE_PROFILES.has(spinProfile)) {
    throw invalidOption(
      "spin-profile",
      "spin-profile must be T-Spins(+), All-Spin(+), or All-Mini(+).",
    );
  }
  if (spinProfile !== null && !mode.score && !preserveB2b) {
    throw invalidOption(
      "spin-profile",
      "spin-profile requires preserve-b2b=on for an unscored PC search.",
    );
  }
  const solutionProbabilities = onOffValue(
    values,
    "solution-probabilities",
    false,
  );
  const scoreProfile = mode.score
    ? normalizedChoice(optionalText(values, "score-profile", 32) ?? "tetrio")
    : null;
  if (scoreProfile !== null && !PC_SCORE_PROFILES.has(scoreProfile)) {
    throw invalidOption(
      "score-profile",
      "score-profile must be tetrio, guideline, or jstris-ultra.",
    );
  }
  const initialB2b = mode.score
    ? optionalInteger(values, "initial-b2b", 0, 65_535)
    : null;
  const failedCount = mode.failedQueue
    ? optionalInteger(values, "failed-count", 1, 4_294_967_295)
    : null;
  return [
    ...command.argvPrefix,
    "--lines", String(lines),
    "--board-mask", `0x${field.occupied.toString(16)}`,
    "--height", String(lines),
    "--pieces", String(emptyCount / 4),
    ...(!typedSave && /^[IOTSZJL]+$/i.test(next)
      ? ["--queue", next.toUpperCase()]
      : ["--patterns", next]),
    ...(hold === "disabled"
      ? ["--no-hold"]
      : ["--hold", hold === "empty" ? "empty" : hold.toUpperCase()]),
    ...(!mode.path && !mode.failedQueue && !mode.tiling && !mode.chance && !typedScoreProduct && !typedMinimumCover && !typedSave
      ? ["--objective", objective]
      : []),
    ...(queueKnowledge === "visible-7"
      ? ["--queue-knowledge", "visible-7"]
      : []),
    ...(mode.score
      ? [...(!typedScoreProduct ? ["--score"] : []), "--score-profile", scoreProfile]
      : []),
    ...(spinProfile !== null ? ["--spin-profile", spinProfile] : []),
    ...(preserveB2b ? ["--preserve-b2b"] : []),
    ...(initialB2b !== null ? ["--initial-b2b", String(initialB2b)] : []),
    ...(solutionProbabilities ? ["--solution-probabilities"] : []),
    ...(failedCount !== null ? ["--failed-count", String(failedCount)] : []),
    ...(mode.tiling ? [] : nativeRuleArguments(values)),
  ];
}

function nativePcScoreFinderArguments(command, values) {
  const field = normalizeSearchField(values.get("field"));
  const queue = validatedNext(values, true).toUpperCase();
  const lines = optionalInteger(values, "lines", 1, DISCORD_PC_FIELD_MAX_ROWS);
  if (lines === null) {
    throw new Error("PC score-finder requires one exact lines value after automatic target planning.");
  }
  const target = (1n << BigInt(lines * 10)) - 1n;
  if ((field.occupied & ~target) !== 0n) {
    throw new Error("field has occupied cells above the requested PC target.");
  }
  const emptyCount = popcount(target & ~field.occupied);
  if (emptyCount % 4 !== 0) {
    throw new Error("the PC target does not contain a whole number of tetrominoes.");
  }
  const hold = normalizedChoice(optionalText(values, "hold", 16) ?? "empty");
  if (!["disabled", "empty"].includes(hold) && !/^[iotszjl]$/u.test(hold)) {
    throw invalidOption(
      "hold",
      "hold must be disabled, empty, or one IOTSZJL piece.",
    );
  }
  const initialB2b = optionalInteger(values, "initial-b2b", 0, 1) ?? 0;
  return [
    ...command.argvPrefix,
    "--lines", String(lines),
    "--board-mask", `0x${field.occupied.toString(16)}`,
    "--height", String(lines),
    "--pieces", String(emptyCount / 4),
    "--queue", queue,
    ...(hold === "disabled"
      ? ["--no-hold"]
      : ["--hold", hold === "empty" ? "empty" : hold.toUpperCase()]),
    "--initial-b2b", String(initialB2b),
    ...nativeRuleArguments(values),
  ];
}

function nativePcAllspinArguments(command, values, exactQueue) {
  const field = normalizeSearchField(values.get("field"));
  const next = validatedNext(values, exactQueue);
  const lines = optionalInteger(values, "lines", 1, DISCORD_PC_FIELD_MAX_ROWS);
  if (lines === null) {
    throw new Error("All-Spin PC search requires one exact lines value after automatic target planning.");
  }

  const target = (1n << BigInt(lines * 10)) - 1n;
  if ((field.occupied & ~target) !== 0n) {
    throw new Error("field has occupied cells above the requested PC target.");
  }
  const emptyCount = popcount(target & ~field.occupied);
  if (emptyCount < 4 || emptyCount % 4 !== 0) {
    throw new Error("the PC target must contain a positive whole number of tetrominoes.");
  }

  const spinProfile = normalizedChoice(
    requiredText(values, "spin-profile", 32),
  );
  if (!SPIN_STRUCTURE_PROFILES.has(spinProfile)) {
    throw invalidOption(
      "spin-profile",
      "spin-profile must be T-Spins(+), All-Spin(+), or All-Mini(+).",
    );
  }
  const hold = onOffValue(values, "hold", true);
  const limits = [
    ["max-patterns", 4_294_967_295],
    ["max-nodes", 4_294_967_295],
    ["max-frontier-states", 4_294_967_295],
    ["max-candidates", 4_294_967_295],
    ["max-memory-mib", 1_048_576],
  ].flatMap(([name, maximum]) => {
    const value = optionalInteger(values, name, 1, maximum);
    return value === null ? [] : [`--${name}`, String(value)];
  });

  // An empty PC field is the native opening-PC contract. Supplying the
  // scenario trio for that field would select PcScenarioQuery, whose typed
  // All-Spin boundary correctly rejects an empty initial board. Only a real
  // initial field may enter the scenario contract.
  const initialFieldArguments = field.occupied === 0n
    ? []
    : [
        "--board-mask", `0x${field.occupied.toString(16)}`,
        "--height", String(lines),
        "--pieces", String(emptyCount / 4),
      ];

  return [
    ...command.argvPrefix,
    "--lines", String(lines),
    ...initialFieldArguments,
    exactQueue ? "--queue" : "--patterns",
    exactQueue ? next.toUpperCase() : next,
    ...(!hold ? ["--no-hold"] : []),
    "--spin-profile", spinProfile,
    ...nativeRuleArguments(values),
    ...limits,
  ];
}

function forwardSearchArguments(command, values, spinFinder) {
  const field = normalizeSearchField(values.get("field"), {
    maxBits: 240,
    maxRows: DISCORD_WIDE_FIELD_MAX_ROWS,
  });
  const requestedHeight = optionalInteger(
    values,
    "height",
    1,
    DISCORD_WIDE_FIELD_MAX_ROWS,
  );
  const height = requestedHeight ?? Math.max(8, field.height);
  if (height < field.height) {
    throw new Error("height must include every occupied row in field.");
  }

  const next = validatedNext(values, !spinFinder);
  const sourceArguments = spinFinder && !/^[IOTSZJL]+$/i.test(next)
    ? ["--patterns", next]
    : ["--queue", next.toUpperCase()];
  const hold = onOffValue(values, "hold", true);

  const requestedProfile = optionalText(values, "spin-profile", 32);
  const profile = normalizedChoice(
    requestedProfile ?? (spinFinder ? "t-spins" : "all-mini-plus"),
  );
  const supportedProfiles = spinFinder
    ? SPIN_STRUCTURE_PROFILES
    : DAMAGE_SPIN_PROFILES;
  if (!supportedProfiles.has(profile)) {
    throw new Error(
      "spin-profile must be disabled (damage only), T-Spins(+), All-Spin(+), or All-Mini(+).",
    );
  }

  const preserveB2b = onOffValue(values, "preserve-b2b", false);
  const initialCombo = optionalInteger(values, "initial-combo", 0, 65_535);
  const initialB2b = optionalInteger(values, "initial-b2b", 0, 65_535);
  const mode = normalizedChoice(
    optionalText(values, "damage-mode", 16) ?? "maximum",
  );
  if (!spinFinder && !new Set(["maximum", "at-least"]).has(mode)) {
    throw new Error("damage-mode must be maximum or at-least.");
  }
  const minimumDamage = optionalInteger(
    values,
    "minimum-damage",
    0,
    4_294_967_295,
  );
  if (!spinFinder && mode === "at-least" && minimumDamage === null) {
    throw new Error("damage-mode=at-least requires minimum-damage.");
  }
  if (!spinFinder && mode === "maximum" && minimumDamage !== null) {
    throw new Error("minimum-damage is available only with damage-mode=at-least.");
  }

  const lines = normalizedChoice(optionalText(values, "lines", 8) ?? "any");
  if (spinFinder && !FORWARD_SPIN_LINES.has(lines)) {
    throw new Error("lines must be any, 0 through 4, or 1+ through 4+.");
  }
  const category = normalizedChoice(
    optionalText(values, "spin-category", 16) ?? "any",
  );
  if (spinFinder && !FORWARD_SPIN_CATEGORIES.has(category)) {
    throw new Error("spin-category must be any, t, or other.");
  }
  const allPieceProfile = /^(?:all-spin|all-mini)(?:-plus)?$/.test(profile);
  if (spinFinder && category === "other" && !allPieceProfile) {
    throw new Error(
      "spin-category other requires an All-Spin or All-Mini spin-profile.",
    );
  }

  return [
    ...command.argvPrefix,
    "--board-mask-v1",
    field.mask,
    "--height",
    String(height),
    ...sourceArguments,
    hold ? "--hold" : "--no-hold",
    "--spin-profile",
    profile,
    ...nativeRuleArguments(values),
    ...(preserveB2b ? ["--preserve-b2b"] : []),
    ...(initialCombo !== null && initialCombo > 0
      ? ["--initial-combo", String(initialCombo)]
      : []),
    ...(spinFinder
      ? [
          ...(initialB2b !== null ? ["--initial-b2b", String(initialB2b)] : []),
          "--lines",
          lines,
          "--spin-category",
          category,
        ]
      : [
          "--initial-b2b",
          String(initialB2b ?? 0),
          ...(mode === "at-least"
            ? ["--minimum-damage", String(minimumDamage)]
            : []),
        ]),
  ];
}

function forwardRenArguments(command, values) {
  const field = normalizeSearchField(values.get("field"), {
    maxBits: 240,
    maxRows: DISCORD_WIDE_FIELD_MAX_ROWS,
  });
  const requestedHeight = optionalInteger(
    values,
    "height",
    1,
    DISCORD_WIDE_FIELD_MAX_ROWS,
  );
  const height = requestedHeight ?? Math.max(8, field.height);
  if (height < field.height) {
    throw new Error("height must include every occupied row in field.");
  }
  const next = validatedNext(values, true).toUpperCase();
  if (next.length > 22) {
    throw new Error("REN next must contain at most 22 pieces.");
  }
  const hold = onOffValue(values, "hold", true);
  return [
    ...command.argvPrefix,
    "--board-mask-v1",
    field.mask,
    "--height",
    String(height),
    "--queue",
    next,
    hold ? "--hold" : "--no-hold",
    ...nativeRuleArguments(values),
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
  const settings = parseSettings(command, values, new Map([["hold", "hold"]]));
  const hold = settings.has("hold")
    ? booleanSetting(settings.get("hold"), "hold") === "true"
    : true;
  return [
    ...command.argvPrefix,
    "--base-mask",
    base.mask,
    "--target-mask",
    target.mask,
    "--height",
    String(Math.max(1, base.height, target.height)),
    ...(/^[IOTSZJL]+$/i.test(next)
      ? ["--queue", next.toUpperCase()]
      : ["--patterns", next]),
    ...(hold ? ["--hold", "empty"] : ["--no-hold"]),
    "--no-mirror",
    ...kicktableArguments(values),
  ];
}

function buildCoverArguments(command, values) {
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

  const resultMode = normalizedChoice(
    optionalText(values, "result-mode", 48) ?? "all-solutions",
  );
  const resultModes = new Set([
    "all-solutions",
    "complete-replay-paths",
    "minimum-solutions",
    "field-average-score",
    "fixed-queue-maximum-score",
    "highest-score-minimum-set",
    "failed-queues",
  ]);
  if (!resultModes.has(resultMode)) {
    throw new Error("result-mode is not a supported Build result aggregation.");
  }

  const visibleHeight = Math.max(1, base.height, target.height);
  const requestedHeight = optionalInteger(
    values,
    "height",
    1,
    DISCORD_WIDE_FIELD_MAX_ROWS,
  );
  const height = requestedHeight ?? (
    resultMode === "complete-replay-paths"
      ? visibleHeight
      : Math.max(8, visibleHeight)
  );
  if (height < visibleHeight) {
    throw new Error("height must include every occupied row in base and target.");
  }
  if (resultMode === "complete-replay-paths" && height > 6) {
    throw new Error("complete-replay-paths requires a Build height from 1 through 6.");
  }

  const aggregationSource = optionalText(values, "aggregation", 16);
  const aggregation = normalizedChoice(aggregationSource ?? "buildability");
  if (!new Set(["buildability", "spin", "tiling"]).has(aggregation)) {
    throw new Error("aggregation must be buildability, spin, or tiling.");
  }

  const preserveB2b = onOffValue(values, "preserve-b2b", false);
  const solutionProbabilities = onOffValue(values, "solution-probabilities", false);
  const spinProfileSource = optionalText(values, "spin-profile", 32);
  const spinProfile = normalizedChoice(spinProfileSource ?? "t-spins");
  if (!SPIN_STRUCTURE_PROFILES.has(spinProfile)) {
    throw new Error("spin-profile must be T-Spins(+), All-Spin(+), or All-Mini(+).");
  }
  if (spinProfileSource !== null && aggregation !== "spin" && !preserveB2b) {
    throw new Error("spin-profile requires aggregation=spin or preserve-b2b=on.");
  }

  const finesse = (optionalText(values, "finesse", 16) ?? "off")
    .trim()
    .toLowerCase();
  if (!new Set(["off", "inputs"]).has(finesse)) {
    throw new Error("finesse must be off or inputs.");
  }
  if (
    finesse !== "inputs" &&
    containsCompletedRow(base.occupied, Math.max(base.height, target.height))
  ) {
    throw new Error("base must not contain an already completed row.");
  }

  const knowledge = optionalText(values, "finesse-knowledge", 16)?.toLowerCase();
  if (knowledge && finesse !== "inputs") {
    throw new Error("finesse-knowledge requires finesse=inputs.");
  }
  if (knowledge && !new Set(["both", "oracle", "visible-7"]).has(knowledge)) {
    throw new Error("finesse-knowledge must be both, oracle, or visible-7.");
  }
  if (aggregation === "tiling") {
    const incompatible = [
      [values.has("kicktable"), "kicktable"],
      [spinProfileSource !== null, "spin-profile"],
      [preserveB2b, "preserve-b2b"],
      [solutionProbabilities, "solution-probabilities"],
      [finesse !== "off", "finesse"],
      [knowledge !== undefined && knowledge !== null, "finesse-knowledge"],
    ].find(([enabled]) => enabled);
    if (incompatible) {
      throw new Error(`${incompatible[1]} is unavailable with aggregation=tiling.`);
    }
  }

  const hold = (optionalText(values, "hold", 16) ?? "empty").trim();
  let holdArguments;
  if (hold.toLowerCase() === "disabled") {
    holdArguments = ["--no-hold"];
  } else if (hold.toLowerCase() === "empty") {
    holdArguments = ["--hold", "empty"];
  } else if (/^[IOTSZJL]$/i.test(hold)) {
    holdArguments = ["--hold", hold.toUpperCase()];
  } else {
    throw new Error("hold must be disabled, empty, or one IOTSZJL piece.");
  }
  const sourcePieces = optionalInteger(
    values,
    "source-pieces",
    1,
    4_294_967_295,
  );
  const scoreMode = new Set([
    "field-average-score",
    "fixed-queue-maximum-score",
    "highest-score-minimum-set",
  ]).has(resultMode);
  const scoreProfileSource = optionalText(values, "score-profile", 32);
  const scoreProfile = normalizedChoice(scoreProfileSource ?? "tetrio");
  if (!new Set(["tetrio", "guideline", "jstris-ultra"]).has(scoreProfile)) {
    throw new Error("score-profile must be tetrio, guideline, or jstris-ultra.");
  }
  const initialB2b = optionalInteger(values, "initial-b2b", 0, 65_535);
  const failedCount = optionalInteger(values, "failed-count", 1, 4_294_967_295);
  if (!scoreMode && (scoreProfileSource !== null || initialB2b !== null)) {
    throw new Error("score-profile and initial-b2b require a score result-mode.");
  }
  if (resultMode !== "failed-queues" && failedCount !== null) {
    throw new Error("failed-count requires result-mode=failed-queues.");
  }
  if (
    resultMode !== "all-solutions" &&
    (aggregation !== "buildability" || preserveB2b || solutionProbabilities ||
      spinProfileSource !== null || finesse !== "off" || knowledge != null)
  ) {
    throw new Error(
      "Non-all Build result modes require aggregation=buildability without spin, B2B, solution-probability, or finesse options.",
    );
  }
  const next = validatedNext(values, resultMode === "fixed-queue-maximum-score");
  const nextArguments = /^[IOTSZJL]+$/i.test(next)
    ? ["--queue", next.toUpperCase()]
    : ["--patterns", next];

  const mirror = (optionalText(values, "mirror", 16) ?? "auto").toLowerCase();
  if (!new Set(["auto", "include", "exclude"]).has(mirror)) {
    throw new Error("mirror must be auto, include, or exclude.");
  }
  const mirrorArguments = mirror === "include"
    ? ["--include-mirror"]
    : mirror === "exclude"
      ? ["--no-mirror"]
      : mirrorMask(base.occupied, height) === base.occupied
        ? ["--include-mirror"]
        : ["--no-mirror"];

  if (resultMode === "minimum-solutions") {
    if (values.has("mirror")) {
      throw new Error("mirror is not available with result-mode=minimum-solutions.");
    }
    return [
      "build",
      "cover",
      "--base-mask",
      base.mask,
      "--target-mask",
      target.mask,
      "--height",
      String(height),
      ...nextArguments,
      ...holdArguments,
      ...(sourcePieces === null ? [] : ["--source-pieces", String(sourcePieces)]),
      "--queue-knowledge",
      "oracle",
      "--objective",
      "min-cover",
      ...kicktableArguments(values, true),
      "--backend",
      "cpu",
      "--no-backend-fallback",
    ];
  }

  return [
    ...command.argvPrefix,
    "--base-mask",
    base.mask,
    "--target-mask",
    target.mask,
    "--height",
    String(height),
    ...nextArguments,
    ...holdArguments,
    ...(sourcePieces === null
      ? []
      : ["--source-pieces", String(sourcePieces)]),
    ...(aggregation === "tiling"
      ? ["--tiling-only"]
      : [
          ...(aggregationSource !== null || aggregation === "spin"
            ? ["--aggregate", aggregation]
            : []),
          ...((aggregation === "spin" || preserveB2b)
            ? ["--spin-profile", spinProfile]
            : []),
          ...(preserveB2b ? ["--preserve-b2b"] : []),
          ...(solutionProbabilities ? ["--solution-probabilities"] : []),
        ]),
    ...(finesse === "inputs"
      ? ["--finesse", "inputs", "--pattern-knowledge", knowledge ?? "both"]
      : []),
    ...(resultMode === "all-solutions"
      ? []
      : ["--result-mode", resultMode]),
    ...(scoreMode
      ? [
          "--score-profile",
          scoreProfile,
          "--initial-b2b",
          String(initialB2b ?? 0),
        ]
      : []),
    ...(resultMode === "failed-queues"
      ? ["--failed-count", String(failedCount ?? 100)]
      : []),
    ...mirrorArguments,
    ...(aggregation === "tiling" ? [] : kicktableArguments(values, true)),
  ];
}

function buildV2Arguments(command, values) {
  const objectives = BUILD_V2_OBJECTIVES[command.capabilityId];
  if (!objectives) {
    throw new Error(`Unknown Build v2 capability '${command.capabilityId}'.`);
  }

  const sourceArguments = command.input === "build-v2-cover"
    ? buildV2MaskSourceArguments(values)
    : command.input === "build-v2-target"
      ? buildV2DocumentSourceArguments(values, "target")
      : buildV2DocumentSourceArguments(values, "solution");
  const supplyArguments = buildV2SupplyArguments(values);
  const holdArguments = buildV2HoldArguments(values);
  const queueKnowledge = normalizedChoice(
    optionalText(values, "queue-knowledge", 16) ?? "oracle",
  );
  if (!new Set(["oracle", "visible-7"]).has(queueKnowledge)) {
    throw new Error("queue-knowledge must be oracle or visible-7.");
  }
  const objective = normalizedChoice(
    optionalText(values, "objective", 32) ?? objectives[0],
  );
  if (!objectives.includes(objective)) {
    throw new Error(
      `${command.capabilityId} accepts only objective ${objectives.join(" or ")}.`,
    );
  }

  const scoreArguments = [];
  if (BUILD_V2_SCORE_CAPABILITIES.has(command.capabilityId)) {
    const scoreProfile = normalizedChoice(
      optionalText(values, "score-profile", 32) ?? "tetrio",
    );
    if (!PC_SCORE_PROFILES.has(scoreProfile)) {
      throw new Error("score-profile must be tetrio, guideline, or jstris-ultra.");
    }
    const initialB2b = optionalInteger(values, "initial-b2b", 0, 65_535) ?? 0;
    scoreArguments.push(
      "--score-profile",
      scoreProfile,
      "--initial-b2b",
      String(initialB2b),
    );
  }

  return [
    ...command.argvPrefix,
    ...sourceArguments,
    ...supplyArguments,
    ...holdArguments,
    "--queue-knowledge",
    queueKnowledge,
    "--objective",
    objective,
    ...scoreArguments,
    ...nativeRuleArguments(values),
    "--backend",
    "cpu",
    "--no-backend-fallback",
  ];
}

function buildV2MaskSourceArguments(values) {
  const base = buildV2BoardMask(requiredText(values, "base-mask", 66), "base-mask");
  const target = buildV2BoardMask(requiredText(values, "target-mask", 66), "target-mask");
  const height = optionalInteger(
    values,
    "height",
    1,
    DISCORD_PC_FIELD_MAX_ROWS,
  );
  if (height === null) throw new Error("/height input is required.");
  const visibleMask = (1n << BigInt(height * 10)) - 1n;
  if ((base.value & ~visibleMask) !== 0n || (target.value & ~visibleMask) !== 0n) {
    throw new Error("base-mask and target-mask must fit inside the requested visible height.");
  }
  if (target.value === 0n) {
    throw new Error("target-mask must contain at least one target cell.");
  }
  if ((base.value & target.value) !== 0n) {
    throw new Error("base-mask and target-mask must not overlap.");
  }
  if (popcount(target.value) % 4 !== 0) {
    throw new Error("target-mask occupied-cell count must be divisible by four.");
  }
  const sourcePieces = optionalInteger(values, "source-pieces", 1, 4_294_967_295);
  return [
    "--base-mask",
    base.canonical,
    "--target-mask",
    target.canonical,
    "--height",
    String(height),
    ...(sourcePieces === null ? [] : ["--source-pieces", String(sourcePieces)]),
  ];
}

function buildV2BoardMask(source, name) {
  const value = source.trim();
  let parsed;
  if (/^0x[0-9a-f]{1,64}$/iu.test(value)) {
    parsed = BigInt(value);
  } else if (/^(?:0|[1-9][0-9]*)$/u.test(value)) {
    parsed = BigInt(value);
  } else {
    throw new Error(`${name} must be canonical decimal or 0x-prefixed hexadecimal.`);
  }
  if (parsed >= (1n << 256n)) {
    throw new Error(`${name} exceeds the 256-bit Build v2 board contract.`);
  }
  return Object.freeze({
    value: parsed,
    canonical: `0x${parsed.toString(16)}`,
  });
}

function buildV2DocumentSourceArguments(values, role) {
  const formatName = `${role}-format`;
  const documentName = `${role}-document`;
  const format = normalizedChoice(requiredText(values, formatName, 16));
  const document = normalizeBuildV2ColoredDocument(
    requiredText(values, documentName, FIELD_MAX_LENGTH),
    { name: documentName, format },
  );
  return [
    `--${formatName}`,
    document.sourceFormat,
    `--${documentName}`,
    document.source,
  ];
}

function buildV2SupplyArguments(values) {
  const hasQueue = values.has("queue");
  const hasPatterns = values.has("patterns");
  if (hasQueue === hasPatterns) {
    throw new DiscordInputError(
      "source.invalid",
      {},
      "Build v2 requires exactly one of queue or patterns.",
    );
  }
  if (hasQueue) {
    const queue = requiredText(values, "queue", NEXT_MAX_LENGTH);
    if (!/^[IOTSZJL]+$/iu.test(queue)) {
      throw new DiscordInputError(
        "source.invalid",
        {},
        "queue must contain only exact IOTSZJL pieces.",
      );
    }
    return ["--queue", queue.toUpperCase()];
  }
  const patterns = requiredText(values, "patterns", NEXT_MAX_LENGTH);
  if (patterns.startsWith("-")) {
    throw new DiscordInputError(
      "source.invalid",
      {},
      "patterns must be a queue-pattern language, not a command-line option.",
    );
  }
  return ["--patterns", patterns];
}

function buildV2HoldArguments(values) {
  const hold = normalizedChoice(optionalText(values, "hold", 16) ?? "empty");
  if (hold === "disabled") return ["--no-hold"];
  if (hold === "empty") return ["--hold", "empty"];
  if (/^[iotszjl]$/u.test(hold)) return ["--hold", hold.toUpperCase()];
  throw new Error("hold must be disabled, empty, or one IOTSZJL piece.");
}

function finesseSearchArguments(command, values) {
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
  return [
    ...command.argvPrefix,
    "--base-mask",
    base.mask,
    "--target-mask",
    target.mask,
    "--height",
    String(Math.max(8, base.height, target.height)),
    ...finesseNextArguments(values),
    ...finessePackedArguments(command, values),
    "--finesse",
    "inputs",
    "--no-mirror",
    ...kicktableArguments(values, true),
  ];
}

function finesseScoreArguments(command, values) {
  const document = normalizeFinesseDocument(values.get("document"));
  return [
    ...command.argvPrefix,
    "--initial-mask",
    document.initialMask,
    "--height",
    String(document.height),
    "--placements",
    document.placements.join(","),
    ...finesseNextArguments(values),
    ...(command.input === "finesse-score-v2"
      ? finesseScoreNamedArguments(values)
      : finessePackedArguments(command, values)),
    ...kicktableArguments(values, true),
  ];
}

function operationDocumentArguments(command, values) {
  if (values.has("attachment")) {
    throw invalidOption(
      "attachment",
      "attachment must be resolved to its bounded CTK3 document before lowering.",
    );
  }
  const document = normalizeOperationDocument(values.get("document"));
  const ruleProfile = normalizedChoice(
    optionalText(values, "rule-profile", 32) ?? "srs-plus",
  );
  const kickProfile = normalizedChoice(
    optionalText(values, "kick-profile", 32) ?? "srs-plus",
  );
  if (!NATIVE_KICKTABLES.has(ruleProfile)) {
    throw invalidOption(
      "rule-profile",
      "rule-profile must be srs-plus, srs, srs-x, jstris-180, or no-kick.",
    );
  }
  if (!NATIVE_KICKTABLES.has(kickProfile)) {
    throw invalidOption(
      "kick-profile",
      "kick-profile must be srs-plus, srs, srs-x, jstris-180, or no-kick.",
    );
  }
  const timeoutSeconds = optionalInteger(values, "timeout-seconds", 1, 900) ?? 900;
  return [
    ...command.argvPrefix,
    "--document",
    document.source,
    "--rule-profile",
    ruleProfile,
    "--kick-profile",
    kickProfile,
    "--timeout-seconds",
    String(timeoutSeconds),
  ];
}

function fieldDocumentArguments(command, values) {
  rejectUnresolvedTypedDocumentAttachment(values);
  const document = normalizeTypedFieldDocument(values.get("document"));
  return [
    ...command.argvPrefix,
    "--document",
    document.source,
  ];
}

function fumenTransformArguments(command, values) {
  rejectUnresolvedTypedDocumentAttachment(values);
  const transform = normalizedChoice(requiredText(values, "transform", 32));
  if (!FUMEN_TRANSFORMS.has(transform)) {
    throw invalidOption("transform", "transform is not a supported closed Fumen transform.");
  }

  const single = optionalText(
    values,
    "document",
    OPERATION_DOCUMENT_MAX_SOURCE_CHARS,
  );
  const combined = optionalText(
    values,
    "documents",
    OPERATION_DOCUMENT_MAX_SOURCE_CHARS,
  );
  if (single && combined) {
    throw invalidOption(
      "documents",
      "document and documents cannot be supplied together.",
    );
  }
  const rawDocuments = combined
    ? boundedNonEmptyLines(combined, "documents", OPERATION_DOCUMENT_MAX_PAGES)
    : single
      ? [single]
      : [];
  const documents = rawDocuments.map((source, index) =>
    normalizeTypedFieldDocument(source, {
      name: `document ${index + 1}`,
      requireFumen: true,
    }).source
  );
  const commentsSource = optionalText(
    values,
    "comments",
    OPERATION_DOCUMENT_MAX_SOURCE_CHARS,
  );
  const comments = commentsSource
    ? boundedNonEmptyLines(
        commentsSource,
        "comments",
        OPERATION_DOCUMENT_MAX_PAGES,
      )
    : [];
  const page = optionalInteger(values, "page", 1, OPERATION_DOCUMENT_MAX_PAGES);
  const offset = optionalSignedInteger(
    values,
    "offset",
    -OPERATION_DOCUMENT_MAX_PAGES,
    OPERATION_DOCUMENT_MAX_PAGES,
  );

  if (transform === "combine") {
    if (documents.length === 0) {
      throw invalidOption("documents", "combine requires one or more Fumen documents.");
    }
  } else if (transform === "text-to-fumen") {
    if (documents.length !== 0) {
      throw invalidOption("document", "text-to-fumen does not accept a document.");
    }
    if (comments.length === 0) {
      throw invalidOption("comments", "text-to-fumen requires one or more comments.");
    }
  } else if (documents.length !== 1) {
    throw invalidOption("document", `${transform} requires exactly one Fumen document.`);
  }
  if (transform !== "text-to-fumen" && comments.length !== 0) {
    throw invalidOption("comments", `${transform} does not accept comments.`);
  }
  if ((transform === "get-page") !== (page !== null)) {
    throw invalidOption(
      "page",
      transform === "get-page"
        ? "get-page requires one page number."
        : `${transform} does not accept page.`,
    );
  }
  if ((transform === "page-shift") !== (offset !== null)) {
    throw invalidOption(
      "offset",
      transform === "page-shift"
        ? "page-shift requires one signed offset."
        : `${transform} does not accept offset.`,
    );
  }

  const arguments_ = [...command.argvPrefix, transform];
  for (const document of documents) {
    arguments_.push("--document", document);
  }
  if (page !== null) arguments_.push("--page", String(page));
  if (offset !== null) arguments_.push("--offset", String(offset));
  for (const comment of comments) arguments_.push("--comment", comment);
  return arguments_;
}

function renderDocumentArguments(command, values) {
  rejectUnresolvedTypedDocumentAttachment(values);
  const document = normalizeTypedFieldDocument(values.get("document"));
  const artifactFormat = normalizedChoice(
    requiredText(values, "artifact-format", 16),
  );
  if (!new Set(["png", "gif"]).has(artifactFormat)) {
    throw invalidOption("artifact-format", "artifact-format must be png or gif.");
  }
  const page = optionalInteger(values, "page", 1, OPERATION_DOCUMENT_MAX_PAGES);
  if (artifactFormat === "gif" && page !== null) {
    throw invalidOption("page", "GIF renders the document timeline and does not accept page.");
  }
  return [
    ...command.argvPrefix,
    "--document",
    document.source,
    "--artifact-format",
    artifactFormat,
    ...(page === null ? [] : ["--page", String(page)]),
  ];
}

function rejectUnresolvedTypedDocumentAttachment(values) {
  if (values.has("attachment")) {
    throw invalidOption(
      "attachment",
      "attachment must be resolved to bounded canonical document text before lowering.",
    );
  }
}

function boundedNonEmptyLines(source, name, maximumLines) {
  const lines = String(source)
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .filter(Boolean);
  if (lines.length === 0 || lines.length > maximumLines) {
    throw invalidOption(
      name,
      `${name} must contain from 1 through ${maximumLines} non-empty lines.`,
    );
  }
  return lines;
}

function optionalSignedInteger(values, name, minimum, maximum) {
  if (!values.has(name)) return null;
  const raw = values.get(name);
  const value = typeof raw === "string" && /^-?\d+$/.test(raw.trim())
    ? Number(raw.trim())
    : raw;
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw invalidOption(
      name,
      `${name} must be an integer from ${minimum} through ${maximum}.`,
    );
  }
  return value;
}

function finesseScoreNamedArguments(values) {
  const hold = normalizedChoice(optionalText(values, "hold", 16) ?? "empty");
  const output = [];
  if (hold === "disabled") {
    output.push("--no-hold");
  } else if (hold === "empty") {
    output.push("--hold", "empty");
  } else if (/^[iotszjl]$/.test(hold)) {
    output.push("--hold", hold.toUpperCase());
  } else {
    throw invalidOption("hold", "hold must be disabled, empty, or one IOTSZJL piece.");
  }

  const requestedKnowledge = normalizedChoice(
    optionalText(values, "knowledge", 16) ?? "both",
  );
  const knowledge = requestedKnowledge === "full-queue"
    ? "oracle"
    : requestedKnowledge;
  if (!["both", "oracle", "visible-7"].includes(knowledge)) {
    throw invalidOption(
      "knowledge",
      "knowledge must be both, oracle, or visible-7.",
    );
  }
  output.push("--pattern-knowledge", knowledge);

  const sourcePieces = optionalInteger(values, "source-pieces", 1, 128);
  if (sourcePieces !== null) {
    output.push("--source-pieces", String(sourcePieces));
  }
  return output;
}

function finesseNextArguments(values) {
  const next = validatedNext(values, false);
  return /^[IOTSZJL]+$/i.test(next)
    ? ["--queue", next.toUpperCase()]
    : ["--patterns", next];
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
  const named = command.input !== "spin-structure";
  const requestedHeight = optionalInteger(
    values,
    "height",
    1,
    DISCORD_WIDE_FIELD_MAX_ROWS,
  );
  const height = requestedHeight ?? Math.max(8, field.height);
  if (height < field.height) {
    throw new Error("height must include every occupied row in field.");
  }
  const lines = (optionalText(values, "lines", 8) ?? "1+").toLowerCase();
  if (!/^(?:any|[0-4]|[1-4]\+)$/.test(lines)) {
    throw new Error("lines must be any, 0 through 4, or 1+ through 4+.");
  }
  if (
    ["spin-structure-cover-v1", "spin-structure-guaranteed-v1"].includes(command.input) &&
    lines === "0"
  ) {
    throw invalidOption("lines", "cover and guaranteed structure lines cannot be zero.");
  }
  const profile = (optionalText(
    values,
    named ? "spin-profile" : "profile",
    32,
  ) ?? "t-spins")
    .toLowerCase()
    .replaceAll("_", "-");
  if (!SPIN_STRUCTURE_PROFILES.has(profile)) {
    throw new DiscordInputError(
      "profile.invalid",
      {},
      "profile must be T-Spins, T-Spins+, All-Mini(+), or All-Spin(+).",
    );
  }
  const settings = named
    ? spinStructureNamedArguments(values, height, pieces.length)
    : spinStructurePackedArguments(
        command,
        values,
        field.height,
        pieces.length,
      );
  const productSettings = spinStructureProductArguments(
    command,
    values,
    pieces,
    profile,
  );
  const rule = ["spin-structure-cover-v1", "spin-structure-guaranteed-v1"]
    .includes(command.input)
    ? strictSpinStructureRuleArguments(values)
    : kicktableArguments(values, true);
  return [
    ...command.argvPrefix,
    "--board-mask-v1",
    field.mask,
    "--pieces",
    pieces,
    ...(named ? ["--height", String(height)] : []),
    "--lines",
    lines,
    "--spin-profile",
    profile,
    ...settings,
    ...rule,
    ...productSettings,
  ];
}

function spinStructureProductArguments(command, values, pieces, profile) {
  if (command.input === "spin-structure-cover-v1") {
    const maxPatterns = optionalInteger(values, "max-patterns", 1, 100_000);
    return [
      "--objective",
      "min-cover",
      ...(maxPatterns === null ? [] : ["--max-patterns", String(maxPatterns)]),
    ];
  }
  if (command.input !== "spin-structure-guaranteed-v1") return [];

  const finalPiece = (optionalText(values, "final-piece", 1) ?? "T").toUpperCase();
  if (!/^[IOTSZJL]$/u.test(finalPiece)) {
    throw invalidOption("final-piece", "final-piece must be one IOTSZJL piece.");
  }
  if (!pieces.includes(finalPiece)) {
    throw invalidOption("final-piece", "final-piece must occur in pieces.");
  }
  if (profile.startsWith("t-spins") && finalPiece !== "T") {
    throw invalidOption("final-piece", "T-Spin profiles require final-piece T.");
  }
  const maxPatterns = optionalInteger(values, "max-patterns", 1, 100_000);
  const dependencyReport = onOffValue(values, "dependency-report", false);
  return [
    "--final-piece",
    finalPiece,
    ...(maxPatterns === null ? [] : ["--max-patterns", String(maxPatterns)]),
    dependencyReport ? "--dependency-report" : "--no-dependency-report",
  ];
}

function strictSpinStructureRuleArguments(values) {
  const rule = normalizedChoice(optionalText(values, "kicktable", 32) ?? "srs-plus");
  if (!new Set(["srs-plus", "srs"]).has(rule)) {
    throw invalidOption("kicktable", "kicktable must be srs-plus or srs.");
  }
  return ["--rule", rule];
}

function spinStructureNamedArguments(values, height, pieceCount) {
  const fillBottom = optionalInteger(values, "fill-bottom", 0, height - 1);
  const fillTop = optionalInteger(values, "fill-top", 1, height);
  const effectiveBottom = fillBottom ?? 0;
  const effectiveTop = fillTop ?? Math.min(5, height);
  if (effectiveBottom >= effectiveTop) {
    throw new DiscordInputError("options.spin_fill_bounds");
  }
  const maximumPlacements = optionalInteger(
    values,
    "max-placements",
    1,
    pieceCount,
  );
  const minimalitySource = optionalText(values, "minimality", 32);
  const minimality = minimalitySource === null
    ? null
    : normalizedChoice(minimalitySource);
  if (
    minimality !== null &&
    !new Set(["subset-minimal", "minimum-piece-count"]).has(minimality)
  ) {
    throw invalidOption(
      "minimality",
      "minimality must be subset-minimal or minimum-piece-count.",
    );
  }
  return [
    ...(fillBottom !== null ? ["--fill-bottom", String(fillBottom)] : []),
    ...(fillTop !== null ? ["--fill-top", String(fillTop)] : []),
    ...(maximumPlacements !== null
      ? ["--max-placements", String(maximumPlacements)]
      : []),
    ...(minimality !== null ? ["--minimality", minimality] : []),
  ];
}

function validatedNext(values, fixed) {
  let next;
  try {
    next = requiredText(values, "next", NEXT_MAX_LENGTH);
  } catch (error) {
    throw new DiscordInputError(
      "source.invalid",
      {},
      error instanceof Error ? error.message : "next source is invalid.",
    );
  }
  if (next.startsWith("-")) {
    throw new DiscordInputError(
      "source.invalid",
      {},
      "next must be a queue or pattern, not a command-line option.",
    );
  }
  if (fixed && !/^[IOTSZJL]+$/i.test(next)) {
    throw new DiscordInputError(
      "source.invalid",
      {},
      "next must be an exact queue containing only IOTSZJL pieces.",
    );
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
    pages = decodeFumenWithinPageLimit(
      normalized,
      (bounded) => fumenDecoder.decode(bounded),
      1,
    );
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
  const settings = command.input === "pc-v2"
    ? new Map()
    : parseSettings(command, values, new Map([["hold", "hold"]]));
  const output = [];
  const selectedLines = optionalInteger(
    values,
    "lines",
    1,
    DISCORD_PC_FIELD_MAX_ROWS,
  );
  if (selectedLines !== null) {
    output.push("--lines", String(selectedLines));
  }
  const namedHold = command.input === "pc-v2"
    ? optionalText(values, "hold", 16)
    : null;
  if (namedHold !== null) {
    const normalized = normalizedChoice(namedHold);
    if (!["use", "avoid"].includes(normalized)) {
      throw invalidOption("hold", "hold must be use or avoid.");
    }
    output.push("--hold", normalized === "use" ? "true" : "false");
  } else if (settings.has("hold")) {
    output.push("--hold", booleanSetting(settings.get("hold"), "hold"));
  }
  return output;
}

function spinSettings(command, values) {
  const settings = parseSettings(command, values, new Map([["type", "type"]]));
  const type = (settings.get("type") ?? "TSS").toUpperCase();
  if (!SPIN_TYPES.has(type)) {
    throw new Error("options type must be TSS, TSD, TST, TSPIN, T-SPIN, or ANY; TSM is unavailable.");
  }
  const lines = ({ TSS: "1", TSD: "2", TST: "3" })[type] ?? "any";
  return [
    "--hold",
    "--spin-profile",
    "t-spins",
    "--lines",
    lines,
    "--spin-category",
    "t",
  ];
}

function catFinderSettings(command, values) {
  const settings = command.input === "score-fixed-next-v2"
    ? new Map()
    : parseSettings(command, values, new Map([
        ["initial_b2b", "initial_b2b"],
        ["initial-b2b", "initial_b2b"],
        ["b2b", "initial_b2b"],
      ]));
  const output = [];
  const selectedLines = optionalInteger(values, "lines", 1, 6);
  if (selectedLines !== null) {
    output.push("--lines", String(selectedLines));
  }
  const namedInitialB2b = command.input === "score-fixed-next-v2"
    ? optionalText(values, "initial-b2b", 8)
    : null;
  if (namedInitialB2b !== null) {
    output.push(
      "--initial-b2b",
      onOffValue(values, "initial-b2b", false) ? "true" : "false",
    );
  } else if (settings.has("initial_b2b")) {
    output.push(
      "--initial-b2b",
      booleanSetting(settings.get("initial_b2b"), "initial_b2b"),
    );
  }
  return output;
}

function setupScoreArguments(command, values) {
  const documentFormat = normalizedChoice(
    requiredText(values, "document-format", 16),
  );
  if (!new Set(["ctk3", "fumen"]).has(documentFormat)) {
    throw invalidOption(
      "document-format",
      "document-format must be ctk3 or fumen.",
    );
  }
  const document = normalizeTypedFieldDocument(values.get("document"), {
    name: "document",
  });
  if (document.sourceFormat !== documentFormat) {
    throw invalidOption(
      "document-format",
      `document-format ${documentFormat} does not match the supplied ${document.sourceFormat} document.`,
    );
  }

  const setupSupply = setupScoreSupplyArguments(values, "setup");
  const solutionSupply = setupScoreSupplyArguments(values, "solution");
  const clearHeight = optionalInteger(
    values,
    "clear",
    1,
    DISCORD_PC_FIELD_MAX_ROWS,
  ) ?? 4;
  const holdEnabled = onOffValue(values, "hold", true);
  const scoreProfile = normalizedChoice(
    optionalText(values, "score-profile", 32) ?? "tetrio",
  );
  if (!PC_SCORE_PROFILES.has(scoreProfile)) {
    throw invalidOption(
      "score-profile",
      "score-profile must be tetrio, guideline, or jstris-ultra.",
    );
  }
  const initialB2b = optionalInteger(
    values,
    "initial-b2b",
    0,
    4_294_967_295,
  ) ?? 0;
  const maxPatterns = optionalInteger(
    values,
    "max-patterns",
    1,
    4_294_967_295,
  );

  return [
    ...command.argvPrefix,
    "--document-format", documentFormat,
    "--document", document.source,
    ...setupSupply,
    ...solutionSupply,
    "--clear", String(clearHeight),
    holdEnabled ? "--hold" : "--no-hold",
    "--score-profile", scoreProfile,
    "--initial-b2b", String(initialB2b),
    ...nativeRuleArguments(values),
    ...(maxPatterns === null
      ? []
      : ["--max-patterns", String(maxPatterns)]),
    "--backend", "cpu",
    "--no-backend-fallback",
  ];
}

function setupScoreSupplyArguments(values, role) {
  const queueName = `${role}-queue`;
  const patternsName = `${role}-patterns`;
  if (values.has(queueName) === values.has(patternsName)) {
    throw new DiscordInputError(
      "source.invalid",
      {},
      `Setup score requires exactly one of ${queueName} or ${patternsName}.`,
    );
  }
  if (values.has(queueName)) {
    const queue = requiredText(values, queueName, NEXT_MAX_LENGTH);
    if (!/^[IOTSZJL]+$/iu.test(queue)) {
      throw invalidOption(queueName, `${queueName} must contain only exact IOTSZJL pieces.`);
    }
    return [`--${queueName}`, queue.toUpperCase()];
  }
  const patterns = requiredText(values, patternsName, NEXT_MAX_LENGTH);
  if (patterns.startsWith("-")) {
    throw invalidOption(
      patternsName,
      `${patternsName} must be a queue-pattern language, not a command-line option.`,
    );
  }
  return [`--${patternsName}`, patterns];
}

function setupFinderArguments(command, values) {
  const remaining = setupInventory(
    requiredText(values, "remaining", 64),
    "remaining",
  );
  const defaultPriority = String(command.setupPriority ?? "").toLowerCase();
  if (!SETUP_PRIORITIES.has(defaultPriority)) {
    throw new Error(`/${command.name} has an invalid default setup priority.`);
  }
  const priority = normalizedSetupChoice(
    optionalText(values, "priority", 16) ?? defaultPriority,
  );
  if (!SETUP_PRIORITIES.has(priority)) {
    throw invalidOption("priority", "priority must be all, build, or pc.");
  }

  const output = [
    ...command.argvPrefix,
    "--remaining",
    remaining,
    "--priority",
    priority,
  ];
  const maximumPieces = optionalInteger(values, "max-setup-pieces", 1, 10);
  if (maximumPieces !== null) {
    output.push("--max-setup-pieces", String(maximumPieces));
  }

  const queueKnowledge = optionalText(values, "queue-knowledge", 16);
  if (queueKnowledge !== null) {
    const requested = normalizedSetupChoice(queueKnowledge);
    const normalized = requested === "full-queue" ? "oracle" : requested;
    if (!SETUP_QUEUE_KNOWLEDGE.has(normalized)) {
      throw invalidOption(
        "queue-knowledge",
        "queue-knowledge must be full-queue or visible-7.",
      );
    }
    output.push("--queue-knowledge", normalized);
  }

  const nextCycleSource = optionalText(values, "next-cycle-remaining", 64);
  if (nextCycleSource !== null) {
    const nextCycleRemaining = setupInventory(
      nextCycleSource,
      "next-cycle-remaining",
    );
    const expected = nextCycleRemainingCount(remaining.length);
    if (nextCycleRemaining.length !== expected) {
      throw invalidOption(
        "next-cycle-remaining",
        `next-cycle-remaining must contain exactly ${expected} piece${expected === 1 ? "" : "s"} when remaining contains ${remaining.length}.`,
      );
    }
    output.push("--next-cycle-remaining", nextCycleRemaining);
  }

  const setupLength = optionalText(values, "setup-length", 16);
  if (setupLength !== null) {
    const normalized = normalizedSetupChoice(setupLength);
    if (!SETUP_LENGTHS.has(normalized)) {
      throw invalidOption(
        "setup-length",
        "setup-length must be auto, longer, or shorter.",
      );
    }
    output.push("--setup-length", normalized);
  }
  output.push(...(
    command.input === "setup-v2"
      ? setupNamedArguments(values, remaining)
      : setupFinderPackedArguments(command, values, remaining)
  ));
  output.push(...kicktableArguments(values, true));
  return output;
}

function setupNamedArguments(values, remaining) {
  const requestedMode = optionalText(values, "mode", 16);
  const qbSource = optionalText(values, "qb", 64);
  let mode = requestedMode === null ? null : normalizedChoice(requestedMode);
  if (mode !== null && !new Set(["oracle", "qb"]).has(mode)) {
    throw invalidOption("mode", "mode must be oracle or qb.");
  }
  if (qbSource !== null && mode === null) mode = "qb";
  if (mode === "qb" && qbSource === null) {
    throw new DiscordInputError("options.setup_qb_required");
  }
  if (mode === "oracle" && qbSource !== null) {
    throw new DiscordInputError("options.setup_qb_oracle_conflict");
  }

  const output = [];
  if (qbSource !== null) {
    const qb = pieceInventory(qbSource, "qb");
    if (qb.length > 7 || new Set(qb).size !== qb.length) {
      throw invalidOption(
        "qb",
        "qb must contain from 1 through 7 unique IOTSZJL pieces.",
      );
    }
    if (remaining.length + qb.length > 7) {
      throw new DiscordInputError("options.setup_qb_bag_capacity");
    }
    output.push("--mode", "qb", "--qb", qb);
  } else if (mode !== null) {
    output.push("--mode", mode);
  }

  const borrow = onOffValue(values, "post-cycle-borrow", false);
  if (borrow && remaining.length !== 3) {
    throw new DiscordInputError("options.setup_borrow_cycle");
  }
  if (borrow) output.push("--allow-post-cycle-borrow");
  return output;
}

function normalizedSetupChoice(value) {
  return String(value).trim().toLowerCase().replaceAll("_", "-");
}

function setupInventory(value, name) {
  const pieces = pieceInventory(value, name);
  if (pieces.length < 1 || pieces.length > 7) {
    throw invalidOption(name, `${name} must contain from 1 through 7 pieces.`);
  }
  const counts = new Map();
  for (const piece of pieces) {
    counts.set(piece, (counts.get(piece) ?? 0) + 1);
  }
  if (
    [...counts.values()].some((count) => count > 2) ||
    [...counts.values()].filter((count) => count === 2).length > 1
  ) {
    throw invalidOption(
      name,
      `${name} allows at most one piece kind twice; no piece may appear three times.`,
    );
  }
  return pieces;
}

function nextCycleRemainingCount(remainingCount) {
  return ({ 7: 4, 4: 1, 1: 5, 5: 2, 2: 6, 6: 3, 3: 7 })[remainingCount];
}

function pieceInventory(value, name = "remaining") {
  if (!/^[IOTSZJL]+$/i.test(value)) {
    if (name === "pieces") {
      throw new DiscordInputError(
        "pieces.invalid",
        {},
        "pieces must contain only IOTSZJL pieces.",
      );
    }
    throw invalidOption(name, `${name} must contain only IOTSZJL pieces.`);
  }
  return value.toUpperCase();
}

function allowedOptionNames(command) {
  const input = command.input;
  switch (input) {
    case "pc":
      return new Set(["field", "next", "lines", "options", "kicktable"]);
    case "pc-v2":
      return new Set([
        "field",
        "next",
        "lines",
        "hold",
        "kicktable",
        "queue-knowledge",
        "spin-profile",
        "preserve-b2b",
        "solution-probabilities",
      ]);
    case "pc-path-v2":
      return new Set([
        "field",
        "next",
        "lines",
        "hold",
        "kicktable",
        "spin-profile",
        "preserve-b2b",
      ]);
    case "pc-chance-v2":
      return new Set(["field", "next", "lines", "hold", "kicktable"]);
    case "pc-save-v2":
      return new Set(["field", "next", "lines", "hold", "kicktable"]);
    case "pc-score-v2":
      return ["pc.score", "pc.score-minimals"].includes(command.capabilityId)
        ? new Set([
            "field",
            "next",
            "lines",
            "hold",
            "kicktable",
            "score-profile",
            "spin-profile",
            "initial-b2b",
          ])
        : new Set([
            "field",
            "next",
            "lines",
            "hold",
            "kicktable",
            "queue-knowledge",
            "score-profile",
            "spin-profile",
            "preserve-b2b",
            "initial-b2b",
            "solution-probabilities",
          ]);
    case "pc-tiling-v2":
      return new Set(["field", "next", "lines", "hold"]);
    case "pc-failed-v2":
      return new Set([
        "field",
        "next",
        "lines",
        "hold",
        "kicktable",
        "queue-knowledge",
        "spin-profile",
        "preserve-b2b",
        "failed-count",
      ]);
    case "pc-allspin-exact-v1":
    case "pc-allspin-pattern-v1":
      return new Set([
        "field",
        "next",
        "lines",
        "hold",
        "kicktable",
        "spin-profile",
        "max-patterns",
        "max-nodes",
        "max-frontier-states",
        "max-candidates",
        "max-memory-mib",
      ]);
    case "spin":
      return new Set(["field", "next", "options", "kicktable"]);
    case "forward-spin-v2":
      return new Set([
        "field",
        "next",
        "height",
        "hold",
        "kicktable",
        "spin-profile",
        "lines",
        "spin-category",
        "initial-combo",
        "initial-b2b",
        "preserve-b2b",
      ]);
    case "cover":
      return new Set(["base", "target", "next", "options", "kicktable"]);
    case "build-cover":
      return new Set([
        "base",
        "target",
        "next",
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
      ]);
    case "build-v2-cover":
      return new Set([
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
    case "build-v2-target":
      return new Set([
        "target-format",
        "target-document",
        "queue",
        "patterns",
        "hold",
        "queue-knowledge",
        "objective",
        "kicktable",
        ...(BUILD_V2_SCORE_CAPABILITIES.has(command.capabilityId)
          ? ["score-profile", "initial-b2b"]
          : []),
      ]);
    case "build-v2-supplied":
      return new Set([
        "solution-format",
        "solution-document",
        "queue",
        "patterns",
        "hold",
        "queue-knowledge",
        "objective",
        "kicktable",
        ...(BUILD_V2_SCORE_CAPABILITIES.has(command.capabilityId)
          ? ["score-profile", "initial-b2b"]
          : []),
      ]);
    case "colored":
      return new Set(["field", "next", "kicktable"]);
    case "fixed-next":
      return new Set(["field", "next", "kicktable", "options"]);
    case "forward-damage-v2":
      return new Set([
        "field",
        "next",
        "height",
        "hold",
        "kicktable",
        "spin-profile",
        "damage-mode",
        "minimum-damage",
        "initial-combo",
        "initial-b2b",
        "preserve-b2b",
      ]);
    case "forward-ren-v1":
      return new Set(["field", "next", "height", "hold", "kicktable"]);
    case "score-fixed-next":
      return new Set(["field", "next", "lines", "options", "kicktable"]);
    case "score-fixed-next-v2":
      return new Set(["field", "next", "lines", "initial-b2b", "kicktable"]);
    case "pc-score-finder-v2":
      return new Set(["field", "next", "lines", "hold", "initial-b2b", "kicktable"]);
    case "remaining":
      return new Set([
        "remaining",
        "priority",
        "max-setup-pieces",
        "queue-knowledge",
        "next-cycle-remaining",
        "setup-length",
        "kicktable",
        "options",
      ]);
    case "setup-v2":
      return new Set([
        "remaining",
        "mode",
        "qb",
        "queue-knowledge",
        "next-cycle-remaining",
        "post-cycle-borrow",
        "setup-length",
        "max-setup-pieces",
        "kicktable",
      ]);
    case "setup-score-v1":
      return new Set([
        "document-format",
        "document",
        "setup-queue",
        "setup-patterns",
        "solution-queue",
        "solution-patterns",
        "clear",
        "hold",
        "score-profile",
        "initial-b2b",
        "kicktable",
        "max-patterns",
      ]);
    case "spin-structure":
      return new Set(["pieces", "field", "lines", "profile", "kicktable", "options"]);
    case "spin-structure-v2":
      return new Set([
        "pieces",
        "field",
        "height",
        "lines",
        "spin-profile",
        "kicktable",
        "fill-bottom",
        "fill-top",
        "max-placements",
        "minimality",
      ]);
    case "spin-structure-cover-v1":
      return new Set([
        "pieces",
        "field",
        "height",
        "lines",
        "spin-profile",
        "kicktable",
        "fill-bottom",
        "fill-top",
        "max-placements",
        "minimality",
        "max-patterns",
      ]);
    case "spin-structure-guaranteed-v1":
      return new Set([
        "pieces",
        "field",
        "height",
        "lines",
        "spin-profile",
        "kicktable",
        "fill-bottom",
        "fill-top",
        "max-placements",
        "minimality",
        "final-piece",
        "max-patterns",
        "dependency-report",
      ]);
    case "verify":
      return new Set(["scope"]);
    case "finesse-search":
      return new Set(["target", "next", "base", "kicktable", "options"]);
    case "finesse-score":
      return new Set(["document", "next", "kicktable", "options"]);
    case "finesse-score-v2":
      return new Set([
        "document",
        "next",
        "kicktable",
        "hold",
        "knowledge",
        "source-pieces",
      ]);
    case "operation-document-v1":
      return new Set([
        "document",
        "attachment",
        "rule-profile",
        "kick-profile",
        "timeout-seconds",
      ]);
    case "field-document-v1":
      return new Set(["document", "attachment"]);
    case "fumen-transform-v1":
      return new Set([
        "transform",
        "document",
        "attachment",
        "documents",
        "page",
        "offset",
        "comments",
      ]);
    case "render-document-v1":
      return new Set(["document", "attachment", "artifact-format", "page"]);
    default:
      throw new Error(`Unknown slash-command input contract: ${input}`);
  }
}

function kicktableArguments(values, native = false) {
  const selected = optionalText(values, "kicktable", 32);
  if (!selected) return [];
  const normalized = selected.toLowerCase();
  const supported = native ? NATIVE_KICKTABLES : SFINDER_KICKTABLES;
  if (!supported.has(normalized)) {
    throw new Error(
      native
        ? "kicktable must be srs-plus, srs, srs-x, jstris-180, or no-kick; custom kick tables are unavailable."
        : "kicktable must be srs-plus, srs, srs-x, or jstris-180; custom kick tables are unavailable.",
    );
  }
  return ["--rule", normalized];
}

function nativeRuleArguments(values) {
  const selected = normalizedChoice(
    optionalText(values, "kicktable", 32) ?? "srs-plus",
  );
  if (!NATIVE_KICKTABLES.has(selected)) {
    throw new Error(
      "kicktable must be srs-plus, srs, srs-x, jstris-180, or no-kick; custom kick tables are unavailable.",
    );
  }
  return ["--rule", selected];
}

function normalizedChoice(value) {
  return String(value ?? "").trim().toLowerCase().replaceAll("_", "-");
}

function onOffValue(values, name, defaultValue) {
  const source = optionalText(values, name, 16);
  if (source === null) return defaultValue;
  switch (normalizedChoice(source)) {
    case "on":
    case "true":
    case "yes":
    case "use":
      return true;
    case "off":
    case "false":
    case "no":
    case "avoid":
    case "disabled":
      return false;
    default:
      throw new Error(`${name} must be on or off.`);
  }
}

function mirrorMask(mask, height) {
  let mirrored = 0n;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < 10; x += 1) {
      const source = BigInt(y * 10 + x);
      if ((mask & (1n << source)) === 0n) continue;
      mirrored |= 1n << BigInt(y * 10 + (9 - x));
    }
  }
  return mirrored;
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
    throw invalidOption(
      name,
      `${name} must be an integer from ${minimum} through ${maximum}.`,
    );
  }
  return value;
}

function invalidOption(option, message) {
  return new DiscordInputError("options.invalid", { option }, message);
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
